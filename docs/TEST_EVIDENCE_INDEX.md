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
| Repository Step 0 / PR #4 | accepted | exact Rust `1.97.1` locked workspace; Linux fmt/Clippy/tests; Windows tests; primitives WASM compile; status validation; current-tree high-confidence secret scan | Cloudflare SDK compatibility, Windows Bridge, historical secret remediation or production security |
| Repository Step 1 / PR #6 | accepted | exact `worker 0.8.5` D1/R2/Queue/Durable Object/Static Assets compile and `worker-build 0.8.5` release artifact | real Cloudflare deployment, binding behavior, migrations, DO consistency, remote rollback or production readiness |
| Repository Step 2 / PR #9 | passed in PR, pending merge | typed pure domain/application boundaries, native+WASM state-machine tests, active architecture and v1 contract breaking-change gates | D1 persistence, Access, distributed fencing, Bridge/runtime or production completeness |

### Repository Step 0 Evidence

- baseline: `3fef715c5b74f723d8a30c16471bf62a3609a34b`;
- accepted source head: `c8927bc79ab7f68123fc122409792326043e29b3`;
- permanent Quality Gate run: `31035179366`, conclusion `success`;
- squash merge: `dc2dc2e1a7acd07d89550328309833988bb05a2e`;
- jobs: `Rust Linux and WASM`, `Rust Windows`;
- user/legacy profile data involved: no.

The tracked-file scan covers the accepted tree only. It does not prove that the
known legacy credential was rotated or absent from repository history; that
external remediation remains issue #1.

### Repository Step 1 Evidence

- baseline: `cc345301baaa1e549caf4045ce16739402edca02`;
- technical implementation head: `196804579bd6535b75dd964bc50fd184703b52cb`;
- accepted source head: `990fe8262f933b1a20a0c786a6a5ebc26f4fe7e2`;
- technical Quality Gate run: `31036328555`, conclusion `success`;
- final Quality Gate run: `31036967681`, conclusion `success`;
- squash merge: `cba724a0d7fd116859a30d9e0101e56349c1358c`;
- jobs: `Rust Linux and WASM`, `Rust Windows`, `Cloudflare Worker Release Build`;
- exact runtime/build pins: Rust `1.97.1`, `worker 0.8.5`,
  `wasm-bindgen 0.2.126`, `worker-build 0.8.5`;
- output checks: `build/worker/shim.mjs` and generated Wasm present;
- detailed report:
  [`evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md`](evidence/2026-08-05-repository-step-1-cloudflare-cold-build.md);
- Cloudflare credentials or real resources involved: no;
- user/legacy profile data involved: no.

The upstream `worker-build 0.8.5` installation emitted yanked-package warnings
for two build-tool transitive versions. This is documented as an upgrade and
supply-chain review item; it is not a Worker runtime dependency claim.

### Repository Step 2 Evidence

- baseline: `29956f6a71ea5f76618e97c651276f2a43698870`;
- technical evidence head: `a3d0852e11708297bb7d5e04ed23ff981e774d7c`;
- technical Quality Gate run: `31039199212`, conclusion `success`;
- jobs: `Rust Linux and WASM`, `Rust Windows`, `Cloudflare Worker Release Build`;
- positive architecture and contract checks: passed;
- deliberately forbidden domain dependency fixture: rejected as required;
- deliberately breaking protobuf fixture: rejected as required;
- all governed pure crates: native tests and `wasm32-unknown-unknown` check passed;
- detailed report:
  [`evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md`](evidence/2026-08-05-repository-step-2-domain-contract-skeleton.md);
- Cloudflare credentials, storage resources or user data involved: no.

Final accepted source head, final Quality Gate and squash merge are recorded only
after the PR is fully green and merged. The v1 baseline immutability check is part
of the final candidate gate.

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
