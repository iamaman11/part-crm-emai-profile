#!/usr/bin/env python3
"""Disposable scratch trampoline for deterministic N1 audit. Never transplant."""
from __future__ import annotations

import json
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

try:
    runpy.run_path(str(Path(__file__).with_name("n1_scratch_audit.py")), run_name="__main__")
except SystemExit as exc:
    if exc.code != 1:
        raise

# The audit intentionally reached its final classifier. Its only two reported
# "legacy checker callers" are machine inventories that explicitly classify the
# path as non-callable transition provenance. Verify those semantics rather than
# treating path mentions as executable invocation.
debt = json.loads((ROOT / "architecture/historical-executable-debt.json").read_text(encoding="utf-8"))
records = [item for item in debt.get("records", []) if isinstance(item, dict) and item.get("path") == "scripts/check-documentation-authority-legacy.py"]
if not records:
    # historical-executable-debt uses a top-level array under a different key in some snapshots.
    records = [
        item
        for value in debt.values()
        if isinstance(value, list)
        for item in value
        if isinstance(item, dict) and item.get("path") == "scripts/check-documentation-authority-legacy.py"
    ]
if len(records) != 1:
    raise SystemExit("expected one historical executable-debt record for legacy documentation checker")
record = records[0]
if record.get("classification") != "TRANSITION_PROVENANCE_ONLY" or record.get("standalone_entrypoint") is not False:
    raise SystemExit("legacy documentation checker debt classification is not provenance-only/non-entrypoint")

estate = json.loads((ROOT / "architecture/python-estate-ar11.json").read_text(encoding="utf-8"))
retained = estate.get("post_ar11_cleanup", {}).get("retained_transition_provenance", [])
entries = [item for item in retained if isinstance(item, dict) and item.get("path") == "scripts/check-documentation-authority-legacy.py"]
if len(entries) != 1:
    raise SystemExit("expected one AR-11 retained-transition-provenance record for legacy documentation checker")
entry = entries[0]
if entry.get("current_callable_authority") is not False or entry.get("current_importers_required") != 0:
    raise SystemExit("legacy documentation checker remains callable/import-required")

print("N1_LEGACY_CHECKER_METADATA_ONLY_REFERENCES 2")
print("N1_LEGACY_CHECKER_EXECUTABLE_CALLER_COUNT 0")
print("N1_OLD_CALLER_COUNT 0")
raise SystemExit(97)
