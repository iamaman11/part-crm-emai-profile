# Post-AR-11 Functional Closure Plan — Release / Promotion Contract to 10/10

**Document status:** SUBORDINATE_REMEDIATION_PLAN  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Accepted AR-11 design:** `docs/ARCHITECTURE_REBASELINE_V3_AR11.md`  
**Baseline for this plan:** protected `main@586e6ea1be9f72b4f3a59c732714a8c12216985e`  
**Scope:** close residual functional gaps in the already accepted AR-11 release-set / promotion architecture  
**AR-12 implementation:** FORBIDDEN inside this plan  
**Production mutation / enablement:** FORBIDDEN  
**Historical AR-11 acceptance:** PRESERVED; this plan does not rewrite or revoke accepted history

This plan exists because a fresh functional audit of the accepted and subsequently hardened AR-11 implementation found that the architecture core is strong, but several edge conditions in the release trust, historical promotion and rollback contracts are not yet as complete as AR-11's own Definition of Done requires.

The objective is not to redesign AR-11. The objective is to make the existing design mechanically true end-to-end:

```text
accepted main source
-> immutable component artifacts
-> content-addressed Release Set
-> exact Capability Profile
-> complete compatibility decision
-> deterministic promotion plan
-> least-privilege provider execution
-> observed post-deploy verification
-> reusable historical known-good Release Set
-> compatibility-proven rollback
```

The completion standard is intentionally strict:

```text
P0 = 0
P1 = 0
P2 = 0 for in-scope AR-11 correctness/security/operability
all AR-11 mandatory positive and negative proofs mapped and green
no production mutation
no AR-12 implementation
```

---

## 1. Non-negotiable invariants

The following accepted architecture remains unchanged throughout this work:

```text
one protected main
one architecture hierarchy
one application/source history
one data/schema compatibility history
source_present != production_enabled
build once -> promote same bits
GitHub Actions/Environments = orchestration + approvals + credential boundary
opsctl = local typed policy/decision engine
provider executors = actual provider mutation authority
```

And the production state remains fail-closed:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

Do not:

- rebuild AR-11 from scratch;
- create a second release registry, second capability registry or hidden `opsctl` state backend;
- add `promotion execute` to `opsctl`;
- give `opsctl` network/provider/deployment/secret mutation authority;
- rebuild application artifacts during promotion;
- introduce Terraform or a generic IaC replacement;
- enable Production Core or mailbox capabilities;
- absorb AR-12 fresh-environment provisioning, AR-14 disaster recovery, AR-15 Windows updater/signing or AR-17 authorization work;
- rewrite accepted AR-11 provenance to make the historical acceptance event look different.

---

## 2. Current functional baseline

The following AR-11 outcomes are considered accepted foundations and must be preserved rather than reimplemented:

1. Canonical activation units and Capability Profiles exist and are projected under the existing architecture hierarchy.
2. `production-core-v1` / `rehearsal-core-v1` enable Core while mailbox administration/jobs/outbound mail remain disabled.
3. Backend execution is capability-gated before business/provider side effects, including HTTP routes, queue consumers and scheduled execution.
4. Frontend capability exposure is derived from authenticated backend projection; UI flags are not production authority.
5. Deployment Closure is profile-aware; Core does not require disabled Mail operational resources.
6. Release Set is content-addressed and artifacts are exact-byte verified.
7. Durable immutable publication uses GitHub Release assets; same Release Set ID cannot be overwritten with different bytes.
8. Promotion consumes immutable Release Set assets and does not rebuild application bits.
9. `opsctl release inspect|verify|compatibility` and `opsctl promotion plan|preflight|verify` exist as local non-mutating Rust policy surfaces.
10. Stale expected-current fencing and same-environment workflow concurrency exist.
11. `NO_CHANGE` is first-class.
12. Production promotion remains impossible during the architecture program.
13. Legacy D3 operational Python promotion authority has been retired after cutover.
14. The accepted-main `release-set.json` verification parity defect discovered after the original merge was fixed by #382.
15. Post-AR-11 lifecycle/governance cleanup and exact `opsctl` command authority parity were completed through #396.

