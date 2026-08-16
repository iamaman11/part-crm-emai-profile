#!/usr/bin/env python3
"""Fail closed when Architecture Re-baseline v3 documentation/program authority drifts."""

from __future__ import annotations

import argparse
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any

ACCEPTED_PHASE = "Phase 2I"
CURRENT_PROGRAM = "Architecture Re-baseline v3"
CURRENT_AUTHORITY = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
TRACKING_ISSUE = 266
SUBORDINATE_ISSUE = 268
CURRENT_SLICE = "AR-5"
NEXT_SLICE = "AR-6"
STATUS_DATE = "2026-08-16"
ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5"]
TOPOLOGY = Path("architecture/runtime-topology-ar2.json")
AR2_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR2.md")
AR3_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR3.md")
AR4A_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4A.md")
AR4B_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4B.md")
AR4C_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md")
AR5_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR5.md")

REQUIRED_FILES = (
    Path("README.md"),
    Path("IMPLEMENTATION_PLAN.md"),
    Path("PROFILE_LIFECYCLE_PLAN.md"),
    Path("architecture/accepted-phases.json"),
    Path("architecture/architecture-rebaseline-v3-transition.json"),
    Path("architecture/inventory.json"),
    TOPOLOGY,
    Path("docs/README.md"),
    Path("docs/INDEX.md"),
    Path("docs/status.json"),
    Path("docs/DEVELOPMENT_PLAN.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_AR0.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md"),
    AR2_EVIDENCE,
    AR3_EVIDENCE,
    AR4A_EVIDENCE,
    AR4B_EVIDENCE,
    AR4C_EVIDENCE,
    AR5_EVIDENCE,
    Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"),
    Path("docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"),
    Path("docs/THREAT_MODEL.md"),
    Path("history/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/DEVELOPMENT_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/status_pre_ar_v3_2026-08-15.json"),
    Path("history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/ARCHITECTURE_REBASELINE_V3_PLAN_AR0_ACCEPTED_2026-08-15.md"),
    Path("history/architecture-rebaseline-v3-transition-ar0-accepted-2026-08-15.json"),
)


def read(root: Path, relative: Path) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"missing documentation-authority file: {relative}")
    return path.read_text(encoding="utf-8")


def load_json(root: Path, relative: Path) -> dict[str, Any]:
    try:
        payload = json.loads(read(root, relative))
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid JSON in {relative}: {exc}") from exc
    if not isinstance(payload, dict):
        raise ValueError(f"{relative} must contain a JSON object")
    return payload


