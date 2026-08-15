# Browser Profile Platform — Development Plan

**Status:** normative product phase plan; pre-2J product-readiness remediation is ACTIVE / BLOCKING Phase 2J  
**Date:** 2026-08-12  
**Tracking:** Phase 2I accepted; historical R1–R9 architecture remediation closed; current repository-owned follow-up is issue #203  
**Current remediation authority:** `PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`  
**Production readiness:** unchanged; `production_ready=false`

## 1. Authority And Scope

This document defines the product phase order and the current repository execution boundary.

Current authority is intentionally separated:

- `PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md` — **current pre-2J execution authority**, tracked by issue #203;
- `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md` — **historical accepted R1–R9 closeout record**; R1–R9 remain closed and regression-protected;
- `DEVELOPMENT_PLAN.md` — product phase order and acceptance transitions;
- `ARCHITECTURE.md` + accepted ADRs — stable architecture/security invariants;
- `DATA_CLASSIFICATION.md` — data sensitivity, persistence and disclosure rules;
- `UI_ARCHITECTURE.md` — standalone product/UI target;
- `DEVELOPER_CAPABILITY_MATRIX.md` — capability/evidence accepted on `main`;
- `architecture/accepted-phases.json` — immutable accepted product-phase provenance;
- `status.json` — machine-readable current/readiness projection;
- `THREAT_MODEL.md` — canonical current repository-local threat model.

The detailed development plan that existed immediately before the issue #203 follow-up is preserved
verbatim at `history/DEVELOPMENT_PLAN_PRE_PRODUCT_READINESS_2026-08-12.md`. It is historical context,
not current execution authority where it conflicts with this document or issue #203.

Phase 2I remains the last accepted repository-local product phase. The accepted phase ledger must not
advance while issue #203 is active. A branch or PR is not accepted capability until it is merged under
the exact-head acceptance discipline below.

## 2. Current Repository State

- Accepted repository-local product phase: **Phase 2I**.
- Historical pre-2J architecture remediation: **CLOSED**, R1–R9 accepted.
- Current pre-2J product-readiness remediation: **ACTIVE / BLOCKING**, issue #203.
- Initial repository-owned follow-up findings: **P0=0, P1=5, P2=1**.
- Phase 2J: **blocked / pending repository remediation**.
- Phase 2J external production evidence: **not started**.
- Production readiness: `production_ready=false`.
- Current repository work: issue #203 remediation, beginning with Batch 0 and then bounded Batches A–F.

The current blocker was created after the R1–R9 closeout because additional product/readiness review
found repository-owned gaps in operator autonomy, mailbox-client ownership, outbound mail, production
delivery and Windows update readiness. This follow-up does not reopen R1–R9 and does not relabel any
repository-local evidence as Phase 2J external evidence.

## 3. Accepted Product Phase Ledger

