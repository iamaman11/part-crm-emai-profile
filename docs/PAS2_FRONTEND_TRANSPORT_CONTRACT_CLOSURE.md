# PAS-2/TC-1 — Executable Frontend Transport Contract Closure

**Document status:** BOUNDED_EXECUTION_CONTRACT
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`
**Product scenario:** PAS-2 — Client and browser-profile workflow
**Architecture contracts:** `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`, `docs/ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`
**Public contract policy:** `docs/CONTRACT_POLICY.md`
**Production authorization:** NONE
**FC-6 execution:** NOT AUTHORIZED BY THIS CONTRACT

This is one bounded correction for a demonstrated PAS-2 transport-contract failure. It is not a new
roadmap, generic SDK program, frontend rewrite or permission to redesign unrelated UI, backend,
Cloudflare, Profile Bridge or `opsctl` architecture.

## 1. Demonstrated defect

The current browser path is:

```text
capability-owned Rust contract
-> partial/mixed OpenAPI artifacts
-> generated TypeScript DTO shapes only
-> feature-owned handwritten path/method/body/status assumptions
-> requestJson<T>()
-> JSON.parse() -> unknown -> payload as T
-> application/UI
```

This fails PAS-2 contract integrity in two ways:

1. path, method, parameters, request encoding, accepted status/content type and response shape do
   not have one executable frontend input;
2. an untrusted successful HTTP representation can cross into feature/application code as a caller-
   selected `T` without runtime proof.

Current contract inputs also require closure before runtime generation: the public root declares
OpenAPI 3.1 while retained generated/schema-only artifacts contain mixed 3.0-style semantics,
frontend-consumed operations and response headers are not represented uniformly, and common problem
responses are not uniformly strict enough to generate one trustworthy decoder path.

This is a concrete failed product boundary, not an aesthetic reason to reopen the whole architecture.

## 2. Binding ownership decision

```text
capability-owned Rust contract modules
        = natural semantic authoring owners
                    |
                    v deterministic export/merge/lint
canonical versioned OpenAPI 3.1 wire artifact
        = sole executable frontend compiler input
                    |
                    v deterministic contract compiler
generated capability-grouped leaf operations
        + request encoders
        + response/error/header decoders
        + runtime validators
                    |
                    v
feature-owned infrastructure adapters
                    |
                    v
feature application/presentation models where semantically required
                    |
                    v
