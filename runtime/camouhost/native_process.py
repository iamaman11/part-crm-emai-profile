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
from typing import Any, Callable, Mapping


class NativeLaunchError(RuntimeError):
    """Typed fail-closed error; do not include personal profile data."""


@dataclass
class NativeProfileSession:
    """Hold the Playwright driver connection and observe its native close event."""

    workspace: Path
    writer_active: Callable[[Path], bool]
    manager: Any = field(repr=False)
    context: Any = field(repr=False)
    _closed: asyncio.Event = field(default_factory=asyncio.Event, repr=False)

    def __post_init__(self) -> None:
        if self.context is None:
            raise NativeLaunchError("official Playwright context unavailable")
        # Passive SDK event only: no page, window, process or Firefox internals.
        self.context.on("close", self._on_context_close)
        if self.context.is_closed():
            self._closed.set()

    def _on_context_close(self, *_args: Any) -> None:
        self._closed.set()

    def context_is_closed(self) -> bool:
        context = self.context
        return context is None or bool(context.is_closed())

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
        try:
            with contextlib.redirect_stdout(sys.stderr):
                await manager.__aexit__(None, None, None)
        except BaseException as error:
            try:
                active = self.writer_active(self.workspace)
            except (OSError, RuntimeError) as lock_error:
                raise NativeLaunchError("Playwright driver teardown state is unknown") from lock_error
            if active:
                raise NativeLaunchError("Playwright driver teardown failed while Firefox is active") from error
            raise NativeLaunchError("Playwright driver teardown failed") from error

    async def close_after_browser_exit(self) -> None:
        # Never turn this into a close command. The SDK event must already
        # prove that the context ended, and the OS file lock must be released.
        if not self.context_is_closed():
            raise NativeLaunchError("browser context is still open")
        try:
            active = self.writer_active(self.workspace)
        except (OSError, RuntimeError) as error:
            raise NativeLaunchError("Firefox writer state is unknown") from error
        if active:
            raise NativeLaunchError("Firefox still owns the profile")
        await self._release_playwright()

async def launch_native_camoufox(
    *,
    workspace: Path,
    options: Mapping[str, Any],
    writer_active: Callable[[Path], bool],
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
    if not workspace.is_absolute() or workspace.is_symlink() or not workspace.is_dir():
        raise NativeLaunchError("profile generation workspace unavailable")
    user_data_dir = workspace / "user_data"
    if user_data_dir.is_symlink() or not user_data_dir.is_dir():
        raise NativeLaunchError("browser user-data directory unavailable")
    option_profile = options.get("user_data_dir")
    if (
        not isinstance(option_profile, (str, Path))
        or Path(option_profile).resolve() != user_data_dir.resolve()
    ):
        raise NativeLaunchError("Camoufox options are not bound to the admitted profile")
    executable = options.get("executable_path")
    if not isinstance(executable, (str, Path)) or not Path(executable).is_absolute():
        raise NativeLaunchError("approved Camoufox executable missing")
    if writer_active(workspace):
        raise NativeLaunchError("Firefox already owns this profile")

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
        return NativeProfileSession(
            workspace=workspace,
            writer_active=writer_active,
            manager=manager,
            context=context,
        )
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
