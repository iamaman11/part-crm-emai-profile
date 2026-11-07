#!/usr/bin/env python3
"""Explicit official user persistent-context vs page-inspecting automation routing contract.

Offline negative tests: no browser launch, no test credentials, no cloud calls.
The real browser/lease/Save invariants require separate physical acceptance.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import os
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "runtime" / "camouhost" / "real.py"


def load_runtime():
    spec = importlib.util.spec_from_file_location("camouhost_execution_modes_test", SOURCE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def verify_embedded_python_path_configuration():
    builder_path = SOURCE.parents[2] / "scripts" / "camouhost-runtime-build-windows.py"
    spec = importlib.util.spec_from_file_location("camouhost_runtime_builder_test", builder_path)
    assert spec is not None and spec.loader is not None
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    with tempfile.TemporaryDirectory() as directory:
        python_root = Path(directory) / "python"
        python_root.mkdir()
        path_file = python_root / "python312._pth"
        path_file.write_text("original", encoding="utf-8")
        builder.rewrite_embedded_python_path(python_root)
        expected = "python312.zip\n.\nLib\\site-packages\n..\\camouhost\nimport site\n"
        assert path_file.read_text(encoding="utf-8") == expected


def main() -> int:
    runtime = load_runtime()
    verify_embedded_python_path_configuration()
    # First user Open asks Firefox itself for its standard blank initial
    # tab. No app-controlled Playwright Page command is ever sent. Reopen
    # receives no startup flags, preserving Firefox's native saved tabs.
    with tempfile.TemporaryDirectory() as directory:
        fresh = Path(directory) / "user_data"
        fresh.mkdir()
        assert runtime.firefox_native_first_run_args(fresh) == ["-new-tab", "about:blank"]
        (fresh / "places.sqlite").write_bytes(b"synthetic existing Firefox state")
        assert runtime.firefox_native_first_run_args(fresh) == []

    invoked = []

    def native():
        invoked.append("native")
        return 17

    def automation():
        invoked.append("automation")
        return 18

    def run(env, expected, called):
        invoked.clear()
        error = io.StringIO()
        with (
            patch.dict(os.environ, env, clear=True),
            patch.object(sys, "argv", ["real.py"]),
            patch.object(runtime, "run_native_ipc", side_effect=native),
            patch.object(runtime, "run_ipc", side_effect=automation),
            contextlib.redirect_stderr(error),
        ):
            outcome = runtime.main()
        assert outcome == expected, (env, outcome, expected)
        assert invoked == called, (env, invoked, called)
        return error.getvalue()

    # No implicit managed browser launch when a mode flag is missing/old/unknown.
    for env in ({}, {"CAMOUHOST_NATIVE_PROFILE": "1"},
                {runtime.EXECUTION_MODE_ENV: ""},
                {runtime.EXECUTION_MODE_ENV: "native"},
                {runtime.EXECUTION_MODE_ENV: "playwright"}):
        assert run(env, 2, []), "fail-closed dispatch must explain rejection"

    # Normal CRM user Open uses the official Playwright persistent context,
    # never the page-inspecting automation/admission path.
    run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE}, 17, ["native"])
    run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE,
         runtime.HEADLESS_MODE_ENV: "false"}, 17, ["native"])
    assert run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE,
                runtime.AGENT_DIRECTORY_ENV: "/tmp/never-run-scripts"},
               2, []), "native user context must never execute local Python scripts"
    for invalid in ("true", "virtual", "1", ""):
        assert run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE,
                    runtime.HEADLESS_MODE_ENV: invalid}, 2, []), "no headless fallback"

    # The old managed SDK remains reachable *only* through the explicit
    # automation execution mode. It is not wired to shipping Windows Open.
    assert run({runtime.EXECUTION_MODE_ENV: runtime.AUTOMATION_PLAYWRIGHT_MODE},
               2, []), "automation must not open a managed context without a job"
    run({runtime.EXECUTION_MODE_ENV: runtime.AUTOMATION_PLAYWRIGHT_MODE,
         runtime.AGENT_DIRECTORY_ENV: "/tmp/authorized-test-inbox"},
        18, ["automation"])
    # C09 runs the official SDK on an isolated, exclusively owned R2
    # historical restore; no generic scripts or user-browser attachment.
    run({runtime.EXECUTION_MODE_ENV: runtime.ISOLATED_COLD_PROBE_MODE},
        18, ["automation"])
    for env in (
        {runtime.EXECUTION_MODE_ENV: runtime.ISOLATED_COLD_PROBE_MODE,
         runtime.AGENT_DIRECTORY_ENV: "/tmp/never-execute"},
        {runtime.EXECUTION_MODE_ENV: runtime.ISOLATED_COLD_PROBE_MODE,
         runtime.HEADLESS_MODE_ENV: "true"},
        {runtime.EXECUTION_MODE_ENV: runtime.ISOLATED_COLD_PROBE_MODE,
         runtime.HEADLESS_MODE_ENV: "virtual"},
    ):
        assert run(env, 2, []), "cold probe must not accept scripts or unverified display mode"

    for headless, expected in (("false", False), ("true", True),
                               ("virtual", "virtual")):
        with patch.dict(os.environ, {runtime.HEADLESS_MODE_ENV: headless}, clear=True):
            assert runtime.resolve_headless_mode() == expected
    with patch.dict(os.environ, {runtime.HEADLESS_MODE_ENV: "unknown"}, clear=True):
        try:
            runtime.resolve_headless_mode()
        except runtime.RuntimeContractError:
            pass
        else:
            raise AssertionError("unsupported headless mode accepted")

    # The shipping Windows launcher selects the user-owned SDK context explicitly.
    # This is source wiring coverage, not a substitute for a Rust build.
    rust = SOURCE.parents[2] / "apps/profile-bridge/src/camouhost_process.rs"
    shipping = rust.read_text(encoding="utf-8")
    assert 'RuntimeExecutionOwner::NativeUser => "user-native"' in shipping
    assert 'RuntimeExecutionOwner::IsolatedBrowserStateTest => "isolated-cold-probe"' in shipping
    assert "CAMOUHOST_NATIVE_PROFILE" not in shipping
    assert 'launch_with_browser_visible_admission(' in shipping
    assert 'let test_agent_enabled = !cfg!(windows)' in shipping
    assert "&& self.config.execution_owner != RuntimeExecutionOwner::IsolatedBrowserStateTest" in shipping
    composition = (SOURCE.parents[2] /
                   "apps/profile-bridge/src/shipping_composition.rs").read_text(encoding="utf-8")
    # The historical diagnostic has been removed from shipping entirely.
    # Only disposable CI state tests retain a separately authorized probe.
    assert ".for_isolated_browser_state_test()" not in composition
    assert "run_historical_candidate_probe" not in composition
    app = (SOURCE.parents[2] / "apps/profile-bridge/src/main.rs").read_text(
        encoding="utf-8"
    )
    assert "VerifyHistoricalCandidate" not in app
    native_process = (SOURCE.parents[2] / "runtime/camouhost/native_process.py").read_text(encoding="utf-8")
    assert "from_options=dict(options)" in native_process
    assert "persistent_context=True" in native_process
    assert "subprocess.Popen" not in native_process
    assert "context.pages" not in native_process and "context.new_page" not in native_process

    print("CAMOUHOST_EXECUTION_MODES=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
