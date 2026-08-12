#!/usr/bin/env python3
"""Enforce the accepted one-shot C3G Graph provider migration plus schema-key erratum."""

from __future__ import annotations

import argparse
import copy
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = Path("architecture/pre2j-c3g-contract-authority.json")
ERRATUM_PATH = Path("architecture/pre2j-c3g-contract-authority-erratum.json")
ERRATUM_DOC_PATH = Path("docs/PRE2J_C3G_CONTRACT_MIGRATION_ERRATUM.md")
CHECKER_PATH = Path("scripts/check-pre2j-c3g-contract-authority.py")
STATUS_PATH = Path("docs/status.json")
MAILBOX_FRAGMENT = Path("openapi/v1/fragments/mailboxes.json")
GRAPH_FRAGMENT = Path("openapi/v1/fragments/mailbox-microsoft-graph-onboarding.json")
EXPECTED_DECISION_BASE = "7392581fb4ea0eb40bf2317c34b8fb7f151ca669"
EXPECTED_ERRATUM_BASE = "cf07db195db9a85e5e3ae3a0346528e3eacd39bb"
EXPECTED_ACCEPTED_PROVIDERS = ["GMAIL_API", "IMAP", "BROWSER_FALLBACK"]
EXPECTED_GRAPH_PROVIDER = "MICROSOFT_GRAPH"
AUTHORITY_SCHEMA_NAME = "MailboxProviderDto"
ACTUAL_FRAGMENT_SCHEMA_NAME = "MailboxProvider"
EXPECTED_MIGRATION = {
    "path": str(MAILBOX_FRAGMENT),
    "schema": AUTHORITY_SCHEMA_NAME,
    "accepted_values": EXPECTED_ACCEPTED_PROVIDERS,
    "single_added_value": EXPECTED_GRAPH_PROVIDER,
    "compatibility_class": "response_enum_widening_governed_preproduction_migration",
}
EXPECTED_ERRATUM = {
    "schema_version": 1,
    "status": "approved_pending_c3g",
    "decision_base": EXPECTED_ERRATUM_BASE,
    "tracking_issue": 228,
    "accepted_authority_issue": 226,
    "accepted_authority_pr": 227,
    "implementation_issue": 225,
    "correction": {
        "path": str(MAILBOX_FRAGMENT),
        "authority_schema": AUTHORITY_SCHEMA_NAME,
        "actual_fragment_schema": ACTUAL_FRAGMENT_SCHEMA_NAME,
        "single_added_value": EXPECTED_GRAPH_PROVIDER,
        "correction_class": "schema_key_erratum_only",
    },
    "rules": {
        "accepted_authority_record_is_immutable": True,
        "substantive_provider_migration_decision_is_unchanged": True,
        "openapi_v1_change_in_erratum_pr_is_forbidden": True,
        "d1_change_in_erratum_pr_is_forbidden": True,
        "runtime_graph_change_in_erratum_pr_is_forbidden": True,
        "erratum_must_be_accepted_before_migration_use": True,
        "erratum_is_immutable_after_acceptance": True,
        "production_ready_must_remain_false": True,
        "phase_2j_must_remain_blocked": True,
    },
}
EXPECTED_SCOPE_POLICY = {
    "provider": EXPECTED_GRAPH_PROVIDER,
    "graph_read": True,
    "graph_send_in_c3g": False,
    "delegated_permissions": ["https://graph.microsoft.com/Mail.Read", "offline_access"],
    "c3g_must_not_pregrant_mail_send": True,
    "c3_imap_smtp_remains_separate": True,
}
EXPECTED_RULES = {
    "authority_must_be_accepted_before_use": True,
    "authority_is_immutable_after_acceptance": True,
    "migration_is_one_shot": True,
    "historical_baseline_is_immutable": True,
    "proto_is_immutable": True,
    "existing_provider_meanings_are_immutable": True,
    "all_other_v1_artifacts_are_immutable": True,
    "production_ready_must_remain_false": True,
    "phase_2j_must_remain_blocked": True,
}


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=ROOT, check=check, text=True, capture_output=True)


