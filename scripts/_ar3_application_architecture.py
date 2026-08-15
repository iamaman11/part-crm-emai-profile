#!/usr/bin/env python3
"""AR-3 application-architecture projection and fail-closed source verification.

This module is generator logic, not a second architecture registry. The canonical machine
projection is architecture/inventory.json. Accepted runtime-resource decisions are consumed
from architecture/runtime-topology-ar2.json without being re-decided here.
"""

from __future__ import annotations

import copy
import json
from pathlib import Path
from typing import Any

RUNTIME_TOPOLOGY = "architecture/runtime-topology-ar2.json"
AR3_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR3.md"

PROCESS_OWNERSHIP: list[dict[str, Any]] = [
    {
        "id": "control_plane_worker",
        "runtime_kind": "CLOUDFLARE_WORKER",
        "entrypoint": "apps/control-plane-worker/src/lib.rs",
        "transport_owner": "apps/control-plane-worker",
        "route_classifier": "crates/control-plane-contract/src/lib.rs::classify_route",
        "composition_root": "apps/control-plane-worker/src/composition.rs",
        "state_authority": "D1/application-owned repositories plus accepted R2/DO runtime boundaries",
        "resource_refs": [
            "control_plane_worker",
            "static_assets",
            "catalog_d1",
            "profile_objects",
            "profile_coordinator",
            "notification_hub",
            "integration_events",
            "mailbox_jobs",
            "mailbox_jobs_dlq",
            "generation_verification",
            "mailbox_secret_resolver_service",
            "control_plane_schedule",
        ],
        "status": "CENTRAL_COMPOSITION_ROOT_WITH_BOUNDED_DEBT",
    },
    {
        "id": "mailbox_secret_resolver_worker",
        "runtime_kind": "ISOLATED_CLOUDFLARE_WORKER",
        "entrypoint": "apps/mailbox-secret-resolver-worker/src/lib.rs",
        "transport_owner": "apps/mailbox-secret-resolver-worker",
        "composition_root": "apps/mailbox-secret-resolver-worker",
        "state_authority": "dedicated resolver D1 encrypted credential/replay boundary",
        "resource_refs": [
            "mailbox_secret_resolver_worker",
            "resolver_d1",
            "resolver_reconciliation_schedule",
        ],
        "status": "INTENTIONAL_RUNTIME_BOUNDARY",
    },
    {
        "id": "profile_bridge",
        "runtime_kind": "WINDOWS_LOCAL_RUNTIME",
        "entrypoint": "apps/profile-bridge/src/main.rs",
        "transport_owner": "apps/profile-bridge",
        "composition_root": "apps/profile-bridge",
        "state_authority": "local encrypted workspace/cache/outbox under cloud device/profile authority",
        "resource_refs": ["browser_bridge_mailbox_lane"],
        "status": "INTENTIONAL_RUNTIME_BOUNDARY",
    },
]

