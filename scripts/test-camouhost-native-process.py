#!/usr/bin/env python3
"""Offline native process boundary tests, without building or launching Firefox."""
from __future__ import annotations

import importlib.util
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
    addon = types.ModuleType("camoufox.addons")
    util = types.ModuleType("camoufox.utils")
    root = types.ModuleType("camoufox")
    def default_addons(paths):
        paths.append("vendor/default-addon")
    addon.add_default_addons = default_addons
    addon.confirm_paths = lambda paths: None
    util.get_env_vars = lambda config, target, path: {
        "CAMOU_CONFIG_1": "pinned" if config["sentinel"] == "pinned" and target == "win" else "unexpected",
    }
    util.get_pref_env_vars = lambda prefs: {
        "CAMOU_PREFS_1": "native" if prefs.get("browser.startup.page") == 3 else "unexpected"
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
            "camoufox.addons": addon,
            "camoufox.utils": util,
        }
        with patch.dict(sys.modules, modules), patch.object(subprocess, "Popen", side_effect=spawn):
            for approved, active in ((False, False), (True, True)):
                try:
                    module.launch_native_camoufox(
                        workspace=workspace, browser=browser, identity={"sentinel":"pinned"},
                        inherited={"SYSTEMROOT":"C:\\Windows"}, writer_active=lambda _: active,
                        admission_granted=approved,
                    )
                except module.NativeLaunchError:
                    pass
                else:
                    raise AssertionError("unauthorized or already-owned profile launched")
                assert not created

            proc = module.launch_native_camoufox(
                workspace=workspace, browser=browser, identity={"sentinel":"pinned"},
                inherited={"SYSTEMROOT":"C:\\Windows", "CAMOU_CONFIG_1":"injected"},
                writer_active=lambda _: False,
                admission_granted=True,
            )
            assert len(created) == 1
            args, kwargs = created[0]
            assert args == [str(browser), "-no-remote", "-profile", str(userdata)]
            assert kwargs["env"]["CAMOU_CONFIG_1"] == "pinned"
            assert kwargs["env"]["CAMOU_PREFS_1"] == "native"
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
