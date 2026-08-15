# Architecture Re-baseline v3 — Repeated Research Review

**Status:** AR-0 research/evidence; not execution authority  
**Audit base:** `5be54c2989dbfa22822d3692e22156f23d2a4602`  
**Tracking:** #266 / #267 / subordinate #268

## Purpose

This record preserves why the v3 candidate changed after repeated reads. The canonical plan is the final target; this file preserves evidence/reasoning without turning intermediate drafts into competing authorities.

## Pass 1 — do not rewrite a mature architecture

The repository already has provider-independent domains, application ports, capability use cases, outer adapters, composition, architecture gates, generated contracts, immutable release mechanics, D1 bootstrap/migrations, resolver isolation, recovery and Profile Bridge runtime.

Conclusion: v3 is delta remediation, not greenfield rewrite.

## Pass 2 — governance, operations, topology, recovery, keys

### Authority is mechanically distributed

Documentation checkers, architecture inventory generation and release-freeze logic encode current predecessor assumptions. AR-1 must switch the full authority closure atomically, not only INDEX/DEVELOPMENT_PLAN/status.

Root `IMPLEMENTATION_PLAN.md` and `PROFILE_LIFECYCLE_PLAN.md` remain current-looking; historical slice documents can contain stale policy wording. Historical truth stays, but every document that can influence implementation needs a status class.

### Recovery/keyrings already exist

Phase 2I repository-local recovery is substantial. Remote/disposable production-like rehearsal is the gap.

Contact/resolver keyrings are versioned; resolver also has key reconciliation/rotation ledger. V3 standardizes lifecycle/governance and rehearses it rather than replacing cryptography.

### Operational authority is not “Python”

Current Cloudflare mutation is repository policy/generation + GitHub Actions/Environment approval + pinned Wrangler. `opsctl` therefore starts read-only with an operations authority ledger. Language replacement is not the goal.

### Runtime topology

Resolver isolation is a justified credential-security boundary and initial KEEP.

`GENERATION_VERIFICATION` Queue remains a DELETE candidate pending proof of no external consumer.

Graph is capability-specific: OAuth/read/delta KEEP; Graph send is not implied.

## Pass 3 — frontend, contracts, research tools, Client Mail

### Frontend is already strongly modular

Sibling feature internals and alias bypass are rejected; router consumes public APIs; shared API is transport primitives; feature APIs use generated DTOs. No broad frontend rewrite.

### Target vs accepted must be machine-visible

`UI_ARCHITECTURE.md` can intentionally describe future target topology; capability matrix/inventory describe accepted behavior. Historical accepted slice docs can also carry old decisions. Add explicit document status to the existing architecture hierarchy rather than another registry.

### Contract inventory must be complete

Generated contract modules must all have one aggregate ownership/generator row. Bounded generators should be preserved; the aggregate map must become complete rather than replacing working generation.

### `tools/` belongs in operational architecture

Runtime-bundle and fake Camouhost are synthetic evidence; fingerprint certification is bounded external research/evidence; R2 canary is ephemeral app-test-data mutation. `profile_browser.py` is direct persistent Camoufox research and must be explicitly external-research-only/hardened or retired.

`cloud_profile_smoke.py` retains obsolete mutable-R2 active-pointer behavior while current authority uses immutable generation objects + D1 active-generation registry. It is a historical quarantine/removal candidate, not supported runtime.

### Client Mail cleanup is concrete

Read/send classifier ownership is split. Outbound Mail use-case core is sound, but Worker transport duplicates eligibility, constructs concrete D1/query/provider adapters, orchestrates source access and chooses provider. Target: application source-access need, outer provider router, composition bundle, no duplicate transport authorization and extension of the existing Worker-boundary checker.

## Pass 4 — data classification, OAuth refresh, concurrency

### Secret “metadata” is not automatically repository-safe

`DATA_CLASSIFICATION.md` requires a split between repository-safe policy and protected live operational identities/handles/state.

Correct model:

- **repository policy registry:** only fields explicitly safe under classification;
- **protected live operational inventory:** actual sensitive active key/credential identity/version, opaque handles, provider/security state and other targeting-relevant configuration.

`opsctl doctor` may compare protected live state to policy, but Git/evidence receives only explicitly allowed redacted/digest/status projections.

### Onboarding/Reauth state machine already exists

Mailbox domain already has versioned/CAS-governed `Pending -> Active -> ReauthRequired -> Active/Disabled/ConfigError`. Gmail/Microsoft OAuth ceremonies already bind actor/tenant/version and handle replay/expiry; no second state machine is needed.

### Refresh is implemented, but refresh concurrency is not yet proven

Resolver supports Google/Microsoft refresh-token grants and runtime refresh paths. The remaining gap is per-handle concurrency: load -> provider refresh -> ordinary encrypted-record upsert lacks a proven revision/lease/fence preventing stale refresh overwrite.

AR-8 must make explicit and implicit refresh share one authority and permanently test races/replay. Invalid/revoked refresh credentials must reconcile durably to existing `ReauthRequired`.

## Pass 5 — independent whole-project re-baseline and production-boundary correction

A further independent read of current GitHub state produced four material corrections/additions.

### 5.1 Production provisioning must not be inside the Architecture Re-baseline

The previous candidate sequence was:

```text
AR-16 production provisioning/promotion
AR-17 whole-project audit
```

