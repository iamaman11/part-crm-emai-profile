# Contributing

## Delivery Model

Changes are delivered as bounded Repository Steps through a branch and pull
request. Direct feature development on `main` is not the acceptance model.

Each PR must:

1. state its baseline commit and bounded objective;
2. preserve architecture and security invariants;
3. include tests for new behavior and negative paths;
4. update contracts/migrations/evidence when applicable;
5. avoid secrets, real browser profiles and uncontrolled PII;
6. pass every applicable permanent workflow;
7. leave no unresolved review thread before merge.

## Local Checks

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo check --locked -p profile-platform-primitives --target wasm32-unknown-unknown
bash scripts/check-tracked-secrets.sh
python -m json.tool docs/status.json
```

## Architecture Rules

- domain crates depend only on allowed primitives/contracts;
- Cloudflare, Windows, Python and browser SDK types stay in adapters/apps;
- every tenant-owned repository call requires typed tenant scope;
- email/client names are not identifiers or paths;
- no generic remote `exec`;
- no mutable active R2 object;
- no snapshot of a live browser directory;
- no blind deletion of Firefox lock files;
- no readiness claim without evidence.

## Test Data

Use synthetic identifiers, generated secrets and disposable profiles. Legacy
profiles under `temp/browser_profiles/` are source evidence: do not launch,
repair, migrate, clean or open their SQLite files in place.

## Pull Request Acceptance

Preferred merge method is squash after green CI. A step is complete only after
merge; branch or draft evidence is not the accepted baseline. Update
`docs/status.json` in the same PR only for properties directly proven by that PR.
