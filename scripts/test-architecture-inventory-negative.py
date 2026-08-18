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


def main() -> int:
    module = load_generator()
    expected = module.build_inventory()
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

    print("Architecture inventory checker rejects stale, tampered, missing and D1-projection drift.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
