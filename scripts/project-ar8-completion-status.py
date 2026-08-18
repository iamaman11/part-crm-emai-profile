#!/usr/bin/env python3
"""Deterministically project the pre-acceptance AR-8 completion state into docs/status.json."""

from __future__ import annotations

import argparse
import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "history/status-ar8c-before-ar8-completion.json"
STATUS = ROOT / "docs/status.json"


def project() -> dict[str, object]:
    payload = json.loads(BASE.read_text(encoding="utf-8"))
    current = payload["current"]
    program = current["architecture_program"]
    progress = program["ar8_progress"]
    progress.update({
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
    })
    program["subject_domain_authorities"] = {
        "credential_authority": "architecture/credential-authority.json",
        "credential_registry_provenance": "architecture/credential-authority-ar8b.json",
        "credential_lifecycle": "architecture/credential-lifecycle.json",
        "operator_contract": "architecture/operator-contract.json",
        "profile_security": "architecture/profile-security.json",
        "architecture_map": "architecture/README.md",
    }
    delivery = copy.deepcopy(current["current_delivery_map"])
    delivery["current_work"] = "AR-8_COMPLETION_CANDIDATE"
    delivery["source_implemented"] = {
        "status": "COMPLETE_CANDIDATE",
        "through": "AR-8F",
        "current_subslice": "AR-8_COMPLETION",
        "current_subslice_source": "PR_362_NOT_ACCEPTED_MAIN",
    }
    delivery["accepted_on_main"] = {"status": "PARTIAL", "through": "AR-8C", "full_ar8_accepted": False}
    delivery["current_blocker"] = {"issue": 361, "status": "FINAL_ACCEPTANCE_PENDING", "blocks": "AR-9"}
    delivery["next_gate"] = {"id": "AR-8_FINAL_ACCEPTANCE", "issue": 361, "on_success": "ACCEPTED_MAIN_REREAD_THEN_AR9"}
    delivery["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    current["current_delivery_map"] = delivery
    current["architecture_complete"] = False
    current["production_core_gate"] = "BLOCKED"
    next_step = current.get("next_repository_step", {})
    next_step.update({
        "name": "AR-8 completion closeout",
        "status": "final_acceptance_pending",
        "tracking_issue": 266,
        "umbrella_issue": 308,
        "implementation_issue": 361,
        "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
    })
    current["next_repository_step"] = next_step
    payload["as_of"] = "2026-08-18"
    return payload


def serialize(payload: object) -> str:
    return json.dumps(payload, indent=2, sort_keys=False) + "\n"


def validate(payload: dict[str, object]) -> None:
    current = payload["current"]
    progress = current["architecture_program"]["ar8_progress"]
    delivery = current["current_delivery_map"]
    if progress.get("current_subslice") != "AR-8_COMPLETION" or progress.get("source_complete_candidate") is not True:
        raise ValueError("AR-8 completion source projection is missing")
    if progress.get("full_ar8_accepted") is not False or progress.get("ar9_blocked") is not True:
        raise ValueError("AR-8 must remain unaccepted and AR-9 blocked before accepted-main reread")
    if delivery.get("source_implemented", {}).get("through") != "AR-8F":
        raise ValueError("delivery map must project source implementation through AR-8F")
    invariants = delivery.get("invariants", {})
    for key, wanted in {
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if invariants.get(key) != wanted:
            raise ValueError(f"CURRENT_DELIVERY_MAP invariant {key} drifted")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        raise ValueError("repository closeout/production gates must remain blocked")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    expected = project()
    validate(expected)
    if args.write:
        STATUS.write_text(serialize(expected), encoding="utf-8", newline="\n")
        print("Wrote docs/status.json AR-8 completion candidate projection.")
    elif args.check:
        actual = json.loads(STATUS.read_text(encoding="utf-8"))
        validate(actual)
        if actual != expected:
            raise SystemExit("docs/status.json is stale; run scripts/project-ar8-completion-status.py --write")
        print("docs/status.json projects AR-8 source completion while accepted-main/AR-9/production remain blocked.")
    else:
        negative = copy.deepcopy(expected)
        negative["current"]["current_delivery_map"]["invariants"]["ar9_blocked"] = False
        try:
            validate(negative)
        except ValueError:
            print("AR-8 completion status negative fixture rejected as expected.")
        else:
            raise SystemExit("premature AR-9 unblock negative fixture unexpectedly passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"AR-8 completion status projection failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
