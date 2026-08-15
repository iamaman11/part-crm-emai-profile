# Architecture Re-baseline v3 — AR-0 Final Delta Audit

**Status:** AR-0 candidate evidence; not active execution authority until AR-1  
**Audit base:** `5be54c2989dbfa22822d3692e22156f23d2a4602`  
**Tracking:** #266 / Draft PR #267; subordinate pre-production authority work #268  
**Production readiness:** `false` and unchanged  
**Production Core v1 gate:** `BLOCKED`

## 1. Audit method

Repeated repository reads covered backend/application layering, frontend/contracts, architecture fitness tests, Cloudflare topology/promotion, D1 migrations, resolver credential/key storage, recovery, Profile Bridge, documentation authority, GitHub governance and operational/research executables.

Classifications: `PRESENT`, `PARTIAL`, `MISSING`, `CONFLICT`, `SUPERSEDED`; runtime also uses `KEEP/SIMPLIFY/DEFER/DELETE_CANDIDATE`.

The audit intentionally distinguishes:

- source present;
- accepted architecture;
- production-enabled capability;
- architecture complete;
- production gate authorized;
- production ready.

These are not interchangeable states.

## 2. Baseline truth

- Phase 2I remains accepted; historical Phase 2J remains blocked; `production_ready=false`.
- R1–R9 and Pre-2J A/B/C/D1/D2/repository-side D3 remain accepted history/evidence.
- #203/current Pre-2J plan remain execution authority until separately accepted AR-1.
- Pre-2J D3 external issue #251 is currently **open** and must be classified during authority cutover rather than rewritten as if v3 always existed.
- audit-base `main` is `5be54c2989dbfa22822d3692e22156f23d2a4602`.
- PR #267 currently starts from that exact base.
- GitHub branch metadata reports `main` as `protected=false` and repository rulesets are empty.
- the GitHub `production` Environment is materially protected: a required reviewer exists, `can_admins_bypass=false`, and its custom deployment branch policy contains only `main`.
- therefore production-environment protection and `main` merge protection are two different security boundaries; the former is a strength, the latter is a current governance gap.

## 3. Primary sequencing correction discovered after the first candidate

The first candidate plan placed real production provisioning/promotion in AR-16 and only then ran the whole-project audit in AR-17. That ordering is rejected.

Correct mandatory lifecycle:

```text
AR-0..AR-15
architecture + operational remediation/rehearsal
NO production rollout
        ->
AR-16
final whole-project 10/10 audit
P0=0 / P1=0
NO production mutation
        ->
AR-17
architecture closeout
freeze canonical authorities
Production Core v1 gate BLOCKED -> AUTHORIZED
production_ready remains false
NO production deployment
        ->
AR ENDS
        ->
PC-1 Production Core v1 provisioning/promotion
        ->
PC-2 Mailbox Administration
        ->
PC-3 Mailbox Jobs/Automation
        ->
PC-4 Outbound/subsequent capabilities
```

Reason: production-like provisioning, migration, release, recovery and convergence can and must be proven in rehearsal/staging/disposable environments. Real production mutation adds operational risk but is not required to prove architecture correctness.

## 4. Backend/application

### PRESENT

- provider-independent domains;
- application ports and extracted capability use cases;
- outer Cloudflare adapters;
- capability Worker dispatch;
- real composition module;
- dependency/ownership/thin-Worker negative gates;
- governed D1/idempotency/concurrency patterns;
- provider-neutral outbound-mail ports/use case.

### PARTIAL hotspots

- broad `application-ports`/`cloudflare-adapters` require ownership measurement, not speculative splits;
- concrete construction also exists in `lib.rs::binding_probe`, outside `composition.rs`;
- Client Mail read/send classifier ownership is split;
- Outbound Mail handler duplicates eligibility, constructs D1/query/intent/provider adapters, owns source-message access plumbing and provider selection;
- shared Profile/Generation application cluster is cohesive/inward enough that extraction is unproven.

AR-4 candidates: composition-root consolidation; Client Mail classifier normalization; Outbound Mail source-access/provider-router/composition extraction with existing Worker-boundary checker extension; profile application extraction only if AR-3 proves benefit.

## 5. Rust workspace / modularity conclusion

The repository already has a layered workspace with domains, use-cases, ports, adapters and executable/composition roots. The quality gap is not “too few crates” or “too much code in one language”. The gap is proof of semantic ownership, dependency direction and singular composition authority.

Cross-domain dependencies must be intentional, acyclic, documented and fitness-tested. Crate split/merge is allowed only when it measurably reduces ownership ambiguity, fan-in/fan-out, compile/change blast radius or cognitive load.

