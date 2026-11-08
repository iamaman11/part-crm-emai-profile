#!/usr/bin/env python3
"""Official Camoufox + passive Playwright close-event boundary tests."""
from __future__ import annotations

import asyncio
import importlib.util
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


class FakeContext:
    """No page/window/process API: only the documented Playwright close event."""

    def __init__(self, closed=False):
        self.closed = closed
        self.handlers = []
        self.close_calls = 0

    def on(self, event, handler):
        assert event == "close"
        self.handlers.append(handler)

    def is_closed(self):
        return self.closed

    def user_close(self):
        self.closed = True
        for handler in self.handlers:
            handler(self)


class FakePlaywrightManager:
    instances = []

    def __init__(self):
        self.exit_calls = 0
        self.playwright = object()
        self.instances.append(self)

    async def __aenter__(self):
        return self.playwright

    async def __aexit__(self, *_args):
        self.exit_calls += 1
        return False


async def verify() -> int:
    module = load()
    source = SOURCE.read_text(encoding="utf-8")
    assert "sessionCheckpoints.json" not in source
    assert "EnumWindows" not in source and "Get-Process" not in source
    assert "PostMessage" not in source and "WM_CLOSE" not in source
    assert "subprocess" not in module.__dict__
    assert "writer_active" not in source

    contexts = []
    launches = []

    async def async_new_browser(playwright, **kwargs):
        context = FakeContext()
        contexts.append(context)
        launches.append((playwright, kwargs))
        return context

    package = types.ModuleType("camoufox")
    package.__path__ = []
    camoufox_api = types.ModuleType("camoufox.async_api")
    camoufox_api.AsyncNewBrowser = async_new_browser
    playwright_package = types.ModuleType("playwright")
    playwright_package.__path__ = []
    playwright_api = types.ModuleType("playwright.async_api")
    playwright_api.async_playwright = FakePlaywrightManager

    with tempfile.TemporaryDirectory() as temporary:
        workspace = pathlib.Path(temporary) / "generation"
        user_data = workspace / "user_data"
        user_data.mkdir(parents=True)
        browser = pathlib.Path(temporary) / "camoufox.exe"
        browser.write_bytes(b"approved disposable browser fixture")
        options = {
            "executable_path": str(browser),
            "user_data_dir": str(user_data),
            "headless": False,
            "env": {"CAMOU_CONFIG_1": "{}"},
        }
        with patch.dict(sys.modules, {
            "camoufox": package,
            "camoufox.async_api": camoufox_api,
            "playwright": playwright_package,
            "playwright.async_api": playwright_api,
        }):
            try:
                await module.launch_native_camoufox(
                    workspace=workspace, options=options,
                    admission_granted=False, headless=False,
                )
            except module.NativeLaunchError:
                pass
            else:
                raise AssertionError("unadmitted profile launched")
            assert not launches

            session = await module.launch_native_camoufox(
                workspace=workspace, options=options,
                admission_granted=True, headless=False,
            )
            manager = FakePlaywrightManager.instances[-1]
            assert launches[-1][0] is manager.playwright
            assert launches[-1][1] == {
                "headless": False,
                "from_options": options,
                "persistent_context": True,
            }
            assert session.context is contexts[-1]
            assert len(contexts[-1].handlers) == 1

            waiting = asyncio.create_task(session.wait_for_browser_close())
            await asyncio.sleep(0)
            assert not waiting.done(), "close waiter must not invent a close"
            contexts[-1].user_close()
            await waiting
            assert session.context_is_closed()

            await session.close_after_browser_exit()
            assert manager.exit_calls == 1
            assert contexts[-1].close_calls == 0
            assert session.manager is None and session.context is None

            # A close racing listener registration is still observed.
            already_closed = FakeContext(closed=True)
            async def closed_browser(playwright, **kwargs):
                launches.append((playwright, kwargs))
                return already_closed
            camoufox_api.AsyncNewBrowser = closed_browser
            raced = await module.launch_native_camoufox(
                workspace=workspace, options=options,
                admission_granted=True, headless=False,
            )
            await asyncio.wait_for(raced.wait_for_browser_close(), timeout=1)
            await raced.close_after_browser_exit()
            assert already_closed.close_calls == 0

            # If the official launch fails before a context is admitted,
            # only the Playwright driver connection is released.
            async def failed_browser(_playwright, **_kwargs):
                raise RuntimeError("synthetic SDK launch failure")
            camoufox_api.AsyncNewBrowser = failed_browser
            manager_count = len(FakePlaywrightManager.instances)
            try:
                await module.launch_native_camoufox(
                    workspace=workspace, options=options,
                    admission_granted=True, headless=False,
                )
            except module.NativeLaunchError:
                pass
            else:
                raise AssertionError("failed SDK launch unexpectedly succeeded")
            assert len(FakePlaywrightManager.instances) == manager_count + 1
            assert FakePlaywrightManager.instances[-1].exit_calls == 1

            virtual_options = dict(options)
            virtual_options["headless"] = "virtual"
            async def virtual_browser(playwright, **kwargs):
                launches.append((playwright, kwargs))
                virtual_context = FakeContext()
                contexts.append(virtual_context)
                return virtual_context
            camoufox_api.AsyncNewBrowser = virtual_browser
            virtual = await module.launch_native_camoufox(
                workspace=workspace, options=virtual_options,
                admission_granted=True, headless="virtual",
            )
            assert launches[-1][1]["headless"] == "virtual"
            virtual.context.user_close()
            await virtual.wait_for_browser_close()
            await virtual.close_after_browser_exit()

    print("CAMOUHOST_PASSIVE_PLAYWRIGHT_CLOSE=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(verify()))
