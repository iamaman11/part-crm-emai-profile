# Post-AR-11 / Pre-AR-12 Hardening Execution Plan

**Document status:** SUBORDINATE_EXECUTION_PLAN  
**Tracking issue:** #375  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Lifecycle authority:** `architecture/architecture-acceptance-policy.json` + `architecture/architecture-program-sequence.json` + immutable acceptance Git metadata  
**Scope:** post-AR-11 cleanup and architectural hardening only  
**AR-12 implementation:** FORBIDDEN until this plan's Definition of Done is satisfied  
**Production mutation:** FORBIDDEN

This document is an execution checklist for issue #375. It does not create a second lifecycle authority, does not record acceptance, and must never be used to decide the accepted/current AR slice independently of the generic Git-derived acceptance mechanism.

## 1. Objective and non-negotiable invariants

The application remains one modular product with one protected `main`, one architecture and one data/schema history. Capabilities may be implemented continuously and enabled independently:

```text
source_present != production_enabled
```

Post-AR-11 cleanup is not application-functionality cleanup. Do not delete source from `apps/**`, `crates/**`, `frontend/**`, `runtime/**`, `migrations/**` or `contracts/**` merely because a capability is currently production-disabled, belongs to mailbox functionality, was implemented before the current activation model, or is not part of Production Core v1.

Until AR-17 acceptance, preserve mechanically:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

AR-12 implementation must not begin during this work. No production provisioning, production promotion, production secret mutation, provider mutation from `opsctl`, capability enablement, or GitHub Environment approval bypass is permitted.

## 2. Live baseline to re-read before every bounded unit

At the start of each unit, re-read live GitHub state rather than trusting this snapshot. At plan creation time:

- protected `main`: `970c97be0a82f6cd06706a58ba8a9df590dc2604`;
- accepted AR-11: yes, represented by the generic acceptance migration bootstrap in `architecture/architecture-acceptance-policy.json`;
- derived current slice: AR-12;
- AR-12 implementation: not started;
- tracking issue #375: open;
- observed protected required contexts: 23;
- historical expected applicable permanent workflows: 17, but this number is not a permanent constant and must be recomputed from the live registry;
- abandoned/prototype branch `agent/post-ar11-projection-opsctl-normalization`: ahead of this baseline and useful only as evidence/prototype; do not merge it wholesale.

Before modifying source, compare the live base with any prototype branch and classify each candidate change as: port unchanged, port with redesign, superseded, or reject.

## 3. Historical executable safety model

Every suspicious historical executable must be classified before removal:

```text
CURRENT_INVARIANT
UPGRADE_ROLLBACK_REQUIRED
TRANSITION_PROVENANCE_ONLY
DEAD
```

Only `DEAD` is removable.

If a historical executable contains a unique current invariant:

```text
port invariant -> prove parity -> switch caller -> classify predecessor DEAD -> remove predecessor
```

Never restore/materialize retired executable code from Git history merely to execute it. Static Git archaeology (`git show`, `git cat-file`, `git merge-base`, `git rev-parse`) is evidence-only and permitted. Historical names such as `AR8`, `AR10`, `pre2j` or `phase2i` are not proof of deadness.

Continue to enforce `architecture/historical-executable-debt.json` and `scripts/check-historical-executable-debt.py`. Unknown Python executables must fail closed under the canonical Python estate.

## 4. Bounded execution sequence

The default implementation sequence is Unit A through Unit F. Boundaries may be refined only when live evidence proves a cleaner split. Do not combine unrelated lifecycle, `opsctl`, inventory, runtime and naming changes in one large PR.

### Unit A — Lifecycle projection authority finalization

Goal: make Git-derived lifecycle semantics unambiguous and remove tracked snapshot ownership of accepted/current AR state without introducing a second derivation algorithm.

Tasks:

1. Introduce/finalize `architecture/lifecycle-projection-policy.json` as a policy describing projections, not a new acceptance state store.
2. Bind it from `architecture/architecture-acceptance-policy.json`.
3. Define `docs/status.json`, `architecture/inventory.json` and `architecture/architecture-rebaseline-v3-transition.json` as compatibility/human projections only.
4. Ensure no tracked field such as `current_slice`, `accepted_checkpoint`, `accepted_architecture_slices`, `next_slice` or `current_delivery_map` can decide acceptance/current-slice state.
5. Remove AR-8/AR-10/AR-11 lifecycle mutation logic and monkey-patching from the inventory generation path.
6. Prefer one reusable lifecycle semantic authority. `opsctl` must not invent a divergent Git acceptance parser.
7. Replace AR-8-named lifecycle compatibility executables with a neutral current validator only after exact invariant mapping and parity proof.
8. Correct stale current documentation to state AR-11 accepted and AR-12 Git-derived/current while explicitly marking documentation as non-authoritative lifecycle projection.

Required checks:

- generic acceptance derivation still reports AR-11 accepted and AR-12 current;
- missing/conflicting/non-contiguous acceptance evidence fails closed;
- no second source commit is required to project future AR acceptance;
- `architecture_complete=false`, `production_core_gate=BLOCKED`, `production_ready=false`, `production_mutation=false` remain unchanged;
- inventory write/check is deterministic and does not advance lifecycle state;
- documentation checker rejects a projection that attempts to become acceptance authority;
- full repository/Python estate gates remain green.

### Unit B — Canonical `opsctl` role and exact command registry

Goal: define `opsctl` as the repository-local Operational Policy & Decision Control Plane and mechanically register the implementation surface.

Target role:

```text
inspect -> classify -> plan -> preflight -> verify -> explain
```

Never:

```text
provision -> deploy -> provider mutate -> secret readback -> database mutate -> customer-state mutate
```

Tasks:

1. Evolve `architecture/operator-contract.json` rather than creating a competing registry unless a separate implementation-architecture contract is demonstrably necessary.
2. Register every command family with namespace, actions, mode, authority/source, inputs, output type, side effects, network authority, provider mutation authority, secret authority, activation owner and ACTIVE/RESERVED status.
3. Register the actual active surface: `doctor`, `status`, `inventory`, `credentials status`, `credentials rotation-plan`, D1 status/plan/compatibility/verify, release inspect/verify/compatibility and promotion plan/preflight/verify.
4. Mark recovery as RESERVED for AR-14 and readiness as RESERVED for AR-16; they must have no executable surface before their owning slice.
5. Remove obsolete flat spellings from current authority.
6. Add a permanent fail-closed gate enforcing actual Rust CLI surface == canonical registered active surface and rejecting premature reserved commands.

Permanent safety assertions:

```text
provider_mutation = false
network_authority = false
secret_readback = false
production_mutation = false
customer_state_mutation = false
database_mutation = false
deployment_mutation = false
production_child_process_spawn_sites = 0
```

Explicitly reject production `Command::new("wrangler")`, `Command::new("node")`, `Command::new("npx")`, provider HTTP clients, GitHub mutation clients, D1 mutation clients and secret retrieval.

Required checks:

- authority->implementation and implementation->authority exact-surface parity;
- negative tests for unknown/unowned commands and premature reserved namespaces;
- source scan for child-process/network/provider mutation authority;
- operator contract contains current namespaced spellings only;
- existing command semantics remain behaviorally unchanged.

### Unit C — `opsctl` internal modularity

Goal: make the operator tool understandable to a new developer in minutes and scalable through AR-16 without a central god-parser or mixed responsibility modules.

Tasks:

1. Separate repository IO, lifecycle policy consumption and operator presentation.
2. Keep `lib.rs` as composition/dispatch root, not business/policy implementation warehouse.
3. Move subsystem-specific parser details next to the owning module where this reduces central coupling.
4. Preserve clear vertical modules for credentials, D1, release, promotion, future recovery and readiness.
5. Define a coherent machine-readable output envelope with stable fields such as schema version, command, status, mode, mutation executed, decision and evidence, while allowing typed command-specific payloads.
6. Keep runtime/application crates independent from `opsctl`.

