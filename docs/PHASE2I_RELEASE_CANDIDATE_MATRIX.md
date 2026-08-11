# Phase 2I Release-Candidate Hardening Matrix

**Status:** in progress — repository-local Phase 2I evidence only  
**Issue:** #167  
**Exact pre-2I base:** `0449e9f0576f7d26b1e1debd882cfecf92a50c53`  
**Production readiness:** unchanged; `production_ready=false`

## 1. Purpose

Phase 2I proves the accepted standalone product as an integrated repository-local release candidate before Phase 2J may collect or promote real production evidence. This document records the evidence contract for the hardening work; it is not an acceptance claim by itself.

Historical repository-local standalone composition remains preserved in `tests/cross-component/standalone-acceptance.json`. Phase 2I adds `tests/cross-component/phase2i-release-candidate.json` rather than rewriting the historical accepted proof.

The permanent checker is `scripts/check-phase2i-hardening.py`. `scripts/check-architecture.py` executes both its positive policy and its negative self-test fixtures so the matrix cannot silently lose required capability, security, failure, recovery or External-exclusion coverage.

## 2. Current integrated capability matrix

| Capability | Required repository-local outcome | Current evidence family |
|---|---|---|
| Identity | Active membership and governed identity before capability access | Worker access/session + identity application + Step 4 ACL evidence |
| Clients | Grant-safe command/query/detail path | client Worker/application + Client UI |
| Profiles | Grant-safe profile path plus immutable generation lifecycle | profile/generation Worker + Bridge/coordinator evidence |
| Mailboxes | Authorization -> eligibility -> transient provider/body query | Client Mail Worker + query/mailbox application + UI |
| Devices | Durable claim/fencing and offline-safe execution | device domain/application + Bridge operator flow |
| Realtime | Durable-first catch-up and metadata-only invalidation | notification realtime application/Worker/frontend + Phase 2G policy |
| UI | Feature-owned standalone operator surfaces with generated/public boundaries | router + Phase 2H policy + safe mail body |

This matrix is intentionally cross-capability. A source file existing is not sufficient: the permanent checker also preserves high-value authority boundaries such as authorization-before-projection, authenticated Client Mail ingress, invalidation-only realtime behavior and fail-closed Bridge ownership/recovery.

## 3. Security negative matrix

Phase 2I must retain executable evidence for all of the following:

- tenant isolation before projection/provider/device/realtime exposure;
- neutral IDOR disclosure for missing, foreign and unauthorized resources;
- revocation/suspension before query/provider/realtime exposure;
- no foreign result-count leakage on denied query paths;
- mailbox content absent from logs, audit, metrics, realtime, browser persistence and support/evidence bundles;
- realtime as invalidation/refetch only, never canonical business/query state.

The manifest is metadata-only. It rejects unsafe evidence paths, email-like payloads and sensitive-looking manifest keys. Negative self-tests deliberately remove or corrupt mandatory controls and must fail.

## 4. Failure and concurrency matrix

The release-candidate program must prove that:

- duplicate/replayed delivery has one logical canonical effect;
- stale claim/generation/coordinator fencing cannot overwrite newer authority;
- terminal failures are not represented as empty/success results;
- active or uncertain Profile Bridge writer ownership fails closed;
- device-offline paths preserve pending/remediation state rather than false success;
- provider outages map to bounded retry/auth/terminal semantics.

Repository-local evidence may use deterministic fakes and injected failures. It must not be relabeled as real provider, physical-device or production Cloudflare proof.

## 5. Recovery matrix

Required repository-local recovery properties are:

- browser execution requires current generation and fencing context;
- dirty state becomes a new immutable, exactly verified candidate before authoritative activation;
- failed remote activation/commit preserves recoverable dirty local state;
- mailbox authentication expiry is explicit and remediable, never silent success;
- realtime reconnect performs durable cursor catch-up before live continuation.

The later Phase 2I tranches add executable D1/R2/DO/Bridge backup/restore/disaster-recovery drills. Those drills are not claimed complete merely by this initial matrix gate.

## 6. External evidence that remains excluded

The Phase 2I repository-local program must continue to exclude these production claims:

- production Cloudflare deployment behavior;
- real Camoufox execution;
- real mailbox provider execution;
- production device-key protection;
- trusted signing/update chain;
- physical multi-device acceptance;
- remote R2/key recovery;
- independent cryptographic review.

These remain Phase 2J/External evidence. Missing or failed External evidence keeps `production_ready=false`.

## 7. Phase 2I delivery state

This first tranche establishes the permanent release-candidate evidence schema, current capability/security/failure/recovery coverage and fail-closed policy self-tests. It does **not** mark Phase 2I accepted.

Remaining normative Phase 2I work from `DEVELOPMENT_PLAN.md` continues in order: deeper integrated E2E scenarios, failure injection matrices, executable backup/restore/DR drills, metadata-safe SLO indicators, capacity/cost/query-plan bounds, dependency/license/security and threat-model closure, allowlist-only support bundle verification, then release-candidate contract/migration freeze and exact-head full acceptance.
