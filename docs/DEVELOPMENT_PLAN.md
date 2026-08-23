# Browser Profile Platform — Development Plan

**Document status:** GENERATED_PROJECTION  
**Canonical program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Live execution tracker:** #441  
**Functional Closure umbrella:** #399  
**Production Core gate:** `BLOCKED`  
**Production readiness:** `false`

This document is the compact developer-facing execution map. It is not a second roadmap or semantic authority. When moving state differs, protected `main` + the canonical plan + #441 win. Exact moving SHAs belong in the live tracker, not this tracked projection.

## 1. Current snapshot

```text
F1/F2  ACCEPTED
N1     ACCEPTED
#454   NEXT — sole pre-N2 implementation blocker
N2     BLOCKED on #454
N3     BLOCKED on N2
N4     BLOCKED on N3
N5     BLOCKED on N4
PF-1   BLOCKED on N5 + fresh #430 entry reread
AR-12  NOT STARTED
production_mutation = false
```

`source_present != production_enabled` remains binding.

## 2. Remaining work before N2

There is exactly one implementation transaction before N2: **#454**.

F1 already made Release Set v3 the current writer/model. #454 answers whether executable historical-v2 compatibility has a real current consumer or durable obligation.

### 2.1 Discover the real current consumer state

On a fresh exact baseline, inspect:

- `architecture/release-set-v2.json` callers/references;
- historical-v2 decoder/verification callers;
- promotion, rollback, deployment snapshot and expected-current paths;
- actual current staging Release Set identity;
- actual known-good rollback Release Set identity;
- any explicit current #399/#421/FC-6 durable v2 input;
- current GitHub workflows/releases/evidence that are execution inputs rather than provenance.

Repository search alone is insufficient because staging/known-good and hosted GitHub state are external observations.

### 2.2 Make one bounded decision

```text
real current v2 consumer/durable obligation exists
-> name exact consumer + identity/version
-> keep minimum isolated historical-v2 read/verify compatibility only
-> current writer/model remains v3-only
-> no v2 -> v3 semantic coercion
-> explicit retirement condition

OR

real current v2 consumer/durable obligation = NONE
-> retire executable v2 compatibility/current-v2 authority
-> remove compatibility-only code/workflows/tests after zero-caller/invariant proof
-> preserve history in Git/releases/evidence only
```

Historical acceptance, hypothetical future use and “tests already exist” are not compatibility requirements.

### 2.3 #454 acceptance / N2 entry

One unchanged exact candidate head must prove:

```text
current Release Set writer/model = v3
architecture/release-set-v2.json current semantic authority = 0
historical v2 current consumer = NAMED_EXACTLY OR NONE
historical v2 executable compatibility = JUSTIFIED_AND_ISOLATED OR RETIRED
current competing v2 semantic owner = 0
v2 -> v3 semantic coercion = 0
old_current_callers = 0 for every deleted predecessor
old_unique_current_invariants = 0 for every deleted predecessor
positive + negative/fail-closed proof = green
all applicable permanent workflows = green
live protected required contexts = green
behind_by = 0
blocking reviews = 0
unresolved review threads = 0
production_mutation = false
```

Only accepted #454 protected `main` may become the N2 base.

#399 and #421 do **not** add another pre-N2 phase. Their FC-6 obligations are preserved through normalization and are freshly re-baselined only after PF-3.

## 3. Current implementation order

```text
F1/F2 ACCEPTED
-> N1 ACCEPTED
-> #454 NEXT
-> N2 Python-estate authority retirement
-> N3 current GitHub-governance normalization
-> N4 operator/provenance authority cleanup
-> N5 runtime semantic-authority retirement
-> PF-1 lifecycle + bounded inventory cutover
-> PF-2 Hosted Operational Evidence
-> PF-3 Architecture Fitness + architecture-forming freeze
-> fresh #399/#421 re-baseline
-> FC-6 real staging same-bits / rollback proof
-> FC-7 final AR-11 functional audit
-> AR-12
-> AR-13
-> AR-14
-> AR-15 Windows delivery/updater
-> AR-16 final whole-project audit only
-> AR-17 qualification / Production Core gate decision only
-> PC-1 Production Core v1
```

F1/F2/N1…N5 are bounded foundation/normalization transactions, not new lifecycle slices. Do not invent F3/N1.5/PF-0 or another architecture program for a bounded defect.

## 4. Efficient execution rules

Quality comes from strong invariants that are cheap to re-check, not from phase proliferation.

For #454 and N2–N5:

```text
fresh protected main
-> discover real callers/consumers + natural owner
-> preserve still-valid behavior/security invariants
-> switch current callers
-> old callers = 0
-> old unique current invariants = 0
-> delete/demote DEAD predecessor
-> targeted fast/local proof
-> one unchanged exact head through permanent CI/governance
-> guarded merge under current live governance
-> accepted-main reread
```

Rules:

- prefer small reviewable PRs, but do not turn every PR into a new program phase;
- do not create a compatibility bridge without a named current consumer/durable obligation;
- do not replace retired JSON/Python/Node authorities with equivalent Rust/JSON/YAML/TOML registries;
- do not create a new plan document when an existing canonical owner or tracker can hold the requirement;
- use `python scripts/verify-fast.py` and targeted tests before expensive permanent CI; full CI is acceptance proof, not an interactive formatter/compiler loop;
- preserve exact-head, fail-closed, zero-caller and zero-unique-invariant proofs.

## 5. Current development environment

The current interactive development environment has the connected **GitHub plugin** and does not assume that a local `gh` CLI exists.

For hosted GitHub operations use the plugin directly: repository/branch reads, PRs, issues, reviews, required contexts, workflow/status observations, comments and repository-file/branch/PR mutations. Do not block work on missing `gh` and do not shell-scrape GitHub as a substitute.

Repository-local verification stays local. This tool choice does **not** change product architecture: `opsctl` remains offline and must not gain GitHub/network/provider authority. GitHub Actions and provider workflows may use their own pinned hosted tooling.

If an older runbook prescribes `gh`, treat the command as an intent to query/mutate GitHub and use the equivalent connected GitHub-plugin action in this environment; update the runbook when it is otherwise touched.

## 6. N2 target

N2 starts only after accepted #454.

N2 must:

- retire the AR-6/AR-10/AR-11 per-file Python-estate chain as current authority;
- govern Python by source-derived role/effects, with no successor 1:1 file registry;
- remove AR-6/Python-estate sentinels from `opsctl doctor` and repository-root detection;
- keep `runtime/camouhost/real.py` as the real Camoufox adapter behind Profile Bridge + versioned IPC + `runtime-lock.json`;
- keep `runtime/camouhost/main.py` synthetic/test-only;
- prove retired direct Python profile/browser executables remain absent;
- replace bespoke high-risk provider mutation helpers with pinned official provider tooling where parity is proved;
- hand unique facts discovered in old validators/generators to their natural N4/N5/PF owners instead of creating a successor registry.

Detailed N2 DoD remains in `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md` and #441.

## 7. Later architecture gates

- **N3:** one current desired GitHub-governance configuration + live observation + typed evaluation; historical overlay reconstruction retires.
- **N4:** typed Rust `CommandRegistry`/effect ownership becomes authoritative; operator JSON cannot authorize Rust behavior.
- **N5:** runtime semantics return to Product Rust, `runtime-lock.json`, Bridge/IPC contracts, real Camouhost adapter and tests; `runtime-cutover-ar10.json` retires when dead.
- **PF-1:** typed lifecycle evaluator + bounded owner projections; no global authority bag; legacy Node/Python lifecycle/inventory predecessors retire.
- **PF-2:** hosted observation -> strict DTO -> typed Rust evidence policy -> immutable artifact/attestation.
- **PF-3:** machine-enforced fitness + architecture-forming freeze. After PF-3, ordinary FC/AR/PC work may not invent a new generic architecture framework.

## 8. Production roadmap

PC-1 is the first production release and remains bounded to the Production Core: identity/users, clients/customer cards, browser profiles and bulk profile operations, client↔profile bindings, real Camoufox runtime, Windows Profile Bridge, AR-15 updater/delivery, profile persistence/restore, grants/access, audit, health/readiness/observability and required recovery/notification foundations.

Mailbox administration, mailbox jobs/automation and outbound mail may remain source-present and tested on the same `main` while `production_enabled=false`; later PC profiles enable them through the same Release / Capability Profile authority.

```text
PC-1 Production Core v1
PC-2 Mailbox Administration
PC-3 Mailbox Jobs / Automation
PC-4 Outbound / later capabilities
```

## 9. Canonical references

Use these instead of duplicating their detail here:

- `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` — current program authority;
- `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md` — permanent architecture rules;
- `docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md` — detailed #454/N2–N5 ownership/retirement contract;
- #441 — live mutable execution tracker and exact current baseline;
- #454 — sole current pre-N2 implementation transaction;
- `docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md` / #430 — PF-1;
- `docs/PF3_ARCHITECTURE_FITNESS_BASELINE.md` / #431 — PF-3;
- #399 / #421 — Functional Closure obligations and later FC-6 re-baseline;
- `architecture/accepted-phases.json` + Git history — immutable accepted product-phase provenance.
