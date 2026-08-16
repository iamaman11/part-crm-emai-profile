#!/usr/bin/env python3
"""Fail-closed GitHub Actions source and supply-chain policy.

Permanent pull-request workflows must execute against the literal candidate head rather than
GitHub's synthetic pull-request merge ref. External actions are accepted only by immutable
40-hex commit SHA. Checkout credentials must never persist into repository jobs.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ACCEPTED_CHECKOUT_SHA = "f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a"
EXACT_PR_SOURCE_REF = "${{ github.event.pull_request.head.sha || github.sha }}"
FULL_SHA = re.compile(r"[0-9a-fA-F]{40}")
USES_PATTERN = re.compile(r"^(?P<indent>\s*)uses:\s*(?P<target>[^\s#]+)", re.MULTILINE)
PULL_REQUEST_PATTERN = re.compile(r"^  pull_request:\s*(?:#.*)?$", re.MULTILINE)
PULL_REQUEST_TARGET_PATTERN = re.compile(r"^  pull_request_target:\s*(?:#.*)?$", re.MULTILINE)
EXACT_VERIFY_STEP = "- name: Verify exact source checkout"
EXACT_VERIFY_COMMAND = "python -c \"import os, subprocess; actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(); expected = os.environ['EXPECTED_SOURCE_SHA']; assert actual == expected, f'expected {expected}, got {actual}'\""


def workflow_files(root: Path) -> list[Path]:
    workflow_root = root / ".github" / "workflows"
    if not workflow_root.is_dir():
        raise ValueError(f"workflow directory is missing: {workflow_root}")
    return sorted(
        path
        for path in workflow_root.iterdir()
        if path.is_file() and path.suffix in {".yml", ".yaml"}
    )


def checkout_block(lines: list[str], uses_index: int, uses_indent: int) -> str:
    step_indent = max(uses_indent - 2, 0)
    prefix = " " * step_indent + "- "
    end = uses_index + 1
    while end < len(lines):
        candidate = lines[end]
        if candidate.startswith(prefix):
            break
        end += 1
    return "\n".join(lines[uses_index + 1 : end])


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    checkout_occurrences = 0
    external_action_occurrences = 0

    for path in workflow_files(root):
        text = path.read_text(encoding="utf-8")
        relative = path.relative_to(root)
        has_pull_request = bool(PULL_REQUEST_PATTERN.search(text))
        if PULL_REQUEST_TARGET_PATTERN.search(text):
            errors.append(f"{relative}: pull_request_target is forbidden; use unprivileged pull_request plus trusted post-merge jobs")

        lines = text.splitlines()
        pr_checkout_count = 0
        for match in USES_PATTERN.finditer(text):
            target = match.group("target")
            line_number = text.count("\n", 0, match.start()) + 1
            if target.startswith("./"):
                continue
            if target.startswith("docker://"):
                errors.append(f"{relative}:{line_number}: docker action references require an explicit governed digest policy")
                continue
            external_action_occurrences += 1
            if "@" not in target:
                errors.append(f"{relative}:{line_number}: external action must use owner/repo@<40-hex-sha>; found {target}")
                continue
            action, pin = target.rsplit("@", 1)
            if not FULL_SHA.fullmatch(pin):
                errors.append(f"{relative}:{line_number}: external action {action} must use an exact 40-hex commit SHA; found {pin}")

            if action != "actions/checkout":
                continue

            checkout_occurrences += 1
            if pin != ACCEPTED_CHECKOUT_SHA:
                errors.append(
                    f"{relative}:{line_number}: actions/checkout must use accepted exact SHA "
                    f"{ACCEPTED_CHECKOUT_SHA}; found {pin}"
                )

            uses_line_index = line_number - 1
            block = checkout_block(lines, uses_line_index, len(match.group("indent")))
            if "persist-credentials: false" not in block:
                errors.append(f"{relative}:{line_number}: actions/checkout must set persist-credentials: false")
            if has_pull_request:
                pr_checkout_count += 1
                if f"ref: {EXACT_PR_SOURCE_REF}" not in block:
                    errors.append(
                        f"{relative}:{line_number}: pull_request checkout must bind exact candidate source with "
                        f"ref: {EXACT_PR_SOURCE_REF}"
                    )

        if has_pull_request:
            verify_step_count = text.count(EXACT_VERIFY_STEP)
            verify_command_count = text.count(EXACT_VERIFY_COMMAND)
            if verify_step_count != pr_checkout_count or verify_command_count != pr_checkout_count:
                errors.append(
                    f"{relative}: every pull_request checkout must have one cross-platform exact-source "
                    f"verification step; checkouts={pr_checkout_count}, steps={verify_step_count}, "
                    f"commands={verify_command_count}"
                )

    if checkout_occurrences == 0:
        errors.append("no actions/checkout pins found in workflows")
    if external_action_occurrences == 0:
        errors.append("no external GitHub Actions references found in workflows")
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
        print(f"GitHub Actions policy failed: {error}", file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("GitHub Actions use immutable external-action SHAs and exact PR candidate checkout.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
