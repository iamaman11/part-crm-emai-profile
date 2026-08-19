#!/usr/bin/env python3
"""Supported real Camouhost adapter for generation-scoped Camoufox execution.

Stdout is reserved for the versioned Bridge IPC protocol. Runtime/library diagnostics
are redirected to stderr and never include profile payload, fingerprint config, proxy
credentials, or entropy material.
"""

from __future__ import annotations

import contextlib
import copy
import hashlib
import importlib.metadata
import json
import os
import re
import sys
from pathlib import Path
from typing import Any

IPC_VERSION = "1"
MAX_FRAME_LENGTH = 512
MAX_CONFIG_BYTES = 1024 * 1024
MAX_URL_BYTES = 2048
SESSION_PATTERN = re.compile(r"[A-Za-z0-9_-]{8,96}\Z")
CONFIG_NAME = "camoufox-config.json"
USER_DATA_NAME = "user_data"
BRIDGE_LOCK_NAME = ".profile-platform.lock"
RUNTIME_LOCK_NAME = "runtime-lock.json"

PROFILE_ROOT_ENV = "CAMOUHOST_PROFILE_ROOT"
RUNTIME_LOCK_ENV = "CAMOUHOST_RUNTIME_LOCK"
EXPECTED_RUNTIME_LOCK_SHA256_ENV = "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256"
EXPECTED_CONFIG_SHA256_ENV = "CAMOUHOST_EXPECTED_CONFIG_SHA256"
EXPECTED_PROBE_SHA256_ENV = "CAMOUHOST_EXPECTED_PROBE_SHA256"
INITIAL_URL_ENV = "CAMOUHOST_INITIAL_URL"
HEADLESS_MODE_ENV = "CAMOUHOST_HEADLESS_MODE"
PROXY_CONFIG_ENV = "CAMOUHOST_PROXY_CONFIG_PATH"

FINGERPRINT_PROBE = """
() => ({
  userAgent: navigator.userAgent,
  platform: navigator.platform,
  language: navigator.language,
  languages: Array.from(navigator.languages || []),
  hardwareConcurrency: navigator.hardwareConcurrency,
  deviceMemory: navigator.deviceMemory,
  screen: {
    width: screen.width,
    height: screen.height,
    availWidth: screen.availWidth,
    availHeight: screen.availHeight,
    colorDepth: screen.colorDepth,
    devicePixelRatio: window.devicePixelRatio,
  },
  webgl: (() => {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl');
    if (!gl) return null;
    const debug = gl.getExtension('WEBGL_debug_renderer_info');
    return {
      vendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
      renderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
    };
  })(),
})
"""


class RuntimeContractError(ValueError):
    """Raised when the repository-owned runtime contract is not satisfied."""


def emit(frame: str) -> None:
    sys.stdout.write(frame + "\n")
    sys.stdout.flush()


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def valid_sha256(value: str | None) -> bool:
    return value is not None and bool(re.fullmatch(r"[0-9a-f]{64}", value))


def read_regular_file(path: Path, maximum_bytes: int | None = None) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise RuntimeContractError("required runtime file is not a regular file")
    if maximum_bytes is not None and path.stat().st_size > maximum_bytes:
        raise RuntimeContractError("required runtime file exceeds its bounded size")
    return path.read_bytes()


def load_canonical_json(path: Path, maximum_bytes: int | None = None) -> tuple[dict[str, Any], bytes]:
    raw = read_regular_file(path, maximum_bytes)
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeContractError("runtime JSON is invalid") from error
    if not isinstance(value, dict) or canonical_json(value) != raw:
        raise RuntimeContractError("runtime JSON is not canonical")
    return value, raw


def browser_environment() -> dict[str, str]:
    allowed = {
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "GDK_BACKEND",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "LOGNAME",
        "PATH",
        "SHELL",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USER",
        "USERPROFILE",
        "WAYLAND_DISPLAY",
        "WSL_DISTRO_NAME",
        "WSL_INTEROP",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_RUNTIME_DIR",
    }
    return {key: value for key, value in os.environ.items() if key in allowed}


def runtime_lock_path() -> Path:
    configured = os.environ.get(RUNTIME_LOCK_ENV)
    path = Path(configured) if configured else Path(__file__).with_name(RUNTIME_LOCK_NAME)
    if not path.is_absolute():
        path = path.resolve()
    return path