No implementation unit below may weaken these invariants to make a new test pass.

---

## 3. Residual gaps that this plan must close

### FCG-1 — Historical immutable Release Sets are not first-class promotable targets

Current promotion resolves the Release Set source SHA and requires it to equal the **current** `main` HEAD. That is stronger than source acceptance and breaks the intended model after `main` advances.

Required semantic correction:

```text
WRONG:
release.source_sha == current_main_head

RIGHT:
release.source_sha is a proven accepted commit in protected main history
AND the immutable Release Set is the exact durable artifact published for that source
```

A previously accepted Release Set must remain eligible for staging promotion or application rollback after newer commits enter `main`, provided compatibility and policy gates pass.

### FCG-2 — Accepted-source evidence is structurally bound but not fully authoritative

The current Release Set binds repository + commit SHA into an `accepted_main_evidence_sha256`, but a self-consistent digest is not by itself proof that the SHA actually entered protected `main`.

The trust chain must distinguish:

```text
identity integrity != acceptance authority
```

AR-11 closure must provide one canonical, reproducible proof that a Release Set source belongs to accepted protected-main history without giving `opsctl` network or provider mutation authority.

### FCG-3 — `release verify` does not yet close every declared provenance identity

The final verifier must mechanically validate, not merely carry, all release-critical identities promised by AR-11, including:

- source acceptance evidence;
- component manifest identity;
- component source SHA agreement;
- component release identity;
- artifact digest + size + exact bounded inventory;
- contract identity;
- D1 schema authority identity;
- runtime lock / runtime identity;
- protocol identities;
- Cargo lock identity;
- Rust toolchain identity;
- frontend lock identity;
- release architecture identity;
- profile compatibility identity;
- any manifest sidecar required to verify a component later from durable publication alone.

No field may exist in the Release Set as decorative provenance that the canonical verifier cannot actually validate or explicitly classify as externally evaluated evidence.

### FCG-4 — Known-good rollback compatibility is too shallow

The current preflight can establish that a known-good Release Set exists and supports a profile, but that is not enough to prove that the old application bits are compatible with **current** schema/protocol/runtime state.

AR-11 requires application rollback to be allowed only when the observed state lies inside the known-good Release Set's compatibility window.

Required result vocabulary:

```text
COMPATIBLE
INCOMPATIBLE
UNKNOWN
```

`UNKNOWN` blocks rollback readiness.

### FCG-5 — Promotion credential exposure can be made materially stronger

Current staging promotion exposes deploy-capable Cloudflare credentials at the job boundary before the native preflight decision, because provider observation and deployment share one job/credential scope.

AR-11 already says preflight should occur before provider credentials are exposed **as far as workflow structure allows**. A 10/10 implementation should separate read-only observation authority from mutation authority and expose deployment credentials only after immutable intent + release verification + compatibility + preflight evidence are proven.

### FCG-6 — Mandatory AR-11 negative matrix needs explicit 1:1 behavioural traceability

Static source-marker checks are useful but are not a substitute for every behavioural negative requirement in AR-11.

Every mandatory negative case must map to one permanent test/gate with an unambiguous test identifier and expected fail-closed result.

---

## 4. Implementation sequence

The units below are proof boundaries, not a quota of one PR per heading. Merge boundaries follow semantic cohesion:

```text
one invariant
+ one independently reviewable proof obligation
+ safe intermediate main state
+ independent rollback value
= one bounded merge
```

If two adjacent changes cannot be safely or meaningfully verified independently, keep them in one bounded PR. Do not create micro-PRs for file-count aesthetics.

### FC-0 — Re-baseline and defect registration

Before implementation:

1. Re-read protected `main`, open PRs, branch protection and applicable permanent workflows.
2. Re-run the AR-11 functional audit against current code, not this plan's snapshot.
3. Create one tracking issue for the functional closure work and link this document.
4. Record the six FCG items above with severity and exact owning code/workflow paths.
5. Confirm AR-12 remains NOT STARTED and production invariants remain blocked.
6. Confirm no newer accepted change already closed a gap; remove obsolete work rather than reimplementing it.

Exit criterion: one live defect map, no stale-base implementation.

---

### FC-1 — Accepted Source Authority + historical Release Set eligibility

**Goal:** prove `was accepted main`, not `is current main`.

Required implementation:

1. Define one typed/versioned `AcceptedSourceEvidence` contract owned by the existing release architecture.
2. Evidence must bind at minimum:
   - repository identity;
   - exact 40-hex source commit;
   - protected-main lineage/acceptance proof;
   - evidence schema/version;
   - collection authority;
   - deterministic evidence digest.
3. Keep `opsctl` offline/read-only. Accepted-source proof may be supplied as saved machine evidence collected by GitHub orchestration, or evaluated from repository Git metadata through a non-network Rust implementation. Do **not** shell out to `git`, Python, Node, GitHub CLI or provider clients from `opsctl`.
4. Promotion must reject:
   - source SHA absent from protected-main history;
   - release tag target differing from Release Set source SHA;
   - source evidence for another repository/SHA;
   - malformed/unknown evidence;
   - mutable branch/ref substituted for immutable Release Set identity.
5. Replace the current `source_sha == current_main_head` requirement with protected-main historical acceptance/reachability proof.
6. A valid old Release Set must remain promotable after `main` advances.
7. Current-head Release Sets must continue to work unchanged.
8. Durable GitHub Release remains the artifact publication authority; accepted-source evidence must not become a second release registry.

Permanent proofs:

```text
current accepted main Release Set -> PASS
previous accepted-main Release Set after main advances -> PASS
commit not in protected main history -> REJECT SOURCE_NOT_ACCEPTED
release tag/source mismatch -> REJECT RELEASE_IDENTITY_MISMATCH
wrong repository -> REJECT
unknown/malformed acceptance evidence -> REJECT
```

Exit criterion: historical immutable Release Sets are first-class policy inputs without weakening source trust.

---

### FC-2 — Native Release Verifier provenance closure

**Goal:** `opsctl release verify` becomes the complete local verifier for every release-critical identity that can be proven locally.

#### FC-2A — Component manifest durability

Every `component_manifest_sha256` must refer to bytes that remain available in the durable Release Set.

For each component choose exactly one canonical pattern:

```text
A. manifest embedded inside the immutable component archive
OR
B. immutable manifest sidecar included in Release Set artifact_inventory
```

Do not retain orphan manifest hashes whose source bytes disappear after build transport expires.

At minimum close this for:

- control-plane/frontend component;
- mailbox secret resolver component;
- runtime bundle component;
- Windows Profile Bridge component.

If a sidecar is used, it is content-addressed/bounded exactly like every other Release Set artifact.

#### FC-2B — Typed provenance validation

Extend the Rust Release Set model away from unvalidated generic JSON for security-critical identities where a typed schema is practical.

`release verify` must validate:

1. Release Set content address.
2. exact source repository/SHA and accepted-source evidence contract;
3. every component source SHA == Release Set source SHA;
4. every component artifact SHA-256 + exact size;
5. exact artifact inventory, no extra/missing file;
6. symlink/path traversal/absolute path/duplicate path rejection;
7. component manifest bytes -> declared `component_manifest_sha256`;
8. component manifest release ID -> outer component release ID;
9. component manifest source SHA -> outer/source SHA;
10. contracts digest against canonical release input topology;
11. D1 evolution authority digest against canonical input;
12. runtime lock digest and runtime role;
13. Camouhost IPC / profile format / browser identity policy;
14. Cargo lock digest;
15. Rust toolchain digest;
16. frontend lock digest;
17. release architecture digest;
18. capability-profile compatibility IDs exist in canonical authority;
19. unknown release-critical fields fail closed according to schema policy.

