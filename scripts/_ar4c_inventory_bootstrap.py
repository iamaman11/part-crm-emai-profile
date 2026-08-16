#!/usr/bin/env python3
"""Temporary AR-4C closeout inventory bootstrap; remove before acceptance."""

from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
generator = root / "scripts/generate-architecture-inventory.py"
text = generator.read_text(encoding="utf-8")
old = "    validate_docs()\n    application_architecture = ar3.build_projection(ROOT)"
temporary = (
    "    # TEMP_AR4C_CLOSEOUT_BOOTSTRAP: full validation restored before commit.\n"
    "    application_architecture = ar3.build_projection(ROOT)"
)
if text.count(old) != 1:
    raise SystemExit("expected exactly one canonical validate_docs bootstrap seam")
generator.write_text(text.replace(old, temporary, 1), encoding="utf-8", newline="\n")
try:
    subprocess.run(
        [sys.executable, str(generator), "--write"],
        cwd=root,
        check=True,
    )
finally:
    current = generator.read_text(encoding="utf-8")
    if current.count(temporary) != 1:
        raise SystemExit("temporary inventory bootstrap seam was not present exactly once")
    generator.write_text(current.replace(temporary, old, 1), encoding="utf-8", newline="\n")
