# Architecture Acceptance Protocol

**Status:** BINDING  
**Program:** Architecture Re-baseline v3  
**Tracking issue:** #375  
**Machine policy:** `architecture/architecture-acceptance-policy.json`  
**Static program order:** `architecture/architecture-program-sequence.json`

## Purpose

Architecture slices use one source history and one acceptance merge. Acceptance is no longer recorded by a per-slice closeout transformer, a CI workflow that rewrites its own PR branch, or a second source-changing PR whose only purpose is to say that the first PR was accepted.

The forward protocol is:

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
  -> append-only immutable acceptance tag
  -> derive accepted checkpoint / current slice from static program order
```

## Architecture PR identity

From AR-12 forward an architecture slice must identify itself twice and consistently:

```text
PR title:   AR-12: ...
head branch agent/ar12-...
```

The required `GitHub Governance Contract` rejects a mismatch, a single-sided architecture signal, a non-`main` base, a candidate behind the exact base, or a slice other than the state-machine-derived current slice. Non-architecture PRs remain ordinary repository changes and do not advance the architecture program.

## Source and state model

`main` is the only source history. `architecture/architecture-program-sequence.json` contains only the stable sequence and predecessor/successor relation; it must not contain mutable `accepted`, `current`, or equivalent lifecycle flags.

Acceptance-only metadata does not require another source commit. From AR-12 forward it is an append-only annotated Git tag under:

```text
architecture/accepted/<lowercase-slice-id>
```

The tag points at the accepted `main` squash merge. Its message is a versioned JSON acceptance record binding the PR, pre-merge base, exact candidate SHA/tree, accepted merge SHA/tree, protected required-check rollup, applicable permanent-workflow rollup, review/thread counts, accepted-main reread and production-state invariants.

Tags are append-only. They may not be force-moved, overwritten or deleted by the acceptance procedure.

## Generic recorder

`.github/workflows/architecture-acceptance-recorder.yml` is the only post-merge metadata writer. It runs only for merged `AR-N:` pull requests and has one allowed mutation boundary: create a new annotated acceptance tag and its `refs/tags/...` reference through the GitHub Git API.

It may not edit repository files, push a branch, mutate a GitHub Environment, rebuild release bits, call a provider, or change production state. Before creating the tag it re-proves candidate/base/merge tree identity, confirms the merge is in `main`, checks every required status context on the exact candidate, checks every observed applicable `PERMANENT_REQUIRED` workflow, and rejects blocking reviews or unresolved review threads. An existing matching tag is an idempotent success; an existing conflicting tag fails closed.

The workflow is classified separately as `POST_MERGE_METADATA`; it is neither a release/promotion mutator nor a manual operational authority.

## AR-11 migration anchor

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

## Derived lifecycle state

The accepted checkpoint is the highest contiguous accepted slice starting from the frozen historical baseline, the AR-11 migration anchor and then acceptance tags. A tag after a gap is invalid.

The current slice is the successor of that accepted checkpoint. Therefore `docs/status.json`, `architecture/inventory.json`, transition JSON and human navigation documents are projections. They may be regenerated for developer convenience, but they are not independent lifecycle authority and may never manufacture acceptance.

Missing, conflicting, malformed or non-contiguous acceptance metadata fails closed.

## Production state machine

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

AR-17 authorization still does not provision or enable production. `PC-1` remains the first stage allowed to perform Production Core provisioning/promotion and, only after its own protected evidence, set the applicable production-ready state.

## Forbidden forward procedure

The following patterns are retired:

- `arXX-acceptance-closeout` branches as a required lifecycle stage;
- per-slice `*-closeout-once.py` or equivalent transformers;
- temporary workflows with source-write permission that edit, commit or push the PR branch;
- a second source PR solely to record that a prior exact-head merge was accepted;
- force-moving/replacing/deleting acceptance tags;
- repeated manually maintained `current_slice` / `accepted_checkpoint` values treated as independent authority.

A cleanup/refactor PR after an accepted slice is allowed when it changes real repository policy/code/DX. It is not an acceptance closeout and may not rewrite the meaning of the already accepted source tree.
