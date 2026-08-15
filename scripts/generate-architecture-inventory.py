#!/usr/bin/env python3
"""Generate and verify the canonical architecture inventory after AR-1.

The proven workspace/migration/route/generated-contract core is reused byte-for-byte from
`_architecture_inventory_core.py`. This public generator owns the current authority/document-status
projection and remains the only generator for `architecture/inventory.json`.
"""

from __future__ import annotations

import argparse
import copy
import json
import subprocess
import sys
from pathlib import Path

import _architecture_inventory_core as core

ROOT = Path(__file__).resolve().parents[1]
INVENTORY_PATH = ROOT / "architecture" / "inventory.json"

CURRENT_AUTHORITY = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
TRANSITION = "architecture/architecture-rebaseline-v3-transition.json"
TRACKING_ISSUE = 266

REQUIRED_INDEX_LINKS = [
    "ARCHITECTURE_REBASELINE_V3_PLAN.md",
    "ARCHITECTURE_REBASELINE_V3_AR0.md",
    "ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md",
    "DEVELOPMENT_PLAN.md",
    "ARCHITECTURE.md",
    "DATA_CLASSIFICATION.md",
    "UI_ARCHITECTURE.md",
    "DEVELOPER_CAPABILITY_MATRIX.md",
    "accepted-phases.json",
    "REALTIME_NOTIFICATIONS.md",
    "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
    "PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
    "status.json",
    "THREAT_MODEL.md",
    "inventory.json",
]

DOCUMENT_STATUS = [
    {
        "path": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "status": "CURRENT_AUTHORITY",
        "scope": "architecture_program_execution",
    },
    {
        "path": "docs/DEVELOPMENT_PLAN.md",
        "status": "GENERATED_PROJECTION",
        "scope": "product_program_projection_and_accepted_phase_provenance",
    },
    {
        "path": "docs/status.json",
        "status": "GENERATED_PROJECTION",
        "scope": "machine_readable_program_and_readiness_state",
    },
    {
        "path": "docs/INDEX.md",
        "status": "GENERATED_PROJECTION",
        "scope": "documentation_authority_navigation",
    },
    {
        "path": "README.md",
        "status": "GENERATED_PROJECTION",
        "scope": "repository_entrypoint",
    },
    {
        "path": "docs/README.md",
        "status": "GENERATED_PROJECTION",
        "scope": "documentation_entrypoint",
    },
    {
        "path": "docs/ARCHITECTURE.md",
        "status": "STABLE_AUTHORITY",
        "scope": "accepted_architecture_invariants",
    },
    {
        "path": "docs/DATA_CLASSIFICATION.md",
        "status": "STABLE_AUTHORITY",
        "scope": "data_privacy_classification",
    },
    {
        "path": "docs/THREAT_MODEL.md",
        "status": "STABLE_AUTHORITY",
        "scope": "repository_security_threat_model",
    },
    {
        "path": "docs/UI_ARCHITECTURE.md",
        "status": "STABLE_AUTHORITY",
        "scope": "ui_architecture_target",
    },
    {
        "path": "docs/DEVELOPER_CAPABILITY_MATRIX.md",
        "status": "STABLE_AUTHORITY",
        "scope": "accepted_capability_evidence",
    },
    {
        "path": "architecture/accepted-phases.json",
        "status": "STABLE_AUTHORITY",
        "scope": "immutable_accepted_product_phase_provenance",
    },
    {
        "path": "architecture/architecture-rebaseline-v3-transition.json",
        "status": "GENERATED_PROJECTION",
        "scope": "architecture_program_transition_state",
    },
    {
        "path": "docs/ARCHITECTURE_REBASELINE_V3_AR0.md",
        "status": "EVIDENCE",
        "scope": "ar0_research_acceptance",
    },
    {
        "path": "docs/ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md",
        "status": "EVIDENCE",
        "scope": "ar0_second_pass_research",
    },
    {
        "path": "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
        "status": "ACCEPTED_HISTORICAL",
        "scope": "superseded_predecessor_forward_execution",
    },
    {
        "path": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
        "status": "ACCEPTED_HISTORICAL",
        "scope": "accepted_r1_r9_closeout",
    },
    {
        "path": "IMPLEMENTATION_PLAN.md",
        "status": "SUPERSEDED",
        "scope": "compatibility_entrypoint_to_preserved_history",
    },
    {
        "path": "PROFILE_LIFECYCLE_PLAN.md",
        "status": "SUPERSEDED",
        "scope": "compatibility_entrypoint_to_preserved_history",
    },
]


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
    authority_plan = (ROOT / "docs" / "ARCHITECTURE_REBASELINE_V3_PLAN.md").read_text(encoding="utf-8")
    architecture = (ROOT / "docs" / "ARCHITECTURE.md").read_text(encoding="utf-8")
    matrix = (ROOT / "docs" / "DEVELOPER_CAPABILITY_MATRIX.md").read_text(encoding="utf-8")

    required_plan_markers = (
        "Current architecture/program authority:** `ARCHITECTURE_REBASELINE_V3_PLAN.md`",
        "Tracking:** issue #266",
        "AR-0   Delta Architecture Inventory",
        "AR-1   Architecture Authority Re-baseline",
        "AR-16  Final Whole-project 10/10 Audit",
        "AR-17  Architecture Closeout + Production Core Gate",
        "PC-1 Production Core v1",
        "`production_ready=false`",
        "Immutable Accepted Phase Provenance",
    )
    for marker in required_plan_markers:
        if marker not in plan:
            raise SystemExit(f"DEVELOPMENT_PLAN.md is missing AR-1 program marker: {marker}")

    stale_plan_markers = (
        "Current remediation authority:** `PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`",
        "current repository-owned follow-up is issue #203",
        "Current repository work: issue #203 remediation",
        "issue #203 product-readiness remediation\n  -> Batch 0",
    )
    for marker in stale_plan_markers:
        if marker in plan:
            raise SystemExit(f"DEVELOPMENT_PLAN.md retains stale forward authority: {marker}")

    required_authority_markers = (
        "Document status:** CURRENT_AUTHORITY",
        "Tracking issue:** #266",
        "AR-1   Architecture Authority Re-baseline",
        "AR-16  Final Whole-project 10/10 Audit",
        "AR-17  Architecture Closeout + Production Core Gate",
        "source_present != production_enabled",
        "one concern -> one legitimate mutable authority",
    )
    for marker in required_authority_markers:
        if marker not in authority_plan:
            raise SystemExit(f"current architecture authority is missing binding marker: {marker}")

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
    for marker in required_architecture_markers:
        if marker not in architecture:
            raise SystemExit(f"ARCHITECTURE.md lost accepted architecture marker: {marker}")

    required_matrix_markers = (
        "| Client contact protection | Composed |",
        "| Client Registry 2.0 | Composed |",
        "| Read models/global search | Composed / Synthetic |",
        "| Client-scoped mailbox message search/body | Composed / Synthetic |",
        "| Device job/browser mailbox execution | Composed / Synthetic |",
        "| Realtime UserNotificationHub | Composed / Synthetic |",
        "| Complete standalone UI | Composed / Synthetic |",
        "| Integrated release-candidate hardening | Composed / Synthetic |",
        "| A5 | Feature-sliced SPA route composition | **Accepted through Phase 2I**",
        "| A8 | CQRS/read-model boundary | **Accepted through Phase 2I**",
        "| 6.2 | Durable-before-notify | **Accepted through Phase 2I.**",
        "| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2I.**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2I.**",
        "| 6.6 | Profile materialization | **Accepted repository-local through Phase 2I**",
    )
    for marker in required_matrix_markers:
        if marker not in matrix:
            raise SystemExit(f"DEVELOPER_CAPABILITY_MATRIX.md lost accepted capability marker: {marker}")

    for contract in core.GENERATED_CONTRACTS:
        for key in ("canonical_source", "openapi", "typescript", "generator"):
            relative_path = contract[key]
            if not (ROOT / relative_path).is_file():
                raise SystemExit(f"generated contract {contract['name']} references missing {key}: {relative_path}")

    status = json.loads((ROOT / "docs" / "status.json").read_text(encoding="utf-8"))
    current = status.get("current", {})
    program = current.get("architecture_program", {}) if isinstance(current, dict) else {}
    if status.get("production_ready") is not False:
        raise SystemExit("docs/status.json must remain production_ready=false during AR-1")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        raise SystemExit("docs/status.json must keep AR-1 architecture/gate state fail closed")
    if program.get("authority") != CURRENT_AUTHORITY or program.get("tracking_issue") != TRACKING_ISSUE:
        raise SystemExit("docs/status.json must project the v3 current authority tracked by #266")
    if program.get("current_slice") != "AR-1" or program.get("next_slice_after_acceptance") != "AR-2":
        raise SystemExit("docs/status.json must project AR-1 -> AR-2 sequencing")


