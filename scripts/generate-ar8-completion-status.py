#!/usr/bin/env python3
"""Deterministically project accepted AR-9 and current AR-10 into canonical status authorities."""

from __future__ import annotations

import argparse
import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_STATUS = ROOT / "history/status-ar8c-before-ar8-completion.json"
BASE_TRANSITION = ROOT / "history/architecture-rebaseline-v3-transition-before-ar8-completion.json"
STATUS = ROOT / "docs/status.json"
TRANSITION = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
AR8_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-18-ar8-final-acceptance.json"
AR8_EXACT_GREEN_HEAD = "81d1f0c26ff0bd3a688c2d5dc000b93640479e47"
AR8_IMPLEMENTATION_MERGE = "874666f6ef6eb003425c9677d558378d6dc0daaf"
AR9_ISSUE = 366
AR9_PR = 367
AR9_EXACT_GREEN_HEAD = "6110a32ade85d08c6ad93d9064190fff768e7cc2"
AR9_IMPLEMENTATION_MERGE = "5933a5e30a534209138485556b4a895706af765a"
AR9_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar9-final-acceptance.json"
AR9_AUTHORITY = "architecture/d1-evolution-ar9.json"
AR9_PROJECTION = "architecture/inventory.json::d1_evolution"
AR10_ISSUE = 368
AR10_AUTHORITY = "architecture/runtime-cutover-ar10.json"
AR10_PROJECTION = "architecture/inventory.json::runtime_cutover"
AR10_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR10.md"


def ar8_progress(*, projection_fields: bool) -> dict[str, object]:
    value: dict[str, object] = {
        "umbrella_issue": 308,
        "accepted_subslices": ["AR-8A", "AR-8B", "AR-8C", "AR-8D", "AR-8E", "AR-8F"],
        "current_subslice": None,
        "current_implementation_issue": None,
        "mandatory_remaining": [],
        "full_ar8_accepted": True,
        "ar9_blocked": False,
        "production_mutation": False,
        "implementation_entry_gate": "AR8_ACCEPTED_MAIN_AR9_CURRENT",
        "source_complete_candidate": False,
        "completion_pr": 362,
        "implemented_through": "AR-8F",
        "accepted_top_level_slice": "AR-8",
        "exact_green_head": AR8_EXACT_GREEN_HEAD,
        "implementation_merge": AR8_IMPLEMENTATION_MERGE,
        "applicable_permanent_workflows": "14/14",
        "accepted_main_reread": AR8_IMPLEMENTATION_MERGE,
        "acceptance_evidence": AR8_ACCEPTANCE_EVIDENCE,
    }
    if projection_fields:
        value.update({
            "credential_authority_source": "architecture/credential-authority.json",
            "credential_registry_provenance": "architecture/credential-authority-ar8b.json",
            "canonical_projection": "architecture/inventory.json::subject_domain_authorities",
        })
    return value


