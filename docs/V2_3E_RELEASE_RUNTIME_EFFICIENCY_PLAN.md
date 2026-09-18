# V2.3E Release/runtime materialization efficiency plan

**Status:** CURRENT bounded implementation plan inside V2.3E.  
**Owner:** existing Release/runtime owners.  
**Provider/Production effect:** none.

## Problem

The accepted-main Release Set build currently gives the Windows runtime component the global source SHA as part of its component identity and republishes the full raw `runtime-bundle.tar` into every aggregate Release Set. The post-#764 run proved that the runtime build itself is fast, while publication of the unchanged ~1.19 GB raw runtime asset can dominate the transaction.

That is safe but unnecessarily couples unrelated source churn to a heavy runtime rebuild/reupload.

## Target model

```text
accepted main source
-> exact Release Set identity remains source-bound
-> each component has an exact immutable component identity

runtime component identity
-> complete declared runtime inputs only
-> exact immutable GitHub Release tagged by runtime component release_id
-> reuse only when the current runtime-input identity resolves to that exact durable release
-> no latest/known-good/history selection
```

The aggregate Release Set keeps the logical runtime artifact SHA-256, size and component-manifest SHA-256, but does not duplicate the heavy runtime bytes. Materialization hydrates the runtime only from the exact component release ID recorded in the Release Set and then normal Release verification checks the bytes and embedded manifest.

## Runtime input identity

The runtime input identity must cover every repository input that can change runtime bytes or their security-critical admission, including:

- Camouhost runtime sources;
- runtime lock, including embedded Python distribution and locked Python package graph;
- patched Camoufox lock and patch;
- runtime materializer and packager policy;
- patched-candidate verifier and WebGL patch checker.

The runtime component manifest uses a new source-independent component schema. Its release ID is derived from that complete runtime-input digest, not from unrelated global source SHA churn.

The aggregate Release Set still binds the runtime component to the exact accepted-main candidate and independently records the same current runtime-input digest. Release verification must fail closed if the embedded runtime manifest and current Release Set runtime-input identity disagree.

## Durable publication and reuse

GitHub Releases remain the sole durable publication authority.

For an exact current runtime-input identity:

1. Compute the exact runtime component release ID.
2. Probe that exact GitHub Release identity and exact expected asset inventory.
3. If no release exists, build once on Windows and stage publication as a draft.
4. Upload only the exact expected assets, prove their SHA-256/size metadata, and publish the draft only after the inventory is complete.
5. Release Set publication has one serialized accepted-main owner. If an exact draft exists from an interrupted prior run, resume it only when every already-uploaded asset is byte-identical. An incomplete GitHub `starter` asset may be deleted only while the release is still draft; a failed upload is not blindly retried in the same run.
6. If an already-published release is incomplete, has unexpected assets, or any uploaded asset conflicts with local bytes, fail closed. Never clobber a published content-addressed release.
7. If a complete exact published component already exists, download only the small manifest and observe exact release-asset SHA-256/size metadata; do not download, rebuild or re-upload the heavy runtime.
8. Publish only a small runtime observation as CI transport for aggregate Release Set finalization.
9. The aggregate Release Set publishes no duplicate runtime tar and uses the same draft/exact publication primitive.

A mutable cache, a latest pointer, previous/known-good selection, historical fallback, or blind retry is forbidden.

## Archive-format decision

This transaction intentionally keeps the shipping runtime as the existing deterministic raw PAX `runtime-bundle.tar`.

The current Windows delivery path has a purpose-built fail-closed streaming PAX-tar reader. Switching to gzip/zstd/another compressed shipping format would widen that security-sensitive extraction boundary and add decompression ownership/dependencies. It is therefore **not** folded into this efficiency change merely to reduce transport size.

The recurring cost is removed by immutable component reuse. A future compressed shipping representation is justified only as a separate bounded runtime-distribution change with equally strict path/type/size/inventory proof. Compression is not a prerequisite for this transaction.

## Publication bounds and diagnostics

- Runtime component publication has an explicit bounded job timeout and logs exact release ID and artifact size before upload.
- Individual GitHub API operations and heavy asset upload are bounded independently by the shared publication helper.
- Aggregate Release Set publication has a much smaller bounded job timeout because it no longer carries the runtime tar.
- New content-addressed publications are staged as drafts; only an exact complete inventory is promoted to published state.
- Accepted-main Release Set builds are serialized under one publication owner, so two source SHAs cannot race the same runtime identity.
- Interrupted draft publication is resumable without deleting uploaded exact bytes. Only incomplete draft `starter` residue is eligible for deterministic cleanup on a subsequent serialized run.
- Conflicting bytes or an incomplete published release fail closed.
- A failed component publication does not trigger alternate component selection, latest/known-good fallback, or blind clobber.
- Byte-identical replay remains accepted only for the exact same content-addressed identity.
- The publication helper's decision logic is exercised in pull-request CI, so the post-merge push workflow does not own an untested copy of the lifecycle.

## Acceptance

The transaction is complete when:

```text
unrelated accepted-main source change
-> new exact Release Set
-> same runtime-input digest
-> same immutable runtime component release ID
-> no runtime rebuild
-> no heavy runtime CI transport
-> no duplicate runtime upload

runtime input change
-> new runtime component release ID
-> one Windows materialization/build
-> one durable component publication
-> aggregate Release Set binds exact new component

release verify
-> exact source accepted
-> exact component artifact SHA/size
-> exact embedded manifest
-> runtime input digest equality
-> unpacked runtime inventory remains inside the component manifest
```

Profile Bridge remains source-bound in this transaction because it is small and fast to publish; adding a second reuse path now would increase moving parts without addressing the observed bottleneck.

No D1, Worker, Access, other provider or Production mutation is part of this plan.
