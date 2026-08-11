# Phase 2I Repository-Local Threat Model

Status: in progress; repository-local release-candidate scope only. Production readiness remains false.

## Trust boundaries

The accepted system separates browser UI, Control Plane Worker, D1 catalog, R2 immutable generations, Durable Object coordination, mailbox providers, device execution, and the local Profile Bridge. Application/domain layers remain provider-agnostic; adapters own provider mechanics.

## Threat and control matrix

| Threat | Required control | Repository-local evidence |
|---|---|---|
| Cross-tenant or IDOR access | live membership/capability/grant checks before projection or provider access; neutral denial | identity ACL, query boundary and Phase 2I hardening gates |
| Result-count disclosure | denied paths return no foreign projection/count information | query application and projection tests |
| Revoked actor receives realtime data | current authorization before catch-up/live delivery; durable cursor semantics | Phase 2G realtime policy and notification tests |
| Realtime becomes business authority | metadata-only invalidation followed by authorized refetch | frontend realtime policy and self-tests |
| Duplicate/replayed commands | idempotency/replay receipts and transaction boundaries | D1 command/mailbox tests and cross-component acceptance |
| Concurrent/stale writer | fencing, expected-version compare-and-swap and single-writer coordination | coordinator, device and generation tests |
| Unverified generation activation | immutable candidate, exact verification, then authoritative activation | generation registry, encrypted-generation and R2 boundary tests |
| Failed generation commit destroys recovery state | retained dirty local state until verified remote commit | Profile Bridge retained-operator tests |
| Provider outage or authentication expiry appears successful | explicit retry/auth/suspended/failed states | mailbox application failure tests |
| Offline/busy device appears successful | durable retry/remediation state; no false completion | device application/domain tests |
| Corrupt backup is restored | point-in-time restore plus schema/data/integrity validation | Phase 2I D1 backup/restore drill |
| Sensitive or high-cardinality telemetry | class-only metric dimensions; forbidden identifier/data classes | operational-bounds policy and negative fixtures |
| Sensitive support evidence | allowlist-only class fields and forbidden-data policy | support-bundle policy and negative fixtures |
| Dependency or CI source substitution | exact dependency versions, lock integrity, SHA-pinned actions, approved registries only | Phase 2I supply-chain policy |

## Residual External risks

Phase 2I does not convert repository-local tests into proof of production Cloudflare behavior, real mailbox-provider behavior, real Camoufox execution, physical multi-device behavior, production device-key protection, trusted signing, remote R2/key recovery, or independent cryptographic review. Those remain Phase 2J evidence requirements.

## Closure rule

A repository-owned threat is closed for Phase 2I only when its control is executable in permanent CI or explicitly represented by a fail-closed policy with negative fixtures. Any unresolved repository-owned architecture/security finding blocks Phase 2I acceptance. External evidence gaps continue to keep production readiness false without being misreported as repository-local failures.
