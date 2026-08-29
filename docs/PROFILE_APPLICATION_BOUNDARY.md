# Profile Application Boundary

**Status:** current bounded application contract for Profile create/read/relationship/launch semantics.

**Scope:** repository architecture only. Generation persistence, coordinator state, local Bridge lifecycle
and mailbox routes retain their own natural owners. This document does not claim production readiness or
select the current execution transaction.

## Purpose

The Profile capability owns public Profile application semantics behind provider-neutral application
contracts. Transport, persistence and device/runtime effects remain outer concerns:

```text
browser / generated operation
  -> apps/control-plane-worker/src/profiles.rs
     authenticated actor context + transport mapping
  -> crates/use-cases
     Profile authorization + application sequencing
  -> crates/application-ports
     provider-neutral Profile / launch ports
  -> crates/cloudflare-adapters
     governed D1 evidence / persistence adapters
```

Concrete provider construction belongs to composition roots. Worker transport must not become a second
Profile ACL owner, construct ad-hoc authorization SQL or duplicate application policy.

## Profile Create Ownership

The application use case owns these decisions and ordering rules:

1. only a tenant owner may execute profile create; non-owner disclosure remains neutral `not_found`;
2. owner authorization is evaluated before request-body/idempotency parsing, preserving the disclosure boundary;
3. the use case creates the typed `BrowserProfile` aggregate in its initial state;
4. exact idempotency replay returns the prior logical result without a new write;
5. a concurrent unique conflict is rechecked for exact replay and is otherwise a conflict;
6. the D1 adapter maps typed Profile state and command evidence into the existing governed write path;
7. the governed D1 transaction retains atomic command journal, Profile row, idempotency, audit and outbox behavior;
8. provider failures map to stable application failure classes rather than leaking SDK/storage errors.

## Visible Profile Query Ownership

The Worker parses the Profile ID and calls the application query. The D1 adapter retains the existing
disclosure-safe visibility query, including the rule that a Client assignment is projection data and
**not** Profile authorization.

Storage strings are converted into typed Profile state/version values. Invalid stored values fail as
integrity errors instead of being relabeled as a missing business resource.

## Profile Relationship Ownership

Profile-to-Client relationship history remains owned by the canonical assignment model. Attach,
standalone detach and atomic reassign all use that owner; no second relationship table or Client-card
shortcut becomes authoritative.

Relationship semantics remain non-authorizing:

```text
Profile assigned to Client
!=
actor authorized for Profile
```

Client -> Profiles projections independently authorize the Client and every returned Profile before
projection. Browser-controlled assignment/client identity is never trusted as durable mutation
authority.

## Authorized Profile Launch Ownership

The public Profile launch operation is an orchestration capability, not a new authorization owner.
The canonical chain is:

```text
Client Detail / Profile action
  -> generated `launchProfile` operation
  -> Profile launch admission
  -> existing Profile open authorization
  -> server-owned active actor-bound device resolution
  -> existing device/Profile/generation authorization
  -> existing execution preconditions
  -> bounded one-time launch authority
  -> machine-authenticated Bridge redemption
  -> fresh authorization/state revalidation
  -> atomic one-time claim consumption
```

Permanent rules:

1. the browser/public request selects the Profile action only; it does **not** assert trusted `deviceId`
   or `generationId`;
2. active generation and active actor-bound device are resolved from authoritative server state;
3. Profile access uses the existing Profile authorization owner; launch-specific code must not duplicate
   Profile ACL SQL or policy;
4. device authorization and execution-readiness semantics remain in their existing owners and are
   composed by launch orchestration;
5. the one-time launch-authority store owns issuance, digest-only persistence, exact target binding,
   bounded expiry, replay/concurrency semantics and atomic consumption; it does not become a second ACL
   owner;
6. raw claim material is bearer material and is never persisted server-side, logged or emitted in
   telemetry/evidence; the browser response is non-cacheable;
7. Bridge redemption authenticates the actual machine through the dedicated machine perimeter and the
   existing device-principal owner, using edge-verified mTLS certificate identity rather than a retained
   human Access bearer or new static launch bearer;
8. redemption re-resolves current actor/Profile/device/generation/readiness state **before** consuming
   the claim. Revocation, reassignment, generation change, expiry, replay or wrong machine fails closed;
9. successful redemption returns only the bounded typed identity needed by the existing Bridge/operator
   and coordinator boundaries;
10. assignment remains irrelevant to authorization except as independently authorized UI projection
    context.

The canonical browser HTTP contract is authored in the Rust control-plane contract and projected through
OpenAPI into generated TypeScript operation/validator code. Handwritten frontend method/path/response
semantics for launch are forbidden.

## Command Evidence

Profile commands reuse provider-neutral command evidence for idempotency, server-owned payload identity,
audit/outbox identity and bounded command timing. The browser never supplies a trusted digest to stand in
for server authorization or semantic validation.

For launch specifically, idempotency evidence binds issuance to the exact server-resolved
actor/Profile/device/generation context. Replay cannot mutate the original authority binding.

## Failure And Effect Ordering

All Profile commands and launch admission fail closed before their first protected effect when identity,
membership, authorization, capability admission or required state is absent.

For launch:

```text
browser authorization
-> current server state resolution
-> device/execution preconditions
-> bounded authority issuance

machine redemption
-> machine authentication
-> load digest-bound authority
-> current state revalidation
-> atomic consume
-> coordinator/runtime handoff
```

Claim issuance is therefore not durable permission to launch later if authority/state has changed.

## CI Enforcement

Permanent checks and tests protect the boundary at the cheapest sufficient tiers:

- Profile application/relationship owners remain capability-owned and provider-neutral;
- direct Worker D1/provider orchestration and duplicate authorization paths are rejected;
- assignment-derived authorization remains negative evidence;
- launch OpenAPI/generated frontend drift is rejected;
- public launch contains no caller-selected trusted device/generation;
- one-time authority tests cover exact binding, expiry, replay/concurrency and fresh revalidation;
- machine redemption and coordinator adapters validate exact typed responses and fail closed on changed
  device/session/epoch/fence state;
- production Bridge boundary checks reject claim-only success, a second shipping launcher and
  production-reachable synthetic runtime paths.

Exact-head green CI is acceptance evidence only for the exact candidate that produced it; it never grants
Production authorization.

## Related Boundaries

- coordinator lease/fencing: [`PROFILE_COORDINATOR.md`](PROFILE_COORDINATOR.md);
- local workspace/runtime lifecycle: [`LOCAL_PROFILE_LIFECYCLE.md`](LOCAL_PROFILE_LIFECYCLE.md);
- Profile generations: [`PROFILE_GENERATION_APPLICATION_BOUNDARY.md`](PROFILE_GENERATION_APPLICATION_BOUNDARY.md);
- mandatory architecture invariants: [`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md).

`source_present != production_enabled` remains binding. Production admission is owned only by the
Release / Capability Profile and exact target authorization process.