The authoritative immutable provenance remains `architecture/accepted-phases.json`. The product order is:

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
pre-2J product-readiness remediation #203                     ACTIVE / BLOCKING
Phase 2J real production evidence + controlled rollout        BLOCKED / PENDING REPOSITORY REMEDIATION
```

Phase 2J cannot become `unblocked_not_started`, active or accepted before accepted Batch F explicitly
closes repository-owned P0/P1 findings and updates issue #171 from a fresh exact accepted `main`.

## 4. Non-Negotiable Clean Architecture

The only valid dependency direction is inward:

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

Mandatory ownership rules:

1. Domain code does not know Cloudflare, D1, R2, Durable Objects, Queue, Gmail, SMTP, Windows or React.
2. Use cases own orchestration, authorization intent, idempotency/retry decisions and transaction semantics.
3. Application ports are defined by application needs; adapters translate provider/storage/runtime details.
4. Worker ingress remains thin composition/transport; React is never ACL/business-rule authority.
5. Provider SDK types and SQL/D1 representations do not leak into inner layers.
6. Capability ownership stays explicit; no central backend/frontend god facade is introduced.
7. New crates are created only when a real isolation/ownership benefit exists.
8. Public browser HTTP DTO authority is Rust -> deterministic OpenAPI -> TypeScript.
9. Assignment, mailbox-client association and other relationships never imply authorization.
10. Same-D1 invariants use one atomic transaction where possible.
11. D1/R2/DO/Queue/provider/Windows cross-boundary workflows use durable transitions, idempotency,
    fencing/versioning and reconciliation rather than fake distributed transactions.

## 5. Current Remediation Scope — Issue #203

The canonical details, invariants, non-goals and acceptance conditions live in
`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`. The initial repository-owned findings are:

| Finding | Severity | Required outcome before Phase 2J |
|---|---:|---|
| F1 — ACTIVE Member cannot create Client/Profile with creator ownership | P1 | Member self-service creation with atomic explicit creator grants |
| F2 — mailbox-client ownership is not a durable explicit relationship | P1 | Zero/one active Client association per mailbox, audited bind/unbind/rebind, association != ACL |
| F3 — complete outbound mail is not a proven repository-owned product flow | P1 | Compose/reply/reply-all/forward/send with neutral contracts, Gmail send and standards-based SMTP path |
| F4 — production Cloudflare deployment/promotion path is not accepted | P1 | Workers Static Assets, immutable release, isolated staging, protected promotion and D1-safe rollout |
| F5 — production-style Windows Profile Bridge updater is not accepted | P1 | Signed/versioned manifest, verification, side-by-side activation, health check and bounded LKG rollback |
| F6 — mailbox onboarding remains too close to opaque secret-handle setup | P2 | Executable Gmail OAuth and IMAP/SMTP onboarding/rotation/re-auth/revoke workflows |

Repository-owned severity at activation is therefore **P0=0, P1=5, P2=1**.

## 6. Fixed Product Decisions For #203

These decisions are already fixed and are not planning questions for implementation batches:

- **Web UI:** React/Vite is served through Cloudflare Workers Static Assets as part of one controlled
  Worker/UI release unit and production origin; no separate Pages production project.
- **Client/Profile creation:** any ACTIVE Member may create; creation atomically persists the resource
  and an explicit creator grant. No tenant-wide visibility follows. Owner retains revoke/reassign/admin authority.
- **Mailbox ↔ Client:** mailbox has zero/one active Client association; Client has zero/many mailboxes;
  bind/unbind/rebind are explicit audited conflict-safe commands; association and authorization are separate.
- **Outbound mail:** compose/new, reply, reply-all, forward and send are mandatory before Phase 2J;
  application contracts are provider-neutral; Gmail API send and standards-based IMAP/SMTP support are required;
  Microsoft Graph is not claimed unless actually implemented and accepted.
- **Deployment:** merge to `main` does not imply blind production deployment. Target flow is PR ->
  exact-head CI -> accepted merge -> immutable build -> staging -> smoke/migration checks -> protected
  production promotion. D1 migrations require compatibility/fail-forward; code rollback is not assumed
  to be a database rollback.
- **Architecture:** Clean Architecture, explicit capability ownership and inward dependencies remain mandatory.

## 7. Bounded Execution Order

No mega-PR is allowed. Each batch starts from the latest accepted `main`; oversized work is divided at
capability/application/transaction boundaries rather than arbitrary file counts.

### Batch 0 — Governance/docs authority

Goal: make repository authority truthful before product implementation starts.

Required state after acceptance:

- Phase 2I remains accepted;
- R1–R9 remain CLOSED accepted history;
- `PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md` is current #203 execution authority;
- issue #203 is ACTIVE/BLOCKING;
- initial P0=0/P1=5/P2=1 is machine-readable;
- Phase 2J is blocked/pending repository remediation;
- `production_ready=false`;
- next repository work is #203 remediation, not Phase 2J.

Batch 0 contains no product code, no `openapi/v1/**` changes, no workflow weakening and no Phase 2J
external-evidence claim.

### Batch A — Member Client/Profile creation

- A1 — Client ACTIVE Member create + atomic creator grant.
- A2 — Profile ACTIVE Member create + atomic creator grant.

Owner-only administration remains distinct from ordinary Member creation. Creation does not create
tenant-wide visibility.

### Batch B — Mailbox-client relationship

Implement inner relationship model -> D1 schema/transaction -> eligibility/member authorization ->
bind/unbind/rebind UI. Association never becomes an ACL shortcut.

### Batch C — Mailbox onboarding and outbound mail

Implement Gmail OAuth; IMAP/SMTP credential lifecycle; provider-neutral compose/reply/reply-all/forward/send;
Gmail send adapter; SMTP adapter; frontend UX; retry/idempotency/ambiguous-outcome reconciliation.

### Batch D — Cloudflare delivery

Implement Workers Static Assets; canonical Wrangler/env/bindings; immutable source/artifact provenance;
isolated staging/prod; protected promotion; D1 expand/migrate/contract compatibility; safe fail-forward/
rollback behavior; dynamic-route versus SPA-fallback tests.

### Batch E — Windows updater

Implement versioned signed manifest; digest/signature verification and key-rotation policy; download staging;
side-by-side install; safe activation; health check; last-known-good rollback/recovery; release publisher integration.

### Batch F — Full pre-2J re-audit and closeout

Re-run the complete pre-2J repository-owned review. Acceptance requires repository-owned P0=0/P1=0,
clean architecture, authorization, mail, deployment, updater, documentation authority and exact-head CI.
Only after accepted Batch F may issue #171 be updated to the new exact accepted `main`, Phase 2J return
to `unblocked_not_started`, and real Phase 2J external production evidence begin. `production_ready=false`
remains unchanged at that transition.

## 8. Public Contract And Migration Discipline

- accepted `openapi/v1/**` remains frozen during Batch 0;
- later public API changes require explicit owning batch and compatibility/versioning proof;
- canonical Rust source owns governed browser DTOs and deterministic generated artifacts;
- additive compatible evolution is preferred within v1;
- D1 migrations are forward-only, contiguous and replay-safe;
- migrations used in staged delivery must be compatible with the code versions that can coexist during promotion;
- destructive/contract migration requires explicit sequencing and cannot rely on code rollback to reverse D1 state;
- aggregate versions are checked and cross-boundary outcomes use durable reconciliation.

## 9. Security, Privacy And Authorization Completion Rules

A capability is incomplete if its happy path works while an applicable negative property remains unproven:

- tenant isolation and IDOR resistance;
- live membership and explicit resource-grant behavior;
- neutral not-found/unauthorized disclosure;
- relationship/assignment not treated as authorization;
- raw PII/secret/mail-content absence from logs/audit/events/metrics/support bundles;
- replay/duplicate neutrality;
- version/concurrency conflict behavior;
- provider ambiguous-outcome recovery;
- stale device/session/generation fencing;
- safe recovery after partial external failure.

The frontend may improve discoverability and presentation but never becomes the authority for these rules.

## 10. Exact-Head Acceptance Discipline

Every bounded acceptance candidate follows one immutable-head protocol:

1. Start from latest accepted `main` and keep `behind_by=0` at acceptance.
2. Review the complete PR diff and confirm scope boundaries.
3. Confirm zero blocking reviews and zero unresolved review threads.
4. Inspect PR Conversation for current blocking instructions/checkpoints.
5. Determine the actual permanent `.github/workflows/**` inventory from the candidate tree.
6. Require every mandatory permanent workflow to contain real jobs and finish `completed/success` on the
   exact final candidate SHA. Zero-job/skipped workflows are not evidence.
7. Any new commit invalidates all previous exact-head CI evidence.
8. Remove temporary diagnostic workflows before the final candidate.
9. Verify frozen `openapi/v1/**` net diff is zero where the batch requires it.
10. Verify `production_ready=false` and current documentation/readiness interlocks.
11. Run the repository's fresh pre-ready exact-head interlock.
12. Mark the PR ready only after the pre-ready interlock passes on the same head.
13. Run/verify the fresh post-ready exact-head interlock on that unchanged head.
14. Squash merge only with an explicit expected head SHA.
15. Re-read `main` after merge and record accepted source head, squash SHA and workflow evidence in the
    owning issue before beginning the next batch.

No workflow result from an earlier SHA, no skipped/empty job set, no stale branch comparison and no
manual assertion substitutes for this protocol.

## 11. Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / PENDING REPOSITORY REMEDIATION

**Purpose after unblocking:** close real-world evidence that repository-local CI cannot prove. Phase 2J
is the only phase that may eventually permit `production_ready=true`.

**Current rule:** do not execute or claim Phase 2J evidence while issue #203 remains active/blocking.
The following is future Phase 2J scope only after accepted Batch F:

1. prove production Cloudflare deployment/runtime behavior on the accepted delivery path;
2. prove trusted Windows signing/update/rollback on supported hosts;
3. prove primary + secondary physical Windows hosts and real multi-device concurrency/recovery;
4. prove production device-key protection/unwrap/revoke/recovery;
5. execute escrow/key restore and remote backup/recovery drills;
6. complete privacy/retention/product-license approval;
7. complete real provider/fingerprint certification for supported lanes;
8. complete independent security/cryptographic review where applicable;
9. accept monitoring/on-call/runbook/rollout/rollback procedures;
10. perform staged rollout with explicit rollback trigger criteria;
11. promote `production_ready=true` only after every mandatory external gate has immutable reviewed evidence.

Missing or failed mandatory evidence keeps `production_ready=false`. Repository-local/synthetic proof is
input to readiness work, never a substitute for External evidence.

## 12. Standalone Product Definition Of Done

The standalone roadmap completes only after Phase 2J is accepted. At minimum:

- issue #203 repository-owned P0/P1 gaps have been closed and regression-protected;
- ACTIVE Member Client/Profile creation is self-service with atomic explicit creator grants;
- mailbox-client association is durable, explicit, auditable and separate from ACL;
- authorized Client Mail supports inbound reading plus compose/reply/reply-all/forward/send;
- mailbox onboarding is executable without hidden operator-only secret setup;
- UI + Worker ship as a controlled Cloudflare Workers Static Assets release;
- staging/protected production promotion and D1-compatible rollout are accepted;
- Windows Profile Bridge update/health/LKG rollback is production-style and verified;
- all accepted authorization, privacy, fencing, durability and recovery invariants remain intact;
- real provider/physical-host/signing/key/remote-runtime evidence is accepted in Phase 2J;
- all permanent workflows are green on exact accepted heads with zero blocking review state;
- only then may `production_ready=true` be considered.

External CRM remains future-only until the standalone product passes Phase 2J.

## 13. Immediate Next Action

Phase 2I remains the last accepted repository-local product phase. The historical R1–R9 remediation is
closed and must not be reopened. The active repository work is issue #203 under
`PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`.

Complete and accept **Batch 0** first. Do not begin Batch A before Batch 0 is merged under exact-head
discipline. Do not begin Phase 2J while issue #203 remains active/blocking. After Batch 0, start A1 from
the resulting accepted `main`.

Until accepted Batch F explicitly clears the repository blocker:

```text
issue #203 product-readiness remediation
  -> Batch 0
  -> A1
  -> A2
  -> B
  -> C
  -> D
  -> E
  -> F
  -> fresh #171 checkpoint on accepted main
  -> Phase 2J unblocked_not_started
  -> real External production evidence
```

Throughout the repository-owned remediation, `production_ready=false` remains immutable and no Phase 2J
external-evidence acceptance claim is permitted.

## Immutable Accepted Phase Provenance

These records are a compact projection of `architecture/accepted-phases.json`. They are retained in the
normative plan because the permanent architecture gate verifies that accepted product history cannot be
silently rewritten while current remediation authority changes.

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
