#!/usr/bin/env python3
"""Prove the real architecture inventory checker rejects deterministic drift."""

from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "generate-architecture-inventory.py"
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


def assert_lifecycle_projection_sync(module: ModuleType, expected: dict[str, object]) -> None:
    derived = module.derive_lifecycle_state()
    accepted = derived["accepted_checkpoint"]
    current = derived["current_slice"]

    delivery = expected.get("current_delivery_map")
    if not isinstance(delivery, dict):
        raise AssertionError("inventory lost current_delivery_map projection")
    if delivery.get("accepted_checkpoint") != accepted:
        raise AssertionError("inventory accepted_checkpoint diverged from canonical Git-derived state")
    if delivery.get("current_work") != current:
        raise AssertionError("inventory current_work diverged from canonical Git-derived state")
    accepted_on_main = delivery.get("accepted_on_main")
    if not isinstance(accepted_on_main, dict) or accepted_on_main.get("through") != accepted:
        raise AssertionError("inventory accepted_on_main projection diverged from canonical Git-derived state")

    status = json.loads(STATUS.read_text(encoding="utf-8"))
    status_current = status.get("current")
    status_program = status_current.get("architecture_program") if isinstance(status_current, dict) else None
    if not isinstance(status_program, dict):
        raise AssertionError("docs/status.json lost architecture_program projection")
    accepted_slices = status_program.get("accepted_slices")
    if not isinstance(accepted_slices, list) or not accepted_slices or accepted_slices[-1] != accepted:
        raise AssertionError("docs/status.json accepted_slices diverged from canonical Git-derived state")
    if status_program.get("current_slice") != current:
        raise AssertionError("docs/status.json current_slice diverged from canonical Git-derived state")
    if status_current.get("current_delivery_map") != delivery:
        raise AssertionError("docs/status.json current_delivery_map diverged from generated inventory")

    transition = json.loads(TRANSITION.read_text(encoding="utf-8"))
    transition_slices = transition.get("accepted_slices")
    if not isinstance(transition_slices, list) or not transition_slices or transition_slices[-1] != accepted:
        raise AssertionError("transition accepted_slices diverged from canonical Git-derived state")
    if transition.get("current_slice") != current:
        raise AssertionError("transition current_slice diverged from canonical Git-derived state")
    if transition.get("current_delivery_map") != delivery:
        raise AssertionError("transition current_delivery_map diverged from generated inventory")

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


def main() -> int:
    module = load_generator()
    expected = module.build_inventory()
    assert_lifecycle_projection_sync(module, expected)

    d1_evolution = expected.get("d1_evolution")
    if not isinstance(d1_evolution, dict):
        raise AssertionError("AR-9 generator lost architecture/inventory.json::d1_evolution")
    if d1_evolution.get("source_authority") != "architecture/d1-evolution-ar9.json":
        raise AssertionError("AR-9 D1 projection lost its single source authority")

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
        "Architecture inventory checker rejects stale, tampered, missing and D1-projection drift; "
        "lifecycle projections match canonical Git-derived state without monkey-patching."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
