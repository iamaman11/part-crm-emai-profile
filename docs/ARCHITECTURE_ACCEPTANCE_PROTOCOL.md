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
  -> behind_by = 0
  -> blocking reviews = 0
  -> unresolved review threads = 0
  -> guarded squash merge bound to expected candidate head SHA
  -> accepted-main reread
  -> candidate tree == accepted merge tree
  -> immutable acceptance metadata
  -> derive accepted checkpoint / current slice from static program order
```

## Source and state model

`main` is the only source history. `architecture/architecture-program-sequence.json` contains only the stable sequence and predecessor/successor relation; it must not contain mutable `accepted`, `current`, or equivalent lifecycle flags.

Acceptance-only metadata does not require another source commit. From AR-12 forward it is an append-only annotated Git tag under:

```text
architecture/accepted/<lowercase-slice-id>
```

The tag points at the accepted `main` squash merge. Its message is a versioned JSON acceptance record binding the PR, pre-merge base, exact candidate SHA/tree, accepted merge SHA/tree, permanent-workflow rollup, review/thread counts, accepted-main reread and production invariants.

Tags are append-only. They may not be force-moved, overwritten or deleted by the acceptance procedure.

## AR-11 migration anchor

AR-11 predates this generic protocol by one merge and is the bounded migration bootstrap:

- PR #374;
- exact-green candidate `3ce30a55eb6b6390a5175572aeb66c29134bfc81`;
- candidate tree `94850c7669ec189d5cbaa880a260d2324fa9c9a6`;
- accepted merge `0166a49ba3fefb1d4abf8b48d4983c3e3c145de3`;
- accepted merge tree `94850c7669ec189d5cbaa880a260d2324fa9c9a6`;
- 17/17 applicable permanent workflows successful;
- `behind_by=0`, blocking reviews `=0`, unresolved review threads `=0`.

The machine policy stores this exact bootstrap record. The permanent validator re-derives the accepted merge tree and first-parent identity from Git. Historical accepted state through AR-10 remains frozen provenance; acceptance tags become mandatory at AR-12.

## Derived lifecycle state

The accepted checkpoint is the highest contiguous accepted slice starting from the frozen historical baseline, the AR-11 migration anchor and then acceptance tags.

The current slice is the successor of that accepted checkpoint.

Therefore `docs/status.json`, `architecture/inventory.json`, transition JSON and human navigation documents are projections. They may be regenerated for developer convenience, but they are not independent lifecycle authority and may never manufacture acceptance.

Missing, conflicting or non-contiguous acceptance metadata fails closed.

## Forbidden forward procedure

The following patterns are retired:

- `arXX-acceptance-closeout` branches as a required lifecycle stage;
- per-slice `*-closeout-once.py` or equivalent transformers;
- temporary workflows with `contents: write` that edit, commit or push the PR branch;
- a second source PR solely to record that a prior exact-head merge was accepted;
- force-moving/replacing acceptance tags;
- repeated manually maintained `current_slice` / `accepted_checkpoint` values treated as independent authority.

A cleanup/refactor PR after an accepted slice is allowed when it changes real repository policy/code/DX. It is not an acceptance closeout and may not rewrite the meaning of the already accepted source tree.

## Production boundary

This protocol never authorizes production. Through AR-16:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

Only the canonical AR-17 transition may change the architecture/gate authorization state, and PC-1 remains the first production enablement/provisioning stage.
