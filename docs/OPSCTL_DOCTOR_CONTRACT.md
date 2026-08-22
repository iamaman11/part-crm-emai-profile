# opsctl doctor — Permanent Diagnostic Contract

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Mandatory architecture requirements:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**opsctl boundary:** `docs/OPSCTL_ARCHITECTURE_BOUNDARY.md`  
**Python boundary:** `docs/PYTHON_USAGE_BOUNDARY.md`  
**Pre-PF-1 normalization:** `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md`  
**PF-1:** `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md`  
**Production authorization:** NONE

`opsctl doctor` is the repository/operator diagnostic composition surface. It is not a semantic-authority registry, lifecycle engine, provider observer, deployment executor, Product Runtime health service, background daemon, or substitute for exact-head GitHub CI.

## 1. Permanent role

`doctor` answers a bounded question:

> Is the local repository/tooling state structurally usable enough to run supported `opsctl` read/plan/verify workflows, and which owned diagnostic checks fail?

It may aggregate typed diagnostic results from bounded modules. It must not duplicate those modules' business/release/D1/lifecycle/evidence semantics.

Target flow:

```text
local repository/filesystem observations
        ↓
strict adapters / typed repository facts
        ↓
bounded diagnostic checks
        ↓
DoctorReport
        ↓
machine/human output adapter
```

## 2. Hard effect boundary

Allowed effects:

```text
FilesystemRead
repository-root resolution
local regular-file/symlink metadata observation
local contract decoding through owned adapters
stdout/stderr rendering
```

Forbidden effects:

```text
ProcessExecution
Python/Node subprocess
Git/GitHub subprocess or API
NetworkAccess
ProviderRead
ProviderWrite
SecretResolve
DatabaseWrite
DeploymentMutation
RuntimeExecution
GeneratedProjectionWrite
```

`doctor` does not launch Camouhost/Camoufox, Wrangler, `git`, `gh`, Python validators, Node validators, provider SDKs, or hosted observation clients.

## 3. No legacy sentinel registry

Current `doctor.rs` and `repository.rs` depend on transitional sentinels including:

```text
architecture/python-estate-ar6.json
architecture/operator-contract.json
scripts/generate-architecture-inventory.py
scripts/python-estate-ar6.py
```

Those dependencies are temporary and must disappear as N2/N4/PF-1 retire the corresponding authorities.

Repository identity must not be defined by files scheduled for retirement.

Target repository-root proof uses durable repository markers owned by the surviving architecture, for example a small combination of:

```text
root Cargo.toml / workspace identity
architecture-program-sequence static contract
canonical migration roots
surviving application/workspace structure
```

The exact marker set is chosen during implementation and must remain minimal, durable, cross-platform and independent of generated projections or AR-qualified historical artifacts.

`architecture/inventory.json` must not become a required semantic root sentinel if doing so would make repository identity depend on a generated projection.

## 4. Typed diagnostic model

`doctor` must not operate as:

```rust
fn doctor(values: Vec<serde_json::Value>) -> serde_json::Value
```

Instead bounded adapters produce typed diagnostic observations and typed check results.

Conceptually:

```rust
struct DoctorReport {
    schema_version: DoctorReportVersion,
    status: DoctorStatus,
    checks: Vec<DoctorCheckResult>,
}

enum DoctorCheckResult {
    Pass { check: DoctorCheckId },
    Fail { check: DoctorCheckId, reason: DoctorReasonCode },
    NotApplicable { check: DoctorCheckId },
}
```

Exact types are implementation-owned. The invariant is that `serde_json::Value`, `Path`/`PathBuf`, raw provider responses and filesystem handles do not become semantic report identities.

## 5. Diagnostic ownership

`doctor` may call bounded local Rust diagnostic APIs, but it does not reimplement their rules.

Examples:

```text
D1 repository structural diagnostics
    -> D1 repository/catalog owner

Release contract local decode diagnostics
    -> release contract owner

Runtime manifest local decode diagnostics
    -> runtime manifest adapter/owner

Architecture/lifecycle local contract diagnostics after PF-1
    -> PF-1 typed architecture owner

Operator command registry diagnostics after N4
    -> Rust CommandRegistry owner
```

A domain-specific failure is reported by `doctor`; `doctor` does not become the canonical owner of that invariant.

## 6. What doctor must stop doing

The current implementation generically checks every listed JSON object for only a numeric `schema_version`. This is transitional and too weak for the target architecture.

After the owning cutovers:

- `doctor` must not maintain a manually duplicated `AUTHORITIES` list that becomes a second authority catalog;
- it must not treat every JSON document as semantically equivalent;
- it must not retain `INTERNAL_NATIVE_IMPLEMENTATION_CONTRACT` as a JSON string parsed at runtime merely to prove its own source behavior;
- it must not require retired Python/Node validators to exist;
- it must not validate generated projection existence as a substitute for validating its natural owner;
- it must not report `ok` solely because files parse as JSON.

