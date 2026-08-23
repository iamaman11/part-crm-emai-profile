#!/usr/bin/env python3
"""Disposable scratch trampoline for deterministic N1 audit. Never transplant."""
from __future__ import annotations

import runpy
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
N1_INPUT_HEAD = "e0b96f33687185034b855a5186e8e2daac13e810"
for relative in (
    "scripts/_ar3_application_architecture.py",
    "scripts/generate-architecture-inventory-engine.py",
    "docs/status.json",
    "architecture/runtime-topology-ar2.json",
):
    data = subprocess.check_output(["git", "show", f"{N1_INPUT_HEAD}:{relative}"], cwd=ROOT)
    path = ROOT / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)

runpy.run_path(str(Path(__file__).with_name("n1_scratch_audit.py")), run_name="__main__")
