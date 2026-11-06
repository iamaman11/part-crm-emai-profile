#!/usr/bin/env python3
"""Offline native process boundary tests, without building or launching Firefox."""
from __future__ import annotations

import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import types
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "runtime" / "camouhost" / "native_process.py"


def load():
    spec = importlib.util.spec_from_file_location("camouhost_native_process_test", SOURCE)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def main() -> int:
    module = load()
    util = types.ModuleType("camoufox.utils")
    root = types.ModuleType("camoufox")
    observed_launch_options = []

    def official_options(**kwargs):
        observed_launch_options.append(kwargs)
        assert kwargs["ff_version"] == 158
        assert kwargs["os"] == "windows"
        assert kwargs["env"].get("CAMOU_CONFIG_1") is None
        assert kwargs["firefox_user_prefs"]["browser.startup.page"] == 3
        assert kwargs["firefox_user_prefs"]["privacy.baselineFingerprintingProtection"] is False
        assert kwargs["config"] == {"sentinel": "pinned", "navigator.maxTouchPoints": 7}
        assert kwargs["exclude_addons"] == []
        # Simulate the *real* Camoufox SDK changing an already saved field.
        altered = dict(kwargs["config"])
        altered["navigator.maxTouchPoints"] = len(observed_launch_options) % 3
        altered["addons"] = ["official/default-addon"]
        return {"env": {**kwargs["env"], "CAMOU_CONFIG_1": json.dumps(altered),
                        "CAMOU_PREFS_1": "native"}}
    util.launch_options = official_options
    util.get_env_vars = lambda config, target, path: {
        "CAMOU_CONFIG_1": json.dumps(config, sort_keys=True)
    }

    with tempfile.TemporaryDirectory() as temp:
        workspace = pathlib.Path(temp) / "generation"
        userdata = workspace / "user_data"
        userdata.mkdir(parents=True)
        browser = pathlib.Path(temp) / "camoufox.exe"
        browser.write_bytes(b"approved-disposable-test-fixture")
        created = []
        def spawn(args, **kwargs):
            created.append((args, kwargs))
            return None
        modules = {
            "camoufox": root,
            "camoufox.utils": util,
        }
        with patch.dict(sys.modules, modules), patch.object(subprocess, "Popen", side_effect=spawn):
            for approved, active in ((False, False), (True, True)):
                try:
                    module.launch_native_camoufox(
                        workspace=workspace, browser=browser, identity={"sentinel":"pinned", "navigator.maxTouchPoints":7},
                        inherited={"SYSTEMROOT":"C:\\Windows"}, writer_active=lambda _: active,
                        admission_granted=approved,
                    )
                except module.NativeLaunchError:
                    pass
                else:
                    raise AssertionError("unauthorized or already-owned profile launched")
                assert not created

            proc = module.launch_native_camoufox(
                workspace=workspace, browser=browser,
                identity={"sentinel":"pinned", "navigator.maxTouchPoints":7},
                inherited={"SYSTEMROOT":"C:\\Windows", "CAMOU_CONFIG_1":"injected"},
                writer_active=lambda _: False,
                admission_granted=True,
            )
            assert len(created) == 1
            args, kwargs = created[0]
            assert args == [str(browser), "-no-remote", "-profile", str(userdata)]
            actual = json.loads(kwargs["env"]["CAMOU_CONFIG_1"])
            assert actual["sentinel"] == "pinned"
            assert actual["navigator.maxTouchPoints"] == 7, (
                "upstream SDK must not change pinned identity on Open"
            )
            assert actual["addons"] == ["official/default-addon"]
            assert kwargs["env"]["CAMOU_PREFS_1"] == "native"
            assert len(observed_launch_options) == 1
            assert "playwright" not in str(args).lower()
            assert proc.profile_state() == "WRITER_NOT_OBSERVED"
            assert not hasattr(proc, "process"), "Bridge must not supervise native processes"
            proc.writer_active = lambda _: True
            assert proc.profile_state() == "PROFILE_BUSY"
            assert proc.observed_native_writer
            proc.writer_active = lambda _: False
            assert proc.profile_state() == "QUIESCENT_REQUIRES_FILE_VALIDATION"
            # Lock release is not a Save witness, even after a native crash.
            assert proc.profile_state() != "SAVED"
            def unknown(_):
                raise OSError("untrusted path hidden")
            proc.writer_active = unknown
            assert proc.profile_state() == "UNKNOWN_PRESERVE_LOCAL"
            assert len(created) == 1, "observation must not relaunch the browser"

    print("NATIVE_CAMOUFOX_PROCESS_BOUNDARY=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