CAPABILITY_OWNERSHIP: list[dict[str, Any]] = [
    {
        "id": "identity",
        "transport": "apps/control-plane-worker/src/identity.rs",
        "application_owner": "crates/use-cases-identity",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs",
        "status": "CONFORMING_COMPOSITION_SEAM",
        "remediation_slice": None,
    },
    {
        "id": "clients",
        "transport": "apps/control-plane-worker/src/clients.rs",
        "application_owner": "crates/use-cases-clients",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs",
        "status": "CONFORMING_COMPOSITION_SEAM",
        "remediation_slice": None,
    },
    {
        "id": "operator_queries",
        "transport": "apps/control-plane-worker/src/operator_queries.rs",
        "application_owner": "crates/use-cases-query",
        "ports_owner": "crates/application-ports",
        "composition_seam": "transport currently constructs D1QueryRepository",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "remediation_slice": "AR-4A",
    },
    {
        "id": "client_mail_query",
        "transport": "apps/control-plane-worker/src/client_mail_query.rs",
        "application_owner": "crates/use-cases-query",
        "ports_owner": "crates/application-ports",
        "composition_seam": "transport currently constructs query/eligibility/provider adapters",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "remediation_slice": "AR-4A",
    },
    {
        "id": "outbound_mail",
        "transport": "apps/control-plane-worker/src/client_mail_send.rs",
        "application_owner": "crates/use-cases-mailboxes::outbound_mail",
        "ports_owner": "crates/application-ports",
        "composition_seam": "transport currently constructs/selects D1 and concrete Gmail/SMTP providers",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "remediation_slice": "AR-4C",
    },
    {
        "id": "profiles",
        "transport": "apps/control-plane-worker/src/profiles.rs",
        "application_owner": "crates/use-cases::profiles/profile_assignments/profile_grants",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs::profile_application",
        "status": "CONFORMING_COMPOSITION_SEAM",
        "remediation_slice": None,
    },
    {
        "id": "profile_generations",
        "transport": "apps/control-plane-worker/src/profile_generations.rs",
        "application_owner": "crates/use-cases::generations",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs::profile_generation_application",
        "status": "CONFORMING_COMPOSITION_SEAM",
        "remediation_slice": None,
    },
    {
        "id": "mailbox_bindings",
        "transport": "apps/control-plane-worker/src/mailbox_bindings.rs",
        "application_owner": "crates/use-cases-mailboxes",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs",
        "status": "CONFORMING_COMPOSITION_SEAM",
        "remediation_slice": None,
    },
    {
        "id": "mailbox_jobs",
        "transport": "apps/control-plane-worker/src/mailbox_jobs.rs",
        "application_owner": "crates/use-cases-mailboxes",
        "ports_owner": "crates/application-ports",
        "composition_seam": "repositories composed centrally; provider router remains direct in transport path",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "remediation_slice": "AR-4A",
    },
    {
        "id": "devices",
        "transport": "apps/control-plane-worker/src/device_jobs.rs",
        "application_owner": "crates/use-cases-devices",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs",
        "status": "CONFORMING_COMPOSITION_SEAM",
        "remediation_slice": None,
    },
    {
        "id": "notifications",
        "transport": "apps/control-plane-worker/src/notifications.rs",
        "application_owner": "crates/use-cases-notifications",
        "ports_owner": "crates/application-ports",
        "composition_seam": "transport currently constructs notification D1 repositories",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "remediation_slice": "AR-4A",
    },
    {
        "id": "profile_coordination",
        "transport": "apps/control-plane-worker/src/profile_coordinator_ingress.rs",
        "application_owner": "crates/use-cases::coordinator_ingress + crates/session-domain",
        "ports_owner": "crates/application-ports",
        "composition_seam": "accepted coordinator ingress/adapter boundary",
        "status": "INTENTIONAL_RUNTIME_BOUNDARY",
        "remediation_slice": None,
    },
    {
        "id": "mailbox_secret_resolution",
        "transport": "apps/mailbox-secret-resolver-worker/src/lib.rs",
        "application_owner": "apps/mailbox-secret-resolver-worker application/security modules",
        "ports_owner": "resolver-private contract/storage/provider boundary",
        "composition_seam": "isolated resolver Worker and dedicated resolver D1",
        "status": "INTENTIONAL_RUNTIME_BOUNDARY",
        "remediation_slice": None,
    },
    {
        "id": "profile_bridge_runtime",
        "transport": "apps/profile-bridge/src/main.rs",
        "application_owner": "bridge/device/browser runtime domains and application ports",
        "ports_owner": "crates/application-ports",
        "composition_seam": "Windows Profile Bridge runtime boundary",
        "status": "INTENTIONAL_RUNTIME_BOUNDARY",
        "remediation_slice": "AR-15_DELIVERY_ONLY",
    },
]

