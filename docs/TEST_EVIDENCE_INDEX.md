# Test And Evidence Index

**Статус:** normative evidence registry  
**Дата:** 2026-08-05

A claim is accepted only within the scope of its referenced evidence. Passing one
smoke test does not promote unrelated production gates.

## 1. Existing Evidence

| Evidence | Status | Proves | Does not prove |
|---|---|---|---|
| `RESEARCH_FINDINGS.md` | verified baseline | corpus inventory, runtime observations, known prototype defects | production implementation or unchanged historical source tree |
| `CLOUD_PROFILE_SMOKE_TEST.md` | passed with limitations | synthetic one-device create/close/encrypt/R2/restore/replay and bounded fingerprint replay | multi-device, production keys, fencing, disaster recovery, full fingerprint certification |
| ADR-0002 evidence | accepted partial | locally executed immutable cloud generation model is viable | arbitrary runtime/device portability |
| Step 0 Quality Gate | pending PR CI | exact Rust workspace on Linux/Windows, primitives on WASM, status and tracked-secret checks | Cloudflare SDK compatibility, Windows Bridge or production security |

## 2. Required Permanent CI Evidence

Every applicable PR must provide:

- formatting, lint and unit/integration tests;
- exact toolchain and locked dependency build;
- architecture/forbidden dependency checks;
- contract/schema compatibility;
- migration replay where storage changes;
- authorization and negative isolation tests where public API changes;
- no-secret/no-PII tracked artifact checks;
- deterministic or bounded replay tests for async commands;
- changed status/evidence only after the corresponding tests exist.

## 3. Required External Evidence

| Gate | Required artifact |
|---|---|
| Legacy credential rotation | provider-side revocation/rotation confirmation and incident reference without secret value |
| Cloudflare staging | deployment ID, resource inventory, binding smoke, rollback result and cost boundary |
| Windows runtime | signed/test-signed artifact digest, host/runtime manifest and lifecycle report |
| Multi-device | two independent host manifests and transfer/revoke results |
| Key management | algorithm review, test vectors, rotation and clean-environment escrow restore report |
| Stable release | trusted signature verification, SBOM and update rollback report |
| Production recovery | D1/R2/key clean-environment game day report |
| Privacy | accepted retention matrix and export/delete/reconciliation report |

## 4. Evidence Naming

New evidence should live under `docs/evidence/` and use:

```text
YYYY-MM-DD-repository-step-N-short-name.md
```

Each report records:

- source commit and artifact digests;
- environment/runtime versions;
- exact scope and test inputs;
- results and failures;
- limitations and unproven properties;
- links to CI runs or external evidence references;
- whether user data was involved.

Secret values, raw cookies, mailbox content and uncontrolled screenshots are
prohibited in evidence documents.

## 5. Promotion Rule

`docs/status.json` may move a property from `not_proven` or `blocked` only when a
merged permanent test or reviewed external evidence entry is present. Removing a
test, invalidating an environment or superseding an ADR may downgrade status.
