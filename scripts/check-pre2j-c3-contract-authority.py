#!/usr/bin/env python3
"""Enforce the separately accepted one-shot C3 IMAP/SMTP additive-v1 contract authority."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = Path("architecture/pre2j-c3-contract-authority.json")
STATUS_PATH = Path("docs/status.json")
EXPECTED_DECISION_BASE = "97453acc0cdec17ee919f9305f3e1bc252674e32"
EXPECTED_ALLOWED_PATH = "openapi/v1/fragments/mailbox-imap-smtp-onboarding.json"
EXPECTED_CAPABILITY_POLICY = {
    "mailbox_provider": "IMAP",
    "imap_read_search": True,
    "smtp_send_readiness": True,
    "smtp_send_execution_in_c3": False,
    "microsoft_graph_forbidden": True,
    "password_auth_outlook_claim_forbidden": True,
}
EXPECTED_AUTHENTICATION_POLICY = {
    "password": "standards_servers_only",
    "microsoft_oauth2": "entra_xoauth2_imap_smtp",
    "microsoft_oauth2_scopes": [
        "https://outlook.office.com/IMAP.AccessAsUser.All",
        "https://outlook.office.com/SMTP.Send",
        "offline_access",
    ],
}
EXPECTED_TRANSPORT_POLICY = {
    "plaintext_forbidden": True,
    "supported_modes": ["IMPLICIT_TLS", "STARTTLS"],
    "targets_must_be_bounded_and_ssrf_safe": True,
}
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
        "status": "approved_pending_c3",
        "decision_base": EXPECTED_DECISION_BASE,
        "tracking_issue": 221,
        "parent_batch_issue": 214,
        "umbrella_blocker_issue": 203,
        "policy": "one_shot_additive_v1_fragment",
        "allowed_path": EXPECTED_ALLOWED_PATH,
        "credential_boundary": "MAILBOX_SECRET_RESOLVER",
    }
    errors = [
        f"{AUTHORITY_PATH}: {key} must be {wanted!r}"
        for key, wanted in expected.items()
        if authority.get(key) != wanted
    ]
    if authority.get("capability_policy") != EXPECTED_CAPABILITY_POLICY:
        errors.append(
            f"{AUTHORITY_PATH}: capability_policy must preserve C3 read/send separation and forbid Graph/password Outlook overclaim"
        )
    if authority.get("authentication_policy") != EXPECTED_AUTHENTICATION_POLICY:
        errors.append(
            f"{AUTHORITY_PATH}: authentication_policy must match the accepted password/Microsoft OAuth2 standards-protocol policy exactly"
        )
    if authority.get("transport_policy") != EXPECTED_TRANSPORT_POLICY:
        errors.append(
            f"{AUTHORITY_PATH}: transport_policy must require bounded encrypted IMAP/SMTP transport exactly"
        )
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
        errors.append("C3 contract exception requires active_blocking pre-2J product remediation")
    if remediation.get("tracking_issue") != 203:
        errors.append("C3 contract exception requires umbrella blocker #203")
    if remediation.get("plan") != "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md":
        errors.append("C3 contract exception requires the canonical pre-2J product-readiness plan")
    phase_2j = current.get("phase_2j")
    if (
        not isinstance(phase_2j, dict)
        or phase_2j.get("status") != "blocked_pending_repository_remediation"
    ):
        errors.append("Phase 2J must remain blocked while the C3 exception is active")
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false while the C3 exception is active")
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
        return [
            "C3 contract authority must be accepted on the base branch before any v1 exception is consumed"
        ]
    if allowed_path_present_in_base:
        return ["the one-shot C3 v1 exception is already consumed; the accepted fragment is immutable"]
    if changes != [("A", EXPECTED_ALLOWED_PATH)]:
        rendered = ", ".join(f"{status}:{path}" for status, path in changes)
        return ["only one absent-to-added C3 fragment is authorized; observed v1 changes: " + rendered]
    return []


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=ROOT, check=check, text=True, capture_output=True)


def base_has(base_ref: str, path: Path | str) -> bool:
    return git("cat-file", "-e", f"{base_ref}:{path}", check=False).returncode == 0


def base_text(base_ref: str, path: Path | str) -> str:
    return git("show", f"{base_ref}:{path}").stdout


def openapi_changes(base_ref: str) -> list[tuple[str, str]]:
    output = git("diff", "--name-status", base_ref, "--", "openapi/v1").stdout
    changes: list[tuple[str, str]] = []
    for raw in output.splitlines():
        fields = raw.split("\t")
        if len(fields) != 2:
            changes.append((fields[0] if fields else "?", "<rename-or-invalid>"))
        else:
            changes.append((fields[0], fields[1]))
    return changes


def accepted_authority_errors(base_ref: str) -> list[str]:
    errors = authority_errors(load_json(AUTHORITY_PATH))
    errors.extend(remediation_errors(load_json(STATUS_PATH)))
    accepted_in_base = base_has(base_ref, AUTHORITY_PATH)
    if accepted_in_base:
        current_text = (ROOT / AUTHORITY_PATH).read_text(encoding="utf-8")
        if base_text(base_ref, AUTHORITY_PATH) != current_text:
            errors.append("accepted C3 contract authority is immutable after acceptance")
    fragment_path = Path(EXPECTED_ALLOWED_PATH)
    fragment_in_base = base_has(base_ref, EXPECTED_ALLOWED_PATH)
    if fragment_in_base:
        if not (ROOT / fragment_path).is_file():
            errors.append("accepted C3 contract fragment cannot be removed")
        elif base_text(base_ref, EXPECTED_ALLOWED_PATH) != (ROOT / fragment_path).read_text(
            encoding="utf-8"
        ):
            errors.append("accepted C3 contract fragment is immutable after consumption")
    return errors


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
        [("M", "openapi/v1/fragments/mailbox-gmail-oauth.json")],
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
        "status": "approved_pending_c3",
        "decision_base": EXPECTED_DECISION_BASE,
        "tracking_issue": 221,
        "parent_batch_issue": 214,
        "umbrella_blocker_issue": 203,
        "policy": "one_shot_additive_v1_fragment",
        "allowed_path": EXPECTED_ALLOWED_PATH,
        "credential_boundary": "MAILBOX_SECRET_RESOLVER",
        "capability_policy": EXPECTED_CAPABILITY_POLICY,
        "authentication_policy": EXPECTED_AUTHENTICATION_POLICY,
        "transport_policy": EXPECTED_TRANSPORT_POLICY,
        "rules": EXPECTED_RULES,
    }
    assert not authority_errors(valid)
    tampered = dict(valid)
    tampered["allowed_path"] = "openapi/v1/fragments/anything.json"
    assert authority_errors(tampered)
    tampered_capability = dict(valid)
    tampered_capability["capability_policy"] = {
        **EXPECTED_CAPABILITY_POLICY,
        "microsoft_graph_forbidden": False,
    }
    assert authority_errors(tampered_capability)
    tampered_auth = dict(valid)
    tampered_auth["authentication_policy"] = {
        **EXPECTED_AUTHENTICATION_POLICY,
        "microsoft_oauth2_scopes": ["https://graph.microsoft.com/Mail.ReadWrite"],
    }
    assert authority_errors(tampered_auth)
    tampered_transport = dict(valid)
    tampered_transport["transport_policy"] = {
        **EXPECTED_TRANSPORT_POLICY,
        "plaintext_forbidden": False,
    }
    assert authority_errors(tampered_transport)
    print("C3 one-shot IMAP/SMTP contract authority negative policy self-test passed.")


def check_authority_only(base_ref: str) -> None:
    errors = accepted_authority_errors(base_ref)
    if errors:
        raise SystemExit("\n".join(errors))
    print("Accepted C3 contract authority and consumed fragment are immutable.")


def check_repository(base_ref: str) -> None:
    errors = accepted_authority_errors(base_ref)
    accepted_in_base = base_has(base_ref, AUTHORITY_PATH)
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
        print("Accepted C3 authority permits exactly one additive v1 fragment: " + EXPECTED_ALLOWED_PATH)
    elif accepted_in_base:
        print("Accepted C3 one-shot contract authority is immutable; v1 contract is unchanged.")
    else:
        print("C3 one-shot contract authority candidate is valid and has not been consumed in this PR.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-ref")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--authority-only",
        action="store_true",
        help=(
            "verify the accepted C3 authority/consumed fragment without claiming ownership of "
            "unrelated future governed v1 exceptions"
        ),
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not args.base_ref:
        parser.error("--base-ref is required unless --self-test is used")
    if args.authority_only:
        check_authority_only(args.base_ref)
    else:
        check_repository(args.base_ref)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
