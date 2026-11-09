#!/usr/bin/env python3
"""Explicit official user persistent-context vs page-inspecting automation routing contract.

Offline negative tests: no browser launch, no test credentials, no cloud calls.
The real browser/lease/Save invariants require separate physical acceptance.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
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
            patch.object(runtime, "run_ipc", side_effect=automation),
            contextlib.redirect_stderr(error),
        ):
            outcome = runtime.main()
        assert outcome == expected, (env, outcome, expected)
        assert invoked == called, (env, invoked, called)
        return error.getvalue()

    # The canonical local-first user launch never redeems an online claim.
    # This is the same official async Playwright path, not a second engine.
    async def local_open(_folder, _config, _first_run):
        invoked.append("local")
        return 19

    config = {key: {} for key in runtime.WEBGL_REQUIRED_KEYS}
    config_json = runtime.canonical_json(config).decode("utf-8")
    request = json.dumps({"config_json": config_json, "first_run": True})
    with patch.object(sys, "stdin", io.StringIO(request)):
        assert runtime.read_local_open_request() == (config, True)

    with tempfile.TemporaryDirectory() as folder:
        for environment, outcome in (
            ({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE}, 19),
            ({}, 2),
            ({runtime.EXECUTION_MODE_ENV: runtime.ISOLATED_COLD_PROBE_MODE}, 2),
            ({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE,
              runtime.AGENT_DIRECTORY_ENV: folder}, 2),
        ):
            invoked.clear()
            with (patch.dict(os.environ, environment, clear=True),
                  patch.object(sys, "argv", ["real.py", "--local-open", folder]),
                  patch.object(sys, "stdin", io.StringIO(request)),
                  patch.object(runtime, "run_local_profile", side_effect=local_open),
                  contextlib.redirect_stderr(io.StringIO())):
                assert runtime.main() == outcome
            assert invoked == (["local"] if outcome == 19 else [])

    local_source = (SOURCE.parents[2] / "apps/profile-bridge/src/local_browser_open.rs").read_text(
        encoding="utf-8"
    )
    local_owner = local_source.split("pub(crate) fn open_with_before_launch(", 1)[1].split("\n}\n", 1)[0]
    local_open_after_dirty = local_owner.split("mark_local_unsynced(workspace.path())?;", 1)[1]
    assert 'run_python(' in local_open_after_dirty
    assert '"--local-open"' in local_open_after_dirty
    assert 'before_launch' in local_open_after_dirty
    assert ".local-unsynced" in local_source
    assert "ControlPlaneEnrollment" not in local_owner
    assert "MachineHttpPort" not in local_owner and "SharedControlPlaneCoordinator" not in local_owner
    assert "windows_delivery_runtime" not in local_source.split("#[cfg(test)]", 1)[0]
    composition = (SOURCE.parents[2] /
                   "apps/profile-bridge/src/shipping_composition.rs").read_text(encoding="utf-8")
    local_first = composition.split("    pub fn run_initial(", 1)[1].split(
        "    pub fn run_delivery_command(", 1
    )[0]
    assert "local_browser_open::open_with_before_launch" in local_first
    assert "generation_initialization::initialize(" not in local_first
    assert "ControlPlaneEnrollment" not in local_first
    assert "claim_launch_intent" not in local_first

    # No implicit managed browser launch when a mode flag is missing/old/unknown.
    for env in ({}, {"CAMOUHOST_NATIVE_PROFILE": "1"},
                {runtime.EXECUTION_MODE_ENV: ""},
                {runtime.EXECUTION_MODE_ENV: "native"},
                {runtime.EXECUTION_MODE_ENV: "playwright"}):
        assert run(env, 2, []), "fail-closed dispatch must explain rejection"

    # Normal CRM user Open has exactly one local-first entrypoint. Direct
    # USER_NATIVE_MODE invocation without --local-open is rejected.
    assert run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE}, 2, [])
    assert not hasattr(runtime, "run_native_ipc")
    assert not hasattr(runtime, "run_native_ipc_async")
    assert run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE,
                runtime.AGENT_DIRECTORY_ENV: "/tmp/never-run-scripts"},
               2, []), "native user context must never execute local Python scripts"
    for invalid in ("true", "virtual", "1", ""):
        assert run({runtime.EXECUTION_MODE_ENV: runtime.USER_NATIVE_MODE,
                    runtime.HEADLESS_MODE_ENV: invalid}, 2, []), "no headless fallback"

    # The isolated Python Playwright automation IPC remains reachable only
    # through its explicit disposable-profile execution mode. It is not wired
    # to shipping Windows Open.
    assert run({runtime.EXECUTION_MODE_ENV: runtime.AUTOMATION_PLAYWRIGHT_MODE},
               2, []), "automation must not open a managed context without a job"
    run({runtime.EXECUTION_MODE_ENV: runtime.AUTOMATION_PLAYWRIGHT_MODE,
         runtime.AGENT_DIRECTORY_ENV: "/tmp/authorized-test-inbox"},
        18, ["automation"])
    # The isolated cold-probe adapter is test/automation-only and cannot be
    # reached from the user-native Rust Open path.
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

    # Normal CRM Open has one Rust owner: local_browser_open. The
    # retired claim/ManagedCamouhostProcess IPC path is not part of the app.
    lib = (SOURCE.parents[2] / "apps/profile-bridge/src/lib.rs").read_text(
        encoding="utf-8"
    )
    composition = (SOURCE.parents[2] /
                   "apps/profile-bridge/src/shipping_composition.rs").read_text(encoding="utf-8")
    assert "pub mod camouhost_process;" not in lib
    assert "pub mod shipping_preflight;" not in lib
    assert not (SOURCE.parents[2] / "apps/profile-bridge/src/shipping_preflight.rs").exists()
    assert "ManagedCamouhostProcess" not in composition
    assert "ManagedCamouhostIpc" not in composition
    assert "pub fn run_claim(" not in composition
    assert "local_browser_open::open" in composition
    assert "start_sync_after_close(intent)" in composition
    run_initial = composition.split("    pub fn run_initial(", 1)[1].split(
        "    pub fn run_delivery_command(", 1
    )[0]
    assert run_initial.count("start_sync_after_close(intent)") >= 4
    assert "Every post-restore local failure may leave the" in run_initial
    assert "if error != crate::local_browser_open::LocalOpenError::StaleLocalWorkspace {" in run_initial
    app = (SOURCE.parents[2] / "apps/profile-bridge/src/main.rs").read_text(
        encoding="utf-8"
    )
    assert "profilebridge://claim/" in app
    assert "old_claim_and_v1_user_open_are_not_supported_application_modes" in app
    native_process = (SOURCE.parents[2] / "runtime/camouhost/native_process.py").read_text(encoding="utf-8")
    assert "AsyncNewBrowser" in native_process and "async_playwright" in native_process
    assert "persistent_context=True" in native_process
    assert 'context.on("close"' in native_process
    assert "sessionCheckpoints.json" not in native_process
    assert "EnumWindows" not in native_process and "Get-Process" not in native_process
    assert "PostMessage" not in native_process and "WM_CLOSE" not in native_process
    assert ".is_symlink(" not in native_process and ".is_dir(" not in native_process
    assert ".resolve(" not in native_process
    local_runtime = SOURCE.read_text(encoding="utf-8").split("async def run_local_profile", 1)[1].split("\n\ndef main", 1)[0]
    assert "wait_for_browser_close" in local_runtime
    assert "close_after_browser_exit" in local_runtime
    assert "WindowsBrowserExitWitness" not in local_runtime
    assert "profile_state" not in local_runtime
    assert not hasattr(runtime, "firefox_native_first_run_args")
    local_runtime = SOURCE.read_text(encoding="utf-8").split(
        "async def run_local_profile", 1
    )[1].split("\n\ndef main", 1)[0]
    assert "load_canonical_json" not in local_runtime
    assert "require_bridge_writer_lock" not in local_runtime
    assert ".mkdir(" not in local_runtime and ".write_bytes(" not in local_runtime

    print("CAMOUHOST_EXECUTION_MODES=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
