#!/usr/bin/env python3
"""Native Camoufox process owner; no Playwright, tabs, DOM, or browser commands.

This is the next-generation launch primitive. The Bridge's Rust control-plane
owner MUST prove and redeem the device/profile/tenant lease and immutable
generation before invoking this module. It does not replace current IPC yet.

The official Camoufox SDK owns fingerprint serialization and default extensions;
Firefox owns SessionStore, navigation, window/tab closure, and network state.
"""
from __future__ import annotations

import copy
import os
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
    firefox_prefs: Mapping[str, Any] | None = None,
) -> dict[str, str]:
    """Serialize the already-persisted fingerprint through official Camoufox.

    Never redraw identity, navigate, set offline mode, inspect DOM, or call
    Playwright. Only the upstream launch API may add its stock addons.
    """
    if not browser.is_absolute() or browser.is_symlink() or not browser.is_file():
        raise NativeLaunchError("approved browser executable missing")
    if not isinstance(identity, Mapping) or not identity:
        raise NativeLaunchError("immutable browser fingerprint missing")
    if any(not isinstance(k, str) for k in identity):
        raise NativeLaunchError("invalid fingerprint keys")

    from camoufox.addons import add_default_addons, confirm_paths
    from camoufox.utils import get_env_vars, get_pref_env_vars

    config = copy.deepcopy(dict(identity))
    # Camoufox 0.5.7's normal launch_options() invokes this same upstream
    # addon routine, but we do not ask Playwright to start or own Firefox.
    # A failed official addon resolution is an explicit launch error.
    addons: list[str] = []
    add_default_addons(addons)
    if addons:
        confirm_paths(addons)
        config["addons"] = addons

    try:
        upstream = {
            **get_env_vars(config, "win", path=browser),
            **get_pref_env_vars(dict(firefox_prefs or {})),
        }
    except (OSError, ValueError, TypeError) as error:
        raise NativeLaunchError("upstream browser configuration unavailable") from error
    # Deliberately accept ONLY prevalidated Windows OS variables from the
    # caller: no ambient secrets or arbitrary CAMOU_CONFIG overrides.
    if any(not isinstance(k, str) or not isinstance(v, str) for k, v in inherited.items()):
        raise NativeLaunchError("inherited environment contains invalid values")
    inherited_clean = {
        k: v for k, v in inherited.items()
        if not k.startswith(("CAMOU_CONFIG_", "CAMOU_PREFS_"))
    }
    return {**inherited_clean, **{key: str(value) for key, value in upstream.items()}}


@dataclass
class NativeCamoufoxProcess:
    """Read-only operating-system lifecycle witness for the native browser."""

    process: subprocess.Popen[bytes]
    workspace: Path
    writer_active: Callable[[Path], bool]
    observed_native_writer: bool = False
    browser_exit_witness: Any | None = None

    def exit_state(self) -> str:
        """A zero-exit launcher is NEVER proof that native Firefox has closed.

        GUI launchers can exit successfully after delegating to a browser
        process which remains alive. Require independent native profile lock
        activity observed during this exact session before a clean close can
        become a candidate for Save. No URL, tab or DOM event is consulted.
        """
        try:
            active = self.writer_active(self.workspace)
        except (OSError, RuntimeError):
            return "UNKNOWN_PRESERVE_LOCAL"
        if active:
            self.observed_native_writer = True
            return "RUNNING"
        status = self.process.poll()
        if status is None:
            return "STARTING_OR_QUIESCING"
        if status != 0:
            return "ABNORMAL_EXIT_PRESERVE_LOCAL"
        if not self.observed_native_writer or self.browser_exit_witness is None:
            # Windows GUI bootstrap may exit 0 while Firefox descendants
            # continue. A retained *native browser* OS handle is mandatory.
            return "UNKNOWN_PRESERVE_LOCAL"
        try:
            if not self.browser_exit_witness.safely_exited():
                return "RUNNING_OR_ABNORMAL_PRESERVE_LOCAL"
        except (OSError, RuntimeError):
            return "UNKNOWN_PRESERVE_LOCAL"
        return "CLOSED_AND_UNLOCKED"


def launch_native_camoufox(
    *,
    workspace: Path,
    browser: Path,
    identity: Mapping[str, Any],
    inherited: Mapping[str, str],
    writer_active: Callable[[Path], bool],
    admission_granted: bool,
    witness_factory: Callable[[], Any] | None = None,
) -> NativeCamoufoxProcess:
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
        firefox_prefs={"browser.startup.page": 3},
    )
    # -no-remote prohibits joining an unrelated running Firefox. -profile
    # binds the already-authorized generation. No URL or page command is passed.
    try:
        process = subprocess.Popen(
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
    # The caller may supply a WindowsBrowserExitWitness rooted in this exact
    # already-authorized process instance; its held OS handle defeats PID reuse.
    # Failure to capture the witness cannot produce a positive Save. Never
    # terminate a possibly-running user browser merely to recover bookkeeping.
    witness = None
    if witness_factory is not None:
        try:
            witness = witness_factory()
        except (OSError, RuntimeError):
            pass
    return NativeCamoufoxProcess(
        process, workspace, writer_active, browser_exit_witness=witness
    )


__all__ = [
    "NativeLaunchError",
    "NativeCamoufoxProcess",
    "prepare_native_environment",
    "launch_native_camoufox",
]
