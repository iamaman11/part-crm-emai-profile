# Architecture Re-baseline v3 — AR-11 Immutable Release Set, Production Capability Profile, Progressive Activation, Frontend Projection & Promotion Architecture

**Document status:** CURRENT_IMPLEMENTATION_CANDIDATE  
**Owning issue:** #372  
**Parent program:** #266  
**Subordinate pre-production authority issue:** #268  
**Accepted predecessor:** AR-10 / PR #371 + acceptance closeout PR #373  
**Exact start base:** `main@1730a5655b07cda21ea9eb3c2cd7a754c7143ca3`  
**Canonical implementation branch:** `agent/ar11-release-capability-promotion`  
**Completion PR:** one Draft PR for the complete AR-11 cutover  
**Production mutation:** `false`

## 1. Purpose and authority boundary

AR-11 establishes the repository-owned release and promotion policy architecture required for one `main`, one architecture hierarchy and one compatibility/data history while allowing source-present functionality to remain production-disabled until its own activation gate.

This file is a subordinate implementation plan/evidence document. It does **not** replace `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`, issue #266 or the canonical machine hierarchy in `architecture/inventory.json`.

After AR-11 the system must answer four independent questions mechanically:

1. **WHAT SOURCE?** — which exact accepted Git SHA is the release source;
2. **WHAT BITS?** — which immutable component artifacts were built from that source;
3. **WHAT MAY RUN?** — which activation units are enabled by the selected Capability Profile;
4. **MAY THIS ENVIRONMENT MOVE TO IT?** — whether schema/runtime/protocol/resource compatibility and promotion gates allow the transition.

Target chain:

```text
accepted main SHA
  -> immutable component releases
  -> Release Set
  -> Production Capability / Release Profile
  -> profile-aware Deployment Closure
  -> deterministic Promotion Plan
  -> GitHub Environment approval/orchestration
  -> provider executors
  -> post-deploy machine verification
```

AR-11 authorizes none of the provider mutation in that chain.

Throughout AR-11:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

PC-1 remains the only future owner of the first production rollout after successful AR-17 authorization.

## 2. Central model: Release Set and Capability Profile are different authorities

AR-11 must never collapse artifact identity and activation policy.

### 2.1 Release Set — WHAT BITS?

A Release Set identifies the exact immutable bits and compatibility identities that compose one releasable candidate, including at minimum:

- repository + accepted source commit SHA;
- control-plane Worker release identity;
- frontend artifact identity;
- mailbox-secret-resolver Worker release identity;
- runtime-bundle identity;
- public/generated contract identity;
- Catalog D1 schema contract;
- Resolver D1 schema contract;
- Bridge protocol identity;
- Camouhost IPC identity;
- profile-generation/profile-format identity;
- runtime/browser identity policy;
- toolchain/lockfile/provenance identities;
- bounded artifact inventory.

### 2.2 Capability Profile — WHAT MAY RUN?

A Capability Profile selects a reviewed activation combination from the source-present release without modifying application bits.

Conceptual `production-core-v1` profile:

```text
identity              ENABLED
users                 ENABLED
clients               ENABLED
browser_profiles      ENABLED
profile_runtime       ENABLED
camoufox              ENABLED
mailbox_admin         DISABLED
mailbox_jobs          DISABLED
outbound_mail         DISABLED
```

Environment selection must reference one known profile identity + digest. Arbitrary independent production flags such as `ENABLE_MAIL=true` or `VITE_ENABLE_MAIL=true` are forbidden as activation authority.

## 3. Activation unit is not the same as application owner

Existing architecture capability names such as `clients`, `client_mail`, `profiles`, `mailboxes`, `devices` and `notifications` describe bounded/application ownership. Production rollout needs finer activation granularity.

AR-11 therefore introduces an explicit `activation_unit` concept. Example units include:

- `mailbox_admin`;
- `mailbox_client_binding`;
- `mailbox_read`;
- `mailbox_jobs`;
- `outbound_mail`;
- `browser_profiles`;
- `profile_runtime`;
- `camoufox`.

`application_owner != activation_unit` is intentional. A route or worker surface can remain owned by `mailboxes` or `client_mail` while its independent production activation unit is disabled.

This avoids artificial bounded-context/crate fragmentation while allowing PC-1/PC-2/PC-3/PC-4 to be profile transitions over one application tree.

## 4. AR-11A — Canonical Release Architecture Authority

Extend the existing canonical hierarchy; do not create a second registry.

`architecture/inventory.json` remains the canonical machine architecture hierarchy. Normalized source JSON is allowed only when it is deterministically projected into that inventory and permanent drift checks prove zero divergence.

The canonical projection must cover at least:

- `release_architecture`;
- `production_capabilities`;
- `release_profiles`;
- `deployment_closures`;
- `component_release_owners`;
- `promotion_policy`;
- `compatibility_dimensions`;
- `artifact_authority`.

A permanent generator/validator path must prove deterministic source -> inventory projection and reject drift, unknown fields and duplicate authority.

## 5. AR-11B — Production Capability / Activation Unit model

Each capability record must mechanically define at least:

```text
capability_id
architecture_owner
application_owner
activation_unit
source_present
accepted
production_enabled
dependencies[]
incompatible_with[]
backend_enforcement[]
frontend_projection
required_components[]
required_bindings[]
required_resources[]
required_credentials[]
schema_requirements[]
protocol_requirements[]
runtime_requirements[]
requires_windows_profile_bridge
windows_compatibility
activation_gate
```

The model consumes issue #268 and must fail closed when required evidence is missing, stale, incompatible or unknown.

Capability dependency closure is transitive. A profile is invalid if it enables a dependent while disabling or failing a required dependency.

For the current Production Core target, `camoufox` depends on the accepted Profile Bridge/runtime chain and future AR-15 Windows delivery evidence. Until AR-15 is accepted and compatible, the relevant delivery requirement remains `UNSATISFIED`; AR-11 must not fabricate Windows readiness.

## 6. AR-11C — Execution Surface Coverage and central backend enforcement

`source_present != production_enabled` must cover every side-effecting executable surface, not just HTTP routes.

Inventory and classify at least:

- HTTP/API routes;
- Queue producers;
- Queue consumers;
- scheduled handlers;
- Durable Object ingress;
- service bindings;
- device/Profile Bridge commands;
- background jobs;
- outbound provider side effects;
- frontend navigation/actions/deep links.

Every execution surface has at least:

```text
surface_id
activation_unit
enforcement_point
disabled_behavior
```

Permanent CI must require every side-effecting execution surface to belong to exactly one activation unit or explicit foundation surface.

Backend enforcement must be centralized conceptually as one `CapabilityGate` evaluating `(surface, effective_profile)` before business side effects. Scattered environment conditionals are not an accepted authority.

Disabled capability negative proofs include at minimum:

- `mailbox_jobs=DISABLED` -> create-job API rejected;
- `mailbox_jobs=DISABLED` -> scheduled dispatcher cannot act;
- `mailbox_jobs=DISABLED` -> queued/replayed message cannot produce provider side effects;
- `outbound_mail=DISABLED` -> send API rejected;
- `outbound_mail=DISABLED` -> old/replayed intent cannot dispatch;
- direct API invocation remains rejected even when frontend state is manipulated.

## 7. AR-11D — Profile-aware Deployment Closure

A Capability Profile determines the exact required provider resource/binding/credential closure.

Core resources must not accidentally inherit Mail-only requirements merely because Mail code exists in the same source tree or Wrangler configuration.

The model must derive required closure for at least:

- Workers/components;
- D1 databases;
- R2 buckets;
- Durable Objects;
- Queues + DLQs;
- service bindings;
- routes/schedules;
- Access/provider configuration;
- credential metadata identities.

