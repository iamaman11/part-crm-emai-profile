#!/usr/bin/env python3
from pathlib import Path

path = Path("docs/DEVELOPMENT_PLAN.md")
lines = path.read_text(encoding="utf-8").splitlines()
matched = 0
for index, line in enumerate(lines):
    if line.startswith("**Tracking:**"):
        lines[index] = line.rstrip()
        matched += 1
if matched != 1:
    raise SystemExit(f"Tracking line: expected one match, found {matched}")
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