def build_inventory() -> dict[str, object]:
    core.validate_route_ownership()
    validate_docs()

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

    inventory: dict[str, object] = {
        "schema_version": 2,
        "workspace_members": core.workspace_members(),
        "d1_migrations": {"directory": "migrations/d1", "files": core.migration_files()},
        "routing": {
            "composed_entrypoint": "crates/control-plane-contract/src/lib.rs::classify_route",
            "dynamic_namespaces": ["/api", "/auth", "/bridge"],
            "classifiers": [
                {"capability": capability, "module": path} for capability, path in core.CLASSIFIERS
            ],
            "public_routes": routes,
        },
        "generated_contracts": core.GENERATED_CONTRACTS,
        "documentation_authority": {
            "current_program": CURRENT_AUTHORITY,
            "tracking_issue": TRACKING_ISSUE,
            "current_slice": "AR-1",
            "transition": TRANSITION,
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
            "accepted_architecture_slices": ["AR-0"],
            "current_architecture_slice": "AR-1",
            "next_architecture_slice_after_acceptance": "AR-2",
            "architecture_complete": False,
            "production_core_gate": "BLOCKED",
            "production_ready": False,
            "production_mutation_allowed_during_ar0_ar17": False,
            "production_ready_authority": "PC-1_AFTER_AR-17_AUTHORIZATION",
        },
    }

    for item in DOCUMENT_STATUS:
        if not (ROOT / item["path"]).is_file():
            raise SystemExit(f"document-status inventory path missing: {item['path']}")
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
        raise SystemExit("inventory self-test failed to detect deterministic workspace drift")

    route = copy.deepcopy(expected)
    route["routing"]["public_routes"][0]["route_class"] = "UnknownApi"
    if serialized(route) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect route drift")

    authority = copy.deepcopy(expected)
    authority["documentation_authority"]["current_program"] = "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"
    if serialized(authority) == serialized(expected):
        raise SystemExit("inventory self-test failed to distinguish current and historical program authority")

    state = copy.deepcopy(expected)
    state["program_state"]["production_core_gate"] = "AUTHORIZED"
    if serialized(state) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect premature Production Core authorization")

    documentation = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "check-documentation-authority.py"), "--root", str(ROOT), "--self-test"],
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
    print("Architecture inventory AR-1 negative self-test passed.")


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
        print("Architecture inventory and AR-1 authority consistency are current.")
    else:
        self_test(expected)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
