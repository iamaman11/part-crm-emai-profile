# opsctl — project operational semantics

`opsctl` is the repository-specific Rust CLI for local, typed policy evaluation over explicit files and
observations. It is not Product Runtime, a provider/GitHub client, a deployment executor, a secret
resolver or a hidden state database.

The binding boundary is [`docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`](../../docs/OPSCTL_ARCHITECTURE_BOUNDARY.md).
The executable grammar is [`src/help.txt`](src/help.txt); run `opsctl --help` for exact flags.

## Effect model

```text
outer workflow / official provider tool / developer shell
    -> explicit secret-free observation or local artifact
    -> strict adapter + typed opsctl policy
    -> decision / plan / verification JSON on stdout
```

Every current command is `ReadOnlyMetadata`. `opsctl` does not execute Python, Node, Wrangler or a
provider API; does not read provider credentials; does not mutate a database/deployment/customer state;
and does not authorize Production. GitHub/Environments retain orchestration and approval authority,
while official provider executors retain actual mutation authority.

## Current executable surface

```text
doctor
status --acceptance-evidence-json PATH

credentials status
credentials rotation-plan

hosted-evidence operational-credential seal
hosted-evidence operational-credential verify
hosted-evidence external-review-attestation verify

d1 repository
d1 status
d1 plan
d1 compatibility
d1 verify

release finalize
release inspect
release verify
release compatibility

promotion plan
promotion preflight
promotion verify
```

Important distinctions:

- `doctor` checks only canonical local repository structure (`Cargo.toml`, catalog migrations and
  resolver migrations); it has no subprocess bridge or semantic authority catalog.
- `status` derives lifecycle state from an explicit acceptance observation. It does not observe
  GitHub itself.
- `hosted-evidence` evaluates explicit secret-free observation/artifact bytes plus caller-supplied
  clock/source identity where required.
- `d1` consumes saved Wrangler/provider observations and repository contracts; it never applies a
  migration.
- `release finalize` writes canonical machine output to stdout from explicit packaging observations;
  `inspect`, `verify` and `compatibility` remain local policy/artifact verification.
- `promotion plan`, `preflight` and `verify` evaluate saved snapshots/evidence. They do not deploy,
  promote or roll back anything.

There are no placeholder command families. An unrecognized command fails closed; future functionality
is added only with an owning consumer, invariant and accepted bounded transaction.

## Usage

From the repository root:

```text
cargo run --locked --manifest-path tools/opsctl/Cargo.toml -- --help
cargo run --locked --manifest-path tools/opsctl/Cargo.toml -- --root . doctor
cargo run --locked --manifest-path tools/opsctl/Cargo.toml -- --root . d1 repository
```

For action-specific required flags, use `--help` and the strict parser/tests. Machine output is
versioned JSON; errors are machine-readable and exit non-zero.

## Source layout

```text
src/main.rs             thin parse -> execute -> output/exit adapter
src/cli.rs              accepted CLI grammar
src/help.txt            executable help contract
src/lib.rs              read-only composition root/effect metadata
src/doctor.rs           local structure checks only
src/hosted_evidence.rs  typed hosted-evidence policy
src/d1/                 D1 adapters/models/policy
src/release/            release finalization/inspection/verification/compatibility
src/promotion/          plan/preflight/post-state verification
core/                   pure `opsctl-core` semantic boundary where justified
```

Physical modules may evolve, but `adapters -> typed core` and `Product Runtime -X-> opsctl/opsctl-core`
remain mandatory. A new provider, mutation engine, plugin framework, persistent opsctl state or
checker-for-checker is outside this tool's role.
