# Browser Profile Platform — Development Plan

**Document status:** GENERATED_PROJECTION  
**Current architecture/program authority:** `ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Tracking:** issue #266  
**Accepted product phase:** Phase 2I  
**Current accepted architecture checkpoint:** AR-8C — operational credential lifecycle and staging provider foundation
**Current gate / next implementation:** issue #352 post-AR-8C cleanup / DX; AR-8D is blocked until acceptance
**Architecture complete:** `false`  
**Production Core gate:** `BLOCKED`  
**Production readiness:** `production_ready=false`

## 1. Authority and scope

This document projects product history and the active implementation order. It is not a second program
authority. The single current architecture/program execution authority is
`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, tracked by issue #266.

Stable architecture/security/data authorities remain scoped authorities rather than competing roadmaps:
`ARCHITECTURE.md`, accepted ADRs, `DATA_CLASSIFICATION.md`, `THREAT_MODEL.md`, `UI_ARCHITECTURE.md`,
generated contracts and `architecture/accepted-phases.json`.

The former issue #203 pre-2J product-readiness program is accepted predecessor history. Its exact former
plan and this file's former #203-oriented projection are preserved under `history/`; their old execution
order is not the forward queue after AR-1.

Phase 2I remains the last accepted repository-local product phase. The accepted phase ledger does not
advance during Architecture Re-baseline v3.

## 2. Current repository state

- Accepted repository-local product phase: **Phase 2I**.
- AR-0 — Delta Architecture Inventory: **DONE / ACCEPTED** through PR #267.
- AR-1 — Architecture Authority Re-baseline: **DONE / ACCEPTED**.
- AR-2 — Runtime Topology + D3 Compatibility: **DONE / ACCEPTED**.
- AR-3 — Application Architecture Contract: **DONE / ACCEPTED**.
- AR-4A — Composition-root consolidation: **DONE / ACCEPTED**.
- AR-4B — Client Mail route ownership: **DONE / ACCEPTED**.
- AR-4C — Outbound Mail composition extraction: **DONE / ACCEPTED**.
- AR-4D remains **NOT REQUIRED** unless later accepted evidence reopens it.
- AR-5 — Wrangler / Runtime Authority Cleanup: **DONE / ACCEPTED**.
- AR-6 — Full Python Estate + read-only Rust opsctl: **DONE / ACCEPTED**.
- AR-7 — Environments + GitHub Governance + Operational Boundaries: **DONE / ACCEPTED**.
- AR-8A / AR-8B / AR-8C: **DONE / ACCEPTED** inside active AR-8.
- Post-AR-8C cleanup / DX issue #352: **CURRENT ACCEPTANCE GATE**.
- AR-8D: **NEXT IMPLEMENTATION SUBSLICE, BLOCKED UNTIL #352 ACCEPTANCE**.
- AR-8E / AR-8F remain mandatory after AR-8D; AR-9 remains blocked until full AR-8 acceptance.
- AR-9…AR-17: ordered future architecture slices.
- `architecture_complete=false`.
- `production_core_gate=BLOCKED`.
- `production_ready=false`.
- Real production mutation: **forbidden throughout AR-0…AR-17**.
- AR-2 decision authority: `architecture/runtime-topology-ar2.json`.
- `GENERATION_VERIFICATION=DELETE`; AR-5 accepted source/Wrangler/deployment authority cleanup while preserving synchronous verification semantics.
- Historical D3/#251 repository-side machinery is preserved; its old production lane is disabled for forward execution.
- AR-6 accepted `architecture/python-estate-ar6.json` and the capability-bounded read-only `tools/opsctl` foundation; Draft PR #269 remains feasibility history only.
- AR-7 accepted classic `main` protection and Environment boundaries; AR-8A/AR-8B/AR-8C are accepted, including the AR-8C staging provider/credential foundation. Issue #352 is the current cleanup/DX gate; AR-8D cannot start until it is accepted.

## 2A. CURRENT_DELIVERY_MAP

Canonical machine projection: `architecture/inventory.json::current_delivery_map`. This section is a human-readable projection, not a second roadmap or release authority.

