#!/usr/bin/env python3
"""Deterministically project pre-acceptance AR-8 completion into status and transition authorities."""

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


def progress(*, projection_fields: bool) -> dict[str, object]:
    value: dict[str, object] = {
        "umbrella_issue": 308,
        "accepted_subslices": ["AR-8A", "AR-8B", "AR-8C"],
        "current_subslice": "AR-8_COMPLETION",
        "current_implementation_issue": 361,
        "mandatory_remaining": [],
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "production_mutation": False,
        "implementation_entry_gate": "AR8_COMPLETION_PR_362_FINAL_ACCEPTANCE",
        "source_complete_candidate": True,
        "completion_pr": 362,
        "implemented_through": "AR-8F",
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
    value["current_work"] = "AR-8_COMPLETION_CANDIDATE"
    value["source_implemented"] = {
        "status": "COMPLETE_CANDIDATE",
        "through": "AR-8F",
        "current_subslice": "AR-8_COMPLETION",
        "current_subslice_source": "PR_362_NOT_ACCEPTED_MAIN",
    }
    value["accepted_on_main"] = {"status": "PARTIAL", "through": "AR-8C", "full_ar8_accepted": False}
    value["current_blocker"] = {"issue": 361, "status": "FINAL_ACCEPTANCE_PENDING", "blocks": "AR-9"}
    value["next_gate"] = {"id": "AR-8_FINAL_ACCEPTANCE", "issue": 361, "on_success": "ACCEPTED_MAIN_REREAD_THEN_AR9"}
    value["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return value


def project_status() -> dict[str, object]:
    payload = json.loads(BASE_STATUS.read_text(encoding="utf-8"))
    current = payload["current"]
    current["architecture_program"]["ar8_progress"] = progress(projection_fields=False)
    current["architecture_program"]["subject_domain_authorities"] = {
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
    step = current.get("next_repository_step", {})
    step.update({
        "name": "AR-8 completion closeout",
        "status": "final_acceptance_pending",
        "tracking_issue": 266,
        "umbrella_issue": 308,
        "implementation_issue": 361,
        "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
    })
    current["next_repository_step"] = step
    payload["as_of"] = "2026-08-18"
    return payload


def project_transition() -> dict[str, object]:
    payload = json.loads(BASE_TRANSITION.read_text(encoding="utf-8"))
    payload["status"] = "ACTIVE_DURING_AR8_COMPLETION_CANDIDATE"
    payload["ar8_progress"] = progress(projection_fields=True)
    payload["current_delivery_map"] = delivery_map(payload["current_delivery_map"])
    policy = payload["architecture_inventory_policy"]
    policy["ar8_current_subject_projection"] = "architecture/inventory.json::subject_domain_authorities"
    policy["ar8_current_composition_root"] = "architecture/credential-authority.json"
    policy["ar8b_registry_role"] = "IMMUTABLE_ACCEPTED_PROVENANCE_DATASET"
    app = payload.get("application_architecture", {})
    app["program_handoff_status"] = "AR8_COMPLETION_CANDIDATE"
    app["program_next_required_subslice"] = "AR-8_FINAL_ACCEPTANCE"
    return payload


def serialized(payload: object) -> str:
    return json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def validate(status: dict[str, object], transition: dict[str, object]) -> None:
    current = status["current"]
    candidate = current["architecture_program"]["ar8_progress"]
    if candidate != progress(projection_fields=False):
        raise ValueError("docs/status.json AR-8 completion progress drifted")
    if transition.get("ar8_progress") != progress(projection_fields=True):
        raise ValueError("architecture transition AR-8 completion progress drifted")
    expected_delivery = current["current_delivery_map"]
    if expected_delivery != transition.get("current_delivery_map"):
        raise ValueError("status and transition CURRENT_DELIVERY_MAP projections must match exactly")
    invariants = expected_delivery.get("invariants", {})
    required = {
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }
    for key, wanted in required.items():
        if invariants.get(key) != wanted:
            raise ValueError(f"CURRENT_DELIVERY_MAP invariant {key} drifted")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED" or status.get("production_ready") is not False:
        raise ValueError("accepted-main/production gates must remain fail-closed")


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
        print("Wrote AR-8 completion candidate status and transition projections.")
    elif args.check:
        actual_status = json.loads(STATUS.read_text(encoding="utf-8"))
        actual_transition = json.loads(TRANSITION.read_text(encoding="utf-8"))
        validate(actual_status, actual_transition)
        if actual_status != expected_status or actual_transition != expected_transition:
            raise SystemExit("AR-8 completion status/transition projections are stale; run --write")
        print("AR-8 source-complete status/transition are current while accepted-main, AR-9 and production remain blocked.")
    else:
        negative = copy.deepcopy(expected_status)
        negative["current"]["current_delivery_map"]["invariants"]["ar9_blocked"] = False
        try:
            validate(negative, expected_transition)
        except ValueError:
            print("AR-8 completion status/transition negative fixture rejected as expected.")
        else:
            raise SystemExit("premature AR-9 unblock negative fixture unexpectedly passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"AR-8 completion projection failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
