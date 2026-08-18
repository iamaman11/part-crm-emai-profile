# opsctl — project operational semantics

`opsctl` is the project-specific Rust operator CLI introduced by Architecture Re-baseline v3 AR-6 and structurally hardened by AR-9. It is **not** a runtime dependency of the application, not a second backend, not a generic IaC engine, and not a hidden state database.

Its responsibility is deliberately narrow:

```text
repository / release / provider evidence
            ↓
    typed Rust policy
            ↓
 decision / plan / verification
            ↓
 bounded external executor where an owning AR explicitly authorizes it
```

Git/GitHub remain source/orchestration/approval authority. Provider state remains runtime truth. Wrangler/provider APIs remain low-level Cloudflare executors. `opsctl` answers whether observed state matches accepted authority and whether a transition is mechanically allowed.

## Target command-family architecture

The accepted target shape is:

```text
opsctl
├── doctor
├── status
├── inventory
│
├── d1
│   ├── status
│   ├── plan
│   ├── compatibility
│   └── verify
│
├── release
│   ├── inspect
│   ├── verify
│   └── compatibility
│
├── promotion
│   ├── plan
│   ├── preflight
│   └── verify
│
├── credentials
│   ├── status
│   ├── readiness
│   └── rotation-plan
│
├── recovery
│   ├── inspect
│   ├── plan
│   └── verify
│
└── readiness
```

This tree is an **evolution contract**, not a claim that every family is executable in AR-9. Source presence and CLI/production activation are separate states. Future namespaces may exist in Rust before activation, but they must remain unreachable from CLI parsing until their owning AR accepts the semantics.

| Family | AR-9 executable state | Owning evolution slice |
|---|---|---|
| `doctor` | active legacy read-only command | AR-10 closes the remaining Python bridge |
| `status` | active read-only command | retained |
| `inventory` | active read-only command | retained |
| `d1 status/plan/compatibility/verify` | active native Rust policy | AR-9 |
| `credentials/*` | namespace reserved; current flat `credential-lifecycle` and `rotation-plan` preserved | AR-10 layout normalization; AR-13 rehearsal-backed rotation/readiness |
| `release/*` | source-reserved, not executable | AR-11 |
| `promotion/*` | source-reserved, not executable | AR-11 |
| `recovery/*` | source-reserved, not executable | AR-14 |
| `readiness` | source-reserved, not executable | AR-16 |

AR-9 deliberately does **not** create placeholder commands that return “not implemented”. An unowned future command is rejected by the parser. This keeps unknown state fail-closed and avoids falsely creating operational authority.

## Current source layout

The Rust library is now organized around command/policy ownership rather than one large `lib.rs`/`d1.rs` pair:

```text
tools/opsctl/src/
├── main.rs                  # thin parse -> execute -> output/exit adapter
├── lib.rs                   # composition root only
├── cli.rs                   # accepted CLI grammar
├── error.rs                 # typed machine error output
├── repository.rs            # canonical repository authority reads/root resolution
├── doctor.rs                # sole accepted AR-6 Python validator bridge
├── status.rs
├── inventory.rs
├── credentials/
│   └── mod.rs
├── d1.rs                    # D1 facade/public contract
├── d1/
│   ├── model.rs
│   ├── authority.rs
│   ├── status.rs
│   ├── compatibility.rs
│   ├── plan.rs
│   ├── verify.rs
│   ├── util.rs
│   └── tests.rs
├── release/
│   └── mod.rs               # reserved for AR-11
├── promotion/
│   └── mod.rs               # reserved for AR-11
├── recovery/
│   └── mod.rs               # reserved for AR-14
└── readiness.rs             # reserved for AR-16
```

`main.rs` must remain trivial. Command parsing is separate from policy. D1 model/authority/classification/compatibility/planning/verification are separately testable library modules. Reserved families contain only compile-time ownership/target metadata and no provider, process, credential, or mutation authority.

## Current accepted command surface

The original AR-6 repository/metadata surface remains read-only:

```text
opsctl doctor
opsctl status
opsctl inventory
opsctl credential-lifecycle
opsctl rotation-plan
```

`doctor` retains the **one** explicitly accepted AR-6 compatibility bridge that invokes the canonical Python architecture/Python-estate validators in read-only `--check` mode. AR-9 isolates this sole `Command::new` site in `doctor.rs`; no other `opsctl` module may spawn a child process.

AR-10 owns removal of even this compatibility bridge from the operator binary by bringing the required validation semantics behind native Rust boundaries. That does **not** authorize global deletion or rewriting of repository Python validators.

AR-9 adds native Rust D1 policy semantics:

```text
opsctl d1 status
opsctl d1 plan
opsctl d1 compatibility
opsctl d1 verify
```

These commands do not connect to Cloudflare and do not execute Wrangler. GitHub Actions obtains machine-readable D1 state through the pinned Wrangler/provider executor; `opsctl d1` consumes saved provider state together with canonical D1 evolution/release authorities.

```text
GitHub Actions / protected Environment
    -> pinned Wrangler: provider state read / migrations apply
    -> opsctl d1: project classification / compatibility / plan / verification
```

The AR-9 D1 subtree has no child-process site, network client, provider SDK, credential input, database mutation client, or filesystem mutation authority. Its only third-party Rust dependency is exact-pinned `serde_json` for structural JSON parsing. The standalone `tools/opsctl/Cargo.lock` remains the dependency lock authority for this operator binary.

Typical read-only flow:

```text
opsctl --root . d1 status \
  --component resolver \
  --ledger-json artifacts/d1/resolver-ledger.json

opsctl --root . d1 plan \
  --component resolver \
  --ledger-json artifacts/d1/resolver-ledger.json \
  --release-manifest artifacts/release/release-manifest.json
```

`status` classifies saved ledger state against canonical history. `plan`, `compatibility`, and `verify` also consume the component schema contract from a release manifest. Outputs are versioned JSON and always report `mutation_executed=false`.

## Permanent architectural invariants

The intended final state is mechanically enforceable, not stylistic guidance:

```text
opsctl_is_project_specific = true
opsctl_is_runtime_dependency = false
git_is_release_source_of_truth = true
provider_state_is_runtime_truth = true
hidden_opsctl_state_backend = false
opsctl_business_domain_authority = false
opsctl_operational_policy_authority = true
duplicate_mutation_authorities = 0
new_opsctl_python_spawn = 0
new_opsctl_node_spawn = 0
typed_machine_output = true
fail_closed_unknown_state = true
linux_supported = true
windows_supported = true
schema_compatibility_machine_enforced = true
release_compatibility_machine_enforced = true
recovery_preconditions_machine_enforced = true
```

During AR-9, `new_opsctl_python_spawn=0` means **no new Python spawn sites** beyond the single accepted AR-6 `doctor` bridge. AR-10 is the explicit closure owner for that remaining bridge.

`opsctl` must never acquire its own persistent state backend, generic IaC graph, plugin platform, deployment scheduler, or hidden `state.json`. It must not be called by a Worker or Profile Bridge runtime. It operates on the system from the outside.

Mutation-shaped commands such as `opsctl d1 apply`, `opsctl d1 migrate`, `opsctl deploy`, or an unowned generic `opsctl promote` remain absent. GitHub Actions/Environments own orchestration, approvals, concurrency and credential exposure; provider executors own provider mutation; `opsctl` owns only the project-specific typed policy assigned by the current architecture authority.