| Delivery dimension | Current status | Scope / gate |
|---|---|---|
| Source implemented | **PARTIAL** | Accepted source exists through AR-8C; AR-8D source is `NOT_STARTED_BY_GATE`. |
| Accepted on main | **PARTIAL** | AR-8A/AR-8B/AR-8C are accepted; `full_ar8_accepted=false`. |
| Staging live | **PARTIAL** | AR-8C staging provider/credential foundation is live and smoke-verified only; this is not a full-product or production claim. |
| Production authorized | **NO** | `production_core_gate=BLOCKED`; only successful AR-17 may authorize the Production Core gate. |
| Production enabled | **NO** | `production_ready=false`; only successful PC-1 after AR-17 authorization may enable accepted `production-core-v1` scope. |
| Current blocker | **#352 OPEN** | Post-AR-8C cleanup / DX acceptance blocks AR-8D implementation. |
| Next gate | **#352 acceptance** | Only after #352 acceptance may AR-8D implementation begin. |

`source_present != production_enabled` is mechanically enforced. Staging success never implies production authorization or enablement.

## 3. Accepted product phase ledger

The immutable authority is `architecture/accepted-phases.json`:

```text
Phase 0 architecture convergence                              ACCEPTED
Phase 1A durable event/outbox foundation                      ACCEPTED
Phase 1B notification domain + retry/DLQ/catch-up/operations  ACCEPTED
Phase 2A client-domain/contact foundation                     ACCEPTED
Phase 2B protected contact persistence/lifecycle              ACCEPTED
Phase 2C merge/assignment/projections/Client Registry UI      ACCEPTED
Phase 2D query/CQRS/global search/client-mail query            ACCEPTED
Phase 2E mailbox domain + cloud provider lane                 ACCEPTED
Phase 2F device jobs + browser/Bridge mailbox lane            ACCEPTED
Phase 2G durable realtime notification hub                    ACCEPTED
Phase 2H standalone UI/admin UX                               ACCEPTED
Phase 2I E2E/security/recovery/operations hardening           ACCEPTED
Architecture Re-baseline v3                                   ACTIVE
```

## 4. Current architecture execution order

```text
AR-0   Delta Architecture Inventory                              DONE
AR-1   Architecture Authority Re-baseline                        DONE
AR-2   Runtime Topology + D3 Compatibility                       DONE / ACCEPTED
AR-3   Application Architecture Contract                         DONE / ACCEPTED
AR-4A  Composition-root consolidation                            DONE / ACCEPTED
AR-4B  Client Mail route ownership                               DONE / ACCEPTED
AR-4C  Outbound Mail composition extraction                      DONE / ACCEPTED
AR-4D  Profile extraction — NOT REQUIRED by AR-3; reopen only by later accepted evidence
AR-5   Wrangler / Runtime Authority Cleanup                      DONE / ACCEPTED
AR-6   Full Python Estate + read-only Rust opsctl                DONE / ACCEPTED
AR-7   Environments + GitHub Governance + Operational Boundaries DONE / ACCEPTED
AR-8   Secrets / Keys / OAuth Refresh Concurrency                 ACTIVE — AR-8A/B/C ACCEPTED; #352 GATES AR-8D
AR-9   D1 Evolution / Schema Compatibility
AR-10  Runtime and Historical Executable Simplification
AR-11  Release-set / Promotion Architecture
AR-12  Fresh Rehearsal Environment
AR-13  Rotation Rehearsal
AR-14  Remote Recovery Rehearsal
AR-15  Windows Delivery Program — inherited Batch E
AR-16  Final Whole-project 10/10 Audit
AR-17  Architecture Closeout + Production Core Gate
```

No production provisioning or promotion belongs to an AR slice.

AR-16 requires final whole-project repository-owned `P0=0` and `P1=0` with no production mutation.
AR-17 may close the architecture program and authorize the Production Core gate while still keeping
`production_ready=false`.

Only after AR-17:

```text
PC-1 Production Core v1
PC-2 Mailbox Administration
PC-3 Mailbox Jobs / Automation
PC-4 Outbound / subsequent capabilities
```

