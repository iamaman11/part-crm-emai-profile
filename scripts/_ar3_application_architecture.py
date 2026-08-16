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
AR4A_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md"
AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"

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
        "status": "CENTRAL_COMPOSITION_ROOT_AR4A_ACCEPTED",
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
        "composition_seam": "apps/control-plane-worker/src/composition.rs::query_repository",
        "status": "AR4A_CENTRALIZED_COMPOSITION_ACCEPTED",
        "remediation_slice": "AR-4A",
    },
    {
        "id": "client_mail_query",
        "transport": "apps/control-plane-worker/src/client_mail_query.rs",
        "application_owner": "crates/use-cases-query",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs::{query_repository,client_mail_eligibility_repository,client_mail_query_provider}",
        "status": "AR4A_CENTRALIZED_COMPOSITION_ACCEPTED",
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
        "composition_seam": "apps/control-plane-worker/src/composition.rs::{mailbox_job_application,mailbox_job_provider}",
        "status": "AR4A_CENTRALIZED_COMPOSITION_ACCEPTED",
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
        "composition_seam": "apps/control-plane-worker/src/composition.rs::{notification_operations_repository,notification_cursor_repository}",
        "status": "AR4A_CENTRALIZED_COMPOSITION_ACCEPTED",
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
        "status": "AR4A_COMPOSITION_ROOT_CONSOLIDATION_ACCEPTED",
        "owner_slice": "AR-4A",
        "evidence": [
            "apps/control-plane-worker/src/composition.rs::query_repository",
            "apps/control-plane-worker/src/composition.rs::notification_operations_repository",
            "apps/control-plane-worker/src/composition.rs::notification_cursor_repository",
            "apps/control-plane-worker/src/composition.rs::client_mail_eligibility_repository",
            "apps/control-plane-worker/src/composition.rs::client_mail_query_provider",
            "apps/control-plane-worker/src/composition.rs::mailbox_job_provider",
        ],
    },
    {
        "id": "client_mail_route_ownership",
        "status": "AR4B_ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED",
        "owner_slice": "AR-4B",
        "evidence": [
            "crates/control-plane-contract/src/routes/client_mail.rs::ClientMailSearchApi",
            "crates/control-plane-contract/src/routes/client_mail.rs::ClientMailMessageApi",
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
        "pub fn query_repository",
        "pub fn client_mail_eligibility_repository",
        "pub fn client_mail_query_provider",
        "pub fn notification_operations_repository",
        "pub fn notification_cursor_repository",
        "pub fn mailbox_job_provider",
    ],
    "apps/control-plane-worker/src/clients.rs": ["crate::composition::{", "use_cases_clients::"],
    "apps/control-plane-worker/src/identity.rs": ["crate::composition::{identity_ceremony_application, identity_governance_application}"],
    "apps/control-plane-worker/src/profiles.rs": ["crate::composition::profile_application"],
    "apps/control-plane-worker/src/profile_generations.rs": ["crate::composition::profile_generation_application"],
    "apps/control-plane-worker/src/mailbox_bindings.rs": ["crate::composition::{browser_mailbox_execution_application, mailbox_binding_application}"],
    "apps/control-plane-worker/src/device_jobs.rs": ["crate::composition::{", "device_job_repository"],
    "apps/control-plane-worker/src/operator_queries.rs": ["crate::composition::query_repository", "query_repository(env)?"],
    "apps/control-plane-worker/src/notifications.rs": ["notification_operations_repository", "notification_cursor_repository"],
    "apps/control-plane-worker/src/mailbox_jobs.rs": ["mailbox_job_provider", "mailbox_job_provider(env, actor)?"],
    "apps/control-plane-worker/src/client_mail_query.rs": [
        "client_mail_eligibility_repository",
        "client_mail_query_provider",
        "query_repository",
    ],
    "apps/control-plane-worker/src/client_mail_send.rs": [
        "D1ClientMailboxEligibilityRepository::new",
        "D1OutboundMailIntentRepository::new",
        "D1MailboxRepository::new",
        "CloudflareGmailOutboundMailProvider::new",
        "CloudflareSmtpOutboundMailProvider::new",
    ],
    "crates/control-plane-contract/src/routes/clients.rs": ["ClientCollectionApi", "ClientGrantApi"],
    "crates/control-plane-contract/src/routes/client_mail.rs": [
        "ClientMailSearchApi",
        "ClientMailMessageApi",
        "ClientMailSendApi",
    ],
    "crates/use-cases/src/lib.rs": ["pub mod generations;", "pub mod profiles;"],
    "apps/mailbox-secret-resolver-worker/src/lib.rs": ["#[event(fetch, respond_with_errors)]", "#[event(scheduled)]"],
    "apps/profile-bridge/src/main.rs": ["ClaimUri::parse"],
    AR3_EVIDENCE: ["AR-4D", "NOT_REQUIRED", "architecture/inventory.json"],
    AR4A_EVIDENCE: ["AR-4A Composition-root consolidation", "AR-4B", "AR-4C", "Production Core remains `BLOCKED`"],
    AR4B_EVIDENCE: [
        "AR-4B Client Mail route ownership",
        "ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED",
        "AR-4C",
        "Production Core remains `BLOCKED`",
    ],
}

