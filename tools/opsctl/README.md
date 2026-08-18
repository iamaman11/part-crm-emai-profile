# opsctl — project operational semantics

`opsctl` is the project-specific Rust operational interface introduced by Architecture Re-baseline v3 AR-6 and extended by AR-9.

The original AR-6 repository/metadata surface remains read-only:

```text
opsctl doctor
opsctl status
opsctl inventory
opsctl credential-lifecycle
opsctl rotation-plan
```

`doctor` retains the one explicitly accepted AR-6 compatibility bridge that runs the canonical Python architecture/Python-estate validators in read-only `--check` mode. AR-9 does not multiply that bridge.

AR-9 adds native Rust D1 policy semantics:

```text
opsctl d1 status
opsctl d1 plan
opsctl d1 compatibility
opsctl d1 verify
```

These commands do not connect to Cloudflare and do not execute Wrangler. GitHub Actions obtains machine-readable D1 state with the pinned Wrangler/provider executor and saves it to a file; `opsctl d1` consumes that file together with the canonical D1 evolution authority and, where required, a component release manifest.

The authority boundary is deliberate:

```text
GitHub Actions / protected Environment
    -> pinned Wrangler: provider state read / migrations apply
    -> opsctl d1: project classification / compatibility / plan / verification
```

The AR-9 D1 code has no child-process site, network client, provider SDK, credential input, database mutation client, or filesystem mutation authority. Its only third-party Rust dependency is exact-pinned `serde_json` for typed structural JSON parsing. The standalone `tools/opsctl/Cargo.lock` remains the dependency lock authority for this operator binary.

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

`status` classifies the saved ledger against repository history. `plan`, `compatibility`, and `verify` also consume the component schema contract from the release manifest. Outputs are versioned JSON and always report `mutation_executed=false`.

Mutation commands such as `opsctl d1 apply`, `opsctl d1 migrate`, `opsctl deploy`, or `opsctl promote` are intentionally absent. GitHub Actions/Environments remain orchestration, approval, concurrency, and credential boundaries; Wrangler/provider APIs remain the actual mutation mechanisms when the owning lifecycle authorizes mutation.
