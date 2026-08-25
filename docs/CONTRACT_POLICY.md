# Contract Compatibility Policy

**Status:** normative current-contract policy  
**Updated:** 2026-08-25

## 1. Contract categories

Current contract governance distinguishes three different things. They must not be collapsed into one
permanent freeze model.

1. **Historical Release Set artifacts** are immutable exact bytes/hashes. They prove what was accepted
   or released at a historical point. They are never rewritten to make current development easier.
2. **`contracts/baseline/`** is immutable compatibility evidence for the accepted floor represented by
   that baseline. It is not the current runtime source and is not automatically refreshed from current
   contracts.
3. **Current version roots** (`openapi/v1/**`, `proto/**`) are living current contracts. Compatible
   additive evolution is permitted. Ordinary breaking evolution requires a new major root or an
   explicit governed migration/re-baseline decision.

The existence of historical bytes does not require retaining obsolete runtime adapters, generated
projections, one-shot authority checkers or CI sentinels. Git history and immutable Release Set assets
preserve history; current executable architecture preserves only current semantics and real consumers.

## 2. Current roots and ownership

- canonical browser API: deterministic merge of `openapi/v1/openapi.json` and
  `openapi/v1/fragments/*.json`;
- profile/CRM protobuf: `proto/profile/v1/`;
- Bridge protobuf: `proto/bridge/v1/`;
- natural semantic authoring owners: capability-owned Rust contracts/domain/application boundaries.

For browser HTTP the target authority chain is:

```text
capability-owned Rust contract
        ↓
canonical OpenAPI 3.1
        ↓
strict non-repairing validator/compiler
        ↓
generated leaf operations + runtime validators
```

OpenAPI is the sole versioned executable frontend compiler input, but it is a projection of natural
capability semantics rather than a second manually maintained business authority.

## 3. Compatible current-v1 evolution

Generally compatible changes include:

- adding a new endpoint with a unique operation ID;
- adding an optional request/response capability that old consumers may ignore;
- adding a uniquely named schema/component/fragment without collision;
- adding a protobuf message;
- adding a protobuf field with a new number;
- widening a request enum when the server already accepts the new value;
- making a response contract more precise when that precision describes wire behavior already emitted
  and does not invalidate existing compatible consumers;
- adding default-deny capabilities.

Compatible additive v1 changes are allowed by the permanent compatibility verifier. A current v1 file
is not byte-frozen merely because an older accepted revision exists.

## 4. Ordinary breaking evolution

Without an explicit governed migration/re-baseline decision, the permanent verifier rejects changes
such as:

- removing/renaming an existing path or operation ID;
- removing/renaming required request/response schema members in an incompatible direction;
- introducing new required request input that old compatible consumers cannot supply;
- changing protobuf package/message/field identity or reusing a field number;
- changing the meaning of an existing authorization or opaque identifier contract;
- silently changing status/media/header/body semantics relied on by consumers.

The normal remedy is a new major root or a governed migration. A one-shot historical exception script
is not a permanent architecture mechanism.

## 5. Permanent verification

`scripts/check-contract-compatibility.py` is the permanent current-contract evolution decision path.
It must:

1. deterministically compose current OpenAPI v1;
2. lint current OpenAPI/protobuf roots;
3. compare current contracts with the immutable compatibility baseline;
4. allow compatible additive evolution;
5. reject deliberately breaking fixtures and ordinary incompatible v1 evolution;
6. fail closed on malformed/ambiguous contract structure.

`contracts/baseline/**` and accepted phase/history provenance remain separately protected as immutable
evidence. That integrity protection is not a second current-v1 evolution policy.

The consumed Pre-2J B4/C2/C3/C3G one-shot contract-authority checkers and machine authority records are
retired from current architecture. Their decisions remain reconstructible from Git history. D3 and
C5/C6 are separate concerns and are not retired by this contract-policy change.

## 6. PAS-2 explicitly governed pre-production destructive re-baseline

PAS-2 has a narrow explicit exception to the ordinary breaking-v1 rule: the application is not in
production and has no external compatible clients. Transaction B may therefore destructively
re-baseline current `/api/v1` once to remove historical `requestDigest` protocol debt without a
compatibility adapter, optional legacy field, fallback runtime or artificial `/api/v2` island.

This authorization is valid only for the complete PAS-2 Transaction B described by
`docs/PAS2_FRONTEND_TRANSPORT_CONTRACT_CLOSURE.md`:

- `requestDigest` is removed end-to-end from Rust DTOs, OpenAPI, frontend and server ingress;
- the browser retains only application-owned `Idempotency-Key` identity for a logical command;
- the server computes an internal `PayloadFingerprint` after strict typed decode;
- same key + same fingerprint replays, same key + different fingerprint conflicts;
- `PayloadFingerprint` never becomes an HTTP field, OpenAPI member or release/contract digest
  authority;
- old current-v1 runtime support is not retained after the atomic cutover.

Historical Release Set artifacts remain immutable. The old v1 continues to exist in Git history and
historical evidence, but after the accepted cutover it is not an active runtime contract, rollback
path or CI authority. The post-cutover current v1/baseline becomes the new active compatibility floor
through a separately visible governed acceptance step.

## 7. Compiler and generated-code policy

The frontend compiler validates producer output; it does not repair it. It must reject unsupported
OpenAPI/JSON Schema constructs, mixed/legacy dialect semantics, network/unresolved references,
duplicate operation IDs, incomplete path parameters, unsupported serialization/media types and
permissive problem schemas. It must not auto-convert `nullable`, substitute schemas, insert missing
headers/statuses, or add compiler-owned protocol extensions.

The frontend compiler is the **single current owner of compiler-supported-subset admission and its
ephemeral operation IR**. Capability-owned Rust modules own product/public contract semantics;
`scripts/check-contract-compatibility.py` owns current-version compatibility evolution; producer-drift
checks prove only that canonical projections still match their natural Rust producers. Those
mechanisms must not independently reimplement the frontend compiler's supported-subset policy. The
compiler IR is deterministic, untracked and never a contract, release input, runtime input or second
semantic authority.

Generated TypeScript operations, encoders and runtime validators are mechanical projections and may
not be edited into a second semantic authority. Python may implement/orchestrate the deterministic
compiler/build boundary but must not own product wire semantics. Browser HTTP remains OpenAPI 3.1 +
JSON; protobuf is reserved for real process/binary boundaries.

## 8. Digest discipline

SHA-256 remains available for repository Release Set/artifact/evidence identity where already
specified. That does not make every internal hash a public contract.

PAS-2 server `PayloadFingerprint` is an internal idempotency comparison value. It must not create a
parallel external canonical-JSON lineage, frontend hashing requirement or cross-component release
identity. `x-part-crm-request-digest`, `part-crm-json-v1` and frontend request canonicalization are not
part of the target architecture.