def load_runtime_lock() -> tuple[dict[str, Any], str]:
    lock, raw = load_canonical_json(runtime_lock_path(), MAX_CONFIG_BYTES)
    required_top = {
        "browser",
        "camouhost_ipc_version",
        "components",
        "fingerprint_config_schema",
        "fingerprint_policy_version",
        "python",
        "python_source",
        "runtime_role",
        "schema_version",
    }
    if set(lock) != required_top:
        raise RuntimeContractError("runtime lock shape is invalid")
    if lock.get("schema_version") != 1 or lock.get("runtime_role") != "real_camoufox":
        raise RuntimeContractError("runtime lock identity is unsupported")
    if lock.get("camouhost_ipc_version") != int(IPC_VERSION):
        raise RuntimeContractError("runtime IPC identity is unsupported")
    expected = os.environ.get(EXPECTED_RUNTIME_LOCK_SHA256_ENV)
    digest = sha256_bytes(raw)
    if expected is not None and (not valid_sha256(expected) or expected != digest):
        raise RuntimeContractError("runtime lock digest mismatch")
    return lock, digest


def verify_python_components(lock: dict[str, Any]) -> None:
    components = lock.get("components")
    if not isinstance(components, dict) or set(components) != {
        "browserforge",
        "camoufox_python",
        "playwright",
    }:
        raise RuntimeContractError("runtime component lock is invalid")
    distributions = {
        "browserforge": "browserforge",
        "camoufox_python": "camoufox",
        "playwright": "playwright",
    }
    for key, distribution in distributions.items():
        expected = components.get(key)
        if not isinstance(expected, str) or importlib.metadata.version(distribution) != expected:
            raise RuntimeContractError("installed runtime component does not match lock")


def resolve_headless_mode() -> bool | str:
    value = os.environ.get(HEADLESS_MODE_ENV, "false")
    if value == "false":
        return False
    if value == "virtual":
        return "virtual"
    raise RuntimeContractError("unsupported Camouhost headless mode")


def resolve_initial_url() -> str | None:
    value = os.environ.get(INITIAL_URL_ENV)
    if value is None or value == "":
        return None
    if len(value.encode("utf-8")) > MAX_URL_BYTES or "\n" in value or "\r" in value:
        raise RuntimeContractError("initial URL is invalid")
    if not value.startswith(("https://", "http://", "about:")):
        raise RuntimeContractError("initial URL scheme is not permitted")
    return value


def resolve_proxy() -> dict[str, str] | None:
    configured = os.environ.get(PROXY_CONFIG_ENV)
    if configured is None:
        return None
    path = Path(configured)
    if not path.is_absolute():
        raise RuntimeContractError("proxy config path must be absolute")
    proxy, _ = load_canonical_json(path, 16 * 1024)
    if not set(proxy).issubset({"server", "username", "password", "bypass"}) or "server" not in proxy:
        raise RuntimeContractError("proxy config shape is invalid")
    if not all(isinstance(value, str) and value for value in proxy.values()):
        raise RuntimeContractError("proxy config values are invalid")
    return proxy


def require_bridge_writer_lock(root: Path) -> None:
    bridge_lock = root / BRIDGE_LOCK_NAME
    if bridge_lock.is_symlink() or not bridge_lock.is_file():
        raise RuntimeContractError("Bridge writer ownership evidence is missing")


def resolve_profile_root(require_bridge_lock: bool) -> Path:
    raw = os.environ.get(PROFILE_ROOT_ENV)
    if raw is None:
        raise RuntimeContractError("profile root is missing")
    candidate = Path(raw)
    if not candidate.is_absolute() or candidate.is_symlink() or not candidate.is_dir():
        raise RuntimeContractError("profile root is invalid")
    root = candidate.resolve(strict=True)
    if require_bridge_lock:
        require_bridge_writer_lock(root)
    return root


def load_generation_config(root: Path) -> tuple[dict[str, Any], str]:
    config, raw = load_canonical_json(root / CONFIG_NAME, MAX_CONFIG_BYTES)
    expected = os.environ.get(EXPECTED_CONFIG_SHA256_ENV)
    digest = sha256_bytes(raw)
    if not valid_sha256(expected) or expected != digest:
        raise RuntimeContractError("fingerprint config digest mismatch")
    return config, digest