Compile-time/source tests and PF-3 fitness rules prove no-child-process/no-network behavior; runtime self-description must not duplicate that truth.

## 7. Output contract

Machine output remains versioned and stable.

Required properties:

```text
explicit schema_version
explicit command = doctor
closed DoctorStatus vocabulary
stable typed check IDs/reason codes
no secrets
no raw provider payloads
no environment dumps
no absolute-path leakage unless explicitly requested for human diagnostics
stdout reserved for machine output in machine mode
stderr for human diagnostics
```

A diagnostic failure is distinguishable from an input/decode/I/O failure.

`doctor` should support a useful non-zero exit on failed required checks while keeping the JSON report parseable. Exact exit-code mapping is part of the stable CLI output contract.

## 8. Relationship to exact-head CI

`doctor` is a fast local diagnostic helper. It never proves:

```text
protected-main branch protection state
hosted required checks
GitHub review/thread state
provider deployment state
Hosted Evidence attestation existence
real staging/production readiness
```

Those require explicit hosted/provider observations and their owning evaluators/workflows.

A green local `doctor` cannot substitute for protected exact-head CI.

## 9. Relationship to `opsctl status`

`doctor` and `status` remain distinct:

```text
doctor
    local structural/tooling diagnostics

status
    bounded current program/operator projection from explicit owned inputs
```

Neither command becomes a hidden global authority bag.

If current implementation has overlapping output, the owning cutover must converge it rather than preserve duplicate semantics for compatibility without a proved consumer.

## 10. N2 / N4 / PF-1 required work

### N2

- remove `architecture/python-estate-ar6.json` and `scripts/python-estate-ar6.py` from `doctor` requirements;
- remove those files from repository-root sentinels;
- ensure no `doctor -> Python` subprocess is reintroduced;
- replace per-file Python estate assumptions with source-derived role/effect diagnostics where `doctor` needs only a bounded result.

### N4

- stop requiring `architecture/operator-contract.json` as CLI semantic authority;
- consume/check the Rust-owned command/effect registry or its bounded diagnostic projection;
- remove operator-contract sentinels from repository-root detection when the predecessor loses current authority.

### PF-1

- remove `scripts/generate-architecture-inventory.py` and other retired inventory/lifecycle predecessors from `doctor` and repository-root detection;
- do not make generated `architecture/inventory.json` a semantic authority;
- if inventory freshness is diagnosed, call the owned typed inventory check path rather than merely parsing the JSON file;
- ensure `doctor`, `repository.rs`, quality workflows and audit workflows all converge on the same surviving owners.

## 11. PF-3 fitness requirements

PF-3 must mechanically enforce at least:

```text
opsctl_doctor_process_execution = 0
opsctl_doctor_network_access = 0
opsctl_doctor_provider_access = 0
opsctl_doctor_python_or_node_child_process = 0
opsctl_doctor_legacy_authority_sentinel = 0
doctor_generic_json_authority_bag = 0
doctor_duplicate_semantic_policy = 0
repository_root_depends_on_generated_projection = 0  # unless an explicit reviewed exception proves necessity
```

## 12. Positive proofs

At minimum prove:

- a minimal valid repository passes;
- each surviving bounded local diagnostic can report pass/fail deterministically;
- malformed versioned contracts fail closed through their own adapters;
- symlink/non-regular required local files fail closed where applicable;
- output is deterministic for the same typed observations;
- Linux and Windows behavior agrees for equivalent repository facts;
- no retired AR-6/N4/PF-1 sentinel is required after its cutover.

## 13. Negative proofs

At minimum reject/prove absence of:

- child-process execution;
- network/provider access;
- hidden secret/environment readback;
- generic `serde_json::Value` semantic aggregation;
- a reintroduced retired Python/Node validator requirement;
- generated inventory used as semantic source;
- domain policy duplicated inside doctor;
- repository-root failure solely because a retired/generated predecessor is absent.

## 14. Definition of Done

`opsctl doctor` reaches the target architecture only when:

1. it is read-only local diagnostic composition;
2. no child process/network/provider/runtime/secret effect exists;
3. repository-root detection depends only on durable surviving markers;
4. AR-6 Python-estate artifacts are not required;
5. `operator-contract.json` is not required as CLI semantic authority after N4;
6. Python/Node PF-1 predecessors are not required after PF-1;
7. generated projections are never semantic inputs;
8. bounded diagnostic owners are called rather than duplicated;
9. machine report/exit semantics are versioned and tested;
10. PF-3 fitness prevents regression;
11. exact-head CI remains the final hosted acceptance authority.
