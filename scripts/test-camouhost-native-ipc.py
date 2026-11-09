#!/usr/bin/env python3
"""The retired native-user IPC path must not return as a second browser owner."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import sys
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "runtime" / "camouhost" / "real.py"


def main() -> int:
    spec = importlib.util.spec_from_file_location("camouhost_native_ipc_retired", SOURCE)
    assert spec is not None and spec.loader is not None
    runtime = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = runtime
    spec.loader.exec_module(runtime)

    assert not hasattr(runtime, "run_native_ipc")
    assert not hasattr(runtime, "run_native_ipc_async")
    with (
        patch.dict(os.environ, {runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE}, clear=True),
        patch.object(sys, "argv", ["real.py"]),
    ):
        assert runtime.main() == 2

    source = SOURCE.read_text(encoding="utf-8")
    local_runtime = source.split("async def run_local_profile", 1)[1].split("\n\ndef main", 1)[0]
    assert "wait_for_browser_close" in local_runtime
    assert "context.on(\"close\"" in source
    assert "profile_state" not in local_runtime
    for forbidden in (
        "WindowsBrowserExitWitness",
        "firefox_writer_active",
        "wait_for_browser_quiescence",
        "EnumWindows",
        "CreateToolhelp32Snapshot",
        "WM_CLOSE",
        "PostMessage",
    ):
        assert forbidden not in source, f"retired browser observer remains: {forbidden}"
    print("CAMOUHOST_USER_PROFILE_CLOSE_EVENT=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