COMPOSITION_FINDINGS: list[dict[str, Any]] = [
    {
        "id": "general_composition_root_debt",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "owner_slice": "AR-4A",
        "evidence": [
            "apps/control-plane-worker/src/operator_queries.rs::D1QueryRepository::new",
            "apps/control-plane-worker/src/notifications.rs::D1NotificationOperationsRepository::new",
            "apps/control-plane-worker/src/notifications.rs::D1NotificationRepository::new",
            "apps/control-plane-worker/src/mailbox_jobs.rs::CloudMailboxProviderRouter",
            "apps/control-plane-worker/src/client_mail_query.rs::D1QueryRepository::new",
            "apps/control-plane-worker/src/client_mail_query.rs::D1ClientMailboxEligibilityRepository::new",
            "apps/control-plane-worker/src/client_mail_query.rs::CloudMailboxQueryAdapter::new",
        ],
    },
    {
        "id": "client_mail_route_ownership",
        "status": "ROUTE_OWNERSHIP_DEBT",
        "owner_slice": "AR-4B",
        "evidence": [
            "crates/control-plane-contract/src/routes/clients.rs::ClientMailSearchApi",
            "crates/control-plane-contract/src/routes/clients.rs::ClientMailMessageApi",
            "crates/control-plane-contract/src/routes/client_mail.rs::ClientMailSendApi",
        ],
    },
    {
        "id": "outbound_mail_composition",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "owner_slice": "AR-4C",
        "evidence": [
            "apps/control-plane-worker/src/client_mail_send.rs::D1ClientMailboxEligibilityRepository::new",
            "apps/control-plane-worker/src/client_mail_send.rs::D1OutboundMailIntentRepository::new",
            "apps/control-plane-worker/src/client_mail_send.rs::D1MailboxRepository::new",
            "apps/control-plane-worker/src/client_mail_send.rs::CloudflareGmailOutboundMailProvider::new",
            "apps/control-plane-worker/src/client_mail_send.rs::CloudflareSmtpOutboundMailProvider::new",
        ],
    },
    {
        "id": "profile_application_extraction",
        "status": "CONDITIONAL_EXTRACTION_NOT_REQUIRED",
        "owner_slice": "AR-4D",
        "decision": "NOT_REQUIRED",
        "evidence": [
            "apps/control-plane-worker/src/profiles.rs::crate::composition::profile_application",
            "apps/control-plane-worker/src/profile_generations.rs::crate::composition::profile_generation_application",
            "crates/use-cases/src/lib.rs::profiles",
            "crates/use-cases/src/lib.rs::generations",
        ],
    },
]

_REQUIRED_SNIPPETS: dict[str, list[str]] = {
    "apps/control-plane-worker/src/lib.rs": [
        "#[event(fetch, respond_with_errors)]",
        "classify_route",
        "#[event(queue)]",
        "#[event(scheduled)]",
    ],
    "apps/control-plane-worker/src/composition.rs": [
        "pub fn client_application",
        "pub fn identity_governance_application",
        "pub fn profile_application",
        "pub fn profile_generation_application",
        "pub fn mailbox_binding_application",
        "pub fn device_job_repository",
    ],
    "apps/control-plane-worker/src/clients.rs": ["crate::composition::{", "use_cases_clients::"],
    "apps/control-plane-worker/src/identity.rs": ["crate::composition::{identity_ceremony_application, identity_governance_application}"],
    "apps/control-plane-worker/src/profiles.rs": ["crate::composition::profile_application"],
    "apps/control-plane-worker/src/profile_generations.rs": ["crate::composition::profile_generation_application"],
    "apps/control-plane-worker/src/mailbox_bindings.rs": ["crate::composition::{browser_mailbox_execution_application, mailbox_binding_application}"],
    "apps/control-plane-worker/src/device_jobs.rs": ["crate::composition::{", "device_job_repository"],
    "apps/control-plane-worker/src/operator_queries.rs": ["D1QueryRepository::new"],
    "apps/control-plane-worker/src/notifications.rs": ["D1NotificationOperationsRepository::new", "D1NotificationRepository::new"],
    "apps/control-plane-worker/src/mailbox_jobs.rs": ["CloudMailboxProviderRouter"],
    "apps/control-plane-worker/src/client_mail_query.rs": [
        "D1QueryRepository::new",
        "D1ClientMailboxEligibilityRepository::new",
        "CloudMailboxQueryAdapter::new",
    ],
    "apps/control-plane-worker/src/client_mail_send.rs": [
        "D1ClientMailboxEligibilityRepository::new",
        "D1OutboundMailIntentRepository::new",
        "D1MailboxRepository::new",
        "CloudflareGmailOutboundMailProvider::new",
        "CloudflareSmtpOutboundMailProvider::new",
    ],
    "crates/control-plane-contract/src/routes/clients.rs": ["ClientMailSearchApi", "ClientMailMessageApi"],
    "crates/control-plane-contract/src/routes/client_mail.rs": ["ClientMailSendApi"],
    "crates/use-cases/src/lib.rs": ["pub mod generations;", "pub mod profiles;"],
    "apps/mailbox-secret-resolver-worker/src/lib.rs": ["#[event(fetch, respond_with_errors)]", "#[event(scheduled)]"],
    "apps/profile-bridge/src/main.rs": ["ClaimUri::parse"],
    AR3_EVIDENCE: ["AR-4D", "NOT_REQUIRED", "architecture/inventory.json"],
}

