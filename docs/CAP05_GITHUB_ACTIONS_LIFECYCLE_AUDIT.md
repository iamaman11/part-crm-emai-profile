# CAP-05 — GitHub Actions, checks, and lifecycle invariant audit

**Issue:** #496  
**Document role:** one-time research/audit output; **NOT** a new semantic authority, workflow registry, CI framework, or implementation authorization  
**Audit base:** protected `main@c0854be18d6687b5a51c4977c3268b0e92380ef1`  
**Audit branch:** `cap05/github-actions-lifecycle-audit`  
**Provider/staging/production mutation:** none  
**Workflow/checker/governance mutation:** none  

## 1. Executive conclusion

The repository does not have a simple problem of “too many checks”. It has a generally strong set of specialized architecture, security, contract, platform, and operational proofs, but several lifecycle and orchestration boundaries have drifted as the architecture program advanced.

The highest-confidence findings are:

1. `architecture/github-actions-registry.json` plus `.github/scripts/github-actions-registry.mjs` has become a second lifecycle/admission authority. The script hard-codes `EXPECTED_ACTIVE = 23`, `EXPECTED_PERMANENT = 21`, and specific historical AR-10/AR-11 workflow identities as permanently required. `architecture/architecture-acceptance-policy.json` consumes that classification when deciding which workflows every architecture candidate must pass. This contradicts the current program authority, which explicitly states that historical workflow counts/names/SHAs are observations, never timeless constants, and forbids a new hand-maintained global authority catalog.
2. `External Evidence Metadata` and `External Readiness Projection` are separate protected required contexts, but both call the same `cargo test --locked --manifest-path tools/opsctl/Cargo.toml --test external_evidence_policy` checker path. The readiness workflow has no unique checker path in the audited YAML. This is a proven duplicate CI caller/protected context, not a competing Rust policy implementation.
3. Several workflows mix permanent objective invariants with completed phase/cutover ceremony. The clearest example is `camoufox-runtime-gate.yml`: current real-runtime/platform proof is mixed with AR-10 cutover/closeout checks. `release-architecture-gate.yml` and `ar11-fc6-operator-transport.yml` are stage/program checks but are classified as permanent by the current Actions registry.
4. The large `quality-gate.yml`, `repository-quality-audit-gate.yml`, and `cross-component-acceptance-gate.yml` repeatedly compile/test overlapping Rust, WASM, frontend, and historical checker surfaces. This is primarily duplicated orchestration and scope, not proof that semantic policy is duplicated.
5. `local-profile-gate.yml` includes full `opsctl`/architecture lifecycle verification and Windows artifact work in a Local Profile context. That is a caller/scope ownership problem: unrelated release/lifecycle changes can affect a profile-specific protected context.
6. Release/component build responsibilities are duplicated. The current Release Set v3 flow is the canonical durable build/publish boundary, while other jobs independently rebuild worker/resolver/Profile Bridge artifacts. The standalone resolver release flow is a predecessor-retirement candidate, but external/current consumer count is not proven zero by this audit.
7. `resolver-d1-first-bootstrap.yml` protects a one-time/fresh-database ceremony and its checker self-requires the workflow/checker surface. It is not safe to delete today: AR-12/AR-13 still contain fresh-environment/rotation obligations and the authority describes a future external bootstrap ceremony. Its correct lifecycle is `STAGE_BOUND / TRANSITIONAL`, with destructive retirement only after a verified successor and zero remaining consumer/compatibility obligation.

No current evidence justifies adding a new generic invariant registry or checker-for-checker. The right target is tiered, risk-based execution around existing natural checker owners, plus explicit retirement of transitional estate.

## 2. Method and evidence grades

This audit used fresh GitHub repository/live observations and the exact audited `main` tree. Chat checkpoints were not treated as authority.

Evidence grades:

- `PROVEN`: directly observed from the exact repository, branch protection projection, workflow/check-run data, or a successful exact-main hosted check whose implementation was read.
- `UNPROVEN`: the available GitHub integration could not expose enough history or live detail to establish the fact without guessing.
- `CANDIDATE`: evidence identifies a likely retirement/consolidation target, but the required zero-consumer or replacement proof is not complete.

The direct Actions workflow-registration listing endpoint is not exposed by the available connector. Exact-main `GitHub Governance Hosted State` succeeded, and its implementation runs the live Actions-registry audit, so the canonical/live registration match at that run is indirectly `PROVEN`; direct enumeration through this audit client remains `UNPROVEN`.

## 3. Fresh protected-main / live-governance baseline

### 3.1 Exact source state

- protected branch: `main`
- exact HEAD: `c0854be18d6687b5a51c4977c3268b0e92380ef1`
- tree: `b0e28628d272999c4803a68f981a6566465ccc48`
- source merge: PR #490, `PAS-2 TC-2: frontend runtime transport cutover`
- audit branch was created from that exact HEAD and was `ahead=0 / behind=0` before the audit artifact commit
- repository rulesets returned an empty collection; the repository currently relies on classic branch protection
- the full `/branches/main/protection` endpoint is not readable by this integration (`403`), so live review-count/admin/conversation-resolution details are `UNPROVEN`; the required status-check projection is exposed by the branch endpoint