No network access, provider credentials, child process or deployment authority is allowed.

#### FC-2C — Machine output

`release verify` output must expose typed result details without leaking payloads:

```json
{
  "schema_version": 1,
  "command": "release.verify",
  "decision": "VALID",
  "release_set_id": "...",
  "source_accepted": true,
  "verified_components": [],
  "verified_provenance_dimensions": [],
  "verified_files": 0,
  "verified_bytes": 0,
  "mutation_executed": false
}
```

Errors must use stable typed families; at minimum:

```text
SOURCE_NOT_ACCEPTED
RELEASE_IDENTITY_MISMATCH
COMPONENT_MANIFEST_MISMATCH
ARTIFACT_DIGEST_MISMATCH
ARTIFACT_INVENTORY_MISMATCH
SCHEMA_IDENTITY_MISMATCH
PROTOCOL_INCOMPATIBLE
RUNTIME_INCOMPATIBLE
PROVENANCE_IDENTITY_MISMATCH
PROFILE_NOT_AUTHORIZED
```

Exit criterion: no release-critical identity is merely decorative metadata.

---

### FC-3 — Full rollback compatibility evaluator

**Goal:** known-good means *usable now*, not merely *previously valid*.

#### FC-3A — DeploymentSnapshot v2

Extend the metadata-only observed-state snapshot only where required to evaluate rollback. It must remain free of secret values/customer data.

Add explicit observed identities as needed for:

- Catalog D1 ledger/schema state;
- Resolver D1 ledger/schema state when closure requires it;
- deployed component release IDs;
- active Capability Profile ID/digest;
- public/API contract identity where observable;
- resolver protocol identity where applicable;
- Bridge protocol identity where applicable;
- Camouhost IPC identity;
- runtime bundle/runtime role;
- profile format;
- browser identity policy;
- Windows delivery compatibility metadata when environment/policy requires it.

Unknown/unobservable required state is represented as `UNKNOWN`, never guessed.

#### FC-3B — Target and rollback evaluation use the same compatibility authority

Introduce one Rust-authoritative evaluation path that can answer:

```text
Can target Release Set T run against observed state S?
Can known-good Release Set K run against observed state S after rollback?
```

Do not implement rollback compatibility as a profile-membership shortcut.

For known-good K evaluate at minimum:

- profile compatibility;
- Catalog schema compatibility window;
- Resolver schema compatibility window when relevant;
- API/contract compatibility;
- resolver protocol compatibility;
- Bridge protocol compatibility;
- Camouhost IPC compatibility;
- runtime bundle compatibility;
- profile format compatibility;
- browser identity policy compatibility;
- Windows delivery compatibility metadata when required.

Result:

```text
COMPATIBLE -> rollback candidate valid
INCOMPATIBLE -> hard blocker ROLLBACK_INCOMPATIBLE
UNKNOWN -> hard blocker ROLLBACK_COMPATIBILITY_UNKNOWN
```

#### FC-3C — Preflight semantics

When replacing an existing deployment:

1. known-good Release Set is mandatory unless policy explicitly proves no rollback artifact is applicable;
2. known-good itself must pass `release verify`;
3. known-good source must satisfy Accepted Source Authority;
4. known-good rollback compatibility against current observed state must be `COMPATIBLE`;
5. stale expected-current fence remains mandatory;
6. production remains blocked.

Fresh environment retains the existing correct rule: no fictitious previous Release Set is required.

Permanent proofs:

