#!/usr/bin/env python3
"""Fail-closed GitHub Actions source and supply-chain policy.

Pre-merge pull-request workflows execute against the literal candidate head rather than GitHub's
synthetic merge ref. The single governed POST_MERGE_METADATA workflow is a distinct lifecycle class:
it may run only for merged pull_request:closed events, must bind the exact accepted merge commit,
and may write only append-only tag metadata through the GitHub Git API. External actions are
accepted only by immutable 40-hex commit SHA. Checkout credentials never persist into jobs.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ACCEPTED_CHECKOUT_SHA = "f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a"
EXACT_PR_SOURCE_REF = "${{ github.event.pull_request.head.sha || github.sha }}"
EXACT_MERGE_SOURCE_REF = "${{ github.event.pull_request.merge_commit_sha }}"
FULL_SHA = re.compile(r"[0-9a-fA-F]{40}")
USES_PATTERN = re.compile(r"^(?P<indent>\s*)uses:\s*(?P<target>[^\s#]+)", re.MULTILINE)
PULL_REQUEST_PATTERN = re.compile(r"^  pull_request:\s*(?:#.*)?$", re.MULTILINE)
PULL_REQUEST_TARGET_PATTERN = re.compile(r"^  pull_request_target:\s*(?:#.*)?$", re.MULTILINE)
EXACT_VERIFY_STEP = "- name: Verify exact source checkout"
EXACT_VERIFY_COMMAND = "python -c \"import os, subprocess; actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(); expected = os.environ['EXPECTED_SOURCE_SHA']; assert actual == expected, f'expected {expected}, got {actual}'\""
POST_MERGE_VERIFY_STEP = "- name: Verify exact accepted merge checkout"
POST_MERGE_VERIFY_COMMAND = 'test "$(git rev-parse HEAD)" = "$MERGE_SHA"'
REGISTRY = Path("architecture/github-actions-registry.json")
POST_MERGE_CATEGORY = "POST_MERGE_METADATA"


def workflow_files(root: Path) -> list[Path]:
    workflow_root = root / ".github" / "workflows"
    if not workflow_root.is_dir():
        raise ValueError(f"workflow directory is missing: {workflow_root}")
    return sorted(path for path in workflow_root.iterdir() if path.is_file() and path.suffix in {".yml", ".yaml"})


def post_merge_metadata_paths(root: Path) -> set[str]:
    registry = root / REGISTRY
    if not registry.is_file():
        return set()
    payload = json.loads(registry.read_text(encoding="utf-8"))
    registrations = payload.get("active_registrations", [])
    if not isinstance(registrations, list):
        raise ValueError(f"{REGISTRY}: active_registrations must be a list")
    result: set[str] = set()
    for row in registrations:
        if not isinstance(row, dict):
            raise ValueError(f"{REGISTRY}: registration must be an object")
        if row.get("category") == POST_MERGE_CATEGORY:
            workflow_path = row.get("path")
            if not isinstance(workflow_path, str):
                raise ValueError(f"{REGISTRY}: POST_MERGE_METADATA path must be a string")
            result.add(workflow_path)
    return result


def checkout_block(lines: list[str], uses_index: int, uses_indent: int) -> str:
    step_indent = max(uses_indent - 2, 0)
    prefix = " " * step_indent + "- "
    end = uses_index + 1
    while end < len(lines):
        if lines[end].startswith(prefix):
            break
        end += 1
    return "\n".join(lines[uses_index + 1 : end])


def validate_post_merge_metadata(relative: Path, text: str, checkout_count: int) -> list[str]:
    errors: list[str] = []
    label = relative.as_posix()
    if not re.search(r"^\s{4}types:\s*\[closed\]\s*(?:#.*)?$", text, re.MULTILINE):
        errors.append(f"{label}: POST_MERGE_METADATA must trigger only on pull_request types: [closed]")
    if "github.event.pull_request.merged == true" not in text:
        errors.append(f"{label}: POST_MERGE_METADATA must fail closed unless pull_request.merged == true")
    if "contents: write" not in text:
        errors.append(f"{label}: POST_MERGE_METADATA requires explicit contents: write only for tag metadata creation")
    if checkout_count != 1:
        errors.append(f"{label}: POST_MERGE_METADATA must have exactly one accepted-merge checkout; observed={checkout_count}")
    if text.count(POST_MERGE_VERIFY_STEP) != checkout_count or text.count(POST_MERGE_VERIFY_COMMAND) != checkout_count:
        errors.append(f"{label}: POST_MERGE_METADATA checkout must have one exact accepted-merge verification step")

    forbidden = (
        "git push ",
        "git push\n",
        "git commit ",
        "git commit\n",
        "git checkout -b",
        "git switch -c",
        "/contents/",
        "/branches/",
        "/environments/",
        "wrangler deploy",
        "terraform apply",
    )
    lower = text.lower()
    for marker in forbidden:
        if marker.lower() in lower:
            errors.append(f"{label}: POST_MERGE_METADATA contains forbidden source/provider mutation marker {marker!r}")
    for required in ("git/tags", "git/refs", "refs/tags/", "architecture/accepted/"):
        if required not in text:
            errors.append(f"{label}: POST_MERGE_METADATA missing append-only tag boundary marker {required!r}")
    return errors


def post_merge_negative_self_test() -> bool:
    fixture = f"""