Binding probes and readiness checks must verify only the active profile's required closure.

If `mailbox_jobs=DISABLED`, missing Mailbox Queue infrastructure must not fail Core health. If `mailbox_jobs=ENABLED`, the missing Queue is a fail-closed readiness error.

## 8. AR-11E — Immutable multi-component Release Set

Reuse and compose existing accepted component release builders; do not replace their semantics.

`ReleaseSet v1` must be content-addressed from canonical JSON containing immutable identity fields.

```text
release_set_id = "release-set-v1-sha256-" + SHA256(canonical_payload)
```

Human-readable display versions may exist but never replace the digest authority.

Minimum structure:

```text
schema_version
release_set_id
source { repository, commit_sha }
components { control_plane, secret_resolver, runtime_bundle, ... }
contracts
protocols
schemas
runtime_compatibility
capability_profile_compatibility
platform_delivery_requirements
build_provenance
artifact_inventory
```

Release Set compatibility must consume, not duplicate, AR-9 D1 compatibility semantics.

Required compatibility dimensions include:

- API/OpenAPI identity;
- frontend <-> API contract digest;
- Catalog D1 min/max/target;
- Resolver D1 min/max/target;
- Bridge protocol;
- Camouhost IPC;
- runtime-bundle format;
- profile-generation/profile-format;
- browser identity policy;
- resolver service protocol;
- Windows Bridge/update compatibility requirements.

Cross-component compatibility result is one of `COMPATIBLE`, `INCOMPATIBLE`, `UNKNOWN`; `UNKNOWN` fails closed.

The cross-component dependency graph must be acyclic or otherwise prove an explicit compatible intermediate state. Unsatisfied cycles invalidate the release.

## 9. Build-once and same-bits promotion invariants

A component release is built once, digested, published and then reused byte-for-byte through rehearsal, staging and future production.

Forbidden:

```text
build staging from main
build production again from same SHA
```

Promotion receives immutable artifact identities, never a source branch for rebuild.

Environment overlays may change only environment/provider identity such as account/resource IDs, Worker names, routes, service/binding handles and secret handles. They must not change Worker JS/WASM, frontend bundle, application code, contract contents or runtime bundle bytes.

Post-promotion verification proves same-bits identity.

## 10. AR-11F — Durable Artifact Publication Authority

Actions artifacts remain CI/evidence transport, not the only long-term production release authority.

AR-11 defines one canonical durable immutable release artifact authority. Preferred project model: GitHub Release assets keyed by content-addressed Release Set ID, with optional non-authoritative mirrors/caches only.

Publication is immutable:

- if a release ID does not exist, publish exact verified artifacts;
- if it exists, verify byte equality;
- any byte difference for the same release ID is fatal;
- overwrite is forbidden.

Build workflow holds no deployment/provider credentials.

## 11. AR-11G — `opsctl release`

AR-11 activates these previously reserved Rust commands:

```text
opsctl release inspect
opsctl release verify
opsctl release compatibility
```

`opsctl` remains a local policy engine over repository files and saved machine evidence:

```text
network = false
provider credentials = false
provider mutation = false
Wrangler spawn = false
Python/Node child process = false
hidden state backend = false
```

### `release inspect`

Parse Release Set and emit typed/versioned machine output with immutable identity, source SHA, component identities, schema/protocol/runtime identities and compatible profiles. No side effects.

### `release verify`

Verify canonical JSON, Release Set digest, source identity, component manifest identity, artifact bytes/digests, duplicates/missing/unknown components, contract/schema/runtime/toolchain identities and bounded artifact inventory. Unknown state/field fails closed where the schema requires closure.

### `release compatibility`

Aggregate accepted compatibility policies:

- AR-9 D1 compatibility;
- API/protocol compatibility;
- Bridge/Camouhost/runtime compatibility;
- capability dependency closure;
- Windows delivery requirements metadata;
- rollback compatibility.

