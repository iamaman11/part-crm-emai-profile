# Architecture Acceptance Protocol

**Status:** BINDING for exact-head acceptance; AR lifecycle sections are historical provenance

**Current execution order:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` + Issue #266

**Historical AR protocol:** Issue #375, `architecture/architecture-acceptance-policy.json`,
`architecture/architecture-program-sequence.json`

## Purpose

Every bounded transaction uses one source history and one acceptance merge. Acceptance is not recorded
by a second source-changing PR whose only purpose is to say that the first PR was accepted.

The exact-head/review/merge/reread rules below remain binding. References that derive `AR-12…AR-17`
from the frozen static sequence describe the historical AR program only; they do not name or authorize
the active CAP transaction.

The binding protocol for every CAP transaction is:

```text
exact candidate
  -> all applicable permanent workflows success on that exact head
  -> all protected required status contexts success
  -> behind_by = 0
  -> blocking reviews = 0
  -> unresolved review threads = 0
  -> guarded merge bound to expected candidate head SHA using the transaction-authorized merge method
  -> accepted-main reread / merge is present in main history
  -> candidate tree == accepted merge tree
```

The merge method is not a second acceptance authority. A bounded transaction may require a merge commit
when parent lineage is acceptance evidence, while historical AR slices used squash merge. Whatever method
is authorized for the transaction, the exact candidate head, accepted-main tree and required parent
relationships must be re-proved after merge. Changing the method to evade those proofs is forbidden.

The historical AR protocol appended:

```text
-> immutable AR acceptance tag
-> derive historical AR checkpoint/current slice from its static sequence
```

That suffix does not apply to CAP transaction selection or create a second source commit.

## Transactions that require post-merge proof

When a transaction Definition of Done depends on evidence that cannot exist before merge — for example
protected-main checks, immutable publication, release evidence or another exact-main proof — source merge
is not the acceptance checkpoint. The mandatory order is:

```text
merge
  -> verify accepted main commit/tree and required merge lineage
  -> post-merge required checks succeed on that exact main
  -> required publication/proof succeeds for that exact main
  -> immutable acceptance checkpoint is created through the existing acceptance metadata authority
  -> mutable execution tracker is updated from fresh accepted-main facts
  -> owning Issue is closed explicitly
```

This order is fail-closed. A tracker update cannot manufacture the immutable acceptance checkpoint, and
an Issue closure cannot substitute for missing post-merge proof. The acceptance metadata writer may not
update the mutable tracker or close the owning Issue; those actions occur only after a fresh reread of the
completed checkpoint.

An owning Issue whose Definition of Done requires post-merge proof must not be auto-closed by a pull
request merge keyword such as `Closes`, `Fixes` or `Resolves`. The PR may reference the owning Issue, but
closure is explicit and occurs only after the required post-merge evidence and acceptance checkpoint
exist.

The existing annotated-tag namespace remains the single acceptance-metadata authority. A CAP transaction
may use a typed record in that namespace when its evidence shape differs from historical AR records; this
does not create another source registry or feed CAP acceptance into the frozen AR slice state machine.

## Candidate identity scopes

An implementation PR candidate and a Production candidate are related but not identical concepts:

```text
PR exact candidate
  = candidate commit/tree + exact-head CI/review/merge evidence

ReleaseCandidateId
  = accepted source/tree + immutable release artifacts + migrations/contracts
    + selected capability profile/effective-set digest

DeploymentAuthorizationEnvelope
  = ReleaseCandidateId + exact target/config/bindings + target observations/evidence/risks/authority
