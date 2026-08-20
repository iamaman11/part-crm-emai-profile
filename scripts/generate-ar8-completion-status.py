#!/usr/bin/env python3
"""Validate and explicitly refresh non-authoritative lifecycle snapshots.

Live architecture lifecycle state belongs only to
`.github/scripts/architecture-acceptance.mjs derive`. Normal checks intentionally tolerate stale
accepted/current snapshot fields so an acceptance tag never requires a second source merge.
`--write` is an explicit projection refresh, not an acceptance event.
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
ACCEPTANCE_POLICY = ROOT / "architecture/architecture-acceptance-policy.json"
LIFECYCLE_POLICY = ROOT / "architecture/lifecycle-projection-policy.json"
SEQUENCE = ROOT / "architecture/architecture-program-sequence.json"
STATUS = ROOT / "docs/status.json"
TRANSITION = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
INVENTORY = ROOT / "architecture/inventory.json"
DERIVER = ROOT / ".github/scripts/architecture-acceptance.mjs"
INVENTORY_GENERATOR = ROOT / "scripts/generate-architecture-inventory.py"
DERIVER_ID = ".github/scripts/architecture-acceptance.mjs derive"
PROJECTION_ROLE = "NON_AUTHORITATIVE_LIFECYCLE_COMPATIBILITY_SNAPSHOT"

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
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def validate_policies(
    acceptance: dict[str, Any], lifecycle: dict[str, Any], sequence: dict[str, Any]
) -> list[dict[str, Any]]:
    if (
        acceptance.get("kind") != "ARCHITECTURE_ACCEPTANCE_POLICY"
        or acceptance.get("status") != "current"
        or acceptance.get("source_branch") != "main"
        or acceptance.get("source_history_count") != 1
    ):
        fail("architecture acceptance single-history boundary drifted")
    projection = acceptance.get("projection_policy")
    if not isinstance(projection, dict) or projection.get("authoritative") is not False:
        fail("tracked lifecycle projections became authoritative")

    live = lifecycle.get("live_state_authority")
    update = lifecycle.get("projection_update_rule")
    consumers = lifecycle.get("consumer_policy")
    if (
        lifecycle.get("kind") != "LIFECYCLE_PROJECTION_POLICY"
        or lifecycle.get("status") != "current"
        or not isinstance(live, dict)
        or live.get("deriver") != DERIVER_ID
        or live.get("tracked_mutable_lifecycle_state") is not False
        or not isinstance(update, dict)
        or update.get("explicit_sync_source") != DERIVER_ID
        or update.get("explicit_projection_write_is_acceptance_event") is not False
        or update.get("ordinary_checks_must_not_require_snapshot_equal_live_state") is not True
        or update.get("post_acceptance_projection_source_commit_required") is not False
        or not isinstance(consumers, dict)
        or consumers.get("tracked_snapshot_may_decide_accepted_or_current_slice") is not False
        or consumers.get("duplicate_lifecycle_derivation_algorithm_forbidden") is not True
    ):
        fail("lifecycle projection one-authority/stale-safe policy drifted")

    if (
        sequence.get("kind") != "ARCHITECTURE_PROGRAM_SEQUENCE"
        or sequence.get("state_model") != "STATIC_ORDER_ONLY"
        or sequence.get("mutable_lifecycle_state_forbidden") is not True
    ):
        fail("architecture program sequence is not static-order-only")
    slices = sequence.get("slices")
    if not isinstance(slices, list) or not slices or any(not isinstance(item, dict) for item in slices):
        fail("architecture program sequence is malformed")
    for item in slices:
        for forbidden in ("accepted", "current", "accepted_checkpoint", "current_slice"):
            if forbidden in item:
                fail(f"static architecture sequence stores mutable lifecycle field {forbidden}")
    return slices


def derive() -> dict[str, Any]:
    result = subprocess.run(
        ["node", str(DERIVER), "derive"], cwd=ROOT, text=True, capture_output=True, check=False
    )
    if result.returncode != 0:
        details = "\n".join(part.strip() for part in (result.stdout, result.stderr) if part.strip())
        fail(details or "canonical lifecycle derive failed")
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ProjectionError("canonical lifecycle derive returned malformed JSON") from error
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        fail("canonical lifecycle derive returned invalid schema")
    accepted = value.get("accepted_checkpoint")
    current = value.get("current_slice")
    if not isinstance(accepted, str) or not accepted:
        fail("canonical lifecycle derive lost accepted_checkpoint")
    if current is not None and (not isinstance(current, str) or not current):
        fail("canonical lifecycle derive returned invalid current_slice")
    return value


def ordered_state(
    derived: dict[str, Any], slices: list[dict[str, Any]]
) -> tuple[list[str], dict[str, Any] | None, str | None]:
    accepted = derived["accepted_checkpoint"]
    index = next((index for index, item in enumerate(slices) if item.get("id") == accepted), None)
    if index is None:
        fail(f"accepted checkpoint absent from static sequence: {accepted}")
    current = derived.get("current_slice")
    expected_current = slices[index].get("successor")
    if current != expected_current:
        fail(f"canonical current slice disagrees with static successor: {current!r} != {expected_current!r}")
    current_entry = next((item for item in slices if item.get("id") == current), None)
    next_slice = current_entry.get("successor") if current_entry is not None else None
    return [str(item["id"]) for item in slices[: index + 1]], current_entry, next_slice


def validate_history(status: dict[str, Any], transition: dict[str, Any]) -> None:
    current = status.get("current")
    program = current.get("architecture_program") if isinstance(current, dict) else None
    if not isinstance(program, dict):
        fail("docs/status.json architecture_program snapshot is missing")
    for key, expected in HISTORICAL_ACCEPTANCE.items():
        left = program.get(key)
        right = transition.get(key)
        if not isinstance(left, dict) or left != right:
            fail(f"immutable historical provenance diverged: {key}")
        for field, wanted in expected.items():
            if left.get(field) != wanted:
                fail(f"immutable historical provenance drifted: {key}.{field}")
        if left.get("metadata_only") is not True or left.get("production_mutation") is not False:
            fail(f"historical acceptance lost metadata-only boundary: {key}")


def validate_fail_closed(status: dict[str, Any], transition: dict[str, Any]) -> None:
    current = status.get("current")
    if not isinstance(current, dict) or status.get("production_ready") is not False:
        fail("docs/status.json may not enable production")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        fail("docs/status.json lifecycle snapshot is not fail-closed")
    state_model = transition.get("state_model")
    if not isinstance(state_model, dict):
        fail("transition state_model is missing")
    if (
        state_model.get("architecture_complete") is not False
        or state_model.get("production_core_gate") != "BLOCKED"
        or state_model.get("production_ready") is not False
    ):
        fail("transition lifecycle snapshot is not fail-closed")
    status_delivery = current.get("current_delivery_map")
    transition_delivery = transition.get("current_delivery_map")
    if not isinstance(status_delivery, dict) or status_delivery != transition_delivery:
        fail("status/transition current_delivery_map snapshots diverged")
    invariants = status_delivery.get("invariants")
    if not isinstance(invariants, dict):
        fail("current_delivery_map invariants are missing")
    for key, wanted in {
        "source_present_not_equal_production_enabled": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if invariants.get(key) != wanted:
            fail(f"current_delivery_map is not fail-closed: {key}")


def validate_all(
    status: dict[str, Any], transition: dict[str, Any], acceptance: dict[str, Any],
    lifecycle: dict[str, Any], sequence: dict[str, Any]
) -> None:
    validate_policies(acceptance, lifecycle, sequence)
    validate_history(status, transition)
    validate_fail_closed(status, transition)


def current_projection(current_slice: str | None, current_name: str | None) -> dict[str, Any]:
    return {
        "slice": current_slice,
        "name": current_name,
        "status": "NOT_STARTED",
        "lifecycle_authority": DERIVER_ID,
        "projection_role": PROJECTION_ROLE,
        "production_mutation": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
    }


def explicit_sync() -> None:
    acceptance = load_json(ACCEPTANCE_POLICY)
    lifecycle = load_json(LIFECYCLE_POLICY)
    sequence = load_json(SEQUENCE)
    slices = validate_policies(acceptance, lifecycle, sequence)
    derived = derive()
    accepted_slices, current_entry, next_slice = ordered_state(derived, slices)
    if (
        derived.get("architecture_complete") is not False
        or derived.get("production_core_gate") != "BLOCKED"
        or derived.get("production_ready") is not False
        or derived.get("production_mutation") is not False
    ):
        fail("pre-AR-12 projection sync refuses non-fail-closed canonical lifecycle state")

    generated = subprocess.run(
        [sys.executable, str(INVENTORY_GENERATOR), "--write"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if generated.returncode != 0:
        details = "\n".join(part.strip() for part in (generated.stdout, generated.stderr) if part.strip())
        fail(details or "inventory projection write failed")
    inventory = load_json(INVENTORY)
    delivery = inventory.get("current_delivery_map")
    if not isinstance(delivery, dict):
        fail("generated inventory lost current_delivery_map")

    status = load_json(STATUS)
    transition = load_json(TRANSITION)
    current = status.get("current")
    program = current.get("architecture_program") if isinstance(current, dict) else None
    if not isinstance(current, dict) or not isinstance(program, dict):
        fail("docs/status.json current architecture projection is missing")

    current_slice = derived.get("current_slice")
    current_name = current_entry.get("name") if current_entry is not None else None
    projection = current_projection(current_slice, current_name)

    program["accepted_slices"] = accepted_slices
    program["current_slice"] = current_slice
    program["next_slice_after_acceptance"] = next_slice
    program.pop("ar11_current", None)
    program["current_slice_projection"] = copy.deepcopy(projection)
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
        "lifecycle_authority": DERIVER_ID,
        "projection_role": PROJECTION_ROLE,
    }
    implementation = status.get("implementation")
    if isinstance(implementation, dict):
        accepted_id = derived["accepted_checkpoint"].lower().replace("-", "")
        current_id = str(current_slice).lower().replace("-", "") if current_slice else "complete"
        implementation["architecture_rebaseline_v3"] = f"active_issue_266_{accepted_id}_accepted_{current_id}_current"
    status["production_ready"] = False

    transition["status"] = f"ACTIVE_AFTER_{derived['accepted_checkpoint'].replace('-', '')}_ACCEPTANCE"
    transition["accepted_slices"] = accepted_slices
    transition["current_slice"] = current_slice
    transition["next_slice_after_acceptance"] = next_slice
    transition["current_delivery_map"] = copy.deepcopy(delivery)
    transition.pop("ar11_current", None)
    transition["current_slice_projection"] = copy.deepcopy(projection)
    transition_state = transition.get("state_model")
    if not isinstance(transition_state, dict):
        fail("transition state_model is missing")
    transition_state["architecture_complete"] = False
    transition_state["production_core_gate"] = "BLOCKED"
    transition_state["production_ready"] = False
    application = transition.get("application_architecture")
    if isinstance(application, dict):
        accepted_id = derived["accepted_checkpoint"].replace("-", "")
        current_id = str(current_slice).replace("-", "") if current_slice else "COMPLETE"
        application["program_handoff_status"] = f"{accepted_id}_ACCEPTED_{current_id}_NOT_STARTED"
        application["program_next_required_subslice"] = current_slice

    write_json(STATUS, status)
    write_json(TRANSITION, transition)
    validate_all(status, transition, acceptance, lifecycle, sequence)
    print(
        f"Synchronized non-authoritative lifecycle projections: {derived['accepted_checkpoint']} accepted, "
        f"{current_slice} current, production blocked."
    )


def self_test(
    status: dict[str, Any], transition: dict[str, Any], acceptance: dict[str, Any],
    lifecycle: dict[str, Any], sequence: dict[str, Any]
) -> None:
    validate_all(status, transition, acceptance, lifecycle, sequence)

    stale_status = copy.deepcopy(status)
    stale_transition = copy.deepcopy(transition)
    stale_status["current"]["architecture_program"]["current_slice"] = "AR-0"
    stale_transition["current_slice"] = "AR-0"
    validate_all(stale_status, stale_transition, acceptance, lifecycle, sequence)

    bad = copy.deepcopy(status)
    bad["production_ready"] = True
    try:
        validate_all(bad, transition, acceptance, lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("premature production-ready negative fixture unexpectedly passed")

    bad = copy.deepcopy(transition)
    bad["state_model"]["production_core_gate"] = "AUTHORIZED"
    try:
        validate_all(status, bad, acceptance, lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("premature production authorization negative fixture unexpectedly passed")

    bad = copy.deepcopy(status)
    bad["current"]["architecture_program"]["ar10_acceptance"]["implementation_merge"] = "0" * 40
    try:
        validate_all(bad, transition, acceptance, lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("historical acceptance mutation negative fixture unexpectedly passed")

    bad_lifecycle = copy.deepcopy(lifecycle)
    bad_lifecycle["projection_update_rule"]["post_acceptance_projection_source_commit_required"] = True
    try:
        validate_all(status, transition, acceptance, bad_lifecycle, sequence)
    except ProjectionError:
        pass
    else:
        fail("second projection source-commit requirement unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.write:
        explicit_sync()
        return 0

    status = load_json(STATUS)
    transition = load_json(TRANSITION)
    acceptance = load_json(ACCEPTANCE_POLICY)
    lifecycle = load_json(LIFECYCLE_POLICY)
    sequence = load_json(SEQUENCE)
    if args.self_test:
        self_test(status, transition, acceptance, lifecycle, sequence)
        print("Lifecycle projection negative matrix passed, including stale-snapshot tolerance.")
    else:
        validate_all(status, transition, acceptance, lifecycle, sequence)
        print("Lifecycle snapshots are fail-closed; live accepted/current state remains Git-derived.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ProjectionError as error:
        print(f"lifecycle projection check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
