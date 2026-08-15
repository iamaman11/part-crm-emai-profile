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
CURRENT_SLICE = "AR-2"
NEXT_SLICE = "AR-3"
STATUS_DATE = "2026-08-15"
ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2"]
TOPOLOGY = Path("architecture/runtime-topology-ar2.json")
AR2_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR2.md")

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
        errors.append("docs/status.json must be the AR-2 schema/date projection")
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false throughout AR-2")
    current = status.get("current") if isinstance(status.get("current"), dict) else {}
    if current.get("accepted_product_phase") != ACCEPTED_PHASE:
        errors.append("docs/status.json accepted product phase must remain Phase 2I")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        errors.append("AR-2 architecture/gate state must remain fail closed")

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

    predecessor = current.get("predecessor_external_d3") if isinstance(current.get("predecessor_external_d3"), dict) else {}
    if predecessor.get("issue") != 251:
        errors.append("AR-2 must preserve predecessor issue #251 provenance")
    if predecessor.get("role") != "AR2_CLASSIFIED_SUPERSEDED_FORWARD_PRODUCTION_SEQUENCE":
        errors.append("issue #251 must be classified as superseded forward production sequencing")
    if predecessor.get("legacy_production_lane") != "DISABLED_BY_AR2":
        errors.append("legacy D3 production lane must be disabled after AR-2")

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

    if transition.get("schema_version") != 7 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR2_MERGE":
        errors.append("architecture transition must encode accepted AR-2 state")
    if transition.get("tracking_issue") != TRACKING_ISSUE or transition.get("current_authority") != CURRENT_AUTHORITY:
        errors.append("architecture transition authority drifted")
    if transition.get("accepted_slices") != ACCEPTED_SLICES:
        errors.append("architecture transition accepted_slices drifted")
    if transition.get("current_slice") != CURRENT_SLICE or transition.get("next_slice_after_acceptance") != NEXT_SLICE:
        errors.append("architecture transition must encode AR-2 -> AR-3 sequencing")
    transition_state = transition.get("state_model") if isinstance(transition.get("state_model"), dict) else {}
    if transition_state.get("architecture_complete") is not False or transition_state.get("production_core_gate") != "BLOCKED" or transition_state.get("production_ready") is not False:
        errors.append("transition state must remain fail closed through AR-2")
    runtime = transition.get("runtime_topology") if isinstance(transition.get("runtime_topology"), dict) else {}
    if runtime.get("decision_authority") != str(TOPOLOGY) or runtime.get("generation_verification_decision") != "DELETE" or runtime.get("legacy_d3_production_forward_execution") != "DISABLED":
        errors.append("transition lost accepted AR-2 runtime-topology decisions")

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
        errors.append("architecture inventory AR-2 program state is stale")
    if program_state.get("production_ready") is not False or program_state.get("production_core_gate") != "BLOCKED":
        errors.append("architecture inventory must remain fail closed")

    common = ("Architecture Re-baseline v3", "issue #266", "AR-2", "AR-3", "production_ready=false")
    require(root_readme, common, "README.md", errors)
    require(docs_readme, common, "docs/README.md", errors)
    require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-2", "AR-3"), "docs/INDEX.md", errors)
    require(development, ("Document status:** GENERATED_PROJECTION", "AR-1   Architecture Authority Re-baseline", "AR-2   Runtime Topology + D3 Compatibility", "AR-3   Application Architecture Contract", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)
    require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-2", "AR-2   Runtime Topology + D3 Compatibility", "AR-3   Application Architecture Contract", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)
    require(ar2_evidence, ("AR-2 Runtime Topology + D3 Compatibility", "GENERATION_VERIFICATION = DELETE", "legacy D3 production lane", "AR-5", "AR-11", "PC-1"), "AR-2 evidence", errors)
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
        ("slice rollback", Path("docs/status.json"), '"current_slice": "AR-2"', '"current_slice": "AR-1"', "current_slice"),
        ("premature architecture closeout", Path("docs/status.json"), '"architecture_complete": false', '"architecture_complete": true', "architecture_complete"),
        ("premature gate authorization", Path("docs/status.json"), '"production_core_gate": "BLOCKED"', '"production_core_gate": "AUTHORIZED"', "Production"),
        ("premature production readiness", Path("docs/status.json"), '"production_ready": false', '"production_ready": true', "production_ready"),
        ("generation queue resurrection", TOPOLOGY, '"decision": "DELETE"', '"decision": "KEEP"', "GENERATION_VERIFICATION"),
        ("legacy D3 production resurrection", TOPOLOGY, '"legacy_d3_production_lane": "DISABLE_FORWARD_EXECUTION"', '"legacy_d3_production_lane": "KEEP"', "D3"),
        ("historical #203 resurrected", Path("docs/status.json"), '"forward_execution_authority": false', '"forward_execution_authority": true', "#203"),
    ]
    for label, relative, old, new, expected in fixtures:
        with tempfile.TemporaryDirectory(prefix="ar2-document-authority-") as directory:
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
    print("Architecture Re-baseline v3 AR-2 documentation authority negative fixtures passed.")
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
    print("Architecture Re-baseline v3 AR-2 documentation/program authority is consistent.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