def require(text: str, markers: tuple[str, ...], label: str, errors: list[str]) -> None:
    for marker in markers:
        if marker not in text:
            errors.append(f"{label} missing authority marker: {marker}")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    try:
        for relative in REQUIRED_FILES:
            read(root, relative)
        ledger = load_json(root, Path("architecture/accepted-phases.json"))
        status = load_json(root, Path("docs/status.json"))
        transition = load_json(root, Path("architecture/architecture-rebaseline-v3-transition.json"))
        topology = load_json(root, TOPOLOGY)
        inventory = load_json(root, Path("architecture/inventory.json"))
        root_readme = read(root, Path("README.md"))
        docs_readme = read(root, Path("docs/README.md"))
        index = read(root, Path("docs/INDEX.md"))
        development = read(root, Path("docs/DEVELOPMENT_PLAN.md"))
        plan = read(root, Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"))
        ar2_evidence = read(root, AR2_EVIDENCE)
        ar3_evidence = read(root, AR3_EVIDENCE)
        ar4a_evidence = read(root, AR4A_EVIDENCE)
        ar4b_evidence = read(root, AR4B_EVIDENCE)
        ar4c_evidence = read(root, AR4C_EVIDENCE)
        ar5_evidence = read(root, AR5_EVIDENCE)
        pre2j_stub = read(root, Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"))
        implementation_stub = read(root, Path("IMPLEMENTATION_PLAN.md"))
        lifecycle_stub = read(root, Path("PROFILE_LIFECYCLE_PLAN.md"))
    except ValueError as exc:
        return [str(exc)]

    phases = ledger.get("accepted_phases")
    accepted_phase = None
    if isinstance(phases, list) and phases and isinstance(phases[-1], dict):
        accepted_phase = phases[-1].get("phase")
    if accepted_phase != ACCEPTED_PHASE:
        errors.append(f"accepted phase ledger must end at {ACCEPTED_PHASE}; observed {accepted_phase!r}")

    if status.get("schema_version") != 4 or status.get("as_of") != STATUS_DATE:
        errors.append("docs/status.json must be the current AR-5 schema/date projection")
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false throughout accepted AR-5")
    current = status.get("current") if isinstance(status.get("current"), dict) else {}
    if current.get("accepted_product_phase") != ACCEPTED_PHASE:
        errors.append("docs/status.json accepted product phase must remain Phase 2I")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        errors.append("AR-5 architecture_complete/Production Core gate state must remain fail closed")

    program = current.get("architecture_program") if isinstance(current.get("architecture_program"), dict) else {}
    expected_program = {
        "name": CURRENT_PROGRAM,
        "status": "active",
        "authority": CURRENT_AUTHORITY,
        "tracking_issue": TRACKING_ISSUE,
        "subordinate_preproduction_issue": SUBORDINATE_ISSUE,
        "current_slice": CURRENT_SLICE,
        "next_slice_after_acceptance": NEXT_SLICE,
        "runtime_topology_decision": str(TOPOLOGY),
        "runtime_topology_evidence": str(AR2_EVIDENCE),
    }
    for key, expected in expected_program.items():
        if program.get(key) != expected:
            errors.append(f"docs/status.json architecture_program.{key} must be {expected!r}")
    if program.get("accepted_slices") != ACCEPTED_SLICES:
        errors.append(f"docs/status.json accepted_slices must be {ACCEPTED_SLICES!r}")
    ar5 = program.get("ar5_acceptance") if isinstance(program.get("ar5_acceptance"), dict) else {}
    if (
        program.get("runtime_authority_cleanup_evidence") != str(AR5_EVIDENCE)
        or ar5.get("issue") != 290
        or ar5.get("implementation_pr") != 291
        or ar5.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"
        or ar5.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"
        or ar5.get("applicable_permanent_workflows") != "13/13"
    ):
        errors.append("docs/status.json AR-5 acceptance provenance drifted")

    predecessor = current.get("predecessor_external_d3") if isinstance(current.get("predecessor_external_d3"), dict) else {}
    if predecessor.get("issue") != 251:
        errors.append("AR-2 must preserve predecessor issue #251 provenance")
    if predecessor.get("role") != "AR2_CLASSIFIED_SUPERSEDED_FORWARD_PRODUCTION_SEQUENCE":
        errors.append("issue #251 must be classified as superseded forward production sequencing")
    if predecessor.get("legacy_production_lane") != "DISABLED_BY_AR2":
        errors.append("legacy D3 production lane must be disabled after AR-2")
    if predecessor.get("current_state") != "closed_not_planned_after_ar2_acceptance":
        errors.append("issue #251 must remain closed not_planned after AR-2 acceptance")

    pre2j = current.get("pre2j_product_readiness_remediation") if isinstance(current.get("pre2j_product_readiness_remediation"), dict) else {}
    if not (
        pre2j.get("status") == "active_blocking"
        and pre2j.get("tracking_issue") == 203
        and pre2j.get("authority_role") == "HISTORICAL_PREDECESSOR_BLOCKER_ONLY"
        and pre2j.get("forward_execution_authority") is False
    ):
        errors.append("#203 predecessor blocker lifecycle must remain compatible but non-forward")
    phase2j = current.get("phase_2j") if isinstance(current.get("phase_2j"), dict) else {}
    if phase2j.get("status") != "blocked_pending_repository_remediation" or phase2j.get("forward_execution_authority") is not False:
        errors.append("historical Phase 2J state must remain blocked/non-forward")

    if transition.get("schema_version") != 8 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR5_MERGE":
        errors.append("architecture transition must encode accepted AR-5 state")
    if transition.get("tracking_issue") != TRACKING_ISSUE or transition.get("current_authority") != CURRENT_AUTHORITY:
        errors.append("architecture transition authority drifted")
    if transition.get("accepted_slices") != ACCEPTED_SLICES:
        errors.append("architecture transition accepted_slices drifted")
    if transition.get("current_slice") != CURRENT_SLICE or transition.get("next_slice_after_acceptance") != NEXT_SLICE:
        errors.append("architecture transition must encode AR-5 -> AR-6 sequencing")
    transition_state = transition.get("state_model") if isinstance(transition.get("state_model"), dict) else {}
    if transition_state.get("architecture_complete") is not False or transition_state.get("production_core_gate") != "BLOCKED" or transition_state.get("production_ready") is not False:
        errors.append("transition state must remain fail closed through AR-5")
    runtime = transition.get("runtime_topology") if isinstance(transition.get("runtime_topology"), dict) else {}
    if runtime.get("decision_authority") != str(TOPOLOGY) or runtime.get("generation_verification_decision") != "DELETE" or runtime.get("legacy_d3_production_forward_execution") != "DISABLED":
        errors.append("transition lost accepted AR-2 runtime-topology decisions")
    cleanup = transition.get("runtime_authority_cleanup") if isinstance(transition.get("runtime_authority_cleanup"), dict) else {}
    if (
        runtime.get("generation_verification_source_binding_removal") != "ACCEPTED_AR5"
        or runtime.get("runtime_authority_cleanup_evidence") != str(AR5_EVIDENCE)
        or cleanup.get("status") != "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP"
        or cleanup.get("evidence") != str(AR5_EVIDENCE)
        or cleanup.get("implementation_pr") != 291
        or cleanup.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"
        or cleanup.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"
        or cleanup.get("next_required_slice") != "AR-6"
        or cleanup.get("production_mutation") is not False
    ):
        errors.append("transition AR-5 runtime-authority cleanup acceptance drifted")

    if topology.get("slice") != "AR-2" or topology.get("production_mutation") is not False:
        errors.append("AR-2 topology authority must remain non-mutating")
    generation = topology.get("generation_verification") if isinstance(topology.get("generation_verification"), dict) else {}
    if generation.get("decision") != "DELETE" or generation.get("source_binding_removal_slice") != "AR-5":
        errors.append("GENERATION_VERIFICATION delete decision drifted")
    d3 = topology.get("d3_compatibility") if isinstance(topology.get("d3_compatibility"), dict) else {}
    if d3.get("legacy_d3_production_lane") != "DISABLE_FORWARD_EXECUTION" or d3.get("generalize_release_semantics_in") != "AR-11":
        errors.append("AR-2 D3 compatibility decision drifted")

    doc_authority = inventory.get("documentation_authority") if isinstance(inventory.get("documentation_authority"), dict) else {}
    program_state = inventory.get("program_state") if isinstance(inventory.get("program_state"), dict) else {}
    if doc_authority.get("current_program") != CURRENT_AUTHORITY or doc_authority.get("current_slice") != CURRENT_SLICE:
        errors.append("architecture inventory documentation authority is stale")
    if doc_authority.get("runtime_topology_decision") != str(TOPOLOGY):
        errors.append("architecture inventory must point to the accepted AR-2 topology decision")
    if program_state.get("accepted_architecture_slices") != ACCEPTED_SLICES or program_state.get("current_architecture_slice") != CURRENT_SLICE or program_state.get("next_architecture_slice_after_acceptance") != NEXT_SLICE:
        errors.append("architecture inventory AR-5 program state is stale")
    if program_state.get("production_ready") is not False or program_state.get("production_core_gate") != "BLOCKED":
        errors.append("architecture inventory must remain fail closed")
    inventory_cleanup = inventory.get("runtime_authority_cleanup") if isinstance(inventory.get("runtime_authority_cleanup"), dict) else {}
    if (
        inventory_cleanup.get("status") != "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP"
        or inventory_cleanup.get("evidence") != str(AR5_EVIDENCE)
        or inventory_cleanup.get("implementation_pr") != 291
        or inventory_cleanup.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"
        or inventory_cleanup.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"
        or inventory_cleanup.get("next_required_slice") != "AR-6"
        or inventory_cleanup.get("production_mutation") is not False
    ):
        errors.append("architecture inventory AR-5 runtime-authority cleanup projection drifted")

    common = ("Architecture Re-baseline v3", "issue #266", "AR-5", "AR-6", "production_ready=false")
    require(root_readme, common, "README.md", errors)
    require(docs_readme, common, "docs/README.md", errors)
    require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-5", "AR-6"), "docs/INDEX.md", errors)
    require(development, ("Document status:** GENERATED_PROJECTION", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)
    require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-5", "Next slice:** AR-6", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)
    require(ar2_evidence, ("AR-2 Runtime Topology + D3 Compatibility", "GENERATION_VERIFICATION = DELETE", "legacy D3 production lane", "AR-5", "AR-11", "PC-1"), "AR-2 evidence", errors)
    require(ar3_evidence, ("AR-3 Application Architecture Contract", "EVIDENCE / AR-3 accepted", "AR-4A", "AR-4B", "AR-4C", "NOT_REQUIRED", "architecture/inventory.json"), "AR-3 evidence", errors)
    require(ar4a_evidence, ("AR-4A Composition-root consolidation", "EVIDENCE / AR-4A accepted", "f257a30a1df437812edb5c9e4b33c3de7e0740bc", "74672285ef0146c2dc6da298024b378438e5a75d", "AR-4B", "AR-4C", "Production Core remains `BLOCKED`"), "AR-4A evidence", errors)
    require(ar4b_evidence, ("AR-4B Client Mail route ownership", "EVIDENCE / AR-4B accepted", "7ccdd1b0ed0c0eae974cd9bde15c87524315c023", "04b62c97813010ac283d8b70c81089f1c16f5672", "AR-4C", "Production Core remains `BLOCKED`"), "AR-4B evidence", errors)
    require(ar4c_evidence, ("AR-4C Outbound Mail composition extraction", "EVIDENCE / AR-4C accepted", "c62f3a7fb00acf16fa1a8a00d9d2f101949cf8a3", "d8382d1578c4911287fb76dd0b9966b23aa85c25", "AR-5", "Production Core remains `BLOCKED`"), "AR-4C evidence", errors)
    require(ar5_evidence, ("AR-5 Wrangler / Runtime Authority Cleanup", "EVIDENCE / AR-5 accepted", "afed435bb714794d6c4f252be6b44c592ee31b2b", "82d251a1d6666199c6eace393eedc1766157fcee", "13/13 success", "AR-6", "Production Core remains `BLOCKED`"), "AR-5 evidence", errors)
    require(pre2j_stub, ("ACCEPTED_HISTORICAL", "SUPERSEDED_FOR_FORWARD_EXECUTION", "Former tracking issue:** #203", "Current program authority"), "pre-2J compatibility stub", errors)
    require(implementation_stub, ("Document status:** SUPERSEDED", "history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"), "IMPLEMENTATION_PLAN.md", errors)
    require(lifecycle_stub, ("Document status:** SUPERSEDED", "history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"), "PROFILE_LIFECYCLE_PLAN.md", errors)
    return errors


def copy_fixture(source_root: Path, target_root: Path) -> None:
    for relative in REQUIRED_FILES:
        source = source_root / relative
        target = target_root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def mutate(path: Path, old: str, new: str) -> bool:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        return False
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
    return True


def self_test(root: Path) -> bool:
    baseline = validate(root)
    if baseline:
        print("documentation-authority self-test requires a valid baseline")
        for error in baseline:
            print(error)
        return False
    fixtures = [
        ("tracking rollback", Path("docs/status.json"), '"tracking_issue": 266', '"tracking_issue": 203', "tracking_issue"),
        ("slice rollback", Path("docs/status.json"), '"current_slice": "AR-5"', '"current_slice": "AR-4C"', "current_slice"),
        ("premature architecture closeout", Path("docs/status.json"), '"architecture_complete": false', '"architecture_complete": true', "architecture_complete"),
        ("premature gate authorization", Path("docs/status.json"), '"production_core_gate": "BLOCKED"', '"production_core_gate": "AUTHORIZED"', "Production"),
        ("premature production readiness", Path("docs/status.json"), '"production_ready": false', '"production_ready": true', "production_ready"),
        ("generation queue resurrection", TOPOLOGY, '"decision": "DELETE"', '"decision": "KEEP"', "GENERATION_VERIFICATION"),
        ("legacy D3 production resurrection", TOPOLOGY, '"legacy_d3_production_lane": "DISABLE_FORWARD_EXECUTION"', '"legacy_d3_production_lane": "KEEP"', "D3"),
        ("historical #203 resurrected", Path("docs/status.json"), '"forward_execution_authority": false', '"forward_execution_authority": true', "#203"),
    ]
    for label, relative, old, new, expected in fixtures:
        with tempfile.TemporaryDirectory(prefix="ar3-document-authority-") as directory:
            fixture = Path(directory)
            copy_fixture(root, fixture)
            path = fixture / relative
            if not mutate(path, old, new):
                print(f"negative fixture source marker missing for {label}: {old}")
                return False
            errors = validate(fixture)
            if not errors or not any(expected.lower() in error.lower() for error in errors):
                print(f"negative fixture {label} was not rejected by the expected invariant: {errors}")
                return False
    print("Architecture Re-baseline v3 AR-5 documentation authority negative fixtures passed.")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    if args.self_test:
        return 0 if self_test(root) else 1
    errors = validate(root)
    if errors:
        for error in errors:
            print(error)
        return 1
    print("Architecture Re-baseline v3 AR-5 documentation/program authority is consistent.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