UI
```

OpenAPI is the single versioned executable wire artifact and the only frontend compiler input. It is
not a second manually edited semantic owner beside Rust. Generated TypeScript, validators and any
compiler IR are mechanical projections. Compiler IR is ephemeral, untracked and never a contract,
registry or runtime input.

## 3. Layer boundaries

| Layer | Owns | Must not own |
| --- | --- | --- |
| Capability Rust contract | Public DTO and operation semantics | Frontend implementation |
| Canonical OpenAPI | Versioned wire projection/compatibility surface | Product/use-case policy |
| Contract compiler | Strict mechanical transformation | Handwritten endpoint decisions |
| Generated operation | Method, path/query/header/body encoding, declared statuses/media types/headers and decoders | Business workflows, UI state, retry policy |
| Contract runtime | Bounded parsing and generated validation primitives | Endpoint registry or operation policy |
| HTTP transport | Fetch effect, credentials plumbing, abort/timeout, bounded response bytes, raw status/headers | JSON/DTO schemas, endpoint paths, success semantics |
| Feature adapter | Wire DTO to feature/application adaptation and error mapping | Duplicated URL/method/status/JSON contract |
| Application/UI | Product and presentation semantics | Raw HTTP data or transport assertions |

Generated operations are leaf infrastructure artifacts. They do not form a global application SDK,
service container, feature facade or business service layer. Physical output may be grouped by
capability to avoid a file-per-operation estate, but every exported operation remains independently
importable and tree-shakeable.

## 4. Required execution order

Staging-baseline adoption was completed under temporary #486 authority and that mechanism was removed
by #487. The resulting readiness audit recorded `FC-6 READY TO BEGIN / NOT STARTED`. PAS-2/TC-1 is the
subsequently discovered bounded repository correction and finishes before readiness is decided again:

```text
fresh accepted-main/provider re-baseline
-> Transaction A: canonical OpenAPI contract closure
-> accepted-main exact-head green
-> Transaction B: destructive frontend transport cutover
-> accepted-main exact-head green
-> fresh FC-6 read-only readiness audit
-> only a separate explicit instruction may start FC-6
```

Two meaningful transactions are permitted; a series of tiny mechanism/validator/cleanup PRs is not.
Transaction A introduces no parallel frontend runtime. Transaction B may use temporary candidate-only
bridges while under development, but accepted `main` receives the complete active-frontend cutover and
predecessor deletion together.

### 4.1 Transaction A — canonical OpenAPI contract closure

1. Discover every active browser HTTP consumer and every Rust/OpenAPI producer without creating a
   permanent inventory.
2. Classify the composed `openapi/v1` tree, auxiliary generated OpenAPI files, frontend TypeScript
   outputs, generators, tests, CI callers and release/digest consumers.
3. Preserve historical immutable Release Set assets. Change current v1 only through the existing
   compatibility policy; a breaking change requires a governed version/migration decision.
4. Establish one strictly linted OpenAPI 3.1 compiler input. Unsupported/mixed dialect semantics,
   unresolved or network `$ref`, duplicate operation IDs, incomplete path parameters and unsupported
   serialization/media types fail closed.
5. Close frontend-consumed operation coverage, including declared request headers, idempotency,
   response headers, exact success response alternatives, no-body semantics and common problem
   responses. When OpenAPI 3.1 cannot express required response-header presence, use one documented
   project-prefixed extension emitted by the Rust owner and consumed directly by the compiler, or move
   the value into a governed response body; do not create a side registry.
6. Make capability-owned Rust exporters reproduce the accepted current contract representation;
   do not introduce a central handwritten operation registry.
7. Delete superseded schema-only OpenAPI/TypeScript generators and artifacts whose only consumers are
   their own tests, drift gates or documentation. Retain only a named durable exact-byte consumer.
8. Prove deterministic merge/export twice and run compatibility plus deliberately breaking fixtures.

Transaction A does not add a new frontend client, decoder path or runtime bypass.

### 4.2 Transaction B — destructive frontend cutover

1. Introduce one effect-only HTTP transport returning raw status, headers and streaming-size-bounded
   bytes. It must not classify operation success, parse JSON or know endpoint DTOs.
2. Compile capability-grouped leaf operations from the canonical OpenAPI input. Each operation owns
   request path/query/header/body encoding and declared response status/content-type/header/body
   decoding.
3. Validate outbound operation inputs where their values originate at runtime; compile-time TypeScript
   alone is not proof of user/runtime input.
4. Validate every inbound success and declared error representation in every build mode, including
   production. No development-only validation switch is permitted.
5. Switch all active feature API adapters. Feature/application/UI code receives only validated wire
   results or feature-owned models/errors.
6. Add feature models only where frontend semantics actually differ. Mechanical copies of every wire
   DTO are forbidden; direct component imports of generated transport internals are also forbidden.
7. Delete `requestJson<T>`, network-boundary `payload as T`, generic path/method mutation helpers,
   handwritten migrated operation metadata and direct browser API `fetch` callers.
8. Replace predecessor-preserving string/path gates with the smallest specialized anti-bypass checks
   and executable negative fixtures. Do not create a generic linter framework or checker registry.
9. Inspect the complete diff and prove the human-owned semantic/checker/generator/predecessor surface
   is smaller. Record generated bytes/module count and production bundle impact; unexplained or
   unbounded growth blocks acceptance.

## 5. Runtime result and error contract

The operation layer, not the generic transport, decides whether a response matches the contract.

Minimum distinct failures:

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

Declared non-2xx representations are also generated and validated. Invalid error payloads are
contract failures; they are not fabricated into a trusted application problem. Unexpected `2xx` is
not success. `204` succeeds only for a declared no-body response and never returns a fabricated body.
A required body missing at any declared body-bearing status fails closed.

Diagnostics may contain bounded `operationId`, status, normalized media type, validation path,
contract fingerprint, generator version and correlation ID. Raw response/request bodies, secrets and
PII are not logged by default.

Generic automatic retry is outside this concern and forbidden in the HTTP transport. Any later retry
requires explicit operation idempotency evidence plus feature/application policy.

## 6. Technology and representation constraints

- Browser HTTP remains OpenAPI 3.1 + JSON. Protobuf is reserved for a genuine independently justified
  process/binary boundary and is not introduced as a browser transport workaround.
- Accepted Release Set, contract and evidence identities remain on the repository's SHA-256
  discipline. BLAKE3 does not become a second authoritative digest lineage.
- `opsctl` and `opsctl-core` do not participate in product runtime or frontend contract generation.
- Python may remain a bounded deterministic build adapter/orchestrator, but it does not become an
  OpenAPI semantic owner, runtime validator or second contract registry.
- A pinned OpenAPI 3.1/JSON Schema-capable compiler/validator may be adopted only after a bounded spike
  proves supported-subset correctness, deterministic output, browser/CSP compatibility and acceptable
  bundle impact. Unsupported constructs fail generation; `any` or validation omission is forbidden.
- Generated validation assertions are allowed only inside generated code after successful proof.
  Handwritten network-boundary assertions are forbidden.
- Generated files contain no timestamps, random IDs, absolute paths, filesystem-dependent ordering or
  machine-dependent formatting.

## 7. Acceptance proofs

PAS-2/TC-1 is accepted only when one unchanged exact head proves all applicable items:

1. same contract + compiler version produces byte-identical generated output on repeated runs;
2. malformed JSON at a declared success status yields `MalformedBodyError`;
3. valid JSON with the wrong schema yields `ContractDecodeError` and never reaches UI success state;
4. unexpected `2xx`, wrong content type, missing required body and an undeclared body-bearing response
   status fail closed;
5. declared response headers and declared problem bodies are validated;
6. method, path parameters, query serialization, headers, request body, status and media type are
   derived from the canonical OpenAPI input;
7. unsupported OpenAPI construct/dialect/reference and duplicate operation ID fail generation;
8. raw `fetch` is confined to the concrete HTTP transport; non-HTTP realtime parsing remains governed
   by its existing strict boundary;
9. active feature/UI code contains no `requestJson<T>`, response-bound `as T`, manual migrated route
   literal or direct generated-transport-internal import;
10. old runtime/generator/checker paths have zero live callers and zero unique current invariants, then
    are deleted in the owning transaction;
11. feature ownership, sibling-feature isolation, confidentiality and capability admission remain
    unchanged;
12. generated and production-bundle footprint is measured; human-owned authority/compatibility/
    checker surface is net smaller;
13. frontend unit/component tests prove malformed success cannot enter UI success state;
14. existing backend contract/route, security, compatibility, release and cross-component suites pass;
15. all applicable permanent exact-head CI and protected required contexts are green, `behind_by=0`,
    reviews/threads are clear, and accepted `main` is reread before FC-6 readiness.

## 8. Stop conditions

Stop `BLOCKED` rather than widening scope when:

- canonical Rust and accepted public contract semantics cannot be reconciled without a governed
  breaking-version decision;
- a proposed compiler silently accepts unsupported OpenAPI 3.1/JSON Schema semantics;
- the change requires a global SDK, registry, DI/plugin framework or duplicated application model;
- accepted `main` would retain two active frontend transport implementations;
- predecessor deletion is deferred to an unspecified cleanup phase;
- generated/bundle growth is unexplained or the simplification ledger is not net positive;
- the change touches unrelated backend/domain/UI/Cloudflare/Bridge/`opsctl` architecture;
- staging or production mutation is proposed as part of this repository-only contract correction.

PAS-2/TC-1 ends at executable transport-contract closure. It does not start FC-6, authorize
production, freeze the final architecture form or create a reusable redesign exception.