def stable_probe_digest(page: Any) -> str:
    probe = page.evaluate(FINGERPRINT_PROBE)
    return sha256_bytes(canonical_json(probe))


def extract_camoufox_config(options: dict[str, Any]) -> dict[str, Any]:
    env = options.get("env")
    if not isinstance(env, dict):
        raise RuntimeContractError("Camoufox did not materialize config environment")
    chunks: list[tuple[int, str]] = []
    for name, value in env.items():
        if not isinstance(name, str) or not name.startswith("CAMOU_CONFIG_"):
            continue
        suffix = name.rsplit("_", 1)[-1]
        if not suffix.isdigit():
            raise RuntimeContractError("Camoufox config chunk is invalid")
        chunks.append((int(suffix), str(value)))
    if not chunks:
        raise RuntimeContractError("Camoufox did not materialize fingerprint config")
    try:
        value = json.loads("".join(chunk for _, chunk in sorted(chunks)))
    except json.JSONDecodeError as error:
        raise RuntimeContractError("materialized Camoufox config is invalid") from error
    if not isinstance(value, dict):
        raise RuntimeContractError("materialized Camoufox config shape is invalid")
    return value


def camoufox_kwargs(
    lock: dict[str, Any],
    root: Path,
    config: dict[str, Any],
) -> dict[str, Any]:
    browser = lock.get("browser")
    if not isinstance(browser, dict) or not isinstance(browser.get("version"), str):
        raise RuntimeContractError("browser lock is invalid")
    user_data_dir = root / USER_DATA_NAME
    if user_data_dir.is_symlink():
        raise RuntimeContractError("browser user-data directory may not be a symlink")
    user_data_dir.mkdir(exist_ok=True)
    return {
        "browser": browser["version"],
        "config": copy.deepcopy(config),
        "enable_cache": True,
        "env": browser_environment(),
        "headless": resolve_headless_mode(),
        "i_know_what_im_doing": True,
        "persistent_context": True,
        "proxy": resolve_proxy(),
        "user_data_dir": str(user_data_dir),
    }


def launch_verified_context(
    lock: dict[str, Any],
    root: Path,
    config: dict[str, Any],
    expected_probe_sha256: str,
) -> tuple[Any, Any]:
    from camoufox.sync_api import Camoufox

    if not valid_sha256(expected_probe_sha256):
        raise RuntimeContractError("profile-stable probe digest is invalid")
    manager = Camoufox(**camoufox_kwargs(lock, root, config))
    try:
        with contextlib.redirect_stdout(sys.stderr):
            context = manager.__enter__()
        page = context.pages[0] if context.pages else context.new_page()
        observed = stable_probe_digest(page)
        if observed != expected_probe_sha256:
            raise RuntimeContractError("profile-stable fingerprint drift detected")
        initial_url = resolve_initial_url()
        if initial_url is not None:
            page.goto(initial_url, wait_until="domcontentloaded", timeout=90_000)
        return manager, context
    except BaseException:
        with contextlib.suppress(BaseException):
            with contextlib.redirect_stdout(sys.stderr):
                manager.__exit__(*sys.exc_info())
        raise


def close_context(manager: Any, _context: Any, _root: Path) -> None:
    # Camoufox.__exit__ synchronously closes the persistent BrowserContext and tears down
    # Playwright. Firefox may retain user_data/lock or .parentlock path markers after that
    # clean close; marker presence alone is not evidence that an OS-level writer lock remains.
    # We therefore never delete those markers and never use their mere existence to relabel a
    # proven clean close as a crash/recovery event.
    with contextlib.redirect_stdout(sys.stderr):
        manager.__exit__(None, None, None)


