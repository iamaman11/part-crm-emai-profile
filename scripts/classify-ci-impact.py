#!/usr/bin/env python3
"""Conservative advisory CI impact classifier for #802.

Every required workflow/job is still executed. This is NOT a status or
attestation authorizing gate suppression, merge, release, or provider actions.
Until required GitHub branch-protection contexts can be verified, the output
is informational only. Unknown input and runtime/CI mutations require FULL.
"""
from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path, PurePosixPath

HEX_SHA = re.compile(r"^[0-9a-f]{40}$")
DOCUMENT_PREFIXES = ("docs/",)
DOCUMENT_EXACT = frozenset({"README.md", "CONTRIBUTING.md", "CHANGELOG.md"})
FRONTEND_PREFIXES = ("frontend/",)
FRONTEND_TEST_PREFIXES = ("tests/frontend/",)
CLASSIFICATIONS = {"FAST", "TARGETED", "FULL"}


def valid_repo_path(path: str) -> bool:
    if not path or path.strip() != path or "\\" in path or "\n" in path:
        return False
    parsed = PurePosixPath(path)
    return not parsed.is_absolute() and all(piece not in {"", ".", ".."} for piece in path.split("/"))


def classify(event: str, changed: list[str], *, acceptance_boundary: bool = False) -> tuple[str, str]:
    if event == "push" or acceptance_boundary:
        return "FULL", "accepted-main / K1-K3 or release acceptance requires all exact-head checks"
    if event != "pull_request" or not changed:
        return "FULL", "unrecognized event or missing change evidence"
    if any(not valid_repo_path(path) for path in changed):
        return "FULL", "unsafe or malformed changed path"
    if all(path.startswith(DOCUMENT_PREFIXES) or path in DOCUMENT_EXACT for path in changed):
        return "FAST", "only standalone documentation paths changed"
    if all(
        path.startswith(FRONTEND_PREFIXES) or path.startswith(FRONTEND_TEST_PREFIXES)
        or path.startswith(DOCUMENT_PREFIXES) or path in DOCUMENT_EXACT
        for path in changed
    ):
        return "TARGETED", "frontend plus standalone documentation; dependent UI/contract tests required"
    return "FULL", "runtime, governance, security, lockfile, CI, dependency or unknown path: fail-closed"


def changed_paths(base: str, head: str) -> list[str] | None:
    if not HEX_SHA.fullmatch(base) or not HEX_SHA.fullmatch(head):
        return None
    try:
        result = subprocess.run(
            ["git", "diff", "--no-renames", "--name-only", "-z", base + "..." + head, "--"],
            check=True, capture_output=True,
        )
        return [path.decode("utf-8") for path in result.stdout.split(b"\0") if path]
    except (OSError, UnicodeDecodeError, subprocess.CalledProcessError):
        return None


def self_test() -> None:
    fixtures = [
        ("pull_request", ["docs/architecture.md"], False, "FAST"),
        ("pull_request", ["README.md", "docs/checks.md"], False, "FAST"),
        ("pull_request", ["frontend/src/main.tsx"], False, "TARGETED"),
        ("pull_request", ["frontend/src/main.tsx", "docs/readme.md"], False, "TARGETED"),
        ("pull_request", ["apps/profile-bridge/src/main.rs"], False, "FULL"),
        ("pull_request", ["frontend/src/main.tsx", "crates/contracts/src/lib.rs"], False, "FULL"),
        ("pull_request", ["Cargo.lock"], False, "FULL"),
        ("pull_request", [".github/workflows/quality-gate.yml"], False, "FULL"),
        ("pull_request", ["scripts/classify-ci-impact.py"], False, "FULL"),
        ("pull_request", ["docs/ok.md", "../unsafe.rs"], False, "FULL"),
        ("pull_request", [], False, "FULL"),
        ("pull_request", ["docs/only.md"], True, "FULL"),
        ("push", ["docs/only.md"], False, "FULL"),
        ("workflow_dispatch", ["docs/only.md"], False, "FULL"),
    ]
    for event, changed, acceptance, expected in fixtures:
        actual, _ = classify(event, changed, acceptance_boundary=acceptance)
        if actual != expected:
            raise AssertionError(f"CI impact: {event} {changed!r}: expected {expected}, got {actual}")
    for unsafe in ["/root/absolute", "../up", "docs//bad", "docs/./bad", "docs/../bad", "docs/a\nb"]:
        if valid_repo_path(unsafe):
            raise AssertionError(f"unsafe repo path accepted: {unsafe!r}")
    if changed_paths("not-a-sha", "0" * 40) is not None:
        raise AssertionError("unknown Git identity accepted")
    print("Conservative CI-impact classifier self-tests: PASS; no gate is suppressed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--event", choices=("pull_request", "push"))
    parser.add_argument("--base")
    parser.add_argument("--head")
    parser.add_argument("--acceptance-boundary", action="store_true")
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        if args.event is None:
            return 0
    if not args.event:
        parser.error("--event is required unless --self-test is used")
    changed = (
        changed_paths(args.base or "", args.head or "")
        if args.event == "pull_request" and not args.acceptance_boundary
        else []
    )
    mode, reason = classify(
        args.event, changed or [], acceptance_boundary=args.acceptance_boundary
    )
    assert mode in CLASSIFICATIONS
    print(f"CI_IMPACT_ADVISORY={mode}")
    print(f"CI_IMPACT_REASON={reason}")
    print(f"CI_IMPACT_CHANGED_PATHS={len(changed or [])}")
    print("CI_IMPACT_GATE_EFFECT=NONE; all previously required jobs still execute")
    if args.github_output is not None:
        with args.github_output.open("a", encoding="utf-8") as output:
            output.write(f"impact_mode={mode}\n")
            output.write("impact_is_advisory_only=true\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
