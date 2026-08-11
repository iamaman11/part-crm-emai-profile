#!/usr/bin/env python3
"""Validate the allowlist-only Phase 2I support/evidence bundle contract."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

POLICY = Path("tests/support/phase2i-support-bundle.json")
EXPECTED_SECTIONS = {
    "capabilitySummary": {"capability_class", "result_class", "status_class"},
    "failureSummary": {"operation_class", "dependency_class", "reason_class", "result_class"},
    "recoverySummary": {"dependency_class", "operation_class", "result_class"},
    "operationalSummary": {"metric_class", "status_class", "result_class"},
}
EXPECTED_FORBIDDEN_CLASSES = {
    "tenant_identifier",
    "actor_identifier",
    "identity_identifier",
    "client_identifier",
    "profile_identifier",
    "mailbox_identifier",
    "message_identifier",
    "device_identifier",
    "job_identifier",
    "email_address",
    "mail_subject",
    "mail_body",
    "contact_plaintext",
    "credential_material",
    "access_token",
    "cookie_material",
    "browser_storage",
    "decrypted_generation_bytes",
    "generation_key_material",
    "raw_database_export",
}
EXPECTED_EVIDENCE = {
    "tests/cross-component/phase2i-release-candidate.json",
    "tests/operations/phase2i-operational-bounds.json",
    "docs/PHASE2I_DISASTER_RECOVERY_RUNBOOK.md",
}
FORBIDDEN_KEY = re.compile(
    r"(?:tenant|actor|identity|client|profile|mailbox|message|device|job|email|subject|body|contact|credential|token|cookie|secret|storage|plaintext|key|database_export).*(?:id|value|text|address|bytes|material)?$",
    re.IGNORECASE,
)
EMAIL_LIKE = re.compile(r"[^\s@]+@[^\s@]+\.[^\s@]+")
OPAQUE_ID_LIKE = re.compile(
    r"\b(?:tenant|actor|identity|client|profile|mailbox|message|device|job|claim|generation)_[A-Za-z0-9_-]{6,}\b",
    re.IGNORECASE,
)


def validate_bundle_payload(payload: object, allowed_fields: set[str], path: str = "$") -> list[str]:
    """Validate a generated support payload against one section's low-cardinality allowlist."""
    if not isinstance(payload, dict):
        return [f"{path} support payload must be an object"]
    errors: list[str] = []
    for key, value in payload.items():
        if key not in allowed_fields:
            errors.append(f"{path}.{key} is not an allowlisted support field")
            continue
        if FORBIDDEN_KEY.search(key):
            errors.append(f"{path}.{key} is a forbidden sensitive/high-cardinality field")
        if not isinstance(value, str) or not re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", value):
            errors.append(f"{path}.{key} must be a bounded low-cardinality class value")
            continue
        if EMAIL_LIKE.search(value) or OPAQUE_ID_LIKE.search(value):
            errors.append(f"{path}.{key} contains identifier-like or address-like data")
    return errors


def validate_policy(root: Path, policy: object) -> list[str]:
    if not isinstance(policy, dict):
        return ["Phase 2I support bundle policy must be an object"]
    errors: list[str] = []
    expected_scalars = {
        "schemaVersion": 1,
        "phase": "Phase 2I",
        "scope": "repository-local-support-evidence",
        "productionReady": False,
        "releaseState": "in-progress",
    }
    for key, expected in expected_scalars.items():
        if policy.get(key) != expected:
            errors.append(f"support bundle policy {key} changed unexpectedly")

    sections = policy.get("sections")
    if not isinstance(sections, dict) or set(sections) != set(EXPECTED_SECTIONS):
        actual = sorted(sections) if isinstance(sections, dict) else []
        errors.append(f"support section set changed: expected={sorted(EXPECTED_SECTIONS)} actual={actual}")
    else:
        for name, expected_fields in EXPECTED_SECTIONS.items():
            entry = sections.get(name)
            if not isinstance(entry, dict) or set(entry) != {"allowedFields"}:
                errors.append(f"support section {name} must contain exactly allowedFields")
                continue
            fields = entry.get("allowedFields")
            if not isinstance(fields, list) or set(fields) != expected_fields:
                errors.append(f"support section {name} allowlist changed unexpectedly")
            elif any(FORBIDDEN_KEY.search(field) for field in fields):
                errors.append(f"support section {name} contains sensitive/high-cardinality allowlist fields")

    forbidden = policy.get("forbiddenDataClasses")
    if not isinstance(forbidden, list) or set(forbidden) != EXPECTED_FORBIDDEN_CLASSES:
        errors.append("support bundle forbidden-data class set changed unexpectedly")

    evidence = policy.get("evidenceSources")
    if not isinstance(evidence, list) or set(evidence) != EXPECTED_EVIDENCE:
        errors.append("support bundle evidence source set changed unexpectedly")
    else:
        for relative in evidence:
            path = Path(relative)
            if path.is_absolute() or ".." in path.parts or not (root / path).is_file():
                errors.append(f"support bundle evidence source is missing or unsafe: {relative}")
    return errors


def self_test(root: Path, policy: dict[str, object]) -> None:
    fixtures = [
        (
            "identifier field",
            {"operation_class": "mail_query", "client_id": "client_01jsensitive"},
            EXPECTED_SECTIONS["failureSummary"],
        ),
        (
            "email value",
            {"operation_class": "mail_query", "result_class": "person@example.com"},
            EXPECTED_SECTIONS["failureSummary"],
        ),
        (
            "opaque identifier value",
            {"operation_class": "mail_query", "result_class": "profile_01jsensitive"},
            EXPECTED_SECTIONS["failureSummary"],
        ),
    ]
    for label, payload, allowed in fixtures:
        if not validate_bundle_payload(payload, allowed):
            raise ValueError(f"support bundle negative fixture unexpectedly passed: {label}")

    candidate = json.loads(json.dumps(policy))
    candidate["productionReady"] = True
    if not validate_policy(root, candidate):
        raise ValueError("support bundle production-ready promotion fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    path = args.root / POLICY
    try:
        policy = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"cannot load Phase 2I support bundle policy: {error}")
        return 1
    errors = validate_policy(args.root, policy)
    if errors:
        for error in errors:
            print(error)
        return 1
    if args.self_test:
        try:
            self_test(args.root, policy)
        except (KeyError, TypeError, ValueError) as error:
            print(error)
            return 1
        print("Phase 2I support bundle negative fixtures rejected as expected.")
        return 0
    print("Phase 2I allowlist-only metadata-safe support bundle policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