### 3.2 Open parallel work

At re-baseline the only open PR was draft PR #494, `CAP-01: single Capability Policy owner`, on branch `issue-492-capability-policy-owner`. CAP-05 does not modify or reuse that branch.

### 3.3 Protected required contexts

The exact protected `main` projection contains 23 required contexts, all bound to the GitHub Actions app:

| Required context | Producing workflow |
| --- | --- |
| Certification Linux And WASM | `certification-gate.yml` |
| Certification Windows | `certification-gate.yml` |
| Cloudflare Worker Release Build | `quality-gate.yml` |
| D1 Catalog Migrations | `quality-gate.yml` |
| Encrypted Generation Linux And WASM | `encrypted-generation-gate.yml` |
| Encrypted Generation Windows | `encrypted-generation-gate.yml` |
| External Evidence Metadata | `external-evidence-gate.yml` |
| External Readiness Projection | `external-readiness-gate.yml` |
| External Review Attestations | `external-review-attestation-gate.yml` |
| GitHub Governance Contract | `github-governance-gate.yml` |
| Invariants And Fail-Closed Boundaries | `repository-quality-audit-gate.yml` |
| Local Profile Linux | `local-profile-gate.yml` |
| Local Profile Windows | `local-profile-gate.yml` |
| React Operator UI | `frontend-gate.yml` |
| Registry Domain D1 Adapter Worker And Contract | `profile-generation-gate.yml` |
| Repository-Local Standalone Flow | `cross-component-acceptance-gate.yml` |
| Resolver D1 first-bootstrap implementation | `resolver-d1-first-bootstrap.yml` |
| Runtime Bundle Linux | `runtime-bundle-gate.yml` |
| Runtime Bundle Windows | `runtime-bundle-gate.yml` |
| Rust Linux and WASM | `quality-gate.yml` |
| Rust Windows And Profile Bridge Artifact | `quality-gate.yml` |
| Real Camoufox cold-launch proof | `camoufox-runtime-gate.yml` |
| Profile Bridge Windows regression | `camoufox-runtime-gate.yml` |

No stale required-context name without a producing current workflow/job was observed. On open PR #494, the current protected context names appeared as actual check runs. Protected workflows use unfiltered `pull_request` triggers; this audit found no protected context made bypassable by a `paths` filter. The path-filtered AR-11 FC-6 transport workflow is not a protected required context.

### 3.4 Exact-main check state

For `main@c0854be...`:

- 19 workflow runs were associated with the exact source SHA in the available run query;
- 33 check runs were associated with the commit;
- no `failure` or `cancelled` conclusion was observed in that exact-main check-run snapshot;
- post-main hosted checks included successful `GitHub Governance Hosted State` and `Operational Credential Hosted State`.

This is a point-in-time acceptance observation, not a historical reliability statistic.

## 4. Complete workflow classification snapshot

The table below classifies every tracked `.github/workflows/*.yml` file on the audited tree. A mixed workflow is explicitly marked instead of inventing one false semantic owner.

