#!/usr/bin/env python3
"""Validate and explicitly synchronize non-authoritative lifecycle projections.

Live accepted/current architecture state is owned exclusively by the generic Git acceptance
machinery (`.github/scripts/architecture-acceptance.mjs derive`) plus the static program sequence
and acceptance policy.

Normal checks deliberately do NOT require tracked snapshot accepted/current values to equal live
Git-derived state. That keeps future append-only acceptance metadata sufficient: accepting a slice
must never require a second source commit merely to refresh projections.

`--write` is an explicit, deterministic projection refresh. It consumes canonical derive output,
delegates inventory generation to the current inventory generator, and refreshes only registered
compatibility/status fields. It is not an acceptance event and never writes Git metadata.
"""

from __future__ import annotations

import argparse
import copy
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "architecture/architecture-acceptance-policy.json"
LIFECYCLE_POLICY = ROOT / "architecture/lifecycle-projection-policy.json"
SEQUENCE = ROOT / "architecture/architecture-program-sequence.json"
STATUS = ROOT / "docs/status.json"
TRANSITION = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
INVENTORY = ROOT / "architecture/inventory.json"
ACCEPTANCE_DERIVER = ROOT / ".github/scripts/architecture-acceptance.mjs"
INVENTORY_GENERATOR = ROOT / "scripts/generate-architecture-inventory.py"

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


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def validate_static_authority(policy: dict[str, Any], sequence: dict[str, Any]) -> list[dict[str, Any]]:
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
    if not isinstance(slices, list) or not slices or any(not isinstance(item, dict) for item in slices):
        fail("architecture program sequence must contain slice objects")
    for item in slices:
        for forbidden in ("accepted", "current", "current_slice", "accepted_checkpoint"):
            if forbidden in item:
                fail(f"static architecture program sequence stores mutable lifecycle field {forbidden}")
    return slices


def validate_lifecycle_projection_policy(lifecycle: dict[str, Any]) -> None:
    live = lifecycle.get("live_state_authority")
    update = lifecycle.get("projection_update_rule")
    consumer = lifecycle.get("consumer_policy")
    if (
        lifecycle.get("schema_version") != 1
        or lifecycle.get("kind") != "LIFECYCLE_PROJECTION_POLICY"
        or lifecycle.get("status") != "current"
        or not isinstance(live, dict)
        or live.get("deriver") != ".github/scripts/architecture-acceptance.mjs derive"
        or live.get("tracked_mutable_lifecycle_state") is not False
        or not isinstance(update, dict)
        or update.get("explicit_sync_source") != ".github/scripts/architecture-acceptance.mjs derive"
        or update.get("explicit_projection_write_is_acceptance_event") is not False
        or update.get("ordinary_checks_must_not_require_snapshot_equal_live_state") is not True
        or update.get("post_acceptance_projection_source_commit_required") is not False
        or not isinstance(consumer, dict)
        or consumer.get("tracked_snapshot_may_decide_accepted_or_current_slice") is not False
        or consumer.get("duplicate_lifecycle_derivation_algorithm_forbidden") is not True
    ):
        fail("lifecycle projection policy lost one-authority / stale-safe boundary")


