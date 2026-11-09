#!/usr/bin/env python3
"""Phase 2F ownership guard for the accepted native Camoufox/Playwright model.

The predecessor RetainedDirtyClose operator is a synthetic/unit-test fixture,
not the user-facing browser or Save owner. Guard the real shipping Open/Close/
Sync chain instead of requiring the retired production operator to return.
"""

from __future__ import annotations

import argparse
from pathlib import Path

SOURCES = {
    "cli": "apps/profile-bridge/src/main.rs",
    "shipping": "apps/profile-bridge/src/shipping_composition.rs",
    "local": "apps/profile-bridge/src/local_browser_open.rs",
    "native": "runtime/camouhost/native_process.py",
    "adapter": "runtime/camouhost/real.py",
    "operator": "apps/profile-bridge/src/operator_flow.rs",
    "locks": "apps/profile-bridge/src/local_profile/quiescence.rs",
    "generation": "apps/profile-bridge/src/generation_initialization.rs",
    "authoritative": "apps/profile-bridge/src/authoritative_generation.rs",
}


def function_body(source: str, marker: str) -> str:
    start = source.find(marker)
    if start < 0:
        return ""
    opening = source.find("{", start)
    if opening < 0:
        return ""
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening : index + 1]
    return ""


def check_sources(sources: dict[str, str]) -> list[str]:
    failures: list[str] = []
    cli = sources["cli"]
    shipping = sources["shipping"]
    local = sources["local"]
    native = sources["native"]
    adapter = sources["adapter"]
    operator = sources["operator"]
    locks = sources["locks"]
    generation = sources["generation"]
    authoritative = sources["authoritative"]

    for required in ("BridgeCommand::InitialProfile(intent)", "BridgeCommand::SyncClosed(intent)",
                     "run_closed_profile_sync(&intent)", '"--sync-closed"'):
        if required not in cli:
            failures.append(f"shipping Bridge CLI lost single native Open/Sync dispatch: {required}")

    initial = function_body(shipping, "pub fn run_initial(")
    local_open = initial.find("crate::local_browser_open::open(")
    sync_handoff = initial.find("start_sync_after_close(intent);")
    if local_open < 0 or sync_handoff < local_open:
        failures.append("normal native Open must hand off to the same post-close Sync")
    if "Err(ShippingCompositionError::LocalOpen(error))" not in initial:
        failures.append("abnormal adapter exit must retain the same fail-closed Sync handoff")

    closed = function_body(shipping, "pub fn run_closed_sync(")
    file_gate = closed.find("wait_for_native_writer_release(")
    save = closed.find("sync_after_native_close(")
    if file_gate < 0 or save < file_gate:
        failures.append("native file-release proof must precede the existing closed-profile Sync")
    if 'workspace.path().join(".local-unsynced")' not in closed:
        failures.append("the existing persistent dirty marker must govern pending cloud Sync")
    for required in ("seal_closed_workspace_streaming(", "publish_closed_streamed_successor(",
                     "sync_closed_local_draft("):
        if required not in shipping:
            failures.append(f"single existing snapshot/cloud Save owner missing: {required}")

    opened = function_body(local, "pub fn open(")
    marker = opened.find("mark_local_unsynced(workspace.path())?;")
    launch = opened.find('run_python(runtime_root, workspace.path(), "--local-open")')
    if marker < 0 or launch < marker:
        failures.append("durable local dirty evidence must precede native browser launch")
    if "BridgeWorkspaceLock::acquire_or_recover_interrupted(&workspace" not in opened:
        failures.append("native Open must hold the existing per-profile OS admission through the shared recovery owner")
    lock_admission = function_body(locks, "pub(crate) fn acquire_or_recover_interrupted(")
    if (
        "Self::acquire(workspace, device_id, epoch)" not in lock_admission
        or "Self::recover_interrupted_launch(workspace, device_id, epoch)" not in lock_admission
    ):
        failures.append("native Open must hold the existing per-profile OS admission through the shared recovery owner")
    recovery = function_body(locks, "pub(crate) fn recover_interrupted_launch(")
    if (
        "generation.try_lock().map_err(map_try_lock_error)?" not in recovery
        or "browser_writer_locks_absent(workspace)" not in recovery
        or "filesystem::BridgeWorkspaceLock::replace_interrupted(" not in recovery
    ):
        failures.append("native Open must hold the existing per-profile OS admission through the shared recovery owner")
    if opened.count("browser_writer_locks_absent(&workspace)") < 2:
        failures.append("native Open must verify the existing Firefox file-access gate")

    if (
        "pub fn sync_closed_local_draft<" not in generation
        or generation.count("BridgeWorkspaceLock::acquire_or_recover_interrupted(") != 3
    ):
        failures.append("closed DRAFT Sync must use the shared same-device Bridge recovery owner")
    if (
        "fn sync_closed_ready(" not in shipping
        or shipping.count("BridgeWorkspaceLock::acquire_or_recover_interrupted(") != 3
    ):
        failures.append("closed READY Sync must use the shared same-device Bridge recovery owner")
    if (
        "pub fn reconcile_closed_draft_with_confirmed_current<" not in authoritative
        or "pub(crate) fn reconcile_closed_ready_with_confirmed_current<" not in authoritative
        or authoritative.count("BridgeWorkspaceLock::acquire_or_recover_interrupted(") != 2
    ):
        failures.append("confirmed-current DRAFT/READY reconciliation must use the shared Bridge recovery owner")

    if "current.profile_version() != intent.expected_version" not in local:
        failures.append("stale local CURRENT must fail closed before browser launch")

    for required in ('from playwright.async_api import async_playwright',
                     'from camoufox.async_api import AsyncNewBrowser',
                     'persistent_context=True', 'self.context.on("close"',
                     'self._browser.on("disconnected"'):
        if required not in native:
            failures.append(f"official native SDK/Playwright integration missing: {required}")
    for forbidden in ("WindowsBrowserExitWitness", "has_visible_window(",
                      "EnumWindows(", "sessionCheckpoints.json", "context.close("):
        if forbidden in native:
            failures.append(f"custom native browser lifecycle is forbidden: {forbidden}")

    if 'return asyncio.run(run_local_profile(Path(sys.argv[2])))' not in adapter:
        failures.append("user-native adapter must select exactly the local SDK path")
    if 'print("user-native runtime requires --local-open"' not in adapter:
        failures.append("user-native path must reject the legacy automation IPC")

    if '#[cfg(any(test, feature = "synthetic-test-bin"))]\npub struct ProfileBridgeOperator' not in operator:
        failures.append("retired operator must remain test/synthetic only")
    if '#[cfg(any(test, feature = "synthetic-test-bin"))]\npub struct RuntimeSessionOrchestrator' not in sources.get("runtime", ""):
        failures.append("retired managed runtime orchestrator must remain test/synthetic only")

    return failures


