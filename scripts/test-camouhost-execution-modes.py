#!/usr/bin/env python3
"""Explicit normal-native vs official Playwright automation routing contract.

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
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "runtime" / "camouhost" / "real.py"


def load_runtime():
    spec = importlib.util.spec_from_file_location("camouhost_execution_modes_test", SOURCE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def main() -> int:
    runtime = load_runtime()
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

    # Normal CRM user Open always launches native; never page-managing Playwright.
    run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE}, 17, ["native"])
    run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE,
         runtime.HEADLESS_MODE_ENV: "false"}, 17, ["native"])
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

    # The shipping Windows launcher must select the native path explicitly.
    # This is source wiring coverage, not a substitute for a Rust build.
    rust = SOURCE.parents[2] / "apps/profile-bridge/src/camouhost_process.rs"
    shipping = rust.read_text(encoding="utf-8")
    assert 'RuntimeExecutionOwner::NativeUser => "user-native"' in shipping
    assert 'RuntimeExecutionOwner::IsolatedHistoricalProbe => "isolated-cold-probe"' in shipping
    assert "CAMOUHOST_NATIVE_PROFILE" not in shipping
    assert 'launch_with_browser_visible_admission(' in shipping
    assert 'let test_agent_enabled = !cfg!(windows)' in shipping
    assert "&& self.config.execution_owner != RuntimeExecutionOwner::IsolatedHistoricalProbe" in shipping
    composition = (SOURCE.parents[2] /
                   "apps/profile-bridge/src/shipping_composition.rs").read_text(encoding="utf-8")
    assert composition.count(".for_isolated_historical_probe()") == 1
    assert composition.index(".for_isolated_historical_probe()") < composition.index(
        "pub fn run(claim:"
    ), "ordinary user Open must never request Playwright"

    print("CAMOUHOST_EXECUTION_MODES=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
