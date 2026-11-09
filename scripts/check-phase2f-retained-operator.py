#!/usr/bin/env python3
"""Phase 2F ownership guard for the accepted native Camoufox/Playwright model.

The predecessor RetainedDirtyClose operator is a synthetic/unit-test fixture,
not the user-facing browser or Save owner. Guard the real shipping Open/Close/
Sync chain instead of requiring the retired production operator to return.
"""

from __future__ import annotations

import argparse
import re
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


LOCK_ACQUIRE = "BridgeWorkspaceLock::acquire_or_recover_interrupted("
LOCK_BINDING = re.compile(
    r"\blet\s+(writer|guard|lock)\s*=\s*" + re.escape(LOCK_ACQUIRE)
)


def guarded_effect(body: str, effect: str, *, require_native_gate: bool = False) -> bool:
    """Every matching filesystem effect must stay inside its own existing OS lock.

    This checks the ordering (acquire -> native file proof -> effect -> release),
    not the number of scopes; retries and exact-Commit reconciliation can add
    legitimately guarded operations without introducing another lock owner.
    """
    positions = [match.start() for match in re.finditer(re.escape(effect), body)]
    if not positions:
        return False
    admissions = list(LOCK_BINDING.finditer(body))
    for position in positions:
        earlier = [match for match in admissions if match.end() <= position]
        if not earlier:
            return False
        admitted = earlier[-1]
        name = admitted.group(1)
        release = re.compile(r"\b" + re.escape(name) + r"\s*\.release\s*\(")
        inside = body[admitted.end():position]
        # The latest matching admission must still be live at the effect.
        if release.search(inside):
            return False
        if require_native_gate and "browser_writer_locks_absent(" not in inside:
            return False
        rest = body[position:]
        released = release.search(rest)
        if released is None or LOCK_BINDING.search(rest[:released.start()]):
            return False
    return True


def guarded_effects(body: str, effects: tuple[tuple[str, bool], ...]) -> bool:
    return bool(body) and all(
        guarded_effect(body, effect, require_native_gate=native)
        for effect, native in effects
    )


def mutate_one_admission(source: str, marker: str, ordinal: int) -> str:
    """Negative fixture: bypass exactly ONE scoped recovery admission."""
    body = function_body(source, marker)
    admissions = list(LOCK_BINDING.finditer(body))
    selected = admissions[ordinal]
    original = body
    body = (
        body[:selected.start()]
        + body[selected.start():selected.end()].replace(
            LOCK_ACQUIRE, "BridgeWorkspaceLock::acquire("
        )
        + body[selected.end():]
    )
    return source.replace(original, body, 1)


