#!/usr/bin/env python3
"""Permanent PAS-2 mailbox Client-association architecture sentinel."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FRAGMENT = ROOT / "openapi/v1/fragments/mailbox-client-association.json"
CONTRACT = ROOT / "crates/control-plane-contract/src/mailbox_client_association_api.rs"
WORKER = ROOT / "apps/control-plane-worker/src/mailbox_client_associations.rs"
WORKER_ROOT = ROOT / "apps/control-plane-worker/src/lib.rs"
COMPOSITION = ROOT / "apps/control-plane-worker/src/mailbox_client_association_composition.rs"
FRONTEND_API = ROOT / "frontend/src/features/mailboxes/api.ts"
FRONTEND_UI = ROOT / "frontend/src/features/mailboxes/MailboxesWorkspace.tsx"
RUNTIME_GENERATOR = ROOT / "scripts/generate-frontend-openapi-runtime.py"
TRANSPORT = ROOT / "frontend/src/shared/api/transport.ts"
RUNTIME = ROOT / "frontend/src/shared/api/openapi-runtime.ts"
RETIRED_GENERATOR = ROOT / "scripts/generate-mailbox-client-association-contract.py"
RETIRED_EXPORTER = ROOT / "crates/control-plane-contract/src/bin/export_mailbox_client_association.rs"
RETIRED_FRONTEND_DTO = ROOT / "frontend/src/shared/api/generated/mailbox-client-association.ts"
EXPECTED_PATH = "/api/v1/tenants/{tenantId}/mailboxes/{bindingId}/client-association"


def require(text: str, needle: str, where: str) -> None:
    if needle not in text:
        raise SystemExit(f"{where}: required PAS-2 association evidence is missing: {needle}")


def reject(text: str, needle: str, where: str) -> None:
    if needle in text:
        raise SystemExit(f"{where}: forbidden PAS-2 association predecessor/leakage detected: {needle}")


def rust_struct_fields(text: str, name: str, *, public: bool) -> set[str]:
    match = re.search(rf"struct\s+{re.escape(name)}\s*\{{(?P<body>.*?)\n\}}", text, re.S)
    if match is None:
        raise SystemExit(f"Rust struct {name} is missing")
    prefix = r"pub\s+" if public else ""
    return set(re.findall(rf"^\s+{prefix}([a-z][a-z0-9_]*)\s*:", match.group("body"), re.M))


def parameter_refs(operation: dict[str, object]) -> set[str]:
    refs: set[str] = set()
    parameters = operation.get("parameters")
    if not isinstance(parameters, list):
        raise SystemExit("B4 operation parameters must be an array")
    for parameter in parameters:
        if isinstance(parameter, dict) and isinstance(parameter.get("$ref"), str):
            refs.add(parameter["$ref"])
    return refs


def main() -> int:
    for retired in [RETIRED_GENERATOR, RETIRED_EXPORTER, RETIRED_FRONTEND_DTO]:
        if retired.exists():
            raise SystemExit(f"retired PAS-2 association predecessor still exists: {retired.relative_to(ROOT)}")

    document = json.loads(FRAGMENT.read_text(encoding="utf-8"))
    paths = document.get("paths")
    if not isinstance(paths, dict) or set(paths) != {EXPECTED_PATH}:
        raise SystemExit("B4 public fragment must contain exactly the accepted association resource")
    resource = paths[EXPECTED_PATH]
    if not isinstance(resource, dict) or set(resource) != {"get", "post"}:
        raise SystemExit("B4 association resource must expose exactly GET and POST")

    get_operation = resource["get"]
    post_operation = resource["post"]
    if not isinstance(get_operation, dict) or get_operation.get("operationId") != "getMailboxClientAssociation":
        raise SystemExit("B4 GET operationId drifted")
    if not isinstance(post_operation, dict) or post_operation.get("operationId") != "changeMailboxClientAssociation":
        raise SystemExit("B4 POST operationId drifted")
    if "#/components/parameters/IdempotencyHeader" not in parameter_refs(post_operation):
        raise SystemExit("B4 mutation must preserve canonical Idempotency-Key ownership")

    schemas = document.get("components", {}).get("schemas", {})
    expected_schemas = {
        "ChangeMailboxClientAssociationRequestDto",
        "MailboxClientAssociationMutationReceiptDto",
        "MailboxClientAssociationProjectionDto",
    }
    if not isinstance(schemas, dict) or set(schemas) != expected_schemas:
        raise SystemExit("B4 fragment schema surface expanded or drifted")
    change = schemas["ChangeMailboxClientAssociationRequestDto"]
    if not isinstance(change, dict) or change.get("additionalProperties") is not False:
        raise SystemExit("B4 mutation request must fail closed on unknown fields")
    if set(change.get("required", [])) != {"clientId", "expectedRelationshipVersion"}:
        raise SystemExit("B4 mutation request required-field set drifted")
    properties = change.get("properties", {})
    if not isinstance(properties, dict) or "requestDigest" in properties:
        raise SystemExit("legacy browser requestDigest survived in canonical B4 OpenAPI")
    if properties.get("expectedRelationshipVersion", {}).get("minimum") != 0:
        raise SystemExit("B4 relationship version must preserve never-associated version zero")
    client_id = properties.get("clientId", {})
    variants = client_id.get("oneOf") if isinstance(client_id, dict) else None
    if not isinstance(variants, list) or {variant.get("type") for variant in variants if isinstance(variant, dict)} != {"string", "null"}:
        raise SystemExit("B4 clientId must remain explicit nullable string for bind/unbind")

    contract = CONTRACT.read_text(encoding="utf-8")
    contract_request_fields = rust_struct_fields(
        contract,
        "ChangeMailboxClientAssociationRequestDto",
        public=True,
    )
    if contract_request_fields != {"client_id", "expected_relationship_version"}:
        raise SystemExit(f"B4 canonical request field set drifted: {sorted(contract_request_fields)}")
    for needle in [
        'deny_unknown_fields',
        'deserialize_with = "required_nullable_string"',
        "MailboxClientAssociationMutationReceiptDto",
        "MailboxClientAssociationProjectionDto",
    ]:
        require(contract, needle, str(CONTRACT.relative_to(ROOT)))
    for forbidden in ["request_digest", "openapi_fragment", "secret_handle", "access_token", "provider_token", "profile_id"]:
        reject(contract.lower(), forbidden, str(CONTRACT.relative_to(ROOT)))

    worker = WORKER.read_text(encoding="utf-8")
    for needle in [
        "ChangeMailboxClientAssociationRequestDto",
        "MailboxClientAssociationMutationReceiptDto",
        "MailboxClientAssociationProjectionDto",
        "execute_mailbox_client_association",
        "get_mailbox_client_association",
        "command_evidence::from_request(request, actor, &body)",
    ]:
        require(worker, needle, str(WORKER.relative_to(ROOT)))
    for forbidden in [
        "struct ChangeMailboxClientAssociationRequest",
        "request_digest",
        "valid_digest",
        "serde_json::Value",
        "D1Database",
        "query!(",
        "mailbox_client_association_commands",
        "mailbox_client_association_state",
    ]:
        reject(worker, forbidden, str(WORKER.relative_to(ROOT)))

    composition = COMPOSITION.read_text(encoding="utf-8")
    require(
        composition,
        "D1MailboxClientAssociationApplicationRepository",
        str(COMPOSITION.relative_to(ROOT)),
    )
    require(composition, "D1_CATALOG_BINDING", str(COMPOSITION.relative_to(ROOT)))

    worker_root = WORKER_ROOT.read_text(encoding="utf-8")
    require(worker_root, "mod mailbox_client_associations;", str(WORKER_ROOT.relative_to(ROOT)))
    require(
        worker_root,
        "if mailbox_client_associations::is_client_association_path(&path)",
        str(WORKER_ROOT.relative_to(ROOT)),
    )

    runtime_generator = RUNTIME_GENERATOR.read_text(encoding="utf-8")
    for needle in [
        'OUTPUT = ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "operations.ts"',
        "validated canonical OpenAPI v1 compiler input",
        "invokeOperation",
        "UnexpectedStatusError",
        "ContractDecodeError",
    ]:
        require(runtime_generator, needle, str(RUNTIME_GENERATOR.relative_to(ROOT)))
    reject(runtime_generator, "requestDigest", str(RUNTIME_GENERATOR.relative_to(ROOT)))

    transport = TRANSPORT.read_text(encoding="utf-8")
    for failure in [
        "NetworkError",
        "TimeoutError",
        "AbortedError",
        "ResponseTooLargeError",
    ]:
        require(transport, failure, str(TRANSPORT.relative_to(ROOT)))
    for needle in ["executeTransport", "MAX_RESPONSE_BYTES", "credentials: 'same-origin'"]:
        require(transport, needle, str(TRANSPORT.relative_to(ROOT)))

    runtime = RUNTIME.read_text(encoding="utf-8")
    for failure in [
        "UnexpectedStatusError",
        "UnexpectedContentTypeError",
        "MalformedBodyError",
        "ContractDecodeError",
    ]:
        require(runtime, failure, str(RUNTIME.relative_to(ROOT)))
    require(runtime, "invokeOperation", str(RUNTIME.relative_to(ROOT)))

    frontend_api = FRONTEND_API.read_text(encoding="utf-8")
    for needle in [
        "changeMailboxClientAssociation as changeMailboxClientAssociationOperation",
        "getMailboxClientAssociation as getMailboxClientAssociationOperation",
        "from '../../shared/api/generated/operations'",
        "idempotencyKey: newIdempotencyKey()",
        "expectedRelationshipVersion",
        "listMailboxRelationshipOverview",
        "Promise.all",
    ]:
        require(frontend_api, needle, str(FRONTEND_API.relative_to(ROOT)))
    for forbidden in [EXPECTED_PATH, "requestDigest", "requestJson", "fetch(", "payload as"]:
        reject(frontend_api, forbidden, str(FRONTEND_API.relative_to(ROOT)))

    frontend_ui = FRONTEND_UI.read_text(encoding="utf-8")
    for needle in [
        "Assigned and unassigned mailboxes",
        "relationshipFilter",
        "UNASSIGNED",
        "ASSIGNED",
        "Exact Client ID",
        "Mailbox status",
        "Manage relationship",
        "Explicit mailbox → Client relationship",
        "association.clientId ?? 'Unassigned'",
        "association.relationshipVersion",
        "association.canManage",
        "association.mailboxExecutable",
        "Bind Client",
        "Rebind Client",
        "unbind Client",
        "profile assignment is not used as authorization",
        "Current relationship after the failed command",
    ]:
        require(frontend_ui, needle, str(FRONTEND_UI.relative_to(ROOT)))
    for forbidden in ["ProfileAssignment", "profileAssignmentId", "assignedProfileId"]:
        reject(frontend_ui, forbidden, str(FRONTEND_UI.relative_to(ROOT)))

    print("PAS-2 mailbox Client association canonical contract, generated runtime and predecessor-retirement evidence passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
