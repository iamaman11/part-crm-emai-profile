#!/usr/bin/env python3
"""Official Camoufox/Playwright launcher for one admitted local profile.

Camoufox and Playwright own the browser lifecycle. The Bridge validates the
local folder, starts the official persistent context, and passively waits for
Playwright's BrowserContext "close" event. It never inspects pages, windows,
processes or Firefox SessionStore, and never sends browser-close commands.
"""
from __future__ import annotations

import asyncio
import contextlib
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Mapping


class NativeLaunchError(RuntimeError):
    """Typed fail-closed error; do not include personal profile data."""


@dataclass
class NativeProfileSession:
    """Hold the Playwright driver and observe its public close/disconnect events."""

    manager: Any = field(repr=False)
    context: Any = field(repr=False)
    _closed: asyncio.Event = field(default_factory=asyncio.Event, repr=False)
    _browser: Any = field(default=None, repr=False)
    _browser_disconnected: bool = field(default=False, repr=False)

    def __post_init__(self) -> None:
        if self.context is None:
            raise NativeLaunchError("official Playwright context unavailable")
        # Passive public SDK events only: no page, window, process or Firefox internals.
        self.context.on("close", self._on_context_close)
        # Official Playwright observation only: a content-tab crash is not a
        # browser Close and must never trigger ProfileBridge Sync or a page action.
        self.context.on("page", self._observe_page_crash)
        for page in self.context.pages:
            self._observe_page_crash(page)
        self._browser = getattr(self.context, "browser", None)
        if self._browser is not None:
            self._browser.on("disconnected", self._on_browser_disconnected)
        if self.context.is_closed():
            self._closed.set()

    def _observe_page_crash(self, page: Any) -> None:
        # Do not inspect a page, its URL, identity, content or cookies.
        page.on("crash", self._on_page_crash)

    def _on_page_crash(self, *_args: Any) -> None:
        # The only data crossing Python -> Rust is this exact constant.
        # A best-effort diagnostic cannot disrupt the user browser.
        try:
            print("camouhost_page_event kind=crash", file=sys.stderr, flush=True)
        except OSError:
            pass

    def _on_context_close(self, *_args: Any) -> None:
        self._closed.set()

    def _on_browser_disconnected(self, *_args: Any) -> None:
        # A browser disconnect is an official Playwright signal for a crash or
        # browser shutdown. Rust still requires native file-lock proof before
        # it may snapshot; this callback never claims the files are released.
        self._browser_disconnected = True
        self._closed.set()

    def context_is_closed(self) -> bool:
        context = self.context
        return (
            self._browser_disconnected
            or context is None
            or bool(context.is_closed())
        )

    async def wait_for_browser_close(self) -> None:
        if self.context_is_closed():
            self._closed.set()
        await self._closed.wait()
        if not self.context_is_closed():
            raise NativeLaunchError("Playwright close event did not confirm a closed context")

    async def _release_playwright(self) -> None:
        manager = self.manager
        if manager is None:
            return
        # async_playwright().__aexit__ stops only the Playwright driver
        # connection. Unlike AsyncCamoufox.__aexit__, it does not call
        # BrowserContext.close() and cannot close a live browser.
        self.manager = None
        self.context = None
        self._browser = None
        try:
            with contextlib.redirect_stdout(sys.stderr):
                await manager.__aexit__(None, None, None)
        except BaseException as error:
            raise NativeLaunchError("Playwright driver teardown failed") from error

    async def close_after_browser_exit(self) -> None:
        # Never turn this into a close command. The official SDK event must
        # already prove that the context ended. Rust owns the native file-lock
        # proof before it permits snapshot and Sync.
        if not self.context_is_closed():
            raise NativeLaunchError("browser context is still open")
        await self._release_playwright()

async def launch_native_camoufox(
    *,
    workspace: Path,
    options: Mapping[str, Any],
    admission_granted: bool,
    headless: bool = False,
) -> NativeProfileSession:
    """Launch through Camoufox's public AsyncNewBrowser + Playwright APIs.

    Options are the pinned output of upstream launch_options(). The returned
    persistent BrowserContext is left entirely under Camoufox/Firefox control.
    """
    if not admission_granted:
        raise NativeLaunchError("authoritative writer admission required")
    if headless not in (False, "virtual"):
        raise NativeLaunchError("user Camoufox context must be headful or virtual-headful")
    if not isinstance(options, Mapping):
        raise NativeLaunchError("official Camoufox options unavailable")
    if options.get("headless") != headless:
        raise NativeLaunchError("Camoufox headless mode differs from approved runtime mode")
    # Rust has already validated the absolute workspace, user_data directory,
    # symlink state and exclusive writer admission. Keep only a lexical binding
    # check so the adapter does not inspect the profile filesystem.
    user_data_dir = workspace / "user_data"
    option_profile = options.get("user_data_dir")
    if not isinstance(option_profile, (str, Path)) or str(option_profile) != str(user_data_dir):
        raise NativeLaunchError("Camoufox options are not bound to the admitted profile")
    executable = options.get("executable_path")
    if not isinstance(executable, (str, Path)) or not Path(executable).is_absolute():
        raise NativeLaunchError("approved Camoufox executable missing")
    from camoufox.async_api import AsyncNewBrowser
    from playwright.async_api import async_playwright

    manager = async_playwright()
    playwright = None
    try:
        print("camouhost_stage phase=launch step=sdk_enter", file=sys.stderr, flush=True)
        with contextlib.redirect_stdout(sys.stderr):
            playwright = await manager.__aenter__()
            context = await AsyncNewBrowser(
                playwright,
                headless=headless,
                from_options=dict(options),
                persistent_context=True,
            )
        return NativeProfileSession(manager=manager, context=context)
    except BaseException as error:
        if playwright is not None:
            with contextlib.suppress(BaseException):
                await manager.__aexit__(type(error), error, error.__traceback__)
        raise NativeLaunchError("official Camoufox Playwright launch failed") from error


__all__ = [
    "NativeLaunchError",
    "NativeProfileSession",
    "launch_native_camoufox",
]