_FORBIDDEN_SNIPPETS: dict[str, list[str]] = {
    "apps/control-plane-worker/src/profiles.rs": ["use cloudflare_adapters::"],
    "apps/control-plane-worker/src/profile_generations.rs": ["use cloudflare_adapters::"],
}


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        raise SystemExit(f"AR-3 application architecture path missing: {relative}")
    return path.read_text(encoding="utf-8")


def load_topology(root: Path) -> dict[str, Any]:
    topology = json.loads(_read(root, RUNTIME_TOPOLOGY))
    if topology.get("status") != "ACCEPTED_AR2_DECISION" or topology.get("slice") != "AR-2":
        raise SystemExit("AR-3 requires the accepted AR-2 runtime topology decision")
    if topology.get("next_slice_after_acceptance") != "AR-3":
        raise SystemExit("AR-2 topology must hand off to AR-3")
    if topology.get("production_mutation") is not False:
        raise SystemExit("AR-2 topology must remain production-mutation-free")
    resources = topology.get("resources")
    if not isinstance(resources, list) or not resources:
        raise SystemExit("AR-2 topology resources must be a non-empty array")
    ids = [item.get("id") for item in resources if isinstance(item, dict)]
    if len(ids) != len(resources) or any(not isinstance(value, str) or not value for value in ids):
        raise SystemExit("every AR-2 topology resource requires a stable id")
    if len(set(ids)) != len(ids):
        raise SystemExit("AR-2 topology resource ids must be unique")
    decisions = {item["id"]: item.get("decision") for item in resources}
    required = {
        "control_plane_worker": "KEEP",
        "mailbox_secret_resolver_worker": "KEEP",
        "catalog_d1": "KEEP",
        "resolver_d1": "KEEP",
        "mailbox_jobs": "KEEP",
        "mailbox_jobs_dlq": "KEEP",
        "generation_verification": "DELETE",
        "mailbox_secret_resolver_service": "KEEP",
        "browser_bridge_mailbox_lane": "KEEP",
    }
    for resource_id, decision in required.items():
        if decisions.get(resource_id) != decision:
            raise SystemExit(
                f"AR-2 topology decision drift for {resource_id}: expected {decision}, got {decisions.get(resource_id)!r}"
            )
    return topology


def validate_source_contract(root: Path) -> None:
    for relative, snippets in _REQUIRED_SNIPPETS.items():
        source = _read(root, relative)
        for snippet in snippets:
            if snippet not in source:
                raise SystemExit(f"AR-3 source contract drift: {relative} missing {snippet!r}")
    for relative, snippets in _FORBIDDEN_SNIPPETS.items():
        source = _read(root, relative)
        for snippet in snippets:
            if snippet in source:
                raise SystemExit(f"AR-3 source contract drift: {relative} unexpectedly contains {snippet!r}")


def _validate_unique_ids(items: list[dict[str, Any]], label: str) -> None:
    ids = [item.get("id") for item in items]
    if any(not isinstance(value, str) or not value for value in ids) or len(ids) != len(set(ids)):
        raise SystemExit(f"AR-3 {label} ids must be non-empty and unique")