def delivery_map(base: dict[str, object]) -> dict[str, object]:
    value = copy.deepcopy(base)
    value["accepted_checkpoint"] = "AR-9"
    value["current_work"] = "AR-10"
    value["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-9",
        "current_subslice": "AR-10",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR9_CLOSEOUT",
    }
    value["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-9",
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "acceptance_evidence": AR9_ACCEPTANCE_EVIDENCE,
    }
    value["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    value["next_gate"] = {
        "id": "AR-10_ACCEPTANCE",
        "issue": AR10_ISSUE,
        "on_success": "AR-11_BECOMES_CURRENT",
    }
    value["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "ar10_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return value


def ar8_acceptance() -> dict[str, object]:
    return {
        "issue": 361,
        "umbrella_issue": 308,
        "implementation_pr": 362,
        "exact_green_head": AR8_EXACT_GREEN_HEAD,
        "implementation_merge": AR8_IMPLEMENTATION_MERGE,
        "applicable_permanent_workflows": "14/14",
        "behind_by": 0,
        "blocking_reviews": 0,
        "unresolved_review_threads": 0,
        "accepted_main_reread": AR8_IMPLEMENTATION_MERGE,
        "evidence": AR8_ACCEPTANCE_EVIDENCE,
        "metadata_only": True,
        "production_mutation": False,
    }


def ar9_acceptance() -> dict[str, object]:
    return {
        "issue": AR9_ISSUE,
        "implementation_pr": AR9_PR,
        "exact_green_head": AR9_EXACT_GREEN_HEAD,
        "implementation_merge": AR9_IMPLEMENTATION_MERGE,
        "applicable_permanent_workflows": "15/15",
        "behind_by": 0,
        "blocking_reviews": 0,
        "unresolved_review_threads": 0,
        "accepted_main_reread": AR9_IMPLEMENTATION_MERGE,
        "evidence": AR9_ACCEPTANCE_EVIDENCE,
        "metadata_only": True,
        "production_mutation": False,
    }


def ar10_projection() -> dict[str, object]:
    return {
        "tracking_issue": AR10_ISSUE,
        "source_authority": AR10_AUTHORITY,
        "canonical_projection": AR10_PROJECTION,
        "evidence": AR10_EVIDENCE,
        "production_mutation": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
    }


def project_status() -> dict[str, object]:
    payload = json.loads(BASE_STATUS.read_text(encoding="utf-8"))
    current = payload["current"]
    program = current["architecture_program"]
    accepted_slices = list(program.get("accepted_slices", []))
    for accepted in ("AR-8", "AR-9"):
        if accepted not in accepted_slices:
            accepted_slices.append(accepted)
    program["accepted_slices"] = accepted_slices
    program["current_slice"] = "AR-10"
    program["next_slice_after_acceptance"] = "AR-11"
    program["ar8_progress"] = ar8_progress(projection_fields=False)
    program["ar8_acceptance"] = ar8_acceptance()
    program.pop("ar9_current", None)
    program["ar9_acceptance"] = ar9_acceptance()
    program["ar10_current"] = ar10_projection()
    program["subject_domain_authorities"] = {
        "credential_authority": "architecture/credential-authority.json",
        "credential_registry_provenance": "architecture/credential-authority-ar8b.json",
        "credential_lifecycle": "architecture/credential-lifecycle.json",
        "operator_contract": "architecture/operator-contract.json",
        "profile_security": "architecture/profile-security.json",
        "architecture_map": "architecture/README.md",
    }
    current["current_delivery_map"] = delivery_map(current["current_delivery_map"])
    current["architecture_complete"] = False
    current["production_core_gate"] = "BLOCKED"
    current["next_repository_step"] = {
        "name": "AR-10 — Runtime and Historical Executable Simplification",
        "status": "current",
        "tracking_issue": AR10_ISSUE,
        "program_tracking_issue": 266,
        "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "slice_authority": AR10_AUTHORITY,
        "canonical_projection": AR10_PROJECTION,
        "previous_acceptance_evidence": AR9_ACCEPTANCE_EVIDENCE,
    }
    payload["production_ready"] = False
    payload["as_of"] = "2026-08-19"
    return payload


def project_transition() -> dict[str, object]:
    payload = json.loads(BASE_TRANSITION.read_text(encoding="utf-8"))
    payload["status"] = "ACTIVE_AFTER_AR9_ACCEPTANCE"
    payload["current_slice"] = "AR-10"
    payload["next_slice_after_acceptance"] = "AR-11"
    accepted_slices = list(payload.get("accepted_slices", []))
    for accepted in ("AR-8", "AR-9"):
        if accepted not in accepted_slices:
            accepted_slices.append(accepted)
    payload["accepted_slices"] = accepted_slices
    payload["ar8_progress"] = ar8_progress(projection_fields=True)
    payload["ar8_acceptance"] = ar8_acceptance()
    payload.pop("ar9_current", None)
    payload["ar9_acceptance"] = ar9_acceptance()
    payload["ar10_current"] = ar10_projection()
    payload["current_delivery_map"] = delivery_map(payload["current_delivery_map"])
    policy = payload["architecture_inventory_policy"]
    policy["ar8_current_subject_projection"] = "architecture/inventory.json::subject_domain_authorities"
    policy["ar8_current_composition_root"] = "architecture/credential-authority.json"
    policy["ar8b_registry_role"] = "IMMUTABLE_ACCEPTED_PROVENANCE_DATASET"
    policy["ar9_d1_evolution_projection"] = AR9_PROJECTION
    policy["ar9_d1_evolution_source"] = AR9_AUTHORITY
    policy["ar9_acceptance_evidence"] = AR9_ACCEPTANCE_EVIDENCE
    policy["ar10_runtime_cutover_projection"] = AR10_PROJECTION
    policy["ar10_runtime_cutover_source"] = AR10_AUTHORITY
    app = payload.get("application_architecture", {})
    app["program_handoff_status"] = "AR9_ACCEPTED_AR10_CURRENT"
    app["program_next_required_subslice"] = "AR-10"
    return payload


def serialized(payload: object) -> str:
    return json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def validate(status: dict[str, object], transition: dict[str, object]) -> None:
    current = status["current"]
    program = current["architecture_program"]
    if program.get("current_slice") != "AR-10" or "AR-9" not in program.get("accepted_slices", []):
        raise ValueError("program must advance to AR-10 only after accepted AR-9")
    if program.get("ar8_progress") != ar8_progress(projection_fields=False):
        raise ValueError("docs/status.json accepted AR-8 progress drifted")
    if transition.get("ar8_progress") != ar8_progress(projection_fields=True):
        raise ValueError("architecture transition accepted AR-8 progress drifted")
    if program.get("ar9_acceptance") != ar9_acceptance() or transition.get("ar9_acceptance") != ar9_acceptance():
        raise ValueError("status and transition AR-9 acceptance projection drifted")
    if program.get("ar10_current") != ar10_projection() or transition.get("ar10_current") != ar10_projection():
        raise ValueError("status and transition AR-10 authority projection drifted")
    expected_delivery = current["current_delivery_map"]
    if expected_delivery != transition.get("current_delivery_map"):
        raise ValueError("status and transition CURRENT_DELIVERY_MAP projections must match exactly")
    if expected_delivery.get("next_gate", {}).get("issue") != AR10_ISSUE:
        raise ValueError("AR-10 acceptance gate must point to implementation issue #368")
    invariants = expected_delivery.get("invariants", {})
    required = {
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "ar10_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }
    for key, wanted in required.items():
        if invariants.get(key) != wanted:
            raise ValueError(f"CURRENT_DELIVERY_MAP invariant {key} drifted")
    if current.get("architecture_complete") is not False:
        raise ValueError("architecture must remain incomplete during AR-10")
    if current.get("production_core_gate") != "BLOCKED" or status.get("production_ready") is not False:
        raise ValueError("production gates must remain fail-closed during AR-10")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    expected_status = project_status()
    expected_transition = project_transition()
    validate(expected_status, expected_transition)
    if args.write:
        STATUS.write_text(serialized(expected_status), encoding="utf-8", newline="\n")
        TRANSITION.write_text(serialized(expected_transition), encoding="utf-8", newline="\n")
        print("Wrote accepted AR-9 status/transition and current AR-10 runtime-cutover projections.")
    elif args.check:
        actual_status = json.loads(STATUS.read_text(encoding="utf-8"))
        actual_transition = json.loads(TRANSITION.read_text(encoding="utf-8"))
        validate(actual_status, actual_transition)
        if actual_status != expected_status or actual_transition != expected_transition:
            raise SystemExit("accepted AR-9/current AR-10 status/transition projections are stale; run --write")
        print("AR-9 accepted-main status/transition and current AR-10 runtime-cutover projection are current while production remains blocked.")
    else:
        negative = copy.deepcopy(expected_status)
        negative["current"]["current_delivery_map"]["invariants"]["production_core_gate"] = "AUTHORIZED"
        try:
            validate(negative, expected_transition)
        except ValueError:
            print("Post-AR-9 premature production authorization fixture rejected as expected.")
        else:
            raise SystemExit("premature production authorization negative fixture unexpectedly passed")
        stale_issue = copy.deepcopy(expected_status)
        stale_issue["current"]["current_delivery_map"]["next_gate"]["issue"] = AR9_ISSUE
        try:
            validate(stale_issue, expected_transition)
        except ValueError:
            print("Stale AR-9 acceptance issue projection rejected as expected.")
        else:
            raise SystemExit("stale AR-9 acceptance issue negative fixture unexpectedly passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"AR-9 accepted/current AR-10 projection failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