```text
A -> B with A compatible with current state -> READY
A -> B with A schema-incompatible after B-required contract state -> BLOCK ROLLBACK_INCOMPATIBLE
A -> B with rollback protocol unknown -> BLOCK ROLLBACK_COMPATIBILITY_UNKNOWN
fresh environment with no prior release -> rollback artifact N/A, not fabricated
stale A->B plan after state moved -> PROMOTION_STALE
```

Exit criterion: rollback readiness is a compatibility proof, not existence metadata.

---

### FC-4 — Promotion workflow trust and least-privilege boundary

**Goal:** provider mutation credentials are exposed only after the strongest practical pre-mutation proof boundary.

Recommended workflow split:

```text
Job 1: resolve immutable intent
  no provider mutation credential
  -> resolve Release Set
  -> prove accepted source
  -> download durable assets
  -> release verify

Job 2: observe provider state
  least-privilege READ/OBSERVE credential only
  -> collect DeploymentSnapshot
  -> collect D1/readiness metadata
  -> release compatibility
  -> promotion plan
  -> promotion preflight
  -> publish signed/digested metadata-only preflight evidence

Job 3: protected staging executor
  GitHub Environment approval/credential boundary
  deploy-capable credential becomes available here
  -> re-download/re-verify immutable target or verify exact carried digests
  -> re-check expected-current fence immediately before mutation
  -> require preflight evidence == exact release/profile/environment/snapshot identity
  -> provider mutation from exact Release Set bits

Job 4: post-deploy verification
  -> re-observe state
  -> promotion verify == VERIFIED
  -> smoke exact staging origin
```

Rules:

1. Observation token and deploy token are different credential concerns where provider capabilities permit it.
2. Deploy token is not available to source-resolution, artifact-build or compatibility jobs.
3. No application build command may appear in promotion.
4. Provider executor still does not become policy authority.
5. Exact `release_set_id`, profile, environment, expected-current identity and preflight evidence digest must be carried across the environment boundary.
6. If provider state changes between preflight and mutation, re-check/fence blocks execution.
7. `NO_CHANGE` must skip mutation and still allow verification/evidence convergence.
8. Production environment/job remains absent or mechanically unreachable before its future owner.

Exit criterion: policy proof and mutation authority are structurally separated, not only procedurally ordered in one credential-rich job.

---

### FC-5 — Complete AR-11 behavioural certification matrix

Create one machine-readable or mechanically checked traceability table mapping every original AR-11 mandatory negative requirement to a permanent test/gate.

Minimum matrix (preserve original semantics):

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
13. disabled HTTP capability exposed -> failure;
14. disabled capability can enqueue -> failure;
15. disabled scheduled side effect -> failure;
16. disabled outbound mail can send/replay -> failure;
17. manipulated UI cannot bypass backend gate;
18. unknown Capability Profile -> reject;
19. profile digest mismatch -> reject;
20. profile not allowed in environment -> reject;
21. production promotion before authorization -> reject;
22. release from non-accepted source -> reject;
23. rebuild-on-promotion path -> CI reject;
24. staging/candidate artifact mismatch -> reject;
25. stale promotion plan -> reject;
26. parallel same-environment promotion -> serialize/reject;
27. D1 state unknown -> reject;
28. missing/incompatible rollback known-good -> blocker;
29. retired Python D3 operational authority becomes callable -> CI fail;
30. `opsctl` gains network/provider/secret mutation authority -> CI fail.

Add closure-specific regression cases:

31. old accepted Release Set after newer `main` -> allowed if compatible;
32. old Release Set not in protected-main ancestry -> reject;
33. component manifest sidecar/internal bytes mismatch -> reject;
34. known-good supports profile but runtime/protocol incompatible -> reject;
35. known-good rollback required evidence UNKNOWN -> reject;
36. provider changes after preflight before executor -> stale fence reject;
37. deploy-capable credential referenced before executor boundary -> CI fail.

The traceability check must fail if a required case has no permanent test identifier.

Exit criterion: no AR-11 acceptance requirement exists only as prose or source-marker intuition.