def load_json(path: Path) -> dict[str, object]:
    value = json.loads((ROOT / path).read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def base_has(base_ref: str, path: Path | str) -> bool:
    return git("cat-file", "-e", f"{base_ref}:{path}", check=False).returncode == 0


def base_text(base_ref: str, path: Path | str) -> str:
    return git("show", f"{base_ref}:{path}").stdout


def base_json(base_ref: str, path: Path | str) -> dict[str, object]:
    value = json.loads(base_text(base_ref, path))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: base root must be an object")
    return value


def authority_errors(authority: dict[str, object]) -> list[str]:
    expected = {
        "schema_version": 1,
        "status": "approved_pending_c3g",
        "decision_base": EXPECTED_DECISION_BASE,
        "tracking_issue": 226,
        "implementation_issue": 225,
        "parent_batch_issue": 214,
        "umbrella_blocker_issue": 203,
        "policy": "governed_preproduction_provider_enum_migration_plus_one_fragment",
        "credential_boundary": "MAILBOX_SECRET_RESOLVER",
        "allowed_new_fragment": str(GRAPH_FRAGMENT),
    }
    errors = [f"{AUTHORITY_PATH}: {key} must be {wanted!r}" for key, wanted in expected.items() if authority.get(key) != wanted]
    if authority.get("existing_fragment_migration") != EXPECTED_MIGRATION:
        errors.append(f"{AUTHORITY_PATH}: existing_fragment_migration must match the accepted provider migration record exactly")
    if authority.get("scope_policy") != EXPECTED_SCOPE_POLICY:
        errors.append(f"{AUTHORITY_PATH}: scope_policy must preserve Graph read-only C3G and separate C3 IMAP/SMTP semantics")
    if authority.get("rules") != EXPECTED_RULES:
        errors.append(f"{AUTHORITY_PATH}: rules must match the accepted one-shot migration policy exactly")
    return errors


def erratum_errors(erratum: dict[str, object]) -> list[str]:
    return [] if erratum == EXPECTED_ERRATUM else [f"{ERRATUM_PATH}: must match the narrowly accepted C3G schema-key erratum exactly"]


def remediation_errors(status: dict[str, object]) -> list[str]:
    current = status.get("current")
    if not isinstance(current, dict):
        return ["docs/status.json: current authority is missing"]
    remediation = current.get("pre2j_product_readiness_remediation")
    if not isinstance(remediation, dict):
        return ["docs/status.json: pre2j product-readiness remediation is missing"]
    errors: list[str] = []
    if remediation.get("status") != "active_blocking":
        errors.append("C3G migration requires active_blocking pre-2J product remediation")
    if remediation.get("tracking_issue") != 203:
        errors.append("C3G migration requires umbrella blocker #203")
    if remediation.get("plan") != "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md":
        errors.append("C3G migration requires the canonical pre-2J product-readiness plan")
    phase_2j = current.get("phase_2j")
    if not isinstance(phase_2j, dict) or phase_2j.get("status") != "blocked_pending_repository_remediation":
        errors.append("Phase 2J must remain blocked while the C3G migration is active")
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false while the C3G migration is active")
    return errors


def openapi_changes(base_ref: str) -> list[tuple[str, str]]:
    output = git("diff", "--name-status", base_ref, "--", "openapi/v1").stdout
    changes: list[tuple[str, str]] = []
    for raw in output.splitlines():
        fields = raw.split("\t")
        changes.append((fields[0], fields[1]) if len(fields) == 2 else (fields[0] if fields else "?", "<rename-or-invalid>"))
    return sorted(changes)


def expected_openapi_changes() -> list[tuple[str, str]]:
    return sorted([("M", str(MAILBOX_FRAGMENT)), ("A", str(GRAPH_FRAGMENT))])


def evaluate_openapi_changes(changes: list[tuple[str, str]], *, authority_accepted_in_base: bool, erratum_accepted_in_base: bool, graph_fragment_present_in_base: bool) -> list[str]:
    if not changes:
        return []
    if not authority_accepted_in_base:
        return ["C3G contract authority must be accepted on the base branch before the migration is consumed"]
    if not erratum_accepted_in_base:
        return ["C3G schema-key erratum must be accepted on the base branch before the migration is consumed"]
    if graph_fragment_present_in_base:
        return ["the one-shot C3G migration is already consumed; accepted C3G artifacts are immutable"]
    if sorted(changes) != expected_openapi_changes():
        rendered = ", ".join(f"{status}:{path}" for status, path in sorted(changes))
        return ["C3G authorizes only MailboxProvider +MICROSOFT_GRAPH plus one new Graph onboarding fragment; observed v1 changes: " + rendered]
    return []


def migrated_mailboxes_errors(base_document: dict[str, object], current_document: dict[str, object]) -> list[str]:
    expected = copy.deepcopy(base_document)
    try:
        schema = expected["components"]["schemas"][ACTUAL_FRAGMENT_SCHEMA_NAME]
        current_schema = current_document["components"]["schemas"][ACTUAL_FRAGMENT_SCHEMA_NAME]
    except (KeyError, TypeError):
        return [f"mailboxes fragment must contain components.schemas.{ACTUAL_FRAGMENT_SCHEMA_NAME}"]
    if not isinstance(schema, dict) or not isinstance(current_schema, dict):
        return [f"{ACTUAL_FRAGMENT_SCHEMA_NAME} schema must remain an object"]
    if schema.get("enum") != EXPECTED_ACCEPTED_PROVIDERS:
        return [f"accepted base {ACTUAL_FRAGMENT_SCHEMA_NAME} enum does not match the governed migration source values"]
    schema["enum"] = [*EXPECTED_ACCEPTED_PROVIDERS, EXPECTED_GRAPH_PROVIDER]
    if current_document != expected:
        return [f"mailboxes fragment may change only by appending MICROSOFT_GRAPH to {ACTUAL_FRAGMENT_SCHEMA_NAME}.enum"]
    return []


def erratum_pr_scope_errors(base_ref: str) -> list[str]:
    if base_has(base_ref, ERRATUM_PATH):
        return []
    output = git("diff", "--name-status", base_ref).stdout
    observed: list[tuple[str, str]] = []
    for raw in output.splitlines():
        fields = raw.split("\t")
        if len(fields) != 2:
            return ["C3G erratum PR contains a rename or unsupported diff shape"]
        observed.append((fields[0], fields[1]))
    expected = sorted([("A", str(ERRATUM_PATH)), ("A", str(ERRATUM_DOC_PATH)), ("M", str(CHECKER_PATH))])
    if sorted(observed) != expected:
        rendered = ", ".join(f"{status}:{path}" for status, path in sorted(observed))
        return ["pre-acceptance C3G erratum PR may change only the erratum JSON, erratum doc, and C3G checker; observed: " + rendered]
    return []


def accepted_authority_errors(base_ref: str) -> list[str]:
    errors = authority_errors(load_json(AUTHORITY_PATH))
    errors.extend(erratum_errors(load_json(ERRATUM_PATH)))
    errors.extend(remediation_errors(load_json(STATUS_PATH)))
    errors.extend(erratum_pr_scope_errors(base_ref))
    if base_has(base_ref, AUTHORITY_PATH) and base_text(base_ref, AUTHORITY_PATH) != (ROOT / AUTHORITY_PATH).read_text(encoding="utf-8"):
        errors.append("accepted C3G contract authority is immutable after acceptance")
    if base_has(base_ref, ERRATUM_PATH) and base_text(base_ref, ERRATUM_PATH) != (ROOT / ERRATUM_PATH).read_text(encoding="utf-8"):
        errors.append("accepted C3G schema-key erratum is immutable after acceptance")
    graph_in_base = base_has(base_ref, GRAPH_FRAGMENT)
    if graph_in_base:
        if not (ROOT / GRAPH_FRAGMENT).is_file():
            errors.append("accepted C3G Graph fragment cannot be removed")
        elif base_text(base_ref, GRAPH_FRAGMENT) != (ROOT / GRAPH_FRAGMENT).read_text(encoding="utf-8"):
            errors.append("accepted C3G Graph fragment is immutable after consumption")
        if base_text(base_ref, MAILBOX_FRAGMENT) != (ROOT / MAILBOX_FRAGMENT).read_text(encoding="utf-8"):
            errors.append("accepted C3G MailboxProvider migration is immutable after consumption")
    return errors


def assert_real_fragment_shape() -> None:
    document = load_json(MAILBOX_FRAGMENT)
    try:
        schemas = document["components"]["schemas"]
    except (KeyError, TypeError) as exc:
        raise AssertionError("mailboxes fragment must contain components.schemas") from exc
    assert isinstance(schemas, dict)
    assert ACTUAL_FRAGMENT_SCHEMA_NAME in schemas
    assert AUTHORITY_SCHEMA_NAME not in schemas


def self_test() -> None:
    expected = expected_openapi_changes()
    assert not evaluate_openapi_changes([], authority_accepted_in_base=False, erratum_accepted_in_base=False, graph_fragment_present_in_base=False)
    assert evaluate_openapi_changes(expected, authority_accepted_in_base=False, erratum_accepted_in_base=False, graph_fragment_present_in_base=False)
    assert evaluate_openapi_changes(expected, authority_accepted_in_base=True, erratum_accepted_in_base=False, graph_fragment_present_in_base=False)
    assert not evaluate_openapi_changes(expected, authority_accepted_in_base=True, erratum_accepted_in_base=True, graph_fragment_present_in_base=False)
    assert evaluate_openapi_changes([("A", str(GRAPH_FRAGMENT))], authority_accepted_in_base=True, erratum_accepted_in_base=True, graph_fragment_present_in_base=False)
    assert evaluate_openapi_changes(expected, authority_accepted_in_base=True, erratum_accepted_in_base=True, graph_fragment_present_in_base=True)
    base = {"components": {"schemas": {ACTUAL_FRAGMENT_SCHEMA_NAME: {"type": "string", "enum": EXPECTED_ACCEPTED_PROVIDERS}}}}
    good = copy.deepcopy(base)
    good["components"]["schemas"][ACTUAL_FRAGMENT_SCHEMA_NAME]["enum"] = [*EXPECTED_ACCEPTED_PROVIDERS, EXPECTED_GRAPH_PROVIDER]
    assert not migrated_mailboxes_errors(base, good)
    bad = copy.deepcopy(good)
    bad["components"]["schemas"][ACTUAL_FRAGMENT_SCHEMA_NAME]["enum"].append("OTHER")
    assert migrated_mailboxes_errors(base, bad)
    old_wrong_key = {"components": {"schemas": {AUTHORITY_SCHEMA_NAME: {"type": "string", "enum": EXPECTED_ACCEPTED_PROVIDERS}}}}
    assert migrated_mailboxes_errors(old_wrong_key, copy.deepcopy(old_wrong_key))
    assert_real_fragment_shape()
    print("C3G governed provider migration + schema-key erratum self-test passed.")


def check_authority_only(base_ref: str) -> None:
    errors = accepted_authority_errors(base_ref)
    if errors:
        raise SystemExit("\n".join(errors))
    print("Accepted C3G contract authority, schema-key erratum, and consumed artifacts are immutable when present.")


def check_repository(base_ref: str) -> None:
    errors = accepted_authority_errors(base_ref)
    authority_accepted_in_base = base_has(base_ref, AUTHORITY_PATH)
    erratum_accepted_in_base = base_has(base_ref, ERRATUM_PATH)
    graph_in_base = base_has(base_ref, GRAPH_FRAGMENT)
    changes = openapi_changes(base_ref)
    errors.extend(evaluate_openapi_changes(changes, authority_accepted_in_base=authority_accepted_in_base, erratum_accepted_in_base=erratum_accepted_in_base, graph_fragment_present_in_base=graph_in_base))
    if changes == expected_openapi_changes() and authority_accepted_in_base and erratum_accepted_in_base and not graph_in_base:
        errors.extend(migrated_mailboxes_errors(base_json(base_ref, MAILBOX_FRAGMENT), load_json(MAILBOX_FRAGMENT)))
        if not (ROOT / GRAPH_FRAGMENT).is_file():
            errors.append("authorized C3G Graph fragment must exist")
        else:
            load_json(GRAPH_FRAGMENT)
    if errors:
        raise SystemExit("\n".join(errors))
    if changes:
        print("C3G one-shot Graph provider contract migration is valid.")
    else:
        print("C3G contract authority + schema-key erratum are valid; no C3G v1 migration is consumed in this change.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-ref")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--authority-only", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    base_ref = args.base_ref or "origin/main"
    if args.authority_only:
        check_authority_only(base_ref)
    else:
        check_repository(base_ref)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