| Workflow | Current trigger / major job(s) | Primary purpose | Lifecycle | Minimum safe target execution | Natural checker/owner and audit finding |
| --- | --- | --- | --- | --- | --- |
| `ar11-fc6-operator-transport.yml` | push-main, path-scoped transaction | `TRANSITION_MIGRATION` + `OPERATIONAL_READINESS` | `STAGE_BOUND` | `MAIN_ONLY` | `.github/scripts/ar11-fc6-operator.mjs`; bounded FC-6/AR-11 ceremony. Registry incorrectly calls it permanent. Retire when its ceremony/consumer is closed. |
| `architecture-acceptance-recorder.yml` | merged architecture PR | `INVARIANT_ENFORCEMENT` + post-merge provenance | `STAGE_BOUND` through current architecture program | `MAIN_ONLY` post-merge | acceptance policy + observer + exact Git/check observations. Current from AR-12 onward; retirement after program closeout unless a new explicit consumer exists. |
| `camoufox-runtime-gate.yml` | PR + main; Linux real Camoufox, Windows Profile Bridge | `FUNCTIONAL_CORRECTNESS` + `BUILD_PLATFORM`; secondary transition proof | mixed `PERMANENT` current runtime + completed AR-10 `TRANSITIONAL` | `PR_AFFECTED_CONTEXT` for deterministic current checks; `RELEASE_CANDIDATE` for real cold launch/artifact proof | current runtime tests are legitimate; `check-ar10-runtime-cutover.py`/AR-10 closeout ceremony should be distilled/retired separately. |
| `certification-gate.yml` | PR + main Linux/WASM + Windows | `INVARIANT_ENFORCEMENT`; secondary functional/build | `PERMANENT` | `PR_AFFECTED_CONTEXT` | Step-10 certification checker, negative authority/raw-signal fixtures, Rust/WASM tests. |
| `cross-component-acceptance-gate.yml` | PR + main all-up local flow | `FUNCTIONAL_CORRECTNESS`; secondary operational/security | mixed permanent E2E + historical phase checks | `PR_AFFECTED_CONTEXT` or `RELEASE_CANDIDATE` | broad repository-local standalone flow; overlaps frontend/profile/generation/quality compilation and historical Phase-2 validation. |
| `d1-evolution-gate.yml` | PR + main; Linux native opsctl/Wrangler + Windows portability | `CONTRACT_COMPATIBILITY`; secondary operational/build | `PERMANENT` mechanism | `PR_AFFECTED_CONTEXT`; portability/recovery proof may be `RELEASE_CANDIDATE` | native `opsctl` D1 compatibility/migration/replay/recovery policy. No need to execute globally for unrelated changes. |
| `d1-migration-executor.yml` | reusable/manual, staging environment | `OPERATIONAL_READINESS` + `CONTRACT_COMPATIBILITY` | `STAGE_BOUND` operational mechanism | `MANUAL_PROTECTED_OPERATION` | explicit authorization/environment fence; observe then mutate only approved plan. Correctly separate from ordinary PR execution. |
| `encrypted-generation-gate.yml` | PR + main Linux/WASM + Windows | `INVARIANT_ENFORCEMENT` + functional/build | `PERMANENT` | `PR_AFFECTED_CONTEXT` | Step-9 generation checker, negative key-output fixture, Rust/WASM/platform proof. |
| `external-evidence-gate.yml` | PR + main | `OPERATIONAL_READINESS` + invariant policy | `PERMANENT` while evidence admission is current | `PR_AFFECTED_CONTEXT` | Rust `opsctl-core external_evidence` plus `external_evidence_policy`. This is the richer of the two duplicate evidence/readiness callers. |
| `external-readiness-gate.yml` | PR + main | `OPERATIONAL_READINESS` | `RETIRED` candidate after atomic consolidation | currently PR; target delete duplicate caller | invokes only the same `external_evidence_policy` integration test already invoked by External Evidence. Proven no unique checker path in YAML. |
| `external-review-attestation-gate.yml` | PR + main | `OPERATIONAL_READINESS` + `CONTRACT_COMPATIBILITY` | `PERMANENT` while PR review evidence is an admission input | `PR_ALWAYS` | Python performs raw GitHub observation; typed Rust owns semantic evidence/review validity. This matches the current Python/Rust boundary. |
| `frontend-gate.yml` | PR + main | `CONTRACT_COMPATIBILITY` + `BUILD_PLATFORM` + invariant boundary | `PERMANENT` | `PR_AFFECTED_CONTEXT` | generated OpenAPI/PAS-2 validation, frontend boundary checker, npm typecheck/test/build. |
| `github-governance-gate.yml` | PR + main + schedule + manual | mixed: `INVARIANT_ENFORCEMENT`, `SECURITY_SUPPLY_CHAIN`, `OPERATIONAL_READINESS` | mixed permanent contract + main/scheduled operational jobs | contract `PR_ALWAYS`; hosted/provider observation `MAIN_ONLY` | protected contract is legitimate; same workflow also performs hosted Actions reconciliation and provider credential observation. Effects/execution levels should not be conflated. |
| `local-profile-gate.yml` | PR + main Linux/Windows | `FUNCTIONAL_CORRECTNESS` + `BUILD_PLATFORM` | `PERMANENT` profile mechanism | `PR_AFFECTED_CONTEXT`; delivery artifact at `RELEASE_CANDIDATE` | Windows job also runs broad `opsctl` lifecycle/status and artifact work. This is misplaced scope in a Local Profile context. |
| `mailbox-secret-resolver-release.yml` | push main | `BUILD_PLATFORM` + `CONTRACT_COMPATIBILITY` | `TRANSITIONAL / RETIRE_CANDIDATE` pending consumer proof | `MAIN_ONLY` | standalone resolver release validates bootstrap alignment and builds an independent artifact. Current Release Set v3 also builds resolver; zero-current-consumer proof is still required before deletion. |
| `profile-generation-gate.yml` | PR + main | `FUNCTIONAL_CORRECTNESS` + `CONTRACT_COMPATIBILITY` | `PERMANENT` | `PR_AFFECTED_CONTEXT` | registry domain/D1 adapter/worker/OpenAPI/migration checks. |
| `quality-gate.yml` | PR + main; four protected jobs | **mixed** invariant/build/contract/functional | mixed | split into `PR_ALWAYS`, `PR_AFFECTED_CONTEXT`, `RELEASE_CANDIDATE` | major orchestration aggregator: numerous Python validators, full Rust/opsctl/workspace tests, WASM, D1/Wrangler, frontend/Worker build, Windows artifact. No single natural semantic owner at workflow level. |
| `release-architecture-gate.yml` | PR + main Linux + Windows | `OPERATIONAL_READINESS` + `TRANSITION_MIGRATION` | `STAGE_BOUND` through release/architecture qualification | `PR_AFFECTED_CONTEXT` or `RELEASE_CANDIDATE` | AR-11 release/staging policy and Production-blocked assertions. Registry incorrectly freezes it as permanent AR-11 workflow. |
| `release-set-build.yml` | push main | `BUILD_PLATFORM` + `CONTRACT_COMPATIBILITY` + operational provenance | `PERMANENT` current Release Set v3 boundary | `MAIN_ONLY` / release-candidate build-publish boundary | canonical build-once/finalize/publish owner for current durable Release Set v3. Duplicated component builds elsewhere should converge around this boundary rather than form parallel release authority. |
| `release-set-promotion.yml` | manual, staging | `OPERATIONAL_READINESS` | `STAGE_BOUND` current staging/promotion mechanism | `MANUAL_PROTECTED_OPERATION` | verify target, read-only preflight, explicit plan/environment fence, exact bits and rollback. Correct execution category. |
| `repository-quality-audit-gate.yml` | PR + main; protected invariant job | `INVARIANT_ENFORCEMENT`, but heavily mixed | mixed permanent invariants + historical/stage checks | narrow permanent core `PR_ALWAYS`; other checks affected/release | useful specialized architecture/fail-closed checkers are aggregated with full tests, AR/history and release checks. Shrink caller scope; do not replace specialized semantic owners. |
| `resolver-d1-first-bootstrap.yml` | PR + main | `TRANSITION_MIGRATION` + `CONTRACT_COMPATIBILITY` | `STAGE_BOUND / TRANSITIONAL` | `PR_AFFECTED_CONTEXT` for source; actual fresh proof `RELEASE_CANDIDATE`/rehearsal | authority is explicitly one-time fresh initialization/future ceremony. Checker self-requires workflow markers. Keep until AR-12/AR-13 successor and zero-consumer proof, then delete whole predecessor chain. |
| `runtime-bundle-gate.yml` | PR + main Linux/Windows | `CONTRACT_COMPATIBILITY` + functional/build | mixed permanent bundle + historical Step-7 naming | `PR_AFFECTED_CONTEXT`; Windows artifact at `RELEASE_CANDIDATE` | current bundle contract is real; historical ceremony and repeated Profile Bridge artifact build should be separated. |

