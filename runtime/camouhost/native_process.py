#!/usr/bin/env python3
"""Native Camoufox launch and browser-owned shutdown observation.

The Bridge's Rust control-plane owner MUST prove and redeem the device/profile/
tenant lease and immutable generation before invoking this module. The official
Camoufox SDK owns fingerprint serialization; Firefox owns SessionStore,
navigation, window/tab closure, and network state.

Clean-close observation is passive: native profile lock plus Firefox
CrashMonitor's current-session checkpoint. No process tree, PID, Playwright
context, tabs, DOM, or page commands are used.
"""
from __future__ import annotations

import copy
import json
import stat
import subprocess
from dataclasses import dataclass
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
        raise NativeLaunchError("native shutdown checkpoint unavailable") from error
    if (
        path.is_symlink()
        or not stat.S_ISREG(metadata.st_mode)
        or (getattr(metadata, "st_file_attributes", 0) & FILE_ATTRIBUTE_REPARSE_POINT)
        or metadata.st_size > MAX_SESSION_CHECKPOINT_BYTES
    ):
        raise NativeLaunchError("native shutdown checkpoint path unavailable")
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



def prepare_native_environment(
    *,
    browser: Path,
    identity: Mapping[str, Any],
    inherited: Mapping[str, str],
    ff_version: int = 158,
    firefox_prefs: Mapping[str, Any] | None = None,
) -> dict[str, str]:
    """Generate startup environment using the official Camoufox SDK only.

    Same upstream config, preference, extension and cache policy as ordinary
    Camoufox launch, without creating a Playwright context or touching pages.
    """
    if not browser.is_absolute() or browser.is_symlink() or not browser.is_file():
        raise NativeLaunchError("approved browser executable missing")
    if not isinstance(identity, Mapping) or not identity:
        raise NativeLaunchError("immutable browser fingerprint missing")
    if any(not isinstance(k, str) for k in identity):
        raise NativeLaunchError("invalid fingerprint keys")
    if type(ff_version) is not int or not 100 <= ff_version <= 999:
        raise NativeLaunchError("locked Firefox major invalid")
    if any(not isinstance(k, str) or not isinstance(v, str) for k, v in inherited.items()):
        raise NativeLaunchError("inherited environment contains invalid values")
    # The source must not be able to override the signed Camoufox identity.
    inherited_clean = {
        k: v for k, v in inherited.items()
        if not k.startswith(("CAMOU_CONFIG_", "CAMOU_PREFS_"))
    }
    from camoufox.utils import get_env_vars, launch_options

    try:
        launch = launch_options(
            executable_path=browser,
            config=copy.deepcopy(dict(identity)),
            enable_cache=True,
            env=inherited_clean,
            exclude_addons=[],
            ff_version=ff_version,
            firefox_user_prefs=dict(firefox_prefs or {}),
            headless=False,
            i_know_what_im_doing=True,
            os="windows",
        )
    except (OSError, ValueError, TypeError) as error:
        raise NativeLaunchError("official Camoufox startup configuration rejected") from error
    official_env = launch.get("env")
    if not isinstance(official_env, dict) or not any(
        isinstance(key, str) and key.startswith("CAMOU_CONFIG_") for key in official_env
    ):
        raise NativeLaunchError("upstream startup fingerprint missing")
    if any(not isinstance(key, str) for key in official_env):
        raise NativeLaunchError("upstream startup environment invalid")

    # The upstream launcher can re-randomize a persisted fingerprint field
    # (physically reproduced with navigator.maxTouchPoints across six opens).
    # Let Camoufox own the startup preferences and default addons, but retain
    # *exactly* the sealed identity supplied by the generation owner. This is
    # not a second fingerprint generator or a change to Firefox internals.
    chunks = sorted(
        (int(key.rsplit("_", 1)[1]), str(value))
        for key, value in official_env.items()
        if key.startswith("CAMOU_CONFIG_") and key.rsplit("_", 1)[1].isdigit()
    )
    try:
        generated = json.loads("".join(value for _, value in chunks))
    except (ValueError, TypeError) as error:
        raise NativeLaunchError("official startup fingerprint malformed") from error
    if not isinstance(generated, dict):
        raise NativeLaunchError("official startup fingerprint invalid")
    pinned = copy.deepcopy(dict(identity))
    addons = generated.get("addons")
    if addons is not None:
        if not isinstance(addons, list) or any(
            not isinstance(value, str) for value in addons
        ):
            raise NativeLaunchError("official browser addons invalid")
        if "addons" in pinned and pinned["addons"] != addons:
            raise NativeLaunchError("persisted addon identity conflicts with upstream")
        pinned["addons"] = addons
    # Official get_env_vars serializes the pinned identity in the same chunked
    # format used at Firefox STARTUP, without launch_options re-generation.
    try:
        pinned_env = get_env_vars(pinned, "win", path=browser)
    except (OSError, TypeError, ValueError) as error:
        raise NativeLaunchError("pinned native fingerprint unavailable") from error
    merged = {
        key: str(value) for key, value in official_env.items()
        if not key.startswith("CAMOU_CONFIG_")
    }
    merged.update({key: str(value) for key, value in pinned_env.items()})
    return merged


