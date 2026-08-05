#!/usr/bin/env python3
"""Reject temporary Step 4 workflows and diagnostics from accepted source heads."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_FILES = (
    ROOT / "step4-diagnostics.txt",
    ROOT / "docs" / "step4-progress.md",
)


def main() -> int:
    errors: list[str] = []
    for path in FORBIDDEN_FILES:
        if path.exists():
            errors.append(f"temporary Step 4 file remains: {path.relative_to(ROOT)}")

    workflow_root = ROOT / ".github" / "workflows"
    for path in sorted(workflow_root.glob("step4-*.yml")):
        errors.append(f"temporary Step 4 workflow remains: {path.relative_to(ROOT)}")
    for path in sorted(workflow_root.glob("step4-*.yaml")):
        errors.append(f"temporary Step 4 workflow remains: {path.relative_to(ROOT)}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("No temporary Step 4 workflows or diagnostics remain.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
