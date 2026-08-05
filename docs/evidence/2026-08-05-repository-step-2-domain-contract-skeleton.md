# Repository Step 2 — Domain And Contract Skeleton Evidence

**Дата:** 2026-08-05  
**Статус:** accepted for bounded domain/contract scope  
**Baseline:** `29956f6a71ea5f76618e97c651276f2a43698870`  
**Technical evidence head:** `a3d0852e11708297bb7d5e04ed23ff981e774d7c`  
**Accepted source head:** `70fdbe8b494f61aeda639e004e54ea088e4ddc3e`  
**Pull request:** #9  
**Technical Quality Gate run:** `31039199212`  
**Final Quality Gate run:** `31039802642`  
**Squash merge:** `14e96a7841c3652767f37ee76151c3cf39be6301`

## 1. Реализованные Pure Boundaries

### Primitives

- opaque validated identifiers that reject email/path-like values;
- distinct tenant, actor, client, profile, generation, session, device, mailbox,
  launch-intent, correlation, idempotency, fencing and secret-handle types;
- mandatory `TenantScope` and verified `ActorContext`;
- aggregate versions with checked increment;
- explicit Unix millisecond value type.

### Identity And Access

- owner/member membership model;
- active/suspended/revoked membership status;
- separate profile and client grants;
- viewer/operator and viewer/editor capabilities;
- default-deny authorization;
- cross-tenant, actor, missing-grant and insufficient-capability rejection;
- owner authorization without converting client assignment into a grant.

### Client And Assignment

- bounded client record and lifecycle;
- historical `ProfileClientAssignment` separate from authorization;
- same-tenant requirement;
- archived clients cannot receive new assignments;
- explicit actor, timestamp and reason;
- close transition preserves assignment history.

### Profile And Generation

- profile lifecycle state machine;
- forbidden state transitions;
- immutable generation reference model;
- only verified, same-tenant, same-profile generation can become active;
- live/dirty/syncing profiles cannot bypass lifecycle rules.

### Session

- actor/device/tenant-bound launch intent;
- single-use redemption, expiry and replay rejection;
- positive monotonic lease epoch;
- fencing token validation;
- stale epoch/token cannot commit or close a lease.

### Mailbox

- provider-neutral Gmail API, IMAP and browser-fallback provider categories;
- secret handle only, without raw credential fields;
- active/revoked binding lifecycle;
- bounded cursor and explicit pending/running/retry/success/failure transitions.

### Application Ports And Use Cases

- typed membership, client, profile, coordinator, object-store, mailbox, clock
  and audit ports;
- repository methods require tenant scope or actor context;
- initial create-client and open-profile decisions;
- unauthorized/foreign profile lookup maps to neutral `not_found`;
- only operator on a ready profile receives an open decision.

## 2. Versioned Contracts

- Rust `ContractVersion` and stable `ProblemCode` taxonomy;
- OpenAPI 3.1 v1 root under `openapi/v1/`;
- Bridge protobuf v1 root;
- profile/CRM protobuf v1 root;
- accepted v1 compatibility floor under `contracts/baseline/`;
- compatibility checker rejects removed API paths/operations/schemas and removed
  or renamed protobuf messages/field numbers;
- deliberately breaking protobuf fixture is required to fail;
- after initial acceptance, ordinary PRs cannot rewrite the v1 baseline; a new
  major root or governed migration is required.

## 3. Architecture Enforcement

`check-architecture.py` applies dependency allowlists to all governed pure crates.
It rejects Cloudflare Worker, Tokio, Axum, SQLx, Windows, Python, Playwright,
HTTP and SQLite provider/runtime dependencies from domain/application boundaries.

The permanent gate also runs a deliberately forbidden fixture containing a
`worker` dependency in `profile-domain` and requires the checker to reject it.
This proves that the negative gate is active rather than merely documenting an
allowlist.

## 4. Permanent CI Result

Quality Gate run `31039199212` succeeded for the technical implementation head.
Quality Gate run `31039802642` repeated the complete gate on the exact accepted
source head after evidence, status and immutable-baseline governance were added.

Both accepted runs covered:

1. `Rust Linux and WASM`
   - repository policy scripts compile;
   - real architecture passes;
   - forbidden architecture fixture is rejected;
   - current v1 contracts pass;
   - breaking protobuf fixture is rejected;
   - accepted v1 baseline immutability gate;
   - rustfmt and Clippy with warnings denied;
   - all native pure-crate tests;
   - all governed pure crates compile for `wasm32-unknown-unknown`;
   - status and tracked-secret checks.
2. `Rust Windows`
   - all native non-Worker workspace tests.
3. `Cloudflare Worker Release Build`
   - existing Worker still checks for WASM;
   - pinned `worker-build 0.8.5` release packaging;
   - shim and Wasm artifact verification.

## 5. Defects Found And Corrected

- initial source formatting was rejected and corrected by exact `cargo fmt`;
- `ApplicationError` lacked `Display` and `std::error::Error`, preventing standard
  error propagation in use-case tests;
- a redundant tenant comparison was removed before Clippy acceptance;
- temporary lockfile/rustfmt write workflows were removed before acceptance.

## 6. Что Это Доказывает

- provider-independent domain rules compile on Linux, Windows and Workers WASM;
- business identifiers cannot be accidentally interchanged at the type boundary;
- identity/membership, assignment and explicit resource grants are distinct;
- profile, session and mailbox transitions fail closed for the covered cases;
- storage/provider SDKs have not entered pure domain/application boundaries;
- current v1 contracts have a machine-enforced compatibility floor;
- accepted v1 baseline cannot be silently rewritten by an ordinary later PR;
- negative architecture and compatibility fixtures are actually rejected;
- the accepted Step 1 Worker build remains green after the domain expansion.

## 7. Что Не Доказано

- D1 schema, migrations, transactions, repository implementation or RLS-like
  tenant isolation behavior;
- Cloudflare Access JWT validation or real membership storage;
- Durable Object persistence, eviction, heartbeat or distributed fencing;
- R2 encryption/upload/restore and Queue redelivery;
- production idempotency, audit and outbox persistence;
- React UI, Windows Bridge or Camouhost runtime;
- full domain completeness or production readiness;
- remote Cloudflare deployment, multi-device behavior, key recovery or
  fingerprint certification.

No Cloudflare credential, real account resource, user profile, mailbox content or
personal data was used in this evidence.
