#!/usr/bin/env python3
"""Official async Camoufox Playwright persistent-context owner and passive close witness.

The Bridge proves the device/profile/generation lease before calling this module.
The upstream Camoufox SDK launches and owns the persistent Playwright context.
This module never opens, closes, navigates, evaluates or enumerates user pages.
Firefox owns SessionStore and browser data; the Bridge observes only the native
profile writer lock and Firefox's current-session shutdown checkpoint.
"""
from __future__ import annotations

import contextlib
import json
import stat
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Mapping


class NativeLaunchError(RuntimeError):
    """Typed fail-closed error; do not include personal profile data."""


SESSION_CHECKPOINTS_NAME = "sessionCheckpoints.json"
SESSION_START_CHECKPOINT = "profile-after-change"
SESSION_CLEAN_CLOSE_CHECKPOINT = "sessionstore-final-state-write-complete"
MAX_SESSION_CHECKPOINT_BYTES = 64 * 1024
FILE_ATTRIBUTE_REPARSE_POINT = 0x0400


def _session_checkpoint_path(workspace: Path) -> Path:
    return workspace / "user_data" / SESSION_CHECKPOINTS_NAME


def _session_checkpoint_mtime_ns(workspace: Path) -> int | None:
    path = _session_checkpoint_path(workspace)
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return None
    except OSError as error:
        raise NativeLaunchError("Firefox shutdown checkpoint unavailable") from error
    if (
        path.is_symlink()
        or not stat.S_ISREG(metadata.st_mode)
        or (getattr(metadata, "st_file_attributes", 0) & FILE_ATTRIBUTE_REPARSE_POINT)
    ):
        raise NativeLaunchError("Firefox shutdown checkpoint path unavailable")
    return metadata.st_mtime_ns


def _read_session_checkpoints(workspace: Path) -> tuple[int, dict[str, bool]] | None:
    path = _session_checkpoint_path(workspace)
    try:
        metadata = path.lstat()
        if (
            path.is_symlink()
            or not stat.S_ISREG(metadata.st_mode)
            or (getattr(metadata, "st_file_attributes", 0) & FILE_ATTRIBUTE_REPARSE_POINT)
            or metadata.st_size <= 0
            or metadata.st_size > MAX_SESSION_CHECKPOINT_BYTES
        ):
            return None
        raw = path.read_bytes()
        after = path.lstat()
        if (
            len(raw) != metadata.st_size
            or after.st_size != metadata.st_size
            or after.st_mtime_ns != metadata.st_mtime_ns
        ):
            return None
        checkpoints = json.loads(raw.decode("utf-8"))
    except (OSError, UnicodeDecodeError, ValueError):
        return None
    if not isinstance(checkpoints, dict):
        return None
    if any(not isinstance(key, str) or not isinstance(value, bool)
           for key, value in checkpoints.items()):
        return None
    return metadata.st_mtime_ns, checkpoints


