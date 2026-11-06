#!/usr/bin/env python3
"""Offline native-only Camouhost IPC regression; no real Firefox or credentials."""
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
    spec = importlib.util.spec_from_file_location("native_camouhost_protocol_fixture", SOURCE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module

class NativeSession:
    def __init__(self):
        self.calls = 0
    def profile_state(self):
        self.calls += 1
        # Launch succeeds upon first OS-held writer. The first observer sees
        # Firefox still holding its profile; second sees released profile.
        return ("PROFILE_BUSY" if self.calls < 3 else
                "QUIESCENT_REQUIRES_FILE_VALIDATION")

def main():
    r = load_runtime()
    native = types.ModuleType("native_process")
    class NativeLaunchError(RuntimeError):
        pass
    native.NativeLaunchError = NativeLaunchError
    called = []
    def launch_native_camoufox(**kwargs):
        called.append(kwargs)
        return NativeSession()
    native.launch_native_camoufox = launch_native_camoufox
    old = {
        "load_runtime_lock": lambda: ({}, "x" * 64),
        "verify_python_components": lambda _: None,
        "resolve_profile_root": lambda require_bridge_lock: pathlib.Path("/tmp/test-root"),
        "load_generation_config": lambda root: ({"test": True}, "a" * 64),
        "firefox_writer_active": lambda root: False,
        "resolve_proxy": lambda: None,
        "packaged_windows_browser": lambda lock: pathlib.Path("/tmp/camoufox.exe"),
        "browser_environment": lambda: {"SYSTEMROOT": "/system"},
    }
    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(r, **old), \
         patch.dict(os.environ, {r.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \
         patch.object(sys, "stdin", io.StringIO(
             f"hello|{r.IPC_VERSION}\nlaunch|{SESSION}\n"
             f"observe_close|{SESSION}\nobserve_close|{SESSION}\n"
             f"close|{SESSION}\n"
         )), contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as stdout:
        result = r.run_native_ipc()
    lines = stdout.buffer.getvalue().decode("utf-8").splitlines()
    assert result == 0, result
    assert lines == [
        f"hello_ack|{r.IPC_VERSION}",
        f"ready|{SESSION}",
        f"close_observed|{SESSION}|false",
        f"close_observed|{SESSION}|true",
        f"closed|{SESSION}|true",
    ], lines
    assert len(called) == 1 and called[0]["admission_granted"] is True
    assert called[0]["identity"] == {"test": True}

    # A missing native writer must NEVER be classified as closed/Saved.
    class MissingWriter:
        def profile_state(self):
            return "WRITER_NOT_OBSERVED"
    with patch.dict(sys.modules, {"native_process": native}), patch.multiple(r, **old), \
         patch.dict(os.environ, {r.EXPECTED_PROBE_SHA256_ENV: "b" * 64}), \
         patch.object(sys, "stdin", io.StringIO(
             f"hello|{r.IPC_VERSION}\nlaunch|{SESSION}\n"
         )), patch.object(r.time, "monotonic", side_effect=[0.0, 32.0]), \
         patch.object(native, "launch_native_camoufox", return_value=MissingWriter()), \
         contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding="utf-8")) as failed:
        bad = r.run_native_ipc()
    assert bad == 5 and failed.buffer.getvalue().decode("utf-8").splitlines()[-1] == "error|runtime"

    print("CAMOUHOST_NATIVE_IPC_BOUNDARY=PASS")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