That sequence is rejected because it mutates production before proving the architecture has zero unresolved P0/P1 findings.

Correct sequence:

```text
AR-0..AR-15 remediation/rehearsal
  -> AR-16 final whole-project audit (P0=0/P1=0; no production mutation)
  -> AR-17 Architecture Closeout (gate authorization only; no production mutation)
  -> PC-1 Production Core v1 provisioning/promotion
  -> PC-2 Mailbox Administration
  -> PC-3 Mailbox Jobs/Automation
  -> PC-4 Outbound/subsequent capabilities
```

Architecture completion, production gate authorization and production readiness are now explicitly separate states.

At successful AR-17:

```text
architecture_complete = true
production_core_gate = AUTHORIZED
production_ready = false
```

Only successful PC-1 evidence may make Core v1 production-ready.

### 5.2 Source-present is not production-enabled

The source tree may contain mailbox/outbound/automation code while Production Core v1 exposes only users, clients and browser-profile/Camoufox capabilities.

This requires one machine Production Capability / Release Profile authority with server-side fail-closed semantics. UI visibility is a projection, not the security boundary.

The architecture inventory is the foundation; do not create a second independent capability registry.

### 5.3 Full Python estate must be evidence-complete before migration conclusions

The previous candidate classified several representative Python tools, but a later independent review could not honestly certify a complete per-file `.py` census with exact LOC/callers/mutation authority from the available evidence.

Therefore AR-6 is strengthened:

- inventory every repository-owned `.py` executable/script;
- exact LOC;
- owner/purpose;
- direct/workflow callers;
- remote reads/mutations;
- secret/customer-data access;
- current mutable authority;
- lifetime/replacement;
- exact disposition.

Allowed dispositions:

`KEEP_PYTHON`, `MOVE_OPSCTL_READ`, `MOVE_OPSCTL_MUTATION_LATER`, `MERGE`, `RETIRE`, `DELETE`.

After cutover, a new unclassified executable Python file fails CI.

`opsctl` still starts read-only (`inventory`, `plan`, `doctor`, `drift`).

### 5.4 GitHub production protection and `main` protection are not the same boundary

Fresh verification shows:

- `production` Environment has a required reviewer;
- `can_admins_bypass=false`;
- custom deployment branch policy allows only `main`;
- `main` branch metadata reports `protected=false` and status checks off;
- repository rulesets endpoint returns an empty list.

Conclusion: production approval controls are a strength, but PR/required-check discipline is not yet mechanically equivalent. AR-7/closeout must reconcile this gap with branch protection and/or rulesets rather than relying only on procedure.

### 5.5 D3 predecessor status was rechecked

Issue #251 is currently open. Any earlier statement that it was closed is superseded by this fresh direct read.

AR-1/AR-2 must classify #251 as predecessor/current external D3 work without rewriting accepted history. V3 must not provision obsolete production resources merely to satisfy an old unaccepted sequence if the final architecture intentionally supersedes that target.

### 5.6 D1 locking must solve a proven concurrency problem, not satisfy a pattern

Migration concurrency remains mandatory, but a database-level distributed lock is not automatically mandatory.

First target:

- one legitimate migration executor;
- protected workflow/ops concurrency;
- fail-closed compatibility checks.

Add a D1-level lock/fence only if an independent concurrent executor remains possible and cannot be eliminated cleanly.

## Design corrections caused by repeated research

The final v3 candidate now differs materially from its first draft:

1. AR-1 is a complete mechanically checked authority transaction.
2. Existing architecture inventory is extended, not replaced by parallel registries.
3. Current/target/historical document status is machine-readable.
4. Generated-contract inventory reaches complete ownership/generator coverage.
5. Production Capability / Release Profile becomes a machine dimension of the canonical architecture hierarchy.
6. Operational inventory covers scripts/tools/research executables by role/environment.
7. Full Python census is mandatory before Python->`opsctl` migration conclusions.
8. `opsctl` is authority/cutover-driven, not Python replacement.
9. Recovery/keyring work is convergence/rehearsal, not reconstruction.
10. Credential inventory is two-level and data-classification-driven.
11. Existing onboarding/Reauth/OAuth ceremonies are preserved.
12. Token refresh is recognized as implemented; AR-8 specifically fixes refresh concurrency/reconciliation.
13. Release is an immutable multi-component release set.
14. Runtime simplification includes historical executable quarantine.
15. Client Mail/Outbound Mail has explicit bounded cleanup tasks.
16. Existing frontend architecture and checker frameworks are preserved/extended.
17. Production Environment protection and `main` merge protection are treated as separate governance surfaces.
18. Database locking is added only if a real independent concurrency surface requires it.
19. AR-16 is the final P0/P1 audit; AR-17 is closeout/gate authorization; production begins at PC-1.
20. Later capabilities are activated independently without forks or reduced long-lived production branches.

## Research conclusion

The repository is healthy enough that a broad rewrite would reduce quality. The path to 10/10 is targeted: eliminate authority ambiguity, finish ownership maps, remove residual transport/wiring leakage, complete capability/release-profile and Python machine authorities, make refresh/schema/release concurrency explicit, mechanically enforce GitHub governance, and prove independent operational lifecycles through rehearsal/rotation/recovery before any real production mutation.

AR-0 remains Draft until all permanent workflows pass on the final unchanged head. Every amendment invalidates prior exact-head evidence.
