# D host enroll composition boundary

This note is temporary source-local design evidence for V2.1 prerequisite D and will be folded into `README.md` before acceptance.

`bridge-host-ops enroll` reuses the existing Bridge host-operations owner and the accepted authenticated issue/redeem API. It accepts only control-plane origin, tenant path scope, a user-scoped Access token file, correlation ID and issue idempotency key. Actor identity and authoritative device identity remain server-derived; `deviceId`, `actorId` and `csrSha256` are not CLI inputs.

The Windows path is:

```text
issue authenticated one-shot authority
-> receive server-issued device id + transient claim
-> create/reuse deterministic named LocalMachine CNG RSA signing key with ExportPolicy=None
-> create exact ClientAuth CSR DER from that key
-> redeem claim + exact CSR through stdin-only curl config
-> require returned device id to equal issued server identity
-> validate public certificate fingerprint/EKU/validity/key match
-> CopyWithPrivateKey against the exact persisted CNG key
-> install in LocalMachine/My
-> re-open and prove exact CNG key name + non-exportability
-> emit secret-free device/certificate/shipping receipt
```

The Access token, enrollment claim and CSR request body are never placed in process arguments or normal JSON output. Certificate bytes are accepted only in-memory/stdin for installation and are not emitted as evidence.

A failed or uncertain redeem is not internally retried. Because certificate bytes are deliberately not persisted server-side, the local unbound key is cleaned up and the next attempt requires a fresh issue idempotency key. Installation is fail-closed and cleans up an incomplete local certificate/key association.

No provider, D1-direct, Worker deployment, Access policy, mTLS policy or Production mutation is performed by this command. Hosted real enrollment E2E remains prerequisite E.