#!/usr/bin/env python3
"""Fail closed when Architecture Re-baseline v3 documentation authority drifts."""

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
CURRENT_SLICE = "AR-1"
NEXT_SLICE = "AR-2"
STATUS_DATE = "2026-08-15"

REQUIRED_FILES = (
    Path("README.md"),
    Path("IMPLEMENTATION_PLAN.md"),
    Path("PROFILE_LIFECYCLE_PLAN.md"),
    Path("architecture/accepted-phases.json"),
    Path("architecture/architecture-rebaseline-v3-transition.json"),
    Path("docs/README.md"),
    Path("docs/INDEX.md"),
    Path("docs/status.json"),
    Path("docs/DEVELOPMENT_PLAN.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_AR0.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md"),
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


def forbid(text: str, markers: tuple[str, ...], label: str, errors: list[str]) -> None:
    for marker in markers:
        if marker in text:
            errors.append(f"{label} retains stale current-authority marker: {marker}")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    try:
        for relative in REQUIRED_FILES:
            read(root, relative)
        ledger = load_json(root, Path("architecture/accepted-phases.json"))
        status = load_json(root, Path("docs/status.json"))
        transition = load_json(root, Path("architecture/architecture-rebaseline-v3-transition.json"))
        root_readme = read(root, Path("README.md"))
        docs_readme = read(root, Path("docs/README.md"))
        index = read(root, Path("docs/INDEX.md"))
        development = read(root, Path("docs/DEVELOPMENT_PLAN.md"))
        plan = read(root, Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"))
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

    if status.get("schema_version") != 3:
        errors.append("docs/status.json schema_version must be 3 after AR-1")
    if status.get("as_of") != STATUS_DATE:
        errors.append(f"docs/status.json as_of must be {STATUS_DATE}")
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false throughout AR-1")

    current = status.get("current")
    if not isinstance(current, dict):
        errors.append("docs/status.json missing current projection")
        current = {}
    if current.get("accepted_product_phase") != ACCEPTED_PHASE:
        errors.append("docs/status.json accepted product phase must remain Phase 2I")
    if current.get("architecture_complete") is not False:
        errors.append("architecture_complete must remain false during AR-1")
    if current.get("production_core_gate") != "BLOCKED":
        errors.append("production_core_gate must remain BLOCKED during AR-1")

    program = current.get("architecture_program")
    if not isinstance(program, dict):
        errors.append("docs/status.json missing current architecture_program")
        program = {}
    expected_program = {
        "name": CURRENT_PROGRAM,
        "status": "active",
        "authority": CURRENT_AUTHORITY,
        "tracking_issue": TRACKING_ISSUE,
        "subordinate_preproduction_issue": SUBORDINATE_ISSUE,
        "current_slice": CURRENT_SLICE,
        "next_slice_after_acceptance": NEXT_SLICE,
    }
    for key, expected in expected_program.items():
        if program.get(key) != expected:
            errors.append(f"docs/status.json architecture_program.{key} must be {expected!r}")
    if program.get("accepted_slices") != ["AR-0"]:
        errors.append("docs/status.json accepted_slices must remain exactly ['AR-0'] before AR-1 merge acceptance")

    phase2j = current.get("phase_2j")
    if not isinstance(phase2j, dict) or phase2j.get("forward_execution_authority") is not False:
        errors.append("Phase 2J must not remain forward execution authority after AR-1 cutover")
    predecessor = current.get("predecessor_external_d3")
    if not isinstance(predecessor, dict) or predecessor.get("issue") != 251 or predecessor.get("state_verified") != "open":
        errors.append("issue #251 must remain recorded as the verified-open predecessor for AR-2 classification")

    if transition.get("schema_version") != 6:
        errors.append("architecture transition schema_version must be 6 after AR-1")
    if transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR1_MERGE":
        errors.append("architecture transition must encode AR-1 activation state")
    if transition.get("tracking_issue") != TRACKING_ISSUE:
        errors.append("architecture transition tracking issue must be #266")
    if transition.get("current_authority") != CURRENT_AUTHORITY:
        errors.append("architecture transition current authority drifted")
    if transition.get("current_slice") != CURRENT_SLICE or transition.get("next_slice_after_acceptance") != NEXT_SLICE:
        errors.append("architecture transition must encode AR-1 -> AR-2 sequencing")
    transition_state = transition.get("state_model")
    if not isinstance(transition_state, dict):
        errors.append("architecture transition missing state_model")
        transition_state = {}
    if transition_state.get("architecture_complete") is not False:
        errors.append("transition architecture_complete must remain false")
    if transition_state.get("production_core_gate") != "BLOCKED":
        errors.append("transition production_core_gate must remain BLOCKED")
    if transition_state.get("production_ready") is not False:
        errors.append("transition production_ready must remain false")

    cutover = transition.get("authority_cutover")
    if not isinstance(cutover, dict):
        errors.append("architecture transition missing authority_cutover")
    else:
        if cutover.get("former_tracking_issue") != 203:
            errors.append("transition must preserve #203 as historical predecessor provenance")
        if cutover.get("former_status") != "ACCEPTED_HISTORICAL_SUPERSEDED_FOR_FORWARD_EXECUTION":
            errors.append("former #203 authority must be explicitly historical/superseded")
        if cutover.get("history_rewritten") is not False:
            errors.append("AR-1 must record that accepted history was not rewritten")

    gate_transitions = transition.get("gate_transitions")
    if not isinstance(gate_transitions, dict):
        errors.append("architecture transition missing gate_transitions")
    else:
        ar17 = gate_transitions.get("after_successful_ar17")
        pc1 = gate_transitions.get("after_successful_pc1")
        if not isinstance(ar17, dict) or ar17.get("production_ready") is not False or ar17.get("production_core_gate") != "AUTHORIZED":
            errors.append("AR-17 must authorize the Production Core gate without setting production_ready=true")
        if not isinstance(pc1, dict) or pc1.get("production_ready") is not True or pc1.get("scope") != "production-core-v1":
            errors.append("only PC-1 may project production_ready=true for production-core-v1")

    common = (
        "Architecture Re-baseline v3",
        "issue #266",
        "AR-1",
        "production_ready=false",
    )
    require(root_readme, common, "README.md", errors)
    require(docs_readme, common, "docs/README.md", errors)
    require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "ACCEPTED_HISTORICAL"), "docs/INDEX.md", errors)
    require(development, ("Document status:** GENERATED_PROJECTION", "AR-0   Delta Architecture Inventory", "AR-1   Architecture Authority Re-baseline", "AR-16  Final Whole-project 10/10 Audit", "AR-17  Architecture Closeout + Production Core Gate", "PC-1 Production Core v1", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)
    require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "AR-1   Architecture Authority Re-baseline", "AR-16  Final Whole-project 10/10 Audit", "AR-17  Architecture Closeout + Production Core Gate", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)
    require(pre2j_stub, ("ACCEPTED_HISTORICAL", "SUPERSEDED_FOR_FORWARD_EXECUTION", "Former tracking issue:** #203", "Current program authority"), "pre-2J compatibility stub", errors)
    require(implementation_stub, ("Document status:** SUPERSEDED", "history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"), "IMPLEMENTATION_PLAN.md", errors)
    require(lifecycle_stub, ("Document status:** SUPERSEDED", "history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"), "PROFILE_LIFECYCLE_PLAN.md", errors)

    stale_entrypoint_markers = (
        "Current execution authority is\n  [`docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md`",
        "current pre-2J product-readiness remediation authority, issue #203",
        "Pre-2J product-readiness remediation: ACTIVE / BLOCKING Phase 2J",
    )
    for label, text in (("README.md", root_readme), ("docs/README.md", docs_readme), ("docs/INDEX.md", index), ("docs/DEVELOPMENT_PLAN.md", development)):
        forbid(text, stale_entrypoint_markers, label, errors)

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
        ("tracking issue rollback", Path("docs/status.json"), '"tracking_issue": 266', '"tracking_issue": 203', "tracking_issue"),
        ("slice rollback", Path("docs/status.json"), '"current_slice": "AR-1"', '"current_slice": "Batch A"', "current_slice"),
        ("premature architecture closeout", Path("docs/status.json"), '"architecture_complete": false', '"architecture_complete": true', "architecture_complete"),
        ("premature gate authorization", Path("docs/status.json"), '"production_core_gate": "BLOCKED"', '"production_core_gate": "AUTHORIZED"', "production_core_gate"),
        ("premature production readiness", Path("docs/status.json"), '"production_ready": false', '"production_ready": true', "production_ready"),
        ("AR-17 production-ready collapse", Path("architecture/architecture-rebaseline-v3-transition.json"), '"production_ready": false,\n      "production_mutation": false\n    },\n    "after_successful_pc1"', '"production_ready": true,\n      "production_mutation": false\n    },\n    "after_successful_pc1"', "AR-17"),
        ("historical #203 resurrected", Path("architecture/architecture-rebaseline-v3-transition.json"), '"former_status": "ACCEPTED_HISTORICAL_SUPERSEDED_FOR_FORWARD_EXECUTION"', '"former_status": "CURRENT_AUTHORITY"', "historical/superseded"),
        ("current plan loses authority", Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"), "Document status:** CURRENT_AUTHORITY", "Document status:** TARGET", "CURRENT_AUTHORITY"),
        ("old plan loses historical classification", Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"), "ACCEPTED_HISTORICAL", "CURRENT_AUTHORITY", "ACCEPTED_HISTORICAL"),
    ]

    for label, relative, old, new, expected in fixtures:
        with tempfile.TemporaryDirectory(prefix="ar1-document-authority-") as directory:
            fixture = Path(directory)
            copy_fixture(root, fixture)
            path = fixture / relative
            if not mutate(path, old, new):
                print(f"negative fixture source marker missing for {label}: {old}")
                return False
            errors = validate(fixture)
            if not errors or not any(expected.lower() in error.lower() for error in errors):
                print(f"negative documentation fixture unexpectedly passed: {label}")
                for error in errors:
                    print(error)
                return False
            print(f"negative documentation fixture rejected as expected: {label}")
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
    print("Architecture Re-baseline v3 documentation authority: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
