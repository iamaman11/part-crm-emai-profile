# Contributing

## Authority and current work

Changes are delivered as bounded increments through a branch and pull request.

Current execution order is governed by:

1. `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` — canonical program authority;
2. protected `main` + live tracker #441 — mutable pre-PF-1 execution state;
3. `docs/DEVELOPMENT_PLAN.md` — compact developer-facing projection only.

A projection never overrides the canonical plan or accepted protected `main`. Do not invent a numbered phase merely to continue development or to isolate a bounded defect.

## GitHub access in the current agent environment

The interactive development environment may have the connected **GitHub plugin** without a local `gh` CLI. In that environment:

- use GitHub-plugin actions for repository/branch reads, PRs, issues, reviews, comments, required contexts, workflow/status observations and repository-file/branch/PR writes;
- do not block work because `gh` is absent;
- do not shell-scrape GitHub as a replacement for the connected API surface;
- if an older runbook shows a `gh` command, use the equivalent GitHub-plugin operation and update that runbook when it is otherwise touched.

This is a developer-tooling rule only. It does not move GitHub/network/provider authority into Product Runtime or `opsctl`; `opsctl` remains offline according to `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`.

## Pull-request workflow

Each PR must:

1. state its fresh accepted baseline and bounded objective;
2. identify affected natural owners/contracts/effects and any predecessor being retired;
3. preserve architecture, security and compatibility invariants;
4. include positive and negative/fail-closed tests for changed behavior;
5. avoid secrets, real browser profiles and uncontrolled PII;
6. run fast/targeted local verification before expensive permanent CI;
7. pass every applicable permanent workflow on one unchanged exact final head;
8. be synchronized to the accepted target branch (`behind_by=0`) before final acceptance;
9. have zero blocking reviews and zero unresolved review threads before merge;
10. follow the current binding merge/acceptance protocol and reread accepted `main` after merge.

Do not start the next sequential architecture transaction from an unaccepted sibling branch. Parallel work is allowed only when dependency-independent and non-competing.

## Fast local verification

Use the fast verifier before pushing changes that affect Rust/application/adapter/Worker behavior or repository policy:

```text
python scripts/verify-fast.py
```

Before a live transport/application cutover or exact-head acceptance, use the bounded compile lane:

```text
python scripts/verify-fast.py --with-compile
```

The fast verifier is the interactive feedback loop. Permanent CI is final acceptance evidence, not a formatter/compiler loop.

Useful targeted commands include:

```text
python -m py_compile scripts/*.py
python scripts/check-architecture.py
python scripts/check-contract-compatibility.py
python scripts/check-d1-boundary.py

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets \
  --exclude browser-profile-control-plane-worker \
  --exclude cloudflare-adapters -- -D warnings
cargo test --locked --workspace --all-targets \
  --exclude browser-profile-control-plane-worker \
  --exclude cloudflare-adapters
cargo test --locked -p cloudflare-adapters --lib
cargo test --locked -p browser-profile-control-plane-worker --lib

bash scripts/check-tracked-secrets.sh
```

For D1 migration replay and Worker release packaging use the pinned commands from the owning permanent workflows. Windows, runtime-bundle, local-profile, encrypted-generation, certification, Profile Bridge and real-Camoufox acceptance stay with their dedicated permanent workflows rather than unsupported local approximations.

## Critical-path development rules

Optimize for the shortest safe path to accepted capability:

- one semantic fact has one natural owner;
- prefer small reviewable PRs, but do not turn every PR into a new program phase;
- no current consumer + no durable/persisted obligation means no speculative compatibility bridge;
- cut over callers, prove `old_current_callers=0` and `old_unique_current_invariants=0`, then delete/demote the DEAD predecessor;
- do not replace retired JSON/Python/Node authorities with equivalent successor registries in another format;
- split crates/layers only when ownership or dependency pressure materially benefits from compile-time separation;
- keep security/authorization/idempotency migrations sequential when they touch the same governed mutation surface;
- avoid unrelated cleanup in capability PRs;
- deliver UI with the bounded backend/query capability it projects; UI is never the security boundary;
- keep `source_present != production_enabled`; Release / Capability Profile admission is the only production-enable authority.

## Architecture boundaries

Mandatory prospective rules live in `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md` and related bounded contracts. In particular:

- domain/application core does not depend on Cloudflare, Windows, Python/browser SDKs or `opsctl`;
- external bytes/config enter through strict typed adapters/DTOs;
- Pure Core does not perform filesystem/process/network/provider effects;
- `serde_json::Value` does not cross adapter -> pure-core semantics;
- `opsctl` has no Product Runtime, provider, network or process authority;
- `opsctl doctor` is local read-only diagnostic composition only;
- Python is governed by role/effects, not a permanent per-file registry;
- generated projections are outputs, not semantic inputs;
- context-owned persistence is mutated only through the owning context/application path;
- no generic remote `exec`, secret readback, mutable active R2 object, live-browser-directory snapshot or blind Firefox-lock deletion.

## Governed mutation essentials

When changing a governed tenant mutation, preserve:

- typed tenant scope and authorization before mutation;
- full idempotency identity including tenant/actor/command/request digest/live expiry semantics;
- deterministic audit/outbox/journal identity from governed evidence, never truncated caller-controlled identifiers;
- transaction-fatal grouping of command journal, aggregate mutation, idempotency, audit and outbox writes;
- precomputed fallible version/counter updates rather than saturating arithmetic;
- disclosure-neutral public error taxonomy with no raw SQLite/D1/provider diagnostics;
- permanent regression tests that distinguish business paths from direct integrity-defense paths.

## Test data

Use synthetic identifiers, generated secrets and disposable profiles. Legacy profiles under `temp/browser_profiles/` are source evidence: do not launch, repair, migrate, clean or open their SQLite files in place.

## Exact-head acceptance

Permanent pull-request jobs must test the literal candidate head, not GitHub's synthetic pull-request merge ref. `behind_by=0` remains mandatory before final acceptance.

Do **not** hard-code an obsolete merge mechanism in contributor guidance. Architecture acceptance follows `docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md` and current live governance; the current protocol uses a guarded squash merge bound to the expected candidate head and requires candidate-tree == accepted-merge-tree plus accepted-main reread. If the binding protocol changes later, this guide follows it rather than creating a competing rule.

An open branch, draft PR or green historical run is never an accepted baseline.