Output is typed and includes `compatible`, blockers, warnings and required steps.

## 12. AR-11H — `opsctl promotion`

AR-11 activates:

```text
opsctl promotion plan
opsctl promotion preflight
opsctl promotion verify
```

There is deliberately **no** `promotion execute` command.

### `promotion plan`

Inputs:

- target environment;
- target Release Set;
- target Capability Profile;
- current observed deployment snapshot;
- current D1 ledgers/evidence;
- current effective release/profile.

Output is deterministic and returns either first-class `NO_CHANGE` or an ordered plan partitioned by executor authority, e.g.:

```text
GITHUB_ORCHESTRATION
D1_MIGRATION_EXECUTOR
WRANGLER_DEPLOY
PROVIDER_RESOURCE
CAPABILITY_PROFILE_SWITCH
POST_DEPLOY_VERIFY
```

### `promotion preflight`

Hard gate before provider credentials are exposed as far as workflow structure allows. It verifies exact environment, release/profile identity, accepted source, release validity, dependency/resource closure, D1/protocol/runtime compatibility, credential readiness metadata, rollback candidate availability, current-state identity and expected GitHub Environment.

Any `UNKNOWN` blocks.

Production mutation remains blocked throughout AR-11..AR-17. A diagnostic production plan may be computed, but no executable production mutation path may exist before future gate ownership.

### `promotion verify`

Consumes a versioned provider-collected `DeploymentSnapshot` and verifies expected Release Set, Capability Profile, resources/bindings, D1 state and runtime/protocol/component identities.

Result is one of:

```text
VERIFIED
DRIFTED
INCOMPLETE
UNKNOWN
```

Only `VERIFIED` is success.

## 13. DeploymentSnapshot

Define a versioned metadata-only observed-state schema including at least:

```text
environment
collected_at
workers[] { name, provider deployment/version identity, component release identity }
d1[]
r2[]
queues[]
dlqs[]
durable_object_bindings[]
service_bindings[]
routes[]
schedules[]
credential_metadata_identities[]
capability_profile_identity
```

No secret values or customer data belong in the snapshot.

Provider executors collect state; `opsctl` evaluates it. Provider state remains runtime truth.

## 14. AR-11I — D3 Python -> Rust policy cutover

AR-6 already assigns these operational paths to AR-11 cutover:

```text
scripts/_mailbox_secret_resolver_promotion_core.py
scripts/mailbox-secret-resolver-promotion.py
```

Required order:

1. inventory current accepted Python D3 semantics;
2. port policy semantics to Rust release/promotion modules;
3. replay accepted D3 positive/negative fixtures against Rust;
4. prove parity;
5. cut workflows to Rust;
6. prove one legitimate operational policy authority;
7. retire the Python operational path without global Python rewrite.

Preserve accepted D3 invariants: release identities, staging evidence validation, deployment closure, same-bits production rule, cross-environment secret separation, caller-auth consistency and production-lane fail-closed behavior.

After cutover, a second callable Python D3 operational mutator is an AR-11 failure. Python validators/generators/tests remain allowed where classified.

## 15. AR-11J — GitHub release/promotion workflows

Minimum workflow authority split:

### Release Set Build

```text
accepted main SHA
  -> build required components once
  -> verify component releases
  -> construct Release Set
  -> opsctl release verify
  -> publish immutable durable artifacts
  -> publish release-set manifest
```

Release build has no Cloudflare production token and no deployment secrets.

### Promotion

Inputs are only immutable identities:

```text
release_set_id
capability_profile_id
environment
```

Forbidden in promotion jobs:

- branch/source selection as release authority;
- `git pull` as release resolution;
- cargo/npm application rebuild;
- Wrangler deploy from rebuilt source.

Promotion downloads exact immutable artifacts, verifies digests, runs `opsctl promotion preflight`, crosses the protected GitHub Environment approval boundary, invokes the provider executor, then runs post-deploy verification.

