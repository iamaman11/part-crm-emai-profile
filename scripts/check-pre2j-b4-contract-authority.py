#!/usr/bin/env python3
"""Enforce the separately accepted one-shot B4 additive-v1 contract authority."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = Path("architecture/pre2j-b4-contract-authority.json")
STATUS_PATH = Path("docs/status.json")
EXPECTED_ALLOWED_PATH = "openapi/v1/fragments/mailbox-client-association.json"
EXPECTED_RULES = {
    "authority_must_be_accepted_before_use": True,
    "authority_is_immutable_after_acceptance": True,
    "allowed_path_must_be_absent_in_base": True,
    "existing_v1_paths_are_immutable": True,
    "contracts_baseline_is_immutable": True,
    "proto_is_immutable": True,
    "new_major_version_required_for_breaking_change": True,
}


def load_json(path: Path) -> dict[str, object]:
    value = json.loads((ROOT / path).read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def authority_errors(authority: dict[str, object]) -> list[str]:
    expected = {
        "schema_version": 1,
        "status": "approved_pending_b4",
        "tracking_issue": 211,
        "parent_batch_issue": 209,
        "umbrella_blocker_issue": 203,
        "policy": "one_shot_additive_v1_fragment",
        "allowed_path": EXPECTED_ALLOWED_PATH,
    }
    errors = [
        f"{AUTHORITY_PATH}: {key} must be {wanted!r}"
        for key, wanted in expected.items()
        if authority.get(key) != wanted
    ]
    decision_base = authority.get("decision_base")
    if not isinstance(decision_base, str) or len(decision_base) != 40:
        errors.append(f"{AUTHORITY_PATH}: decision_base must be an exact commit SHA")
    if authority.get("rules") != EXPECTED_RULES:
        errors.append(f"{AUTHORITY_PATH}: rules must match the accepted one-shot policy exactly")
    return errors


def remediation_errors(status: dict[str, object]) -> list[str]:
    current = status.get("current")
    if not isinstance(current, dict):
        return ["docs/status.json: current authority is missing"]
    remediation = current.get("pre2j_product_readiness_remediation")
    if not isinstance(remediation, dict):
        return ["docs/status.json: pre2j product-readiness remediation is missing"]
    errors: list[str] = []
    if remediation.get("status") != "active_blocking":
        errors.append("B4 contract exception requires active_blocking pre-2J product remediation")
    if remediation.get("tracking_issue") != 203:
        errors.append("B4 contract exception requires umbrella blocker #203")
    if remediation.get("plan") != "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md":
        errors.append("B4 contract exception requires the canonical pre-2J product-readiness plan")
    phase_2j = current.get("phase_2j")
    if not isinstance(phase_2j, dict) or phase_2j.get("status") != "blocked_pending_repository_remediation":
        errors.append("Phase 2J must remain blocked while the B4 exception is active")
    return errors


def evaluate_openapi_changes(
    changes: list[tuple[str, str]],
    *,
    authority_accepted_in_base: bool,
    allowed_path_present_in_base: bool,
) -> list[str]:
    if not changes:
        return []
    if not authority_accepted_in_base:
        return ["B4 contract authority must be accepted on the base branch before any v1 exception is consumed"]
    if allowed_path_present_in_base:
        return ["the one-shot B4 v1 exception is already consumed; the accepted fragment is immutable"]
    if changes != [("A", EXPECTED_ALLOWED_PATH)]:
        rendered = ", ".join(f"{status}:{path}" for status, path in changes)
        return [
            "only one absent-to-added B4 fragment is authorized; observed v1 changes: " + rendered
        ]
    return []


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        check=check,
        text=True,
        capture_output=True,
    )


def base_has(base_ref: str, path: Path | str) -> bool:
    result = git("cat-file", "-e", f"{base_ref}:{path}", check=False)
    return result.returncode == 0


def base_text(base_ref: str, path: Path) -> str:
    return git("show", f"{base_ref}:{path}").stdout


def openapi_changes(base_ref: str) -> list[tuple[str, str]]:
    output = git("diff", "--name-status", base_ref, "--", "openapi/v1").stdout
    changes: list[tuple[str, str]] = []
    for raw in output.splitlines():
        fields = raw.split("\t")
        if len(fields) != 2:
            changes.append((fields[0] if fields else "?", "<rename-or-invalid>"))
            continue
        changes.append((fields[0], fields[1]))
    return changes


def self_test() -> None:
    assert not evaluate_openapi_changes(
        [], authority_accepted_in_base=False, allowed_path_present_in_base=False
    )
    assert evaluate_openapi_changes(
        [("A", EXPECTED_ALLOWED_PATH)],
        authority_accepted_in_base=False,
        allowed_path_present_in_base=False,
    )
    assert not evaluate_openapi_changes(
        [("A", EXPECTED_ALLOWED_PATH)],
        authority_accepted_in_base=True,
        allowed_path_present_in_base=False,
    )
    assert evaluate_openapi_changes(
        [("M", "openapi/v1/openapi.json")],
        authority_accepted_in_base=True,
        allowed_path_present_in_base=False,
    )
    assert evaluate_openapi_changes(
        [("M", EXPECTED_ALLOWED_PATH)],
        authority_accepted_in_base=True,
        allowed_path_present_in_base=True,
    )
    assert evaluate_openapi_changes(
        [("A", EXPECTED_ALLOWED_PATH), ("A", "openapi/v1/fragments/extra.json")],
        authority_accepted_in_base=True,
        allowed_path_present_in_base=False,
    )
    valid = {
        "schema_version": 1,
        "status": "approved_pending_b4",
        "decision_base": "1" * 40,
        "tracking_issue": 211,
        "parent_batch_issue": 209,
        "umbrella_blocker_issue": 203,
        "policy": "one_shot_additive_v1_fragment",
        "allowed_path": EXPECTED_ALLOWED_PATH,
        "rules": EXPECTED_RULES,
    }
    assert not authority_errors(valid)
    tampered = dict(valid)
    tampered["allowed_path"] = "openapi/v1/fragments/anything.json"
    assert authority_errors(tampered)
    print("B4 one-shot contract authority negative policy self-test passed.")


def check_repository(base_ref: str) -> None:
    authority = load_json(AUTHORITY_PATH)
    errors = authority_errors(authority)
    errors.extend(remediation_errors(load_json(STATUS_PATH)))

    accepted_in_base = base_has(base_ref, AUTHORITY_PATH)
    if accepted_in_base:
        current_text = (ROOT / AUTHORITY_PATH).read_text(encoding="utf-8")
        if base_text(base_ref, AUTHORITY_PATH) != current_text:
            errors.append("accepted B4 contract authority is immutable after acceptance")

    allowed_in_base = base_has(base_ref, EXPECTED_ALLOWED_PATH)
    changes = openapi_changes(base_ref)
    errors.extend(
        evaluate_openapi_changes(
            changes,
            authority_accepted_in_base=accepted_in_base,
            allowed_path_present_in_base=allowed_in_base,
        )
    )

    if errors:
        raise SystemExit("\n".join(errors))

    if changes:
        print(
            "Accepted B4 authority permits exactly one additive v1 fragment: "
            + EXPECTED_ALLOWED_PATH
        )
    elif accepted_in_base:
        print("Accepted B4 one-shot contract authority is immutable; v1 contract is unchanged.")
    else:
        print("B4 one-shot contract authority candidate is valid and has not been consumed in this PR.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-ref")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not args.base_ref:
        parser.error("--base-ref is required unless --self-test is used")
    check_repository(args.base_ref)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
