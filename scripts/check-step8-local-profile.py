#!/usr/bin/env python3
"""Permanent policy checks for Repository Step 8 local profile lifecycle."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

SOURCE = Path("apps/profile-bridge/src/local_profile.rs")

REQUIRED_FRAGMENTS = (
    "MaterializationRoot",
    "BridgeWorkspaceLock",
    ".create_new(true)",
    "GenerationInventory",
    "RecoveryClone",
    "verify_clone_only",
    "LocalGenerationState::DirtyLocal",
    "LocalGenerationState::RecoveryRequired",
    "LocalGenerationState::SyncedEvictable",
    "ForgottenWindowPolicy",
    "QuotaPolicy",
    "render_metadata_only",
)

FORBIDDEN_FRAGMENTS = (
    "temp/browser_profiles",
    "temp\\browser_profiles",
    "ProfileGeneration::repair_in_place",
    "repair_source_generation",
)

BROWSER_LOCK_DELETE = re.compile(
    r"remove_(?:file|dir|dir_all)\s*\([^\n;]*(?:\.parentlock|parent\.lock|[\"']lock[\"'])",
    re.IGNORECASE,
)


def check(root: Path) -> list[str]:
    failures: list[str] = []
    source = root / SOURCE
    if not source.is_file():
        return [f"missing Step 8 source: {SOURCE}"]

    text = source.read_text(encoding="utf-8")
    for fragment in REQUIRED_FRAGMENTS:
        if fragment not in text:
            failures.append(f"missing required Step 8 boundary: {fragment}")
    for fragment in FORBIDDEN_FRAGMENTS:
        if fragment in text:
            failures.append(f"forbidden Step 8 source reference: {fragment}")
    if BROWSER_LOCK_DELETE.search(text):
        failures.append("browser-owned lock deletion is forbidden")

    bridge_lib = root / "apps/profile-bridge/src/lib.rs"
    if not bridge_lib.is_file() or "pub mod local_profile;" not in bridge_lib.read_text(
        encoding="utf-8"
    ):
        failures.append("Profile Bridge does not expose the local_profile module")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    args = parser.parse_args()

    failures = check(args.root.resolve())
    if failures:
        for failure in failures:
            print(f"ERROR: {failure}")
        return 1
    print("Repository Step 8 local profile policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
