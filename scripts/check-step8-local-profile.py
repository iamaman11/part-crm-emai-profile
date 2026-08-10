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
DIRTY_CLOSE_SOURCE = SOURCE_ROOT / "dirty_close.rs"

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
    "GenerationObjectExactVerifyPort",
    "put_generation_object_if_absent",
    "GenerationObjectUploadOutcome::ImmutableConflict",
    "verify_generation_object_exact",
    "Ok(PublishedDirtyGeneration",
)

PUBLISH_FORBIDDEN_FRAGMENTS = (
    "D1Database",
    "worker::",
    "R2Bucket",
    "PROFILE_GENERATIONS",
    "GenerationObjectStorePort",
    "GenerationObjectReference",
    ".verify_generation_object(",
    "register_generation",
    "activate_generation",
    "profile_generation_register_commands",
    "profile_generation_activate_commands",
)

DIRTY_CLOSE_REQUIRED_FRAGMENTS = (
    "pub struct RetainedDirtyClose",
    "pub fn begin_after_browser_close",
    "LocalGenerationState::DirtyLocal",
    "publish_verify_and_commit_dirty_generation(",
    ".apply_local_successor",
    "DirtyCloseLocalOutcome::RematerializeRequired",
    "let workspace_lock = self",
    "workspace_lock.release()",
    ".set_locked(false)",
    "workspace_lock_released && coordinator.close_lease(&self.lease).is_ok()",
)

DIRTY_CLOSE_REQUIRED_TESTS = (
    "commit_failure_retains_workspace_lock_and_coordinator_lease",
    "authoritative_commit_releases_ownership_only_after_local_successor",
    "post_commit_candidate_change_releases_old_base_and_requires_rematerialization",
    "post_commit_candidate_read_failure_releases_old_base_and_requires_rematerialization",
)

DIRTY_CLOSE_FORBIDDEN_FRAGMENTS = (
    "D1Database",
    "worker::",
    "R2Bucket",
    "std::process::Command",
    "windows_sys::",
)

POST_COMMIT_SUPERSEDE_GUARD = (
    "self.base.state() == LocalGenerationState::SupersededEvictable"
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


def braced_function_body(production: str, marker: str) -> str:
    start = production.find(marker)
    if start < 0:
        return ""
    opening = production.find("{", start)
    if opening < 0:
        return ""
    depth = 0
    for index in range(opening, len(production)):
        character = production[index]
        if character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return production[opening : index + 1]
    return ""


def publication_function_body(production: str) -> str:
    return braced_function_body(production, "pub async fn publish_prepared_dirty_generation")


def dirty_publish_failures(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    failures: list[str] = []
    for fragment in PUBLISH_REQUIRED_FRAGMENTS:
        if fragment not in production:
            failures.append(f"missing dirty-generation publication invariant: {fragment}")
    for fragment in PUBLISH_FORBIDDEN_FRAGMENTS:
        if fragment in production:
            failures.append(f"dirty publication must not own persistence/catalog activation: {fragment}")

    flow = publication_function_body(production)
    upload = flow.find(".put_generation_object_if_absent")
    conflict = flow.find("GenerationObjectUploadOutcome::ImmutableConflict")
    verify = flow.find(".verify_generation_object_exact")
    published = flow.find("Ok(PublishedDirtyGeneration")
    if min(upload, conflict, verify, published) < 0 or not (upload < conflict < verify < published):
        failures.append(
            "dirty publication must preserve upload -> immutable-conflict gate -> exact verify -> publish order"
        )
    return failures


def dirty_publish_self_test(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    fixture = production.replace(
        ".verify_generation_object_exact", ".verification_removed", 1
    )
    failures = dirty_publish_failures(fixture)
    if not any(
        "missing dirty-generation publication invariant: verify_generation_object_exact" in failure
        for failure in failures
    ):
        return ["dirty-publication missing-exact-verification fixture unexpectedly passed"]
    return []


def dirty_close_failures(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    failures: list[str] = []
    for fragment in DIRTY_CLOSE_REQUIRED_FRAGMENTS:
        if fragment not in production:
            failures.append(f"missing retained dirty-close invariant: {fragment}")
    for fragment in DIRTY_CLOSE_FORBIDDEN_FRAGMENTS:
        if fragment in production:
            failures.append(f"retained dirty close must remain provider-independent: {fragment}")
    for test_name in DIRTY_CLOSE_REQUIRED_TESTS:
        if test_name not in source:
            failures.append(f"missing retained dirty-close recovery test: {test_name}")
    if POST_COMMIT_SUPERSEDE_GUARD not in production:
        failures.append(
            "retained dirty close must rematerialize after authoritative commit once base is superseded"
        )

    flow = braced_function_body(production, "pub async fn finalize")
    finalize = flow.find("publish_verify_and_commit_dirty_generation(")
    successor = flow.find(".apply_local_successor")
    superseded = flow.find(POST_COMMIT_SUPERSEDE_GUARD)
    rematerialize = flow.find("DirtyCloseLocalOutcome::RematerializeRequired")
    lock_take = flow.find("let workspace_lock = self")
    lock_release = flow.find("workspace_lock.release()")
    local_unlock = flow.find(".set_locked(false)")
    lease_close = flow.find("coordinator.close_lease(&self.lease)")
    positions = (
        finalize,
        successor,
        superseded,
        rematerialize,
        lock_take,
        lock_release,
        local_unlock,
        lease_close,
    )
    if min(positions) < 0 or not (
        finalize
        < successor
        < superseded
        < rematerialize
        < lock_take
        < lock_release
        < local_unlock
        < lease_close
    ):
        failures.append(
            "retained dirty close must preserve authoritative finalize -> local successor/rematerialize -> workspace release -> local unlock -> coordinator close order"
        )
    return failures


def dirty_close_self_test(source: str) -> list[str]:
    fixture = source.replace(POST_COMMIT_SUPERSEDE_GUARD, "false", 1)
    failures = dirty_close_failures(fixture)
    if not any(
        "must rematerialize after authoritative commit once base is superseded" in failure
        for failure in failures
    ):
        return ["retained dirty-close missing post-commit supersede guard fixture unexpectedly passed"]
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
        if "pub mod dirty_close;" not in bridge_lib_text:
            failures.append("Profile Bridge does not expose the dirty_close module")

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

    dirty_close_source = root / DIRTY_CLOSE_SOURCE
    if not dirty_close_source.is_file():
        failures.append(f"missing retained dirty-close source: {DIRTY_CLOSE_SOURCE}")
    else:
        dirty_close_text = dirty_close_source.read_text(encoding="utf-8")
        failures.extend(dirty_close_failures(dirty_close_text))
        failures.extend(dirty_close_self_test(dirty_close_text))

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
    print("Repository Step 8 local profile, dirty-generation and retained-close policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
