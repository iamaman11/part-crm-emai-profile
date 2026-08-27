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
  -> guarded squash merge bound to expected candidate head SHA
  -> accepted-main reread / merge is present in main history
  -> candidate tree == accepted merge tree
```

The historical AR protocol appended:

```text
-> immutable AR acceptance tag
-> derive historical AR checkpoint/current slice from its static sequence
```

That suffix does not apply to CAP transaction selection or create a second source commit.

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

## Historical AR generic recorder

`.github/workflows/architecture-acceptance-recorder.yml` is the only post-merge metadata writer. It runs only for merged `AR-N:` pull requests and has one allowed mutation boundary: create a new annotated acceptance tag and its `refs/tags/...` reference through the GitHub Git API.

It may not edit repository files, push a branch, mutate a GitHub Environment, rebuild release bits, call a provider, or change production state. Before creating the tag it re-proves candidate/base/merge tree identity, confirms the merge is in `main`, checks every required status context on the exact candidate, checks every observed applicable `PERMANENT_REQUIRED` workflow, and rejects blocking reviews or unresolved review threads. An existing matching tag is an idempotent success; an existing conflicting tag fails closed.

The workflow is classified separately as `POST_MERGE_METADATA`; it is neither a release/promotion mutator nor a manual operational authority.

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
- repeated manually maintained `current_slice` / `accepted_checkpoint` values treated as independent authority.

A cleanup/refactor PR after an accepted slice is allowed when it changes real repository policy/code/DX. It is not an acceptance closeout and may not rewrite the meaning of the already accepted source tree.
