# Architecture Re-baseline v3 — AR-5 Wrangler / Runtime Authority Cleanup

**Document status:** EVIDENCE / AR-5 accepted  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Tracking:** #266 / implementation #290 / closeout #292  
**Implementation PR:** #291  
**Exact-green implementation head:** `afed435bb714794d6c4f252be6b44c592ee31b2b`  
**Accepted implementation merge:** `82d251a1d6666199c6eace393eedc1766157fcee`  
**Applicable permanent workflows:** **13/13 success** on the unchanged exact head  
**Production mutation:** forbidden

## 1. Purpose

AR-5 applies the accepted AR-2 `GENERATION_VERIFICATION = DELETE` topology decision to the canonical runtime and deployment authority. The implementation removes a legacy Queue identity that had no accepted Queue envelope workload or independent consumer while preserving synchronous profile-generation verification through `ProfileGenerationVerifyApi -> execute_verify_generation`.

AR-5 is runtime/deployment authority remediation. It does not replace the accepted AR-4C application-architecture remediation: `application_architecture` remains accepted through AR-4C and AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it.

## 2. Accepted runtime authority

The accepted control-plane Queue producer authority is exactly:

- `INTEGRATION_EVENTS`;
- `MAILBOX_JOBS`.

`MAILBOX_JOBS` retains its accepted DLQ/consumer semantics. `GENERATION_VERIFICATION` is absent from canonical staging/production Wrangler producer bindings, the control-plane contract constant set, the `/bindings` runtime probe, deployment-manifest authority, and the control-plane Queue workload model.

Generation verification remains synchronous application authority; AR-5 does not change its public HTTP route, authentication, state machine, D1/R2 behavior, or application use case.

## 3. Accepted evidence

- implementation issue: #290;
- implementation PR: #291;
- exact-green implementation head: `afed435bb714794d6c4f252be6b44c592ee31b2b`;
- accepted implementation merge: `82d251a1d6666199c6eace393eedc1766157fcee`;
- permanent PR workflows: **13/13 success**;
- implementation branch at acceptance: `behind_by=0`;
- unresolved review threads: **0**;
- blocking reviews: **0**.

The initial implementation candidate correctly failed the permanent Quality Gate because `scripts/cloudflare-deploy-config.py` still encoded the obsolete three-producer deployment model. The candidate was corrected rather than weakening the gate; the final exact head passed canonical Cloudflare deploy configuration, fail-closed environment fixtures, runtime binding topology, immutable release provenance, D3/bootstrap authority, Rust/WASM/native/Windows builds and the rest of the permanent workflow set.

## 4. Preserved invariants

- accepted AR-2 runtime-topology decision remains the provenance input;
- AR-4C remains the latest accepted application-architecture remediation;
- AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it;
- resolver Worker/D1/service isolation remains intact;
- `INTEGRATION_EVENTS`, `MAILBOX_JOBS` and mailbox DLQ remain real transport boundaries;
- required secret names and staging/production isolation are unchanged;
- no public HTTP/OpenAPI semantics changed;
- no D1 migration/schema changed;
- no Cloudflare/provider production resource was created, updated or deleted;
- `architecture_complete=false`;
- Production Core remains `BLOCKED`;
- `production_ready=false`.

## 5. Accepted machine state and handoff

After this mandatory post-merge authority closeout:

```text
accepted architecture checkpoint = AR-5
runtime authority cleanup = ACCEPTED
application architecture = ACCEPTED_THROUGH_AR4C
AR-4D = NOT_REQUIRED
next slice = AR-6 — Full Python Estate + read-only Rust opsctl
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

AR-6 must start from the accepted AR-5 main state and must not reintroduce a second mutable authority for any lifecycle concern.