```

V2, R1, R2 and R3 retain one `ReleaseCandidateId`. Rehearsal/staging and Production envelopes are
target-specific; they must not be called identical merely because they reference the same release. A
material release-identity change repeats affected V2 and R evidence. A target/config/evidence change
invalidates and re-evaluates the affected envelope. The authorized and deployed Production pair must
match exactly.

## Historical AR PR identity

The historical AR protocol required an AR slice to identify itself twice and consistently:

```text
PR title:   AR-12: ...
head branch agent/ar12-...
```

The related governance contract rejects malformed AR signals. CAP execution PRs are ordinary bounded
repository changes: they follow Issue #266 and do not advance or derive authority from the frozen AR
sequence.

## Historical AR source and state model

`main` is the only source history. `architecture/architecture-program-sequence.json` contains only the stable sequence and predecessor/successor relation; it must not contain mutable `accepted`, `current`, or equivalent lifecycle flags.

Acceptance-only metadata does not require another source commit. From AR-12 forward it is an append-only annotated Git tag under:

```text
architecture/accepted/<lowercase-slice-id>
```

The tag points at the accepted `main` squash merge. Its message is a versioned JSON acceptance record binding the PR, pre-merge base, exact candidate SHA/tree, accepted merge SHA/tree, protected required-check rollup, applicable permanent-workflow rollup, review/thread counts, accepted-main reread and production-state invariants.

Tags are append-only. They may not be force-moved, overwritten or deleted by the acceptance procedure.

## Acceptance metadata recorder

`.github/workflows/architecture-acceptance-recorder.yml` is the only post-merge metadata writer. Its
historical AR path records merged `AR-N:` transactions; typed CAP paths may be added only when a bounded
transaction requires post-merge evidence that cannot be represented by the historical AR record.

The writer has one allowed mutation boundary: create a new annotated acceptance tag and its
`refs/tags/...` reference through the GitHub Git API. It may not edit repository files, push a branch,
mutate a GitHub Environment/provider, rebuild release bits, update the program tracker, close an Issue,
or change Production state.

Before creating any tag it must re-prove the exact record type's required source/merge identity and hosted
evidence. For a post-merge CAP record this includes the accepted-main tree/lineage, every required status
context on that exact main, any required exact-main publication and durable artifact identity, and explicit
non-authorization of Production/provider mutation. Existing matching metadata is an idempotent success;
existing malformed or conflicting metadata fails closed.

The workflow is classified separately as `POST_MERGE_METADATA`; it is neither a release/promotion mutator
nor a manual operational authority. Historical AR evidence remains an input only to the frozen AR state
machine; a typed CAP record is validated separately and does not advance historical AR lifecycle state.

## Historical AR-11 migration anchor

AR-11 predates this generic protocol by one merge and is the bounded migration bootstrap:

- PR #374;
- exact-green candidate `3ce30a55eb6b6390a5175572aeb66c29134bfc81`;
- candidate tree `94850c7669ec189d5cbaa880a260d2324fa9c9a6`;
- accepted merge `0166a49ba3fefb1d4abf8b48d4983c3e3c145de3`;
- accepted merge tree `94850c7669ec189d5cbaa880a260d2324fa9c9a6`;
- 23/23 protected required contexts successful;
- 17/17 applicable permanent workflows successful;
- `behind_by=0`, blocking reviews `=0`, unresolved review threads `=0`.

The machine policy stores this exact bootstrap record. The permanent validator re-derives the accepted merge tree and first-parent identity from Git. Historical accepted state through AR-10 remains frozen provenance; acceptance tags become mandatory at AR-12.

## Historical AR derived lifecycle state

The accepted checkpoint is the highest contiguous accepted slice starting from the frozen historical baseline, the AR-11 migration anchor and then acceptance tags. A tag after a gap is invalid.

The current slice is the successor of that accepted checkpoint. Therefore `docs/status.json`, `architecture/inventory.json`, transition JSON and human navigation documents are projections. They may be regenerated for developer convenience, but they are not independent lifecycle authority and may never manufacture acceptance.

Missing, conflicting, malformed or non-contiguous acceptance metadata fails closed.

## Historical AR production state machine

For accepted AR-12 through AR-16 the acceptance record must remain:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

AR-17 is the only architecture acceptance allowed to produce:

```text
architecture_complete = true
production_core_gate = AUTHORIZED
production_ready = false
production_mutation = false
```

These historical AR state values do not authorize current Production. Current authorization is the
separate CAP-08 exact-candidate R3 decision defined by the CAP execution program.

## Forbidden forward procedure

The following patterns are retired:

- `arXX-acceptance-closeout` branches as a required lifecycle stage;
- per-slice `*-closeout-once.py` or equivalent transformers;
- temporary workflows with source-write permission that edit, commit or push the PR branch;
- a second source PR solely to record that a prior exact-head merge was accepted;
- force-moving/replacing/deleting acceptance tags;
- repeated manually maintained `current_slice` / `accepted_checkpoint` values treated as independent authority;
- merge-keyword auto-close of an owning Issue whose Definition of Done requires post-merge proof.

A cleanup/refactor PR after an accepted slice is allowed when it changes real repository policy/code/DX. It is not an acceptance closeout and may not rewrite the meaning of the already accepted source tree.