AR-9 D1 migration commands/workflow remain the migration authority; AR-11 only coordinates their ordering.

## 16. Schema rollout and rollback model

AR-11 respects expand/contract compatibility windows from AR-9.

Contract migrations are not automatically coupled to code rollout. Rollback is split into:

1. **application Release Set rollback** — allowed only when current schema/protocol/runtime state is within the prior release compatibility window;
2. **database recovery/rollback** — owned later by AR-14, not AR-11.

If current schema is incompatible with the known-good Release Set, automatic application rollback is blocked.

Cloud release state may classify `current`, `candidate` and `known_good`, but `opsctl` must not hide this as mutable private state; it classifies provider/deployment evidence.

## 17. AR-11K — Promotion concurrency, fencing and idempotency

Promotion must serialize by environment/release lane, for example `promotion-staging` and future `promotion-production`.

Define deterministic transaction identity from immutable transition inputs, e.g. SHA-256 of environment + current release + target release + target profile.

Repeated identical input is idempotent.

Every executable promotion plan carries expected-current identity/fence. A stale A->B plan must fail if provider state has moved to B->C or any unexpected identity.

Parallel same-environment promotion is serialized or rejected before mutation.

## 18. AR-11L — Frontend Capability Projection & fail-closed UX

AR-11 does not rebuild the product UI. Existing feature modules remain. AR-11 binds frontend exposure to the same effective Capability Profile used by backend enforcement.

Target flow:

```text
canonical Capability Profile
  -> backend effective capabilities
  -> authenticated/session/bootstrap projection
  -> frontend capability state
  -> router/navigation/actions/feature entrypoints
```

Frontend must not independently determine production authorization.

For a disabled capability:

- navigation is hidden/disabled;
- route entry is unavailable/safely handled;
- actions/buttons are unavailable;
- background UI action is impossible;
- deep links fail safely.

UI projection is a UX boundary only. Backend CapabilityGate remains the security boundary.

Independent production-authority build/runtime flags such as `VITE_ENABLE_MAIL`, `SHOW_MAILBOXES` or `ENABLE_OUTBOUND_MAIL` are forbidden.

## 19. AR-11M — Supply-chain and artifact boundary

Each Release Set binds at least:

- source SHA;
- component digests;
- lockfile digests;
- toolchain identity;
- generated-contract digest;
- schema authority digest;
- runtime manifest digest;
- artifact inventory;
- deterministic SBOM identity where practical without introducing a new supply-chain platform.

Artifact verification rejects unexpected files, symlinks, absolute paths, `../` traversal, duplicate paths, unknown executables, secret files and environment-specific secret material.

## 20. AR-11N — Permanent Release Architecture Gate

Add a permanent required workflow/gate that checks at least:

- canonical release architecture projection and zero second registry;
- Capability Profiles and activation dependency graph;
- execution-surface coverage;
- central backend fail-closed enforcement;
- profile-aware deployment closure/binding probe;
- Release Set schema/content addressing;
- `opsctl release/*`;
- `opsctl promotion/*`;
- D3 Rust parity and no dual Python operational authority;
- build-once/no-rebuild/same-bits invariants;
- durable artifact publication policy;
- frontend projection from backend capability authority;
- production remains blocked.

## 21. Machine output and typed errors

All release/promotion commands emit versioned machine output. Example shape:

```json
{
  "schema_version": 1,
  "command": "promotion.preflight",
  "decision": "BLOCKED",
  "blockers": [],
  "warnings": [],
  "mutation_executed": false
}
```

Required typed error families include at minimum:

```text
RELEASE_IDENTITY_MISMATCH
ARTIFACT_DIGEST_MISMATCH
SOURCE_NOT_ACCEPTED
SCHEMA_INCOMPATIBLE
PROTOCOL_INCOMPATIBLE
CAPABILITY_DEPENDENCY_UNSATISFIED
PROFILE_NOT_AUTHORIZED
PROMOTION_STALE
ROLLBACK_INCOMPATIBLE
PROVIDER_STATE_UNKNOWN
```

