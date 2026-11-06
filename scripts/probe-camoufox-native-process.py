#!/usr/bin/env python3
"""Opt-in physical native Camoufox launch on Windows, using a disposable workspace.

Takes a *read-only* fingerprint config from a test generation, but never opens
the original Firefox user_data, signs in to any site, logs cookies, or runs
Playwright. This probe does NOT claim ordinary CRM access or Save acceptance.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ValueError("source module unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser", type=Path, required=True)
    parser.add_argument("--fingerprint", type=Path, required=True)
    parser.add_argument("--installed-real", type=Path, required=True)
    parser.add_argument("--hold-seconds", type=int, default=4)
    args = parser.parse_args()
    if sys.platform != "win32" or not 1 <= args.hold_seconds <= 15:
        print("NATIVE_CAMOUFOX_PROBE=PLATFORM_UNSUPPORTED")
        return 2
    if any(not x.is_absolute() or not x.is_file() for x in (
        args.browser, args.fingerprint, args.installed_real
    )):
        print("NATIVE_CAMOUFOX_PROBE=INVALID_SOURCE")
        return 2

    source = Path(__file__).resolve().parents[1] / "runtime/camouhost/native_process.py"
    native = load_module("g5_native_probe_process", source)
    real = load_module("g5_native_probe_shipping", args.installed_real)
    # The existing generation's fingerprint is read but never modified.
    config = json.loads(args.fingerprint.read_text(encoding="utf-8"))
    if not isinstance(config, dict):
        raise ValueError("existing test fingerprint config invalid")
    with tempfile.TemporaryDirectory(prefix="g5-native-camoufox-") as scratch:
        workspace = Path(scratch)
        (workspace / "user_data").mkdir()
        handle = native.launch_native_camoufox(
            workspace=workspace,
            browser=args.browser,
            identity=config,
            inherited=real.browser_environment(),
            writer_active=real.firefox_writer_active,
            admission_granted=True,  # Authorized disposable diagnostic, NOT CRM.
        )
        try:
            time.sleep(args.hold_seconds)
            running = handle.exit_state() == "RUNNING"
            print(f"NATIVE_CAMOUFOX_PROCESS_RUNNING={'PASS' if running else 'FAIL'}")
            print(f"NATIVE_FIREFOX_WRITER_ACTIVE={int(real.firefox_writer_active(workspace))}")
        finally:
            # Stop only the disposable test's own process tree, never any CRM
            # browser or a historical test user's running native profile.
            if handle.process.poll() is None:
                subprocess.run(
                    ["taskkill", "/T", "/F", "/PID", str(handle.process.pid)],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False, timeout=8,
                )
                handle.process.wait(timeout=8)
        state = handle.exit_state()
        print(f"NATIVE_CAMOUFOX_FORCED_TEST_EXIT={state}")
        return 0 if running and state == "ABNORMAL_EXIT_PRESERVE_LOCAL" else 1


if __name__ == "__main__":
    raise SystemExit(main())