Required checks:

- no runtime/application dependency on `tools/opsctl`;
- no semantic behavior expansion;
- parser golden/negative tests;
- Rust fmt, clippy and tests on Linux/Windows-supported paths;
- output compatibility tests for active families.

### Unit D — Inventory modularization and deterministic aggregation

Goal: make `architecture/inventory.json` a deterministic compatibility/operator projection assembled from domain-owned authorities, not a giant source of everything or lifecycle authority.

Tasks:

1. Map each inventory section to an existing domain authority where one already exists (for example D1, runtime, release, credentials, operator contract).
2. Do not create parallel registries merely to split files.
3. Use typed builders/models with explicit `schema_version` and strict validation at trust boundaries.
4. Consider JSON Schema only where it improves repository/IDE validation without creating manually duplicated schema drift.
5. Do not convert inventory to protobuf; protobuf remains appropriate for wire/IPC concerns, not Git-reviewed architecture metadata.
6. Remove lifecycle constants, AR-specific mutation overlays and global monkey-patching from generation.
7. Preserve stable architecture information intentionally; unknown giant chunks must not be preserved magically without ownership.
8. Make repeated generation byte-stable/idempotent.

Required checks:

- every generated section has an explainable owner/source;
- repeated generation produces identical bytes;
- generator cannot advance accepted/current/next AR state;
- malformed/unknown authority input fails closed;
- stable architecture information is retained by explicit ownership, not accidental pass-through;
- giant-file rewrite risk is materially reduced.

### Unit E — Final historical executable retirement

Goal: retire only evidence-proven zombie executable machinery after successor parity exists.

Priority review set includes:

- `scripts/generate-architecture-inventory-engine.py`;
- `scripts/generate-ar8-completion-status.py`;
- `scripts/check-documentation-authority-legacy.py`;
- historically named pre2j / phase / AR checkers;
- D3 successor/bootstrap checks that may still contain permanent current invariants.

Tasks:

1. Build caller graph and invariant map for each candidate.
2. Preserve `CURRENT_INVARIANT` and `UPGRADE_ROLLBACK_REQUIRED` functionality.
3. Ensure `TRANSITION_PROVENANCE_ONLY` executables have zero accidental current authority/callers.
4. Remove only `DEAD` executables after parity proof.
5. Update historical executable debt taxonomy and canonical Python estate after every addition/removal.
6. Perform a semantic scan, not only filename grep, for closeout writers, current-slice writers, historical materialization, self-writing CI, legacy promotion executors and orphan bootstrap machinery.

Required checks:

- unknown Python fails closed;
- no retired executable materialization/execution path remains;
- no unique invariant is lost;
- static historical documents/evidence remain available where required.

### Unit F — Final pre-AR-12 audit and closure of #375

Goal: prove the repository can begin AR-12 without historical lifecycle machinery and without weakening production/runtime gates.

Audit dimensions:

- lifecycle authority singularity;
- production fail-closed state;
- preservation of independently activatable application functionality;
- historical executable debt;
- inventory ownership/aggregation;
- `opsctl` role, registry, modularity and safety;
- developer-facing architecture clarity;
- Python estate completeness;
- permanent CI and protected branch evidence.

Do not close #375 until every final DoD item below is proven.

## 5. PR and exact-head discipline

Every bounded unit must follow:

```text
exact accepted main
-> bounded branch
-> Draft PR
-> one concern
-> implementation
-> freeze exact candidate SHA
-> full applicable CI
-> re-read main/reviews/threads
-> ready for review
-> guarded squash bound to expected exact head
-> accepted-main re-read
-> candidate tree == merge tree
```

After final CI begins on a chosen exact candidate, any source change invalidates the old CI evidence. Re-run the complete applicable evidence set on the new exact SHA.

Before merge prove:

```text
behind_by = 0
blocking_reviews = 0
unresolved_review_threads = 0
all applicable permanent workflows = SUCCESS
all protected required contexts = SUCCESS
```

The live registry and branch protection are authoritative; never hard-code 17 workflows or 23 contexts as timeless constants.

