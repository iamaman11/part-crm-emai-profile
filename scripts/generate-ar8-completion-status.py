#!/usr/bin/env python3
"""Validate tracked lifecycle compatibility projections without owning live AR state.

This path is intentionally retained because permanent checks and developer tooling already call it.
It is no longer a source-writing architecture lifecycle generator.

Live accepted/current architecture state is owned exclusively by the generic Git acceptance
machinery (`.github/scripts/architecture-acceptance.mjs`) plus the static program sequence and
acceptance policy. The tracked status/transition files are compatibility projections: this checker
validates their immutable historical provenance and fail-closed production boundaries, but it MUST
NOT compare their snapshot `accepted_slices`/`current_slice` fields with live Git-derived state.
That rule is what allows a future accepted AR tag to advance lifecycle state without a second source
commit.
"""

from __future__ import annotations

import argparse
import copy
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "architecture/architecture-acceptance-policy.json"
SEQUENCE = ROOT / "architecture/architecture-program-sequence.json"
STATUS = ROOT / "docs/status.json"
TRANSITION = ROOT / "architecture/architecture-rebaseline-v3-transition.json"

PROJECTION_PATHS = {
    "docs/status.json",
    "architecture/inventory.json",
    "architecture/architecture-rebaseline-v3-transition.json",
    "README.md",
    "docs/README.md",
    "docs/INDEX.md",
    "docs/DEVELOPMENT_PLAN.md",
    "docs/DEVELOPER_CAPABILITY_MATRIX.md",
}

HISTORICAL_ACCEPTANCE = {
    "ar8_acceptance": {
        "issue": 361,
        "implementation_pr": 362,
        "exact_green_head": "81d1f0c26ff0bd3a688c2d5dc000b93640479e47",
        "implementation_merge": "874666f6ef6eb003425c9677d558378d6dc0daaf",
        "evidence": "docs/evidence/2026-08-18-ar8-final-acceptance.json",
    },
    "ar9_acceptance": {
        "issue": 366,
        "implementation_pr": 367,
        "exact_green_head": "6110a32ade85d08c6ad93d9064190fff768e7cc2",
        "implementation_merge": "5933a5e30a534209138485556b4a895706af765a",
        "evidence": "docs/evidence/2026-08-19-ar9-final-acceptance.json",
    },
    "ar10_acceptance": {
        "issue": 368,
        "implementation_pr": 371,
        "exact_green_head": "c7f8ac9704433d3e52d3b79f985c9ac60aa068db",
        "implementation_merge": "7ab5edf583f541d08ff732624af25881d430d427",
        "evidence": "docs/evidence/2026-08-19-ar10-final-acceptance.json",
    },
}


class ProjectionError(ValueError):
    pass


def fail(message: str) -> None:
    raise ProjectionError(message)


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {path.relative_to(ROOT)}: {error}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain one JSON object")
    return value


def validate_static_authority(policy: dict[str, Any], sequence: dict[str, Any]) -> None:
    if (
        policy.get("schema_version") != 1
        or policy.get("kind") != "ARCHITECTURE_ACCEPTANCE_POLICY"
        or policy.get("status") != "current"
        or policy.get("program_sequence") != "architecture/architecture-program-sequence.json"
        or policy.get("source_branch") != "main"
        or policy.get("source_history_count") != 1
    ):
        fail("generic architecture acceptance policy identity/single-history boundary drifted")

    projection = policy.get("projection_policy")
    if not isinstance(projection, dict):
        fail("architecture acceptance policy lost projection_policy")
    if (
        projection.get("authoritative") is not False
        or projection.get("generated_or_human_projection_only") is not True
        or projection.get("stale_projection_must_not_create_acceptance_authority") is not True
        or set(projection.get("paths", [])) != PROJECTION_PATHS
    ):
        fail("tracked lifecycle projection policy drifted from non-authoritative compatibility role")

    if (
        sequence.get("schema_version") != 1
        or sequence.get("kind") != "ARCHITECTURE_PROGRAM_SEQUENCE"
        or sequence.get("state_model") != "STATIC_ORDER_ONLY"
        or sequence.get("mutable_lifecycle_state_forbidden") is not True
    ):
        fail("architecture program sequence is no longer static-order-only")
    slices = sequence.get("slices")
    if not isinstance(slices, list) or not slices:
        fail("architecture program sequence is empty")
    for item in slices:
        if not isinstance(item, dict):
            fail("architecture program sequence contains a non-object slice")
        for forbidden in ("accepted", "current", "accepted_checkpoint", "current_slice"):
            if forbidden in item:
                fail(f"static architecture program sequence stores mutable lifecycle field {forbidden}")


