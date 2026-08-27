# Python Usage and Authority Boundary

**Document status:** SUBORDINATE_NORMATIVE_CONTRACT
**Program authority:** [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)
**Production authorization:** NONE

Python is allowed when its role/effects are explicit. It is not a default architecture layer and must
not become a second current owner for Product Runtime, release/capability policy, lifecycle, evidence,
fitness or provider mutation. The source tree and current callers are the estate observation; no
hand-maintained per-file Python registry is current authority.

## Permanent rule

```text
Python may adapt, observe, generate, validate, test and orchestrate developer-local proof.
Python may not create a second semantic owner or an ungoverned effect path.
```

Language choice is secondary to ownership and effects. A real cross-language adapter can be correct;
a small helper that duplicates policy or bypasses admission is not.

## Allowed roles

### Product Runtime cross-language adapter

`runtime/camouhost/real.py` is the current canonical example: a versioned/bounded Bridge IPC adapter to
Camoufox/BrowserForge/Playwright. Rust owners retain profile/business/lifecycle policy, Profile Bridge
retains writer/process composition, runtime identity is pinned, stdout is protocol-only, diagnostics
are stderr, inputs are bounded and no independent cloud/database/provider authority is acquired.

`runtime/camouhost/main.py` is synthetic/test-only and must never be selected as a Production runtime by
an ungoverned flag.

### Repository validator or observer

Python may observe source/layout/contracts and implement bounded structural checks. It may not maintain
a second capability/release/lifecycle/business registry. A new permanent checker follows the full
consumer/risk/invariant/tier/negative-proof/retirement standard in
`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`.

### Deterministic generator or renderer

Python may render OpenAPI/frontend contracts, provider-native configuration, release artifacts or test
fixtures when natural semantic owners are inputs and output is a deterministic projection/artifact. A
generator does not gain Production-enable or provider-mutation authority.

### Tests and developer-local orchestration

Tests may use temporary files, SQLite, subprocesses and synthetic fixtures without becoming Production
evidence. `scripts/verify-fast.py` may orchestrate local tools; it is non-authoritative, carries no
provider credentials and cannot substitute for permanent exact-head CI.

### Outer hosted/provider observation

A protected workflow may run a bounded Python observer/canary only when its owning stage explicitly
authorizes the environment, credentials and read/mutation effect. Output is secret-free versioned
observation for a typed owner. Default Python provider mutation authority is zero.

## Forbidden roles and directions

```text
Python as Product/domain/application semantic owner
Python as Release/Capability/Production authority
Python as D1/lifecycle/evidence/fitness duplicate owner
Python as hidden provider/GitHub mutation executor
Python secret readback or credential-report surface
Python runtime path bypassing Profile Bridge
opsctl -> Python child process
Product Runtime -> repository validator/generator
hand-maintained current Python estate registry
```

The permitted direction is:

```text
workflow / developer shell / bounded Python observer
-> explicit versioned data or CLI input
-> typed Rust owner/policy
```

Never `opsctl -> Python` for semantic work. Product Runtime calls only the specifically owned Camouhost
adapter boundary, not arbitrary repository Python.

## New or changed Python entrypoint

Every touched entrypoint records and proves:

1. one allowed role and natural owner;
2. exact effects (`filesystem`, `process`, `network`, `provider read/write`, `secret`, `runtime`);
3. current consumers and whether any predecessor is replaced;
4. versioned input/output contract where durable or cross-language;
5. secret/PII/logging constraints;
6. positive and negative/fail-closed tests;
7. no duplicate semantic authority;
8. explicit retirement condition when transitional;
9. no Production/provider mutation unless the owning stage and protected workflow authorize it.

Do not rewrite a valid Python boundary into Rust merely for language uniformity. Do retire or converge a
touched Python path that duplicates current policy, has no consumer/obligation or bypasses the accepted
effect boundary.