def materialize_candidate_identity(root: Path) -> dict[str, str]:
    """Create exact generation identity once under an already acquired Bridge writer lock."""
    from camoufox.sync_api import Camoufox
    from camoufox.utils import launch_options

    lock, runtime_lock_sha256 = load_runtime_lock()
    verify_python_components(lock)
    if root.is_symlink() or not root.is_dir():
        raise RuntimeContractError("candidate generation root is invalid")
    root = root.resolve(strict=True)
    require_bridge_writer_lock(root)
    config_path = root / CONFIG_NAME
    user_data_dir = root / USER_DATA_NAME
    if config_path.exists() or config_path.is_symlink() or user_data_dir.is_symlink():
        raise RuntimeContractError("candidate identity already exists or path is unsafe")
    user_data_dir.mkdir(exist_ok=True)

    browser = lock.get("browser")
    if not isinstance(browser, dict) or not isinstance(browser.get("version"), str):
        raise RuntimeContractError("browser lock is invalid")
    with contextlib.redirect_stdout(sys.stderr):
        options = launch_options(
            browser=browser["version"],
            enable_cache=True,
            env=browser_environment(),
            headless=resolve_headless_mode(),
            os="windows",
        )
    config = extract_camoufox_config(options)
    config_bytes = canonical_json(config)
    config_path.write_bytes(config_bytes)
    config_sha256 = sha256_bytes(config_bytes)

    manager = Camoufox(**camoufox_kwargs(lock, root, config))
    with contextlib.redirect_stdout(sys.stderr):
        context = manager.__enter__()
    try:
        page = context.pages[0] if context.pages else context.new_page()
        probe_sha256 = stable_probe_digest(page)
    finally:
        close_context(manager, context, root)

    return {
        "fingerprint_config_sha256": config_sha256,
        "fingerprint_policy_version": str(lock["fingerprint_policy_version"]),
        "profile_stable_probe_sha256": probe_sha256,
        "runtime_lock_sha256": runtime_lock_sha256,
    }


def valid_frame(raw: str) -> bool:
    return (
        0 < len(raw) <= MAX_FRAME_LENGTH
        and "\r" not in raw
        and "\0" not in raw
        and raw.endswith("\n")
    )


def run_ipc() -> int:
    try:
        lock, _ = load_runtime_lock()
        verify_python_components(lock)
        root = resolve_profile_root(require_bridge_lock=True)
        config, _ = load_generation_config(root)
        expected_probe = os.environ.get(EXPECTED_PROBE_SHA256_ENV)
        if not valid_sha256(expected_probe):
            raise RuntimeContractError("expected profile-stable probe digest is missing")
    except (OSError, RuntimeContractError, importlib.metadata.PackageNotFoundError):
        emit("error|identity")
        return 4

    negotiated = False
    active_session: str | None = None
    manager: Any | None = None
    context: Any | None = None

    for raw in sys.stdin:
        if not valid_frame(raw):
            emit("error|protocol")
            return 2
        parts = raw[:-1].split("|")

        if parts == ["hello", IPC_VERSION] and not negotiated and active_session is None:
            negotiated = True
            emit(f"hello_ack|{IPC_VERSION}")
            continue

        if (
            len(parts) == 2
            and parts[0] == "launch"
            and negotiated
            and active_session is None
            and SESSION_PATTERN.fullmatch(parts[1]) is not None
        ):
            try:
                manager, context = launch_verified_context(lock, root, config, expected_probe)
            except BaseException:
                emit("error|runtime")
                return 5
            active_session = parts[1]
            emit(f"ready|{active_session}")
            continue

        if (
            len(parts) == 2
            and parts[0] == "close"
            and active_session is not None
            and parts[1] == active_session
            and manager is not None
            and context is not None
        ):
            try:
                close_context(manager, context, root)
            except BaseException:
                emit(f"closed|{active_session}|false")
                return 6
            emit(f"closed|{active_session}|true")
            return 0

        emit("error|protocol")
        return 2

    if manager is not None and context is not None:
        with contextlib.suppress(BaseException):
            close_context(manager, context, root)
    return 3 if negotiated or active_session is not None else 0


def main() -> int:
    if len(sys.argv) == 3 and sys.argv[1] == "--materialize-identity":
        try:
            report = materialize_candidate_identity(Path(sys.argv[2]))
        except (OSError, RuntimeContractError, importlib.metadata.PackageNotFoundError):
            print("candidate identity materialization failed", file=sys.stderr)
            return 7
        print(json.dumps(report, ensure_ascii=True, separators=(",", ":"), sort_keys=True))
        return 0
    if len(sys.argv) != 1:
        print("unsupported Camouhost invocation", file=sys.stderr)
        return 2
    return run_ipc()


if __name__ == "__main__":
    raise SystemExit(main())