@dataclass
class NativeProfileSession:
    """Hold the SDK manager alive and observe Firefox's own profile lifecycle."""

    workspace: Path
    writer_active: Callable[[Path], bool]
    manager: Any = field(repr=False)
    context: Any = field(repr=False)
    startup_checkpoint_mtime_ns: int | None = None
    observed_native_writer: bool = False
    active_session_checkpoint_seen: bool = False
    clean_shutdown_checkpoint_mtime_ns: int | None = None
    clean_shutdown_invalidated: bool = False

    def profile_state(self) -> str:
        try:
            active = self.writer_active(self.workspace)
        except (OSError, RuntimeError):
            if self.clean_shutdown_checkpoint_mtime_ns is not None:
                self.clean_shutdown_invalidated = True
            return "UNKNOWN_PRESERVE_LOCAL"

        if self.clean_shutdown_checkpoint_mtime_ns is not None:
            if self.clean_shutdown_invalidated or active:
                self.clean_shutdown_invalidated = True
                return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"
            checkpoint = _read_session_checkpoints(self.workspace)
            if (
                checkpoint is None
                or checkpoint[0] != self.clean_shutdown_checkpoint_mtime_ns
            ):
                self.clean_shutdown_invalidated = True
                return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"
            return "CLEAN_SHUTDOWN_VERIFIED"

        if active:
            self.observed_native_writer = True
            checkpoint = _read_session_checkpoints(self.workspace)
            if checkpoint is not None:
                checkpoint_mtime_ns, values = checkpoint
                if (
                    checkpoint_mtime_ns != self.startup_checkpoint_mtime_ns
                    and values.get(SESSION_START_CHECKPOINT) is True
                    and values.get(SESSION_CLEAN_CLOSE_CHECKPOINT) is not True
                ):
                    self.active_session_checkpoint_seen = True
            return "PROFILE_BUSY"

        if not self.observed_native_writer:
            return "WRITER_NOT_OBSERVED"
        if not self.active_session_checkpoint_seen:
            return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"

        checkpoint = _read_session_checkpoints(self.workspace)
        if checkpoint is None:
            return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"
        checkpoint_mtime_ns, values = checkpoint
        if (
            values.get(SESSION_START_CHECKPOINT) is True
            and values.get(SESSION_CLEAN_CLOSE_CHECKPOINT) is True
        ):
            self.clean_shutdown_checkpoint_mtime_ns = checkpoint_mtime_ns
            return "CLEAN_SHUTDOWN_VERIFIED"
        return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"

    async def _release_playwright(self) -> None:
        manager = self.manager
        if manager is None:
            return
        self.manager = None
        self.context = None
        try:
            with contextlib.redirect_stdout(sys.stderr):
                await manager.__aexit__(None, None, None)
        except BaseException as error:
            try:
                active = self.writer_active(self.workspace)
            except (OSError, RuntimeError) as lock_error:
                raise NativeLaunchError("Playwright teardown state is unknown") from lock_error
            if active:
                raise NativeLaunchError("Playwright teardown failed while Firefox is active") from error
            # Camoufox.__exit__ tears down its Playwright driver in a finally
            # block. Firefox is already gone, so duplicate close errors cannot
            # alter or invalidate the native profile bytes.

    async def close(self) -> None:
        if self.profile_state() != "CLEAN_SHUTDOWN_VERIFIED":
            raise NativeLaunchError("clean Firefox shutdown is not verified")
        await self._release_playwright()

    async def close_after_browser_exit(self) -> None:
        try:
            active = self.writer_active(self.workspace)
        except (OSError, RuntimeError) as error:
            raise NativeLaunchError("Firefox writer state is unknown") from error
        if active:
            raise NativeLaunchError("refusing to detach Playwright from a live Firefox profile")
        await self._release_playwright()

    async def abort_startup(self) -> None:
        await self._release_playwright()


async def launch_native_camoufox(
    *,
    workspace: Path,
    options: Mapping[str, Any],
    writer_active: Callable[[Path], bool],
    admission_granted: bool,
    headless: bool = False,
) -> NativeProfileSession:
    """Launch the admitted profile through Camoufox's official Playwright API.

    Options are the pinned output of the upstream launch_options() function.
    from_options prevents the SDK from drawing a second fingerprint on Open.
    The persistent context remains alive for the entire user browser session.
    """
    if not admission_granted:
        raise NativeLaunchError("authoritative writer admission required")
    if headless not in (False, "virtual"):
        raise NativeLaunchError("user Camoufox context must be headful or virtual-headful")
    if options.get("headless") != headless:
        raise NativeLaunchError("Camoufox headless mode differs from approved runtime mode")
    if not workspace.is_absolute() or workspace.is_symlink() or not workspace.is_dir():
        raise NativeLaunchError("profile generation workspace unavailable")
    user_data_dir = workspace / "user_data"
    if user_data_dir.is_symlink() or not user_data_dir.is_dir():
        raise NativeLaunchError("browser user-data directory unavailable")
    if not isinstance(options, Mapping):
        raise NativeLaunchError("official Camoufox options unavailable")
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
    startup_checkpoint_mtime_ns = _session_checkpoint_mtime_ns(workspace)

    from camoufox.async_api import AsyncCamoufox

    manager = AsyncCamoufox(
        from_options=dict(options),
        headless=headless,
        persistent_context=True,
    )
    try:
        # Fixed, nonsecret milestone: distinguish SDK startup from time spent
        # computing pinned options. Never inspect or manipulate user pages.
        print("camouhost_stage phase=launch step=sdk_enter", file=sys.stderr, flush=True)
        with contextlib.redirect_stdout(sys.stderr):
            context = await manager.__aenter__()
    except BaseException as error:
        raise NativeLaunchError("official Camoufox Playwright launch failed") from error
    return NativeProfileSession(
        workspace=workspace,
        writer_active=writer_active,
        manager=manager,
        context=context,
        startup_checkpoint_mtime_ns=startup_checkpoint_mtime_ns,
    )


__all__ = [
    "NativeLaunchError",
    "NativeProfileSession",
    "launch_native_camoufox",
]
