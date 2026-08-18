# Architecture Re-baseline v3 — AR-8 Acceptance Evidence

**Status:** ACCEPTED
**Umbrella issue:** #308
**Completion issue:** #361
**Implementation PR:** #362
**Exact-green candidate:** `81d1f0c26ff0bd3a688c2d5dc000b93640479e47`
**Guarded merge / accepted main:** `874666f6ef6eb003425c9677d558378d6dc0daaf`
**Applicable permanent workflows:** `14/14 success`

AR-8 is accepted as the complete project-wide secrets / keys / credentials hardening checkpoint. The single completion candidate covered AR-8D encryption/service-auth/static credential lifecycle hardening, AR-8E Google/Microsoft OAuth application credentials, AR-8F metadata-only operator/rehearsal contracts, and the additive Camoufox/Profile Bridge protected-domain correction discovered before closeout.

Acceptance discipline was satisfied on one unchanged candidate: `behind_by=0`, zero blocking reviews, zero unresolved review threads, guarded merge with the exact expected head, followed by accepted-main reread at `874666f6ef6eb003425c9677d558378d6dc0daaf`.

Current subject-domain authority is intentionally stage-independent:
- `architecture/credential-authority.json` — composition root over accepted AR-8B provenance;
- `architecture/credential-lifecycle.json` — durable lifecycle semantics;
- `architecture/profile-security.json` — profile/Camoufox protected-domain ownership boundaries;
- `architecture/operator-contract.json` — metadata-only operator contract.

Historical `architecture/ar8-*` artifacts and `docs/evidence/ar8-*-candidate.json` remain provenance/evidence, not competing mutable authority.

AR-8 does **not** accept real Camoufox runtime implementation, production profile-key provider/recovery proof, Windows updater/signing, or production deployment. Those remain owned by later AR-10 / AR-13 / AR-14 / AR-15 gates as applicable. Production remains fail-closed:

```text
architecture_complete=false
production_core_gate=BLOCKED
production_ready=false
```

The program therefore advances only to **AR-9 — D1 Evolution / Schema Compatibility**. No production mutation occurred in AR-8.
