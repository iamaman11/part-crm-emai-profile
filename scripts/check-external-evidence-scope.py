#!/usr/bin/env python3
"""Enforce strict timestamp, environment and raw-IP scope rules for external evidence."""

from __future__ import annotations

import argparse
import ipaddress
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator

TIMESTAMP_RE = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z\Z")
EVIDENCE_DATE_RE = re.compile(r"ev-([0-9]{8})-[a-z0-9][a-z0-9-]{2,47}\Z")
IPV6_CANDIDATE_RE = re.compile(
    r"(?<![0-9A-Za-z_.%-])"
    r"\[?[0-9A-Fa-f.%_-]*(?::[0-9A-Fa-f.%_-]*){2,}\]?"
    r"(?![0-9A-Za-z_.%-])"
)

GATE_ENVIRONMENTS: dict[str, frozenset[str]] = {
    "legacy_credential_rotation": frozenset({"none"}),
    "cloudflare_environment": frozenset({"dev", "staging", "production"}),
    "windows_primary_host": frozenset({"staging", "production"}),
    "windows_secondary_host": frozenset({"staging", "production"}),
    "trusted_windows_signing": frozenset({"production"}),
    "offline_key_escrow_restore": frozenset({"staging", "production"}),
    "privacy_retention_approval": frozenset({"none"}),
    "product_license": frozenset({"none"}),
    "real_fingerprint_certification": frozenset({"staging", "production"}),
    "production_device_key_unwrap": frozenset({"production"}),
    "remote_r2_d1_atomicity": frozenset({"staging", "production"}),
    "independent_security_review": frozenset({"none"}),
}


class ScopeValidationError(ValueError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    return parser.parse_args()


def parse_strict_utc(value: Any, where: str) -> datetime:
    if not isinstance(value, str) or not TIMESTAMP_RE.fullmatch(value):
        raise ScopeValidationError(
            f"{where}: timestamp must be exact whole-second YYYY-MM-DDTHH:MM:SSZ"
        )
    try:
        parsed = datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except ValueError as exc:
        raise ScopeValidationError(f"{where}: invalid UTC calendar timestamp") from exc
    return parsed


def walk_strings(value: Any, path: str = "$") -> Iterator[tuple[str, str]]:
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, list):
        for index, item in enumerate(value):
            yield from walk_strings(item, f"{path}[{index}]")
    elif isinstance(value, dict):
        for key, item in value.items():
            yield from walk_strings(item, f"{path}.{key}")


def reject_raw_ipv6(value: str, where: str) -> None:
    for match in IPV6_CANDIDATE_RE.finditer(value):
        candidate = match.group(0).strip("[]")
        try:
            address = ipaddress.ip_address(candidate)
        except ValueError:
            continue
        if address.version == 6:
            raise ScopeValidationError(f"{where}: raw IPv6 address is prohibited")


def validate_record(path: Path) -> None:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ScopeValidationError(f"{path}: unreadable JSON") from exc
    if not isinstance(data, dict):
        raise ScopeValidationError(f"{path}: top-level value must be an object")

    gate = data.get("gate")
    if not isinstance(gate, str) or gate not in GATE_ENVIRONMENTS:
        raise ScopeValidationError(f"{path}.gate: unsupported gate")

    scope = data.get("scope")
    if not isinstance(scope, dict):
        raise ScopeValidationError(f"{path}.scope: expected object")
    environment = scope.get("environment")
    if environment not in GATE_ENVIRONMENTS[gate]:
        allowed = sorted(GATE_ENVIRONMENTS[gate])
        raise ScopeValidationError(
            f"{path}.scope.environment: {environment!r} is invalid for {gate}; allowed={allowed}"
        )

    evidence_id = data.get("evidence_id")
    if not isinstance(evidence_id, str):
        raise ScopeValidationError(f"{path}.evidence_id: expected string")
    match = EVIDENCE_DATE_RE.fullmatch(evidence_id)
    if match is None:
        raise ScopeValidationError(f"{path}.evidence_id: invalid evidence ID")

    observed_at = parse_strict_utc(data.get("observed_at"), f"{path}.observed_at")
    if match.group(1) != observed_at.strftime("%Y%m%d"):
        raise ScopeValidationError(
            f"{path}: evidence ID date must equal the observed_at UTC date"
        )

    review = data.get("review")
    if review is not None:
        if not isinstance(review, dict):
            raise ScopeValidationError(f"{path}.review: expected object")
        reviewed_at = parse_strict_utc(review.get("reviewed_at"), f"{path}.review.reviewed_at")
        if reviewed_at < observed_at:
            raise ScopeValidationError(f"{path}: reviewed_at must not precede observed_at")

    for json_path, value in walk_strings(data):
        reject_raw_ipv6(value, f"{path}{json_path}")


def validate_tree(root: Path) -> int:
    records_dir = root / "evidence" / "external" / "records"
    if not records_dir.is_dir():
        raise ScopeValidationError(f"missing records directory: {records_dir}")
    records = sorted(records_dir.glob("*.json"))
    for path in records:
        validate_record(path)
    print(f"external evidence scope gate passed: {len(records)} immutable record(s)")
    return 0


def main() -> int:
    args = parse_args()
    try:
        return validate_tree(args.root.resolve())
    except ScopeValidationError as exc:
        print(f"external evidence scope gate failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
