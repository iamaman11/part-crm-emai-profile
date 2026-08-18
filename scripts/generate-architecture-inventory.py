#!/usr/bin/env python3
"""Generate canonical inventory with current subject-domain AR-8 completion projection."""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
ENGINE_PATH = ROOT / "scripts/_architecture_inventory_engine.py"
INVENTORY_PATH = ROOT / "architecture/inventory.json"
SUBJECTS = {
    "credential_authority": "architecture/credential-authority.json",
    "credential_lifecycle": "architecture/credential-lifecycle.json",
    "operator_contract": "architecture/operator-contract.json",
    "profile_security": "architecture/profile-security.json",
}

spec = importlib.util.spec_from_file_location("architecture_inventory_engine", ENGINE_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load architecture inventory engine")
engine = importlib.util.module_from_spec(spec)
spec.loader.exec_module(engine)


def load_json(relative: str) -> dict[str, Any]:
    path = ROOT / relative
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{relative} must contain one JSON object")
    return payload


def file_sha256(relative: str) -> str:
    return hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()


def subject_projection() -> dict[str, object]:
    authority = load_json(SUBJECTS["credential_authority"])
    lifecycle = load_json(SUBJECTS["credential_lifecycle"])
    operator = load_json(SUBJECTS["operator_contract"])
    profile = load_json(SUBJECTS["profile_security"])
    if authority.get("kind") != "CURRENT_CREDENTIAL_AUTHORITY" or authority.get("status") != "current":
        raise ValueError("current credential authority composition root is invalid")
    if lifecycle.get("kind") != "CREDENTIAL_LIFECYCLE_AUTHORITY" or lifecycle.get("status") != "current":
        raise ValueError("current credential lifecycle authority is invalid")
    if operator.get("kind") != "OPERATOR_CONTRACT_AUTHORITY" or operator.get("mode") != "READ_ONLY_METADATA_ONLY":
        raise ValueError("current operator authority is invalid")
    if profile.get("kind") != "PROFILE_SECURITY_AUTHORITY" or profile.get("status") != "current":
        raise ValueError("current profile security authority is invalid")
    domains = [entry.get("id") for entry in profile.get("security_domains", [])]
    if len(domains) != 6 or any(not isinstance(value, str) for value in domains):
        raise ValueError("profile security projection requires exactly six domains")
    return {
        "schema_version": 1,
        "role": "CURRENT_SUBJECT_DOMAIN_PROJECTION",
        "composition_root": SUBJECTS["credential_authority"],
        "registry_provenance": "architecture/credential-authority-ar8b.json",
        "sources": {
            name: {
                "path": path,
                "sha256": file_sha256(path),
            }
            for name, path in SUBJECTS.items()
        },
        "credential_lifecycle_concern_ids": [entry.get("id") for entry in lifecycle.get("concerns", [])],
        "profile_security_domain_ids": domains,
        "operator_mode": operator.get("mode"),
        "completion_provenance": {
            "lifecycle_candidate": "docs/evidence/ar8-completion-lifecycle-candidate.json",
            "operator_rehearsal_candidate": "docs/evidence/ar8-operator-rehearsal-candidate.json",
            "secret_transport_successor_candidate": "docs/evidence/ar8-d-secret-transport-successor-candidate.json",
        },
        "source_completion": {
            "tracking_issue": 361,
            "completion_pr": 362,
            "implemented_through": "AR-8F",
            "state": "FINAL_ACCEPTANCE_PENDING",
            "accepted_main_through": "AR-8C",
            "full_ar8_accepted": False,
            "ar9_blocked": True,
            "production_mutation": False,
        },
    }


def completion_delivery_map(base: dict[str, object]) -> dict[str, object]:
    delivery = copy.deepcopy(base)
    delivery["current_work"] = "AR-8_COMPLETION_CANDIDATE"
    delivery["source_implemented"] = {
        "status": "COMPLETE_CANDIDATE",
        "through": "AR-8F",
        "current_subslice": "AR-8_COMPLETION",
        "current_subslice_source": "PR_362_NOT_ACCEPTED_MAIN",
    }
    delivery["accepted_on_main"] = {
        "status": "PARTIAL",
        "through": "AR-8C",
        "full_ar8_accepted": False,
    }
    delivery["current_blocker"] = {
        "issue": 361,
        "status": "FINAL_ACCEPTANCE_PENDING",
        "blocks": "AR-9",
    }
    delivery["next_gate"] = {
        "id": "AR-8_FINAL_ACCEPTANCE",
        "issue": 361,
        "on_success": "ACCEPTED_MAIN_REREAD_THEN_AR9",
    }
    invariants = delivery.setdefault("invariants", {})
    invariants.update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return delivery


def completion_progress(base: dict[str, object]) -> dict[str, object]:
    progress = copy.deepcopy(base)
    progress.update({
        "accepted_subslices": ["AR-8A", "AR-8B", "AR-8C"],
        "current_subslice": "AR-8_COMPLETION",
        "current_implementation_issue": 361,
        "mandatory_remaining": [],
        "implementation_entry_gate": "AR8_COMPLETION_PR_362_FINAL_ACCEPTANCE",
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "production_mutation": False,
        "source_complete_candidate": True,
        "completion_pr": 362,
        "implemented_through": "AR-8F",
    })
    return progress


def build_inventory() -> dict[str, object]:
    expected = engine.build_inventory()
    expected["subject_domain_authorities"] = subject_projection()
    expected["current_delivery_map"] = completion_delivery_map(expected["current_delivery_map"])
    expected["program_state"]["ar8_progress"] = completion_progress(expected["program_state"]["ar8_progress"])
    expected["program_state"]["current_delivery_map"] = copy.deepcopy(expected["current_delivery_map"])
    documentation = expected.setdefault("documentation_authority", {})
    documentation["current_credential_authority"] = SUBJECTS["credential_authority"]
    documentation["credential_registry_provenance"] = "architecture/credential-authority-ar8b.json"
    documentation["credential_lifecycle"] = SUBJECTS["credential_lifecycle"]
    documentation["operator_contract"] = SUBJECTS["operator_contract"]
    documentation["profile_security"] = SUBJECTS["profile_security"]
    documentation["ar8_completion_tracking_issue"] = 361
    documentation["ar8_completion_pr"] = 362
    return expected


def serialized(payload: object) -> str:
    return json.dumps(payload, indent=2, sort_keys=False) + "\n"


def check_current(expected: dict[str, object]) -> None:
    actual = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
    if actual != expected:
        raise SystemExit("architecture/inventory.json is stale; run scripts/generate-architecture-inventory.py --write")


def run_subject_checks() -> None:
    for command in (
        ["node", ".github/scripts/architecture-authority-check.mjs"],
        ["node", ".github/scripts/profile-security-authority-check.mjs"],
    ):
        result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
        if result.returncode != 0:
            details = "\n".join(value.strip() for value in (result.stdout, result.stderr) if value.strip())
            raise SystemExit(details or f"subject authority validator failed: {' '.join(command)}")


def self_test(expected: dict[str, object]) -> None:
    drift = copy.deepcopy(expected)
    drift["subject_domain_authorities"]["source_completion"]["ar9_blocked"] = False
    if drift == expected:
        raise SystemExit("subject projection negative fixture did not mutate")
    premature = copy.deepcopy(expected)
    premature["current_delivery_map"]["invariants"]["production_ready"] = True
    if premature == expected:
        raise SystemExit("production readiness negative fixture did not mutate")
    payload, detected = engine.validate_credential_authority_source()
    engine.credential_negative_self_test(payload, detected)
    engine.ar3.negative_self_test(ROOT)
    run_subject_checks()
    print("Architecture inventory subject-domain / AR-8 completion fail-closed self-tests passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    mode.add_argument("--credential-self-test", action="store_true")
    args = parser.parse_args()
    if args.credential_self_test:
        payload, detected = engine.validate_credential_authority_source()
        engine.credential_negative_self_test(payload, detected)
        engine.print_credential_check_summary(detected, self_tested=True)
        return 0
    expected = build_inventory()
    if args.write:
        INVENTORY_PATH.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print("Wrote architecture/inventory.json with current subject-domain AR-8 completion projection.")
    elif args.check:
        check_current(expected)
        run_subject_checks()
        print("Architecture inventory projects current subject-domain authorities and AR-8 completion candidate while acceptance/production remain blocked.")
    else:
        self_test(expected)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
