# Repository Step 4 — Identity, Clients And ACL Evidence

**Дата:** 2026-08-06  
**Статус:** accepted  
**Baseline:** `5667779d59413d4736e58d6eb83a892dfdd2f522`  
**Technical evidence head:** `5b187ebd786cdca068ed209b79642ecaaebe3be6`  
**Accepted source head:** `1174a0720bc1c44fbb0c8e22b5c0cbac5f0810ad`  
**Pull request:** #15  
**Technical Quality Gate run:** `31052479944`  
**Final exact-head Quality Gate run:** `31052742660`  
**Squash merge:** `bd3db24ffc62d50654e385e587cab3e6a01b928c`

## 1. Реализованная Identity Boundary

Step 4 adds two adapters that converge on the same verified external identity
contract:

- a Cloudflare Access JWT adapter that parses bounded claims, requires `RS256`,
  validates issuer, audience, validity times and subject, selects a matching RSA
  signing JWK and verifies the original JWT signing input through Workers
  WebCrypto;
- a deterministic fake identity adapter used by repository tests.

The application resolves the verified external subject through a tenant-scoped
active membership lookup. Only an active membership produces an `ActorContext`.
Missing, suspended and revoked memberships fail closed for the covered API
surface.

## 2. Membership And Owner Governance

The accepted slice implements:

- idempotent owner bootstrap only when the tenant membership boundary is empty;
- invitations and invitation acceptance into an active member identity;
- membership activation, suspension and revocation;
- atomic owner transfer from the current active owner to an active member;
- database and domain guards that prevent a command from leaving zero active
  owners.

Owner transfer and membership status changes use typed governed-command tables
and transaction-fatal D1 triggers. A stale expected version, invalid role,
invalid transition or last-owner violation aborts the command before a partial
aggregate/idempotency/audit/outbox envelope can commit.

## 3. Clients, Profiles, Assignments And ACL

The versioned Worker API exposes covered owner/member flows for:

- session resolution;
- owner bootstrap and transfer;
- invitation creation and acceptance;
- membership status changes;
- client creation and tenant-scoped client query;
- profile metadata creation and tenant-scoped profile query;
- historical profile/client assignment;
- explicit client and profile grant/revoke.

Assignments are deliberately historical business associations, not
authorization. Member visibility is obtained only from an explicit same-tenant
grant; active owners have the governed owner capability. Tenant-scoped query
projections return the same empty/neutral result for foreign, missing or
unauthorized resources where disclosure is prohibited.

## 4. Atomic Mutation Envelope

Covered Step 4 mutations treat the following as one atomic unit:

1. aggregate or governed command state;
2. idempotency result;
3. sanitized audit event;
4. outbox event.

Migration `0003_governed_commands.sql` supplies transaction-fatal precondition
checks for owner transfer, invitation creation, membership status, profile
creation, profile assignment and explicit grants. Permanent SQLite tests force
stale versions and downstream envelope failures and prove that trigger side
effects roll back with the full transaction. Successful paths prove the command,
audit and projection versions commit together.

## 5. Adapter And Contract Boundaries

Raw `worker::d1` access remains confined to `crates/cloudflare-adapters`.
Repository reads require typed tenant scope; mutations receive verified actor or
verified bootstrap context. A permanent governed-write gate rejects reintroduction
of superseded direct mutation methods outside the guarded repositories.

OpenAPI v1 was extended additively. The accepted v1 compatibility floor remains
immutable, including the existing `correlation_id` problem field. The Worker
keeps the prior health, binding-probe, static-assets and fail-closed bridge
routes while composing the authenticated Step 4 routes into the release WASM.

## 6. Permanent CI Result

Technical Quality Gate run `31052479944` succeeded on technical head
`5b187ebd786cdca068ed209b79642ecaaebe3be6`.

Final exact-head Quality Gate run `31052742660` repeated the complete permanent
gate on documentation-complete accepted source head
`1174a0720bc1c44fbb0c8e22b5c0cbac5f0810ad` and succeeded.

### Rust Linux and WASM

