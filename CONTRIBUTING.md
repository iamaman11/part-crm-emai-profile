# Contributing

## Delivery Model

Changes are delivered as bounded increments through a branch and pull request.
Repository Steps 0–10 are historical accepted milestones; new work must not invent
a numbered step merely to continue development.

Each PR must:

1. state its baseline commit and bounded objective;
2. preserve architecture and security invariants;
3. include tests for new behavior and negative paths;
4. update contracts, migrations, capability matrix and evidence when applicable;
5. avoid secrets, real browser profiles and uncontrolled PII;
6. pass every applicable permanent workflow on the exact final head;
7. leave no unresolved review thread before merge.

Read [`docs/DEVELOPER_CAPABILITY_MATRIX.md`](docs/DEVELOPER_CAPABILITY_MATRIX.md)
before changing composition. It distinguishes currently composed executable paths
from reusable libraries, synthetic evidence, target architecture and external gates.

## Local Checks

The following commands match the core Linux/WASM quality lane more closely than a
single unrestricted workspace command:

```text
python -m py_compile scripts/*.py
python scripts/check-architecture.py
python scripts/check-contract-compatibility.py
python scripts/check-d1-boundary.py
python scripts/check-step4-governed-writes.py
python scripts/check-step5-profile-coordinator.py
python scripts/check-step6-windows-bridge.py

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets \
  --exclude browser-profile-control-plane-worker \
  --exclude cloudflare-adapters -- -D warnings
cargo test --locked --workspace --all-targets \
  --exclude browser-profile-control-plane-worker \
  --exclude cloudflare-adapters
cargo test --locked -p cloudflare-adapters --lib
cargo test --locked -p browser-profile-control-plane-worker --lib

cargo check --locked --target wasm32-unknown-unknown \
  -p profile-platform-primitives \
  -p contracts \
  -p control-plane-contract \
  -p identity-access-domain \
  -p client-domain \
  -p profile-domain \
  -p session-domain \
  -p mailbox-domain \
  -p bridge-domain \
  -p application-ports \
  -p use-cases

bash scripts/check-tracked-secrets.sh
python -m json.tool docs/status.json >/dev/null
```

For D1 migration replay and Worker release packaging, use the pinned commands from
`.github/workflows/quality-gate.yml`. Windows, runtime-bundle, local-profile,
encrypted-generation, certification and external-evidence behavior is accepted by
its dedicated permanent workflow rather than by an unsupported local approximation.

## Governed Mutation Evidence

Every Worker mutation that writes tenant state through a governed D1 envelope must
preserve these properties:

- decide idempotency using tenant, actor, key, exact command name, exact request
  digest and the live expiry boundary; key-only replay is forbidden;
- derive audit and outbox identifiers with the shared domain-separated request
  evidence helpers; never truncate or prefix caller-controlled idempotency keys to
  form tenant-wide evidence identities;
- keep command journal, aggregate mutation, idempotency, audit and outbox writes in
  one transaction-fatal D1 batch so a late failure cannot leave partial evidence;
- precompute fallible aggregate version increments before mutation and never use
  saturating arithmetic to hide overflow;
- map mutation failures through the shared Worker taxonomy: disclosure-neutral
  missing/unauthorized resources, version conflict, invalid state, uniqueness or
  idempotency conflict, integrity failure, internal preflight failure and unknown
  dependency failure are distinct classes;
- never expose raw SQLite, D1 or provider diagnostics in public problem responses.

When changing a governed mutation, update the permanent regression checks rather
than adding a temporary workflow. Tests must distinguish the semantic/business path
from direct SQL bypass or integrity-defense paths; do not accept whichever trigger
message happens to fire first.

## Architecture Rules

- domain crates depend only on allowed primitives/contracts;
- Cloudflare, Windows, Python and browser SDK types stay in adapters/apps;
- every tenant-owned repository call requires typed tenant scope;
- every fallible version/counter update is computed before aggregate mutation;
- a profile becomes `READY` only through verified generation activation;
- unknown dynamic `/api/*`, `/auth/*` and `/bridge/*` paths fail closed;
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

Preferred merge method is squash after green CI. An increment is complete only
after merge; branch or draft evidence is not the accepted baseline. Update
`docs/status.json` in the same PR only for properties directly proven by that PR.
