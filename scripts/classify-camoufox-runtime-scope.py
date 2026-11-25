#!/usr/bin/env python3
"""Fail-closed PR scope classifier for the Camoufox Runtime Gate.

Accepted main uses this same policy only after GitHub independently proves
the exact accepted PR and a previous executed real Camoufox/Bridge Windows test.
Both PR and main suppress heavy browser replay only when every intervening path
is mechanically proven outside the runtime/Bridge proof surface. Unknown paths
are heavy by default.
"""

from __future__ import annotations

import argparse
from fnmatch import fnmatchcase
from pathlib import Path
from typing import Iterable
from unicodedata import category

EXACT_PROVEN_UNRELATED = frozenset(
    {
        # Existing release-ops-only exceptions retained from the original gate.
        ".github/workflows/release-set-promotion.yml",
        ".github/workflows/v2-staging-envelope-observation.yml",
        ".github/scripts/release-operational-ar11.mjs",
        ".github/scripts/ar8-d-secret-transport-successor.mjs",
        ".github/scripts/release-architecture-ar11.mjs",
        # These are bounded AR11 orchestration/evidence inputs, not Camoufox,
        # Bridge or the runtime identity topology consumed by its packager.
        "architecture/ar11-behavioural-certification.json",
        "scripts/download-release-set-ar11.py",
        "scripts/promotion-operational-outcome-ar11.py",
        # Staging-only AR11 overlay consumes immutable runtime identities;
        # it is not imported by the official Camoufox/Bridge test owner.
        "scripts/release-core-overlay-ar11.py",
        "scripts/release-set-assets-ar11.sh",
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


def is_safe_git_path(path: str) -> bool:
    """Reject ambiguous Git names before any unrelated-path exemption.

    Git can store names with whitespace and control bytes. Never normalize a
    tracked path into a different path that would accidentally match a safe
    exemption; malformed names belong to full/fail-closed proof.
    """
    return (
        isinstance(path, str)
        and bool(path)
        and not path.startswith("/")
        and "\\" not in path
        and all(part and part not in {".", ".."} and part == part.strip()
                for part in path.split("/"))
        and not any(category(char).startswith("C") for char in path)
    )


def is_proven_unrelated(path: str) -> bool:
    if not is_safe_git_path(path):
        return False
    # The Windows Camoufox/Bridge proof consumes no frontend/src source.
    # The independent frontend and cross-component gates still execute; only
    # isolated React/CSS source may inherit exact published-byte browser proof.
    if path.startswith("frontend/src/") and path.endswith((".ts", ".tsx", ".css")):
        return True
    if path in EXACT_PROVEN_UNRELATED:
        return True
    if path.startswith(PROVEN_UNRELATED_PREFIXES):
        return True
    # PurePosixPath.match() also matches suffixes (nested/scripts/d1-*). A
    # safe glob must match only a filename under its exact repository root.
    prefix, separator, basename = path.rpartition("/")
    return bool(separator) and any(
        prefix == pattern_prefix and fnmatchcase(basename, pattern_basename)
        for pattern_prefix, pattern_basename in
        (pattern.rsplit("/", 1) for pattern in PROVEN_UNRELATED_GLOBS)
    )


def classify(paths: Iterable[str]) -> tuple[bool, list[str], list[str]]:
    exact_paths = list(paths)
    if not exact_paths or any(not isinstance(path, str) or not path for path in exact_paths):
        raise ValueError("changed path set is empty")
    unrelated = sorted(path for path in exact_paths if is_proven_unrelated(path))
    heavy = sorted(path for path in exact_paths if not is_proven_unrelated(path))
    return bool(heavy), unrelated, heavy


def decode_git_paths(raw: bytes) -> list[str]:
    """Parse `git diff --name-only -z` without line/shell escaping or trim."""
    if not raw or not raw.endswith(b"\0"):
        raise ValueError("Git path stream is not NUL terminated")
    names = raw[:-1].split(b"\0")
    if not all(names):
        raise ValueError("Git path stream contains an empty filename")
    return [name.decode("utf-8", errors="strict") for name in names]


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
        ("frontend-only", [
            "frontend/src/app/main.tsx",
            "frontend/src/components/dialog.ts",
            "frontend/src/styles/app.css",
        ], False),
        ("frontend-package-is-heavy", ["frontend/package.json"], True),
        ("frontend-public-is-heavy", ["frontend/public/index.html"], True),
        ("frontend-generated-is-heavy", ["frontend/src/generated/schema.json"], True),
        ("frontend-asset-is-heavy", ["frontend/src/icons/profile.svg"], True),
        ("frontend-cargo-is-heavy", ["frontend/src/Cargo.toml"], True),
        ("frontend-with-rust-is-heavy", [
            "frontend/src/app/main.tsx",
            "apps/profile-bridge/src/main.rs",
        ], True),
        ("frontend-with-runtime-lock-is-heavy", [
            "frontend/src/app/main.tsx",
            "runtime/camouhost/runtime-lock.json",
        ], True),
        ("frontend-traversal-is-heavy", ["frontend/src/../runtime/camouhost/real.ts"], True),
        ("frontend-ambiguous-backslash-is-heavy", [r"frontend/src\main.tsx"], True),
        ("d1-workflow", [".github/workflows/d1-migration-executor.yml"], False),
        ("opsctl-only", ["tools/opsctl/src/main.rs"], False),
        ("ar11-only", [
            "architecture/ar11-behavioural-certification.json",
            "scripts/download-release-set-ar11.py",
            "scripts/promotion-operational-outcome-ar11.py",
            ".github/workflows/release-set-promotion.yml",
        ], False),
        ("ar11-overlay-only", ["scripts/release-core-overlay-ar11.py"], False),
        ("ar11-asset-materializer-only", ["scripts/release-set-assets-ar11.sh"], False),
        ("release-architecture-is-runtime-input", [
            "architecture/release-architecture-ar11.json",
        ], True),
        ("ar11-asset-materializer-with-runtime", [
            "scripts/release-set-assets-ar11.sh",
            "runtime/camouhost/real.py",
        ], True),
        ("ar11-overlay-plus-real-runtime", [
            "scripts/release-core-overlay-ar11.py",
            "runtime/camouhost/real.py",
        ], True),
        ("ar11-overlay-plus-bridge", [
            "scripts/release-core-overlay-ar11.py",
            "apps/profile-bridge/src/main.rs",
        ], True),
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
            [".github/workflows/ci.yml"],
            True,
        ),
        (
            "browser-d1-name-is-not-broadly-exempt",
            ["scripts/test-browser-mail-execution-d1.py"],
            True,
        ),
        ("mixed-fails-closed", ["docs/note.md", "runtime/new.py"], True),
        ("ar11-with-bridge", ["scripts/download-release-set-ar11.py", "apps/profile-bridge/src/main.rs"], True),
        ("ar11-with-lock", ["scripts/promotion-operational-outcome-ar11.py", "Cargo.lock"], True),
        ("ar11-with-classifier", ["architecture/ar11-behavioural-certification.json", "scripts/classify-camoufox-runtime-scope.py"], True),
        ("ar11-with-authority", ["scripts/download-release-set-ar11.py", "architecture/release-architecture-ar11.json"], True),
        ("non-root-suffix", ["nested/scripts/check-d1-adapter.py"], True),
        ("non-root-workflow", ["nested/.github/workflows/d1-migration-executor.yml"], True),
        ("glob-does-not-cross-directories", ["scripts/check-d1-test/nested.py"], True),
        ("leading-whitespace", [" scripts/download-release-set-ar11.py"], True),
        ("trailing-whitespace", ["scripts/download-release-set-ar11.py "], True),
        ("nested-leading-whitespace", ["docs/ note.md"], True),
        ("traversal", ["docs/../runtime/camouhost/real.py"], True),
        ("embedded-newline", ["docs/unsafe\nname.md"], True),
        ("embedded-control", ["docs/unsafe\tname.md"], True),
        ("unicode-format-control", ["docs/unsafe\u202ename.md"], True),
        ("unicode-direction-isolate", ["docs/unsafe\u2066name.md"], True),
        ("unicode-next-line-control", ["docs/unsafe\u0085name.md"], True),
        ("embedded-backslash", [r"docs\runtime\real.py"], True),
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
    for raw, expected_paths in (
        (b"scripts/download-release-set-ar11.py\0", ["scripts/download-release-set-ar11.py"]),
        (b"docs/unsafe\nname.md\0", ["docs/unsafe\nname.md"]),
        (b" scripts/download-release-set-ar11.py\0", [" scripts/download-release-set-ar11.py"]),
    ):
        if decode_git_paths(raw) != expected_paths:
            raise AssertionError("Git NUL stream changed the exact filename")
    for invalid in (
        b"", b"docs/note.md", b"docs/note.md\0\0", b"\0", b"docs/note.md\0\xff\0",
    ):
        try:
            decode_git_paths(invalid)
        except (ValueError, UnicodeDecodeError):
            pass
        else:
            raise AssertionError("malformed Git path stream was accepted")
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

    try:
        raw_paths = decode_git_paths(args.paths_file.read_bytes())
    except (ValueError, UnicodeDecodeError) as error:
        raise SystemExit("Invalid exact Git NUL path stream") from error
    heavy, unrelated, heavy_paths = classify(raw_paths)

    print("changed paths:")
    for path in sorted(raw_paths):
        print(f"  {path!r}")
    if heavy:
        print("full runtime proof required; runtime-relevant or unclassified paths:")
        for path in heavy_paths:
            print(f"  {path!r}")
        print("HEAVY=true")
    else:
        print("mechanically proven runtime-unrelated PR; heavy browser replay deferred to accepted main")
        for path in unrelated:
            print(f"  unrelated: {path!r}")
        print("HEAVY=false")

    if args.github_output is not None:
        with args.github_output.open("a", encoding="utf-8", newline="\n") as handle:
            handle.write(f"heavy={'true' if heavy else 'false'}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