## 5. Derived caller/checker graph

This section is a one-time derived snapshot, not a hand-maintained authority. Internal Rust test functions under a single `cargo test` target are treated as nodes owned by that crate/test target rather than as separate CI authorities.

### 5.1 Governance / architecture chain

```text
GitHub Governance Contract
  -> github-governance-gate.yml / contract
  -> actionlint + workflow-security structural checks
  -> check-opsctl-readonly.py and architecture/PF/AR structural validators
  -> github-actions-registry.mjs contract/self-test
  -> objective GitHub/workflow/ownership/supply-chain invariants
```

Current positive: the protected contract exists and is exact-head PR-visible.  
Current negative proof: specialized self-tests/fixtures are present for governance, registry, credential lifecycle, profile security, etc.  
Finding: `github-actions-registry.mjs` is not merely structural; its fixed counts and permanence classifications affect architecture acceptance and therefore compete with current lifecycle authority.

### 5.2 Runtime / profile chain

```text
Real Camoufox cold-launch proof
  -> camoufox-runtime-gate.yml
  -> pinned Camoufox/BrowserForge/Playwright install + real launch tests
  -> current Profile Bridge/Camouhost/runtime contract

Profile Bridge Windows regression
  -> camoufox-runtime-gate.yml
  -> Windows Profile Bridge tests/build

Runtime Bundle Linux/Windows
  -> runtime-bundle-gate.yml
  -> Step-7/runtime-bundle checkers + fake Camouhost + Rust/WASM/platform tests

Local Profile Linux/Windows
  -> local-profile-gate.yml
  -> Step-8/local-profile checker + single-writer negative + profile tests
```

Finding: current runtime obligations coexist with historical AR/Step ceremony and repeated artifact builds.

### 5.3 Generation / certification chain

```text
Encrypted Generation Linux/Windows
  -> encrypted-generation-gate.yml
  -> Step-9 checker + negative fixture + domain/WASM/platform tests

Certification Linux/WASM + Windows
  -> certification-gate.yml
  -> Step-10 checker + negative authority/raw-signal fixtures + Rust/WASM/platform tests
```

These are natural specialized checker families. Running the same objective policy on multiple platforms is not semantic duplication.

### 5.4 Frontend / registry-domain chain

```text
React Operator UI
  -> frontend-gate.yml
  -> feature-boundary/PAS-2/OpenAPI validation
  -> npm typecheck/test/build

Registry Domain D1 Adapter Worker And Contract
  -> profile-generation-gate.yml
  -> domain/worker Rust tests + WASM
  -> registry/OpenAPI/migration structural validators
```

These are current product/contract surfaces; target scope is affected-context, not unconditional global execution.

### 5.5 D1 chain

```text
D1 Catalog Migrations
  -> quality-gate.yml
  -> local/pinned Wrangler migration proof

Native opsctl D1 proof (non-protected job)
  -> d1-evolution-gate.yml
  -> typed D1 compatibility/plan/replay/recovery policy

Protected D1 Migration Executor
  -> d1-migration-executor.yml
  -> observe/plan
  -> explicit staging authorization
  -> bounded remote mutation
```

The read/decision/mutation layering is strong. The main problem is execution scope/cost, not competing D1 semantic owners.

### 5.6 External evidence chain

```text
External Evidence Metadata
  -> external-evidence-gate.yml
  -> opsctl-core::external_evidence
  -> integration test external_evidence_policy

External Readiness Projection
  -> external-readiness-gate.yml
  -> integration test external_evidence_policy   # same checker

External Review Attestations
  -> raw GitHub observation adapters
  -> typed Hosted Evidence / external evidence Rust policy
```

