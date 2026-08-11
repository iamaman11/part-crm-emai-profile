# Phase 2I Release-Candidate Hardening Matrix

**Status:** in progress — repository-local Phase 2I evidence only  
**Issue:** #167  
**Exact pre-2I base:** `0449e9f0576f7d26b1e1debd882cfecf92a50c53`  
**Production readiness:** unchanged; `production_ready=false`

## Purpose

Phase 2I proves the accepted standalone product as an integrated repository-local release candidate before Phase 2J may promote real production evidence. Historical standalone evidence remains preserved in `tests/cross-component/standalone-acceptance.json`; Phase 2I adds new evidence instead of rewriting accepted history.

Permanent Phase 2I policy is enforced by:

- `scripts/check-phase2i-hardening.py`;
- `scripts/check-phase2i-operational-bounds.py`;
- `scripts/check-phase2i-supply-chain.py`;
- `scripts/check-phase2i-support-bundle.py`;
- `scripts/check-architecture.py`, which runs policy checks and negative fixtures.

## Integrated capability surface

| Capability | Repository-local release-candidate property |
|---|---|
| Identity | Active membership and governed authorization before capability access |
| Clients | Grant-safe command, query and detail paths |
| Profiles | Grant-safe profile access plus immutable generation lifecycle |
| Mailboxes | Authorization and eligibility before transient provider access |
| Devices | Durable claim, fencing and offline-safe execution |
| Realtime | Durable-first catch-up and metadata-only invalidation |
| UI | Feature-owned standalone operator surface with safe public boundaries |

The Cross-Component Acceptance Gate executes the integrated identity/query/mailbox/device invariants, realtime/UI negative policies, generation checks, recovery drills, application/adapter composition, synthetic Profile Bridge flow, and frontend contract/build in one workflow.

## Security, failure and concurrency

Repository-local evidence covers tenant isolation, neutral denied-resource behavior, revocation before exposure, result-count non-disclosure, metadata-only realtime behavior, duplicate/replay safety, stale-fence rejection, explicit terminal failures, Profile Bridge busy handling, offline-device remediation, and bounded mailbox provider failure states.

These properties are enforced by executable tests or fail-closed policy checks with negative fixtures.

## Recovery and disaster recovery

`docs/PHASE2I_DISASTER_RECOVERY_RUNBOOK.md` defines the repository-local recovery contract. CI executes:

- D1-compatible point-in-time backup and restore with schema/data/integrity comparison and corrupt-backup rejection;
- immutable encrypted-generation recovery checks;
- coordinator replay/projection-repair/fencing checks;
- retained dirty-local Profile Bridge recovery checks.

Remote provider, remote object/key, and physical multi-device recovery remain Phase 2J evidence and are not claimed by Phase 2I.

## Operational bounds

`tests/operations/phase2i-operational-bounds.json` plus `scripts/check-phase2i-operational-bounds.py` enforce low-cardinality operational dimensions and source-backed capacity limits.

| Bound | Current source-backed limit |
|---|---:|
| Query page | 100 |
| Claimable device jobs | 50 |
| Realtime audience page | 200 |
| Mailbox job attempts | 10 |
| Mailbox retry ceiling | 900000 ms |

Production latency, error-budget, provider-rate, Cloudflare-cost and physical-device calibration remain External evidence.

## Supply chain and threat model

`scripts/check-phase2i-supply-chain.py` enforces SHA-pinned GitHub Actions, exact dependency versions, approved lockfile sources and integrity metadata. The Cross-Component gate additionally checks installed Rust and npm dependency license metadata after dependencies are materialized.

`docs/PHASE2I_THREAT_MODEL.md` maps repository-owned threats to permanent controls and records the External residual-risk boundary.

## Support/evidence bundle

`tests/support/phase2i-support-bundle.json` and `scripts/check-phase2i-support-bundle.py` define an allowlist-only support evidence contract. Only bounded class-level fields are accepted; negative fixtures prove that identifier-like or address-like payloads are rejected.

## Delivery state

Completed in the current Phase 2I branch:

1. release-candidate manifest and hardening gate;
2. expanded integrated E2E/security/failure coverage;
3. repository-local D1/R2/coordinator/Bridge recovery drills and runbook;
4. operational indicator policy and source-backed capacity/query-plan bounds;
5. supply-chain source policy, threat model and installed dependency license checks;
6. allowlist-only support/evidence bundle policy.

Still required before Phase 2I acceptance:

1. complete the current exact-head CI validation for the new supply-chain/license/support gates;
2. establish the release-candidate contract/migration freeze;
3. perform final exact-head acceptance with no unresolved repository-owned findings, `behind_by=0`, clean reviews/threads and all 12 permanent workflows successful on one unchanged head.

Phase 2J remains blocked and `production_ready=false` remains unchanged until Phase 2I is accepted.