on:
  pull_request:
    types: [closed]
permissions:
  contents: write
jobs:
  record:
    if: github.event.pull_request.merged == true
    steps:
      - name: Verify exact accepted merge checkout
        run: {POST_MERGE_VERIFY_COMMAND}
      - run: |
          echo git/tags git/refs refs/tags/ architecture/accepted/
          git push origin HEAD:main
"""
    errors = validate_post_merge_metadata(Path("synthetic/post-merge.yml"), fixture, 1)
    return any("forbidden source/provider mutation marker 'git push " in error for error in errors)


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    checkout_occurrences = 0
    external_action_occurrences = 0
    post_merge_paths = post_merge_metadata_paths(root)
    observed_post_merge: set[str] = set()

    for path in workflow_files(root):
        text = path.read_text(encoding="utf-8")
        relative = path.relative_to(root)
        relative_name = relative.as_posix()
        has_pull_request = bool(PULL_REQUEST_PATTERN.search(text))
        is_post_merge = relative_name in post_merge_paths
        if is_post_merge:
            observed_post_merge.add(relative_name)
        if PULL_REQUEST_TARGET_PATTERN.search(text):
            errors.append(f"{relative}: pull_request_target is forbidden; use unprivileged pull_request plus trusted post-merge jobs")
        if is_post_merge and not has_pull_request:
            errors.append(f"{relative}: POST_MERGE_METADATA must be a pull_request:closed workflow")

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
                errors.append(f"{relative}:{line_number}: actions/checkout must use accepted exact SHA {ACCEPTED_CHECKOUT_SHA}; found {pin}")

            uses_line_index = line_number - 1
            block = checkout_block(lines, uses_line_index, len(match.group("indent")))
            if "persist-credentials: false" not in block:
                errors.append(f"{relative}:{line_number}: actions/checkout must set persist-credentials: false")
            if has_pull_request:
                pr_checkout_count += 1
                expected_ref = EXACT_MERGE_SOURCE_REF if is_post_merge else EXACT_PR_SOURCE_REF
                if f"ref: {expected_ref}" not in block:
                    lifecycle = "accepted merge" if is_post_merge else "exact candidate"
                    errors.append(f"{relative}:{line_number}: pull_request {lifecycle} checkout must bind ref: {expected_ref}")

        if has_pull_request:
            if is_post_merge:
                errors.extend(validate_post_merge_metadata(relative, text, pr_checkout_count))
            else:
                verify_step_count = text.count(EXACT_VERIFY_STEP)
                verify_command_count = text.count(EXACT_VERIFY_COMMAND)
                if verify_step_count != pr_checkout_count or verify_command_count != pr_checkout_count:
                    errors.append(
                        f"{relative}: every pull_request checkout must have one cross-platform exact-source verification step; "
                        f"checkouts={pr_checkout_count}, steps={verify_step_count}, commands={verify_command_count}"
                    )

    for relative in sorted(post_merge_paths - observed_post_merge):
        errors.append(f"{relative}: POST_MERGE_METADATA registration has no tracked workflow file")
    if len(post_merge_paths) > 1:
        errors.append(f"POST_MERGE_METADATA workflow authority must be singular; observed={len(post_merge_paths)}")
    if not post_merge_negative_self_test():
        errors.append("POST_MERGE_METADATA branch-push negative fixture unexpectedly passed")
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
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"GitHub Actions policy failed: {error}", file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("GitHub Actions use immutable action pins with typed exact candidate/post-merge source binding and branch-push rejection.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