def mutation_guard_coverage(
    sources: dict[str, str],
    owner: str,
    marker: str,
    expected_failure: str,
) -> list[str]:
    """Removing ANY admission from a protected phase must fail the gate."""
    failures: list[str] = []
    body = function_body(sources[owner], marker)
    count = len(list(LOCK_BINDING.finditer(body)))
    if count == 0:
        return [f"negative fixture has no admissions: {owner}/{marker}"]
    for ordinal in range(count):
        changed = dict(sources)
        changed[owner] = mutate_one_admission(sources[owner], marker, ordinal)
        if not any(expected_failure in failure for failure in check_sources(changed)):
            failures.append(
                f"unguarded phase unexpectedly accepted: {owner}/{marker} #{ordinal + 1}"
            )
    return failures


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

    opened = function_body(local, "pub(crate) fn open_with_health(")
    marker = opened.find("mark_local_unsynced(workspace.path())?;")
    launch = opened.find("run_python(", marker)
    launch_call = opened[launch : launch + 256] if launch >= 0 else ""
    if (
        marker < 0
        or launch < marker
        or '"--local-open"' not in launch_call
        or "pending_health" not in launch_call
    ):
        failures.append("durable local dirty evidence and health handoff must precede native browser launch")
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

    draft = function_body(generation, "pub fn sync_closed_local_draft<")
    ready = function_body(shipping, "fn sync_closed_ready(")
    draft_effects = (
        ("root.clear_profile_current_pending_sync(&current)", True),
        ("digest_closed_workspace_streaming(&workspace, &inventory)", True),
        ("seal_closed_workspace_streaming(", True),
        ("root.set_profile_current_pending_sync(&workspace", True),
        ("finish_pending_sync_under_writer(root, &current", True),
    )
    ready_effects = (
        ("finish_pending_sync_under_writer(root, &active", False),
        ("root.clear_profile_current_pending_sync(&active)", True),
        ("digest_closed_workspace_streaming(&workspace, &inventory)", True),
        ("seal_closed_workspace_streaming(", True),
        ("root.set_profile_current_pending_sync(&workspace", True),
    )
    if not guarded_effects(draft, draft_effects):
        failures.append("closed DRAFT Sync must use the shared same-device Bridge recovery owner")
    if not guarded_effects(ready, ready_effects):
        failures.append("closed READY Sync must use the shared same-device Bridge recovery owner")

    # The invoked finalizer must also enforce the native gate itself: the
    # READY exact-CURRENT recovery path calls it without a duplicate outer probe.
    finish = function_body(generation, "pub(crate) fn finish_pending_sync_under_writer(")
    for requirement in (
        "workspace.pending_sync() != Some(pending)",
        "browser_writer_locks_absent(workspace)",
        "root.mark_profile_current_pending_sync_committed(workspace)",
        "root.clear_profile_current_pending_sync(&committed)",
    ):
        if requirement not in finish:
            failures.append(f"exact committed Sync finalizer lost its native/marker gate: {requirement}")

    classic_restore = function_body(authoritative, "fn ensure_authoritative_generation_to<")
    streamed_restore = function_body(authoritative, "fn materialize_streamed_authoritative_generation<")
    if (
        "pub fn reconcile_closed_draft_with_confirmed_current<" not in authoritative
        or "pub(crate) fn reconcile_closed_ready_with_confirmed_current<" not in authoritative
        or not guarded_effects(
            classic_restore,
            (("verify_workspace_matches_snapshot(", True),
             ("confirm_closed_current_matches_cloud(", True)),
        )
        or not guarded_effects(
            streamed_restore,
            (("verify_workspace_matches_streamed_snapshot(", True),
             ("confirm_closed_current_matches_cloud(", True)),
        )
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
    for owner, marker, expected in (
        ("generation", "pub fn sync_closed_local_draft<",
         "closed DRAFT Sync must use the shared same-device Bridge recovery owner"),
        ("shipping", "fn sync_closed_ready(",
         "closed READY Sync must use the shared same-device Bridge recovery owner"),
        ("authoritative", "fn ensure_authoritative_generation_to<",
         "confirmed-current DRAFT/READY reconciliation must use the shared Bridge recovery owner"),
        ("authoritative", "fn materialize_streamed_authoritative_generation<",
         "confirmed-current DRAFT/READY reconciliation must use the shared Bridge recovery owner"),
    ):
        failures.extend(mutation_guard_coverage(sources, owner, marker, expected))

    # Removing one native-file gate must not be masked by another guarded phase.
    for owner, marker, omitted, expected in (
        ("generation", "pub fn sync_closed_local_draft<",
         "browser_writer_locks_absent(&workspace)",
         "closed DRAFT Sync must use the shared same-device Bridge recovery owner"),
        ("shipping", "fn sync_closed_ready(",
         "browser_writer_locks_absent(&workspace)",
         "closed READY Sync must use the shared same-device Bridge recovery owner"),
        ("generation", "pub(crate) fn finish_pending_sync_under_writer(",
         "browser_writer_locks_absent(workspace)",
         "exact committed Sync finalizer lost its native/marker gate"),
    ):
        body = function_body(sources[owner], marker)
        if omitted not in body:
            failures.append(f"native-gate negative fixture missing: {owner}/{marker}")
            continue
        changed = dict(sources)
        changed[owner] = sources[owner].replace(body, body.replace(omitted, "native_gate_skipped"), 1)
        if not any(expected in failure for failure in check_sources(changed)):
            failures.append(f"missing native gate unexpectedly accepted: {owner}/{marker}")

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
