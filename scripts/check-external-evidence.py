#!/usr/bin/env python3
"""Validate immutable, metadata-only external production evidence records."""

from __future__ import annotations

import argparse
import ipaddress
import json
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

SCHEMA_VERSION = 1
MAX_RECORD_BYTES = 64 * 1024
ID_RE = re.compile(r"ev-[0-9]{8}-[a-z0-9][a-z0-9-]{2,47}\Z")
TOKEN_RE = re.compile(r"[a-z][a-z0-9._-]{2,95}\Z")
LOGIN_RE = re.compile(r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?\Z")
SHA256_RE = re.compile(r"[0-9a-f]{64}\Z")
EMAIL_RE = re.compile(
    r"(?i)(?<![a-z0-9._%+-])[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}(?![a-z0-9.-])"
)
WINDOWS_USER_PATH_RE = re.compile(r"(?i)[a-z]:\\users\\")
UNIX_USER_PATH_RE = re.compile(r"/(?:home|users)/[^/\s]+")
PEM_RE = re.compile(r"-----BEGIN [A-Z0-9 ]*(?:PRIVATE KEY|CERTIFICATE)-----")
AUTH_RE = re.compile(r"(?i)\b(?:authorization|bearer|basic)\s*[:= ]")
URI_CREDENTIAL_RE = re.compile(r"(?i)[a-z][a-z0-9+.-]*://[^/\s:@]+:[^/\s@]+@")
HEX_SECRET_RE = re.compile(
    r"(?i)\b(?:key|secret|token|password)[-_ ]?[=:][-_ ]?[0-9a-f]{24,}\b"
)
BASE64_SECRET_RE = re.compile(
    r"(?i)\b(?:key|secret|token|password)[-_ ]?[=:][-_ ]?[A-Za-z0-9+/]{24,}={0,2}\b"
)

GATE_CHECKS: dict[str, tuple[str, ...]] = {
    "legacy_credential_rotation": (
        "old_credential_revoked",
        "old_credential_authentication_rejected",
        "provider_access_logs_reviewed",
        "replacement_in_approved_secret_store",
        "repository_regression_scan_passed",
    ),
    "cloudflare_environment": (
        "isolated_resources_provisioned",
        "access_policy_enforced",
        "cost_limit_configured",
        "remote_smoke_passed",
    ),
    "windows_primary_host": (
        "physical_host_attested",
        "bridge_release_executed",
        "real_camouhost_lifecycle_passed",
        "metadata_only_support_bundle_reviewed",
    ),
    "windows_secondary_host": (
        "independent_physical_host_attested",
        "device_grant_applied",
        "restore_and_launch_passed",
        "revocation_enforced",
    ),
    "trusted_windows_signing": (
        "trusted_certificate_chain_verified",
        "signed_binary_digest_verified",
        "windows_verification_passed",
        "update_verification_passed",
    ),
    "offline_key_escrow_restore": (
        "dual_control_exercised",
        "clean_environment_restore_passed",
        "rotation_recovery_passed",
        "key_loss_policy_approved",
    ),
    "privacy_retention_approval": (
        "retention_values_approved",
        "acceptable_use_policy_approved",
        "export_delete_flow_approved",
        "support_access_policy_approved",
    ),
    "product_license": (
        "license_selected",
        "third_party_notices_reviewed",
        "redistribution_rights_approved",
    ),
    "real_fingerprint_certification": (
        "ten_cold_launches_completed",
        "profile_stable_signals_passed",
        "origin_deterministic_signals_passed",
        "network_coherence_passed",
        "specialized_sites_reviewed",
        "no_unresolved_fail_signals",
        "cross_profile_isolation_passed",
    ),
    "production_device_key_unwrap": (
        "os_key_protection_verified",
        "device_identity_verified",
        "unwrap_authorization_verified",
        "revocation_verified",
        "recovery_path_verified",
    ),
    "remote_r2_d1_atomicity": (
        "immutable_upload_verified",
        "pointer_cas_verified",
        "nonce_claim_verified",
        "rollback_verified",
        "orphan_reconciliation_verified",
    ),
    "independent_security_review": (
        "reviewer_independence_confirmed",
        "threat_model_scope_reviewed",
        "cryptographic_scope_reviewed",
        "findings_resolved_or_accepted",
        "residual_risk_accepted",
    ),
}

TOP_LEVEL_REQUIRED = {
    "schema_version",
    "evidence_id",
    "gate",
    "status",
    "observed_at",
    "scope",
    "checks",
    "references",
    "artifact_digests_sha256",
    "limitations",
}
TOP_LEVEL_OPTIONAL = {"review", "supersedes"}
SCOPE_FIELDS = {"environment", "subject_id"}
REVIEW_FIELDS = {"github_login", "review_reference", "reviewed_at"}
CHECK_FIELDS = {"code", "outcome"}
STATUSES = {"pending", "passed", "failed"}
ENVIRONMENTS = {"none", "dev", "staging", "production"}
OUTCOMES = {"pass", "fail"}


class DuplicateKey(ValueError):
    pass


class ValidationError(ValueError):
    pass


@dataclass(frozen=True)
class Record:
    path: Path
    data: dict[str, Any]
    evidence_id: str
    gate: str
    status: str
    observed_at: datetime
    supersedes: str | None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    return parser.parse_args()


def object_no_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKey(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def require_exact_fields(
    value: dict[str, Any],
    required: set[str],
    optional: set[str],
    where: str,
) -> None:
    missing = sorted(required - value.keys())
    unknown = sorted(value.keys() - required - optional)
    if missing or unknown:
        raise ValidationError(f"{where}: missing={missing}, unknown={unknown}")


def require_string(value: Any, where: str) -> str:
    if not isinstance(value, str):
        raise ValidationError(f"{where}: expected string")
    return value


def parse_utc(value: Any, where: str) -> datetime:
    text = require_string(value, where)
    if not text.endswith("Z"):
        raise ValidationError(f"{where}: timestamp must use UTC Z suffix")
    try:
        parsed = datetime.fromisoformat(text[:-1] + "+00:00")
    except ValueError as exc:
        raise ValidationError(f"{where}: invalid RFC3339 timestamp") from exc
    if parsed.tzinfo != timezone.utc or parsed.microsecond != 0:
        raise ValidationError(f"{where}: timestamp must be whole-second UTC")
    return parsed


def validate_reference(value: Any, where: str, github_only: bool = False) -> str:
    text = require_string(value, where)
    if text.startswith("https://github.com/"):
        parsed = urlsplit(text)
        if parsed.scheme != "https" or parsed.netloc != "github.com":
            raise ValidationError(f"{where}: invalid GitHub reference")
        if parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise ValidationError(
                f"{where}: reference must not contain credentials, query or fragment"
            )
        parts = [part for part in parsed.path.split("/") if part]
        if len(parts) < 4 or any(not part or part in {".", ".."} for part in parts):
            raise ValidationError(
                f"{where}: reference must identify a reviewable GitHub object"
            )
        return text
    if github_only:
        raise ValidationError(f"{where}: terminal review must be a GitHub reference")
    if text.startswith("provider-case:"):
        token = text.removeprefix("provider-case:")
        if not TOKEN_RE.fullmatch(token):
            raise ValidationError(f"{where}: invalid provider case token")
        return text
    if text.startswith("review-report:sha256:"):
        digest = text.removeprefix("review-report:sha256:")
        if not SHA256_RE.fullmatch(digest):
            raise ValidationError(f"{where}: invalid report digest")
        return text
    raise ValidationError(f"{where}: unsupported reference type")


def reject_sensitive_text(raw: str, where: str) -> None:
    checks = (
        (EMAIL_RE, "email address"),
        (WINDOWS_USER_PATH_RE, "Windows user path"),
        (UNIX_USER_PATH_RE, "Unix user path"),
        (PEM_RE, "PEM material"),
        (AUTH_RE, "authorization material"),
        (URI_CREDENTIAL_RE, "URI credential"),
        (HEX_SECRET_RE, "hex secret-like material"),
        (BASE64_SECRET_RE, "base64 secret-like material"),
    )
    for pattern, label in checks:
        if pattern.search(raw):
            raise ValidationError(f"{where}: prohibited {label}")
    candidates = re.findall(
        r"(?<![0-9A-Za-z])(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?![0-9A-Za-z])",
        raw,
    )
    for token in candidates:
        try:
            ipaddress.ip_address(token)
        except ValueError:
            continue
        raise ValidationError(f"{where}: raw IP address is prohibited")


def canonical_json(data: dict[str, Any]) -> str:
    return json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def validate_record(path: Path) -> Record:
    raw_bytes = path.read_bytes()
    if len(raw_bytes) > MAX_RECORD_BYTES:
        raise ValidationError(f"{path}: record exceeds {MAX_RECORD_BYTES} bytes")
    try:
        raw = raw_bytes.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ValidationError(f"{path}: record must be UTF-8") from exc
    reject_sensitive_text(raw, str(path))
    try:
        data = json.loads(raw, object_pairs_hook=object_no_duplicates)
    except (json.JSONDecodeError, DuplicateKey) as exc:
        raise ValidationError(f"{path}: invalid JSON: {exc}") from exc
    if not isinstance(data, dict):
        raise ValidationError(f"{path}: top-level value must be an object")
    if raw != canonical_json(data):
        raise ValidationError(
            f"{path}: JSON must be canonical (sorted keys, two-space indent, final newline)"
        )

    require_exact_fields(data, TOP_LEVEL_REQUIRED, TOP_LEVEL_OPTIONAL, str(path))
    if data["schema_version"] != SCHEMA_VERSION:
        raise ValidationError(f"{path}: schema_version must equal {SCHEMA_VERSION}")

    evidence_id = require_string(data["evidence_id"], f"{path}.evidence_id")
    if not ID_RE.fullmatch(evidence_id) or path.stem != evidence_id:
        raise ValidationError(
            f"{path}: evidence_id must match the filename and approved pattern"
        )

    gate = require_string(data["gate"], f"{path}.gate")
    if gate not in GATE_CHECKS:
        raise ValidationError(f"{path}: unsupported gate {gate!r}")
    status = require_string(data["status"], f"{path}.status")
    if status not in STATUSES:
        raise ValidationError(f"{path}: unsupported status {status!r}")
    observed_at = parse_utc(data["observed_at"], f"{path}.observed_at")

    scope = data["scope"]
    if not isinstance(scope, dict):
        raise ValidationError(f"{path}.scope: expected object")
    require_exact_fields(scope, SCOPE_FIELDS, set(), f"{path}.scope")
    environment = require_string(scope["environment"], f"{path}.scope.environment")
    if environment not in ENVIRONMENTS:
        raise ValidationError(f"{path}.scope.environment: unsupported value")
    subject_id = require_string(scope["subject_id"], f"{path}.scope.subject_id")
    if subject_id != "none" and not TOKEN_RE.fullmatch(subject_id):
        raise ValidationError(
            f"{path}.scope.subject_id: must be opaque and token-shaped"
        )

    checks = data["checks"]
    if not isinstance(checks, list):
        raise ValidationError(f"{path}.checks: expected array")
    allowed_codes = set(GATE_CHECKS[gate])
    seen_codes: set[str] = set()
    outcomes: dict[str, str] = {}
    for index, check in enumerate(checks):
        where = f"{path}.checks[{index}]"
        if not isinstance(check, dict):
            raise ValidationError(f"{where}: expected object")
        require_exact_fields(check, CHECK_FIELDS, set(), where)
        code = require_string(check["code"], f"{where}.code")
        outcome = require_string(check["outcome"], f"{where}.outcome")
        if code not in allowed_codes:
            raise ValidationError(
                f"{where}: check code is not defined for gate {gate}"
            )
        if code in seen_codes:
            raise ValidationError(f"{where}: duplicate check code {code}")
        if outcome not in OUTCOMES:
            raise ValidationError(f"{where}: unsupported outcome")
        seen_codes.add(code)
        outcomes[code] = outcome

    references = data["references"]
    if not isinstance(references, list) or not (1 <= len(references) <= 10):
        raise ValidationError(f"{path}.references: expected 1..10 references")
    validated_references = [
        validate_reference(value, f"{path}.references[{index}]")
        for index, value in enumerate(references)
    ]
    if len(set(validated_references)) != len(validated_references):
        raise ValidationError(f"{path}.references: duplicate reference")

    digests = data["artifact_digests_sha256"]
    if not isinstance(digests, list) or len(digests) > 10:
        raise ValidationError(
            f"{path}.artifact_digests_sha256: expected at most 10 digests"
        )
    for index, digest in enumerate(digests):
        if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
            raise ValidationError(
                f"{path}.artifact_digests_sha256[{index}]: invalid SHA-256"
            )
    if len(set(digests)) != len(digests):
        raise ValidationError(f"{path}.artifact_digests_sha256: duplicate digest")

    limitations = data["limitations"]
    if not isinstance(limitations, list) or len(limitations) > 20:
        raise ValidationError(f"{path}.limitations: expected at most 20 tokens")
    for index, limitation in enumerate(limitations):
        if not isinstance(limitation, str) or not TOKEN_RE.fullmatch(limitation):
            raise ValidationError(
                f"{path}.limitations[{index}]: use a bounded token, not free text"
            )
    if len(set(limitations)) != len(limitations):
        raise ValidationError(f"{path}.limitations: duplicate token")

    review = data.get("review")
    if status == "pending":
        if review is not None:
            raise ValidationError(
                f"{path}: pending evidence must not contain terminal review"
            )
        if any(outcome == "fail" for outcome in outcomes.values()):
            raise ValidationError(
                f"{path}: pending evidence cannot contain a final fail result"
            )
    else:
        if not isinstance(review, dict):
            raise ValidationError(f"{path}: terminal evidence requires review")
        require_exact_fields(review, REVIEW_FIELDS, set(), f"{path}.review")
        login = require_string(
            review["github_login"], f"{path}.review.github_login"
        )
        if not LOGIN_RE.fullmatch(login):
            raise ValidationError(
                f"{path}.review.github_login: invalid GitHub login"
            )
        validate_reference(
            review["review_reference"],
            f"{path}.review.review_reference",
            github_only=True,
        )
        reviewed_at = parse_utc(
            review["reviewed_at"], f"{path}.review.reviewed_at"
        )
        if reviewed_at < observed_at:
            raise ValidationError(
                f"{path}: reviewed_at must not precede observed_at"
            )

    required_codes = set(GATE_CHECKS[gate])
    if status == "passed":
        missing = sorted(required_codes - outcomes.keys())
        failed = sorted(
            code for code, outcome in outcomes.items() if outcome != "pass"
        )
        if missing or failed:
            raise ValidationError(
                f"{path}: passed evidence missing={missing}, failed={failed}"
            )
        if not digests:
            raise ValidationError(
                f"{path}: passed evidence requires at least one artifact digest"
            )
    elif status == "failed":
        if not any(outcome == "fail" for outcome in outcomes.values()):
            raise ValidationError(
                f"{path}: failed evidence requires at least one failed check"
            )

    supersedes_raw = data.get("supersedes")
    supersedes: str | None
    if supersedes_raw is None:
        supersedes = None
    else:
        supersedes = require_string(supersedes_raw, f"{path}.supersedes")
        if not ID_RE.fullmatch(supersedes) or supersedes == evidence_id:
            raise ValidationError(f"{path}.supersedes: invalid evidence ID")

    return Record(
        path,
        data,
        evidence_id,
        gate,
        status,
        observed_at,
        supersedes,
    )


def validate_lineage(records: list[Record]) -> None:
    by_id: dict[str, Record] = {}
    for record in records:
        if record.evidence_id in by_id:
            raise ValidationError(f"duplicate evidence ID: {record.evidence_id}")
        by_id[record.evidence_id] = record

    child_by_parent: dict[str, str] = {}
    for record in records:
        if record.supersedes is None:
            continue
        previous = by_id.get(record.supersedes)
        if previous is None:
            raise ValidationError(
                f"{record.path}: dangling supersedes {record.supersedes}"
            )
        if previous.gate != record.gate:
            raise ValidationError(
                f"{record.path}: supersedes must stay within one gate"
            )
        if record.observed_at <= previous.observed_at:
            raise ValidationError(
                f"{record.path}: superseding evidence must be newer"
            )
        existing_child = child_by_parent.get(previous.evidence_id)
        if existing_child is not None:
            raise ValidationError(
                "forked evidence lineage: "
                f"{previous.evidence_id} is superseded by "
                f"{existing_child} and {record.evidence_id}"
            )
        child_by_parent[previous.evidence_id] = record.evidence_id

    for record in records:
        seen: set[str] = set()
        current = record
        while current.supersedes is not None:
            if current.evidence_id in seen:
                raise ValidationError(
                    f"cycle in evidence lineage at {current.evidence_id}"
                )
            seen.add(current.evidence_id)
            current = by_id[current.supersedes]

    leaves_by_gate: dict[str, list[str]] = {}
    for record in records:
        if record.evidence_id not in child_by_parent:
            leaves_by_gate.setdefault(record.gate, []).append(record.evidence_id)
    forks = {
        gate: ids for gate, ids in leaves_by_gate.items() if len(ids) > 1
    }
    if forks:
        raise ValidationError(
            f"multiple active evidence lineages per gate: {forks}"
        )


def validate_tree(root: Path) -> int:
    records_dir = root / "evidence" / "external" / "records"
    if not records_dir.is_dir():
        raise ValidationError(f"missing records directory: {records_dir}")
    unexpected = [
        path
        for path in records_dir.iterdir()
        if path.name not in {"README.md", ".gitkeep"} and path.suffix != ".json"
    ]
    if unexpected:
        raise ValidationError(
            f"unexpected files in records directory: {unexpected}"
        )
    records = [
        validate_record(path) for path in sorted(records_dir.glob("*.json"))
    ]
    validate_lineage(records)
    print(f"external evidence gate passed: {len(records)} immutable record(s)")
    return 0


def main() -> int:
    args = parse_args()
    try:
        return validate_tree(args.root.resolve())
    except (OSError, ValidationError) as exc:
        print(f"external evidence gate failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
