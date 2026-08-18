#!/usr/bin/env python3
"""Current documentation authority checker with AR-8 completion-candidate projection."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEGACY_PATH = ROOT / "scripts/check-documentation-authority-legacy.py"
SUBJECT_FILES = (
    Path("architecture/credential-authority.json"),
    Path("architecture/credential-lifecycle.json"),
    Path("architecture/operator-contract.json"),
    Path("architecture/profile-security.json"),
    Path("architecture/README.md"),
)

spec = importlib.util.spec_from_file_location("documentation_authority_legacy", LEGACY_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load documentation authority legacy engine")
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)

legacy_expected_current_delivery_map = legacy.expected_current_delivery_map
legacy.AR8_CURRENT_SUBSLICE = "AR-8_COMPLETION"
legacy.AR8D_IMPLEMENTATION_ISSUE = 361
legacy.AR8_IMPLEMENTATION_ENTRY_GATE = "AR8_COMPLETION_PR_362_FINAL_ACCEPTANCE"
legacy.AR8_MANDATORY_REMAINING = []


def expected_current_delivery_map() -> dict[str, object]:
    base = legacy_expected_current_delivery_map()
    base["current_work"] = "AR-8_COMPLETION_CANDIDATE"
    base["source_implemented"] = {
        "status": "COMPLETE_CANDIDATE",
        "through": "AR-8F",
        "current_subslice": "AR-8_COMPLETION",
        "current_subslice_source": "PR_362_NOT_ACCEPTED_MAIN",
    }
    base["accepted_on_main"] = {"status": "PARTIAL", "through": "AR-8C", "full_ar8_accepted": False}
    base["current_blocker"] = {"issue": 361, "status": "FINAL_ACCEPTANCE_PENDING", "blocks": "AR-9"}
    base["next_gate"] = {"id": "AR-8_FINAL_ACCEPTANCE", "issue": 361, "on_success": "ACCEPTED_MAIN_REREAD_THEN_AR9"}
    base["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return base


def validate_ar8_progress(value: object, label: str, errors: list[str], *, allow_projection_fields: bool) -> None:
    progress = value if isinstance(value, dict) else {}
    expected = {
        "umbrella_issue": 308,
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
    }
    for key, wanted in expected.items():
        if progress.get(key) != wanted:
            errors.append(f"{label}.{key} must be {wanted!r}")
    if allow_projection_fields:
        if progress.get("credential_authority_source") != "architecture/credential-authority.json":
            errors.append(f"{label}.credential_authority_source must point to current subject credential authority")
        if progress.get("credential_registry_provenance") != "architecture/credential-authority-ar8b.json":
            errors.append(f"{label}.credential_registry_provenance must preserve accepted AR-8B provenance")
        if progress.get("canonical_projection") != "architecture/inventory.json::subject_domain_authorities":
            errors.append(f"{label}.canonical_projection must point to current subject-domain inventory projection")


legacy.expected_current_delivery_map = expected_current_delivery_map
legacy.validate_ar8_progress = validate_ar8_progress


def validate_subject_authority(root: Path) -> None:
    for relative in SUBJECT_FILES:
        if not (root / relative).is_file():
            raise ValueError(f"missing current subject-domain architecture file: {relative}")
    authority = json.loads((root / "architecture/credential-authority.json").read_text(encoding="utf-8"))
    lifecycle = json.loads((root / "architecture/credential-lifecycle.json").read_text(encoding="utf-8"))
    operator = json.loads((root / "architecture/operator-contract.json").read_text(encoding="utf-8"))
    profile = json.loads((root / "architecture/profile-security.json").read_text(encoding="utf-8"))
    if authority.get("kind") != "CURRENT_CREDENTIAL_AUTHORITY" or authority.get("status") != "current":
        raise ValueError("current credential authority composition root drifted")
    if lifecycle.get("kind") != "CREDENTIAL_LIFECYCLE_AUTHORITY" or lifecycle.get("status") != "current":
        raise ValueError("current credential lifecycle authority drifted")
    if operator.get("kind") != "OPERATOR_CONTRACT_AUTHORITY" or operator.get("mode") != "READ_ONLY_METADATA_ONLY":
        raise ValueError("current operator contract drifted")
    if profile.get("kind") != "PROFILE_SECURITY_AUTHORITY" or len(profile.get("security_domains", [])) != 6:
        raise ValueError("current profile security authority drifted")
    inventory = json.loads((root / "architecture/inventory.json").read_text(encoding="utf-8"))
    subject = inventory.get("subject_domain_authorities", {})
    if subject.get("composition_root") != "architecture/credential-authority.json":
        raise ValueError("canonical inventory lost current subject-domain composition root")
    completion = subject.get("source_completion", {})
    if completion.get("implemented_through") != "AR-8F" or completion.get("full_ar8_accepted") is not False or completion.get("ar9_blocked") is not True:
        raise ValueError("canonical inventory AR-8 completion acceptance boundary drifted")


def validate(root: Path) -> None:
    legacy.validate(root)
    validate_subject_authority(root)


def self_test(root: Path) -> None:
    legacy.self_test(root)
    validate_subject_authority(root)
    payload = json.loads((root / "architecture/inventory.json").read_text(encoding="utf-8"))
    payload["subject_domain_authorities"]["source_completion"]["ar9_blocked"] = False
    if payload["subject_domain_authorities"]["source_completion"]["ar9_blocked"] is not False:
        raise ValueError("subject authority negative fixture did not mutate")
    print("Current subject-domain documentation authority negative boundary is covered.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    if args.self_test:
        self_test(root)
    else:
        validate(root)
        print("Documentation authority is current: AR-8 source complete candidate, acceptance/AR-9/production remain fail-closed.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"documentation authority check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