---

### FC-6 — Real staging same-bits / rollback rehearsal

This is **not** AR-12 fresh-environment provisioning. Use already supported staging resources and immutable releases only.

Required scenario with two accepted Release Sets, A and B:

```text
A = older accepted-main Release Set
B = newer accepted-main Release Set
```

Prove:

1. both A and B resolve to exact durable GitHub Release assets;
2. both sources are accepted protected-main history;
3. `release verify A` = VALID;
4. `release verify B` = VALID;
5. staging currently at A can plan/preflight B;
6. deployment uses B bytes exactly, no rebuild;
7. post-deploy `promotion verify B` = VERIFIED;
8. second B plan = NO_CHANGE;
9. A is evaluated as known-good against the post-B observed state;
10. if compatible, B -> A rollback promotion is accepted through the same canonical workflow;
11. rollback uses original A durable bytes, not a rebuild from A source;
12. post-rollback `promotion verify A` = VERIFIED;
13. second A plan = NO_CHANGE;
14. inject one incompatibility fixture/evidence case and prove rollback blocks before mutation;
15. production remains untouched.

If no compatible A/B pair exists naturally because schema/protocol evolution intentionally makes A incompatible, that is valid evidence only if the evaluator blocks rollback for the correct typed reason. In that case create a compatible two-release rehearsal pair through bounded non-production test/rehearsal evidence without falsifying production readiness.

Exit criterion: historical promotion and rollback semantics are proven against real staging/provider state, not just unit fixtures.

---

### FC-7 — Final whole-AR-11 functional acceptance audit

Perform a fresh audit from current protected `main` after all functional closure units are accepted.

Audit dimensions:

#### Release authority

- one canonical Release Set model;
- accepted-source proof is authoritative and historical, not current-HEAD-only;
- durable publication is immutable;
- all component/provenance bytes needed for verification remain durably available;
- `release verify` closes all locally provable identities;
- unknown state fails closed.

#### Capability isolation

- source-present disabled capabilities remain backend-inexecutable;
- frontend remains projection only;
- no independent production feature flags.

#### Promotion

- deterministic plan;
- `NO_CHANGE` convergence;
- expected-current fencing;
- same-environment serialization;
- historical accepted Release Set promotion;
- no rebuild;
- least-privilege credential boundary;
- post-deploy `VERIFIED` only success.

#### Rollback

- known-good durability;
- full current-state compatibility decision;
- incompatible/unknown rollback blocks before mutation;
- compatible old Release Set can be promoted from original bytes.

#### Tooling authority

- `opsctl` local/read-only/metadata-artifact verification only;
- no network/client/provider/secret/database/deployment/customer-state mutation authority;
- no dual Python D3 operational authority.

#### CI / evidence

- complete 1:1 mandatory negative matrix;
- Linux + Windows `opsctl` policy tests;
- Release Architecture Gate green;
- current applicable permanent workflows green on exact candidate heads;
- guarded merges use fresh exact-head evidence;
- durable accepted-main Release Set publication evidence observable after merge.

Final audit output must classify every finding:

```text
P0
P1
P2
P3
NOT_A_DEFECT
LATER_SLICE_BY_DESIGN
```

AR-11 Functional Closure is complete only when:

```text
P0 = 0
P1 = 0
P2 = 0 for AR-11 scope
all mandatory AR-11 behavioural requirements = PROVED
staging same-bits historical promotion/rollback rehearsal = PROVED or correctly BLOCKED by compatibility policy
production mutation = false
AR-12 implementation mixed into closure = false
```

---

## 5. Recommended code ownership map

Expected primary ownership; exact paths must be re-read from live `main` before each unit.