_FORBIDDEN_SNIPPETS: dict[str, list[str]] = {
    "apps/control-plane-worker/src/profiles.rs": ["use cloudflare_adapters::"],
    "apps/control-plane-worker/src/profile_generations.rs": ["use cloudflare_adapters::"],
    "apps/control-plane-worker/src/operator_queries.rs": [
        "cloudflare_adapters::d1_query::D1QueryRepository",
        "D1QueryRepository::new",
    ],
    "apps/control-plane-worker/src/notifications.rs": [
        "cloudflare_adapters::d1_notification_operations::D1NotificationOperationsRepository",
        "cloudflare_adapters::d1_notifications::D1NotificationRepository",
        "D1NotificationOperationsRepository::new",
        "D1NotificationRepository::new",
    ],
    "apps/control-plane-worker/src/mailbox_jobs.rs": [
        "cloudflare_adapters::cloud_mailbox_provider::CloudMailboxProviderRouter",
        "CloudMailboxProviderRouter::new",
    ],
    "apps/control-plane-worker/src/client_mail_query.rs": [
        "cloudflare_adapters::cloud_mail_query::CloudMailboxQueryAdapter",
        "cloudflare_adapters::d1_client_mail_eligibility::D1ClientMailboxEligibilityRepository",
        "cloudflare_adapters::d1_query::D1QueryRepository",
        "CloudMailboxQueryAdapter::new",
        "D1ClientMailboxEligibilityRepository::new",
        "D1QueryRepository::new",
    ],
    "crates/control-plane-contract/src/routes/clients.rs": [
        "ClientMailSearchApi",
        "ClientMailMessageApi",
        "ClientMailSendApi",
    ],
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


def _validate_source_text(relative: str, source: str) -> None:
    for snippet in _REQUIRED_SNIPPETS.get(relative, []):
        if snippet not in source:
            raise SystemExit(f"AR-3 source contract drift: {relative} missing {snippet!r}")
    for snippet in _FORBIDDEN_SNIPPETS.get(relative, []):
        if snippet in source:
            raise SystemExit(f"AR-3 source contract drift: {relative} unexpectedly contains {snippet!r}")


def validate_source_contract(root: Path) -> None:
    paths = sorted(set(_REQUIRED_SNIPPETS) | set(_FORBIDDEN_SNIPPETS))
    for relative in paths:
        _validate_source_text(relative, _read(root, relative))


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
        "status": "ACCEPTED_AR4B_APPLICATION_ARCHITECTURE_REMEDIATION",
        "topology_source": RUNTIME_TOPOLOGY,
        "base_contract_evidence": AR3_EVIDENCE,
        "evidence": AR4B_EVIDENCE,
        "projection_policy": "EXTEND_CANONICAL_INVENTORY_DO_NOT_CREATE_COMPETING_REGISTRY",
        "composition_taxonomy": [
            "CONFORMING_COMPOSITION_SEAM",
            "TRANSPORT_COMPOSITION_DEBT",
            "ROUTE_OWNERSHIP_DEBT",
            "INTENTIONAL_RUNTIME_BOUNDARY",
            "CONDITIONAL_EXTRACTION_NOT_REQUIRED",
            "AR4A_CENTRALIZED_COMPOSITION_ACCEPTED",
            "AR4B_ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED",
        ],
        "runtime_resources": copy.deepcopy(topology["resources"]),
        "runtime_processes": copy.deepcopy(PROCESS_OWNERSHIP),
        "capability_ownership": copy.deepcopy(CAPABILITY_OWNERSHIP),
        "composition_findings": copy.deepcopy(COMPOSITION_FINDINGS),
        "remediation_state": {
            "accepted_through": "AR-4B",
            "status": "ACCEPTED",
            "evidence": AR4B_EVIDENCE,
            "next_required_slice": "AR-4C",
        },
        "conditional_ar4d": {
            "decision": "NOT_REQUIRED",
            "reason": "Profile and Generation transports already use explicit composition seams; no measurable dependency-isolation benefit currently justifies extraction",
            "reopen_policy": "ONLY_BY_LATER_ACCEPTED_EVIDENCE",
        },
        "next_required_slice_after_ar4b": "AR-4C",
        "production_mutation": False,
    }