Only successful PC-1 may make `production_ready=true` for `production-core-v1`.

## 5. Non-negotiable clean architecture

The valid dependency direction remains inward:

```text
primitives
contracts -> primitives
domains -> contracts + primitives
application-ports -> domains + contracts + primitives
use-cases-* -> application-ports + domains + contracts + primitives
adapters -> application-ports + domains + contracts + primitives + provider/runtime SDKs
apps -> use-cases-* + adapters + contracts + primitives
frontend -> generated public contracts + frontend public feature/entity/shared APIs
```

Existing permanent positive and negative architecture tests remain authoritative. AR work changes a
boundary only when the owning bounded slice proves the delta; it does not reopen already-correct layers.

## 6. Production capability sequencing

`source_present != production_enabled` is binding. Production Core v1 may enable the accepted core
release profile after AR-17 authorization and successful PC-1, while mailbox administration, bulk
mailbox operations, client↔mailbox binding, mailbox jobs/automation and outbound side effects remain
production-disabled until later capability gates.

The current Core scope includes Camoufox/profile runtime through Windows Profile Bridge. Therefore the
unfinished historical Windows updater Batch E is inherited by **AR-15** and is release-blocking for the
current Core scope: signed/versioned update manifest, trust/key rotation, side-by-side staging, safe
quiescent activation, health + Last Known Good rollback, immutable Windows publisher integration,
permanent failure-path tests and a production-equivalent Windows rehearsal must all be accepted before
AR-16/AR-17 can authorize PC-1. Profile Bridge runtime and updater remain separate failure domains.

PC-1 must consume a Production Capability / Release Profile whose cloud release-set and Windows
Bridge/updater/runtime/profile-format compatibility are mutually accepted. PC-1 is not allowed to be the
first exercise of updater signature trust, rollback or activation safety.

UI projection is never the security boundary. Production-disabled capability paths must fail closed in
the backend. The project retains one `main`, one architecture and one schema/compatibility lineage.

## 7. Operational authority

The target split is GitHub Actions/Environments for CI orchestration, merge/deployment gates and approvals,
Rust `opsctl` for typed project-specific operational semantics after bounded cutover, Wrangler/provider APIs
for actual provider mutation, and Python for validators/generators/fixtures/research and explicitly classified
helpers. `opsctl` is not a GitHub workflow wrapper or merge authority. AR-6 already classifies the resolver
promotion core for migration into an `opsctl release/promotion` command family in AR-11; that later cutover
must preserve one mutable authority rather than duplicating the current Python/provider path.

For each mutable concern there is exactly one legitimate mutable authority. Terraform and hidden generic
IaC state are not part of this architecture.

## 8. Public contract and migration discipline

AR-2 changes no public API/OpenAPI semantics and no D1 migration. Historical migration provenance is
preserved. Fresh bootstrap is not an upgrade migration. One legitimate migration executor is required;
a DB-level distributed lock is introduced only when an independent concurrent executor is proven and
cannot be eliminated.

AR-2 also performs no Cloudflare resource mutation. Its `DELETE` decision for the legacy
`GENERATION_VERIFICATION` queue is architecture input; AR-5 owns source/Wrangler cleanup and PC-1 must
not provision that queue for the Production Core release.

## 9. Security, privacy and authorization completion rules

Accepted tenant isolation, neutral disclosure, explicit resource grants, PII/secret/mail-content
redaction, idempotency/replay neutrality, version/fencing, provider ambiguous-outcome recovery and
cross-boundary reconciliation remain release gates. Relationships such as assignment or mailbox-client
association never become ACL shortcuts.

The existing mailbox onboarding and `ReauthRequired` lifecycle remains the OAuth domain authority. AR-8
extends refresh concurrency/revocation safety rather than creating a second OAuth state machine.

Resolver isolation remains intentional after AR-2: the private mailbox-secret-resolver Worker, dedicated
resolver D1 and service binding are a credential/security boundary rather than accidental duplication.

## 10. Exact-head acceptance discipline

Every bounded AR candidate follows one immutable-head protocol:

1. start from the latest accepted `main`;
2. keep the change bounded to the owning AR slice;
3. review the complete diff for unrelated files and forbidden production mutation;
4. require `behind_by=0` before acceptance;
5. require zero blocking reviews and zero unresolved review threads;
6. determine the applicable permanent workflow inventory from the candidate tree;
7. require every applicable workflow to complete successfully on one unchanged exact candidate SHA;
8. treat any new commit as invalidating all prior exact-head CI evidence;
9. re-read docs ↔ machine state ↔ issue ledger on the candidate head;
10. guarded merge commit only after the final base-currentness/mergeability check, binding the expected exact PR head SHA;
11. re-read accepted `main` after merge before starting the next slice.

## 11. Immediate next action

Post-AR-7 exact-head CI and GitHub Actions supply-chain hardening is **DONE / ACCEPTED** through
issue #302 / PR #303; exact-green candidate `e8022adb799184628f5c5706c9651d0245386d55` was merged
through protected `main` as merge commit `d94bc315faad45ed376fb302b843642fb4397659`. The accepted
architecture checkpoint remains AR-7; the immediate next sequential slice is
**AR-8 — Secrets / Keys / OAuth Refresh Concurrency**.

Throughout AR-0…AR-17:

```text
architecture_complete=false
production_core_gate=BLOCKED
production_ready=false
```

## Immutable Accepted Phase Provenance

These records are a compact projection of `architecture/accepted-phases.json`. The permanent architecture
gate verifies them so accepted product history cannot be silently rewritten while program authority changes.

Phase 1A was accepted through issue #114 / implementation PR #115; exact proven source head
`21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`; guarded squash merge
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5`.

Phase 1B was accepted through issue #120 / implementation PR #135; exact proven source head
`22b2ef36a943d07d22755bf467ec6e7c27ef081d`; guarded squash merge
`f081e0709481d6bbaa150f5518ec8552124c78de`.

Phase 2A was accepted through issue #118 / implementation PR #137; exact proven source head
`2d80ee74bc8d05657414ea4e75dcf6f41c723926`; guarded squash merge
`a1eb2833a74d9156bce8f4b1c6e92815cc0d55bc`.

Phase 2B was accepted through issue #138 / implementation PR #140; exact proven source head
`895594e35b77ddd86395300b1644e9df6a712123`; guarded squash merge
`298062ea443c31c69212cb03b3988265b6bbcd48`.

Phase 2C was accepted through issue #142 / implementation PR #143; exact proven source head
`d3ad2e774a98ad5fed2565ba410ba9923062d170`; guarded squash merge
`042d0dc72fa37e99f971d61d21544609a69c6e31`.

Phase 2D was accepted through issue #144 / implementation PR #147; exact proven source head
`ad491e2f0c9ba9f79130923fdde6fe1407af4dc5`; guarded squash merge
`26f8fa82bdad02a5a0867b0d36748b915579ef1c`.

Phase 2E was accepted through issue #148 / implementation PR #152; exact proven source head
`0cefa67abe810db079102462f33ec28fcfc73f69`; guarded squash merge
`6c6ba4564de88b40d282081e701a2d24f1611cc2`.

Phase 2F was accepted through issue #154 / implementation PR #155; exact proven source head
`c36df418f9fa877c5143327e97b60087c33ffd02`; guarded squash merge
`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`.

Phase 2G was accepted through issue #159 / implementation PR #160; exact proven source head
`85ca77b430e7d184204082aea7d51a08fdd72cf9`; guarded squash merge
`48e24f1f365d87a07bf97322c81099dd6a89f046`.

Phase 2H was accepted through issue #163 / implementation PR #164; exact proven source head
`9add9b94d0de255b93e5a7c24584fcf6756462a7`; guarded squash merge
`a32768feddb3da69b872e701bc529aad3521e1b0`.

Phase 2I was accepted through issue #167 / implementation PR #168; exact proven source head
`c1075337cfc582d0f4c00ec34b1aa7cda9ac1101`; guarded squash merge
`800c634147d6300ea3989ff0cf87ade6e2387ee9`.
