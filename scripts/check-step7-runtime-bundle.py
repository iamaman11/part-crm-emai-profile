#!/usr/bin/env python3
"""Enforce permanent Repository Step 7 runtime bundle boundaries."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPOSITORY_REQUIRED = {
    "crates/runtime-bundle-domain/src/lib.rs": (
        "pub struct BundleRelativePath",
        "ReservedSegment",
        "pub struct Sha256Digest",
        "pub struct RuntimeManifest",
        "RUNTIME_MANIFEST_SCHEMA_VERSION",
        "RUNTIME_IPC_VERSION",
        "DuplicateOrCaseCollision",
        "fn unsafe_and_windows_reserved_paths_fail_closed",
        "fn case_colliding_inventory_is_rejected",
    ),
    "tools/runtime_bundle.py": (
        'SOURCE_MARKER = ".synthetic-runtime-root"',
        'DESTINATION_MARKER = ".synthetic-runtime-destination"',
        "ZIP_STORED",
        "FIXED_ZIP_TIME",
        "candidate.is_symlink()",
        "runtime source contains duplicate or case-colliding paths",
        "runtime payload content does not match the manifest",
        "runtime extraction escaped the destination",
    ),
    "runtime/camouhost/main.py": (
        'IPC_VERSION = "1"',
        'emit(f"hello_ack|{IPC_VERSION}")',
        'emit(f"ready|{active_session}")',
        'emit(f"closed|{active_session}|true")',
        "return 3 if negotiated or active_session is not None else 0",
    ),
    "scripts/test-step7-runtime-bundle.py": (
        "first.read_bytes() == second.read_bytes()",
        "payload content does not match",
        "case-colliding",
        "symbolic links",
        "must be empty",
    ),
    "scripts/test-step7-fake-camouhost.py": (
        "launch_before_hello",
        "unsupported_version",
        "session_mismatch",
        "premature_eof",
    ),
}

LEGACY_CORPUS_MARKERS = (
    "temp/browser_profiles",
    "temp\\browser_profiles",
)
NETWORK_INSTALL_MARKERS = (
    "pip install",
    "python -m pip",
    "urllib.request",
    "requests.get",
    "http.client",
)
STEP7_ROOTS = (
    "crates/runtime-bundle-domain",
    "tools/runtime_bundle.py",
    "runtime/camouhost",
    "scripts/test-step7-runtime-bundle.py",
    "scripts/test-step7-fake-camouhost.py",
)
FIXTURE_PREFIX = "tests/runtime-bundle/fixtures/"
POLICY_PATH = "scripts/check-step7-runtime-bundle.py"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    return parser.parse_args()


def relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def scan_files(root: Path, repository_root: bool):
    scan_roots = [root / value for value in STEP7_ROOTS] if repository_root else [root]
    for scan_root in scan_roots:
        if scan_root.is_file():
            yield scan_root
        elif scan_root.exists():
            for path in scan_root.rglob("*"):
                if path.is_file():
                    yield path


def main() -> int:
    root = parse_args().root.resolve()
    repository_root = (root / "Cargo.toml").exists()
    errors: list[str] = []

    for path in scan_files(root, repository_root):
        rel = relative(root, path)
        if rel == POLICY_PATH or (repository_root and rel.startswith(FIXTURE_PREFIX)):
            continue
        if path.suffix not in {".rs", ".py", ".toml", ".json", ".md"}:
            continue
        text = path.read_text(encoding="utf-8")
        for marker in LEGACY_CORPUS_MARKERS:
            if marker in text:
                errors.append(f"legacy profile corpus reference is forbidden in Step 7: {rel}")
        for marker in NETWORK_INSTALL_MARKERS:
            if marker in text:
                errors.append(f"network dependency resolution is forbidden in Step 7: {rel}: {marker}")

    if repository_root:
        for rel, markers in REPOSITORY_REQUIRED.items():
            path = root / rel
            if not path.exists():
                errors.append(f"missing Step 7 runtime bundle boundary: {rel}")
                continue
            text = path.read_text(encoding="utf-8")
            for marker in markers:
                if marker not in text:
                    errors.append(f"missing Step 7 invariant in {rel}: {marker}")

        cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
        if '"crates/runtime-bundle-domain"' not in cargo:
            errors.append("runtime-bundle-domain is missing from the workspace")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Repository Step 7 runtime bundle boundaries are enforced.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
