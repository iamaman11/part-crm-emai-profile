#!/usr/bin/env python3
"""Reject GitHub checkout action pins that still target the deprecated Node 20 runtime."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ACCEPTED_CHECKOUT_SHA = "f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a"
CHECKOUT_PATTERN = re.compile(r"^\s*uses:\s*actions/checkout@([^\s#]+)", re.MULTILINE)


def workflow_files(root: Path) -> list[Path]:
    workflow_root = root / ".github" / "workflows"
    if not workflow_root.is_dir():
        raise ValueError(f"workflow directory is missing: {workflow_root}")
    return sorted(
        path
        for path in workflow_root.iterdir()
        if path.is_file() and path.suffix in {".yml", ".yaml"}
    )


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    occurrences = 0
    for path in workflow_files(root):
        text = path.read_text(encoding="utf-8")
        for match in CHECKOUT_PATTERN.finditer(text):
            occurrences += 1
            pin = match.group(1)
            line = text.count("\n", 0, match.start()) + 1
            if pin != ACCEPTED_CHECKOUT_SHA:
                errors.append(
                    f"{path.relative_to(root)}:{line}: actions/checkout must use "
                    f"exact Node-24-native SHA {ACCEPTED_CHECKOUT_SHA}; found {pin}"
                )
    if occurrences == 0:
        errors.append("no actions/checkout pins found in permanent workflows")
    return errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        errors = validate(args.root.resolve())
    except (OSError, ValueError) as error:
        print(f"GitHub Actions runtime policy failed: {error}", file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(
        "GitHub Actions checkout pins use the accepted Node-24-native exact SHA."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