def derive_lifecycle_state() -> dict[str, Any]:
    result = subprocess.run(
        ["node", str(ACCEPTANCE_DERIVER), "derive"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        details = "\n".join(part.strip() for part in (result.stdout, result.stderr) if part.strip())
        fail(details or "canonical Git-derived lifecycle command failed")
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        fail("canonical Git-derived lifecycle command returned malformed JSON") from error
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        fail("canonical Git-derived lifecycle command returned invalid schema")
    accepted = value.get("accepted_checkpoint")
    current = value.get("current_slice")
    if not isinstance(accepted, str) or not accepted:
        fail("canonical Git-derived lifecycle state is missing accepted_checkpoint")
    if current is not None and (not isinstance(current, str) or not current):
        fail("canonical Git-derived lifecycle state has invalid current_slice")
    for key in ("architecture_complete", "production_ready", "production_mutation"):
        if not isinstance(value.get(key), bool):
            fail(f"canonical Git-derived lifecycle state has invalid {key}")
    if value.get("production_core_gate") not in {"BLOCKED", "AUTHORIZED"}:
        fail("canonical Git-derived lifecycle state has invalid production_core_gate")
    return value


def projection_order(
    derived: dict[str, Any], slices: list[dict[str, Any]]
) -> tuple[list[str], dict[str, Any] | None, str | None]:
    accepted = derived["accepted_checkpoint"]
    accepted_index = next((i for i, item in enumerate(slices) if item.get("id") == accepted), None)
    if accepted_index is None:
        fail(f"canonical accepted checkpoint is absent from static sequence: {accepted}")
    accepted_slices = [str(item["id"]) for item in slices[: accepted_index + 1]]
    current = derived.get("current_slice")
    current_entry = next((item for item in slices if item.get("id") == current), None)
    if current is not None and current_entry is None:
        fail(f"canonical current slice is absent from static sequence: {current}")
    expected_current = slices[accepted_index].get("successor")
    if current != expected_current:
        fail(f"canonical current slice disagrees with static successor: {current!r} != {expected_current!r}")
    next_slice = current_entry.get("successor") if current_entry is not None else None
    return accepted_slices, current_entry, next_slice


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


def validate(
    status: dict[str, Any],
    transition: dict[str, Any],
    policy: dict[str, Any],
    lifecycle: dict[str, Any],
    sequence: dict[str, Any],
) -> None:
    validate_static_authority(policy, sequence)
    validate_lifecycle_projection_policy(lifecycle)
    validate_historical_acceptance(status, transition)
    validate_fail_closed(status, transition)


def sync() -> None:
    policy = load_json(POLICY)
    lifecycle = load_json(LIFECYCLE_POLICY)
    sequence = load_json(SEQUENCE)
    slices = validate_static_authority(policy, sequence)
    validate_lifecycle_projection_policy(lifecycle)
    derived = derive_lifecycle_state()
    accepted_slices, current_entry, next_slice = projection_order(derived, slices)

    if (
        derived.get("architecture_complete") is not False
        or derived.get("production_core_gate") != "BLOCKED"
        or derived.get("production_ready") is not False
        or derived.get("production_mutation") is not False
    ):
        fail("pre-AR-12 explicit projection sync refuses non-fail-closed canonical lifecycle state")

    inventory_write = subprocess.run(
        [sys.executable, str(INVENTORY_GENERATOR), "--write"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if inventory_write.returncode != 0:
        details = "\n".join(
            part.strip() for part in (inventory_write.stdout, inventory_write.stderr) if part.strip()
        )
        fail(details or "architecture inventory explicit projection write failed")
    inventory = load_json(INVENTORY)
    delivery = inventory.get("current_delivery_map")
    if not isinstance(delivery, dict):
        fail("generated architecture inventory lost current_delivery_map")

    status = load_json(STATUS)
    transition = load_json(TRANSITION)
    current = status.get("current")
    program = current.get("architecture_program") if isinstance(current, dict) else None
    if not isinstance(current, dict) or not isinstance(program, dict):
        fail("docs/status.json current architecture program projection is missing")

    current_slice = derived.get("current_slice")
    current_name = current_entry.get("name") if current_entry is not None else None
    current_projection = {
        "slice": current_slice,
        "name": current_name,
        "status": "NOT_STARTED",
        "lifecycle_authority": ".github/scripts/architecture-acceptance.mjs derive",
        "projection_role": "NON_AUTHORITATIVE_LIFECYCLE_COMPATIBILITY_SNAPSHOT",
        "production_mutation": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
    }

    program["accepted_slices"] = accepted_slices
    program["current_slice"] = current_slice
    program["next_slice_after_acceptance"] = next_slice
    program.pop("ar11_current", None)
    program["current_slice_projection"] = current_projection
    current["current_delivery_map"] = copy.deepcopy(delivery)
    current["architecture_complete"] = False
    current["production_core_gate"] = "BLOCKED"
    current["next_repository_step"] = {
        "name": f"{current_slice} — {current_name}" if current_slice and current_name else "Architecture program complete",
        "status": "not_started" if current_slice else "none",
        "tracking_issue": None,
        "program_tracking_issue": 266,
        "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "previous_acceptance_checkpoint": derived["accepted_checkpoint"],
        "lifecycle_authority": ".github/scripts/architecture-acceptance.mjs derive",
        "projection_role": "NON_AUTHORITATIVE_LIFECYCLE_COMPATIBILITY_SNAPSHOT",
    }
    status["production_ready"] = False
    implementation = status.get("implementation")
    if isinstance(implementation, dict):
        implementation["architecture_rebaseline_v3"] = (
            f"active_issue_266_{derived['accepted_checkpoint'].lower().replace('-', '')}_accepted_"
            f"{str(current_slice).lower().replace('-', '')}_current"
            if current_slice
            else f"complete_issue_266_{derived['accepted_checkpoint'].lower().replace('-', '')}_accepted"
        )

    transition["status"] = f"ACTIVE_AFTER_{derived['accepted_checkpoint'].replace('-', '')}_ACCEPTANCE"
    transition["accepted_slices"] = accepted_slices
    transition["current_slice"] = current_slice
    transition["next_slice_after_acceptance"] = next_slice
    transition["current_delivery_map"] = copy.deepcopy(delivery)
    state_model = transition.get("state_model")
    if not isinstance(state_model, dict):
        fail("transition state_model is missing")
    state_model["architecture_complete"] = False
    state_model["production_core_gate"] = "BLOCKED"
    state_model["production_ready"] = False

    write_json(STATUS, status)
    write_json(TRANSITION, transition)
    validate(status, transition, policy, lifecycle, sequence)
    print(
        f"Synchronized non-authoritative lifecycle projections: "
        f"{derived['accepted_checkpoint']} accepted, {current_slice} current, production blocked."
    )


def self_test(
    status: dict[str, Any],
    transition: dict[str, Any],
    policy: dict[str, Any],
    lifecycle: dict[str, Any],
    sequence: dict[str, Any],
) -> None:
    validate(status, transition, policy, lifecycle, sequence)

    stale_status = copy.deepcopy(status)
    stale_transition = copy.deepcopy(transition)
    stale_status["current"]["architecture_program"]["current_slice"] = "AR-0"
    stale_transition["current_slice"] = "AR-0"
    validate(stale_status, stale_transition, policy, lifecycle, sequence)

    bad_policy = copy.deepcopy(policy)
    bad_policy["projection_policy"]["authoritative"] = True
    try:
        validate(status, transition, bad_policy, lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("authoritative tracked-projection negative fixture unexpectedly passed")

    bad_lifecycle = copy.deepcopy(lifecycle)
    bad_lifecycle["projection_update_rule"]["post_acceptance_projection_source_commit_required"] = True
    try:
        validate(status, transition, policy, bad_lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("second source-commit requirement negative fixture unexpectedly passed")

    bad_status = copy.deepcopy(status)
    bad_status["production_ready"] = True
    try:
        validate(bad_status, transition, policy, lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("premature production-ready negative fixture unexpectedly passed")

    bad_transition = copy.deepcopy(transition)
    bad_transition["state_model"]["production_core_gate"] = "AUTHORIZED"
    try:
        validate(status, bad_transition, policy, lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("premature production authorization negative fixture unexpectedly passed")

    bad_history = copy.deepcopy(status)
    bad_history["current"]["architecture_program"]["ar10_acceptance"]["implementation_merge"] = "0" * 40
    try:
        validate(bad_history, transition, policy, lifecycle, sequence)
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

    if args.write:
        sync()
        return 0

    status = load_json(STATUS)
    transition = load_json(TRANSITION)
    policy = load_json(POLICY)
    lifecycle = load_json(LIFECYCLE_POLICY)
    sequence = load_json(SEQUENCE)

    if args.self_test:
        self_test(status, transition, policy, lifecycle, sequence)
        print("Lifecycle compatibility projection negative matrix passed, including stale-snapshot tolerance.")
        return 0

    validate(status, transition, policy, lifecycle, sequence)
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