The duplicate is at the caller/context level. Rust remains the semantic owner.

### 5.7 Release chain

```text
release-set-build.yml
  -> exact accepted main
  -> build control-plane/frontend/resolver + Windows Profile Bridge package
  -> Release Set v3 finalize/verify
  -> durable immutable GitHub Release

release-set-promotion.yml
  -> verify immutable target Release Set
  -> read-only provider/staging preflight
  -> explicit protected plan
  -> same-bits promotion / rollback
```

This is the canonical current release boundary. `mailbox-secret-resolver-release.yml` builds an additional standalone resolver artifact and must prove a surviving consumer before it is retained indefinitely.

## 6. Invariant lifecycle findings

| Objective invariant / mechanism | State | Primary owner/checker | Current callers | Conclusion |
| --- | --- | --- | --- | --- |
| Product Runtime must not depend on `opsctl` | `CURRENT` | `scripts/check-opsctl-readonly.py` + Rust dependency boundary | quality/repository-quality and related opsctl validation | Protected; **not missing**. |
| Rust owns external evidence/readiness semantics | `CURRENT` | `opsctl-core` + `external_evidence_policy` | External Evidence, External Readiness, External Review | Semantic owner is singular; caller duplication exists. |
| Real pinned Camoufox path must work | `CURRENT` | runtime/Profile Bridge/Camouhost tests | Camoufox Runtime Gate | Keep objective proof; separate completed AR-10 cutover ceremony. |
| AR-10 cutover/closeout ceremony | `REPLACED` as a transition ceremony | historical AR-10 checker/evidence | still invoked in Camoufox gate | Distill any still-objective rule, then delete transition-only caller/checker/evidence dependencies. |
| Architecture acceptance exact-head/provenance through AR-17 | `CURRENT`, stage-bound | acceptance policy/observer/recorder | post-merge recorder | Keep through current architecture program; exact retirement after closeout unless successor explicitly adopts it. |
| Fixed “23 active / 21 permanent workflows” and named AR-10/AR-11 permanence | `REPLACED` by current program rule | `github-actions-registry.mjs` + JSON registry | governance + architecture acceptance | **High-priority contradiction:** still executable although current authority forbids timeless workflow counts/names. |
| Resolver fresh-D1 first initialization | `CURRENT` but stage-bound | resolver bootstrap script/checker/authority | required first-bootstrap context + release alignment | Not safe to retire until successor/zero-consumer proof; then delete atomically. |
| AR-11 release/staging architecture ceremony | `CURRENT` only as remaining stage obligations; not timeless | release-architecture checker/opsctl release policy | release-architecture workflow | `STAGE_BOUND`, not permanent. |
| FC-6 operator transaction transport | `CURRENT` only for bounded ceremony | AR-11 FC-6 operator script | main path-filter transport workflow | `STAGE_BOUND`; retire at ceremony closure. |
| Current Release Set v3 immutable build/promotion boundary | `CURRENT` | Release Set build/verify/promotion policy | release-set build + promotion | Keep as canonical release boundary. |
| Standalone resolver release artifact | `CANDIDATE REPLACED` | resolver release script | main resolver-release workflow | Duplicate current-build surface; zero-consumer proof is `UNPROVEN`, so no deletion authorized yet. |
| Historical Phase/Step-named validator ceremony | mixed | specialized existing checkers | mega quality/standalone/audit workflows | Do not delete objective rules blindly. Retire historical ceremony only after mapping each surviving objective invariant to its natural checker. |

## 7. Competing / duplicate implementations and callers

### 7.1 Proven semantic-authority conflict — HIGH

`architecture/github-actions-registry.json` and `.github/scripts/github-actions-registry.mjs` classify workflow lifecycle as a fixed canonical set. The script requires exactly 23 active registrations, exactly 21 permanent workflows, and asserts specific AR-10/AR-11 names remain permanent. Architecture acceptance then consumes the `PERMANENT_REQUIRED` category.

The current program authority says the opposite:

```text
Historical workflow counts/names/SHAs are observations, never timeless constants.
new hand-maintained global authority catalog = 0
legacy predecessor retained only for internal CI/docs/self-test = 0
```

Therefore this is a real executable governance contradiction, not style debt.

### 7.2 Proven duplicate protected caller — MEDIUM

`external-readiness-gate.yml` has one semantic test command and it is already executed by `external-evidence-gate.yml`:

```text
cargo test --locked --manifest-path tools/opsctl/Cargo.toml --test external_evidence_policy
```

No unique readiness checker path was found in that workflow. One protected context can be retired after an atomic branch-protection/workflow transaction while preserving the Rust negative fixtures and evidence semantics.

### 7.3 Proven duplicate orchestration — MEDIUM

`quality-gate.yml`, `repository-quality-audit-gate.yml`, and `cross-component-acceptance-gate.yml` repeat substantial portions of:

- workspace/opsctl compilation and tests;
- WASM checks;
- architecture/phase validators;
- frontend install/typecheck/test/build;
- Profile Bridge/runtime artifact work.

