#!/usr/bin/env python3
"""Explicit official user persistent-context vs page-inspecting automation routing contract.

Offline negative tests: no browser launch, no test credentials, no cloud calls.
The real browser/lease/Save invariants require separate physical acceptance.
"""
from __future__ import annotations

import asyncio
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


def verify_first_identity_failure_has_a_safe_distinct_phase(runtime) -> None:
    """A failed first DRAFT draw never becomes a browser-launch failure or leaks Python text."""
    from types import ModuleType

    fake_native = ModuleType("native_process")

    async def unexpected_launch(**_kwargs):
        raise AssertionError("browser must never launch after identity generation fails")

    fake_native.launch_native_camoufox = unexpected_launch
    diagnostics = io.StringIO()
    with (
        tempfile.TemporaryDirectory() as directory,
        patch.dict(sys.modules, {"native_process": fake_native}),
        patch.object(
            runtime,
            "materialize_candidate_identity",
            side_effect=RuntimeError("sensitive test path and secret"),
        ),
        contextlib.redirect_stderr(diagnostics),
    ):
        result = asyncio.run(runtime.run_local_profile(Path(directory), None, True))
    assert result == 5
    assert diagnostics.getvalue().splitlines() == [
        "camouhost_stage phase=launch step=identity_requested",
        "camouhost_failure phase=identity category=unexpected",
    ]

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
            # Opaque future SDK feature options are not an app-owned whitelist.
            "sdk_native_features": {"webgl2": True, "ubo": True, "humanize": True},
        }

    fake_utils.launch_options = official_launch_options
    fake_package.utils = fake_utils
    identity = {"canvas:seed": 12345, "webGl:vendor": "test-gpu"}

    with (
        patch.dict(sys.modules, {"camoufox": fake_package, "camoufox.utils": fake_utils}),
        patch.object(runtime, "camoufox_browser_selector", return_value={"executable_path": "/camoufox.exe"}),
        patch.object(runtime, "locked_firefox_major", return_value=159),
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
        assert base["sdk_native_features"] == {
            "webgl2": True, "ubo": True, "humanize": True,
        }
        assert base["args"] == ["-new-tab", runtime.MANAGED_BOOTSTRAP_TAB_URL]
        assert base["ignore_default_args"] == ["about:blank"]
        assert sdk_calls[0]["config"] == identity
        assert sdk_calls[0]["config"] is not identity
        assert sdk_calls[0]["user_data_dir"] == str(Path("/disposable/user_data"))
        assert (
            sdk_calls[0]["firefox_user_prefs"]["privacy.baselineFingerprintingProtection"]
            is False
        )
        # Do not pass an environment at all: the upstream SDK/native process
        # inherits the OS environment and owns CAMOU_CONFIG_* construction.
        assert "env" not in sdk_calls[0]
        assert identity == {"canvas:seed": 12345, "webGl:vendor": "test-gpu"}

        # A pre-159 profile retains the exact signed canonical configuration,
        # while the newer SDK sees only the currently supported settings.
        legacy = {**identity, "webGl:contextAttributes": {"alpha": True},
                  "webGl2:contextAttributes": {"depth": False}}
        original = json.dumps(legacy, sort_keys=True)
        runtime.camoufox_launch_options({}, Path("/disposable"), legacy)
        assert sdk_calls[-1]["config"] == identity
        assert json.dumps(legacy, sort_keys=True) == original
        assert sdk_calls[-1]["config"] is not legacy

        new = runtime.camoufox_launch_options({}, Path("/disposable"), identity, first_run=True)
        assert new["args"] == ["-new-tab", runtime.MANAGED_BOOTSTRAP_TAB_URL]
        assert new["ignore_default_args"] == ["about:blank"]
        assert new["env"]["CAMOU_CONFIG_1"] == "sdk-composed-identity"
        assert "get_env_vars" not in vars(fake_utils)

        isolated = runtime.camoufox_launch_options({}, Path("/disposable"), identity, isolated=True)
        assert isolated["offline"] is True
        assert isolated["args"] == ["-new-tab", runtime.MANAGED_BOOTSTRAP_TAB_URL]
        assert isolated["ignore_default_args"] == ["about:blank"]
        assert isolated["env"]["CAMOU_CONFIG_1"] == "sdk-composed-identity"


def verify_official_gpu_timeout_policy_is_persistent_and_fail_closed(runtime) -> None:
    """Only two SDK GPU timeouts can select persistent masked rendering."""
    from types import ModuleType

    fake_package = ModuleType("camoufox")
    fake_utils = ModuleType("camoufox.utils")
    fake_fingerprints = ModuleType("camoufox.fingerprints")
    fingerprint_draws = []
    sdk_calls = []
    exact_config = {"navigator.userAgent": "fixture", "webGl2Enabled": True}

    def generate_fingerprint(**kwargs):
        fingerprint_draws.append(kwargs)
        return {"fixture": "one official fpgen identity"}

    def official_options(**kwargs):
        sdk_calls.append(kwargs)
        if len(sdk_calls) <= 2:
            raise TimeoutError("SDK host GPU probe timed out; path deliberately not logged")
        return {
            "env": {
                "CAMOU_CONFIG_1": json.dumps(exact_config),
                "CAMOU_PREFS_1": json.dumps({"sdk-owned": "untouched"}),
            },
            "headless": False,
            "firefox_user_prefs": {"sdk-owned": "untouched"},
        }

    fake_fingerprints.generate_fingerprint = generate_fingerprint
    fake_utils.get_screen_cons = lambda _headless: None
    fake_utils.launch_options = official_options
    fake_package.utils = fake_utils
    fake_package.fingerprints = fake_fingerprints
    lock = {"fingerprint_policy_version": "official-v159-fixture"}

    with (
        tempfile.TemporaryDirectory() as directory,
        patch.dict(sys.modules, {
            "camoufox": fake_package,
            "camoufox.utils": fake_utils,
            "camoufox.fingerprints": fake_fingerprints,
        }),
        patch.object(runtime, "load_runtime_lock", return_value=(lock, "0" * 64)),
        patch.object(runtime, "verify_python_components"),
        patch.object(
            runtime, "camoufox_browser_selector",
            return_value={"executable_path": "/fixture/camoufox.exe"},
        ),
        patch.object(runtime, "locked_firefox_major", return_value=159),
        patch.object(runtime, "resolve_headless_mode", return_value=False),
        patch.object(runtime, "resolve_proxy", return_value=None),
        patch.object(runtime, "generation_webgl2_enabled"),
    ):
        original = Path(directory) / "source"
        original.mkdir()
        diagnostics = io.StringIO()
        with contextlib.redirect_stderr(diagnostics):
            report, first_sdk_options = runtime.materialize_candidate_identity(original)
        assert diagnostics.getvalue().splitlines() == [
            "camouhost_stage phase=launch step=identity_fpgen_started",
            "camouhost_stage phase=launch step=identity_fpgen_ready",
            "camouhost_stage phase=launch step=identity_gpu_probe_started",
            "camouhost_stage phase=launch step=identity_gpu_probe_timeout",
            "camouhost_stage phase=launch step=identity_gpu_retry_started",
            "camouhost_stage phase=launch step=identity_gpu_retry_timeout",
            "camouhost_stage phase=launch step=identity_gpu_fallback_started",
            "camouhost_stage phase=launch step=identity_gpu_fallback_ready",
        ]
        assert report["canvas_noise"] is True
        assert len(fingerprint_draws) == 1
        assert len(sdk_calls) == 3
        assert "canvas_noise" not in sdk_calls[0]
        assert "canvas_noise" not in sdk_calls[1]
        assert sdk_calls[2]["canvas_noise"] is True
        assert sdk_calls[1]["fingerprint"] == sdk_calls[0]["fingerprint"]
        assert sdk_calls[2]["fingerprint"] == sdk_calls[0]["fingerprint"]
        assert sdk_calls[1]["fingerprint"] is not sdk_calls[0]["fingerprint"]
        assert sdk_calls[2]["fingerprint"] is not sdk_calls[0]["fingerprint"]
        assert sdk_calls[1]["executable_path"] == "/fixture/camoufox.exe"
        assert first_sdk_options["env"]["CAMOU_CONFIG_1"] == json.dumps(exact_config)
        assert first_sdk_options["args"] == ["-new-tab", runtime.MANAGED_BOOTSTRAP_TAB_URL]
        assert first_sdk_options["ignore_default_args"] == ["about:blank"]
        assert json.loads(report["sdk_replay_json"])["fingerprint"] == sdk_calls[0]["fingerprint"]
        assert json.loads(report["camoufox_config_json"]) == exact_config
        assert not (original / runtime.CONFIG_NAME).exists()
        assert not (original / runtime.RENDER_POLICY_NAME).exists()

        # Mirror the sole Rust writer's exact persisted bytes. Only this
        # immutable choice, not a second generated fingerprint, is restored.
        policy = runtime.canonical_json({"canvas_noise": True, "schema_version": 1})
        original.joinpath(runtime.RENDER_POLICY_NAME).write_bytes(policy)
        original.joinpath(runtime.CONFIG_NAME).write_text(
            report["camoufox_config_json"], encoding="utf-8"
        )
        runtime.camoufox_launch_options(lock, original, exact_config, first_run=True)
        assert sdk_calls[-1]["canvas_noise"] is True
        assert (
            sdk_calls[-1]["firefox_user_prefs"]["privacy.baselineFingerprintingProtection"]
            is True
        )

        restored = Path(directory) / "cold-restore"
        restored.mkdir()
        restored.joinpath(runtime.RENDER_POLICY_NAME).write_bytes(policy)
        restored.joinpath(runtime.CONFIG_NAME).write_text(
            report["camoufox_config_json"], encoding="utf-8"
        )
        runtime.camoufox_launch_options(lock, restored, exact_config)
        assert sdk_calls[-1]["canvas_noise"] is True
        assert sdk_calls[-1]["config"] == exact_config
        assert len(fingerprint_draws) == 1

        native = Path(directory) / "new-native-default"
        native.mkdir()
        native.joinpath(runtime.RENDER_POLICY_NAME).write_bytes(
            b'{"canvas_noise":false,"schema_version":1}\n'
        )
        runtime.camoufox_launch_options(lock, native, exact_config)
        assert "canvas_noise" not in sdk_calls[-1]
        assert "privacy.baselineFingerprintingProtection" not in sdk_calls[-1]["firefox_user_prefs"]

        restored.joinpath(runtime.RENDER_POLICY_NAME).write_bytes(
            b'{"canvas_noise":"false","schema_version":1}\n'
        )
        try:
            runtime.camoufox_launch_options(lock, restored, exact_config)
        except runtime.RuntimeContractError:
            pass
        else:
            raise AssertionError("malformed rendering policy must fail closed")

        restored.joinpath(runtime.RENDER_POLICY_NAME).write_bytes(policy + b"junk")
        try:
            runtime.camoufox_launch_options(lock, restored, exact_config)
        except runtime.RuntimeContractError:
            pass
        else:
            raise AssertionError("noncanonical rendering policy must fail closed")

        if os.name != "nt":
            restored.joinpath(runtime.RENDER_POLICY_NAME).unlink()
            restored.joinpath(runtime.RENDER_POLICY_NAME).symlink_to(original / runtime.CONFIG_NAME)
            try:
                runtime.camoufox_launch_options(lock, restored, exact_config)
            except runtime.RuntimeContractError:
                pass
            else:
                raise AssertionError("redirected rendering policy must fail closed")

        # A cold timeout followed by a warmed official probe keeps native
        # canvas rendering, the same fpgen draw, and exact replay on Open.
        native_after_retry = Path(directory) / "native-after-retry"
        native_after_retry.mkdir()
        sdk_calls.clear()

        def one_cold_timeout(**kwargs):
            sdk_calls.append(kwargs)
            if len(sdk_calls) == 1:
                raise TimeoutError("cold first WebGL probe")
            return official_options_result()

        def official_options_result():
            return {
                "env": {
                    "CAMOU_CONFIG_1": json.dumps(exact_config),
                    "CAMOU_PREFS_1": json.dumps({"sdk-owned": "untouched"}),
                },
                "headless": False,
                "firefox_user_prefs": {"sdk-owned": "untouched"},
            }

        fake_utils.launch_options = one_cold_timeout
        diagnostics = io.StringIO()
        with contextlib.redirect_stderr(diagnostics):
            native_report, native_options = runtime.materialize_candidate_identity(native_after_retry)
        assert diagnostics.getvalue().splitlines() == [
            "camouhost_stage phase=launch step=identity_fpgen_started",
            "camouhost_stage phase=launch step=identity_fpgen_ready",
            "camouhost_stage phase=launch step=identity_gpu_probe_started",
            "camouhost_stage phase=launch step=identity_gpu_probe_timeout",
            "camouhost_stage phase=launch step=identity_gpu_retry_started",
            "camouhost_stage phase=launch step=identity_gpu_retry_native_ready",
        ]
        assert native_report["canvas_noise"] is False
        assert len(fingerprint_draws) == 2
        assert len(sdk_calls) == 2
        assert sdk_calls[0]["fingerprint"] == sdk_calls[1]["fingerprint"]
        assert sdk_calls[0]["fingerprint"] is not sdk_calls[1]["fingerprint"]
        assert "canvas_noise" not in sdk_calls[0] and "canvas_noise" not in sdk_calls[1]
        assert json.loads(native_report["sdk_replay_json"])["fingerprint"] == sdk_calls[0]["fingerprint"]
        assert native_options["env"]["CAMOU_CONFIG_1"] == json.dumps(exact_config)
        assert not (native_after_retry / runtime.CONFIG_NAME).exists()
        assert not (native_after_retry / runtime.RENDER_POLICY_NAME).exists()

        # A healthy first probe makes only one SDK call with no fallback.
        sdk_calls.clear()
        fake_utils.launch_options = lambda **kwargs: (sdk_calls.append(kwargs), official_options_result())[1]
        first_ok = Path(directory) / "native-first-attempt"
        first_ok.mkdir()
        with contextlib.redirect_stderr(diagnostics := io.StringIO()):
            first_ok_report, _ = runtime.materialize_candidate_identity(first_ok)
        assert first_ok_report["canvas_noise"] is False
        assert len(sdk_calls) == 1
        assert diagnostics.getvalue().splitlines() == [
            "camouhost_stage phase=launch step=identity_fpgen_started",
            "camouhost_stage phase=launch step=identity_fpgen_ready",
            "camouhost_stage phase=launch step=identity_gpu_probe_started",
            "camouhost_stage phase=launch step=identity_gpu_probe_native_ready",
        ]

        # A non-timeout second failure must propagate; never mask it.
        sdk_calls.clear()

        def unrelated_after_timeout(**kwargs):
            sdk_calls.append(kwargs)
            if len(sdk_calls) == 1:
                raise TimeoutError("cold probe")
            raise OSError("unrelated SDK failure with secret")

        fake_utils.launch_options = unrelated_after_timeout
        unrelated = Path(directory) / "unrelated-error"
        unrelated.mkdir()
        with contextlib.redirect_stderr(diagnostics := io.StringIO()):
            try:
                runtime.materialize_candidate_identity(unrelated)
            except OSError:
                pass
            else:
                raise AssertionError("unrelated SDK failure must propagate")
        assert len(sdk_calls) == 2
        assert "identity_gpu_fallback_started" not in diagnostics.getvalue()
        assert not (unrelated / runtime.CONFIG_NAME).exists()

        # An unrelated SDK failure never turns into masked-canvas recovery.
        sdk_calls.clear()
        fake_utils.launch_options = lambda **_kwargs: (_ for _ in ()).throw(OSError("fixture"))
        unrelated = Path(directory) / "unrelated-first-error"
        unrelated.mkdir()
        try:
            runtime.materialize_candidate_identity(unrelated)
        except OSError:
            pass
        else:
            raise AssertionError("unrelated SDK failure must propagate")
        assert not (unrelated / runtime.CONFIG_NAME).exists()
        assert not (unrelated / runtime.RENDER_POLICY_NAME).exists()


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
            "browser": {"version": "159.0-beta.43"},
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
    verify_first_identity_failure_has_a_safe_distinct_phase(runtime)
    # Release tags changed from three to two numeric components in Firefox 159.
    for version, expected_major in (
        ("158.0.1-beta.41", 158),
        ("159.0-beta.43", 159),
        ("160.1.3-beta.1", 160),
    ):
        assert runtime.locked_firefox_major({"browser": {"version": version}}) == expected_major
    for version in ("159", "159.0", "159.0.0", "159.0-beta.0", "159.0-beta.43-extra", "official/beta.43"):
        try:
            runtime.locked_firefox_major({"browser": {"version": version}})
        except runtime.RuntimeContractError:
            pass
        else:
            raise AssertionError(f"unsupported Camoufox version accepted: {version}")
    # A lost Rust diagnostic reader must never interrupt Camoufox's own SDK
    # close lifecycle while the inherited native child guard remains active.
    class ClosedDiagnosticPipe:
        def write(self, _value):
            raise BrokenPipeError("parent diagnostic reader exited")

        def flush(self):
            raise BrokenPipeError("parent diagnostic reader exited")

    with patch.object(runtime.sys, "stderr", ClosedDiagnosticPipe()):
        runtime.report_runtime_stage("launch", "ready")
        runtime.report_runtime_stage("close", "browser_context_closed")
        runtime.report_runtime_failure("launch", RuntimeError("synthetic"))
    diagnostic_output = io.StringIO()
    with contextlib.redirect_stderr(diagnostic_output):
        for step in (
            "identity_requested", "identity_fpgen_started", "identity_fpgen_ready",
            "identity_gpu_probe_started", "identity_gpu_probe_timeout",
            "identity_gpu_retry_started", "identity_gpu_retry_timeout",
            "identity_gpu_fallback_started", "identity_gpu_fallback_ready",
            "identity_gpu_probe_native_ready", "identity_gpu_retry_native_ready",
            "identity_ready",
            "options_ready", "sdk_enter", "browser_entered",
        ):
            runtime.report_runtime_stage("launch", step)
        runtime.report_runtime_stage("launch", "not-a-stage")
        runtime.report_runtime_failure("launch", RuntimeError("profile path and secret must not leak"))
        runtime.report_runtime_failure("identity", RuntimeError("profile path and secret must not leak"))
    safe_lines = diagnostic_output.getvalue().splitlines()
    assert safe_lines == [
        "camouhost_stage phase=launch step=identity_requested",
        "camouhost_stage phase=launch step=identity_fpgen_started",
        "camouhost_stage phase=launch step=identity_fpgen_ready",
        "camouhost_stage phase=launch step=identity_gpu_probe_started",
        "camouhost_stage phase=launch step=identity_gpu_probe_timeout",
        "camouhost_stage phase=launch step=identity_gpu_retry_started",
        "camouhost_stage phase=launch step=identity_gpu_retry_timeout",
        "camouhost_stage phase=launch step=identity_gpu_fallback_started",
        "camouhost_stage phase=launch step=identity_gpu_fallback_ready",
        "camouhost_stage phase=launch step=identity_gpu_probe_native_ready",
        "camouhost_stage phase=launch step=identity_gpu_retry_native_ready",
        "camouhost_stage phase=launch step=identity_ready",
        "camouhost_stage phase=launch step=options_ready",
        "camouhost_stage phase=launch step=sdk_enter",
        "camouhost_stage phase=launch step=browser_entered",
        "camouhost_failure phase=launch category=unexpected",
        "camouhost_failure phase=identity category=unexpected",
    ], safe_lines
    verify_embedded_python_path_configuration()
    verify_upstream_camoufox_config_is_final(runtime)
    verify_official_gpu_timeout_policy_is_persistent_and_fail_closed(runtime)
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
    old = {**config, "webGl:contextAttributes": {"alpha": True}}
    old_json = runtime.canonical_json(old).decode("utf-8")
    with patch.object(sys, "stdin", io.StringIO(
        json.dumps({"config_json": old_json, "first_run": False}) + "\n"
    )):
        assert runtime.read_local_open_request() == (old, False)
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
    assert '.env("PYTHONDONTWRITEBYTECODE", "1")' in local_source, (
        "native runtime must suppress mutable Python caches in SDK subprocesses"
    )
    local_owner = local_source.split("fn open_under_profile_admission(", 1)[1].split("\n}\n", 1)[0]
    cold_branch = local_source.split(
        "let config = if folder.join(CONFIG).exists()", 1
    )[1].split("let first_identity = config.is_none()", 1)[0]
    assert "require_empty_user_data(folder)?" in cold_branch
    assert "Some(read_profile_config(folder)?)" in cold_branch
    assert '"--materialize-identity"' not in local_source
    assert "persist_sdk_identity(folder, &line)?" in local_source
    assert "write_all(IDENTITY_ACK)" in local_source
    assert "camouhost-identity-persisted" in SOURCE.read_text(encoding="utf-8")
    dirty_marker = local_owner.find("mark_local_unsynced(workspace.path())")
    native_launch = local_owner.find("run_python(", dirty_marker)
    assert dirty_marker >= 0 and native_launch > dirty_marker, (
        "durable local dirty evidence must precede native browser launch"
    )
    local_open_after_dirty = local_owner[dirty_marker:]
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
    assert run({runtime.EXECUTION_MODE_ENV: runtime.ISOLATED_COLD_PROBE_MODE,
                runtime.AGENT_DIRECTORY_ENV: "/tmp/never-execute"}, 2, []), "cold probe must reject scripts"
    # Windows C09 is headful-only; Linux CI intentionally permits isolated
    # headless/virtual probes and must not be tested against Windows policy.
    for mode in ("true", "virtual"):
        env = {runtime.EXECUTION_MODE_ENV: runtime.ISOLATED_COLD_PROBE_MODE,
               runtime.HEADLESS_MODE_ENV: mode}
        if os.name == "nt":
            run(env, 2, [])
        else:
            run(env, 18, ["automation"])

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
    assert "local_browser_open::open_with_before_launch_and_after_close" in composition
    assert "start_sync_after_close(" not in composition
    run_initial = composition.split("    pub fn run_initial(", 1)[1].split(
        "    pub fn run_delivery_command(", 1
    )[0]
    # Existing local CURRENT is always opened locally. The only cloud
    # boundary before Open is RestoreRequired for a missing local workspace.
    assert "run_closed_sync(intent)" in run_initial
    assert "run_closed_sync_with_admission(" in composition
    assert "RestoreRequired" in run_initial
    assert "ensure_authoritative_current" in run_initial
    # Network Sync follows a verified native Close, never local Open.
    assert run_initial.index(
        "if native_close_admits_automatic_sync(&open_result)"
    ) < run_initial.index("run_closed_sync(intent)")
    assert "if dirty {" not in run_initial
    assert "crate::local_browser_open::LocalOpenError::StaleLocalWorkspace" not in run_initial
    # Automatic cloud Save needs a positively confirmed native close;
    # no failed launch, uncertain adapter, or busy profile can spawn Sync.
    assert "if native_close_admits_automatic_sync(&open_result)" in run_initial
    native_close_admission = composition.split(
        "fn native_close_admits_automatic_sync(", 1
    )[1].split("    #[cfg(test)]", 1)[0]
    assert "result.is_ok()" in native_close_admission
    assert "DeliveryHealth" not in run_initial
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
