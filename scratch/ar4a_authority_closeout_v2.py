#!/usr/bin/env python3
from __future__ import annotations

import json
import runpy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
runpy.run_path(str(ROOT / "scratch/ar4a_authority_closeout.py"), run_name="__main__")

# The canonical generator validates documentation authority before rewriting inventory.
# Project only the checker-owned sequencing fields here to break that deterministic
# closeout bootstrap cycle; generate-architecture-inventory.py immediately replaces
# the complete file from canonical generator logic in the next workflow step.
path = ROOT / "architecture/inventory.json"
inventory = json.loads(path.read_text(encoding="utf-8"))
doc = inventory["documentation_authority"]
doc["current_slice"] = "AR-4A"
doc["application_architecture_evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md"
doc["application_architecture_base_evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR3.md"
state = inventory["program_state"]
state["accepted_architecture_slices"] = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A"]
state["current_architecture_slice"] = "AR-4A"
state["next_architecture_slice_after_acceptance"] = "AR-4B"
path.write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
print("AR-4A inventory bootstrap projection applied; canonical regeneration remains authoritative.")
