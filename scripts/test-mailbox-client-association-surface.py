#!/usr/bin/env python3
"""Permanent B4 contract/transport/frontend evidence for mailbox Client association."""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FRAGMENT = ROOT / "openapi/v1/fragments/mailbox-client-association.json"
CONTRACT = ROOT / "crates/control-plane-contract/src/mailbox_client_association_api.rs"
WORKER = ROOT / "apps/control-plane-worker/src/mailbox_client_associations.rs"
WORKER_ROOT = ROOT / "apps/control-plane-worker/src/lib.rs"
COMPOSITION = ROOT / "apps/control-plane-worker/src/mailbox_client_association_composition.rs"
FRONTEND_API = ROOT / "frontend/src/features/mailboxes/api.ts"
FRONTEND_UI = ROOT / "frontend/src/features/mailboxes/MailboxesWorkspace.tsx"
EXPECTED_PATH = "/api/v1/tenants/{tenantId}/mailboxes/{bindingId}/client-association"


def require(text: str, needle: str, where: str) -> None:
    if needle not in text:
        raise SystemExit(f"{where}: required B4 evidence is missing: {needle}")


def reject(text: str, needle: str, where: str) -> None:
    if needle in text:
        raise SystemExit(f"{where}: forbidden B4 boundary leakage detected: {needle}")


def rust_struct_fields(text: str, name: str, *, public: bool) -> set[str]:
    match = re.search(rf"struct\s+{re.escape(name)}\s*\{{(?P<body>.*?)\n\}}", text, re.S)
    if match is None:
        raise SystemExit(f"Rust struct {name} is missing")
    prefix = r"pub\s+" if public else ""
    return set(re.findall(rf"^\s+{prefix}([a-z][a-z0-9_]*)\s*:", match.group("body"), re.M))


def main() -> int:
    subprocess.run(
        ["python", "scripts/generate-mailbox-client-association-contract.py", "--check"],
        cwd=ROOT,
        check=True,
    )

    document = json.loads(FRAGMENT.read_text(encoding="utf-8"))
    paths = document.get("paths")
    if not isinstance(paths, dict) or set(paths) != {EXPECTED_PATH}:
        raise SystemExit("B4 public fragment must contain exactly the one accepted association resource")
    resource = paths[EXPECTED_PATH]
    if not isinstance(resource, dict) or set(resource) != {"get", "post"}:
        raise SystemExit("B4 association resource must expose exactly GET and POST")
    if resource["get"].get("operationId") != "getMailboxClientAssociation":
        raise SystemExit("B4 GET operationId drifted")
    if resource["post"].get("operationId") != "changeMailboxClientAssociation":
        raise SystemExit("B4 POST operationId drifted")

    schemas = document.get("components", {}).get("schemas", {})
    expected_schemas = {
        "ChangeMailboxClientAssociationRequestDto",
        "MailboxClientAssociationMutationReceiptDto",
        "MailboxClientAssociationProjectionDto",
    }
    if not isinstance(schemas, dict) or set(schemas) != expected_schemas:
        raise SystemExit("B4 fragment schema surface expanded or drifted")
    change = schemas["ChangeMailboxClientAssociationRequestDto"]
    if "clientId" not in change.get("required", []):
        raise SystemExit("B4 clientId must remain explicit even when null means unbind")
    if change.get("properties", {}).get("expectedRelationshipVersion", {}).get("minimum") != 0:
        raise SystemExit("B4 relationship version must preserve never-associated version zero")

    contract = CONTRACT.read_text(encoding="utf-8")
    contract_request_fields = rust_struct_fields(
        contract,
        "ChangeMailboxClientAssociationRequestDto",
        public=True,
    )
    if contract_request_fields != {"client_id", "expected_relationship_version", "request_digest"}:
        raise SystemExit(f"B4 canonical request field set drifted: {sorted(contract_request_fields)}")
    require(
        contract,
        'deserialize_with = "required_nullable_string"',
        str(CONTRACT.relative_to(ROOT)),
    )
    for forbidden in ["secret_handle", "access_token", "provider_token", "profile_id"]:
        reject(contract.lower(), forbidden, str(CONTRACT.relative_to(ROOT)))

    worker = WORKER.read_text(encoding="utf-8")
    worker_request_fields = rust_struct_fields(
        worker,
        "ChangeMailboxClientAssociationRequest",
        public=False,
    )
    if worker_request_fields != {"client_id", "expected_relationship_version", "request_digest"}:
        raise SystemExit(f"B4 Worker request field set drifted: {sorted(worker_request_fields)}")
    require(worker, "execute_mailbox_client_association", str(WORKER.relative_to(ROOT)))
    require(worker, "get_mailbox_client_association", str(WORKER.relative_to(ROOT)))
    require(worker, "client_id: Value", str(WORKER.relative_to(ROOT)))
    require(worker, "Value::Null => Ok(None)", str(WORKER.relative_to(ROOT)))
    for forbidden in ["D1Database", "query!(", "mailbox_client_association_commands", "mailbox_client_association_state"]:
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

    frontend_api = FRONTEND_API.read_text(encoding="utf-8")
    for needle in [
        "getMailboxClientAssociation",
        "changeMailboxClientAssociation",
        "expectedRelationshipVersion",
        "listMailboxRelationshipOverview",
        "Promise.all",
    ]:
        require(frontend_api, needle, str(FRONTEND_API.relative_to(ROOT)))

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

    print("B4 mailbox Client association contract, thin transport and permission-aware UX evidence passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
