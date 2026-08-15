#!/usr/bin/env python3
from __future__ import annotations

import runpy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
runpy.run_path(str(ROOT / "scratch/ar4a_authority_closeout_v2.py"), run_name="__main__")

for relative, markers in {
    "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md": (
        "**Current accepted architecture checkpoint:** AR-4A — Composition-root consolidation",
        "**Next slice:** AR-4B — Client Mail route ownership",
    ),
    "docs/DEVELOPMENT_PLAN.md": (
        "**Current accepted architecture checkpoint:** AR-4A — Composition-root consolidation",
        "**Next architecture slice:** AR-4B — Client Mail route ownership",
    ),
}.items():
    path = ROOT / relative
    lines = path.read_text(encoding="utf-8").splitlines()
    found = {marker: 0 for marker in markers}
    normalized = []
    for line in lines:
        stripped = line.rstrip()
        if stripped in found:
            found[stripped] += 1
            line = stripped
        normalized.append(line)
    if any(count != 1 for count in found.values()):
        raise SystemExit(f"{relative}: expected each AR-4A authority heading exactly once, observed {found}")
    path.write_text("\n".join(normalized) + "\n", encoding="utf-8", newline="\n")

print("AR-4A closeout changed headings normalized for strict git-diff hygiene.")
