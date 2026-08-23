#!/usr/bin/env python3
"""Generate and verify the canonical Architecture Re-baseline v3 inventory.

The proven workspace/migration/route/generated-contract core remains in
`_architecture_inventory_core.py`. Later AR slices extend the same canonical hierarchy;
AR-8B projects the accepted metadata-only credential authority; AR-8C projects its bounded operational lifecycle without creating a competing registry.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

import _architecture_inventory_core as core
import _ar3_application_architecture as ar3
import credential_authority as current_credentials

ROOT = Path(__file__).resolve().parents[1]
INVENTORY_PATH = ROOT / "architecture" / "inventory.json"
CURRENT_AUTHORITY = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
TRANSITION = "architecture/architecture-rebaseline-v3-transition.json"
AR2_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR2.md"
AR4A_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md"
AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"
AR4C_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"
AR5_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR5.md"
AR6_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR6.md"
AR7_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR7.md"
GOVERNANCE_CONTRACT = "architecture/github-governance-ar7.json"
PYTHON_ESTATE = "architecture/python-estate-ar6.json"
CREDENTIAL_AUTHORITY = "architecture/credential-authority-ar8b.json"
AR8C_PROVIDER_EXECUTION_AUTHORITY = "architecture/ar8-staging-provider-bootstrap-contract.json"
AR8C_PROVIDER_EXECUTION_EVIDENCE = "docs/AR8_STAGING_PROVIDER_BOOTSTRAP.md"
POST_AR8C_CLEANUP_EVIDENCE = "docs/evidence/2026-08-18-post-ar8c-cleanup-closeout.json"
TRACKING_ISSUE = 266
AR8_UMBRELLA_ISSUE = 308
AR8B_IMPLEMENTATION_ISSUE = 309
AR8C_IMPLEMENTATION_ISSUE = 314
ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5", "AR-6", "AR-7"]
CURRENT_SLICE = "AR-8"
NEXT_SLICE = "AR-9"
AR8_ACCEPTED_SUBSLICES = ["AR-8A", "AR-8B", "AR-8C"]
AR8_CURRENT_SUBSLICE = "AR-8D"
AR8D_IMPLEMENTATION_ISSUE = None
AR8_IMPLEMENTATION_ENTRY_GATE = "POST_AR8C_CLEANUP_DX_ACCEPTED_AR8D_IMPLEMENTATION_CURRENT"
AR8_MANDATORY_REMAINING = ["AR-8D", "AR-8E", "AR-8F"]
POST_AR8C_CLEANUP_ISSUE = 352
CURRENT_DELIVERY_CHECKPOINT = "AR-8C"
AR8C_HOSTED_VERIFIED_MAIN = "29519fbf05f8e4c228a0907ee0dafd2c85e3749b"

DOCUMENT_STATUS = [
    {"path": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "status": "CURRENT_AUTHORITY", "scope": "architecture_program_execution"},
    {"path": "docs/DEVELOPMENT_PLAN.md", "status": "GENERATED_PROJECTION", "scope": "product_program_projection_and_accepted_phase_provenance"},
    {"path": "docs/status.json", "status": "GENERATED_PROJECTION", "scope": "machine_readable_program_and_readiness_state"},
    {"path": "docs/INDEX.md", "status": "GENERATED_PROJECTION", "scope": "documentation_authority_navigation"},
    {"path": "README.md", "status": "GENERATED_PROJECTION", "scope": "repository_entrypoint"},
    {"path": "docs/README.md", "status": "GENERATED_PROJECTION", "scope": "documentation_entrypoint"},
    {"path": "docs/ARCHITECTURE.md", "status": "STABLE_AUTHORITY", "scope": "accepted_architecture_invariants"},
    {"path": "docs/DATA_CLASSIFICATION.md", "status": "STABLE_AUTHORITY", "scope": "data_privacy_classification"},
    {"path": "docs/THREAT_MODEL.md", "status": "STABLE_AUTHORITY", "scope": "repository_security_threat_model"},
    {"path": "docs/UI_ARCHITECTURE.md", "status": "STABLE_AUTHORITY", "scope": "ui_architecture_target"},
    {"path": "docs/DEVELOPER_CAPABILITY_MATRIX.md", "status": "STABLE_AUTHORITY", "scope": "accepted_capability_evidence"},
    {"path": "architecture/accepted-phases.json", "status": "STABLE_AUTHORITY", "scope": "immutable_accepted_product_phase_provenance"},
    {"path": TRANSITION, "status": "GENERATED_PROJECTION", "scope": "architecture_program_transition_state"},
    {"path": "docs/ARCHITECTURE_REBASELINE_V3_AR0.md", "status": "EVIDENCE", "scope": "ar0_research_acceptance"},
    {"path": "docs/ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md", "status": "EVIDENCE", "scope": "ar0_second_pass_research"},
    {"path": AR2_EVIDENCE, "status": "EVIDENCE", "scope": "ar2_runtime_topology_and_d3_compatibility_acceptance"},
    {"path": ar3.AR3_EVIDENCE, "status": "EVIDENCE", "scope": "ar3_application_architecture_contract_accepted"},
    {"path": AR4A_EVIDENCE, "status": "EVIDENCE", "scope": "ar4a_composition_root_consolidation_accepted"},
    {"path": AR4B_EVIDENCE, "status": "EVIDENCE", "scope": "ar4b_client_mail_route_ownership_accepted"},
    {"path": AR4C_EVIDENCE, "status": "EVIDENCE", "scope": "ar4c_outbound_mail_composition_accepted"},
    {"path": AR5_EVIDENCE, "status": "EVIDENCE", "scope": "ar5_runtime_authority_cleanup_accepted"},
    {"path": AR6_EVIDENCE, "status": "EVIDENCE", "scope": "ar6_python_estate_and_read_only_opsctl_accepted"},
    {"path": PYTHON_ESTATE, "status": "STABLE_AUTHORITY", "scope": "accepted_ar6_full_python_disposition"},
    {"path": AR7_EVIDENCE, "status": "EVIDENCE", "scope": "ar7_github_governance_and_operational_boundaries_accepted"},
    {"path": GOVERNANCE_CONTRACT, "status": "STABLE_AUTHORITY", "scope": "accepted_ar7_github_governance_contract"},
    {"path": AR8C_PROVIDER_EXECUTION_EVIDENCE, "status": "EVIDENCE", "scope": "ar8c_protected_staging_provider_execution_authority"},
    {"path": AR8C_PROVIDER_EXECUTION_AUTHORITY, "status": "STABLE_AUTHORITY", "scope": "ar8c_staging_provider_execution_contract"},
    {"path": POST_AR8C_CLEANUP_EVIDENCE, "status": "EVIDENCE", "scope": "post_ar8c_cleanup_dx_acceptance"},
    {"path": "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md", "status": "ACCEPTED_HISTORICAL", "scope": "superseded_predecessor_forward_execution"},
    {"path": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md", "status": "ACCEPTED_HISTORICAL", "scope": "accepted_r1_r9_closeout"},
    {"path": "IMPLEMENTATION_PLAN.md", "status": "SUPERSEDED", "scope": "compatibility_entrypoint_to_preserved_history"},
    {"path": "PROFILE_LIFECYCLE_PLAN.md", "status": "SUPERSEDED", "scope": "compatibility_entrypoint_to_preserved_history"},
]


def ar8b_acceptance() -> dict[str, object]:
    return {
        "issue": 309,
        "implementation_pr": 312,
        "exact_green_head": "743276b578bb15042e8a42beff0e14f7698e61b0",
        "implementation_merge": "4e4d1c25226384858ca8905377ee155bedabc6d4",
        "applicable_permanent_workflows": "14/14",
    }


def ar8c_acceptance() -> dict[str, object]:
    payload = load_credential_authority().get("ar8c_operational_lifecycle", {}).get("acceptance")
    if not isinstance(payload, dict):
        raise SystemExit("accepted AR-8C lifecycle acceptance evidence is missing")
    return payload


def expected_ar8_progress() -> dict[str, object]:
    return {
        "umbrella_issue": AR8_UMBRELLA_ISSUE,
        "accepted_subslices": AR8_ACCEPTED_SUBSLICES,
        "current_subslice": AR8_CURRENT_SUBSLICE,
        "current_implementation_issue": AR8D_IMPLEMENTATION_ISSUE,
        "mandatory_remaining": AR8_MANDATORY_REMAINING,
        "implementation_entry_gate": AR8_IMPLEMENTATION_ENTRY_GATE,
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "production_mutation": False,
    }


def current_delivery_map() -> dict[str, object]:
    return {
        "schema_version": 1,
        "canonical_authority": "architecture/inventory.json::current_delivery_map",
        "program": "Architecture Re-baseline v3",
        "accepted_checkpoint": CURRENT_DELIVERY_CHECKPOINT,
        "current_work": "AR-8D_IMPLEMENTATION",
        "source_implemented": {
            "status": "PARTIAL",
            "through": CURRENT_DELIVERY_CHECKPOINT,
            "current_subslice": AR8_CURRENT_SUBSLICE,
            "current_subslice_source": "CURRENT_IMPLEMENTATION_NOT_ACCEPTED",
        },
        "accepted_on_main": {
            "status": "PARTIAL",
            "through": CURRENT_DELIVERY_CHECKPOINT,
            "full_ar8_accepted": False,
        },
        "staging_live": {
            "status": "PARTIAL",
            "scope": "AR-8C_STAGING_PROVIDER_AND_CREDENTIAL_FOUNDATION_ONLY",
            "evidence": AR8C_PROVIDER_EXECUTION_EVIDENCE,
            "hosted_verified_main": AR8C_HOSTED_VERIFIED_MAIN,
        },
        "production_authorized": {
            "status": False,
            "gate": "AR-17_ARCHITECTURE_CLOSEOUT_AND_PRODUCTION_CORE_AUTHORIZATION",
        },
        "production_enabled": {
            "status": False,
            "gate": "PC-1_AFTER_AR-17_AUTHORIZATION",
            "scope": "NONE",
        },
        "post_ar8c_cleanup": {
            "issue": POST_AR8C_CLEANUP_ISSUE,
            "status": "ACCEPTED",
            "evidence": "docs/evidence/2026-08-18-post-ar8c-cleanup-closeout.json",
        },
        "current_blocker": {
            "issue": None,
            "status": "NONE",
            "blocks": "NONE",
        },
        "next_gate": {
            "id": "AR-8D_ACCEPTANCE",
            "issue": None,
            "on_success": "AR-8E_BECOMES_CURRENT",
        },
        "invariants": {
            "source_present_not_equal_production_enabled": True,
            "full_ar8_accepted": False,
            "ar9_blocked": True,
            "architecture_complete": False,
            "production_core_gate": "BLOCKED",
            "production_ready": False,
            "production_mutation": False,
        },
    }


def validate_source_documents() -> None:
    for item in DOCUMENT_STATUS:
        if not (ROOT / item["path"]).is_file():
            raise SystemExit(f"document-status inventory path missing: {item['path']}")
    status = json.loads((ROOT / "docs/status.json").read_text(encoding="utf-8"))
    transition = json.loads((ROOT / TRANSITION).read_text(encoding="utf-8"))
    current = status.get("current", {})
    program = current.get("architecture_program", {}) if isinstance(current, dict) else {}
    if status.get("schema_version") != 6:
        raise SystemExit("docs/status.json schema_version must be 6 after CURRENT_DELIVERY_MAP acceptance")
    if current.get("current_delivery_map") != current_delivery_map():
        raise SystemExit("docs/status.json current_delivery_map drifted from canonical generator projection")
    if transition.get("schema_version") != 13 or transition.get("current_delivery_map") != current_delivery_map():
        raise SystemExit("architecture transition CURRENT_DELIVERY_MAP projection drifted")
    if status.get("production_ready") is not False:
        raise SystemExit("docs/status.json must remain production_ready=false during AR-8")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        raise SystemExit("docs/status.json must keep AR-8 architecture/gate state fail closed")
    if program.get("authority") != CURRENT_AUTHORITY or program.get("tracking_issue") != TRACKING_ISSUE:
        raise SystemExit("docs/status.json current architecture authority drifted")
    if program.get("accepted_slices") != ACCEPTED_SLICES or program.get("current_slice") != CURRENT_SLICE or program.get("next_slice_after_acceptance") != NEXT_SLICE:
        raise SystemExit("docs/status.json must project accepted through AR-7 with active AR-8 and AR-9 blocked")
    if program.get("ar8_progress") != expected_ar8_progress():
        raise SystemExit("docs/status.json must project AR-8A/B accepted / AR-8C current / AR-8D..F mandatory")
    if program.get("runtime_authority_cleanup_evidence") != AR5_EVIDENCE:
        raise SystemExit("docs/status.json must project the accepted AR-5 runtime-authority cleanup evidence")
    if program.get("python_operational_evidence") != AR6_EVIDENCE or program.get("python_estate") != PYTHON_ESTATE:
        raise SystemExit("docs/status.json must project accepted AR-6 Python/opsctl authority")
    if program.get("github_governance_evidence") != AR7_EVIDENCE or program.get("github_governance_contract") != GOVERNANCE_CONTRACT:
        raise SystemExit("docs/status.json must project accepted AR-7 GitHub governance authority")
    runtime_gate = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-cloudflare-runtime-bindings.py")],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if runtime_gate.returncode != 0:
        details = "\n".join(value.strip() for value in (runtime_gate.stdout, runtime_gate.stderr) if value.strip())
        raise SystemExit(f"AR-5 runtime authority gate failed:\n{details}")


def validate_full_documentation_authority() -> None:
    authority = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-documentation-authority.py"), "--root", str(ROOT)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if authority.returncode != 0:
        details = "\n".join(value.strip() for value in (authority.stdout, authority.stderr) if value.strip())
        raise SystemExit(f"documentation authority check failed:\n{details}")


def _normalized_detected(value: dict[str, set[str]]) -> dict[str, tuple[str, ...]]:
    return {name: tuple(sorted(paths)) for name, paths in sorted(value.items())}


def validate_credential_authority(payload: dict[str, Any], detected: dict[str, set[str]]) -> None:
    lifecycle = current_credentials.read_json(ROOT, current_credentials.EXPECTED_LIFECYCLE)
    successor = current_credentials.read_json(
        ROOT, current_credentials.EXPECTED_SECRET_TRANSPORT_SUCCESSOR
    )
    current_credentials.validate_registry(payload, detected, lifecycle, successor)


def validate_credential_authority_source() -> tuple[dict[str, Any], dict[str, set[str]]]:
    state = current_credentials.validate_repository(ROOT)
    return state.registry, state.detected


def load_credential_authority() -> dict[str, Any]:
    payload, _ = validate_credential_authority_source()
    return payload


def credential_negative_self_test(payload: dict[str, Any], detected: dict[str, set[str]]) -> None:
    state = current_credentials.validate_repository(ROOT)
    if payload != state.registry:
        raise ValueError("historical credential shim payload diverged from current authority")
    if _normalized_detected(detected) != _normalized_detected(state.detected):
        raise ValueError("historical credential shim bindings diverged from current authority")
    current_credentials.negative_self_test(state, ROOT)


def print_credential_check_summary(detected: dict[str, set[str]], *, self_tested: bool) -> None:
    state = current_credentials.validate_repository(ROOT)
    if _normalized_detected(detected) != _normalized_detected(state.detected):
        raise ValueError("historical credential shim summary bindings diverged from current authority")
    suffix = " and fail-closed negative fixtures" if self_tested else ""
    print(
        f"Current credential authority covers {len(detected)} tracked static bindings{suffix}; "
        "historical inventory engine delegates credential validation to the neutral owner."
    )


def git_blob_sha(path: Path) -> str:
    relative = path.relative_to(ROOT).as_posix()
    result = subprocess.run(
        ["git", "hash-object", f"--path={relative}", "--", relative],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    digest = result.stdout.strip().lower()
    if result.returncode != 0 or re.fullmatch(r"[0-9a-f]{40}", digest) is None:
        details = result.stderr.strip()
        raise ValueError(f"git hash-object failed for {relative}: {details or digest or result.returncode}")
    return digest


def build_ar8c_operational_lifecycle_projection(payload: dict[str, Any]) -> dict[str, object]:
    lifecycle = payload.get("ar8c_operational_lifecycle")
    if not isinstance(lifecycle, dict):
        raise SystemExit("AR-8C operational lifecycle source is missing")
    concerns = lifecycle.get("concerns")
    hosted = lifecycle.get("hosted_reconciliation")
    github = hosted.get("github") if isinstance(hosted, dict) else None
    cloudflare = hosted.get("cloudflare") if isinstance(hosted, dict) else None
    if not isinstance(concerns, list) or any(not isinstance(item, dict) for item in concerns):
        raise SystemExit("AR-8C operational lifecycle concerns must be a list of objects")
    if not isinstance(github, dict) or not isinstance(cloudflare, dict):
        raise SystemExit("AR-8C hosted reconciliation source is incomplete")
    concern_ids = sorted(
        str(item.get("id"))
        for item in concerns
        if isinstance(item.get("id"), str) and item.get("id")
    )
    if len(concern_ids) != len(concerns) or len(set(concern_ids)) != len(concern_ids):
        raise SystemExit("AR-8C operational lifecycle requires unique stable concern ids")
    return {
        "schema_version": int(lifecycle["schema_version"]),
        "status": str(lifecycle["status"]),
        "implementation_issue": int(lifecycle["implementation_issue"]),
        "accepted_base": str(lifecycle["accepted_base"]),
        "metadata_only": lifecycle.get("metadata_only") is True,
        "stage_order": lifecycle["stage_order"],
        "concern_ids": concern_ids,
        "hosted_reconciliation": {
            "github": {
                "accepted_main_only": github.get("accepted_main_only") is True,
                "pull_request_exposure": github.get("pull_request_exposure") is True,
                "metadata_only": github.get("metadata_only") is True,
                "readback_values": github.get("readback_values") is True,
                "executor_binding": github.get("executor_binding"),
                "live_audit_environments": github.get("live_audit_environments"),
            },
            "cloudflare": {
                "accepted_main_only": cloudflare.get("accepted_main_only") is True,
                "audit_environment": cloudflare.get("audit_environment"),
                "read_only": cloudflare.get("read_only") is True,
                "api_token_binding": cloudflare.get("api_token_binding"),
                "verify_endpoint": cloudflare.get("verify_endpoint"),
                "required_token_status": cloudflare.get("required_token_status"),
                "worker_secret_contract_source": cloudflare.get("worker_secret_contract_source"),
                "deploy_manifest_binding": cloudflare.get("deploy_manifest_binding"),
                "secret_binding_metadata_endpoint": cloudflare.get("secret_binding_metadata_endpoint"),
            },
        },
        "production_mutation": lifecycle.get("production_mutation") is True,
        "opsctl_mutation": lifecycle.get("opsctl_mutation") is True,
        "acceptance": lifecycle.get("acceptance"),
    }


def build_credential_projection(payload: dict[str, Any]) -> dict[str, object]:
    path = ROOT / CREDENTIAL_AUTHORITY
    sections = ("credentials", "dynamic_credential_domains", "future_trust_domains")
    entries: list[dict[str, Any]] = []
    counts: dict[str, int] = {}
    for section in sections:
        raw = payload.get(section)
        if not isinstance(raw, list) or any(not isinstance(entry, dict) for entry in raw):
            raise SystemExit(f"AR-8B credential authority {section} must be a list of objects")
        counts[section] = len(raw)
        entries.extend(raw)
    ids = [entry.get("id") for entry in entries]
    if any(not isinstance(value, str) or not value for value in ids):
        raise SystemExit("AR-8B credential authority projection requires stable logical ids")
    binding_names = sorted(
        {
            binding.get("name")
            for entry in entries
            for binding in entry.get("bindings", [])
            if isinstance(binding, dict) and isinstance(binding.get("name"), str) and binding.get("name")
        }
    )
    future_cutovers = {str(entry["id"]): entry.get("future_cutover") for entry in entries}
    if any(not isinstance(value, str) or not value for value in future_cutovers.values()):
        raise SystemExit("AR-8B credential authority projection requires future_cutover for every authority")
    return {
        "schema_version": int(payload["schema_version"]),
        "status": str(payload["status"]),
        "source_authority": CREDENTIAL_AUTHORITY,
        "source_git_blob_sha1": git_blob_sha(path),
        "metadata_only": True,
        "canonical_environments": payload["canonical_environments"],
        "invariants": payload["invariants"],
        "authority_counts": counts,
        "authority_ids": ids,
        "static_binding_names": binding_names,
        "future_cutovers": future_cutovers,
        "operational_lifecycle": build_ar8c_operational_lifecycle_projection(payload),
    }


def build_inventory() -> dict[str, object]:
    core.validate_route_ownership()
    validate_source_documents()
    application_architecture = ar3.build_projection(ROOT)
    credential_authority = load_credential_authority()
    credential_projection = build_credential_projection(credential_authority)
    routes = [
        {
            "route_class": route_class,
            "capability": capability,
            "methods": methods,
            "path_template": template,
            "example_path": example_path,
            "authenticated": authenticated,
        }
        for route_class, capability, methods, template, example_path, authenticated in core.ROUTE_SPECS
    ]
    return {
        "schema_version": 3,
        "workspace_members": core.workspace_members(),
        "d1_migrations": {"directory": "migrations/d1", "files": core.migration_files()},
        "routing": {
            "composed_entrypoint": "crates/control-plane-contract/src/lib.rs::classify_route",
            "dynamic_namespaces": ["/api", "/auth", "/bridge"],
            "classifiers": [{"capability": capability, "module": path} for capability, path in core.CLASSIFIERS],
            "public_routes": routes,
        },
        "generated_contracts": core.GENERATED_CONTRACTS,
        "application_architecture": application_architecture,
        "runtime_authority_cleanup": {
            "schema_version": 1,
            "status": "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP",
            "evidence": AR5_EVIDENCE,
            "implementation_issue": 290,
            "implementation_pr": 291,
            "exact_green_head": "afed435bb714794d6c4f252be6b44c592ee31b2b",
            "implementation_merge": "82d251a1d6666199c6eace393eedc1766157fcee",
            "applicable_permanent_workflows": "13/13",
            "generation_verification": {
                "wrangler_producer_binding": "ABSENT",
                "runtime_contract_binding": "ABSENT",
                "deployment_manifest_identity": "ABSENT",
                "queue_workload": "ABSENT",
                "verification_authority": "SYNCHRONOUS_APPLICATION_ROUTE",
            },
            "preserved_queue_producers": ["INTEGRATION_EVENTS", "MAILBOX_JOBS"],
            "application_architecture_accepted_through": "AR-4C",
            "ar4d": "NOT_REQUIRED_UNLESS_LATER_ACCEPTED_EVIDENCE_REOPENS",
            "production_mutation": False,
            "next_required_slice": "AR-6",
        },
        "python_operational_authority": {
            "schema_version": 1,
            "status": "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION",
            "evidence": AR6_EVIDENCE,
            "python_estate": PYTHON_ESTATE,
            "implementation_issue": 294,
            "implementation_pr": 295,
            "exact_green_head": "9b06d542873ffa3122e53e107105098e21f5933c",
            "implementation_merge": "d0229fedd81ee870822b6d9394bc4ee313ea3a3c",
            "applicable_permanent_workflows": "13/13",
            "python_summary": {
                "tracked_python_files": 116,
                "KEEP_PYTHON": 108,
                "MIGRATE_TO_RUST": 2,
                "DELETE_AFTER_SEQUENCE": 6,
                "WRAP_WITH_RUST": 0,
            },
            "opsctl": {
                "path": "tools/opsctl",
                "mode": "READ_ONLY_FOUNDATION",
                "commands": ["doctor", "status", "inventory"],
                "third_party_dependencies": False,
                "provider_mutation": False,
            },
            "future_cutovers": {"AR-10": "DELETE_AFTER_SEQUENCE", "AR-11": "MIGRATE_TO_RUST"},
            "application_architecture_accepted_through": "AR-4C",
            "runtime_authority_cleanup_accepted_through": "AR-5",
            "production_mutation": False,
            "next_required_slice": "AR-7",
        },
        "github_governance_authority": {
            "schema_version": 1,
            "status": "ACCEPTED_AR7_GITHUB_GOVERNANCE",
            "evidence": AR7_EVIDENCE,
            "contract": GOVERNANCE_CONTRACT,
            "validator": ".github/scripts/github-governance.mjs",
            "workflow": ".github/workflows/github-governance-gate.yml",
            "implementation_issue": 298,
            "implementation_pr": 299,
            "exact_green_head": "1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7",
            "implementation_merge": "3492273cb9237850e3fa27343cc5edbdb0f66aa1",
            "applicable_permanent_workflows": "14/14",
            "hosted_audit": {"run_id": 31953316327, "contract_job": "success", "hosted_state_job": "success"},
            "direct_main_negative_probe": {
                "result": "HTTP_409_REJECTED",
                "message": "Changes must be made through a pull request. 21 of 21 required status checks are expected.",
                "sentinel_present_after_probe": False,
            },
            "main_protection": {
                "mechanism": "classic_branch_protection",
                "required_check_count": 21,
                "require_pull_request": True,
                "require_conversation_resolution": True,
                "enforce_admins": True,
                "strict_required_status_checks": True,
                "allow_force_pushes": False,
                "allow_deletions": False,
            },
            "environments": {
                "rehearsal": {"allowed_branches": ["main"], "minimum_reviewers": 0},
                "staging": {"allowed_branches": ["main"], "minimum_reviewers": 0},
                "production": {"allowed_branches": ["main"], "minimum_reviewers": 1, "can_admins_bypass": False},
            },
            "production_mutation": False,
            "next_required_slice": "AR-8",
        },
        "credential_authority": credential_projection,
        "current_delivery_map": current_delivery_map(),
        "documentation_authority": {
            "current_program": CURRENT_AUTHORITY,
            "tracking_issue": TRACKING_ISSUE,
            "current_slice": CURRENT_SLICE,
            "transition": TRANSITION,
            "runtime_topology_evidence": AR2_EVIDENCE,
            "runtime_authority_cleanup_evidence": AR5_EVIDENCE,
            "python_operational_evidence": AR6_EVIDENCE,
            "python_estate": PYTHON_ESTATE,
            "github_governance_evidence": AR7_EVIDENCE,
            "github_governance_contract": GOVERNANCE_CONTRACT,
            "credential_authority_source": CREDENTIAL_AUTHORITY,
            "credential_authority_projection": "architecture/inventory.json::credential_authority",
            "current_delivery_map": "architecture/inventory.json::current_delivery_map",
            "ar8_umbrella_issue": AR8_UMBRELLA_ISSUE,
            "application_architecture_evidence": AR4C_EVIDENCE,
            "application_architecture_base_evidence": ar3.AR3_EVIDENCE,
            "application_architecture_projection": "architecture/inventory.json::application_architecture",
            "development_projection": "docs/DEVELOPMENT_PLAN.md",
            "readiness_projection": "docs/status.json",
            "index": "docs/INDEX.md",
            "architecture": "docs/ARCHITECTURE.md",
            "data_classification": "docs/DATA_CLASSIFICATION.md",
            "ui_target": "docs/UI_ARCHITECTURE.md",
            "accepted_capabilities": "docs/DEVELOPER_CAPABILITY_MATRIX.md",
            "security": "docs/THREAT_MODEL.md",
            "accepted_phase_ledger": "architecture/accepted-phases.json",
            "historical_pre2j_product_readiness": "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
            "historical_pre2j_architecture_closeout": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
        },
        "document_status": DOCUMENT_STATUS,
        "program_state": {
            "accepted_product_phase": "Phase 2I",
            "accepted_architecture_slices": ACCEPTED_SLICES,
            "current_architecture_slice": CURRENT_SLICE,
            "next_architecture_slice_after_acceptance": NEXT_SLICE,
            "ar8_progress": expected_ar8_progress(),
            "ar8b_acceptance": ar8b_acceptance(),
            "ar8c_acceptance": ar8c_acceptance(),
            "current_delivery_map": "architecture/inventory.json::current_delivery_map",
            "architecture_complete": False,
            "production_core_gate": "BLOCKED",
            "production_ready": False,
            "production_mutation_allowed_during_ar0_ar17": False,
            "production_ready_authority": "PC-1_AFTER_AR-17_AUTHORIZATION",
        },
    }


def serialized(inventory: dict[str, object]) -> str:
    return json.dumps(inventory, indent=2, ensure_ascii=False) + "\n"


def check_current(expected: dict[str, object]) -> None:
    if not INVENTORY_PATH.is_file():
        raise SystemExit(f"architecture inventory is missing: {INVENTORY_PATH.relative_to(ROOT)}")
    if INVENTORY_PATH.read_text(encoding="utf-8") != serialized(expected):
        raise SystemExit("architecture/inventory.json is stale; run python scripts/generate-architecture-inventory.py --write")


def self_test(expected: dict[str, object]) -> None:
    workspace = copy.deepcopy(expected)
    workspace["workspace_members"] = [*workspace["workspace_members"], "crates/does-not-exist"]
    if serialized(workspace) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect workspace drift")
    authority = copy.deepcopy(expected)
    authority["documentation_authority"]["current_program"] = "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"
    if serialized(authority) == serialized(expected):
        raise SystemExit("inventory self-test failed to distinguish current/historical program authority")
    state = copy.deepcopy(expected)
    state["program_state"]["current_architecture_slice"] = "AR-7"
    if serialized(state) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect active AR-8 rollback")
    ar8_state = copy.deepcopy(expected)
    ar8_state["program_state"]["ar8_progress"]["current_subslice"] = "AR-8C"
    if serialized(ar8_state) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect AR-8D sequencing drift")
    credential = copy.deepcopy(expected)
    credential["credential_authority"]["metadata_only"] = False
    if serialized(credential) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect credential-authority projection drift")
    credential_identity = copy.deepcopy(expected)
    credential_identity["credential_authority"]["source_git_blob_sha1"] = "0" * 40
    if serialized(credential_identity) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect credential-authority source identity drift")
    runtime_cleanup = copy.deepcopy(expected)
    runtime_cleanup["runtime_authority_cleanup"]["status"] = "AR5_CANDIDATE"
    if serialized(runtime_cleanup) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect AR-5 runtime-authority acceptance rollback")
    ownership = copy.deepcopy(expected)
    ownership["application_architecture"]["capability_ownership"][0]["application_owner"] = "apps/control-plane-worker"
    if serialized(ownership) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect application ownership drift")
    ar4d = copy.deepcopy(expected)
    ar4d["application_architecture"]["conditional_ar4d"]["decision"] = "REQUIRED"
    if serialized(ar4d) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect unsupported AR-4D activation")
    delivery = copy.deepcopy(expected)
    delivery["current_delivery_map"]["production_enabled"]["status"] = True
    if serialized(delivery) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect premature production enablement in CURRENT_DELIVERY_MAP")
    gate = copy.deepcopy(expected)
    gate["program_state"]["production_core_gate"] = "AUTHORIZED"
    if serialized(gate) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect premature Production Core authorization")
    credential_payload, detected = validate_credential_authority_source()
    credential_negative_self_test(credential_payload, detected)
    print_credential_check_summary(detected, self_tested=True)
    ar3.negative_self_test(ROOT)
    documentation = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-documentation-authority.py"), "--root", str(ROOT), "--self-test"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if documentation.returncode != 0:
        details = "\n".join(value.strip() for value in (documentation.stdout, documentation.stderr) if value.strip())
        raise SystemExit(f"documentation authority negative self-test failed:\n{details}")
    if documentation.stdout.strip():
        print(documentation.stdout.strip())
    python_ops = copy.deepcopy(expected)
    python_ops["python_operational_authority"]["status"] = "AR6_CANDIDATE"
    if serialized(python_ops) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect AR-6 Python/opsctl acceptance rollback")
    print("Architecture inventory CURRENT_DELIVERY_MAP and active AR-8 fail-closed negative self-tests passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    mode.add_argument("--credential-self-test", action="store_true")
    args = parser.parse_args()

    if args.credential_self_test:
        payload, detected = validate_credential_authority_source()
        credential_negative_self_test(payload, detected)
        print_credential_check_summary(detected, self_tested=True)
        return 0

    expected = build_inventory()
    if args.write:
        INVENTORY_PATH.parent.mkdir(parents=True, exist_ok=True)
        INVENTORY_PATH.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print(f"Wrote {INVENTORY_PATH.relative_to(ROOT)}")
    elif args.check:
        check_current(expected)
        validate_full_documentation_authority()
        print("Architecture inventory CURRENT_DELIVERY_MAP projects AR-8C accepted, #352 blocking AR-8D, and production disabled.")
    else:
        self_test(expected)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
