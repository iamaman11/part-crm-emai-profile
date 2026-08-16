#!/usr/bin/env python3
"""Generate and verify the canonical architecture inventory during AR-3.

The proven workspace/migration/route/generated-contract core remains in
`_architecture_inventory_core.py`. AR-3 consumes the accepted AR-2 topology and projects
runtime-resource/application ownership into the same canonical architecture inventory.
"""

from __future__ import annotations

import argparse
import copy
import json
import subprocess
import sys
from pathlib import Path

import _architecture_inventory_core as core
import _ar3_application_architecture as ar3

ROOT = Path(__file__).resolve().parents[1]
INVENTORY_PATH = ROOT / "architecture" / "inventory.json"
CURRENT_AUTHORITY = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
TRANSITION = "architecture/architecture-rebaseline-v3-transition.json"
RUNTIME_TOPOLOGY = "architecture/runtime-topology-ar2.json"
AR2_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR2.md"
AR4A_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md"
AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"
AR4C_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"
AR5_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR5.md"
TRACKING_ISSUE = 266
ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5"]
CURRENT_SLICE = "AR-5"
NEXT_SLICE = "AR-6"

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
    {"path": RUNTIME_TOPOLOGY, "status": "STABLE_AUTHORITY", "scope": "accepted_ar2_runtime_topology_decision_input_for_ar3"},
    {"path": ar3.AR3_EVIDENCE, "status": "EVIDENCE", "scope": "ar3_application_architecture_contract_accepted"},
    {"path": AR4A_EVIDENCE, "status": "EVIDENCE", "scope": "ar4a_composition_root_consolidation_accepted"},
    {"path": AR4B_EVIDENCE, "status": "EVIDENCE", "scope": "ar4b_client_mail_route_ownership_accepted"},
    {"path": AR4C_EVIDENCE, "status": "EVIDENCE", "scope": "ar4c_outbound_mail_composition_accepted"},
    {"path": AR5_EVIDENCE, "status": "EVIDENCE", "scope": "ar5_runtime_authority_cleanup_accepted"},
    {"path": "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md", "status": "ACCEPTED_HISTORICAL", "scope": "superseded_predecessor_forward_execution"},
    {"path": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md", "status": "ACCEPTED_HISTORICAL", "scope": "accepted_r1_r9_closeout"},
    {"path": "IMPLEMENTATION_PLAN.md", "status": "SUPERSEDED", "scope": "compatibility_entrypoint_to_preserved_history"},
    {"path": "PROFILE_LIFECYCLE_PLAN.md", "status": "SUPERSEDED", "scope": "compatibility_entrypoint_to_preserved_history"},
]


def validate_docs() -> None:
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
    for item in DOCUMENT_STATUS:
        if not (ROOT / item["path"]).is_file():
            raise SystemExit(f"document-status inventory path missing: {item['path']}")
    status = json.loads((ROOT / "docs/status.json").read_text(encoding="utf-8"))
    current = status.get("current", {})
    program = current.get("architecture_program", {}) if isinstance(current, dict) else {}
    if status.get("production_ready") is not False:
        raise SystemExit("docs/status.json must remain production_ready=false during accepted AR-5")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        raise SystemExit("docs/status.json must keep accepted AR-5 architecture/gate state fail closed")
    if program.get("authority") != CURRENT_AUTHORITY or program.get("tracking_issue") != TRACKING_ISSUE:
        raise SystemExit("docs/status.json current architecture authority drifted")
    if program.get("accepted_slices") != ACCEPTED_SLICES or program.get("current_slice") != CURRENT_SLICE or program.get("next_slice_after_acceptance") != NEXT_SLICE:
        raise SystemExit("docs/status.json must project accepted AR-5 -> next AR-6 sequencing")
    if program.get("runtime_topology_decision") != RUNTIME_TOPOLOGY:
        raise SystemExit("docs/status.json must project the accepted AR-2 topology decision")
    if program.get("runtime_authority_cleanup_evidence") != AR5_EVIDENCE:
        raise SystemExit("docs/status.json must project accepted AR-5 runtime-authority cleanup evidence")
    runtime_gate = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-cloudflare-runtime-bindings.py")],
        cwd=ROOT, text=True, capture_output=True, check=False,
    )
    if runtime_gate.returncode != 0:
        details = "\n".join(value.strip() for value in (runtime_gate.stdout, runtime_gate.stderr) if value.strip())
        raise SystemExit(f"AR-5 runtime authority gate failed:\n{details}")


def build_inventory() -> dict[str, object]:
    core.validate_route_ownership()
    validate_docs()
    application_architecture = ar3.build_projection(ROOT)
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
            "topology_decision_source": RUNTIME_TOPOLOGY,
            "evidence": AR5_EVIDENCE,
            "implementation_issue": 290,
            "implementation_pr": 291,
            "exact_green_head": "afed435bb714794d6c4f252be6b44c592ee31b2b",
            "implementation_merge": "82d251a1d6666199c6eace393eedc1766157fcee",
            "applicable_permanent_workflows": "13/13",
            "generation_verification": {
                "topology_decision": "DELETE",
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
        "documentation_authority": {
            "current_program": CURRENT_AUTHORITY,
            "tracking_issue": TRACKING_ISSUE,
            "current_slice": CURRENT_SLICE,
            "transition": TRANSITION,
            "runtime_topology_decision": RUNTIME_TOPOLOGY,
            "runtime_topology_evidence": AR2_EVIDENCE,
            "runtime_topology_projection_owner": "AR-3",
            "runtime_authority_cleanup_evidence": AR5_EVIDENCE,
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
    state["program_state"]["current_architecture_slice"] = "AR-1"
    if serialized(state) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect accepted checkpoint rollback")
    topology = copy.deepcopy(expected)
    topology["documentation_authority"]["runtime_topology_decision"] = "architecture/other.json"
    if serialized(topology) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect topology authority drift")
    runtime_cleanup = copy.deepcopy(expected)
    runtime_cleanup["runtime_authority_cleanup"]["status"] = "AR5_CANDIDATE"
    if serialized(runtime_cleanup) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect AR-5 runtime-authority acceptance rollback")
    ownership = copy.deepcopy(expected)
    ownership["application_architecture"]["capability_ownership"][0]["application_owner"] = "apps/control-plane-worker"
    if serialized(ownership) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect application ownership drift")
    missing_resource = copy.deepcopy(expected)
    missing_resource["application_architecture"]["runtime_resources"] = missing_resource["application_architecture"]["runtime_resources"][1:]
    if serialized(missing_resource) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect missing runtime-resource projection")
    ar4d = copy.deepcopy(expected)
    ar4d["application_architecture"]["conditional_ar4d"]["decision"] = "REQUIRED"
    if serialized(ar4d) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect unsupported AR-4D activation")
    gate = copy.deepcopy(expected)
    gate["program_state"]["production_core_gate"] = "AUTHORIZED"
    if serialized(gate) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect premature Production Core authorization")
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
    print("Architecture inventory accepted AR-5 runtime-authority negative self-test passed.")


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
        print("Architecture inventory and accepted AR-5 runtime-authority projection are current.")
    else:
        self_test(expected)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
