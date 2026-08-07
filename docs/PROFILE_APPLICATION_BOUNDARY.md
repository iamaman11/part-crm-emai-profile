# Profile Application Boundary

**Status:** Phase 0 reference pattern for the migrated profile create / visible-by-ID vertical.

**Scope:** repository architecture only. Assignments, profile grants, generation lifecycle, coordinator behavior and mailbox routes remain outside this slice. This document does not claim production readiness.

## Purpose

Profile create and visible-by-ID are the second bounded Phase 0 vertical moved from Worker-owned orchestration to an application-owned command/query path, following the accepted client reference vertical:

```text
HTTP / Workers SDK
  -> apps/control-plane-worker/src/profiles.rs
     transport parsing + authenticated actor context + protocol mapping
  -> crates/use-cases/src/profiles.rs
     authorization intent + replay/write/query sequencing
  -> crates/application-ports/src/profiles.rs
     provider-neutral inward contract
  -> crates/cloudflare-adapters/src/d1_profiles.rs
     governed D1/idempotency/visibility adapter
  -> existing atomic D1 command/query implementations
```

Concrete D1 construction is isolated in `apps/control-plane-worker/src/composition.rs`. The migrated profile transport must not import provider D1 modules, construct `CreateProfileMutation`, or instantiate D1 repositories directly.

## Profile Create Ownership

The application use case owns these decisions and ordering rules:

1. only a tenant owner may execute profile create; non-owner disclosure remains neutral `not_found`;
2. owner authorization is evaluated before request-body/idempotency parsing, preserving the former disclosure boundary;
3. the use case creates the typed `BrowserProfile` aggregate in its initial state;
4. exact idempotency replay returns the prior logical result without a new write;
5. a concurrent unique conflict is rechecked for exact replay and is otherwise a conflict;
6. the D1 adapter maps typed profile state and `CommandExecutionEvidence` into the existing governed `CreateProfileMutation` / `MutationEnvelope`;
7. the existing D1 batch retains atomic profile command-journal, profile row, idempotency, audit and outbox behavior;
8. provider failures map to stable application failure classes rather than leaking concrete SDK/storage errors.

The HTTP contract remains transport-owned: fresh create is `201`, exact replay is `200`, and the existing camelCase mutation receipt remains unchanged.

## Visible Profile Query Ownership

The Worker parses the profile ID and calls the application query. The D1 adapter retains the existing disclosure-safe visibility query, including the rule that a client assignment is projection data and **not** profile authorization.

Storage strings are converted into typed `ProfileStatus` and `AggregateVersion`. Unknown status values or invalid stored versions fail as integrity errors instead of being relabeled as a missing business resource.

The transport preserves the existing response shape:

- `profileId`;
- `status`;
- `version`;
- optional `linkedClientId`.

## Command Evidence

The vertical reuses provider-neutral `application-ports::CommandExecutionEvidence` for:

- idempotency key;
- request digest;
- deterministic audit event ID;
- deterministic outbox event ID;
- command timestamp;
- idempotency expiry timestamp.

The HTTP/Workers layer derives the evidence, while the concrete D1 mutation envelope remains adapter-owned.

## CI Enforcement

Permanent Repository Quality Audit checks enforce:

- capability-owned `ProfileApplicationPort`, `ProfileCreateWrite`, `ExecuteCreateProfileCommand`, `execute_create_profile` and `get_visible_profile` symbols;
- `check-profile-worker-application-boundary.py` rejects direct D1/provider orchestration in `profiles.rs`;
- the same policy rejects return of superseded create/get profile handlers and DTOs in legacy `api.rs`;
- its negative self-test deliberately restores both a direct D1 import and a legacy profile handler and proves they are rejected;
- the Step 4 governed-write policy proves `profile.create` orchestration in the application layer while retaining the existing atomic governed D1 implementation;
- cross-component acceptance proves live routing through `lib.rs -> profiles.rs -> use-cases` without weakening assignment/grant evidence.

Pure fake-port tests, adapter tests, Worker native tests, WASM checks and the release Worker build remain separate evidence layers.

## Remaining Phase 0 Work

This slice does **not** close architecture gap A0. Remaining Worker-owned route families must migrate in bounded verticals rather than by expanding this PR. In particular, profile assignments, profile grants, identity/governance routes, generation orchestration and mailbox orchestration keep their current ownership until their own accepted slices.

`production_ready=false` remains unchanged.