def read(root: Path) -> dict[str, str]:
    sources = {key: (root / path).read_text(encoding="utf-8") for key, path in SOURCES.items()}
    sources["runtime"] = (root / "apps/profile-bridge/src/runtime_bundle.rs").read_text(encoding="utf-8")
    return sources


def self_test(sources: dict[str, str]) -> list[str]:
    failures: list[str] = []
    mutations = (
        ("local", "mark_local_unsynced(workspace.path())?;", "mark_removed();",
         "durable local dirty evidence"),
        ("local", "BridgeWorkspaceLock::acquire_or_recover_interrupted(&workspace",
         "BridgeWorkspaceLock::acquire(&workspace",
         "native Open must hold the existing per-profile OS admission through the shared recovery owner"),
        ("generation", "BridgeWorkspaceLock::acquire_or_recover_interrupted(&workspace",
         "BridgeWorkspaceLock::acquire(&workspace",
         "closed DRAFT Sync must use the shared same-device Bridge recovery owner"),
        ("shipping", "BridgeWorkspaceLock::acquire_or_recover_interrupted(",
         "BridgeWorkspaceLock::acquire(",
         "closed READY Sync must use the shared same-device Bridge recovery owner"),
        ("authoritative", "BridgeWorkspaceLock::acquire_or_recover_interrupted(",
         "BridgeWorkspaceLock::acquire(",
         "confirmed-current DRAFT/READY reconciliation must use the shared Bridge recovery owner"),
        ("locks", "Self::acquire(workspace, device_id, epoch)", "Ok(())",
         "native Open must hold the existing per-profile OS admission through the shared recovery owner"),
        ("locks", "Self::recover_interrupted_launch(workspace, device_id, epoch)", "Ok(())",
         "native Open must hold the existing per-profile OS admission through the shared recovery owner"),
        ("local", "current.profile_version() != intent.expected_version", "false",
         "stale local CURRENT"),
        ("shipping", "wait_for_native_writer_release(", "unsafe_file_assumption(",
         "native file-release proof"),
        ("native", 'self.context.on("close"', 'self.context.on("missing"',
         "official native SDK/Playwright integration"),
        ("adapter", "return asyncio.run(run_local_profile(Path(sys.argv[2])))",
         "return run_ipc()", "user-native adapter"),
        ("operator", '#[cfg(any(test, feature = "synthetic-test-bin"))]\npub struct ProfileBridgeOperator',
         "pub struct ProfileBridgeOperator", "retired operator"),
    )
    for owner, before, after, expected in mutations:
        if before not in sources[owner]:
            failures.append(f"native ownership negative-fixture source missing: {owner} / {before}")
            continue
        modified = dict(sources)
        modified[owner] = modified[owner].replace(before, after)
        if not any(expected in failure for failure in check_sources(modified)):
            failures.append(f"native ownership negative fixture unexpectedly passed: {expected}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        sources = read(root)
    except OSError as error:
        print(f"ERROR: native ProfileBridge ownership sources unavailable: {error}")
        return 1
    failures = self_test(sources) if args.self_test else check_sources(sources)
    if failures:
        for failure in failures:
            print(f"ERROR: {failure}")
        return 1
    print("Phase 2F native single-owner boundary and negative fixtures passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
