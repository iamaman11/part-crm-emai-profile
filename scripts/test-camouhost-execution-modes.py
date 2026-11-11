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



def verify_upstream_camoufox_config_is_final(runtime) -> None:
    """The official SDK result is authoritative: no second fingerprint serializer."""
    from types import ModuleType

    fake_package = ModuleType("camoufox")
    fake_utils = ModuleType("camoufox.utils")
    sdk_calls = []

    def official_launch_options(**kwargs):
        sdk_calls.append(kwargs)
        return {
            "env": {"CAMOU_CONFIG_1": "sdk-composed-identity", "SAFE": "untouched"},
            "headless": False,
            "user_data_dir": kwargs["user_data_dir"],
            "firefox_user_prefs": {"official-webgl-setting": True},
        }

    fake_utils.launch_options = official_launch_options
    fake_package.utils = fake_utils
    identity = {"canvas:seed": 12345, "webGl:vendor": "test-gpu"}

    with (
        patch.dict(sys.modules, {"camoufox": fake_package, "camoufox.utils": fake_utils}),
        patch.object(runtime, "camoufox_browser_selector", return_value={"executable_path": "/camoufox.exe"}),
        patch.object(runtime, "locked_firefox_major", return_value=158),
        patch.object(runtime, "accessibility_preferences", return_value={}),
        patch.object(runtime, "resolve_headless_mode", return_value=False),
        patch.object(runtime, "resolve_proxy", return_value=None),
    ):
        base = runtime.camoufox_launch_options({}, Path("/disposable"), identity)
        assert base["env"] == {
            "CAMOU_CONFIG_1": "sdk-composed-identity",
            "SAFE": "untouched",
        }
        assert base["firefox_user_prefs"] == {"official-webgl-setting": True}
        assert "args" not in base
        assert sdk_calls[0]["config"] == identity
        assert sdk_calls[0]["config"] is not identity
        assert sdk_calls[0]["user_data_dir"] == str(Path("/disposable/user_data"))
        # Do not pass an environment at all: the upstream SDK/native process
        # inherits the OS environment and owns CAMOU_CONFIG_* construction.
        assert "env" not in sdk_calls[0]
        assert identity == {"canvas:seed": 12345, "webGl:vendor": "test-gpu"}

        new = runtime.camoufox_launch_options({}, Path("/disposable"), identity, first_run=True)
        assert new["args"] == ["-new-tab", "about:blank"]
        assert new["env"]["CAMOU_CONFIG_1"] == "sdk-composed-identity"
        assert "get_env_vars" not in vars(fake_utils)

        isolated = runtime.camoufox_launch_options({}, Path("/disposable"), identity, isolated=True)
        assert isolated["offline"] is True
        assert isolated["args"] == ["-new-tab", runtime.MANAGED_BOOTSTRAP_TAB_URL]
        assert isolated["env"]["CAMOU_CONFIG_1"] == "sdk-composed-identity"


