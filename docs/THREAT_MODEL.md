# Threat Model

**Status:** Canonical current repository-local threat model; Phase 2I accepted repository-local controls
**Production readiness:** `production_ready=false`; Phase 2J External residual risks remain unaccepted
**Method:** trust-boundary and STRIDE-oriented analysis

## 1. Protected assets

- browser profile generations: cookies, login databases, localStorage, IndexedDB and materialized state;
- profile entropy/fingerprint/network identity policy and observations;
- mailbox, proxy and OAuth secret handles;
- root wrapping keys, tenant KEK, generation DEK and device private keys;
- memberships, grants, client/contact records and historical profile assignments;
- launch intents, durable jobs, leases, fencing tokens, sessions and realtime cursors;
- runtime/Bridge installers, signatures, update metadata and immutable generation objects;
- audit, support, operational evidence, backup and recovery artifacts.

## 2. Trust boundaries

1. Browser user -> Cloudflare Access -> verified application actor.
2. Verified actor -> live tenant membership/capability/grant authorization.
3. React SPA -> Rust Control Plane Worker API; UI state is never authorization authority.
4. Worker application orchestration -> D1/R2/Queue/Durable Objects through outer adapters.
5. Mailbox application ports -> cloud provider adapters or browser/device execution lane.
6. Durable device job/claim -> Windows Profile Bridge device identity and fencing context.
7. Bridge -> local workspace/SQLite outbox -> embedded browser runtime process.
8. Immutable encrypted generation -> exact verification -> authoritative catalog activation.
9. Durable notification/realtime event -> current authorization -> metadata-only invalidation -> refetch.
10. Operators -> Cloudflare account, signing material, recovery escrow and production rollout controls.
11. Future CRM -> versioned contracts only; never direct profile/mailbox authority.

## 3. Phase 2I accepted repository-local controls

| Threat | Accepted repository-local control | Permanent evidence class |
|---|---|---|
| Cross-tenant / IDOR access | live membership/capability/grant checks before projection/provider/device/realtime access; neutral denial | identity/query/application boundary gates and cross-component acceptance |
| Result-count / existence disclosure | foreign and absent resources are public-response neutral; denied paths return no foreign projections/counts | query/transport negative fixtures |
| Revoked actor receives realtime data | current authorization before catch-up/live delivery; durable cursor semantics | Phase 2G notification/realtime policy/tests |
| Realtime becomes business authority | metadata-only invalidation followed by authorized refetch; no direct business query mutation | frontend realtime policy/self-tests |
| Duplicate/replayed command | idempotency receipts, replay neutrality and atomic governed mutation envelopes | D1/application/mailbox/device tests |
| Concurrent or stale writer | expected-version CAS, coordinator/device/generation fencing and single-writer ownership | coordinator/device/generation tests |
| Unverified/corrupt generation becomes active | immutable candidate, exact verification, quarantine/fail-closed parsing, then activation | profile-generation/encrypted-generation/R2 gates |
| Failed remote commit destroys recoverable local state | retained dirty/operator-owned state until verified remote commit | Bridge/materialization recovery tests |
| Provider outage/auth expiry reported as success | explicit retry/auth-required/suspended/failed durable states | mailbox application failure tests |
| Offline/busy device reported as success | durable retry/remediation state and bounded claims; no false completion | device domain/application tests |
| Corrupt backup/restore | point-in-time restore plus schema/data/integrity validation | Phase 2I recovery/DR drills |
| Sensitive/high-cardinality telemetry | metadata/class-only dimensions and explicit forbidden identifier/content classes | operational-bounds negative policy |
| Sensitive support evidence | allowlist-only support fields and sanitizer/forbidden-data policy | support-bundle negative policy |
| Dependency/CI source substitution | exact dependency locks, approved sources and SHA-pinned permanent actions | supply-chain/license/runtime policies |
| Malicious archive/path escape | safe paths, streaming/bounded extraction and deterministic inventory | runtime bundle/materialization gates |
| Browser/runtime command abuse | typed bounded IPC/capability allowlists; no generic privileged command channel | Bridge/runtime contract gates |

## 4. Fail-closed rules

- Unknown membership, grant, device, runtime, mailbox or generation state denies access.
- Foreign and absent resources produce indistinguishable public denial behavior.
- Authorization precedes projection, provider, device and realtime access.
- Unverified/corrupt/quarantined generation cannot become authoritative.
- Dirty or recovery-required local state is not silently evicted or overwritten.
- Expired/stale fencing, claim, generation or session state cannot write newer authority.
- Missing key/recovery evidence quarantines data rather than guessing.
- Signature/update verification failure preserves the previous accepted runtime.
- Confidential mail input stays in request bodies; sanitized mail HTML remains sandboxed and non-networked.
- Technical telemetry/support/evidence never carries raw PII, secrets, mailbox content or unbounded IDs.

## 5. Phase 2J External residual risks

Repository-local Phase 2I evidence does **not** prove production Cloudflare behavior, real mailbox-provider
behavior, real Camoufox/fingerprint behavior, physical multi-device recovery, production device-key
protection, trusted Windows signing/update, remote R2/key recovery, offline escrow restore, independent
cryptographic review, production privacy/retention approval or operational rollout/on-call readiness.

Cloudflare account compromise remains high impact; D1 has no PostgreSQL-style RLS defense in depth; an
authorized compromised endpoint/device can observe plaintext while a profile is in active use; provider
and fingerprint behavior changes independently. These risks are accepted only through the applicable
real Phase 2J evidence/review, never by relabelling synthetic tests.

## 6. Security authority and review gates

This file is the canonical current threat model. [`PHASE2I_THREAT_MODEL.md`](PHASE2I_THREAT_MODEL.md)
is Historical accepted Phase 2I evidence and remains useful provenance, but it does not override this
model.

Update this model whenever a trust boundary, cryptographic protocol, identity provider, operating-system
lane, mailbox provider, tenant model or future CRM adapter changes. Production promotion requires the
Phase 2J evidence matrix and immutable reviewed evidence for all mandatory external security/recovery
controls; until then `production_ready=false`.
