# PAS-2/TC-1 — Executable Frontend Transport Contract Closure

**Document status:** BOUNDED_EXECUTION_CONTRACT  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Product scenario:** PAS-2 — Client and browser-profile workflow  
**Architecture contracts:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`, `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Public contract policy:** `docs/CONTRACT_POLICY.md`  
**Production authorization:** NONE  
**FC-6 execution:** NOT AUTHORIZED BY THIS CONTRACT

PAS-2/TC-1 fixes one demonstrated transport-contract boundary. It is not a generic SDK program, a
frontend rewrite permission, a production rollout, or authority to redesign unrelated Cloudflare,
Profile Bridge, Camoufox, mailbox or `opsctl` architecture.

## 1. Demonstrated defect

The active browser path still duplicates endpoint method/path/body/status assumptions and can move an
untrusted HTTP payload into application/UI code through generic JSON parsing plus caller-selected
TypeScript casts. The target must have one executable contract chain and runtime proof at the network
boundary.

The target chain is:

```text
capability-owned Rust contract
        ↓ deterministic export
canonical OpenAPI 3.1
        ↓ strict non-repairing validator/compiler
generated capability-grouped leaf operations + runtime validators
        ↓
effect-only HTTP transport
        ↓
feature adapters
        ↓
application/UI
```

Rust remains the natural semantic author. OpenAPI is the canonical versioned wire projection and sole
frontend compiler input. Generated TypeScript and compiler IR are mechanical projections, never a
second semantic authority.

## 2. Idempotency ownership decision

`requestDigest` is historical protocol debt and is not part of the target browser contract.
The application is not in production and has no external compatible clients. Therefore PAS-2 uses an
explicit one-time pre-production destructive re-baseline rather than a compatibility adapter, optional
legacy field, v2 island or fallback runtime.

Target command execution:

```text
feature/application
  allocates one Idempotency-Key per logical command
  and retains the same key for retries
        ↓
generated operation
  encodes method/path/headers/body
        ↓
server application boundary
  strictly decodes the typed command
        ↓
server computes internal PayloadFingerprint
        ↓
idempotency store
  same key + same fingerprint      -> replay
  same key + different fingerprint -> conflict
```

`PayloadFingerprint` is server-owned implementation evidence. It:

- is computed after strict typed command decoding;
- is never supplied by the browser or any external client;
- is absent from OpenAPI request DTOs;
- is not a release, contract, artifact or evidence digest authority;
- may use SHA-256 over one normalized typed-command representation owned by the server application
  boundary;
- must not create another cross-component canonical-JSON protocol.

Transaction A removes the newly introduced browser/compiler `requestDigest` authority
(`x-part-crm-request-digest`, `part-crm-json-v1`, frontend canonicalization and digest golden vectors)
but does **not** yet change the active runtime DTO. Transaction B deletes `requestDigest` end-to-end in
one atomic cutover and introduces the internal `PayloadFingerprint` at the server idempotency boundary.

## 3. Compiler rule: validate, never repair

The compiler consumes producer output exactly as emitted. It must fail closed instead of rewriting it.
In particular it must not:

- convert OpenAPI 3.0 `nullable` into JSON Schema 2020-12 unions;
- replace permissive `application/problem+json` schemas with another schema;
- insert request-digest, response-header or browser-policy repair extensions;
- repair missing parameters, statuses, media types, security or response semantics;
- resolve network references or silently accept unsupported constructs.

An invalid producer is fixed at the capability-owned Rust/OpenAPI producer. The validator/compiler may
normalize only its own untracked internal representation after semantic validation; it may not change
wire meaning or emit a repaired canonical contract.

For PAS-2 generated operations, a declared OpenAPI response header is treated as part of the declared
response contract and is validated by the generated operation. This project rule is documented here
rather than encoded through a compiler-inserted side extension.

## 4. Transaction order

```text
fresh accepted-main re-baseline
-> Transaction A: canonical contract/governance closure
-> merge only after exact-head permanent CI is green
-> Transaction B: atomic runtime cutover + requestDigest deletion
-> merge only after exact-head permanent CI is green
-> fresh FC-6 read-only readiness audit
-> only a separate explicit instruction may start FC-6
```

### 4.1 Transaction A — current PR

Transaction A must:

1. keep the active frontend runtime unchanged;
2. establish deterministic canonical OpenAPI 3.1 as the sole compiler input;
3. make validation fail closed without producer repair;
4. remove the candidate-only browser `requestDigest` canonicalizer, extension and golden authority;
5. preserve `Idempotency-Key` as application-owned logical-command identity;
6. delete `contracts/generated/control-plane.openapi.json` and never restore it;
7. delete `frontend_control_plane_openapi_projection` from release topology and delete
   verification-only consumers/drift gates whose sole purpose was keeping that projection alive;
8. remove the consumed Pre-2J B4/C2/C3/C3G one-shot contract-authority checkers and machine authority
   artifacts; D3 and C5/C6 are not implicitly included;
9. keep one permanent current-contract evolution verifier: compatible additive v1 is accepted,
   ordinary breaking v1 is rejected, and a destructive migration requires an explicit governed
   decision;
10. prove deterministic export twice, deliberate negative fixtures, compatibility checks and all
    permanent exact-head CI.

Historical Release Set assets remain immutable exact bytes. `contracts/baseline/` remains immutable
compatibility evidence. Neither category requires retaining obsolete executable checkers or generated
projections in current architecture.

### 4.2 Transaction B — atomic full-stack cutover

Transaction B is accepted only as one complete vertical change:

```text
new generated operation introduced
+ caller switched
+ old endpoint metadata/runtime path deleted
+ requestDigest deleted end-to-end
+ server-owned PayloadFingerprint installed
```

It must:

1. remove `requestDigest` from Rust transport DTOs, canonical OpenAPI, frontend inputs and server ingress;
2. add internal server-owned `PayloadFingerprint` to idempotency/replay comparison;
3. preserve one application-allocated `Idempotency-Key` across retries of the same logical command;
4. compile capability-grouped leaf operations with request encoders and strict success/error/header
   runtime validators;
5. introduce one effect-only HTTP transport returning bounded raw status/headers/bytes;
6. switch all active feature API adapters;
7. delete `requestJson<T>`, network-boundary `payload as T`, `endpoint.ts`, migrated handwritten route
   literals/metadata and direct browser API `fetch` callers;
8. leave no compatibility adapter, optional legacy digest field, dual client or fallback path.

Because there is no production deployment or external compatible client, current `/api/v1` may be
destructively re-baselined once by this explicitly governed Transaction B. The superseded v1 remains
only in Git history and immutable historical Release Set artifacts; it is not an active runtime
contract, rollback target or CI authority after the cutover.

## 5. Layer boundaries

| Layer | Owns | Must not own |
| --- | --- | --- |
| Capability Rust contract | Public DTO and operation semantics | Frontend implementation |
| Canonical OpenAPI | Versioned wire projection | Product/use-case policy |
| Contract compiler | Strict mechanical validation/transformation | Producer repair or endpoint policy |
| Generated operation | Method/path/query/header/body encoding, declared response decoding/validation | Business workflow/retry policy |
| HTTP transport | Fetch effect, credentials, abort/timeout, bounded bytes, raw status/headers | DTO schemas or endpoint success semantics |
| Feature adapter | Wire-to-feature adaptation and error mapping | Duplicated endpoint metadata |
| Application/UI | Product semantics and logical-command idempotency-key lifecycle | Raw HTTP trust/casts |
| Server application boundary | Typed command semantics and internal payload fingerprint | Client-controlled digest protocol |

Generated operations remain leaf infrastructure artifacts. They do not become a global SDK, service
container, endpoint registry or business-service layer.

## 6. Runtime result/error contract for Transaction B

Minimum transport/contract failures remain distinct:

```text
NetworkError
TimeoutError
AbortedError
ResponseTooLargeError
UnexpectedStatusError
UnexpectedContentTypeError
MalformedBodyError
ContractDecodeError
```

Unexpected `2xx` is not success. `204` succeeds only when declared as no-body. Declared non-2xx
representations are validated. Missing required bodies/headers, malformed JSON and schema-invalid JSON
fail closed before application/UI code.

Generic automatic retry is forbidden in the HTTP transport. Retry belongs to application policy and
must reuse the same `Idempotency-Key` for the same logical command.

## 7. Acceptance

Transaction A is accepted only when one unchanged exact head proves its non-repairing compiler input,
negative fixtures, deletion set, permanent compatibility governance and all applicable protected CI.
Transaction B is accepted only when the whole active frontend is on generated validated operations,
`requestDigest` has zero live runtime/contract/frontend/server-ingress ownership, the server-owned
fingerprint replay/conflict matrix is tested, predecessor paths have zero callers and all permanent CI
is green.

Neither transaction authorizes staging, production, provider mutation, credentials, FC-6 or FC-7.