def verify_windows_browser_sdk_executable_path(runtime) -> None:
    """The SDK receives the original Win32 path; extended paths fail closed."""
    if os.name != "nt":
        return

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / "runtime"
        entrypoint = root / "camouhost" / "real.py"
        browser = root / "browser" / "camoufox.exe"
        entrypoint.parent.mkdir(parents=True)
        browser.parent.mkdir(parents=True)
        entrypoint.write_text("# fixture\n", encoding="utf-8")
        browser.write_bytes(b"disposable Windows executable fixture")
        lock = {
            "windows_distribution": {
                "browser": {"executable_path": "browser/camoufox.exe"}
            },
            "browser": {"version": "158.0.0-beta.1"},
        }

        with patch.object(runtime, "__file__", str(entrypoint)):
            selected = runtime.packaged_windows_browser(lock)
            selector = runtime.camoufox_browser_selector(lock)
        assert selected == browser
        assert os.path.samefile(selected, browser)
        assert selector == {"executable_path": browser}
        assert not str(selected).startswith("\\\\?\\")

        with patch.object(runtime, "__file__", "\\\\?\\" + str(entrypoint)):
            try:
                runtime.packaged_windows_browser(lock)
            except runtime.RuntimeContractError as error:
                assert "conventional local drive path" in str(error)
            else:
                raise AssertionError("extended Win32 entrypoint path must fail closed")


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
    verify_upstream_camoufox_config_is_final(runtime)
    verify_windows_browser_sdk_executable_path(runtime)

    # The Rust delivery owner must preserve the OS process path for the
    # Python child while retaining canonicalize() only for identity checks.
    delivery_runtime = (
        SOURCE.parents[2] / "apps/profile-bridge/src/windows_delivery_runtime.rs"
    ).read_text(encoding="utf-8")
    assert "let process_executable = std::env::current_exe()" in delivery_runtime
    assert "fs::canonicalize(&process_executable)" in delivery_runtime
    assert "InstalledDeliveryLayout::from_executable(&process_executable)" in delivery_runtime
    assert "InstalledDeliveryLayout::from_executable(&canonical_executable)" not in delivery_runtime

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
    # A blank DRAFT alone may ask the official SDK for a new identity;
    # existing profiles must provide their persisted config.
    with patch.object(sys, "stdin", io.StringIO(
        json.dumps({"config_json": None, "first_run": True}) + "\n"
    )):
        assert runtime.read_local_open_request() == (None, True)
    with patch.object(sys, "stdin", io.StringIO(
        json.dumps({"config_json": None, "first_run": False}) + "\n"
    )):
        try:
            runtime.read_local_open_request()
        except runtime.RuntimeContractError:
            pass
        else:
            raise AssertionError("READY profile without saved identity was accepted")

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


    native_code = SOURCE.read_text(encoding="utf-8")
    warm_open = native_code.split("async def run_local_profile(", 1)[1].split(
        "\ndef main() -> int:", 1
    )[0]
    assert "verify_python_components(lock)" not in warm_open, (
        "installed warm Open unnecessarily reverified the packaged fpgen model"
    )
    assert "materialize_candidate_identity(root)" in warm_open, (
        "fresh empty DRAFT lost its one official SDK fingerprint draw"
    )
    assert "camouhost-identity-persisted" in warm_open
    local_source = (SOURCE.parents[2] / "apps/profile-bridge/src/local_browser_open.rs").read_text(
        encoding="utf-8"
    )
    local_owner = local_source.split("pub(crate) fn open_with_before_launch(", 1)[1].split("\n}\n", 1)[0]
    cold_branch = local_source.split(
        "let config = if folder.join(CONFIG).exists()", 1
    )[1].split("let first_identity = config.is_none()", 1)[0]
    assert "require_empty_user_data(folder)?" in cold_branch
    assert "Some(read_profile_config(folder)?)" in cold_branch
    assert '"--materialize-identity"' not in local_source
    assert "persist_sdk_identity(folder, &line)?" in local_source
    assert "write_all(IDENTITY_ACK)" in local_source
    assert "camouhost-identity-persisted" in SOURCE.read_text(encoding="utf-8")
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
    assert "local_browser_open::open_with_before_launch" in composition
    assert "start_sync_after_close(intent)" in composition
    run_initial = composition.split("    pub fn run_initial(", 1)[1].split(
        "    pub fn run_delivery_command(", 1
    )[0]
    assert run_initial.count("start_sync_after_close(intent)") == 2
    assert "if error != crate::local_browser_open::LocalOpenError::StaleLocalWorkspace {" in run_initial
    assert "DeliveryHealth" not in run_initial
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
    identity_runtime = SOURCE.read_text(encoding="utf-8").split(
        "def materialize_candidate_identity", 1
    )[1].split("\n\ndef valid_frame", 1)[0]
    assert ".mkdir(" not in identity_runtime and ".write_bytes(" not in identity_runtime
    assert '"camoufox_config_json"' in identity_runtime

    print("CAMOUHOST_EXECUTION_MODES=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
