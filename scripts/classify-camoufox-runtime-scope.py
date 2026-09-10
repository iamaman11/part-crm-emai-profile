#!/usr/bin/env python3
"""Fail-closed PR scope classifier for the Camoufox Runtime Gate.

Accepted main is handled by the workflow and always receives the full proof. This
classifier is only allowed to suppress heavy PR replay when every changed path is
mechanically known to be outside the Camoufox/Profile Bridge runtime proof
surface. Unknown paths are heavy by default.
"""

from __future__ import annotations

import argparse
from pathlib import Path, PurePosixPath
from typing import Iterable

EXACT_PROVEN_UNRELATED = frozenset(
    {
        # Existing release-ops-only exceptions retained from the original gate.
        ".github/workflows/release-set-promotion.yml",
        ".github/workflows/v2-staging-envelope-observation.yml",
        ".github/scripts/release-operational-ar11.mjs",
        ".github/scripts/ar8-d-secret-transport-successor.mjs",
        ".github/scripts/release-architecture-ar11.mjs",
        "architecture/release-architecture-ar11.json",
        # Historical O0 E3A regression fixture: this retired D1-only workflow is
        # intentionally classified from its path even when the file is deleted.
        ".github/workflows/v2-phase-a-d1-command-router.yml",
    }
)

PROVEN_UNRELATED_PREFIXES = (
    "docs/",
    "migrations/d1/",
    # opsctl is outside the root Cargo workspace and is not consumed by the
    # Camoufox/Profile Bridge runtime gate.
    "tools/opsctl/",
)

PROVEN_UNRELATED_GLOBS = (
    ".github/workflows/d1-*.yml",
    ".github/workflows/d1-*.yaml",
    ".github/scripts/d1-*",
    "scripts/check-d1-*",
    "scripts/d1-*",
)


def is_proven_unrelated(path: str) -> bool:
    if path in EXACT_PROVEN_UNRELATED:
        return True
    if path.startswith(PROVEN_UNRELATED_PREFIXES):
        return True
    candidate = PurePosixPath(path)
    return any(candidate.match(pattern) for pattern in PROVEN_UNRELATED_GLOBS)


def classify(paths: Iterable[str]) -> tuple[bool, list[str], list[str]]:
    normalized = [path.strip() for path in paths if path.strip()]
    if not normalized:
        raise ValueError("changed path set is empty")
    unrelated = sorted(path for path in normalized if is_proven_unrelated(path))
    heavy = sorted(path for path in normalized if not is_proven_unrelated(path))
    return bool(heavy), unrelated, heavy


def run_self_test() -> None:
    fixtures: tuple[tuple[str, list[str], bool], ...] = (
        (
            "o0-e3a-d1-only-regression",
            [
                ".github/workflows/v2-phase-a-d1-command-router.yml",
                "scripts/check-d1-migration-executor.mjs",
            ],
            False,
        ),
        ("docs-only", ["docs/architecture-note.md"], False),
        ("d1-workflow", [".github/workflows/d1-migration-executor.yml"], False),
        ("opsctl-only", ["tools/opsctl/src/main.rs"], False),
        ("runtime", ["runtime/camouhost/launcher.py"], True),
        ("profile-bridge", ["apps/profile-bridge/src/main.rs"], True),
        ("dependency-lock", ["Cargo.lock"], True),
        (
            "classifier-change-is-heavy",
            ["scripts/classify-camoufox-runtime-scope.py"],
            True,
        ),
        (
            "gate-change-is-heavy",
            [".github/workflows/camoufox-runtime-gate.yml"],
            True,
        ),
        (
            "browser-d1-name-is-not-broadly-exempt",
            ["scripts/test-browser-mail-execution-d1.py"],
            True,
        ),
        ("mixed-fails-closed", ["docs/note.md", "runtime/new.py"], True),
        ("unknown-fails-closed", ["new-subsystem/file.txt"], True),
    )
    for name, paths, expected_heavy in fixtures:
        actual_heavy, _, _ = classify(paths)
        if actual_heavy != expected_heavy:
            raise AssertionError(
                f"{name}: expected heavy={expected_heavy}, got {actual_heavy}"
            )
    try:
        classify([])
    except ValueError:
        pass
    else:
        raise AssertionError("empty changed path set must fail closed")
    print("Camoufox runtime scope classifier self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--paths-file", type=Path)
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        run_self_test()

    if args.paths_file is None:
        if args.self_test:
            return 0
        parser.error("--paths-file is required unless --self-test is used alone")

    raw_paths = args.paths_file.read_text(encoding="utf-8").splitlines()
    heavy, unrelated, heavy_paths = classify(raw_paths)

    print("changed paths:")
    for path in sorted(path.strip() for path in raw_paths if path.strip()):
        print(f"  {path}")
    if heavy:
        print("full runtime proof required; runtime-relevant or unclassified paths:")
        for path in heavy_paths:
            print(f"  {path}")
        print("HEAVY=true")
    else:
        print("mechanically proven runtime-unrelated PR; heavy browser replay deferred to accepted main")
        for path in unrelated:
            print(f"  unrelated: {path}")
        print("HEAVY=false")

    if args.github_output is not None:
        with args.github_output.open("a", encoding="utf-8", newline="\n") as handle:
            handle.write(f"heavy={'true' if heavy else 'false'}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
