#!/usr/bin/env python3
"""Permanent policy checks for Repository Step 8 local profile lifecycle."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

SOURCE_ROOT = Path("apps/profile-bridge/src")
SOURCE_ENTRY = SOURCE_ROOT / "local_profile.rs"
SOURCE_MODULE = SOURCE_ROOT / "local_profile"
DIRTY_GENERATION_SOURCE = SOURCE_ROOT / "dirty_generation.rs"
DIRTY_PUBLISH_SOURCE = SOURCE_ROOT / "dirty_generation_publish.rs"

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

DIRTY_REQUIRED_FRAGMENTS = (
    "LocalGenerationState::DirtyLocal",
    "!local_record.is_locked()",
    "DirtyGenerationError::CandidateMatchesBase",
    "RecoveryClone::create",
    "clone.verify_clone_only()?",
    "encode_workspace_snapshot",
    "Some(base_generation_id.clone())",
    "seal_generation",
    "SourceChanged",
    "MAX_SNAPSHOT_BYTES",
)

DIRTY_FORBIDDEN_FRAGMENTS = (
    "D1Database",
    "worker::",
    "R2Bucket",
    "PROFILE_GENERATIONS",
    "GenerationObjectUploadPort",
)

PUBLISH_REQUIRED_FRAGMENTS = (
    "GenerationObjectUploadPort",
    "GenerationObjectStorePort",
    "put_generation_object_if_absent",
    "GenerationObjectUploadOutcome::ImmutableConflict",
    "verify_generation_object",
    "Ok(PublishedDirtyGeneration",
)

PUBLISH_FORBIDDEN_FRAGMENTS = (
    "D1Database",
    "worker::",
    "R2Bucket",
    "PROFILE_GENERATIONS",
    "register_generation",
    "activate_generation",
    "profile_generation_register_commands",
    "profile_generation_activate_commands",
)

BROWSER_LOCK_DELETE = re.compile(
    r"remove_(?:file|dir|dir_all)\s*\([^\n;]*(?:\.parentlock|parent\.lock|[\"']lock[\"'])",
    re.IGNORECASE,
)


def source_text(root: Path) -> tuple[str, list[str]]:
    failures: list[str] = []
    entry = root / SOURCE_ENTRY
    module = root / SOURCE_MODULE
    files = [entry]
    if module.is_dir():
        files.extend(sorted(module.rglob("*.rs")))
    missing = [path for path in files if not path.is_file()]
    if missing or not entry.is_file():
        failures.append(f"missing Step 8 source entry: {SOURCE_ENTRY}")
        return "", failures
    return "\n".join(path.read_text(encoding="utf-8") for path in files), failures


def dirty_generation_failures(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    failures: list[str] = []
    for fragment in DIRTY_REQUIRED_FRAGMENTS:
        if fragment not in production:
            failures.append(f"missing dirty-generation invariant: {fragment}")
    for fragment in DIRTY_FORBIDDEN_FRAGMENTS:
        if fragment in production:
            failures.append(f"dirty-generation candidate must remain local-only: {fragment}")
    return failures


def dirty_generation_self_test(source: str) -> list[str]:
    fixture = source.split("#[cfg(test)]", 1)[0] + "\nuse worker::d1::D1Database;\n"
    failures = dirty_generation_failures(fixture)
    if not any("local-only: D1Database" in failure for failure in failures):
        return ["dirty-generation negative storage fixture unexpectedly passed"]
    return []


def dirty_publish_failures(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    failures: list[str] = []
    for fragment in PUBLISH_REQUIRED_FRAGMENTS:
        if fragment not in production:
            failures.append(f"missing dirty-generation publication invariant: {fragment}")
    for fragment in PUBLISH_FORBIDDEN_FRAGMENTS:
        if fragment in production:
            failures.append(f"dirty publication must not own persistence/catalog activation: {fragment}")

    upload = production.find(".put_generation_object_if_absent")
    conflict = production.find("GenerationObjectUploadOutcome::ImmutableConflict")
    verify = production.find(".verify_generation_object")
    published = production.find("Ok(PublishedDirtyGeneration")
    if min(upload, conflict, verify, published) < 0 or not (upload < conflict < verify < published):
        failures.append(
            "dirty publication must preserve upload -> immutable-conflict gate -> verify -> publish order"
        )
    return failures


def dirty_publish_self_test(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    fixture = production.replace(".verify_generation_object", ".verification_removed", 1)
    failures = dirty_publish_failures(fixture)
    if not any("missing dirty-generation publication invariant: verify_generation_object" in failure for failure in failures):
        return ["dirty-publication missing-verification fixture unexpectedly passed"]
    return []


def check(root: Path) -> list[str]:
    text, failures = source_text(root)
    if failures:
        return failures

    for fragment in REQUIRED_FRAGMENTS:
        if fragment not in text:
            failures.append(f"missing required Step 8 boundary: {fragment}")
    for fragment in FORBIDDEN_FRAGMENTS:
        if fragment in text:
            failures.append(f"forbidden Step 8 source reference: {fragment}")
    if BROWSER_LOCK_DELETE.search(text):
        failures.append("browser-owned lock deletion is forbidden")

    bridge_lib = root / "apps/profile-bridge/src/lib.rs"
    if not bridge_lib.is_file():
        failures.append("Profile Bridge library source is missing")
    else:
        bridge_lib_text = bridge_lib.read_text(encoding="utf-8")
        if "pub mod local_profile;" not in bridge_lib_text:
            failures.append("Profile Bridge does not expose the local_profile module")
        if "pub mod dirty_generation;" not in bridge_lib_text:
            failures.append("Profile Bridge does not expose the dirty_generation module")
        if "pub mod dirty_generation_publish;" not in bridge_lib_text:
            failures.append("Profile Bridge does not expose the dirty_generation_publish module")

    dirty_source = root / DIRTY_GENERATION_SOURCE
    if not dirty_source.is_file():
        failures.append(f"missing dirty-generation source: {DIRTY_GENERATION_SOURCE}")
    else:
        dirty_text = dirty_source.read_text(encoding="utf-8")
        failures.extend(dirty_generation_failures(dirty_text))
        failures.extend(dirty_generation_self_test(dirty_text))

    publish_source = root / DIRTY_PUBLISH_SOURCE
    if not publish_source.is_file():
        failures.append(f"missing dirty-generation publication source: {DIRTY_PUBLISH_SOURCE}")
    else:
        publish_text = publish_source.read_text(encoding="utf-8")
        failures.extend(dirty_publish_failures(publish_text))
        failures.extend(dirty_publish_self_test(publish_text))

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
    print("Repository Step 8 local profile and dirty-generation policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())