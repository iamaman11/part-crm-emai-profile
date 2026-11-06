#!/usr/bin/env python3
"""Native Camoufox profile launcher; no Playwright, tabs, DOM, or browser commands.

This is the next-generation launch primitive. The Bridge's Rust control-plane
owner MUST prove and redeem the device/profile/tenant lease and immutable
generation before invoking this module. It does not replace current IPC yet.

The official Camoufox SDK owns fingerprint serialization and default extensions;
Firefox owns SessionStore, navigation, window/tab closure, and network state.
"""
from __future__ import annotations

import copy
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Mapping


class NativeLaunchError(RuntimeError):
    """Typed fail-closed error; do not include personal profile data."""


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
    from camoufox.utils import launch_options

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
    return {key: str(value) for key, value in official_env.items()}


@dataclass
class NativeProfileSession:
    """Observe only Firefox's native profile lock, never its processes or UI.

    Lock release is merely permission to attempt an independently validated
    consistent file snapshot, NOT a successful Save or a clean-exit claim.
    """

    workspace: Path
    writer_active: Callable[[Path], bool]
    observed_native_writer: bool = False

    def profile_state(self) -> str:
        try:
            active = self.writer_active(self.workspace)
        except (OSError, RuntimeError):
            return "UNKNOWN_PRESERVE_LOCAL"
        if active:
            self.observed_native_writer = True
            return "PROFILE_BUSY"
        if not self.observed_native_writer:
            # Cannot infer successful native launch from the absence of a lock.
            return "WRITER_NOT_OBSERVED"
        # A crash may also release the lock. The Save owner must separately
        # validate the exact native files and make a durable checkpoint.
        return "QUIESCENT_REQUIRES_FILE_VALIDATION"


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
    # Only the native Firefox profile lock is observed after this launch.
    return NativeProfileSession(workspace, writer_active)


__all__ = [
    "NativeLaunchError",
    "NativeProfileSession",
    "prepare_native_environment",
    "launch_native_camoufox",
]