`NO_CHANGE` is a first-class successful planning result and must be deterministic.

## 22. Mandatory negative matrix

AR-11 acceptance includes permanent negative proofs for at least:

1. artifact from another SHA -> reject;
2. changed component digest -> reject;
3. Release Set digest mismatch -> reject;
4. missing artifact -> reject;
5. duplicate artifact/component -> reject;
6. unknown component -> reject;
7. contract digest mismatch -> reject;
8. Catalog schema incompatible -> reject;
9. Resolver schema incompatible -> reject;
10. Bridge protocol incompatible -> reject;
11. runtime-bundle incompatible -> reject;
12. capability dependency missing -> reject;
13. disabled HTTP capability exposed -> test failure;
14. disabled capability can enqueue -> test failure;
15. disabled scheduled side effect -> test failure;
16. disabled outbound mail can send/replay -> test failure;
17. manipulated UI cannot bypass backend gate;
18. unknown Capability Profile -> reject;
19. profile digest mismatch -> reject;
20. profile not allowed in environment -> reject;
21. production promotion attempt before AR-17 -> reject;
22. release from non-accepted source -> reject;
23. rebuild-on-promotion path -> CI reject;
24. staging/candidate production artifact mismatch -> reject;
25. stale promotion plan -> reject;
26. parallel same-environment promotion -> serialize/reject;
27. D1 state unknown -> reject;
28. missing rollback-compatible known-good -> blocker/high-risk according to policy;
29. old Python D3 operational authority remains callable after cutover -> CI fail;
30. `opsctl` gains network/provider/secret mutation -> CI fail.

## 23. Positive matrix

Prove on one unchanged exact head:

```text
accepted SHA
  -> immutable control-plane release
  -> immutable resolver release
  -> Release Set
  -> release verify PASS
  -> compatible Capability Profile PASS
  -> deterministic promotion plan
  -> staging-compatible preflight PASS over saved/non-production evidence
  -> same Release Set post-deploy verification PASS where bounded non-prod evidence is owned by AR-11
  -> converged second plan = NO_CHANGE
```

Fresh-environment end-to-end provisioning proof belongs to AR-12.

## 24. Strict boundaries with later slices

AR-11 does **not** absorb:

- AR-12 fresh rehearsal provisioning/convergence proof;
- AR-13 real credential rotation rehearsal;
- AR-14 disaster recovery/restore/RTO/RPO;
- AR-15 signed Windows updater/trust/side-by-side/LKG implementation;
- AR-16 final whole-project audit;
- AR-17 Production Core authorization;
- PC-1 production provisioning/promotion;
- mailbox capability activation.

## 25. Explicit non-goals

```text
NO production deploy
NO production provisioning
NO production capability activation
NO Terraform
NO generic IaC engine
NO hidden opsctl state database
NO opsctl provider credentials
NO opsctl network client
NO opsctl Wrangler spawn
NO rebuild during promotion
NO schema rollback engine
NO global Python rewrite
NO Windows updater implementation
NO disaster recovery implementation
NO mailbox activation
```

## 26. Implementation structure

Logical implementation units, all inside the single AR-11 branch/PR:

```text
AR-11A Canonical Release Architecture Authority
AR-11B Production Capability / Activation Unit model
AR-11C Execution Surface + Backend Enforcement Coverage
AR-11D Profile-aware Deployment Closure
AR-11E Multi-component Immutable Release Set
AR-11F Durable Artifact Publication Authority
AR-11G opsctl release inspect/verify/compatibility
AR-11H opsctl promotion plan/preflight/verify
AR-11I Legacy D3 Python -> Rust parity/cutover
AR-11J GitHub release-set + promotion workflow integration
AR-11K Promotion concurrency/fencing/idempotency
AR-11L Frontend capability projection
AR-11M Supply-chain/release provenance closure
AR-11N Permanent fitness gate + acceptance
```

