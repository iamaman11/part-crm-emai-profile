#!/usr/bin/env python3
"""Permanent AR-10 failure matrix for real Firefox profile-writer ownership.

These tests intentionally exercise operating-system lock ownership rather than marker
existence. They never remove Firefox lock artifacts as part of the production runtime;
all cleanup happens only inside isolated temporary test directories.
"""

from __future__ import annotations

import ctypes
import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import ModuleType
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
REAL_RUNTIME = ROOT / "runtime/camouhost/real.py"


def load_runtime() -> ModuleType:
    spec = importlib.util.spec_from_file_location("ar10_real_camouhost_lock_test", REAL_RUNTIME)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load real Camouhost runtime")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


runtime = load_runtime()


class FirefoxWriterLockTests(unittest.TestCase):
    def profile_root(self, temporary: str) -> Path:
        root = Path(temporary) / "generation"
        (root / runtime.USER_DATA_NAME).mkdir(parents=True)
        return root

    def test_missing_runtime_locks_are_quiescent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            self.assertFalse(runtime.firefox_writer_active(self.profile_root(temporary)))

    def test_ambiguous_primary_probe_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            primary, _ = runtime.firefox_writer_locks(root)
            if os.name == "nt":
                primary.mkdir()
                with self.assertRaises(runtime.RuntimeContractError):
                    runtime.windows_parent_lock_is_active(primary)
            else:
                primary.write_bytes(b"")
                with mock.patch.object(
                    runtime,
                    "unix_parent_lock_is_active",
                    side_effect=runtime.RuntimeContractError("ambiguous probe"),
                ):
                    with self.assertRaises(runtime.RuntimeContractError):
                        runtime.firefox_writer_active(root)

    def test_quiescence_timeout_never_claims_clean_close(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            with (
                mock.patch.object(runtime, "BROWSER_CLOSE_QUIESCENCE_SECONDS", 0.0),
                mock.patch.object(runtime, "BROWSER_CLOSE_POLL_SECONDS", 0.0),
                mock.patch.object(runtime, "firefox_writer_active", return_value=True),
            ):
                with self.assertRaisesRegex(
                    runtime.RuntimeContractError,
                    "writer lock remained active",
                ):
                    runtime.wait_for_browser_quiescence(root)

    @unittest.skipUnless(os.name == "posix", "POSIX Firefox lock semantics")
    def test_posix_stale_primary_marker_is_not_writer_liveness(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            primary, _ = runtime.firefox_writer_locks(root)
            primary.write_bytes(b"")
            self.assertFalse(runtime.firefox_writer_active(root))

    @unittest.skipUnless(os.name == "posix", "POSIX Firefox lock semantics")
    def test_posix_real_fcntl_owner_is_busy(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            primary, _ = runtime.firefox_writer_locks(root)
            primary.write_bytes(b"")
            child = subprocess.Popen(
                [
                    sys.executable,
                    "-c",
                    (
                        "import fcntl, os, sys; "
                        "fd=os.open(sys.argv[1], os.O_RDWR); "
                        "fcntl.lockf(fd, fcntl.LOCK_EX); "
                        "print('locked', flush=True); "
                        "sys.stdin.buffer.read(1)"
                    ),
                    str(primary),
                ],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                self.assertIsNotNone(child.stdout)
                self.assertEqual(child.stdout.readline().strip(), "locked")
                self.assertTrue(runtime.firefox_writer_active(root))
            finally:
                if child.stdin is not None:
                    child.stdin.write("x")
                    child.stdin.flush()
                    child.stdin.close()
                child.wait(timeout=5)
                stderr = child.stderr.read() if child.stderr is not None else ""
                self.assertEqual(child.returncode, 0, stderr)
            self.assertFalse(runtime.firefox_writer_active(root))

    @unittest.skipUnless(os.name == "posix" and sys.platform != "darwin", "Linux legacy lock semantics")
    def test_legacy_marker_without_primary_is_conservatively_busy(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            _primary, legacy = runtime.firefox_writer_locks(root)
            self.assertIsNotNone(legacy)
            assert legacy is not None
            legacy.symlink_to("localhost:+1234")
            self.assertTrue(runtime.firefox_writer_active(root))

    @unittest.skipUnless(os.name == "posix" and sys.platform != "darwin", "Linux legacy lock semantics")
    def test_obsolete_legacy_pid_symlink_is_stale_only_after_primary_is_free(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            primary, legacy = runtime.firefox_writer_locks(root)
            self.assertIsNotNone(legacy)
            assert legacy is not None
            primary.write_bytes(b"")
            legacy.symlink_to("localhost:+1234")
            self.assertFalse(runtime.firefox_writer_active(root))
            self.assertTrue(legacy.is_symlink(), "runtime must not delete Firefox lock artifacts")

    @unittest.skipUnless(os.name == "posix", "POSIX Firefox lock semantics")
    def test_primary_symlink_shape_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            primary, _ = runtime.firefox_writer_locks(root)
            primary.symlink_to("unsupported-target")
            with self.assertRaises(runtime.RuntimeContractError):
                runtime.firefox_writer_active(root)

    @unittest.skipUnless(os.name == "nt", "Windows Firefox lock semantics")
    def test_windows_stale_regular_marker_is_not_writer_liveness(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            primary, _ = runtime.firefox_writer_locks(root)
            primary.write_bytes(b"")
            self.assertFalse(runtime.windows_parent_lock_is_active(primary))
            self.assertFalse(runtime.firefox_writer_active(root))

    @unittest.skipUnless(os.name == "nt", "Windows Firefox lock semantics")
    def test_windows_exclusive_owner_maps_sharing_violation_to_busy(self) -> None:
        from ctypes import wintypes

        with tempfile.TemporaryDirectory() as temporary:
            root = self.profile_root(temporary)
            primary, _ = runtime.firefox_writer_locks(root)
            primary.write_bytes(b"")
            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            create_file = kernel32.CreateFileW
            create_file.argtypes = [
                wintypes.LPCWSTR,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.LPVOID,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.HANDLE,
            ]
            create_file.restype = wintypes.HANDLE
            close_handle = kernel32.CloseHandle
            close_handle.argtypes = [wintypes.HANDLE]
            close_handle.restype = wintypes.BOOL
            generic_read_write = 0x80000000 | 0x40000000
            open_existing = 3
            handle = create_file(
                str(primary),
                generic_read_write,
                0,
                None,
                open_existing,
                0,
                None,
            )
            invalid = ctypes.c_void_p(-1).value
            self.assertNotEqual(handle, invalid, f"failed to establish exclusive owner: {ctypes.get_last_error()}")
            try:
                self.assertTrue(runtime.windows_parent_lock_is_active(primary))
                self.assertTrue(runtime.firefox_writer_active(root))
            finally:
                self.assertTrue(close_handle(handle))
            self.assertFalse(runtime.firefox_writer_active(root))


if __name__ == "__main__":
    unittest.main(verbosity=2)