- source-hygiene gate rejecting temporary Step 4 workflows, diagnostic markers
  and tracked Rust build artifacts;
- architecture dependency positive and deliberate negative checks;
- typed D1 boundary positive and deliberate raw-D1 negative checks;
- governed Step 4 write-surface enforcement;
- additive contract compatibility, deliberate breaking fixture rejection and
  accepted v1 baseline immutability;
- D1 schema, tenant isolation and transaction tests;
- owner/membership/ACL positive and negative tests;
- transaction-fatal command-guard rollback and commit tests;
- deliberate assignment-as-authorization fixture rejected as required;
- rustfmt, Clippy with warnings denied, native tests and adapter tests;
- governed pure crates checked for `wasm32-unknown-unknown`;
- delivery status and current-tree high-confidence secret checks.

### D1 Catalog Migrations

- pinned Wrangler `4.94.0` applies migrations `0001` through `0003` to isolated
  local D1 state;
- migration replay is a no-op;
- catalog, invitation acceptance and governed command tables are queried after
  migration.

### Rust Windows

- all native non-Worker/non-Cloudflare-adapter workspace tests passed.

### Cloudflare Worker Release Build

- authenticated Worker and typed adapters checked for WASM;
- pinned `worker-build 0.8.5` release packaging passed;
- generated shim and Wasm artifact verification passed.

## 7. Доказанные Свойства

The accepted repository evidence proves the covered properties:

- Access and fake identity adapters produce the same application identity input;
- only active same-tenant memberships resolve to an actor;
- owner bootstrap is idempotent and empty-boundary-only;
- owner transfer is governed, atomic and audited;
- the final active owner cannot be suspended or revoked;
- stale optimistic versions abort without partial envelope records;
- suspended/revoked/missing membership is denied on covered flows;
- assignment does not grant client or profile access;
- active owners and explicitly granted active members pass covered client/profile
  queries;
- foreign, missing and unauthorized resources share the neutral disclosure shape;
- raw D1 and unguarded Step 4 write surfaces are rejected by permanent CI;
- the actual Worker dependency graph builds and packages the Step 4 adapters.

## 8. Defects Found And Corrected

- Workers WASM initially lacked the required WebCrypto features and explicit
  Promise error conversion;
- initial route expansion temporarily lost accepted binding and fail-closed
  bridge behavior; the extension was made strictly additive;
- generated OpenAPI temporarily renamed accepted `correlation_id`; the accepted
  field was restored and protected by the compatibility gate;
- the raw-D1 static gate treated generic `.prepare(` calls as D1 access and
  falsely rejected JWT preparation; the gate now matches provider-specific D1
  APIs while retaining the deliberate negative fixture;
- Worker client creation initially targeted a mutation shape newer than the
  accepted Step 3 adapter; composition was aligned with the accepted API;
- ordinary `UPDATE ... WHERE version = ?` in a batch could permit envelope rows
  after a zero-row stale CAS; governed command triggers now make precondition
  failure transaction-fatal;
- superseded direct mutation methods duplicated governed write paths and were
  removed;
- temporary write/debug workflows, trigger notes, an unused WebCrypto prototype
  and accidentally tracked `target/**` output were removed;
- `.gitignore` and permanent source hygiene now prevent Rust build output and
  temporary Step 4 artifacts from returning.

## 9. Ограничения И Внешние Gates

This evidence does not prove:

- remote Cloudflare staging or production deployment;
- real Access policy configuration, account bindings, remote D1 limits,
  contention, Time Travel, backup or restore;
- production credentials, secret rotation or key recovery;
- Durable Object lease/coordinator behavior or distributed fencing;
- encrypted R2 profile generations;
- Windows Profile Bridge, Camouhost/Camoufox runtime or trusted code signing;
- physical multi-device behavior;
- production privacy readiness or production readiness.

No Cloudflare credential, production secret, remote resource, real user profile,
mailbox content or personal data was used. All identity, tenant, client and
profile fixtures were synthetic. `production_ready` remains `false`.
