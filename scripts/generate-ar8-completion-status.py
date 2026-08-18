#!/usr/bin/env python3
"""Deterministically project accepted AR-8 and current AR-9 into canonical status authorities."""

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
ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-18-ar8-final-acceptance.json"
EXACT_GREEN_HEAD = "81d1f0c26ff0bd3a688c2d5dc000b93640479e47"
IMPLEMENTATION_MERGE = "874666f6ef6eb003425c9677d558378d6dc0daaf"
AR9_ISSUE = 366
AR9_AUTHORITY = "architecture/d1-evolution-ar9.json"
AR9_PROJECTION = "architecture/inventory.json::d1_evolution"


def progress(*, projection_fields: bool) -> dict[str, object]:
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
        "exact_green_head": EXACT_GREEN_HEAD,
        "implementation_merge": IMPLEMENTATION_MERGE,
        "applicable_permanent_workflows": "14/14",
        "accepted_main_reread": IMPLEMENTATION_MERGE,
        "acceptance_evidence": ACCEPTANCE_EVIDENCE,
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
    value["accepted_checkpoint"] = "AR-8"
    value["current_work"] = "AR-9"
    value["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-8",
        "current_subslice": "AR-9",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR8_CLOSEOUT",
    }
    value["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-8",
        "full_ar8_accepted": True,
        "acceptance_evidence": ACCEPTANCE_EVIDENCE,
    }
    value["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    value["next_gate"] = {
        "id": "AR-9_ACCEPTANCE",
        "issue": AR9_ISSUE,
        "on_success": "AR-10_BECOMES_CURRENT",
    }
    value["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": True,
        "ar9_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return value


def acceptance() -> dict[str, object]:
    return {
        "issue": 361,
        "umbrella_issue": 308,
        "implementation_pr": 362,
        "exact_green_head": EXACT_GREEN_HEAD,
        "implementation_merge": IMPLEMENTATION_MERGE,
        "applicable_permanent_workflows": "14/14",
        "behind_by": 0,
        "blocking_reviews": 0,
        "unresolved_review_threads": 0,
        "accepted_main_reread": IMPLEMENTATION_MERGE,
        "evidence": ACCEPTANCE_EVIDENCE,
        "metadata_only": True,
        "production_mutation": False,
    }


def ar9_projection() -> dict[str, object]:
    return {
        "tracking_issue": AR9_ISSUE,
        "source_authority": AR9_AUTHORITY,
        "canonical_projection": AR9_PROJECTION,
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
    if "AR-8" not in accepted_slices:
        accepted_slices.append("AR-8")
    program["accepted_slices"] = accepted_slices
    program["current_slice"] = "AR-9"
    program["next_slice_after_acceptance"] = "AR-10"
    program["ar8_progress"] = progress(projection_fields=False)
    program["ar8_acceptance"] = acceptance()
    program["ar9_current"] = ar9_projection()
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
        "name": "AR-9 — D1 Evolution / Schema Compatibility",
        "status": "current",
        "tracking_issue": AR9_ISSUE,
        "program_tracking_issue": 266,
        "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "slice_authority": AR9_AUTHORITY,
        "canonical_projection": AR9_PROJECTION,
        "previous_acceptance_evidence": ACCEPTANCE_EVIDENCE,
    }
    payload["production_ready"] = False
    payload["as_of"] = "2026-08-18"
    return payload


def project_transition() -> dict[str, object]:
    payload = json.loads(BASE_TRANSITION.read_text(encoding="utf-8"))
    payload["status"] = "ACTIVE_AFTER_AR8_ACCEPTANCE"
    payload["current_slice"] = "AR-9"
    payload["next_slice_after_acceptance"] = "AR-10"
    accepted_slices = list(payload.get("accepted_slices", []))
    if "AR-8" not in accepted_slices:
        accepted_slices.append("AR-8")
    payload["accepted_slices"] = accepted_slices
    payload["ar8_progress"] = progress(projection_fields=True)
    payload["ar8_acceptance"] = acceptance()
    payload["ar9_current"] = ar9_projection()
    payload["current_delivery_map"] = delivery_map(payload["current_delivery_map"])
    policy = payload["architecture_inventory_policy"]
    policy["ar8_current_subject_projection"] = "architecture/inventory.json::subject_domain_authorities"
    policy["ar8_current_composition_root"] = "architecture/credential-authority.json"
    policy["ar8b_registry_role"] = "IMMUTABLE_ACCEPTED_PROVENANCE_DATASET"
    policy["ar9_d1_evolution_projection"] = AR9_PROJECTION
    policy["ar9_d1_evolution_source"] = AR9_AUTHORITY
    app = payload.get("application_architecture", {})
    app["program_handoff_status"] = "AR8_ACCEPTED_AR9_CURRENT"
    app["program_next_required_subslice"] = "AR-9"
    return payload


def serialized(payload: object) -> str:
    return json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def validate(status: dict[str, object], transition: dict[str, object]) -> None:
    current = status["current"]
    program = current["architecture_program"]
    if program.get("current_slice") != "AR-9" or "AR-8" not in program.get("accepted_slices", []):
        raise ValueError("program must advance to AR-9 only after accepted AR-8")
    if program.get("ar8_progress") != progress(projection_fields=False):
        raise ValueError("docs/status.json accepted AR-8 progress drifted")
    if transition.get("ar8_progress") != progress(projection_fields=True):
        raise ValueError("architecture transition accepted AR-8 progress drifted")
    if program.get("ar9_current") != ar9_projection() or transition.get("ar9_current") != ar9_projection():
        raise ValueError("status and transition AR-9 authority projection drifted")
    expected_delivery = current["current_delivery_map"]
    if expected_delivery != transition.get("current_delivery_map"):
        raise ValueError("status and transition CURRENT_DELIVERY_MAP projections must match exactly")
    if expected_delivery.get("next_gate", {}).get("issue") != AR9_ISSUE:
        raise ValueError("AR-9 acceptance gate must point to implementation issue #366")
    invariants = expected_delivery.get("invariants", {})
    required = {
        "full_ar8_accepted": True,
        "ar9_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }
    for key, wanted in required.items():
        if invariants.get(key) != wanted:
            raise ValueError(f"CURRENT_DELIVERY_MAP invariant {key} drifted")
    if current.get("architecture_complete") is not False:
        raise ValueError("architecture must remain incomplete after AR-8")
    if current.get("production_core_gate") != "BLOCKED" or status.get("production_ready") is not False:
        raise ValueError("production gates must remain fail-closed after AR-8")


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
        print("Wrote accepted AR-8 status/transition and current AR-9 D1 projections.")
    elif args.check:
        actual_status = json.loads(STATUS.read_text(encoding="utf-8"))
        actual_transition = json.loads(TRANSITION.read_text(encoding="utf-8"))
        validate(actual_status, actual_transition)
        if actual_status != expected_status or actual_transition != expected_transition:
            raise SystemExit("accepted AR-8/current AR-9 status/transition projections are stale; run --write")
        print("AR-8 accepted-main status/transition and current AR-9 D1 projection are current while production remains blocked.")
    else:
        negative = copy.deepcopy(expected_status)
        negative["current"]["current_delivery_map"]["invariants"]["production_core_gate"] = "AUTHORIZED"
        try:
            validate(negative, expected_transition)
        except ValueError:
            print("Post-AR-8 premature production authorization fixture rejected as expected.")
        else:
            raise SystemExit("premature production authorization negative fixture unexpectedly passed")
        stale_issue = copy.deepcopy(expected_status)
        stale_issue["current"]["current_delivery_map"]["next_gate"]["issue"] = None
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
        print(f"AR-8 accepted/current AR-9 projection failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
