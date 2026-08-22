#!/usr/bin/env python3
"""Prove architecture inventory drift and lifecycle projection boundaries."""

from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "generate-architecture-inventory.py"
INVENTORY = ROOT / "architecture" / "inventory.json"
STATUS = ROOT / "docs" / "status.json"
TRANSITION = ROOT / "architecture" / "architecture-rebaseline-v3-transition.json"


def load_generator() -> ModuleType:
    spec = importlib.util.spec_from_file_location("architecture_inventory_generator", GENERATOR)
    if spec is None or spec.loader is None:
        raise SystemExit("could not load architecture inventory generator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def expect_rejected(module: ModuleType, expected: dict[str, object], tampered: dict[str, object]) -> None:
    original_path = module.INVENTORY_PATH
    with tempfile.TemporaryDirectory() as temporary_directory:
        path = Path(temporary_directory) / "inventory.json"
        path.write_text(json.dumps(tampered, indent=2) + "\n", encoding="utf-8", newline="\n")
        module.INVENTORY_PATH = path
        try:
            module.check_current(expected)
        except SystemExit:
            pass
        else:
            raise AssertionError("tampered architecture inventory unexpectedly passed --check")
        finally:
            module.INVENTORY_PATH = original_path


def expect_lifecycle_snapshot_staleness_tolerated(
    module: ModuleType, expected: dict[str, object], actual: dict[str, object]
) -> None:
    stale = copy.deepcopy(actual)
    delivery = stale.get("current_delivery_map")
    if not isinstance(delivery, dict):
        raise AssertionError("tracked inventory lost current_delivery_map")
    delivery["accepted_checkpoint"] = "STALE_NON_AUTHORITATIVE_FIXTURE"
    delivery["current_work"] = "STALE_NON_AUTHORITATIVE_FIXTURE"
    original_path = module.INVENTORY_PATH
    with tempfile.TemporaryDirectory() as temporary_directory:
        path = Path(temporary_directory) / "inventory.json"
        path.write_text(json.dumps(stale, indent=2) + "\n", encoding="utf-8", newline="\n")
        module.INVENTORY_PATH = path
        try:
            module.check_current(expected)
        finally:
            module.INVENTORY_PATH = original_path


def assert_lifecycle_projection_boundaries(module: ModuleType, expected: dict[str, object]) -> None:
    derived = module.derive_lifecycle_state()
    accepted = derived["accepted_checkpoint"]
    current = derived["current_slice"]

    generated_delivery = expected.get("current_delivery_map")
    if not isinstance(generated_delivery, dict):
        raise AssertionError("generated inventory lost current_delivery_map projection")
    if generated_delivery.get("accepted_checkpoint") != accepted:
        raise AssertionError("generated accepted_checkpoint diverged from canonical Git-derived state")
    if generated_delivery.get("current_work") != current:
        raise AssertionError("generated current_work diverged from canonical Git-derived state")
    if generated_delivery.get("projection_role") != "NON_AUTHORITATIVE_LIFECYCLE_COMPATIBILITY_SNAPSHOT":
        raise AssertionError("generated lifecycle projection lost explicit non-authoritative role")
    if generated_delivery.get("lifecycle_authority") != f"{module.ACCEPTANCE_DERIVER} derive":
        raise AssertionError("generated lifecycle projection lost canonical lifecycle authority")

    tracked_inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
    tracked_delivery = tracked_inventory.get("current_delivery_map")
    if not isinstance(tracked_delivery, dict):
        raise AssertionError("tracked inventory lost current_delivery_map projection")
    module.validate_tracked_snapshot_boundary(tracked_inventory)

    snapshot_accepted = tracked_delivery.get("accepted_checkpoint")
    snapshot_current = tracked_delivery.get("current_work")
    if not isinstance(snapshot_accepted, str) or not snapshot_accepted:
        raise AssertionError("tracked inventory snapshot lost accepted checkpoint")
    if not isinstance(snapshot_current, str) or not snapshot_current:
        raise AssertionError("tracked inventory snapshot lost current slice")

    status = json.loads(STATUS.read_text(encoding="utf-8"))
    status_current = status.get("current")
    status_program = status_current.get("architecture_program") if isinstance(status_current, dict) else None
    if not isinstance(status_program, dict):
        raise AssertionError("docs/status.json lost architecture_program projection")
    accepted_slices = status_program.get("accepted_slices")
    if (
        not isinstance(accepted_slices, list)
        or not accepted_slices
        or accepted_slices[-1] != snapshot_accepted
        or status_program.get("current_slice") != snapshot_current
    ):
        raise AssertionError("docs/status.json lifecycle snapshot diverged from tracked inventory")
    if status_current.get("current_delivery_map") != tracked_delivery:
        raise AssertionError("docs/status.json current_delivery_map diverged from tracked inventory")

    current_next = status_current.get("next_repository_step")
    if (
        not isinstance(current_next, dict)
        or current_next.get("previous_acceptance_checkpoint") != snapshot_accepted
        or current_next.get("projection_role") != "NON_AUTHORITATIVE_LIFECYCLE_COMPATIBILITY_SNAPSHOT"
    ):
        raise AssertionError("docs/status.json current next_repository_step lost lifecycle projection semantics")

    historical_next = status.get("next_repository_step")
    historical_note = status.get("historical_status_note")
    if (
        not isinstance(historical_next, dict)
        or historical_next.get("historical") is not True
        or historical_next.get("scope") != "historical_repository_steps_0_10"
        or historical_next.get("forward_execution_authority") is not False
        or not isinstance(historical_note, str)
        or "next_repository_step" not in historical_note
    ):
        raise AssertionError("legacy root next_repository_step is ambiguous with current lifecycle projection")

    transition = json.loads(TRANSITION.read_text(encoding="utf-8"))
    transition_slices = transition.get("accepted_slices")
    if (
        not isinstance(transition_slices, list)
        or not transition_slices
        or transition_slices[-1] != snapshot_accepted
        or transition.get("current_slice") != snapshot_current
    ):
        raise AssertionError("transition lifecycle snapshot diverged from tracked inventory")
    if transition.get("current_delivery_map") != tracked_delivery:
        raise AssertionError("transition current_delivery_map diverged from tracked inventory")

    policy = module.load_json(module.LIFECYCLE_PROJECTION_POLICY)
    snapshots = policy.get("tracked_compatibility_snapshots")
    registered = {
        item.get("path")
        for item in snapshots
        if isinstance(item, dict)
        and item.get("classification") == "TRANSITION_PROVENANCE_ONLY_FOR_LIFECYCLE_STATE"
    } if isinstance(snapshots, list) else set()
    if registered != {
        "architecture/inventory.json",
        "docs/status.json",
        "architecture/architecture-rebaseline-v3-transition.json",
    }:
        raise AssertionError("tracked lifecycle snapshot classification registry drifted")
    consumers = policy.get("consumer_policy")
    updates = policy.get("projection_update_rule")
    if (
        not isinstance(consumers, dict)
        or consumers.get("tracked_snapshot_may_decide_accepted_or_current_slice") is not False
        or consumers.get("future_acceptance_requires_source_projection_commit") is not False
        or not isinstance(updates, dict)
        or updates.get("ordinary_checks_must_not_require_snapshot_equal_live_state") is not True
        or updates.get("post_acceptance_projection_source_commit_required") is not False
    ):
        raise AssertionError("lifecycle projection non-authority/update policy drifted")

    source = GENERATOR.read_text(encoding="utf-8")
    forbidden_current_overlays = (
        "engine.CURRENT_SLICE =",
        "engine.NEXT_SLICE =",
        "engine.CURRENT_DELIVERY_CHECKPOINT =",
        "engine.ACCEPTED_SLICES =",
    )
    for forbidden in forbidden_current_overlays:
        if forbidden in source:
            raise AssertionError(f"current inventory path retains lifecycle monkey-patching: {forbidden}")

    expect_lifecycle_snapshot_staleness_tolerated(module, expected, tracked_inventory)


def main() -> int:
    module = load_generator()
    expected = module.build_inventory()
    assert_lifecycle_projection_boundaries(module, expected)

    d1_evolution = expected.get("d1_evolution")
    if not isinstance(d1_evolution, dict):
        raise AssertionError("AR-9 generator lost architecture/inventory.json::d1_evolution")
    if d1_evolution.get("semantic_authority") != "tools/opsctl/src/d1":
        raise AssertionError("D1 projection lost its typed semantic authority")
    if d1_evolution.get("executable_schema_authority") != [
        "migrations/d1",
        "migrations/resolver-d1",
    ]:
        raise AssertionError("D1 projection lost its executable SQL authority")

    workspace_drift = copy.deepcopy(expected)
    workspace_drift["workspace_members"] = [
        *workspace_drift["workspace_members"],
        "crates/does-not-exist",
    ]
    expect_rejected(module, expected, workspace_drift)

    route_drift = copy.deepcopy(expected)
    route_drift["routing"]["public_routes"][0]["route_class"] = "UnknownApi"
    expect_rejected(module, expected, route_drift)

    missing_d1_projection = copy.deepcopy(expected)
    del missing_d1_projection["d1_evolution"]
    expect_rejected(module, expected, missing_d1_projection)

    tampered_d1_projection = copy.deepcopy(expected)
    tampered_d1_projection["d1_evolution"]["components"][0]["history_digest"] = "0" * 64
    expect_rejected(module, expected, tampered_d1_projection)

    original_path = module.INVENTORY_PATH
    missing_path = ROOT / ".architecture-inventory-negative-missing.json"
    if missing_path.exists():
        raise AssertionError(f"negative fixture path unexpectedly exists: {missing_path}")
    module.INVENTORY_PATH = missing_path
    try:
        module.check_current(expected)
    except SystemExit:
        pass
    else:
        raise AssertionError("missing architecture inventory unexpectedly passed --check")
    finally:
        module.INVENTORY_PATH = original_path

    print(
        "Architecture inventory rejects stable/domain drift, derives live lifecycle state only from "
        "canonical Git authority, and tolerates non-authoritative snapshot staleness without a second source merge."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