This does **not** prove multiple semantic owners. The target is to preserve specialized checkers and consolidate/scope their callers.

### 7.4 Release/component rebuild duplication — MEDIUM

Worker, resolver, and Profile Bridge artifacts are built in more than one workflow. Release Set v3 is the current durable release owner. Other rebuilds are valid only when they prove a distinct PR/platform property; they should not become parallel release identity authorities.

## 8. Orphaned / one-shot / predecessor estate

### Proven

- no protected required context without a current producing job was observed;
- no protected workflow uses a `paths` filter that could make its required context disappear;
- the AR-11 FC-6 transport path filter is on a non-protected, main-only ceremony workflow;
- fixed historical workflow lifecycle assertions remain executable after their originating AR phases.

### Retirement candidates requiring bounded proof

1. `external-readiness-gate.yml` and its required context after consolidation — duplicate checker path is already proven.
2. AR-10 transition/closeout invocations inside `camoufox-runtime-gate.yml` — objective current runtime proof must remain.
3. `mailbox-secret-resolver-release.yml` — delete only after current/external Release Set consumer count is proven zero.
4. resolver first-bootstrap authority/checker/workflow/context — delete only after AR-12/AR-13 successor and zero compatibility/external consumer obligation are proven.
5. AR-11/FC-6 transport and release-architecture ceremony — retire at explicit stage closure, not because a newer phase exists.

### Unreferenced scripts / fixtures

An exhaustive repository-wide “all unreferenced files” conclusion is `UNPROVEN` because the available GitHub code-search index returned incomplete results even for known referenced paths. No file is classified `RETIRED` solely from absence in that incomplete search. Retirement transactions must re-run caller search against an exact fresh tree and prove zero callers before deletion.

## 9. Performance / reliability evidence

Historical p95, queue-time distribution, rerun frequency, and flaky-failure frequency are `UNPROVEN` in this audit. The available connector exposes exact check-run timestamps but not a reliable bounded historical aggregation suitable for a p95 claim. No value is inferred.

### 9.1 Proven point sample — open PR #494

PR #494 produced 29 check runs. Observed job wall-clock durations from check-run `started_at`/`completed_at` include:

| Check | Point duration | Result in sample |
| --- | ---: | --- |
| Rust Windows And Profile Bridge Artifact | 141 s | success |
| Repository-Local Standalone Flow | 136 s | success |
| Native opsctl and pinned Wrangler D1 proof | 120 s | success |
| Local Profile Windows | 118 s | success |
| Real Camoufox cold-launch proof | 109 s | success |
| Release policy Windows regression | 109 s | success |
| Rust Linux and WASM | 96 s | failure |
| Native opsctl D1 CLI portability on Windows | 70 s | success |
| Invariants And Fail-Closed Boundaries | 70 s | failure |
| Profile Bridge Windows regression | 66 s | success |
| GitHub Governance Contract | 59 s | success |
| Cloudflare Worker Release Build | 50 s | failure |
| Runtime Bundle Windows | 40 s | success |
| Resolver D1 first-bootstrap implementation | 38 s | success |
| Encrypted Generation Windows | 38 s | success |
| Certification Windows | 37 s | success |
| D1 Catalog Migrations | 33 s | success |
| External Review Attestations | 32 s | success |
| External Readiness Projection | 27 s | success |
| External Evidence Metadata | 26 s | success |
| React Operator UI | 24 s | success |
| Local Profile Linux | 19 s | failure |
| Encrypted Generation Linux And WASM | 18 s | failure |
| Runtime Bundle Linux | 18 s | success |
| Registry Domain D1 Adapter Worker And Contract | 16 s | failure |
| Certification Linux And WASM | 16 s | failure |
| Release capability and promotion policy | 12 s | failure |

The failures are **not** classified flaky: PR #494 intentionally changes CAP-01 semantic ownership and several gates legitimately rejected that intermediate candidate. This sample proves cost/concurrency shape only.

### 9.2 Critical-path conclusion

For this sample, multiple independent jobs exceeded ~100 seconds. The point critical path was at least 141 seconds after job start, before any queue delay. The main cost signals are repeated checkout/setup/compilation, Windows artifact builds, all-up standalone testing, and real browser installation/cold launch.

Queue/setup split, cache hit ratio, p95, and failure/rerun frequency remain `UNPROVEN` and must be measured from a sufficiently large GitHub Actions history in a later implementation/measurement transaction if that data becomes available.

## 10. Security / effects audit observations

Positive observations:

- protected PR workflows generally use `contents: read` and pinned checkout/action identities;
- secret/provider-capable hosted jobs in `github-governance-gate.yml` are excluded from ordinary PR execution;
- D1 migration and release promotion mutations are behind explicit manual/protected environment/authorization fences;
- current provider observation and typed policy are separated: workflow/Python/Node acquire facts, Rust owns semantic verdicts where the current architecture requires it;
- no production mutation is authorized or performed by this audit.

Finding:

- `github-governance-gate.yml` contains three materially different execution/effect classes in one workflow: protected PR contract, main/scheduled hosted governance reconciliation with Actions write capability, and main/scheduled provider credential observation. They are logically separable even though the protected context itself is read-only. Any future modification should preserve effect isolation and make execution level obvious.

