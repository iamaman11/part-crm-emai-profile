# Threat Model

**Статус:** Phase 0 baseline; requires review before production data  
**Метод:** trust-boundary and STRIDE-oriented analysis

## 1. Защищаемые Активы

- browser profile generations: cookies, login databases, localStorage, IndexedDB;
- profile entropy/fingerprint policy and network bindings;
- mailbox, proxy and OAuth secret handles;
- root wrapping keys, tenant KEK, generation DEK and device private keys;
- memberships, grants, client records and assignments;
- launch intents, leases, fencing tokens and session state;
- runtime/Bridge installers, signatures and update metadata;
- audit, evidence and recovery artifacts.

## 2. Trust Boundaries

1. Browser user -> Cloudflare Access.
2. Access identity -> application membership/grant.
3. React SPA -> Rust Worker API.
4. Browser launch intent -> Windows Profile Bridge.
5. Bridge device key -> device-bound Worker routes.
6. Worker -> D1, Durable Objects, Queues and R2.
7. Bridge -> local encrypted workspace and SQLite outbox.
8. Bridge -> embedded Camouhost IPC -> Camoufox process.
9. Operators -> Cloudflare account, signing keys and recovery escrow.
10. Future CRM -> versioned contracts, never direct profile storage.

## 3. Principal Threats And Required Controls

| Threat | Example | Required controls |
|---|---|---|
| Spoofing | forged Access subject or device | full JWT validation, live membership, device proof-of-possession, short-lived tokens |
| Privilege escalation | viewer calls operator endpoint | server-side capability checks, default deny, IDOR suite, no UI-only authorization |
| Replay | reused launch intent or command | single-use nonce, expiry, actor/device binding, idempotency record |
| Concurrent stale writer | old device uploads later | DO lease epoch, fencing token, expected profile version, immutable object key |
| Secret disclosure | credentials in logs/support bundle | secret handles, redaction, tracked-file scan, bounded audit schemas |
| Storage compromise | R2 snapshot read by attacker | application-layer authenticated encryption and scoped object operations |
| Key loss | Cloudflare account/root secret lost | offline escrow, version inventory, dual control and clean recovery drill |
| Malicious archive | path traversal/symlink escape | safe streaming extraction, canonical paths, inventory and size limits |
| Runtime supply-chain attack | tampered Bridge/Camoufox bundle | content address, signature, SBOM, side-by-side activation and rollback |
| Local theft | copied workspace/device | OS-protected device key, encrypted workspace, revoke and bounded plaintext lifetime |
| Cross-tenant disclosure | unscoped D1 query | single-tenant deployment guard, typed scope, tenant-inclusive keys, negative tests |
| Denial of service/cost abuse | intent flood, queue/R2 growth | rate limits, quotas, retry budgets, DLQ, retention and cost alerts |
| Audit tampering | mutation without trace | mutation envelope, append-only logical events, correlation and reconciliation |
| Browser escape/abuse | generic remote command | typed IPC only, capability allowlist, no generic exec or privileged localhost API |

## 4. Fail-Closed Rules

- Unknown membership, grant, device, runtime or generation state denies access.
- Foreign and absent resources produce indistinguishable public responses.
- Unverified generation cannot become active.
- Dirty local state cannot be evicted.
- Unknown snapshot file is not silently discarded.
- Expired/stale fencing token cannot activate a generation.
- Missing key/recovery evidence quarantines data rather than guessing.
- Signature or update verification failure preserves the previous runtime.

## 5. Explicit Residual Risks

- Cloudflare account compromise remains a high-impact control-plane event.
- D1 lacks PostgreSQL RLS defense in depth.
- A compromised authorized endpoint can observe plaintext while a profile is in use.
- External fingerprint checkers and target sites change independently.
- A physical device with an active unlocked session can expose profile state.
- Current one-device smoke test does not prove multi-device or disaster recovery.

## 6. Review Gates

The threat model must be updated when a trust boundary, cryptographic protocol,
identity provider, operating system lane, mailbox provider, second tenant or CRM
adapter is introduced. Production promotion requires evidence links in
`TEST_EVIDENCE_INDEX.md` for key recovery, device revoke, stale-writer rejection,
archive corruption and clean-environment restore.