def validate_historical_acceptance(status: dict[str, Any], transition: dict[str, Any]) -> None:
    current = status.get("current")
    program = current.get("architecture_program") if isinstance(current, dict) else None
    if not isinstance(program, dict):
        fail("docs/status.json architecture_program compatibility projection is missing")

    for key, expected in HISTORICAL_ACCEPTANCE.items():
        left = program.get(key)
        right = transition.get(key)
        if not isinstance(left, dict) or not isinstance(right, dict):
            fail(f"immutable historical provenance {key} is missing")
        if left != right:
            fail(f"status/transition historical provenance {key} diverged")
        for field, wanted in expected.items():
            if left.get(field) != wanted:
                fail(f"immutable historical provenance {key}.{field} drifted")
        if left.get("metadata_only") is not True or left.get("production_mutation") is not False:
            fail(f"historical acceptance {key} lost metadata-only/non-production boundary")


def validate_fail_closed(status: dict[str, Any], transition: dict[str, Any]) -> None:
    if status.get("production_ready") is not False:
        fail("docs/status.json compatibility projection may not enable production")
    current = status.get("current")
    if not isinstance(current, dict):
        fail("docs/status.json current compatibility projection is missing")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        fail("docs/status.json compatibility projection must remain fail-closed")

    state_model = transition.get("state_model")
    if not isinstance(state_model, dict):
        fail("architecture transition compatibility projection lost state_model")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
    }.items():
        if state_model.get(key) != wanted:
            fail(f"architecture transition compatibility projection must remain fail-closed: {key}")

    status_delivery = current.get("current_delivery_map")
    transition_delivery = transition.get("current_delivery_map")
    if not isinstance(status_delivery, dict) or not isinstance(transition_delivery, dict):
        fail("tracked compatibility projections lost current_delivery_map")
    if status_delivery != transition_delivery:
        fail("status/transition current_delivery_map compatibility snapshots diverged")
    invariants = status_delivery.get("invariants")
    if not isinstance(invariants, dict):
        fail("current_delivery_map.invariants compatibility projection is missing")
    for key, wanted in {
        "source_present_not_equal_production_enabled": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if invariants.get(key) != wanted:
            fail(f"current_delivery_map compatibility invariant drifted: {key}")


def validate(status: dict[str, Any], transition: dict[str, Any], policy: dict[str, Any], sequence: dict[str, Any]) -> None:
    validate_static_authority(policy, sequence)
    validate_historical_acceptance(status, transition)
    validate_fail_closed(status, transition)


def self_test(status: dict[str, Any], transition: dict[str, Any], policy: dict[str, Any], sequence: dict[str, Any]) -> None:
    validate(status, transition, policy, sequence)

    bad_policy = copy.deepcopy(policy)
    bad_policy["projection_policy"]["authoritative"] = True
    try:
        validate(status, transition, bad_policy, sequence)
    except ProjectionError:
        pass
    else:
        fail("authoritative tracked-projection negative fixture unexpectedly passed")

    bad_status = copy.deepcopy(status)
    bad_status["production_ready"] = True
    try:
        validate(bad_status, transition, policy, sequence)
    except ProjectionError:
        pass
    else:
        fail("premature production-ready negative fixture unexpectedly passed")

    bad_transition = copy.deepcopy(transition)
    bad_transition["state_model"]["production_core_gate"] = "AUTHORIZED"
    try:
        validate(status, bad_transition, policy, sequence)
    except ProjectionError:
        pass
    else:
        fail("premature production authorization negative fixture unexpectedly passed")

    bad_history = copy.deepcopy(status)
    bad_history["current"]["architecture_program"]["ar10_acceptance"]["implementation_merge"] = "0" * 40
    try:
        validate(bad_history, transition, policy, sequence)
    except ProjectionError:
        pass
    else:
        fail("historical acceptance mutation negative fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    status = load_json(STATUS)
    transition = load_json(TRANSITION)
    policy = load_json(POLICY)
    sequence = load_json(SEQUENCE)

    if args.self_test:
        self_test(status, transition, policy, sequence)
        print("Lifecycle compatibility projection negative matrix passed.")
        return 0

    validate(status, transition, policy, sequence)
    if args.write:
        print(
            "No source write performed: live accepted/current architecture state is Git-derived; "
            "tracked status/transition data is compatibility-only."
        )
    else:
        print(
            "Lifecycle compatibility projections are fail-closed and historical provenance is stable; "
            "live accepted/current state remains exclusively Git-derived."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ProjectionError as error:
        print(f"lifecycle compatibility projection check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