## 11. Missing objective invariants

This audit does **not** prove a missing product/security invariant that merits a new permanent checker.

Specifically rejected as “missing”:

- `Product Runtime does not depend on opsctl` — already executable in `check-opsctl-readonly.py`;
- external evidence semantic ownership — already Rust-owned with executable negative paths;
- generic “every check must have lifecycle metadata” — useful governance discipline, but adding a global lifecycle registry/checker would recreate the problem #496 is meant to remove.

Two topology risks need correction without inventing new semantic authorities:

1. architecture acceptance must stop treating a hand-maintained fixed workflow-count/name registry as permanent semantic truth;
2. release/platform proofs should increasingly consume one canonical candidate artifact identity instead of treating independent rebuilds as equivalent release evidence where exact-byte identity matters.

The second is a release-topology concern to validate against the current Release Set consumer graph before any new checker is proposed.

## 12. Recommended target CI topology

```text
PR_ALWAYS
  -> small architecture/security/ownership/supply-chain invariants
  -> PR-specific external review attestation when admission-critical

PR_AFFECTED_CONTEXT
  -> capability/domain unit + integration + contract checks
  -> frontend/OpenAPI
  -> D1 evolution when D1/schema/opsctl surfaces change
  -> profile/generation/runtime/certification affected surfaces

MAIN_ONLY
  -> canonical Release Set v3 build/finalize/publish
  -> post-merge architecture acceptance provenance while AR program is active
  -> live GitHub governance/provider observation that cannot safely run on PRs

RELEASE_CANDIDATE
  -> real Camoufox cold launch
  -> Windows/Profile Bridge delivery equivalence
  -> expensive all-up/recovery/platform proofs
  -> candidate artifact identity / same-bits verification

PRODUCTION_ADMISSION
  -> later read-only provider/account/credential/recovery/readiness proof
  -> fail closed before credential exposure or mutation

MANUAL_PROTECTED_OPERATION
  -> staging D1 mutation
  -> staging promotion/rollback
  -> future production mutation only after separate authorization
```

Principles:

- protected admission should prove objective invariants, not historical phase names;
- one specialized semantic checker may have multiple platform callers without becoming duplicate policy;
- expensive external/platform proof runs at the lowest level that still fails closed safely;
- path/affected-context optimization is allowed only where a separate always-on guard makes bypass impossible, or where GitHub required-context semantics remain satisfiable;
- no new global registry or generic CI framework;
- every transitional mechanism must have an exact deletion condition in its bounded transaction.

## 13. Minimal bounded implementation transactions

These are recommendations only. #496 authorizes none of the implementation mutations below.

### CAP05-T1 — remove fixed Actions lifecycle/count semantic authority

**Concern:** architecture acceptance and Actions registry lifecycle ownership.  
**Current predecessor:** `architecture/github-actions-registry.json` permanence categories + `EXPECTED_ACTIVE/EXPECTED_PERMANENT` and named historical permanence in `.github/scripts/github-actions-registry.mjs`.  
**Target:** branch protection required contexts remain the PR admission authority; workflow existence/registration validation is derived from the tracked/live workflow surface without encoding timeless counts/phase permanence. Non-protected stage workflows are owned by their bounded stage contracts, not a global registry.  
**Deletion scope:** hard-coded counts, hard-coded historical permanence assertions, acceptance dependency on `PERMANENT_REQUIRED`, and the JSON lifecycle catalog if no non-semantic consumer remains.  
**Acceptance:** exact current required contexts still fail closed; duplicate/unexpected live workflow registrations remain structurally detectable; no replacement global registry is introduced.

### CAP05-T2 — consolidate duplicate external readiness caller/context

**Concern:** duplicate protected evidence/readiness caller.  
**Natural owner:** Rust external evidence/readiness policy.  
**Target:** one protected context invokes the complete current Rust checker/negative path.  
**Deletion scope:** `external-readiness-gate.yml`, `External Readiness Projection` required context, and any docs/current callers whose only purpose is that duplicate context.  
**Acceptance:** malformed/stale/wrong-account/readiness negative fixtures still fail through the same Rust policy path; branch protection remains exact and satisfiable throughout the transaction.

### CAP05-T3 — separate current Camoufox proof from AR-10 transition ceremony

**Concern:** permanent runtime obligation mixed with completed cutover ceremony.  
**Keep:** pinned real runtime, IPC/Profile Bridge, single-writer/current compatibility proofs.  
**Delete after mapping:** AR-10-only cutover/closeout checker calls and predecessor evidence dependencies with zero unique current invariant.  
**Execution:** deterministic current runtime checks affected-context; real cold-launch/platform proof release-candidate unless a bounded risk analysis proves PR execution necessary.

### CAP05-T4 — decouple Local Profile from opsctl/release lifecycle

**Concern:** profile-specific context owns unrelated lifecycle/opsctl verification.  
**Keep:** Local Profile Linux/Windows functional/profile checker.  
**Move:** architecture lifecycle/status proof to governance/release owner; delivery artifact proof to release-candidate owner.  
**Deletion:** duplicate opsctl/all-workspace lifecycle calls from Local Profile job after their natural caller is proven.