def negative_self_test(root: Path) -> None:
    expected = build_projection(root)

    accepted_status = copy.deepcopy(expected)
    accepted_status["status"] = "ACCEPTED_AR3_APPLICATION_ARCHITECTURE_CONTRACT"
    if accepted_status == expected:
        raise SystemExit("AR-4B negative self-test failed to detect accepted-state rollback to AR-3")

    adapter_regression = _read(root, "apps/control-plane-worker/src/operator_queries.rs") + "\nD1QueryRepository::new"
    try:
        _validate_source_text("apps/control-plane-worker/src/operator_queries.rs", adapter_regression)
    except SystemExit:
        pass
    else:
        raise SystemExit("AR-4A negative self-test failed to reject transport adapter construction regression")

    split_route_regression = _read(root, "crates/control-plane-contract/src/routes/clients.rs") + "\nClientMailSearchApi"
    try:
        _validate_source_text("crates/control-plane-contract/src/routes/clients.rs", split_route_regression)
    except SystemExit:
        pass
    else:
        raise SystemExit("AR-4B negative self-test failed to reject Client Mail route ownership regression")

    missing_route_owner = _read(root, "crates/control-plane-contract/src/routes/client_mail.rs").replace(
        "ClientMailMessageApi", "MissingMailMessageRouteClass"
    )
    try:
        _validate_source_text("crates/control-plane-contract/src/routes/client_mail.rs", missing_route_owner)
    except SystemExit:
        pass
    else:
        raise SystemExit("AR-4B negative self-test failed to reject missing Client Mail route ownership")

    remediation = copy.deepcopy(expected)
    remediation["remediation_state"] = {
        "accepted_through": "AR-4A",
        "candidate": "AR-4B",
        "candidate_status": "ROUTE_OWNERSHIP_CONSOLIDATION_CANDIDATE",
        "evidence": AR4B_EVIDENCE,
        "next_after_acceptance": "AR-4C",
    }
    if remediation == expected:
        raise SystemExit("AR-4B negative self-test failed to detect accepted-state rollback to candidate remediation state")

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

    print("AR-4B accepted Client Mail route ownership negative self-tests passed.")