These are logical units, not separate accepted slices or separate merges.

## 27. Binding execution model — one branch, one Draft PR, one merge

AR-11 is an atomic architecture cutover and has exactly one completion merge into `main`.

```text
accepted AR-10 main
  -> ONE AR-11 branch
  -> ONE Draft PR
  -> all AR-11A..N work
  -> final exact-head verification
  -> ONE guarded merge into main
  -> accepted-main reread
  -> AR-11 accepted / AR-12 current
```

Rules:

1. one working branch: `agent/ar11-release-capability-promotion`;
2. one Draft completion PR;
3. AR-11A..N are internal implementation units only;
4. internal commits/fixes are allowed;
5. no intermediate merge into `main`;
6. no partial AR-11 acceptance;
7. any new commit changes the candidate SHA and invalidates prior exact-head acceptance evidence;
8. final acceptance is only on one unchanged exact head;
9. all applicable permanent workflows must be green on that head;
10. before merge: `behind_by=0`, blocking reviews `=0`, unresolved threads `=0`;
11. one guarded merge bound to the exact expected head SHA;
12. accepted-main reread after merge;
13. only then may canonical state become `AR-11=ACCEPTED`, `AR-12=CURRENT`;
14. no AR-12 implementation is mixed into the AR-11 PR.

## 28. AR-11 Definition of Done

AR-11 is not accepted until one repository tree simultaneously proves:

### Machine authority

- Capability Profile exists under the canonical inventory hierarchy;
- activation units are granular enough for PC-1/2/3/4;
- all side-effecting execution surfaces are classified;
- release architecture projects deterministically into `architecture/inventory.json`;
- competing release/capability registry count is zero.

### Release

- component releases are immutable;
- Release Set is content-addressed;
- multi-component compatibility is checked;
- build-once/no-rebuild/same-bits is mechanically proven;
- durable artifact publication authority is defined and overwrite-safe.

### `opsctl`

These work on Linux + Windows:

```text
release inspect
release verify
release compatibility
promotion plan
promotion preflight
promotion verify
```

with no network, provider credentials, Python/Node child process, Wrangler spawn or provider mutation.

### D3 cutover

- accepted D3 positive/negative invariants are preserved;
- Python operational D3 authority is retired after parity;
- duplicate mutable authority count is zero.

### Capability isolation

Machine proof demonstrates `source_present=true` with `production_enabled=false` while backend execution remains impossible for the disabled activation unit.

### Promotion

- deterministic plan;
- `NO_CHANGE` convergence result;
- stale plan rejected;
- concurrent promotion serialized/rejected;
- same-bits verified;
- rollback compatibility evaluated;
- production execution remains impossible.

### Acceptance

- one unchanged exact candidate SHA;
- all applicable permanent workflows green;
- `behind_by=0`;
- zero blocking reviews;
- zero unresolved threads;
- guarded merge;
- accepted-main reread;
- final canonical state remains:

```text
AR-11 accepted
AR-12 current
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

## 29. First implementation checkpoint

The accepted AR-10 tree already provides two deliberately source-reserved Rust namespaces:

- `tools/opsctl/src/release/mod.rs` with target actions `inspect`, `verify`, `compatibility` and no provider mutation authority;
- `tools/opsctl/src/promotion/mod.rs` with target actions `plan`, `preflight`, `verify`, with GitHub approval and provider mutation authority explicitly false.

AR-11 starts by converting those placeholders into typed, testable local policy modules while preserving the permanent no-network/no-provider/no-child-process boundary. In parallel, the canonical capability/activation authority and execution-surface projection must be established before backend/UI activation logic is wired, so no second flag system emerges.

The current accepted machine inventory already correctly records `accepted_checkpoint=AR-10` and `current_work=AR-11`; human projections containing stale AR-10-current wording must be corrected as part of the AR-11 candidate rather than treated as a new authority.
