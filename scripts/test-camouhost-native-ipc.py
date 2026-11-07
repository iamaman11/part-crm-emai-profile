#!/usr/bin/env python3
"""Offline user-session IPC regression; no real Firefox or credentials."""
from __future__ import annotations

import contextlib
import importlib.util
import io
import os
import pathlib
import sys
import types
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "runtime/camouhost/real.py"
SESSION = "session_01JAR10CLOSE"


def load_runtime():
    spec = importlib.util.spec_from_file_location("camouhost_user_ipc_fixture", SOURCE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class UserSession:
    def __init__(self, states):
        self.states = list(states)
        self.last = "WRITER_NOT_OBSERVED"
        self.closed = 0
        self.detached_after_exit = 0
        self.aborted_startup = 0

    def profile_state(self):
        if self.states:
            self.last = self.states.pop(0)
        return self.last

    async def close(self):
        self.closed += 1

    async def close_after_browser_exit(self):
        self.detached_after_exit += 1

    async def abort_startup(self):
        self.aborted_startup += 1


def main():
    runtime = load_runtime()
    native = types.ModuleType("native_process")

    class NativeLaunchError(RuntimeError):
        pass

    native.NativeLaunchError = NativeLaunchError
    called = []
    sessions = [UserSession([
        "PROFILE_BUSY", "PROFILE_BUSY",
        "CLEAN_SHUTDOWN_VERIFIED", "CLEAN_SHUTDOWN_VERIFIED",
    ])]

    async def launch_native_camoufox(**kwargs):
        called.append(kwargs)
        return sessions.pop(0)

    native.launch_native_camoufox = launch_native_camoufox
    old = {
        "load_runtime_lock": lambda: ({"browser": {"version": "158.0.1-beta.41"}}, "x" * 64),
        "verify_python_components": lambda _: None,
        "resolve_profile_root": lambda require_bridge_lock: pathlib.Path("/tmp/test-root"),
        "load_generation_config": lambda root: ({"test": True}, "a" * 64),
        "firefox_writer_active": lambda root: False,
        "resolve_headless_mode": lambda: False,
        "camoufox_launch_options": lambda lock, root, config, isolated=False: {
            "approved": "official-sdk-options",
            "user_data_dir": str(root / "user_data"),
        },
    }

    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(runtime, **old), \
         patch.dict(os.environ, {runtime.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \
         patch.object(sys, "stdin", io.StringIO(
             f"hello|{runtime.IPC_VERSION}\nlaunch|{SESSION}\n"
             f"observe_close|{SESSION}\nobserve_close|{SESSION}\n"
             f"close|{SESSION}\n"
         )), contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as stdout:
        result = runtime.run_native_ipc()
    lines = stdout.buffer.getvalue().decode("utf-8").splitlines()
    assert result == 0, result
    assert lines == [
        f"hello_ack|{runtime.IPC_VERSION}",
        f"ready|{SESSION}",
        f"close_observed|{SESSION}|false",
        f"close_observed|{SESSION}|true",
        f"closed|{SESSION}|true",
    ], lines
    assert len(called) == 1
    assert called[0]["admission_granted"] is True
    assert called[0]["options"]["approved"] == "official-sdk-options"
    assert called[0]["headless"] is False
    assert sessions == []

    crashed_session = UserSession([
        "PROFILE_BUSY", "CRASH_OR_UNKNOWN_PRESERVE_LOCAL",
        "CRASH_OR_UNKNOWN_PRESERVE_LOCAL",
    ])
    sessions.append(crashed_session)
    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(runtime, **old), \
         patch.dict(os.environ, {runtime.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \
         patch.object(sys, "stdin", io.StringIO(
             f"hello|{runtime.IPC_VERSION}\nlaunch|{SESSION}\n"
             f"observe_close|{SESSION}\nclose|{SESSION}\n"
         )), contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as crashed:
        crash_result = runtime.run_native_ipc()
    crash_lines = crashed.buffer.getvalue().decode("utf-8").splitlines()
    assert crash_result == 6, crash_result
    assert crash_lines == [
        f"hello_ack|{runtime.IPC_VERSION}",
        f"ready|{SESSION}",
        "error|runtime",
        f"closed|{SESSION}|false",
    ], crash_lines
    assert crashed_session.detached_after_exit == 1

    class MissingWriter(UserSession):
        def __init__(self):
            super().__init__(["WRITER_NOT_OBSERVED"])

    missing = MissingWriter()
    sessions.append(missing)
    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(runtime, **old), \
         patch.dict(os.environ, {runtime.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \
         patch.object(sys, "stdin", io.StringIO(
             f"hello|{runtime.IPC_VERSION}\nlaunch|{SESSION}\n"
         )), patch.object(runtime, "time", types.SimpleNamespace(monotonic=(iter([0.0, 32.0])).__next__)), \
         contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as failed:
        bad = runtime.run_native_ipc()
    assert bad == 5
    assert failed.buffer.getvalue().decode("utf-8").splitlines()[-1] == "error|runtime"
    assert missing.aborted_startup == 1

    # A lost Bridge pipe must not close the persistent context while Firefox
    # still owns its profile. The SDK owner is released only after lock release.
    orphaned = UserSession([
        "PROFILE_BUSY", "PROFILE_BUSY",
        "CLEAN_SHUTDOWN_VERIFIED", "CLEAN_SHUTDOWN_VERIFIED",
    ])
    sessions.append(orphaned)
    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(runtime, **old), \
         patch.dict(os.environ, {runtime.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \
         patch.object(sys, "stdin", io.StringIO(
             f"hello|{runtime.IPC_VERSION}\nlaunch|{SESSION}\n"
         )), contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as orphan_output:
        orphan_result = runtime.run_native_ipc()
    orphan_lines = orphan_output.buffer.getvalue().decode("utf-8").splitlines()
    assert orphan_result == 3, orphan_result
    assert orphan_lines == [
        f"hello_ack|{runtime.IPC_VERSION}",
        f"ready|{SESSION}",
    ], orphan_lines
    assert orphaned.closed == 1

    orphaned_crash = UserSession([
        "PROFILE_BUSY", "PROFILE_BUSY", "CRASH_OR_UNKNOWN_PRESERVE_LOCAL",
    ])
    sessions.append(orphaned_crash)
    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(runtime, **old), \
         patch.dict(os.environ, {runtime.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \
         patch.object(sys, "stdin", io.StringIO(
             f"hello|{runtime.IPC_VERSION}\nlaunch|{SESSION}\n"
         )), contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as orphan_crash_output:
        orphan_crash_result = runtime.run_native_ipc()
    assert orphan_crash_result == 3, orphan_crash_result
    assert orphaned_crash.detached_after_exit == 1

    # The original user-native regression: Playwright's asyncio dispatcher must
    # remain runnable while Rust has not sent the next IPC command. A sync
    # sys.stdin.readline() on the owner thread starves this ticker completely.
    class DelayedFrames:
        def __init__(self):
            self.lines = [
                f"hello|{runtime.IPC_VERSION}\\n",
                f"launch|{SESSION}\\n",
                f"observe_close|{SESSION}\\n",
                f"close|{SESSION}\\n",
            ]
            self.index = 0

        def readline(self):
            import time as wallclock
            if self.index == 2:
                wallclock.sleep(0.25)
            line = self.lines[self.index] if self.index < len(self.lines) else ""
            self.index += 1
            return line

    dispatched = []
    idle_session = UserSession([
        "PROFILE_BUSY", "CLEAN_SHUTDOWN_VERIFIED", "CLEAN_SHUTDOWN_VERIFIED",
    ])

    async def launch_with_idle_dispatch(**_kwargs):
        async def event_ticker():
            for _ in range(12):
                await asyncio.sleep(0.02)
                dispatched.append(True)

        asyncio.create_task(event_ticker())
        return idle_session

    native.launch_native_camoufox = launch_with_idle_dispatch
    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(runtime, **old), \\
         patch.dict(os.environ, {runtime.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \\
         patch.object(sys, "stdin", DelayedFrames()), \\
         contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as live_output:
        idle_result = runtime.run_native_ipc()
    assert idle_result == 0
    assert len(dispatched) >= 5, "native Playwright dispatcher was starved during idle IPC"
    assert idle_session.closed == 1
    assert live_output.buffer.getvalue().decode("utf-8").splitlines() == [
        f"hello_ack|{runtime.IPC_VERSION}",
        f"ready|{SESSION}",
        f"close_observed|{SESSION}|true",
        f"closed|{SESSION}|true",
    ]

    print("CAMOUHOST_USER_PLAYWRIGHT_IPC=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