No broad Rust rewrite is justified.

## 6. Frontend/contracts

### PRESENT

- sibling feature internals/alias bypass mechanically rejected;
- router consumes feature public APIs;
- shared API is transport primitives;
- feature APIs use generated DTOs;
- additive one-shot contract authorities and drift checks exist.

### PARTIAL

- target (`UI_ARCHITECTURE`) versus accepted/composed/production-enabled state must become machine-explicit;
- aggregate generated-contract architecture inventory is not yet proven complete for every generated TS module;
- Production Core capability exposure needs one server-authoritative release-profile projection rather than ad-hoc frontend flags.

Correct action: extend existing architecture inventory; do not create another competing registry.

## 7. Production Capability / Release Profile finding

Current repository search does not expose canonical `production_enabled` / `release_profile` semantics in the accepted architecture inventory.

This is an explicit pre-production architecture gap.

Machine authority must distinguish at least:

```text
source_present
accepted
production_enabled
environment
release_profile
dependencies
compatibility
backend_enforcement
frontend_projection
activation_gate
```

Initial intended rollout:

### Production Core v1 enabled after AR closeout + PC-1

- authentication/authorization/membership;
- users;
- clients/customer cards;
- browser profiles;
- Camoufox/profile runtime;
- single and bulk browser-profile operations;
- client <-> browser-profile binding;
- required audit/health/readiness/observability/release/recovery foundations.

### Source-present but production-disabled at Core v1

- mailbox administration;
- bulk mailbox operations;
- client <-> mailbox binding;
- mailbox jobs/automation;
- outbound mail/email side effects unless separately accepted.

No long-lived reduced production branch or mailbox fork is required or allowed.

## 8. Documentation/governance

### PRESENT

Root README, docs index/development/capability matrix, stable architecture/threat/data/contract authorities, documentation drift and architecture-inventory checks.

### CONFLICT/PARTIAL

- AR-1 authority closure is larger than docs: checkers/generators/release-freeze logic encode #203/current plan assumptions;
- `IMPLEMENTATION_PLAN.md` remains current-looking/implementation-ready;
- `PROFILE_LIFECYCLE_PLAN.md` remains current-looking while carrying stale policy assumptions;
- accepted historical slice docs can contain old rules/remaining work and need explicit status rather than rewriting;
- release-freeze/check scripts may hard-code predecessor authority;
- #203/#251/#266/#268 relationships must become explicit provenance/authority relationships;
- branch/ruleset mechanical enforcement is weaker than guarded-merge expectations.

Document statuses target: `CURRENT_AUTHORITY`, `TARGET`, `ACCEPTED_HISTORICAL`, `EVIDENCE`, `RUNBOOK`, `GENERATED_PROJECTION`, with `SUPERSEDED` where a forward status is required.

## 9. GitHub governance finding

Verified current state:

```text
production Environment:
  required reviewer = present
  can_admins_bypass = false
  deployment branch policy = main only

main branch:
  protected = false
  required status checks = off via branch metadata

repository rulesets:
  []
```

Conclusion: production deployment approval has a good protection foundation, but repository merge/governance policy is not yet mechanically at the same level.

Before architecture closeout, CI/PR requirements must be enforced by branch protection and/or rulesets rather than relying only on procedural discipline.

## 10. Architecture inventory

### PRESENT

Existing `architecture/inventory.json` is already the correct machine-readable architecture foundation.

### PARTIAL target

Extend it—not a parallel registry—to complete:

- capability ownership;
- Production Capability / Release Profile;
- runtime-resource ownership;
- concurrency ownership;
- all generated contracts;
- document status;
- executable-tool roles;
- exact Python dispositions;
- environment/release compatibility projection.

## 11. Full Python estate / `opsctl`

### Verified conclusion

The correct architecture question is not “how many Python lines should be rewritten in Rust?”. Python remains appropriate for deterministic validators, generators, fixtures, CI helpers and bounded research/evidence tooling.

### Current evidence gap

A complete mechanically proven per-file Python estate with **every repository-owned `.py`**, exact LOC, callers, workflow callers, remote reads/mutations, data/secret access, current authority and disposition has not yet been completed.

Therefore AR-6 cannot be considered satisfied until that census exists.

Allowed dispositions:

- `KEEP_PYTHON`;
- `MOVE_OPSCTL_READ`;
- `MOVE_OPSCTL_MUTATION_LATER`;
- `MERGE`;
- `RETIRE`;
- `DELETE`.

Rust `opsctl` is currently MISSING and must start read-only:

