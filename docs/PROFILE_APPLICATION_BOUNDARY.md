# Profile Application Boundary

**Status:** Phase 0 accepted architecture for profile create / visible-by-ID / assignment.

**Scope:** repository architecture only. Profile grants, generation lifecycle, coordinator behavior and mailbox routes remain separate verticals. This document does not claim production readiness.

## Purpose

The profile capability now owns create, visible-by-ID and client-assignment orchestration behind provider-neutral application contracts:

```text
HTTP / Workers SDK
  -> apps/control-plane-worker/src/profiles.rs
     transport parsing + authenticated actor context + protocol mapping
  -> crates/use-cases/src/profiles.rs
     create/query authorization + replay/write/query sequencing
  -> crates/use-cases/src/profile_assignments.rs
     assignment authorization + replay/write/version sequencing
  -> crates/application-ports/src/profiles.rs
     provider-neutral inward profile + assignment contracts
  -> crates/cloudflare-adapters/src/d1_profiles.rs
     governed D1/idempotency/visibility adapter
  -> existing atomic D1 command/query implementations
```

Concrete D1 construction is isolated in `apps/control-plane-worker/src/composition.rs`. The migrated profile transport must not import provider D1 modules, construct governed mutation DTOs, or instantiate D1 repositories directly.

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

## Profile Assignment Ownership

`ProfileAssignmentApi` is owned by the same thin `profiles.rs` transport but has a dedicated pure use-case module so assignment semantics do not become mixed into create/query logic.

The accepted ordering and compatibility rules are:

1. assignment remains tenant-owner-only and owner resolution happens before request-body parsing;
2. `assignmentId`, path `profileId`, `clientId`, `reason`, `expectedProfileVersion` and generic `requestDigest` keep their existing protocol roles;
3. the legacy request DTO remains tolerant of unknown fields; Phase 0G does not silently harden that public parsing contract;
4. response aggregate version is `expectedProfileVersion + 1` using checked arithmetic; overflow fails before replay/write;
5. exact pre-write idempotency replay skips the governed write;
6. fresh assignment remains HTTP `200`, result code `assigned`, resource reference equal to the assignment ID;
7. unlike generation Phase 0F, a conflict-class assignment write failure performs exactly one post-conflict exact replay lookup; an exact replay succeeds, while replay miss/conflict remains a conflict;
8. non-conflict write failures do not perform that second replay lookup;
9. `D1ProfileApplicationRepository` maps the inward assignment write to the existing `AssignProfileMutation` and `D1GovernedCommandRepository::assign_profile` transaction rather than duplicating SQL;
10. the existing governed D1 batch remains the source of atomic command-journal, assignment, idempotency, audit and outbox mechanics;
11. assignment remains business/history association only. It never grants profile/client visibility and must remain separate from explicit grant ACLs.

Stable public failure classes remain neutral not-found, version conflict, invalid state, conflict, integrity failure, internal failure and dependency unavailable. Provider/storage failures are not collapsed into business not-found.

## Command Evidence

The profile vertical reuses provider-neutral `application-ports::CommandExecutionEvidence` for:

- idempotency key;
- request digest;
- deterministic audit event ID;
- deterministic outbox event ID;
- command timestamp;
- idempotency expiry timestamp.

The generic request-digest rule remains the existing 16–256-character evidence contract. The HTTP/Workers layer derives evidence, while concrete D1 mutation envelopes remain adapter-owned.

## CI Enforcement

Permanent Repository Quality Audit checks enforce:

- capability-owned `ProfileApplicationPort`, `ProfileAssignmentApplicationPort`, `ProfileCreateWrite`, `ProfileAssignmentWrite`, create/query symbols and dedicated assignment use-case symbols;
- `check-profile-worker-application-boundary.py` rejects direct D1/provider orchestration in `profiles.rs`;
- the same policy requires live `ProfileAssignmentApi` routing through `profiles.rs` and rejects return of superseded create/get/assignment handlers, DTOs or `AssignProfileMutation` in legacy `api.rs`;
- its negative self-test deliberately restores direct provider orchestration and a legacy assignment handler and proves both are rejected;
- Cross-Component acceptance resolves assignment orchestration through `profiles.rs -> profile_assignments.rs -> d1_profiles.rs` while retaining atomic D1 command evidence in `d1_governed_commands.rs`;
- the existing `assignmentDoesNotAuthorize` negative evidence remains mandatory.

Pure fake-port tests separately prove non-owner stop-before-replay/write, overflow-before-replay/write, exact pre-write replay, conflict-only post-write replay and non-conflict no-recheck. Adapter tests preserve stable assignment failure mapping. Worker native/WASM and governed D1 acceptance remain separate evidence layers.

## Remaining Phase 0 Work

This slice does **not** close architecture gap A0. Remaining Worker-owned route families must migrate in bounded verticals rather than by expanding this PR. Profile grants, client grants and identity/membership/invitation governance remain in the legacy governance module until their own accepted slices.

Generation and mailbox orchestration are already behind their accepted application boundaries and are not reopened by this slice.

`production_ready=false` remains unchanged.
