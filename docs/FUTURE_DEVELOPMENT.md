# Future Development — Product Evolution Options

**Status:** FUTURE_PRODUCT_EVOLUTION / NOT_ACTIVE_EXECUTION  
**Current authority:** [`INDEX.md`](INDEX.md) -> [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)  
**Planning prerequisite:** a later explicit product/architecture decision from then-current accepted `main`

## 1. Purpose

This document records non-active product-evolution options. It is not a roadmap competing with the current Architecture Re-baseline v3 / Production Capability program, cannot contain `NEXT`, and must not block the current path to Production Core.

No item in this file has an owning execution Issue merely because it is documented here. A future initiative receives its own Issue/branch/PR/evidence plan only after a fresh accepted-main re-baseline and the sole mutable tracker selects that initiative as current work.

## 2. External CRM / Party Integration

If a future explicit decision activates this initiative, preserve these principles:

1. preserve the standalone opaque `client_id` as the platform resource identity unless a separately accepted migration proves otherwise;
2. introduce/verify an opaque `external_party_ref` rather than deriving IDs from CRM names/contacts;
3. consume versioned CRM Party/Customer projections/events through an isolated adapter;
4. reconcile/link standalone Client and CRM Party without changing profile/generation/session IDs;
5. prove parity before transferring authority for canonical name/contact/status fields;
6. keep profile assignments, browser profiles, generations, sessions, certification, mailbox runtime and Profile Bridge lifecycle owned by this platform unless a future bounded authority transfer explicitly changes that ownership;
7. after an accepted authority transfer, block conflicting local edits or translate explicit commands through the CRM adapter;
8. evaluate any PostgreSQL/SQLx + RLS replacement as a separate architecture migration;
9. evaluate any CRM OIDC identity replacement as a separate security/identity migration;
10. preserve R2 encrypted-generation and Profile Bridge lifecycle boundaries without CRM coupling.

### CRM acceptance principles

Any later CRM initiative must still satisfy:

- versioned contract/event isolation; no CRM SDK/table/entity imports in core domain/application code;
- async durable projections for synchronization, with synchronous HTTP only where a user command legitimately needs an immediate acknowledgement/result;
- tenant/authorization checks before projection/fetch;
- no raw PII in technical identifiers, logs, events or support evidence;
- no standalone feature regression and no requirement to migrate R2 generation objects merely to integrate CRM;
- `source_present != production_enabled` remains binding throughout any CRM introduction.

## 3. Versioned Network Policies And Route Admission

A future product capability may make network admission policy editable without turning network state into browser identity.

### Natural ownership

- one server-side **Network Policy / Route Admission** bounded capability owns policy identity, revision lifecycle, profile bindings and audit;
- the existing `browser-execution-domain::NetworkIdentityPolicy` remains the pure network-observation predicate/projection used by browser launch admission; it does not become a CRUD database aggregate;
- server/provider route inventory owns observable route facts; Camoufox is never the authority for selecting or discovering an acceptable route;
- browser identity, Windows delivery state and `camoufox-config.json` do not own network-policy revisions or proxy credentials.

### Target policy model

A named policy, for example `PL Mobile / Warsaw`, should support:

- lifecycle: `Draft`, `Active`, `Retired`;
- immutable revisions with canonical `policy_id`, `revision` and content digest;
- optional country, region and timezone constraints;
- allowed network classes;
- allowed ASNs;
- allowed exact route IDs and/or canonical route groups resolved through server-managed inventory;
- normalized IPv4/IPv6 CIDR ranges;
- reason/audit metadata for creation, activation, retirement and profile migration.

Predicate semantics should be **AND between constrained categories and OR within an allowed set**. An absent category is unconstrained. Route groups must canonicalize to one route-admission set rather than create a competing evaluation path.

### Profile binding and change lifecycle

- a profile binds `policy_id + revision + digest`, never a mutable “current policy” pointer;
- editing creates a new immutable revision;
- applying another revision to a profile is an explicit command with actor/reason audit and an impact simulation/preview;
- a policy revision change affects the next launch only; it never rewrites an active browser session, an existing generation identity or saved browser state;
- rollback means explicitly rebinding to a previously accepted revision, not editing history in place;
- UI should expose ordinary country/timezone/network-class/route-group controls separately from advanced ASN/CIDR validation and affected-profile preview.

### Route observation and fail-closed launch admission

Before browser navigation, the existing network owner should evaluate a trusted fresh observation including the applicable route identity, public network identity, geography, timezone/coherence, network class and ASN/CIDR facts required by the selected revision.

Unknown, stale, unverifiable or policy-incompatible observations fail closed or request operator remediation/route replacement. A proxy/IP/ASN change must never silently mutate Profile-Stable browser identity to make a launch pass.

Proxy credentials and provider secrets remain outside the profile/network-policy record and outside browser identity. They are resolved only through their authorized secret/provider boundary.

### Explicit non-goals

This future capability does **not** imply:

- autonomous Camoufox `geoip=True` as network authority;
- embedding proxy credentials in a browser profile or `camoufox-config.json`;
- replacing the existing coordinator lease/fencing model;
- a second browser fingerprint owner;
- provider or Production mutation merely because source code for policy management exists.

## 4. Future Acceptance Principles

Every future initiative documented here must still satisfy:

- its own bounded change envelope, natural owner and public/persisted contract analysis;
- one semantic owner per concern and no parallel active runtime path;
- explicit positive/negative acceptance and rollback/recovery impact;
- generated/public contract discipline where applicable;
- exact-head acceptance under the then-current protected governance;
- its own Issue/ADR/branch/PR/evidence plan created only from the then-current accepted `main` after tracker selection;
- `source_present != production_enabled` and separate provider/Production authorization remain binding.

## 5. Explicit Non-Goals Of The Current Program

The current CAP program does not require or authorize:

- CRM Party authority/cutover, CRM OIDC migration, CRM-backed PostgreSQL migration or CRM-specific workflows;
- editable/versioned Network Policy CRUD/UI, route-group inventory management or CIDR management;
- provider inventory mutation or proxy-secret migration for a future Network Policy capability;
- any future feature that prevents the standalone application from operating independently.

Until a future decision is accepted, these options remain future scope only and have no effect on the active transaction, scenario completion or exact-candidate Production Authorization.
