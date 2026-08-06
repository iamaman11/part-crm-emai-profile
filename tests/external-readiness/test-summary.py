#!/usr/bin/env python3
"""Executable synthetic fixtures for external readiness summary invariants."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "check-external-readiness-summary.py"
SPEC = importlib.util.spec_from_file_location("external_readiness_summary", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load external readiness summary module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)

Record = MODULE.Record
SummaryError = MODULE.SummaryError
MANDATORY_REQUIREMENTS = MODULE.MANDATORY_REQUIREMENTS
build_summary = MODULE.build_summary
validate_status = MODULE.validate_status


def record(
    evidence_id: str,
    gate: str,
    status: str,
    environment: str,
    observed_at: str = "2026-08-06T14:00:00Z",
    supersedes: str | None = None,
) -> object:
    return Record(evidence_id, gate, status, observed_at, environment, supersedes)


def assert_empty() -> None:
    summary = build_summary([])
    assert summary["eligible_for_production_review"] is False
    assert summary["active_records"] == []
    assert summary["counts"] == {
        "active_failed": 0,
        "active_passed": 0,
        "active_pending": 0,
        "mandatory_requirements": 12,
        "satisfied_requirements": 0,
        "total_records": 0,
    }
    assert len(summary["missing_requirements"]) == 12


def assert_pending_and_failed() -> None:
    pending = record(
        "ev-20260806-pending-license",
        "product_license",
        "pending",
        "none",
    )
    summary = build_summary([pending])
    assert summary["counts"]["active_pending"] == 1
    assert summary["counts"]["satisfied_requirements"] == 0
    assert summary["eligible_for_production_review"] is False

    failed = record(
        "ev-20260806-failed-rotation",
        "legacy_credential_rotation",
        "failed",
        "none",
    )
    summary = build_summary([failed])
    assert summary["counts"]["active_failed"] == 1
    assert summary["counts"]["satisfied_requirements"] == 0
    assert summary["eligible_for_production_review"] is False


def assert_wrong_environment() -> None:
    staging = record(
        "ev-20260806-staging-cloudflare",
        "cloudflare_environment",
        "passed",
        "staging",
    )
    summary = build_summary([staging])
    assert summary["counts"]["active_passed"] == 1
    assert summary["counts"]["satisfied_requirements"] == 0
    assert {
        "environment": "production",
        "gate": "cloudflare_environment",
    } in summary["missing_requirements"]


def assert_superseded_leaf_only() -> None:
    root = record(
        "ev-20260806-license-pending-root",
        "product_license",
        "pending",
        "none",
        "2026-08-06T13:00:00Z",
    )
    leaf = record(
        "ev-20260806-license-passed-leaf",
        "product_license",
        "passed",
        "none",
        "2026-08-06T14:00:00Z",
        root.evidence_id,
    )
    summary = build_summary([leaf, root])
    assert summary["counts"]["total_records"] == 2
    assert summary["counts"]["active_pending"] == 0
    assert summary["counts"]["active_passed"] == 1
    assert summary["counts"]["satisfied_requirements"] == 1
    assert summary["active_records"] == [
        {
            "environment": "none",
            "evidence_id": leaf.evidence_id,
            "gate": "product_license",
            "observed_date": "2026-08-06",
            "status": "passed",
        }
    ]


def eligible_records() -> list[object]:
    return [
        record(
            f"ev-20260806-eligible-{gate.replace('_', '-')}",
            gate,
            "passed",
            environment,
        )
        for gate, environment in MANDATORY_REQUIREMENTS
    ]


def assert_full_eligibility_is_deterministic_and_private() -> None:
    records = eligible_records()
    summary = build_summary(records)
    reversed_summary = build_summary(list(reversed(records)))
    assert summary == reversed_summary
    assert summary["eligible_for_production_review"] is True
    assert summary["counts"]["active_passed"] == 12
    assert summary["counts"]["satisfied_requirements"] == 12
    assert summary["missing_requirements"] == []

    rendered = json.dumps(summary, sort_keys=True)
    for prohibited in (
        "synthetic-subject-id",
        "review-report:sha256:",
        "github_login",
        "artifact_digests_sha256",
        "checks",
        "references",
    ):
        assert prohibited not in rendered


def assert_duplicate_active_gate_fails_closed() -> None:
    first = record(
        "ev-20260806-license-one",
        "product_license",
        "passed",
        "none",
    )
    second = record(
        "ev-20260806-license-two",
        "product_license",
        "passed",
        "none",
    )
    try:
        build_summary([first, second])
    except SummaryError:
        return
    raise AssertionError("multiple active records for one gate unexpectedly passed")


def assert_production_interlock() -> None:
    with tempfile.TemporaryDirectory() as temp_dir:
        root = Path(temp_dir)
        blocked = root / "blocked.json"
        blocked.write_text('{"production_ready": true}\n', encoding="utf-8")
        try:
            validate_status(blocked, eligible=False)
        except SummaryError:
            pass
        else:
            raise AssertionError("production_ready=true unexpectedly passed without eligibility")

        manual_review_pending = root / "manual-review-pending.json"
        manual_review_pending.write_text('{"production_ready": false}\n', encoding="utf-8")
        validate_status(manual_review_pending, eligible=True)


def main() -> int:
    assert_empty()
    assert_pending_and_failed()
    assert_wrong_environment()
    assert_superseded_leaf_only()
    assert_full_eligibility_is_deterministic_and_private()
    assert_duplicate_active_gate_fails_closed()
    assert_production_interlock()
    print("external readiness synthetic fixtures passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