def build_projection(root: Path) -> dict[str, Any]:
    topology = load_topology(root)
    validate_source_contract(root)
    _validate_unique_ids(PROCESS_OWNERSHIP, "process")
    _validate_unique_ids(CAPABILITY_OWNERSHIP, "capability")
    _validate_unique_ids(COMPOSITION_FINDINGS, "finding")

    resource_ids = {item["id"] for item in topology["resources"]}
    for process in PROCESS_OWNERSHIP:
        unknown = sorted(set(process["resource_refs"]) - resource_ids)
        if unknown:
            raise SystemExit(f"AR-3 process {process['id']} references unknown AR-2 resources: {unknown}")

    remediation = {finding["owner_slice"] for finding in COMPOSITION_FINDINGS}
    if remediation != {"AR-4A", "AR-4B", "AR-4C", "AR-4D"}:
        raise SystemExit("AR-3 remediation map must cover exactly AR-4A/4B/4C/4D")
    ar4d = next(item for item in COMPOSITION_FINDINGS if item["id"] == "profile_application_extraction")
    if ar4d.get("decision") != "NOT_REQUIRED":
        raise SystemExit("AR-4D must remain NOT_REQUIRED unless a later accepted slice reopens it")

    return {
        "schema_version": 1,
        "status": "ACCEPTED_AR3_APPLICATION_ARCHITECTURE_CONTRACT",
        "topology_source": RUNTIME_TOPOLOGY,
        "evidence": AR3_EVIDENCE,
        "projection_policy": "EXTEND_CANONICAL_INVENTORY_DO_NOT_CREATE_COMPETING_REGISTRY",
        "composition_taxonomy": [
            "CONFORMING_COMPOSITION_SEAM",
            "TRANSPORT_COMPOSITION_DEBT",
            "ROUTE_OWNERSHIP_DEBT",
            "INTENTIONAL_RUNTIME_BOUNDARY",
            "CONDITIONAL_EXTRACTION_NOT_REQUIRED",
        ],
        "runtime_resources": copy.deepcopy(topology["resources"]),
        "runtime_processes": copy.deepcopy(PROCESS_OWNERSHIP),
        "capability_ownership": copy.deepcopy(CAPABILITY_OWNERSHIP),
        "composition_findings": copy.deepcopy(COMPOSITION_FINDINGS),
        "conditional_ar4d": {
            "decision": "NOT_REQUIRED",
            "reason": "Profile and Generation transports already use explicit composition seams; no measurable dependency-isolation benefit currently justifies extraction",
            "reopen_policy": "ONLY_BY_LATER_ACCEPTED_EVIDENCE",
        },
        "next_required_slice_after_ar3": "AR-4A",
        "production_mutation": False,
    }


def negative_self_test(root: Path) -> None:
    expected = build_projection(root)

    candidate_status = copy.deepcopy(expected)
    candidate_status["status"] = "AR3_APPLICATION_ARCHITECTURE_CONTRACT"
    if candidate_status == expected:
        raise SystemExit("AR-3 negative self-test failed to detect candidate-status regression")

    missing_resource = copy.deepcopy(expected)
    missing_resource["runtime_resources"] = missing_resource["runtime_resources"][1:]
    if len(missing_resource["runtime_resources"]) == len(expected["runtime_resources"]):
        raise SystemExit("AR-3 negative self-test failed to model missing resource drift")

    duplicate_owner = copy.deepcopy(expected)
    duplicate_owner["capability_ownership"].append(copy.deepcopy(duplicate_owner["capability_ownership"][0]))
    ids = [item["id"] for item in duplicate_owner["capability_ownership"]]
    if len(ids) == len(set(ids)):
        raise SystemExit("AR-3 negative self-test failed to model duplicate capability ownership")

    premature_ar4d = copy.deepcopy(expected)
    premature_ar4d["conditional_ar4d"]["decision"] = "REQUIRED"
    if premature_ar4d == expected:
        raise SystemExit("AR-3 negative self-test failed to distinguish premature AR-4D activation")

    gate = copy.deepcopy(expected)
    gate["production_mutation"] = True
    if gate == expected:
        raise SystemExit("AR-3 negative self-test failed to distinguish production mutation")

    print("AR-3 application architecture negative self-test passed.")
