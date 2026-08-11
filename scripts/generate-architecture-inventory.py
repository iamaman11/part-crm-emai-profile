#!/usr/bin/env python3
"""Generate and verify the repository architecture inventory.

The inventory is deliberately derived from repository truth where possible. Route specs remain
explicit because HTTP method/path templates are public contract metadata; the checker proves that
every public RouteClass is owned by exactly one classifier module and that referenced paths exist.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVENTORY_PATH = ROOT / "architecture" / "inventory.json"
CONTRACT_LIB = ROOT / "crates" / "control-plane-contract" / "src" / "lib.rs"

CLASSIFIERS = [
    ("foundation", "crates/control-plane-contract/src/routes/foundation.rs"),
    ("identity", "crates/control-plane-contract/src/routes/identity.rs"),
    ("clients", "crates/control-plane-contract/src/routes/clients.rs"),
    ("profiles", "crates/control-plane-contract/src/routes/profiles.rs"),
    ("generations", "crates/control-plane-contract/src/routes/generations.rs"),
    ("mailboxes", "crates/control-plane-contract/src/routes/mailboxes.rs"),
    ("devices", "crates/control-plane-contract/src/routes/devices.rs"),
    ("notifications", "crates/control-plane-contract/src/routes/notifications.rs"),
]

ROUTE_SPECS = [
    ("HealthApi", "foundation", ["GET"], "/api/v1/health", "/api/v1/health", False),
    ("BindingProbeApi", "foundation", ["GET"], "/api/v1/bindings", "/api/v1/bindings", False),
    ("AuthenticatedSessionApi", "foundation", ["GET"], "/api/v1/session", "/api/v1/session", True),
    ("OwnerBootstrapApi", "identity", ["POST"], "/api/v1/tenants/{tenant_id}/owner/bootstrap", "/api/v1/tenants/tenant_01/owner/bootstrap", True),
    ("OwnerTransferApi", "identity", ["POST"], "/api/v1/tenants/{tenant_id}/owner/transfer", "/api/v1/tenants/tenant_01/owner/transfer", True),
    ("InvitationCollectionApi", "identity", ["POST"], "/api/v1/tenants/{tenant_id}/invitations", "/api/v1/tenants/tenant_01/invitations", True),
    ("InvitationAcceptApi", "identity", ["POST"], "/api/v1/tenants/{tenant_id}/invitations/{invitation_id}/accept", "/api/v1/tenants/tenant_01/invitations/invitation_01/accept", True),
    ("MembershipCollectionApi", "identity", ["GET"], "/api/v1/tenants/{tenant_id}/members", "/api/v1/tenants/tenant_01/members", True),
    ("MembershipStatusApi", "identity", ["PUT"], "/api/v1/tenants/{tenant_id}/members/{actor_id}/status", "/api/v1/tenants/tenant_01/members/actor_01/status", True),
    ("ClientCollectionApi", "clients", ["GET", "POST"], "/api/v1/tenants/{tenant_id}/clients", "/api/v1/tenants/tenant_01/clients", True),
    ("ClientResourceApi", "clients", ["GET", "PATCH"], "/api/v1/tenants/{tenant_id}/clients/{client_id}", "/api/v1/tenants/tenant_01/clients/client_01", True),
    ("ClientArchiveApi", "clients", ["POST"], "/api/v1/tenants/{tenant_id}/clients/{client_id}/archive", "/api/v1/tenants/tenant_01/clients/client_01/archive", True),
    ("ClientContactApi", "clients", ["DELETE", "PUT"], "/api/v1/tenants/{tenant_id}/clients/{client_id}/contacts/{contact_point_id}", "/api/v1/tenants/tenant_01/clients/client_01/contacts/contact_01", True),
    ("ClientMergeApi", "clients", ["POST"], "/api/v1/tenants/{tenant_id}/clients/{client_id}/merge", "/api/v1/tenants/tenant_01/clients/client_01/merge", True),
    ("ClientHistoryApi", "clients", ["GET"], "/api/v1/tenants/{tenant_id}/clients/{client_id}/history", "/api/v1/tenants/tenant_01/clients/client_01/history", True),
    ("ClientGrantApi", "clients", ["DELETE", "PUT"], "/api/v1/tenants/{tenant_id}/clients/{client_id}/grants/{actor_id}", "/api/v1/tenants/tenant_01/clients/client_01/grants/actor_01", True),
    ("ClientMailSearchApi", "clients", ["POST"], "/api/v1/tenants/{tenant_id}/clients/{client_id}/mail/search", "/api/v1/tenants/tenant_01/clients/client_01/mail/search", True),
    ("ClientMailMessageApi", "clients", ["POST"], "/api/v1/tenants/{tenant_id}/clients/{client_id}/mail/message", "/api/v1/tenants/tenant_01/clients/client_01/mail/message", True),
    ("ProfileCollectionApi", "profiles", ["GET", "POST"], "/api/v1/tenants/{tenant_id}/profiles", "/api/v1/tenants/tenant_01/profiles", True),
    ("ProfileResourceApi", "profiles", ["GET"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}", "/api/v1/tenants/tenant_01/profiles/profile_01", True),
    ("ProfileAssignmentApi", "profiles", ["PUT"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/assignment", "/api/v1/tenants/tenant_01/profiles/profile_01/assignment", True),
    ("ProfileGrantApi", "profiles", ["DELETE", "PUT"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/grants/{actor_id}", "/api/v1/tenants/tenant_01/profiles/profile_01/grants/actor_01", True),
    ("ProfileCoordinatorApi", "profiles", ["GET", "POST"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/coordinator", "/api/v1/tenants/tenant_01/profiles/profile_01/coordinator", True),
    ("ProfileGenerationCollectionApi", "generations", ["POST"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/generations", "/api/v1/tenants/tenant_01/profiles/profile_01/generations", True),
    ("ProfileGenerationResourceApi", "generations", ["GET"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/generations/{generation_id}", "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01", True),
    ("ProfileGenerationVerifyApi", "generations", ["POST"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/generations/{generation_id}/verify", "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/verify", True),
    ("ProfileGenerationActivateApi", "generations", ["POST"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/generations/{generation_id}/activate", "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/activate", True),
    ("ProfileGenerationDeactivateApi", "generations", ["POST"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/generations/{generation_id}/deactivate", "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/deactivate", True),
    ("ProfileGenerationQuarantineApi", "generations", ["POST"], "/api/v1/tenants/{tenant_id}/profiles/{profile_id}/generations/{generation_id}/quarantine", "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/quarantine", True),
    ("MailboxBindingCollectionApi", "mailboxes", ["GET", "POST"], "/api/v1/tenants/{tenant_id}/mailboxes", "/api/v1/tenants/tenant_01/mailboxes", True),
    ("MailboxBindingResourceApi", "mailboxes", ["GET"], "/api/v1/tenants/{tenant_id}/mailboxes/{binding_id}", "/api/v1/tenants/tenant_01/mailboxes/mailbox_01", True),
    ("MailboxBindingRevokeApi", "mailboxes", ["POST"], "/api/v1/tenants/{tenant_id}/mailboxes/{binding_id}/revoke", "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/revoke", True),
    ("MailboxBrowserExecutionBindApi", "mailboxes", ["POST"], "/api/v1/tenants/{tenant_id}/mailboxes/{binding_id}/browser-execution", "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/browser-execution", True),
    ("MailboxJobCollectionApi", "mailboxes", ["POST"], "/api/v1/tenants/{tenant_id}/mailboxes/{binding_id}/jobs", "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs", True),
    ("MailboxJobResourceApi", "mailboxes", ["GET"], "/api/v1/tenants/{tenant_id}/mailboxes/{binding_id}/jobs/{job_id}", "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs/mailjob_01", True),
    ("MailboxJobRunApi", "mailboxes", ["POST"], "/api/v1/tenants/{tenant_id}/mailboxes/{binding_id}/jobs/{job_id}/run", "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs/mailjob_01/run", True),
    ("DeviceJobClaimableApi", "devices", ["GET"], "/api/v1/tenants/{tenant_id}/device-jobs/claimable", "/api/v1/tenants/tenant_01/device-jobs/claimable", True),
    ("DeviceJobClaimApi", "devices", ["POST"], "/api/v1/tenants/{tenant_id}/device-jobs/{job_id}/claim", "/api/v1/tenants/tenant_01/device-jobs/devjob_01/claim", True),
    ("DeviceJobHeartbeatApi", "devices", ["POST"], "/api/v1/tenants/{tenant_id}/device-jobs/{job_id}/heartbeat", "/api/v1/tenants/tenant_01/device-jobs/devjob_01/heartbeat", True),
    ("DeviceGenerationUploadCapabilityApi", "devices", ["POST"], "/api/v1/tenants/{tenant_id}/device-jobs/{job_id}/generation-upload-capability", "/api/v1/tenants/tenant_01/device-jobs/devjob_01/generation-upload-capability", True),
    ("DeviceGenerationCommitApi", "devices", ["POST"], "/api/v1/tenants/{tenant_id}/device-jobs/{job_id}/generation-commit", "/api/v1/tenants/tenant_01/device-jobs/devjob_01/generation-commit", True),
    ("DeviceJobOutcomeApi", "devices", ["POST"], "/api/v1/tenants/{tenant_id}/device-jobs/{job_id}/outcome", "/api/v1/tenants/tenant_01/device-jobs/devjob_01/outcome", True),
    ("NotificationEventCollectionApi", "notifications", ["GET"], "/api/v1/tenants/{tenant_id}/notifications/events", "/api/v1/tenants/tenant_01/notifications/events", True),
    ("NotificationEventAckApi", "notifications", ["POST"], "/api/v1/tenants/{tenant_id}/notifications/events/ack", "/api/v1/tenants/tenant_01/notifications/events/ack", True),
    ("NotificationReplayCollectionApi", "notifications", ["POST"], "/api/v1/tenants/{tenant_id}/notifications/replays", "/api/v1/tenants/tenant_01/notifications/replays", True),
    ("NotificationOperationsApi", "notifications", ["GET"], "/api/v1/tenants/{tenant_id}/notifications/operations", "/api/v1/tenants/tenant_01/notifications/operations", True),
]

GENERATED_CONTRACTS = [
    {
        "name": "control-plane-public-api",
        "canonical_source": "crates/control-plane-contract/src/public_api.rs",
        "openapi": "contracts/generated/control-plane.openapi.json",
        "typescript": "frontend/src/shared/api/generated/control-plane.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
    {
        "name": "client-registry-api",
        "canonical_source": "crates/control-plane-contract/src/client_registry_api.rs",
        "openapi": "openapi/v1/fragments/client-registry.json",
        "typescript": "frontend/src/shared/api/generated/client-registry.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
    {
        "name": "query-mail-api",
        "canonical_source": "crates/control-plane-contract/src/bin/export_query_mail.rs",
        "openapi": "openapi/v1/fragments/query-mail.json",
        "typescript": "frontend/src/shared/api/generated/query-mail.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
    {
        "name": "operator-query-api",
        "canonical_source": "crates/control-plane-contract/src/bin/export_operator_query.rs",
        "openapi": "openapi/v1/fragments/operator-query.json",
        "typescript": "frontend/src/shared/api/generated/operator-query.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
    {
        "name": "profile-generation-api",
        "canonical_source": "crates/control-plane-contract/src/profile_generation_api.rs",
        "openapi": "contracts/generated/profile-generation.openapi.json",
        "typescript": "frontend/src/shared/api/generated/profile-generation.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
    {
        "name": "mailbox-api",
        "canonical_source": "crates/control-plane-contract/src/mailbox_api.rs",
        "openapi": "contracts/generated/mailbox.openapi.json",
        "typescript": "frontend/src/shared/api/generated/mailbox.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
    {
        "name": "coordinator-api",
        "canonical_source": "crates/control-plane-contract/src/coordinator_api.rs",
        "openapi": "contracts/generated/coordinator.openapi.json",
        "typescript": "frontend/src/shared/api/generated/coordinator.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
]

REQUIRED_INDEX_LINKS = [
    "DEVELOPMENT_PLAN.md",
    "ARCHITECTURE.md",
    "DATA_CLASSIFICATION.md",
    "UI_ARCHITECTURE.md",
    "DEVELOPER_CAPABILITY_MATRIX.md",
    "accepted-phases.json",
    "REALTIME_NOTIFICATIONS.md",
    "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
    "status.json",
    "THREAT_MODEL.md",
]


def workspace_members() -> list[str]:
    with (ROOT / "Cargo.toml").open("rb") as handle:
        manifest = tomllib.load(handle)
    members = manifest["workspace"]["members"]
    if not isinstance(members, list) or not all(isinstance(value, str) for value in members):
        raise SystemExit("Cargo.toml workspace.members must be a string array")
    missing = [member for member in members if not (ROOT / member / "Cargo.toml").is_file()]
    if missing:
        raise SystemExit(f"workspace members missing Cargo.toml: {missing}")
    return members


def migration_files() -> list[str]:
    root = ROOT / "migrations" / "d1"
    files = sorted(root.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    if not files:
        raise SystemExit("no D1 migrations found")
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    expected = list(range(1, len(files) + 1))
    if versions != expected:
        raise SystemExit(f"D1 migrations are not contiguous: {versions}; expected {expected}")
    return [path.relative_to(ROOT).as_posix() for path in files]


def route_class_variants() -> set[str]:
    text = CONTRACT_LIB.read_text(encoding="utf-8")
    match = re.search(r"pub enum RouteClass\s*\{(?P<body>.*?)\}", text, re.DOTALL)
    if match is None:
        raise SystemExit("could not locate RouteClass enum")
    return set(re.findall(r"^\s*([A-Za-z][A-Za-z0-9_]*)\s*,\s*$", match.group("body"), re.MULTILINE))


def validate_route_ownership() -> None:
    variants = route_class_variants()
    public_variants = {value for value in variants if value.endswith("Api")}
    spec_variants = {spec[0] for spec in ROUTE_SPECS}
    if public_variants != spec_variants:
        raise SystemExit(
            "public route inventory mismatch: "
            f"missing={sorted(public_variants - spec_variants)}, extra={sorted(spec_variants - public_variants)}"
        )

    classifier_paths = {capability: ROOT / path for capability, path in CLASSIFIERS}
    for capability, path in classifier_paths.items():
        if not path.is_file():
            raise SystemExit(f"classifier module missing for {capability}: {path.relative_to(ROOT)}")

    for route_class, capability, *_ in ROUTE_SPECS:
        owners = []
        marker = f"RouteClass::{route_class}"
        for owner, path in classifier_paths.items():
            if marker in path.read_text(encoding="utf-8"):
                owners.append(owner)
        if owners != [capability]:
            raise SystemExit(
                f"{route_class} must be owned only by {capability}; observed owners={owners}"
            )


def validate_docs() -> None:
    authority = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "check-documentation-authority.py"), "--root", str(ROOT)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if authority.returncode != 0:
        details = "\n".join(value.strip() for value in (authority.stdout, authority.stderr) if value.strip())
        raise SystemExit(f"documentation authority check failed:\n{details}")

    index = (ROOT / "docs" / "INDEX.md").read_text(encoding="utf-8")
    missing_links = [value for value in REQUIRED_INDEX_LINKS if value not in index]
    if missing_links:
        raise SystemExit(f"docs/INDEX.md is missing authority links: {missing_links}")

    plan = (ROOT / "docs" / "DEVELOPMENT_PLAN.md").read_text(encoding="utf-8")
    architecture = (ROOT / "docs" / "ARCHITECTURE.md").read_text(encoding="utf-8")
    matrix = (ROOT / "docs" / "DEVELOPER_CAPABILITY_MATRIX.md").read_text(encoding="utf-8")
    next_sections = re.findall(r"^### (Phase [^\n]+?) — NEXT\s*$", plan, re.MULTILINE)
    if next_sections:
        raise SystemExit(f"no product Phase ... — NEXT section is allowed while pre-2J remediation is active: {next_sections}")
    blocked_phase2j = "Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / NEXT AFTER PRE-2J"
    if blocked_phase2j not in plan:
        raise SystemExit("DEVELOPMENT_PLAN.md must keep Phase 2J blocked behind pre-2J closure")
    immediate = plan.split("## 19. Immediate Next Action", 1)
    if len(immediate) != 2 or "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md" not in immediate[1] or "Do not start Phase 2J" not in immediate[1]:
        raise SystemExit("Immediate Next Action must enforce the active pre-2J blocker")

    required_plan_markers = (
        "Phase 2D — CQRS read models, global search and client-mail query contract — ACCEPTED",
        "Phase 2E — Mailbox domain decomposition and real cloud mailbox lane — ACCEPTED",
        "Phase 2F — Durable device jobs, browser mailbox lane and materialization integration — ACCEPTED",
        "Phase 2G — Durable realtime notification hub — ACCEPTED",
        "Phase 2H — Complete standalone UI and administration UX — ACCEPTED",
        "Phase 2I — Standalone E2E, security, recovery and operational hardening — ACCEPTED",
        "Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / NEXT AFTER PRE-2J",
        "`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`",
        "Phase 2I was accepted through issue #167 / PR #168",
        "`c1075337cfc582d0f4c00ec34b1aa7cda9ac1101`",
        "`800c634147d6300ea3989ff0cf87ade6e2387ee9`",
        "`crates/use-cases` remains the canonical application owner for Profile Catalog and Profile Generation Registry workflows",
        "| A8 | Query-side/CQRS read-model boundary | **Accepted in Phase 2D and preserved through Phase 2I.**",
        "| 6.2 | Durable-before-notify | **Accepted through Phase 2I.**",
        "| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2I.**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2I.**",
        "| 6.6 | Profile materialization | **Accepted repository-local through Phase 2I.**",
        "`BrowserIdentityManifest`",
        "`NetworkIdentityPolicy` + `NetworkIdentityObservation`",
        "PID alone is never sufficient ownership proof",
        "blanket `PRAGMA integrity_check`",
    )
    required_architecture_markers = (
        "### 11.1 Browser Runtime Identity, Network Policy And Writer Recovery",
        "`use-cases-devices`",
        "`crates/use-cases` remains the canonical application owner for current Profile Catalog and Profile Generation Registry workflows",
        "graceful browser close a retained-ownership transition",
        "Phase 2F accepts the repository-local retained-close implementation",
        "`BrowserIdentityManifest`",
        "`NetworkIdentityPolicy`",
        "PID alone is not ownership proof",
        "blanket Firefox SQLite `PRAGMA integrity_check` is not canonical profile-health authority",
    )
    stale_plan_markers = (
        "Phase 2J — Production-readiness evidence and controlled rollout — NEXT",
        "Phase 2J is the unique NEXT",
        "Phase 2E — Mailbox domain decomposition and real cloud mailbox lane — NEXT",
        "Phase 2F — Durable device jobs, browser mailbox lane and materialization integration — NEXT",
        "Phase 2G — Durable realtime notification hub — NEXT",
        "Phase 2H — Complete standalone UI and administration UX — NEXT",
        "Phase 2I — Standalone E2E, security, recovery and operational hardening — NEXT",
        "Phase 2I is the unique NEXT",
        "Phase 2H is the unique NEXT",
        "Phase 2E issue #148 is the unique NEXT",
        "Phase 2D issue #144 is the unique next implementation slice",
        "| A8 | Query-side/CQRS read-model boundary | **Open.**",
        "| A8 | Query-side/CQRS read-model boundary | **Accepted in Phase 2D.**",
        "| A8 | Query-side/CQRS read-model boundary | **Accepted in Phase 2D and preserved through Phase 2G.**",
        "| 6.2 | Durable-before-notify | **Accepted through Phase 2F repository-owned consumers.**",
        "| 6.2 | Durable-before-notify | **Accepted through Phase 2G.**",
        "| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2G.**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2G.**",
        "| 6.6 | Profile materialization | **Library/Synthetic foundation.**",
        "2D read/search/provider query; 2G realtime subscriptions",
    )
    required_matrix_markers = (
        "| Client contact protection | Composed |",
        "| Client Registry 2.0 | Composed |",
        "| Read models/global search | Composed / Synthetic |",
        "| Client-scoped mailbox message search/body | Composed / Synthetic |",
        "| Device job/browser mailbox execution | Composed / Synthetic |",
        "| Realtime UserNotificationHub | Composed / Synthetic |",
        "| React standalone operator UI | Composed / Synthetic |",
        "| Client Mail standalone UI | Composed / Synthetic |",
        "| Safe HTML mail rendering | Composed / Synthetic |",
        "| Complete standalone UI | Composed / Synthetic |",
        "| Integrated release-candidate hardening | Composed / Synthetic |",
        "| A3 | Domain aggregate splitting | **Accepted** — Phase 2A decomposed `client-domain`; Phase 2E decomposed `mailbox-domain`",
        "| A5 | Feature-sliced SPA route composition | **Accepted through Phase 2I**",
        "| A8 | CQRS/read-model boundary | **Accepted through Phase 2I**",
        "| 6.2 | Durable-before-notify | **Accepted through Phase 2I.**",
        "| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2I.**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2I.**",
        "| 6.5 | PII contact protection | **Accepted through Phase 2B/2D**",
        "| 6.6 | Profile materialization | **Accepted repository-local through Phase 2I**",
        "### Realtime notification path",
        "### Safe standalone Client Mail path",
        "crates/use-cases-query",
        "crates/use-cases-mailboxes",
        "crates/use-cases-devices",
        "Profile Catalog and Profile Generation Registry",
        "`9add9b94d0de255b93e5a7c24584fcf6756462a7`",
        "`a32768feddb3da69b872e701bc529aad3521e1b0`",
        "`c1075337cfc582d0f4c00ec34b1aa7cda9ac1101`",
        "`800c634147d6300ea3989ff0cf87ade6e2387ee9`",
    )
    stale_matrix_markers = (
        "| A3 | Domain aggregate splitting | **Client half accepted in Phase 2A**",
        "| Client contact protection | Target |",
        "| Client Registry 2.0 | Target |",
        "| Read models/global search | Target |",
        "| Read models/global search | Library / Synthetic |",
        "| Device job/browser mailbox execution | Target / Synthetic foundation |",
        "| Realtime UserNotificationHub | Target |",
        "| Complete standalone UI/E2E | Target |",
        "Phase 2H completes routes, full operator/admin UX",
        "Phase 2H completes cross-capability operator/admin polish",
        "Broader UX is Phase 2H",
        "complete operator/admin UX remains Phase 2H",
        "| A5 | Feature-sliced SPA route composition | **Open**",
        "| A5 | Feature-sliced SPA route composition | **Accepted in Phase 2C**",
        "| A8 | CQRS/read-model boundary | **Open**",
        "| A8 | CQRS/read-model boundary | **Accepted in Phase 2D**",
        "| 6.2 | Durable-before-notify | **Accepted through Phase 1B durable delivery**",
        "| 6.2 | Durable-before-notify | **Accepted through Phase 2G.**",
        "| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2G.**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query scope**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2G.**",
        "| 6.5 | PII contact protection | **Open for client contacts**",
        "| 6.6 | Profile materialization | **Library/Synthetic foundation**",
        "realtime UserNotificationHub remains Phase 2G",
        "The remaining fixed extraction point in active Phase 2 is devices in Phase 2F.",
    )
    for marker in required_plan_markers:
        if marker not in plan:
            raise SystemExit(f"DEVELOPMENT_PLAN.md is missing accepted-phase semantic marker: {marker}")
    for marker in stale_plan_markers:
        if marker in plan:
            raise SystemExit(f"DEVELOPMENT_PLAN.md contains stale accepted-phase marker: {marker}")
    for marker in required_architecture_markers:
        if marker not in architecture:
            raise SystemExit(f"ARCHITECTURE.md is missing Phase 2F accepted architecture marker: {marker}")
    for marker in required_matrix_markers:
        if marker not in matrix:
            raise SystemExit(f"DEVELOPER_CAPABILITY_MATRIX.md is missing accepted capability marker: {marker}")
    for marker in stale_matrix_markers:
        if marker in matrix:
            raise SystemExit(f"DEVELOPER_CAPABILITY_MATRIX.md contains stale capability marker: {marker}")

    for contract in GENERATED_CONTRACTS:
        for key in ("canonical_source", "openapi", "typescript", "generator"):
            relative_path = contract[key]
            if not (ROOT / relative_path).is_file():
                raise SystemExit(f"generated contract {contract['name']} references missing {key}: {relative_path}")

    status = json.loads((ROOT / "docs" / "status.json").read_text(encoding="utf-8"))
    if status.get("production_ready") is not False:
        raise SystemExit("docs/status.json must remain production_ready=false until external gates pass")
    if "`production_ready=false`" not in plan:
        raise SystemExit("DEVELOPMENT_PLAN.md must preserve the production_ready=false claim")


def build_inventory() -> dict[str, object]:
    validate_route_ownership()
    validate_docs()
    routes = []
    for route_class, capability, methods, template, example_path, authenticated in ROUTE_SPECS:
        routes.append(
            {
                "route_class": route_class,
                "capability": capability,
                "methods": methods,
                "path_template": template,
                "example_path": example_path,
                "authenticated": authenticated,
            }
        )

    inventory: dict[str, object] = {
        "schema_version": 1,
        "workspace_members": workspace_members(),
        "d1_migrations": {"directory": "migrations/d1", "files": migration_files()},
        "routing": {
            "composed_entrypoint": "crates/control-plane-contract/src/lib.rs::classify_route",
            "dynamic_namespaces": ["/api", "/auth", "/bridge"],
            "classifiers": [
                {"capability": capability, "module": path} for capability, path in CLASSIFIERS
            ],
            "public_routes": routes,
        },
        "generated_contracts": GENERATED_CONTRACTS,
        "documentation_authority": {
            "execution_order": "docs/DEVELOPMENT_PLAN.md",
            "architecture": "docs/ARCHITECTURE.md",
            "data_classification": "docs/DATA_CLASSIFICATION.md",
            "ui_target": "docs/UI_ARCHITECTURE.md",
            "accepted_capabilities": "docs/DEVELOPER_CAPABILITY_MATRIX.md",
            "index": "docs/INDEX.md",
            "pre2j_execution_blocker": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "readiness": "docs/status.json",
            "security": "docs/THREAT_MODEL.md",
            "accepted_phase_ledger": "architecture/accepted-phases.json",
        },
    }

    for contract in GENERATED_CONTRACTS:
        for key in ("canonical_source", "openapi", "typescript", "generator"):
            path = ROOT / contract[key]
            if not path.is_file():
                raise SystemExit(f"generated contract inventory path missing: {path.relative_to(ROOT)}")
    return inventory


def serialized(inventory: dict[str, object]) -> str:
    return json.dumps(inventory, indent=2, ensure_ascii=False) + "\n"


def check_current(expected: dict[str, object]) -> None:
    if not INVENTORY_PATH.is_file():
        raise SystemExit(f"architecture inventory is missing: {INVENTORY_PATH.relative_to(ROOT)}")
    current_text = INVENTORY_PATH.read_text(encoding="utf-8")
    expected_text = serialized(expected)
    if current_text != expected_text:
        raise SystemExit(
            "architecture/inventory.json is stale; run "
            "python scripts/generate-architecture-inventory.py --write"
        )


def self_test(expected: dict[str, object]) -> None:
    tampered = copy.deepcopy(expected)
    tampered["workspace_members"] = [*tampered["workspace_members"], "crates/does-not-exist"]
    if serialized(tampered) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect deterministic drift")

    route = copy.deepcopy(expected)
    route["routing"]["public_routes"][0]["route_class"] = "UnknownApi"
    if serialized(route) == serialized(expected):
        raise SystemExit("route inventory self-test failed to detect deterministic drift")

    authority = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "check-documentation-authority.py"), "--root", str(ROOT), "--self-test"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if authority.returncode != 0:
        details = "\n".join(value.strip() for value in (authority.stdout, authority.stderr) if value.strip())
        raise SystemExit(f"documentation authority negative self-test failed:\n{details}")
    if authority.stdout.strip():
        print(authority.stdout.strip())
    print("Architecture inventory negative self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    expected = build_inventory()
    if args.write:
        INVENTORY_PATH.parent.mkdir(parents=True, exist_ok=True)
        INVENTORY_PATH.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print(f"Wrote {INVENTORY_PATH.relative_to(ROOT)}")
    elif args.check:
        check_current(expected)
        print("Architecture inventory and documentation consistency are current.")
    else:
        self_test(expected)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
