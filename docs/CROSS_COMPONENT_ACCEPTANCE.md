# Cross-Component Repository-Local Acceptance

**Status:** accepted repository-local composed/synthetic evidence  
**Tracking:** issue #65 / PR #66, parent epic #43  
**Baseline:** `c1e7896590661ab01cb5c9b32b72b4a7cfa4a38b` (accepted React UI composition)

## Claim

This slice proves that the accepted repository-local components can be exercised in one deterministic CI lane without introducing a fake production environment.

The permanent `Cross-Component Acceptance Gate` validates and executes, in order:

1. a metadata-only deterministic manifest with disposable opaque IDs;
2. governed tenant/client/profile/ACL and assignment-separation D1 invariants;
3. immutable generation registry, verification and activation integrity guards;
4. Cloudflare adapter tests, Worker native tests and Worker WASM compilation;
5. all Profile Bridge tests plus the actual `profile-bridge-synthetic` executable, which must finish exactly with `synthetic-operator-complete state=DIRTY_LOCAL` and no cleanup failure;
6. Node `24.19.0` / npm `11.17.0` clean frontend install, strict TypeScript, Vitest and Vite production build;
7. metadata-only acceptance evidence validation.

The machine-readable contract is `tests/cross-component/standalone-acceptance.json`; `scripts/test-cross-component-acceptance.py` binds it to the accepted composition surfaces and fails closed when those surfaces drift.

## Candidate evidence

Cross-Component Acceptance Gate run `31208530252` completed successfully after the canonical strict synthetic claim fixture was used. The successful run demonstrated:

- manifest/composition validator: passed;
- D1 identity/client/profile/mailbox and governed negative invariants: passed;
- generation registry/integrity invariants: passed;
- Cloudflare adapter suite: 19 passed, 0 failed;
- Worker helper suite: 14 passed, 0 failed;
- Worker WASM check: passed;
- Profile Bridge library/bin/integration suites: 35 passed, 0 failed in aggregate;
- executable synthetic operator flow: exact `DIRTY_LOCAL` terminal output;
- frontend clean install/typecheck/tests/build: passed;
- metadata-only evidence scan: passed.

Final acceptance used exact head `31f358c92c2e09c752155af208b7d2aaf73d472a`. All 12 permanent workflows passed on that unchanged head, including Cross-Component Acceptance Gate `31208960718`; squash merge `eb02f3e81022193fb459b7c46d14afcb19c8900f` established the accepted repository-local composition on `main`. A prior diagnostic run exposed two harness defects — an over-broad evidence-key scanner and an invalid shortened claim fixture. Both were repaired without weakening application parsers or security boundaries.

## Negative evidence carried through the composed lane

- profile assignment does not grant profile access;
- foreign/unauthorized disclosure remains neutral;
- unverified/invalid generation transitions fail closed through existing integrity guards;
- mismatched coordinator lease/device identity is rejected before local/runtime use;
- mailbox request DTOs reject raw credential/message payload shapes and retain secret-handle-only composition;
- unknown API/auth/bridge dynamic routes remain Worker-classified instead of SPA fallback;
- application frontend source rejects browser credential/token persistence surfaces.

## Explicit exclusions

This acceptance does **not** prove or simulate:

- Cloudflare production deployment or remote resource behavior;
- real Camoufox execution or fingerprint certification;
- real Gmail API, IMAP or browser mailbox provider execution;
- production DPAPI/CNG/TPM key protection or recovery;
- trusted code signing;
- physical multi-device runtime;
- external security/certification review;
- remediation of the separately tracked legacy proxy credential/provider.

`docs/status.json` remains authoritative for production readiness and must remain `production_code: false` / not production-ready after this repository-local acceptance.

## Clean verification

```bash
python scripts/test-cross-component-acceptance.py
python scripts/test-d1-schema.py
python scripts/test-step4-identity-acl.py
python scripts/test-step4-command-guards.py
python scripts/test-step5-coordinator-projection.py
python scripts/test-mailbox-vertical-slice.py
python scripts/test-profile-generation-registry.py
python scripts/test-profile-generation-registry-edge-cases.py
python scripts/test-profile-generation-integrity-guards.py
cargo test --locked -p cloudflare-adapters --lib
cargo test --locked -p browser-profile-control-plane-worker --lib
cargo check --locked -p browser-profile-control-plane-worker --target wasm32-unknown-unknown
cargo test --locked -p profile-bridge --all-targets
cd frontend
npm ci
npm run typecheck
npm test
npm run build
```

Repository acceptance still requires every permanent workflow green on the same exact final PR head and squash merge with `expected_head_sha`.
