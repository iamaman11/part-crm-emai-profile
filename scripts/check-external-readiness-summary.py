#!/usr/bin/env python3
"""Build and verify the metadata-only external evidence readiness projection."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

SUMMARY_SCHEMA_VERSION = 1
POLICY_VERSION = 1

MANDATORY_REQUIREMENTS: tuple[tuple[str, str], ...] = (
    ("cloudflare_environment", "production"),
    ("independent_security_review", "none"),
    ("legacy_credential_rotation", "none"),
    ("offline_key_escrow_restore", "production"),
    ("privacy_retention_approval", "none"),
    ("product_license", "none"),
    ("production_device_key_unwrap", "production"),
    ("real_fingerprint_certification", "production"),
    ("remote_r2_d1_atomicity", "production"),
    ("trusted_windows_signing", "production"),
    ("windows_primary_host", "production"),
    ("windows_secondary_host", "production"),
)

SAFE_RECORD_FIELDS = {
    "evidence_id",
    "gate",
    "status",
    "observed_at",
    "scope",
    "supersedes",
}
STATUSES = {"pending", "passed", "failed"}


class SummaryError(ValueError):
    pass


@dataclass(frozen=True)
class Record:
    evidence_id: str
    gate: str
    status: str
    observed_at: str
    environment: str
    supersedes: str | None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--summary", type=Path)
    parser.add_argument("--status", type=Path)
    parser.add_argument("--write", action="store_true")
    return parser.parse_args()


def canonical_json(data: dict[str, Any]) -> str:
    return json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def read_json(path: Path, where: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise SummaryError(f"{where}: unreadable JSON at {path}") from exc


def require_string(value: Any, where: str) -> str:
    if not isinstance(value, str):
        raise SummaryError(f"{where}: expected string")
    return value


def parse_record(path: Path) -> Record:
    data = read_json(path, "external evidence record")
    if not isinstance(data, dict):
        raise SummaryError(f"{path}: top-level value must be an object")

    missing = sorted(SAFE_RECORD_FIELDS - {"supersedes"} - data.keys())
    if missing:
        raise SummaryError(f"{path}: missing projection fields {missing}")

    evidence_id = require_string(data.get("evidence_id"), f"{path}.evidence_id")
    gate = require_string(data.get("gate"), f"{path}.gate")
    status = require_string(data.get("status"), f"{path}.status")
    if status not in STATUSES:
        raise SummaryError(f"{path}.status: unsupported status")
    observed_at = require_string(data.get("observed_at"), f"{path}.observed_at")
    if len(observed_at) != 20 or observed_at[10] != "T" or not observed_at.endswith("Z"):
        raise SummaryError(f"{path}.observed_at: strict validator must run first")

    scope = data.get("scope")
    if not isinstance(scope, dict):
        raise SummaryError(f"{path}.scope: expected object")
    environment = require_string(scope.get("environment"), f"{path}.scope.environment")

    supersedes_raw = data.get("supersedes")
    supersedes = None if supersedes_raw is None else require_string(
        supersedes_raw, f"{path}.supersedes"
    )
    return Record(evidence_id, gate, status, observed_at, environment, supersedes)


def load_records(root: Path) -> list[Record]:
    records_dir = root / "evidence" / "external" / "records"
    if not records_dir.is_dir():
        raise SummaryError(f"missing records directory: {records_dir}")
    records = [parse_record(path) for path in sorted(records_dir.glob("*.json"))]

    by_id: dict[str, Record] = {}
    for record in records:
        if record.evidence_id in by_id:
            raise SummaryError(f"duplicate evidence ID: {record.evidence_id}")
        by_id[record.evidence_id] = record

    for record in records:
        if record.supersedes is not None and record.supersedes not in by_id:
            raise SummaryError(
                f"{record.evidence_id}: dangling supersedes {record.supersedes}"
            )
    return records


def active_records(records: list[Record]) -> list[Record]:
    superseded = {record.supersedes for record in records if record.supersedes is not None}
    active = [record for record in records if record.evidence_id not in superseded]

    active_by_gate: dict[str, Record] = {}
    for record in active:
        previous = active_by_gate.get(record.gate)
        if previous is not None:
            raise SummaryError(
                f"multiple active records for gate {record.gate}: "
                f"{previous.evidence_id}, {record.evidence_id}"
            )
        active_by_gate[record.gate] = record
    return sorted(active, key=lambda item: (item.gate, item.environment, item.evidence_id))


def build_summary(records: list[Record]) -> dict[str, Any]:
    active = active_records(records)
    active_projection = [
        {
            "environment": record.environment,
            "evidence_id": record.evidence_id,
            "gate": record.gate,
            "observed_date": record.observed_at[:10],
            "status": record.status,
        }
        for record in active
    ]

    passed_pairs = {
        (record.gate, record.environment)
        for record in active
        if record.status == "passed"
    }
    missing = [
        {"environment": environment, "gate": gate}
        for gate, environment in MANDATORY_REQUIREMENTS
        if (gate, environment) not in passed_pairs
    ]
    satisfied = len(MANDATORY_REQUIREMENTS) - len(missing)

    status_counts = {status: 0 for status in sorted(STATUSES)}
    for record in active:
        status_counts[record.status] += 1

    return {
        "active_records": active_projection,
        "counts": {
            "active_failed": status_counts["failed"],
            "active_passed": status_counts["passed"],
            "active_pending": status_counts["pending"],
            "mandatory_requirements": len(MANDATORY_REQUIREMENTS),
            "satisfied_requirements": satisfied,
            "total_records": len(records),
        },
        "eligible_for_production_review": not missing,
        "mandatory_requirements": [
            {"environment": environment, "gate": gate}
            for gate, environment in MANDATORY_REQUIREMENTS
        ],
        "missing_requirements": missing,
        "policy_version": POLICY_VERSION,
        "schema_version": SUMMARY_SCHEMA_VERSION,
    }


def resolve_path(root: Path, value: Path | None, default: str) -> Path:
    if value is None:
        return root / default
    return value if value.is_absolute() else root / value


def validate_status(status_path: Path, eligible: bool) -> None:
    data = read_json(status_path, "delivery status")
    if not isinstance(data, dict) or not isinstance(data.get("production_ready"), bool):
        raise SummaryError(f"{status_path}: production_ready must be boolean")
    if data["production_ready"] and not eligible:
        raise SummaryError(
            "production_ready cannot be true while mandatory external evidence is incomplete"
        )


def check_summary(root: Path, summary_path: Path, status_path: Path, write: bool) -> int:
    records = load_records(root)
    summary = build_summary(records)
    expected = canonical_json(summary)

    if write:
        summary_path.parent.mkdir(parents=True, exist_ok=True)
        summary_path.write_text(expected, encoding="utf-8")
    else:
        try:
            committed = summary_path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as exc:
            raise SummaryError(f"missing or unreadable summary: {summary_path}") from exc
        if committed != expected:
            raise SummaryError(
                f"{summary_path}: committed summary differs from deterministic projection; "
                "run with --write"
            )

    validate_status(status_path, bool(summary["eligible_for_production_review"]))
    print(
        "external readiness summary passed: "
        f"records={summary['counts']['total_records']} "
        f"satisfied={summary['counts']['satisfied_requirements']}/"
        f"{summary['counts']['mandatory_requirements']} "
        f"eligible={str(summary['eligible_for_production_review']).lower()}"
    )
    return 0


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    summary_path = resolve_path(root, args.summary, "docs/external-evidence-summary.json")
    status_path = resolve_path(root, args.status, "docs/status.json")
    try:
        return check_summary(root, summary_path, status_path, args.write)
    except (OSError, SummaryError) as exc:
        print(f"external readiness summary failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
