# Profile Generation Application Boundary

**Status:** Phase 0 reference pattern under implementation for generation register / visible-by-ID / verify / activate / deactivate / quarantine.

**Scope:** architecture convergence only. This slice preserves the accepted generation state machine and observable HTTP contract. It does not add lifecycle states, redesign object storage, expand coordinator/Bridge protocols, or promote production readiness.

## Target Ownership

```text
HTTP / Workers SDK
  -> thin generation Worker transport
     actor/path/body/evidence parsing + HTTP mapping
  -> crates/use-cases/src/generations.rs
     authorization + validation + replay + version/state sequencing
  -> crates/application-ports/src/generations.rs
     provider-neutral generation application contract
  -> crates/cloudflare-adapters/src/d1_profile_generation_application.rs
     D1 idempotency + existing governed generation mutations/visibility query
  -> crates/cloudflare-adapters/src/d1_profile_generations.rs
     accepted atomic D1 state machine
```

Concrete D1 construction belongs only in the Worker composition root.

## State And Version Invariants

Generation state remains limited to:

- `REGISTERED`;
- `VERIFIED`;
- `QUARANTINED`.

Two independent aggregate-version domains must not be conflated:

- verify and quarantine advance the **generation** version;
- activate and deactivate advance the **profile** version.

Activation still requires a verified generation. Quarantine still rejects an actively selected generation. Existing D1 governed commands remain the atomic source of truth for those transitions.

All version arithmetic is checked. Saturation and wrapping are forbidden.

## Replay Compatibility

The legacy generation transport performs exact idempotency replay **before** each mutation but does not perform a second replay lookup after a write failure.

Phase 0F intentionally preserves that behavior. A concurrent/unique write conflict remains a conflict rather than being silently converted into a replay by new application orchestration.

This differs intentionally from later mailbox/client boundaries that already owned a post-conflict replay recheck.

## Mutation Authorization And Query Visibility

All five mutation families are tenant-owner-only and must fail disclosure-neutrally before body/evidence processing for non-owners:

- register;
- verify;
- activate;
- deactivate;
- quarantine.

Visible generation GET remains available through the established owner-or-explicit-profile-grant visibility policy. Profile assignment is not authorization.

## Protocol Validation

The migrated transport must preserve legacy validation order and shape:

- malformed profile/generation path IDs -> neutral not found;
- register object key: 16–512 characters, no leading `/`, no `..`, no backslash, only ASCII alphanumeric plus `_-. / :` accepted by the existing rule;
- metadata/container digests: exactly 64 lowercase hexadecimal characters;
- verification reference: 8–256 ASCII alphanumeric / `_` / `-` / `:`;
- command request digest: existing generic 16–256 evidence rule, **not** the mailbox 64-hex rule;
- request DTOs deny unknown fields.

The visible response remains:

- `generationId`;
- `metadataDigest`;
- `containerDigest`;
- `status`;
- `version`;
- `verificationReference`.

`objectKey` is intentionally not exposed by the application read model or HTTP response.

## HTTP Result Compatibility

- register fresh/exact replay -> `201`, result `registered`, version `1`;
- verify fresh/exact replay -> `200`, result `verified`, expected generation version + 1;
- activate fresh/exact replay -> `200`, result `activated`, expected profile version + 1;
- deactivate fresh/exact replay -> `200`, result `deactivated`, expected profile version + 1;
- quarantine fresh/exact replay -> `200`, result `quarantined`, expected generation version + 1.

Stable problem taxonomy remains neutral not-found, version conflict, invalid state, conflict, integrity failure, internal failure and dependency unavailable.

## CI Enforcement Target

`check-generation-worker-application-boundary.py` is the Phase 0F fail-closed policy. Once live routing is switched and native/WASM composition is proven, the permanent Repository Quality Audit will require:

- application-owned generation ports/use cases;
- composition-root construction of `D1ProfileGenerationApplicationRepository`;
- no direct D1/idempotency/mutation types in the Worker generation transport;
- all six generation application calls in the transport;
- the existing D1 repository/mutation state machine retained in the Cloudflare adapter;
- a negative fixture proving direct D1/idempotency transport is rejected.

Pure fake-port tests independently prove mutation authorization, metadata validation, exact replay, checked overflow, legacy no-post-write-replay behavior and member-capable visible query.

## Non-Goals

No real object-store redesign, generation lifecycle feature, coordinator/Bridge expansion, public API feature change, remaining governance migration, or production-readiness promotion is part of this slice.

`production_ready=false` remains unchanged.