@dataclass
class NativeProfileSession:
    """Observe Firefox's native writer lock and current-session shutdown proof.

    The native lock alone admits Open/Ready. Save additionally requires this
    session's startup checkpoint and Firefox's final SessionStore checkpoint;
    a released lock alone is never evidence of a clean shutdown.
    """

    workspace: Path
    writer_active: Callable[[Path], bool]
    startup_checkpoint_mtime_ns: int | None = None
    observed_native_writer: bool = False
    active_session_checkpoint_seen: bool = False

    def profile_state(self) -> str:
        try:
            active = self.writer_active(self.workspace)
        except (OSError, RuntimeError):
            return "UNKNOWN_PRESERVE_LOCAL"

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
            # The OS-held native lock is sufficient for Open/Ready. The
            # current-session checkpoint is an independent Save/clean-close
            # witness and must never add startup delay.
            return "PROFILE_BUSY"

        if not self.observed_native_writer:
            return "WRITER_NOT_OBSERVED"
        if not self.active_session_checkpoint_seen:
            return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"

        checkpoint = _read_session_checkpoints(self.workspace)
        if checkpoint is None:
            return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"
        _, values = checkpoint
        if (
            values.get(SESSION_START_CHECKPOINT) is True
            and values.get(SESSION_CLEAN_CLOSE_CHECKPOINT) is True
        ):
            return "CLEAN_SHUTDOWN_VERIFIED"
        return "CRASH_OR_UNKNOWN_PRESERVE_LOCAL"


def launch_native_camoufox(
    *,
    workspace: Path,
    browser: Path,
    identity: Mapping[str, Any],
    inherited: Mapping[str, str],
    writer_active: Callable[[Path], bool],
    admission_granted: bool,
    ff_version: int = 158,
) -> NativeProfileSession:
    """Start exactly one stock browser without Playwright after owner admission.

    Caller owns the exclusive Bridge workspace lock and native Firefox writer
    preflight. Failed admission must not start any browser or touch user data.
    """
    if not admission_granted:
        raise NativeLaunchError("authoritative writer admission required")
    if not workspace.is_absolute() or workspace.is_symlink() or not workspace.is_dir():
        raise NativeLaunchError("native generation workspace unavailable")
    user_data_dir = workspace / "user_data"
    if user_data_dir.is_symlink() or not user_data_dir.is_dir():
        raise NativeLaunchError("native user data unavailable")
    if writer_active(workspace):
        raise NativeLaunchError("Firefox already owns this profile")
    startup_checkpoint_mtime_ns = _session_checkpoint_mtime_ns(workspace)

    env = prepare_native_environment(
        browser=browser,
        identity=identity,
        inherited=inherited,
        ff_version=ff_version,
        firefox_prefs={
            "browser.startup.page": 3,
            "privacy.baselineFingerprintingProtection": False,
            "accessibility.force_disabled": 0,
        },
    )
    # -no-remote prohibits joining an unrelated running Firefox. -profile
    # binds the already-authorized generation. No URL or page command is passed.
    try:
        subprocess.Popen(
            [str(browser), "-no-remote", "-profile", str(user_data_dir)],
            cwd=str(browser.parent),
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
        )
    except OSError as error:
        raise NativeLaunchError("native Camoufox process launch failed") from error
    # No process polling, process tree, browser hooks or PID-based Save proof.
    # Startup and clean shutdown are witnessed by Firefox's own checkpoint file.
    return NativeProfileSession(workspace, writer_active, startup_checkpoint_mtime_ns)


__all__ = [
    "NativeLaunchError",
    "NativeProfileSession",
    "prepare_native_environment",
    "launch_native_camoufox",
]
