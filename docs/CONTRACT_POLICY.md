# Contract Compatibility Policy

**Статус:** normative v1 policy  
**Дата:** 2026-08-05

## 1. Version Roots

- Web API: `openapi/v1/openapi.json`;
- profile/CRM protobuf: `proto/profile/v1/`;
- Bridge protobuf: `proto/bridge/v1/`;
- Rust contract constants: `crates/contracts`.

The directory/package major version is authoritative. A new incompatible surface
uses a new major root; it is not hidden behind an in-place v1 mutation.

## 2. Stable Problem Taxonomy

Public errors use stable machine codes from `contracts::ProblemCode`:

- `not_found`;
- `forbidden`;
- `invalid_request`;
- `invalid_state`;
- `version_conflict`;
- `lease_conflict`;
- `replay_rejected`;
- `dependency_unavailable`;
- `integrity_failure`;
- `internal_failure`.

Foreign and absent resources may intentionally map to the same `not_found`
response. Internal adapter/SDK errors never become public SDK-specific codes.
Human-readable detail is non-authoritative and must not contain secrets or PII.

## 3. Compatible v1 Changes

Generally compatible:

- add an optional response field;
- add a new endpoint with a unique operation ID;
- add a protobuf message;
- add a protobuf field with a new number;
- add a new problem code when old clients can treat it generically;
- add a new capability that is default-deny.

Compatibility still requires implementation and negative authorization tests.

## 4. Breaking v1 Changes

Forbidden without a new major root or governed migration:

- remove or rename an existing API path/operation ID;
- remove an existing schema property required by the baseline;
- remove/rename a protobuf message;
- remove, rename or reuse a protobuf field number;
- change the protobuf package;
- reinterpret assignment as authorization;
- change an opaque ID to email/name/path semantics;
- expose provider/runtime SDK error types as public contracts.

## 5. Baseline And Gate

`contracts/baseline/` contains the accepted v1 compatibility floor. The permanent
quality gate:

1. lints current OpenAPI/protobuf roots;
2. proves current contracts retain every baseline path/operation/message/field;
3. runs a deliberately breaking fixture and requires the checker to reject it.

Changing the baseline is a governed compatibility decision, not an automatic
side effect of editing current contracts. The PR must state why old clients are
safe or introduce a new major version/cutover plan.

## 6. Generated Code

Generated web/protobuf clients are build artifacts and do not own domain policy.
Frontend, Bridge and future CRM adapters consume versioned contracts; domain
crates remain independent of HTTP, protobuf runtime and provider SDK types.
