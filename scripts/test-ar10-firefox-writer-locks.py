#!/usr/bin/env python3
"""The native Firefox file-lock owner is Rust/ProfileBridge, not Camouhost."""

from __future__ import annotations

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BROWSER_EXECUTION = ROOT / "apps/profile-bridge/src/browser_execution.rs"
LOCAL_OPEN = ROOT / "apps/profile-bridge/src/local_browser_open.rs"
GENERATION_INIT = ROOT / "apps/profile-bridge/src/generation_initialization.rs"
SHIPPING = ROOT / "apps/profile-bridge/src/shipping_composition.rs"
REAL_RUNTIME = ROOT / "runtime/camouhost/real.py"
NATIVE_PROCESS = ROOT / "runtime/camouhost/native_process.py"


def source(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class RustFirefoxWriterLockOwnershipTests(unittest.TestCase):
    def test_rust_owns_real_native_lock_probe_and_os_regressions(self) -> None:
        browser = source(BROWSER_EXECUTION)
        self.assertIn("browser_writer_locks_absent_at", browser)
        self.assertIn("observed_browser_writer_locks_at", browser)
        self.assertIn("windows_parent_lock_requires_live_exclusive_file_handle_to_block", browser)
        self.assertIn("unix_unknown_firefox_marker_never_authorizes_clean_release", browser)
        self.assertIn("browser_lock_is_never_deleted_and_requires_recovery", browser)

    def test_local_open_checks_native_file_access_before_and_after_camouhost(self) -> None:
        opened = source(LOCAL_OPEN)
        self.assertGreaterEqual(opened.count("browser_writer_locks_absent(&workspace)"), 3)
        self.assertIn('"--materialize-identity"', opened)
        self.assertIn('"--local-open"', opened)
        self.assertIn("BrowserNotClosedCleanly", opened)

    def test_existing_sync_rechecks_native_file_access_before_snapshot(self) -> None:
        initialized = source(GENERATION_INIT)
        shipping = source(SHIPPING)
        self.assertIn("browser_writer_locks_absent", initialized)
        self.assertIn("sync_after_native_close", shipping)
        self.assertIn("start_sync_after_close", shipping)
        self.assertIn("--sync-closed", shipping)

    def test_camouhost_contains_no_native_file_lock_or_process_window_observer(self) -> None:
        for path in (REAL_RUNTIME, NATIVE_PROCESS):
            text = source(path)
            for forbidden in (
                "firefox_writer_active",
                "wait_for_browser_quiescence",
                "WindowsBrowserExitWitness",
                "EnumWindows",
                "CreateToolhelp32Snapshot",
                "WM_CLOSE",
                "PostMessage",
            ):
                with self.subTest(path=path.name, forbidden=forbidden):
                    self.assertNotIn(forbidden, text)

    def test_official_playwright_close_event_is_the_only_runtime_signal(self) -> None:
        native = source(NATIVE_PROCESS)
        real = source(REAL_RUNTIME)
        self.assertIn('context.on("close"', native)
        self.assertIn("wait_for_browser_close", native)
        self.assertIn('context.on("close"', real)
        self.assertIn("context_close_observed", real)


if __name__ == "__main__":
    unittest.main(verbosity=2)
