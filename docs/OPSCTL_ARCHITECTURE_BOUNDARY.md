# opsctl Architecture Boundary

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT
**Program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)
**Production authorization:** NONE

`tools/opsctl` is a standalone, project-specific Rust policy/planning/verification CLI over explicit
local files and observations. It is not Product Runtime, a daemon, provider/GitHub client, deployment
executor, secret resolver, browser launcher, hidden state store or shared application service.

## 1. Role and effects

Allowed semantic roles:

```text
inspect
validate
verify
plan
compatibility evaluation
lifecycle/evidence evaluation over explicit observations
canonical external-contract rendering
```

Current effect capability:

```text
FilesystemRead + stdout/stderr presentation
```

Release finalization renders its result to stdout; an outer caller owns any artifact publication.
Current commands have no network, provider, database/deployment/customer-state mutation, secret
readback, system-clock, process execution or Production authorization authority.

GitHub Actions/Environments own orchestration/approval. Official provider tools or explicitly owned
outer adapters collect observations and perform an authorized mutation. `opsctl` evaluates only the
explicit bytes it receives before/after that effect.

## 2. Internal direction

```text
CLI / composition
-> filesystem and strict JSON adapters
-> closed versioned external DTOs
-> typed semantic inputs
-> PURE CORE
-> typed results
-> canonical JSON / human output adapters
```

The internal `tools/opsctl/core` package exists where compile-time separation materially protects
shared release/policy semantics. Not every module must move into it.

Hard boundaries:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider types in pure semantic APIs = 0
hidden clock/env/cwd/randomness in pure policy = 0
Product Runtime -> opsctl or opsctl-core = 0
opsctl -> Python/Node/Wrangler/git/gh = 0
```

Filesystem paths are shell inputs, not semantic identities. Pure policy receives typed normalized
identities/observations. Core reason codes and decisions are typed; rendering belongs to adapters.

## 3. Input, JSON and digest discipline

```text
bounded UTF-8 bytes
-> duplicate-member/depth/size admission
-> closed versioned DTO
-> validation + typed conversion
-> semantic decision
```

Unknown fields fail closed unless the external contract defines an extension point. Breaking meaning
bumps schema version. `serde_json::Value` may exist only in strict decoding/canonicalization adapters.

Semantic JSON identity uses canonical semantic bytes + reviewed SHA-256. Exact artifact identity uses
exact file bytes + SHA-256. Pretty output, semantic identity and artifact identity are separate scopes.

Every JSON artifact is exactly one of: versioned external contract/manifest, observation/evidence,
generated output projection, or isolated historical input with a named current consumer. A generic
authority bag, tracked projection used as semantic input or hidden `state.json` is forbidden.

## 4. Current command ownership

The executable grammar is `tools/opsctl/src/help.txt`; `tools/opsctl/README.md` is navigation. Current
families are:

- `doctor` — minimal local repository structure only;
- `status` — lifecycle projection from an explicit acceptance observation;
- `credentials` — canonical lifecycle/rotation metadata projection;
- `hosted-evidence` — typed verification/sealing of secret-free observations/artifacts;
- `d1 repository/status/plan/compatibility/verify` — D1 policy over saved provider observations and
  repository contracts, never migration apply;
- `release finalize/inspect/verify/compatibility` — immutable Release Set construction/inspection,
  exact-byte verification and policy;
- `promotion plan/preflight/verify` — deterministic policy over saved target snapshots/evidence, never
  deploy/promotion/rollback execution.

Unknown/unowned commands fail in parsing; placeholder namespaces are forbidden. Command composition may
sequence adapter reads and one typed policy call but cannot absorb a bounded owner's semantics.

## 5. Observation and mutation boundary

```text
outer protected workflow
-> exact source/artifact/provider/environment observation
-> strict DTO
-> opsctl plan/preflight/verify
-> explicit allowed/blocked decision
-> separately authorized official executor
-> post-state observation
-> opsctl verification
```

An `allowed` decision is not a provider mutation or human risk acceptance. `opsctl` never turns
`expected_current` into an inferred value, observes live state itself or exposes credentials before the
owning preflight permits the outer workflow to continue.

## 6. Repository discovery and projections

Repository-root discovery uses minimal durable source markers, not generated projections or retired
AR/Python/Node sentinels. `doctor` follows its dedicated contract.

A report/inventory command survives only while a named current consumer needs its distinct output. A
generator, drift gate, self-test or docs caller that exists only for a predecessor belongs to the same
retirement set. Deletion requires zero current callers and zero unique current invariants; do not create
a successor registry merely to preserve shape.

## 7. Shared semantics

Default is no Product Runtime/opsctl sharing. A neutral pure crate is allowed only when at least two real
independent consumers require exactly the same invariant and one owner prevents actual duplication. It
depends on neither consumer and cannot become generic `common`/service/policy infrastructure.

Forbidden:

```text
Product Runtime -> opsctl
Product Runtime <-> RPC/gRPC <-> opsctl
opsctl provider client or deployment scheduler
opsctl plugin/IaC framework
opsctl persistent state backend
```

## 8. Output and error contract

Machine output is versioned and deterministic. Keep I/O, decode, contract validation, semantic
`BLOCKED/UNKNOWN/INCOMPATIBLE` and output encoding failures distinct. Stdout is machine output; stderr
is diagnostics. Never render secrets, raw provider payloads, uncontrolled PII or absolute paths unless
an explicit human diagnostic contract permits them.

## 9. Change acceptance

A new/changed command proves:

1. named current consumer, objective invariant and natural owner;
2. exact allowed effects and explicit forbidden effects;
3. strict DTO/typed-core boundary and version behavior;
4. deterministic positive and negative/fail-closed tests;
5. no Product Runtime dependency, process/network/provider/secret/mutation authority;
6. predecessor caller cutover/deletion or evidenced retirement condition;
7. output schema/error compatibility and cross-platform behavior where applicable;
8. checker lifecycle under `ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`.

`opsctl` may calculate policy and evidence; only the exact CAP-08 R3 decision can authorize Production.