Every pre-AR-12 PR must include the real Camoufox cold-launch proof and Windows Profile Bridge regression whenever applicable. A red Camoufox workflow must be classified by exact failing step before changing runtime code: repository pre-policy, Xvfb/infrastructure, install, launch, identity, persistence or Bridge IPC.

## 6. Merge and acceptance boundaries

Use squash merge with expected exact head SHA for these cleanup PRs unless live governance requires a more specific accepted path. After merge, prove candidate tree == accepted merge tree.

AR-12+ architecture acceptance remains one guarded source merge plus immutable acceptance Git metadata. Do not reintroduce:

- per-AR closeout transformer scripts;
- AR-specific closeout PRs;
- self-writing workflows;
- second source PRs solely to record acceptance;
- manual current-slice state as acceptance authority.

## 7. Definition of Done before AR-12 implementation

### Lifecycle

- [ ] AR-11 is mechanically accepted.
- [ ] AR-12 is mechanically derived as current.
- [ ] No manual current-slice/accepted-checkpoint authority exists.
- [ ] No per-AR closeout machinery or self-writing CI is required.
- [ ] No second acceptance source merge is required.
- [ ] Acceptance-tag protocol remains intact.

### Production and capabilities

- [ ] `architecture_complete=false`.
- [ ] `production_core_gate=BLOCKED`.
- [ ] `production_ready=false`.
- [ ] `production_mutation=false`.
- [ ] No capability was production-enabled accidentally.
- [ ] No application functionality was deleted merely because it is disabled.
- [ ] `source_present != production_enabled` remains mechanically enforced.

### Historical executable debt

- [ ] Every suspicious executable is classified.
- [ ] `DEAD` items are removed.
- [ ] Current invariants are retained or renamed only with parity proof.
- [ ] Upgrade/rollback-required tools are retained.
- [ ] Transition-provenance executables have no accidental current execution authority.
- [ ] No retired executable materialization/execution path remains.

### Inventory

- [ ] Inventory has no live lifecycle ownership.
- [ ] No AR-8 monkey-patch lifecycle model controls current state.
- [ ] Aggregation is deterministic and domain ownership is clear.
- [ ] Stable information has explicit ownership.
- [ ] Historical snapshots are explicitly non-authoritative.
- [ ] Giant-file rewrite risk is materially reduced.

### `opsctl`

- [ ] Canonical role is explicit.
- [ ] Actual CLI equals registered active CLI.
- [ ] Old flat current-authority command names are removed.
- [ ] D1, Release and Promotion are registered.
- [ ] Recovery is RESERVED for AR-14.
- [ ] Readiness is RESERVED for AR-16.
- [ ] No provider client, provider mutation or network authority exists.
- [ ] No secret readback or customer/database/deployment mutation exists.
- [ ] No production child-process execution authority exists.
- [ ] Repository IO, policy and presentation concerns are separated.
- [ ] CLI/module architecture is suitable for AR-12 through AR-16.
- [ ] Output semantics are coherent and machine-readable.
- [ ] Developer documentation clearly explains ownership and extension rules.

### CI and merge proof

- [ ] All applicable permanent workflows are green on one exact final head.
- [ ] All protected required contexts are green on that same exact head.
- [ ] Real Camoufox cold-launch proof is green.
- [ ] Windows Profile Bridge regression is green.
- [ ] `behind_by=0`, blocking reviews=0, unresolved threads=0.
- [ ] Guarded squash is bound to the exact candidate head.
- [ ] Accepted `main` is re-read after merge.
- [ ] Candidate tree equals accepted merge tree.

### Tracking

- [ ] #375 is closed only after every item above is proven.

Only after this checklist is complete may the project begin AR-12 implementation. At that point the intended platform is:

```text
lifecycle = Git-derived
inventory = typed/domain-owned deterministic projection
opsctl = operational policy & decision plane
GitHub = orchestration/approval
Wrangler/provider executors = mutation boundary
application capabilities = independently activatable
production = still BLOCKED
```
