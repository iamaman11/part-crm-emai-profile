#!/usr/bin/env python3
"""Official Camoufox Playwright persistent-context boundary tests."""
from __future__ import annotations

import importlib.util
import json
import os
import pathlib
import sys
import tempfile
import types
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "runtime" / "camouhost" / "native_process.py"


def load():
    spec = importlib.util.spec_from_file_location("camouhost_playwright_process_test", SOURCE)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write_checkpoints(path, values, previous_mtime=None):
    path.write_text(json.dumps(values), encoding="utf-8")
    if previous_mtime is not None:
        observed = path.stat().st_mtime_ns
        target = max(observed, previous_mtime + 1_000_000_000)
        os.utime(path, ns=(target, target))


def main() -> int:
    module = load()
    assert "subprocess" not in module.__dict__, "user Open must not Popen the browser executable"

    class FakeContext:
        # Deliberately no page/DOM/navigation APIs: the adapter must not call them.
        pass

    class FakeCamoufox:
        instances = []

        def __init__(self, **kwargs):
            self.kwargs = kwargs
            self.context = FakeContext()
            self.exit_calls = 0
            self.instances.append(self)

        def __enter__(self):
            return self.context

        def __exit__(self, *_args):
            self.exit_calls += 1
            return False

    package = types.ModuleType("camoufox")
    package.__path__ = []
    sync_api = types.ModuleType("camoufox.sync_api")
    sync_api.Camoufox = FakeCamoufox

    with tempfile.TemporaryDirectory() as temporary:
        workspace = pathlib.Path(temporary) / "generation"
        user_data = workspace / "user_data"
        user_data.mkdir(parents=True)
        browser = pathlib.Path(temporary) / "camoufox.exe"
        browser.write_bytes(b"approved disposable browser fixture")
        checkpoint = user_data / "sessionCheckpoints.json"
        write_checkpoints(checkpoint, {
            "profile-after-change": True,
            "sessionstore-final-state-write-complete": True,
        })
        active = False

        def writer_active(_workspace):
            return active

        options = {
            "executable_path": str(browser),
            "user_data_dir": str(user_data),
            "headless": False,
            "env": {"CAMOU_CONFIG_1": "{}"},
        }

        with patch.dict(sys.modules, {
            "camoufox": package,
            "camoufox.sync_api": sync_api,
        }):
            for admission, writer in ((False, False), (True, True)):
                active = writer
                try:
                    module.launch_native_camoufox(
                        workspace=workspace, options=options,
                        writer_active=writer_active, admission_granted=admission,
                        headless=False,
                    )
                except module.NativeLaunchError:
                    pass
                else:
                    raise AssertionError("unauthorized/already-owned profile launched")
                assert not FakeCamoufox.instances

            active = False
            session = module.launch_native_camoufox(
                workspace=workspace, options=options,
                writer_active=writer_active, admission_granted=True, headless=False,
            )
            manager = FakeCamoufox.instances[-1]
            assert manager.kwargs == {
                "from_options": options,
                "headless": False,
                "persistent_context": True,
            }
            assert session.context is manager.context
            assert session.profile_state() == "WRITER_NOT_OBSERVED"

            active = True
            baseline = checkpoint.stat().st_mtime_ns
            write_checkpoints(checkpoint, {"profile-after-change": True}, baseline)
            assert session.profile_state() == "PROFILE_BUSY"
            assert session.active_session_checkpoint_seen

            active = False
            baseline = checkpoint.stat().st_mtime_ns
            write_checkpoints(checkpoint, {
                "profile-after-change": True,
                "sessionstore-final-state-write-complete": True,
            }, baseline)
            assert session.profile_state() == "CLEAN_SHUTDOWN_VERIFIED"
            session.close()
            assert manager.exit_calls == 1
            assert session.manager is None and session.context is None

            active = True
            baseline = checkpoint.stat().st_mtime_ns
            write_checkpoints(checkpoint, {
                "profile-after-change": True,
                "sessionstore-final-state-write-complete": True,
            }, baseline)
            assert session.profile_state() == "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"

            active = False
            reopened = module.launch_native_camoufox(
                workspace=workspace, options=options,
                writer_active=writer_active, admission_granted=True, headless=False,
            )
            reopened_manager = FakeCamoufox.instances[-1]
            active = True
            baseline = checkpoint.stat().st_mtime_ns
            write_checkpoints(checkpoint, {"profile-after-change": True}, baseline)
            assert reopened.profile_state() == "PROFILE_BUSY"
            active = False
            assert reopened.profile_state() == "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"
            reopened.close_after_browser_exit()
            assert reopened_manager.exit_calls == 1

            active = False
            startup = module.launch_native_camoufox(
                workspace=workspace, options=options,
                writer_active=writer_active, admission_granted=True, headless=False,
            )
            startup_manager = FakeCamoufox.instances[-1]
            startup.abort_startup()
            assert startup_manager.exit_calls == 1

            def unknown(_workspace):
                raise OSError("untrusted lock path")
            unknown_session = module.launch_native_camoufox(
                workspace=workspace, options=options,
                writer_active=writer_active, admission_granted=True, headless=False,
            )
            unknown_session.writer_active = unknown
            assert unknown_session.profile_state() == "UNKNOWN_PRESERVE_LOCAL"
            unknown_session.writer_active = writer_active
            unknown_session.abort_startup()

    print("CAMOUHOST_OFFICIAL_PLAYWRIGHT_CONTEXT=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
