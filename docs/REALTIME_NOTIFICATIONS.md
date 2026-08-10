# Realtime Notifications

**Status:** Phase 2G implementation specification  
**Production readiness:** false

## Responsibility

Realtime is a non-authoritative invalidation overlay on top of the accepted durable notification history and canonical HTTPS query surface. D1 outbox/event/cursor state remains authoritative. A WebSocket connection or Durable Object instance may disappear at any time without changing business truth.

Exactly one notification-hub Durable Object name is derived from each opaque `(TenantId, ActorId)` pair. Every browser tab or device for the same actor reaches that same per-user hub, so the object coordinates live sockets only; it does not own client, profile, mailbox, membership or device state.

## Public Contract

The public realtime envelope is versioned and closed. Version 1 contains only:

- opaque durable `eventId` used for duplicate suppression;
- low-cardinality `resource` kind (`clients`, `profiles`, `mailboxes`, `memberships`, `devices` or `platform`);
- `occurredAtMs` ordering metadata;
- the explicit contract version.

The public envelope has no arbitrary payload slot. Aggregate identifiers, client contact plaintext, mailbox subject/body, credentials, secret handles and other prohibited high-cardinality or confidential values are not valid realtime fields.

## Authorization Boundary

`GET /api/v1/tenants/{tenant_id}/notifications/realtime` is a WebSocket-upgrade-only notification route.

Before a Durable Object stub is resolved, the Worker verifies Cloudflare Access identity, resolves the active tenant membership and creates the trusted actor context. Access JWT, `Authorization` and `Cookie` headers terminate at the Worker boundary and are removed before the request enters the notification hub.

The per-user hub rechecks current notification capability before accepting the socket. Every live event is rechecked against current membership and current client/profile grants immediately before delivery. Historical assignment state is never an ACL. A periodic Durable Object alarm revalidates membership; revoked, suspended or unverifiable actors are disconnected with a policy close without requiring page reload.

## Durable-Before-Live Ordering

Realtime fan-out starts only after the existing notification consumer has durably committed the event as delivered. Reconnect executes the accepted Phase 1B catch-up path before live continuation:

1. resolve current authorization;
2. load the durable actor cursor and currently authorized event page;
3. emit canonical invalidation signals;
4. CAS-advance the durable cursor only after the complete page was handed to the socket;
5. repeat bounded pages until the durable gap is drained;
6. then allow live continuation.

A socket/send failure leaves the cursor unchanged. A CAS race may repeat an event but cannot skip durable history. The frontend uses `eventId` only for bounded duplicate suppression, so repeated realtime delivery does not become repeated logical UI state.

A short-lived synchronization gate prevents live fan-out from crossing an in-progress reconnect catch-up. Its owner token is derived from the WebSocket handshake nonce with SHA-256 rather than from process memory or a millisecond timestamp. A stale gate has a bounded recovery deadline.

## Hibernation And Multiple Connections

The notification hub uses Hibernatable WebSockets. Tenant and actor identifiers are serialized as socket attachment metadata so a reactivated object can reauthorize without treating process memory as durable state.

Live broadcast iterates all sockets currently attached to the per-user hub. Multiple tabs and multiple devices for the same actor therefore receive the same invalidation signal, while each frontend instance maintains its own bounded duplicate set and refetches canonical HTTPS projections.

Client-to-server WebSocket messages are ignored and cannot become commands or business state.

## Frontend Authority

The React bridge strictly parses the closed versioned signal shape. Valid signals call TanStack Query invalidation only; they never call `setQueryData`, persist WebSocket payloads, or synthesize canonical records. Existing authenticated HTTPS query functions remain the only source for refreshed client/profile/mailbox/member/device data.

Reconnect uses bounded backoff. A policy-revocation close stops automatic reconnect churn until tenant context changes.

## Deployment Composition

The Worker composition requires a Durable Object binding named `NOTIFICATION_HUB` whose class is `NotificationHub`, alongside the existing `PROFILE_COORDINATOR` binding. The repository intentionally does not introduce a second production Wrangler source of truth; deployment configuration must add this binding and its Durable Object migration in the same external composition source that already provisions the control-plane Worker.

The `/bindings` probe resolves a deterministic notification-hub stub so missing composition fails at startup/probe time rather than being silently ignored.

## Permanent Evidence

Repository acceptance includes:

- native deterministic evidence that socket loss leaves the cursor replayable;
- CAS-stale duplicate evidence with no skipped durable event;
- bounded cursor-gap drain evidence;
- stale-audience/revocation delivery reauthorization evidence;
- frontend malformed/extra-field/confidential-payload rejection and duplicate suppression tests;
- source-level multi-tab/device evidence requiring all per-user hub sockets to receive broadcast;
- a permanent Phase 2G architecture/privacy policy with negative fixtures for public payload leakage, notify-before-durable ordering and frontend WebSocket authority;
- native, Workers/WASM, frontend and release-composition gates on the unchanged final source head.

Remote provider credentials, production deployment and physical-device evidence remain External. Phase 2G does not promote `production_ready`; it remains false.