```text
opsctl inventory
opsctl plan
opsctl doctor
opsctl drift
```

Mutable operations cut over one lifecycle at a time only after parity/rehearsal and retirement of the old mutable authority.

## 12. Cloudflare topology

### KEEP

Control-plane Worker/Static Assets; dedicated mailbox-secret-resolver/service binding; catalog D1; resolver D1; generation R2; ProfileCoordinator DO; NotificationHub DO; integration-events Queue; `MAILBOX_JOBS` Queue/DLQ; Access + application authorization.

Resolver isolation remains a justified security boundary unless a later threat-model proof says otherwise.

### DELETE_CANDIDATE

`GENERATION_VERIFICATION` Queue remains a deletion candidate pending proof of no external/independent consumer. Removal must atomically cover config, bindings, probes/checkers, runtime assumptions, physical resource and docs/inventory.

Providers: Gmail read/send KEEP; IMAP+SMTP KEEP; Graph OAuth/read/delta KEEP; Graph `Mail.Send` DEFER; browser/Bridge lane KEEP at current evidence level.

## 13. MAILBOX_JOBS Queue / DLQ

Preserve Queue/DLQ and at-least-once semantics. No new mailbox-domain DLQ state machine is required.

Target recovery:

```text
inspect envelope
  -> resolve tenant/binding/job/version
  -> load current D1 authority
  -> validate ownership/version/fence
  -> controlled requeue | rerun | retire
  -> metadata-only evidence
```

Read-only inspect/doctor precedes mutable DLQ actions.

## 14. Executable tooling examples

Current sampled classifications remain hypotheses/evidence until the full Python census:

- `runtime_bundle.py`: synthetic evidence KEEP;
- fake Camouhost runtime: synthetic evidence KEEP;
- `fingerprint_certify.py`: bounded external research/evidence;
- `r2_s3_canary.py`: ephemeral app-test-data mutator;
- `profile_browser.py`: `EXTERNAL_RESEARCH_ONLY` candidate; guard/retire if needed;
- `cloud_profile_smoke.py`: `HISTORICAL_QUARANTINE` candidate because it retains obsolete mutable R2 active-pointer behavior versus immutable generation objects + D1 active-generation authority.

AR-10 executes only classifications proven by the complete AR-6 inventory.

## 15. Operations/release

### PRESENT

Canonical Wrangler; environment required-secret checks; immutable resolver + control-plane/SPA artifacts; exact-source/no-rebuild promotion foundations; protected production Environment; pinned Wrangler; resolver-before-control-plane order; D1 ledger preflight; smoke/attestation mechanisms.

### PARTIAL

- typed environment identity versus derived deploy-manifest authority;
- canonical `rehearsal/staging/production` semantics;
- concern-by-concern operational authority cutover;
- multi-component release-set/protocol/schema compatibility;
- `opsctl` read model;
- repository governance enforcement;
- explicit separation between architecture closeout and actual production mutation.

Wrangler remains Worker/configuration execution authority. GitHub remains orchestration/approval boundary. `opsctl` owns typed project operational semantics after bounded cutover. Python remains valid for clean deterministic repository/CI roles.

## 16. Database evolution

### PRESENT

Catalog/resolver migrations and bootstrap authority; fresh-bootstrap vs upgrade distinction; replay/parity negatives; exact remote migration-ledger preflight; migration/invariant CI.

### PARTIAL

No fully canonical migration classes, per-component min/max schema windows, rollback blocker or unified concurrency ownership yet.

Correction from re-review: do **not** automatically add a database distributed lock. First enforce one legitimate migration executor plus workflow/ops concurrency. Add a DB-level lock only if an independent concurrent mutation surface remains unavoidable.

Preserve historical migration provenance. A fresh current baseline/epoch may be introduced later only with semantic-convergence proof and must not be called `V2` unless a real compatibility break is accepted.

## 17. Credentials/OAuth/keys

### PRESENT — stronger than rewrite assumptions

- mailbox onboarding domain is versioned/CAS-governed and owns `Pending -> Active -> ReauthRequired -> Active/Disabled/ConfigError` transitions;
- Gmail/Microsoft OAuth ceremonies have bounded state/code/expiry/replay semantics and opaque credential handles;
- resolver provider adapter supports authorization-code exchange and refresh-token grant for Google/Microsoft;
- resolver paths automatically refresh near-expiry credentials; explicit Microsoft Graph refresh exists;
- replacement refresh tokens and expiry are persisted;
- resolver encrypted store uses versioned encryption keys and rotation reconciliation;
- contact keyrings are versioned/current-write historical-read foundations.