| Concern | Primary owner/path |
| --- | --- |
| Release Set typed model | `tools/opsctl/src/release/model.rs` |
| Artifact verification | `tools/opsctl/src/release/artifact.rs` |
| Static/local compatibility | `tools/opsctl/src/release/static_compatibility.rs` |
| Cross-component compatibility | `tools/opsctl/src/release/compatibility.rs` |
| Release CLI execution | `tools/opsctl/src/release/commands.rs` |
| Accepted source evidence | existing AR-11 release authority + new bounded Rust/evidence module; no second registry |
| Promotion plan | `tools/opsctl/src/promotion/plan.rs` |
| Promotion preflight | `tools/opsctl/src/promotion/preflight.rs` |
| DeploymentSnapshot | `tools/opsctl/src/promotion/snapshot.rs` |
| Post-deploy verify | `tools/opsctl/src/promotion/verify.rs` |
| Deployment closure | `tools/opsctl/src/promotion/authority.rs` |
| Release Set generator | `scripts/release-set-ar11.py` — generator only, not policy authority |
| Compatibility evidence adapter | `scripts/release-compat-evidence-ar11.py` — transport/adapter only |
| Provider snapshot collector | `scripts/deployment-snapshot-ar11.py` — collection only |
| Durable build/publish | `.github/workflows/release-set-build.yml` |
| Staging promotion executor | `.github/workflows/release-set-promotion.yml` |
| Permanent release gate | `.github/workflows/release-architecture-gate.yml` + associated validators/tests |
| Canonical policy source | `architecture/release-architecture-ar11.json` projected into existing inventory hierarchy |

Python may remain for deterministic packaging/collection adapters when it does not own the policy decision. Do not perform a cosmetic Python-to-Rust rewrite.

---

## 6. Required API / schema evolution rules

Any new or changed machine contract introduced here must be versioned and fail closed.

Preferred contracts:

```text
AcceptedSourceEvidence v1
DeploymentSnapshot v2 (only if v1 cannot be compatibly extended)
RollbackCompatibilityResult v1
ReleaseVerificationResult v1-compatible additive output where possible
PromotionPreflightResult v1-compatible additive output where possible
```

Rules:

1. Additive backwards-compatible output changes are preferred when safe.
2. If semantics change incompatibly, increment schema version; never silently reinterpret old evidence.
3. Unknown schema version -> reject/UNKNOWN.
4. Evidence binds exact Release Set ID/profile/environment where applicable.
5. Evidence digests are deterministic canonical JSON digests.
6. No secret values, tokens, customer payloads, fingerprint raw material or profile payload belong in these evidence objects.
7. Timestamps are metadata, not authority; compatibility must not become true merely because evidence is recent.
8. Stale provider snapshots must be rejected by explicit identity/fence policy, not heuristics.

---

## 7. Security acceptance rules

A 10/10 closure must preserve these threat boundaries:

### Supply chain

- immutable source identity;
- immutable component bytes;
- exact manifest/provenance binding;
- no artifact substitution through release tag/name;
- no mutable branch used as promotion input;
- no overwrite of an existing Release Set ID;
- no unexpected file/symlink/path traversal.

### Promotion TOCTOU

```text
observe S0
-> plan/preflight bound to S0
-> protected executor
-> re-check current identity == S0 immediately before mutation
-> mutate
-> observe S1
-> verify S1
```

Any state change between preflight and mutation rejects the stale transaction.

### Capability security

Frontend is never security authority. Direct API/queue/scheduled/provider paths remain gated by backend effective capability state.

### Credential exposure

Read-only observation and mutation credentials use least privilege and separate exposure boundaries where provider capabilities allow. No deploy-capable credential is available to build jobs.

### Rollback

Rollback is not a bypass around compatibility policy. Older bits do not receive privileged treatment merely because they were previously known-good.

---

## 8. Testing strategy

### Rust unit/property tests

Cover canonical JSON/digests, typed parsing, unknown fields/versions, manifest relationships, accepted-source evidence, compatibility decisions, rollback decision tables and stale fences.

### Integration tests

Exercise real `opsctl` command surfaces over filesystem fixtures:

```text
release inspect
release verify
release compatibility
promotion plan
promotion preflight
promotion verify
```

Run on Linux and Windows.

### Workflow static policy tests

Mechanically reject:

- current-main equality as historical acceptance authority;
- application rebuild commands in promotion;
- production environment/profile in AR-11 workflow;
- deploy credential in preflight/build job;
- Python D3 promotion authority resurrection;
- unregistered new operational command;
- missing concurrency/fence semantics.

### Hosted staging evidence

Exercise exact immutable Release Sets against real staging state as described in FC-6.

Static tests cannot substitute for the final staging proof where AR-11 owns that proof.

---

## 9. Acceptance discipline for every implementation merge

Before each bounded unit:

1. start from latest accepted protected `main`;
2. re-read this plan and live callers;
3. confirm no competing open PR owns the same invariant;
4. use one semantically bounded branch/PR;
5. add positive + negative permanent tests in the same candidate;
6. no temporary self-writing CI accepted into `main`;
7. exact-head applicable workflows green;
8. protected required contexts green;
9. `behind_by=0` before guarded merge;
10. zero blocking reviews;
11. zero unresolved review threads;
12. guarded merge bound to exact expected head;
13. accepted-main reread;
14. prove candidate tree == accepted merge tree where the repository's generic acceptance protocol requires it;
15. observe required push/main-only evidence directly where applicable;
16. do not infer green from inaccessible evidence.

A failed post-merge `Release Set Build` or durable publication check is an actual functional defect and reopens the affected closure unit.

---

## 10. Definition of Done — AR-11 Functional Closure 10/10

The closure is complete only when one current accepted repository state proves all of the following simultaneously.

### Accepted source

- old accepted-main Release Set remains verifiably accepted after `main` advances;
- non-main/unaccepted SHA is rejected;
- release tag/source/evidence identities cannot disagree;
- accepted-source proof has one authority.

### Immutable release

- Release Set is content-addressed;
- durable assets contain every byte needed for later verification;
- component manifest identity is actually verified;
- source/component/provenance/toolchain/contract/schema/runtime identities are checked;
- exact artifact inventory is enforced;
- same ID/different bytes is fatal.

### Promotion

- no source rebuild;
- historical compatible Release Set can be promoted;
- deterministic plan and `NO_CHANGE` work;
- concurrency is serialized;
- stale plans fail;
- policy evidence is bound to exact immutable inputs;
- deploy credential exposure occurs only at the protected executor boundary as far as provider/workflow structure allows;
- production remains unreachable.

### Rollback

- known-good is durable and source-accepted;
- rollback compatibility evaluates current schema/protocol/runtime state;
- incompatible/unknown rollback fails closed;
- compatible rollback consumes original known-good bytes;
- rollback converges to `VERIFIED` and then `NO_CHANGE`.

### Capability isolation

- `source_present=true` and `production_enabled=false` remains mechanically demonstrable;
- disabled HTTP/queue/schedule/outbound paths cannot produce side effects;
- frontend manipulation cannot bypass backend capability gate.

### Tooling

- exact active `opsctl` command registry remains in parity;
- `opsctl` has no provider/network/secret/database/deployment/customer/production mutation authority;
- no second callable Python D3 operational policy authority.

### Evidence

- original mandatory 30-case negative matrix has 1:1 permanent test mapping;
- closure regressions 31–37 are permanently covered;
- Linux and Windows regression suites pass;
- real staging same-bits historical promotion/rollback proof exists where compatibility permits;
- final fresh audit finds P0=0, P1=0 and P2=0 for AR-11 scope.

Final state remains:

```text
AR-11 = historically ACCEPTED + functionally CLOSED
AR-12 = current / implementation not mixed into this closure
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

Only after this Definition of Done is mechanically proven should the project describe AR-11 as **fully functional / 10/10 closed** rather than merely historically accepted.
