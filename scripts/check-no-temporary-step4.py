#!/usr/bin/env python3
"""Reject temporary Step 4 artifacts from accepted source heads."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_FILES = (
    ROOT / "step4-diagnostics.txt",
    ROOT / "docs" / "step4-progress.md",
)
FORBIDDEN_TEST_MARKERS = (
    "exact-head-trigger.md",
    "final-source-boundary.md",
    "governed-writes.md",
    "http-boundary.md",
    "post-hardening-gate.md",
    "technical-gate-trigger.md",
    "webcrypto-boundary.md",
)


def tracked_paths(prefix: str) -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", prefix],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in result.stdout.splitlines() if line]


def main() -> int:
    errors: list[str] = []
    for path in FORBIDDEN_FILES:
        if path.exists():
            errors.append(f"temporary Step 4 file remains: {path.relative_to(ROOT)}")

    gitignore_lines = {
        line.strip()
        for line in (ROOT / ".gitignore").read_text(encoding="utf-8").splitlines()
    }
    if "/target/" not in gitignore_lines:
        errors.append(".gitignore must exclude the repository Rust /target/ directory")

    workflow_root = ROOT / ".github" / "workflows"
    patterns = (
        "step4-*.yml",
        "step4-*.yaml",
        "repository-step4-*.yml",
        "repository-step4-*.yaml",
    )
    for pattern in patterns:
        for path in sorted(workflow_root.glob(pattern)):
            errors.append(f"temporary Step 4 workflow remains: {path.relative_to(ROOT)}")

    marker_root = ROOT / "tests" / "identity-acl"
    for name in FORBIDDEN_TEST_MARKERS:
        path = marker_root / name
        if path.exists():
            errors.append(f"temporary Step 4 gate marker remains: {path.relative_to(ROOT)}")

    for path in tracked_paths("target"):
        errors.append(f"tracked Rust build artifact remains: {path}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("No temporary Step 4 workflows, diagnostics, gate markers or build artifacts remain.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