### PARTIAL — exact AR-8 delta

1. refresh concurrency is not proven race-safe; current conceptual path is load -> provider refresh -> ordinary encrypted-record upsert without a visible per-handle fence;
2. provider revocation -> application `ReauthRequired` reconciliation must be one durable lifecycle;
3. credential inventory must separate repository-safe policy from protected live state;
4. uniform logical credential issue/import -> validate -> bind -> switch -> verify -> revoke-previous lifecycle remains incomplete;
5. production-like rotation rehearsal remains missing.

No second OAuth/onboarding state machine is created.

## 18. Recovery

Repository-local Phase 2I recovery is PRESENT. Missing level is disposable remote catalog/resolver D1 restore, R2/key recovery, credential/OAuth re-establishment, Queue reconciliation, measured RTO/RPO and full post-restore application invariants.

AR-14 provides this evidence without touching production.

## 19. Release model

Immutable component artifacts, exact-source/no-rebuild and deployment evidence foundations are PRESENT.

Explicit release-set object with all component identities/digests, protocol compatibility, per-DB schema windows, deploy order/topology revision and same-bits promotion policy is PARTIAL.

AR-11 completes the model. Real production promotion happens only in PC-1 after AR closeout.

## 20. Windows

Profile Bridge runtime is substantial/composed-synthetic. Missing capability is production updater/publisher: signed manifest, trusted-key policy, staged side-by-side install, activation, health, LKG rollback and failed-update recovery. AR-15 owns updater only.

## 21. Summary

### PRESENT

Strong layered architecture, backend/frontend negative gates, capability use-case separation, generated-contract discipline, canonical Wrangler/resolver boundary, immutable release foundations, D1 migration/bootstrap, versioned keyrings, real OAuth refresh primitives, repository-local recovery, exact-head CI discipline, substantial Bridge runtime and protected production Environment approval.

### PARTIAL

Composition singularity; Client Mail ownership; Outbound Mail thin transport; capability/release-profile machine authority; complete generated-contract coverage; document status; exact executable/Python roles; environment/deploy authority; GitHub branch/ruleset enforcement; race-safe credential refresh; logical credential lifecycle; D1 schema compatibility; release-set compatibility; remote recovery/rotation; developer/operator single path.

### MISSING

Read-only Rust `opsctl`; complete exact Python disposition inventory; canonical rehearsal environment lifecycle; protected-live credential inventory integration; schema-window enforcement; production Windows updater; remote whole-system recovery rehearsal.

### CONFLICT

- first PR #267 candidate sequencing put production provisioning before final audit;
- observable unprotected `main` + empty rulesets versus guarded-merge expectation;
- current-looking stale root plans/historical docs without full status classification;
- any future dual operational mutator;
- any refresh implementation that remains uncoordinated after AR-8;
- source-present mailbox/outbound code being mistaken for production authorization.

## 22. Corrected execution gates

### AR-16 — final audit

Must run on latest accepted `main` and end with:

```text
P0=0
P1=0
```

No production mutation.

### AR-17 — architecture closeout

May set only:

```text
architecture_complete=true
production_core_gate=AUTHORIZED
production_ready=false
```

No production deployment.

### PC-1 — Production Core v1

First real production provisioning/promotion. Only successful protected production evidence may set `production_ready=true` for Core v1 scope.

Later capabilities activate independently through PC-2/PC-3/PC-4.

## 23. D3 compatibility

Repository-side D3 machinery is high quality and should be preserved as predecessor evidence. Issue #251 is currently open. V3 must classify its future status explicitly during AR-1/AR-2.

If an unaccepted D3 external target conflicts materially with the accepted v3 topology, do not provision obsolete production resources merely to close old sequencing. Preserve accepted repository-side evidence and consciously supersede only the unaccepted external target.

## 24. AR-0 acceptance recommendation

Accept AR-0 only after the canonical plan, this audit, repeated-review record and machine transition all reflect:

- corrected `AR-16 audit -> AR-17 closeout -> PC-1 production` sequencing;
- Production Capability / Release Profile requirement;
- exact Python -> opsctl disposition requirement;
- GitHub governance split between production Environment and unprotected `main`;
- environment/secret/release/recovery findings;
- no-production-mutation AR invariant.

Every permanent workflow must then pass on one exact unchanged candidate head. Any amendment invalidates previous exact-head evidence.

AR-0 changes no product code, OpenAPI, migration SQL, workflow, Cloudflare/provider resource, secret or deployment. AR-1 is the first authority-changing slice.
