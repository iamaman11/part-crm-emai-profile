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

    workspace_drift = copy.deepcopy(expected)
    workspace_drift["workspace_members"] = [
        *workspace_drift["workspace_members"],
        "crates/does-not-exist",
    ]
    expect_rejected(module, expected, workspace_drift)

    route_drift = copy.deepcopy(expected)
    route_drift["routing"]["public_routes"][0]["route_class"] = "UnknownApi"
    expect_rejected(module, expected, route_drift)

    with tempfile.TemporaryDirectory() as temporary_directory:
        original_path = module.INVENTORY_PATH
        module.INVENTORY_PATH = Path(temporary_directory) / "missing.json"
        try:
            module.check_current(expected)
        except SystemExit:
            pass
        else:
            raise AssertionError("missing architecture inventory unexpectedly passed --check")
        finally:
            module.INVENTORY_PATH = original_path

    print("Architecture inventory checker rejects stale, tampered and missing inventory.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