### CAP05-T5 — shrink mega PR orchestration without changing semantic owners

**Concern:** `quality-gate`, `repository-quality-audit-gate`, `cross-component-acceptance-gate` duplicate setup/build/test work.  
**Target:** one small always-on invariant lane plus affected-context capability lanes and a later all-up/release-candidate lane.  
**Constraint:** do not merge specialized Rust/Python/MJS semantic checkers merely for cosmetic consolidation. Reuse/caching/orchestration may be consolidated; policy owner remains natural specialized checker.  
**Deletion:** duplicate workspace/npm/WASM/artifact invocations only after each protected invariant has one surviving caller.

### CAP05-T6 — canonical release/component build convergence

**Concern:** multiple independent component artifact builds around a Release Set v3 canonical boundary.  
**Target:** Release Set build is the sole durable release identity owner; PR/platform jobs prove source/platform correctness without creating competing release identities.  
**Precondition:** exact consumer graph for standalone resolver and Profile Bridge artifacts.  
**Deletion candidate:** `mailbox-secret-resolver-release.yml` and predecessor artifact path only if current/external/persisted consumers = 0.

### CAP05-T7 — first-bootstrap lifecycle closure after successor proof

**Concern:** one-time fresh-D1 mechanism is self-preserved as permanent required context.  
**Do not execute yet.**  
**Retirement condition:** AR-12/AR-13-approved fresh-environment/migration successor verified; current product/security obligation for predecessor = 0; external/persisted consumer = 0; same release/migration compatibility proof survives.  
**Deletion scope:** first-bootstrap authority/implementation projection, bootstrap script if replaced, checker, negative fixtures, workflow, required context, and docs/current callers in one bounded retirement transaction.

### CAP05-T8 — stage-bound AR-11/FC-6 workflow retirement

**Concern:** program-stage workflows classified timelessly permanent.  
**Target:** explicit stage-bound owner and retirement condition for `release-architecture-gate.yml` and `ar11-fc6-operator-transport.yml`.  
**Retirement:** only when their current FC/release/program consumer has closed and no AR-12..AR-17 obligation remains.  
**Deletion:** workflow/checker/fixture/current docs together; historical evidence remains historical provenance only.

## 14. Blocking assessment

| Finding | FC-6 | Release | Production admission |
| --- | --- | --- | --- |
| Fixed workflow-count/name registry contradicts current program authority | **Not a new FC-6 blocker by current authority**; must be resolved before using that registry as a future lifecycle/admission authority | governance debt; implementation transaction should precede CI topology changes | Production is already blocked by AR-17; no separate production authorization is created |
| External Evidence/Readiness duplicate context | non-blocking improvement | non-blocking if one complete checker survives | non-blocking if admission semantics remain in surviving Rust checker |
| AR-10 ceremony mixed with current Camoufox proof | non-blocking while current proof remains green | cleanup before/with release-CI optimization | current PAS-4/AR-15 obligations still require real runtime proof |
| Mega-gate duplicate orchestration/cost | non-blocking | performance/maintainability improvement | no independent production effect |
| Local Profile contains unrelated opsctl lifecycle work | non-blocking | maintainability/scope improvement | no independent production effect |
| Standalone resolver release predecessor candidate | non-blocking until zero-consumer proof | deletion blocked on consumer proof | no independent production effect |
| First-bootstrap lifecycle is stage-bound, not permanent | keep until successor/zero-consumer proof | may be required for fresh/rehearsal path | no production deletion before successor proof |

CAP-05 research does not itself amend the current program prerequisite chain. Production remains blocked for the existing AR-17/Production Core reasons in the current architecture authority; this audit does not create or remove production authorization.

## 15. Definition-of-Done assessment for #496

- all 23 workflow files classified: **YES**
- all 23 protected contexts mapped to producing workflow/job family: **YES**
- invoked checker/test families have owner/purpose: **YES**, at CI target/family granularity; individual Rust test functions remain internal to their owning cargo target
- current invariants linked to real checker/caller or finding: **YES**
- transitional checks have retirement condition or finding: **YES**
- competing/duplicate mechanisms identified: **YES**
- orphan/one-shot/predecessor candidates identified without guessing zero-consumer state: **YES**
- missing-invariant review performed without creating a checker-for-checker: **YES**
- point duration/critical-path evidence recorded: **YES**
- historical p95/flakiness/rerun/queue metrics: **UNPROVEN**, explicitly not guessed
- target topology proposed: **YES**
- bounded implementation transactions with deletion scope: **YES**
- FC-6/release/production blocking classification: **YES**
- workflow/checker/governance/provider/staging/production mutation performed by audit: **NO**

## 16. Research stop rule

This document closes the read-only research scope only. It does not authorize deleting, reclassifying, path-filtering, merging, dispatching, mutating protected settings, running FC-6, or touching staging/production/provider state.

Before any implementation transaction:

```text
fresh main + branch/protection/checks
-> exact concern + natural owner
-> exact current callers/consumers
-> positive + negative acceptance
-> atomic required-context/workflow/checker cutover
-> predecessor deletion in same bounded concern when retirement is proven
-> exact-head CI / required checks
-> guarded merge
```
