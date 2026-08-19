#!/usr/bin/env python3
"""Fail-closed Camouhost outer adapter for the AR-10 real Camoufox runtime.

This module owns only the bounded Camouhost process boundary. Profile lifecycle,
lease/fencing, network preflight, generation materialization and publication remain
owned by the native Profile Bridge.
"""

from __future__ import annotations

import hashlib
import importlib.metadata
import json
import os
import re
import sys
import stat
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Mapping

IPC_VERSION = "1"
MAX_FRAME_LENGTH = 512
SESSION_PATTERN = re.compile(r"[A-Za-z0-9_-]{8,96}\Z")
SHA256_PATTERN = re.compile(r"[0-9a-f]{64}\Z")
RUNTIME_MANIFEST_ENV = "CAMOUHOST_RUNTIME_MANIFEST"
GENERATION_ROOT_ENV = "CAMOUHOST_GENERATION_ROOT"
IDENTITY_MANIFEST_FILE = "browser-identity.json"
FINGERPRINT_CONFIG_FILE = "camoufox-fingerprint.json"
USER_DATA_DIR = "user_data"
IDENTITY_SCHEMA = "profile-platform-browser-identity-v1"
RUNTIME_SCHEMA = "profile-platform-camoufox-runtime-v1"


class ContractError(RuntimeError):
    """Expected fail-closed contract violation; message is intentionally non-sensitive."""


@dataclass(frozen=True)
class PreparedRuntime:
    runtime_manifest: Mapping[str, Any]
    generation_root: Path
    user_data_dir: Path
    fingerprint_config: Mapping[str, Any]
    fingerprint_sha256: str


def _regular_file(path: Path) -> None:
    metadata = path.lstat()
    if path.is_symlink() or not stat.S_ISREG(metadata.st_mode):
        raise ContractError("required file is not regular")


def _regular_dir(path: Path) -> None:
    metadata = path.lstat()
    if path.is_symlink() or not stat.S_ISDIR(metadata.st_mode):
        raise ContractError("required directory is invalid")


def _load_json(path: Path, *, max_bytes: int = 1_048_576) -> Any:
    _regular_file(path)
    raw = path.read_bytes()
    if not raw or len(raw) > max_bytes:
        raise ContractError("json payload size is invalid")
    try:
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError("json payload is invalid") from error


def canonical_json_bytes(value: Any) -> bytes:
    try:
        return json.dumps(
            value,
            ensure_ascii=True,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise ContractError("fingerprint config is not canonicalizable") from error


def canonical_sha256(value: Any) -> str:
    return hashlib.sha256(canonical_json_bytes(value)).hexdigest()


def _hex_digest(value: Any) -> str:
    if not isinstance(value, str) or SHA256_PATTERN.fullmatch(value) is None:
        raise ContractError("sha256 field is invalid")
    return value


def _runtime_manifest(path: Path) -> Mapping[str, Any]:
    value = _load_json(path, max_bytes=32_768)
    if not isinstance(value, dict) or value.get("schema") != RUNTIME_SCHEMA:
        raise ContractError("runtime manifest schema is invalid")
    if value.get("candidate") is not True:
        raise ContractError("runtime manifest must be a candidate during AR-10")
    if value.get("launch_eligible") is not True:
        raise ContractError("runtime candidate is not launch eligible")
    if value.get("ipc_version") != IPC_VERSION:
        raise ContractError("runtime ipc version mismatch")
    if value.get("runtime_policy") != "camoufox-runtime-v1":
        raise ContractError("runtime policy mismatch")
    if value.get("fingerprint_policy_version") != 1:
        raise ContractError("fingerprint policy mismatch")
    if value.get("platform") != "windows-x86_64":
        raise ContractError("runtime platform mismatch")

    packages = value.get("packages")
    browser = value.get("browser")
    python = value.get("python")
    if not isinstance(packages, dict) or not isinstance(browser, dict) or not isinstance(python, dict):
        raise ContractError("runtime component inventory is invalid")
    expected = {
        "camoufox": "0.5.4",
        "browserforge": "1.2.4",
        "playwright": "1.55.0",
    }
    for distribution, version in expected.items():
        item = packages.get(distribution)
        if not isinstance(item, dict) or item.get("version") != version:
            raise ContractError("runtime component pin mismatch")
    _hex_digest(packages["camoufox"].get("wheel_sha256"))
    if python.get("version") != "3.12.10":
        raise ContractError("python runtime pin mismatch")
    if browser.get("repository") != "daijro/camoufox" or browser.get("channel") != "official/stable":
        raise ContractError("browser source pin mismatch")
    if browser.get("version") != "152.0.4-beta.28":
        raise ContractError("browser version pin mismatch")
    _hex_digest(browser.get("artifact_sha256"))
    return value


def _identity(generation_root: Path) -> tuple[Mapping[str, Any], str]:
    identity = _load_json(generation_root / IDENTITY_MANIFEST_FILE, max_bytes=32_768)
    if not isinstance(identity, dict) or identity.get("schema") != IDENTITY_SCHEMA:
        raise ContractError("browser identity schema is invalid")
    if identity.get("fingerprint_policy_version") != 1:
        raise ContractError("browser identity policy mismatch")
    config_digest = _hex_digest(identity.get("fingerprint_config_sha256"))
    expected_size = identity.get("fingerprint_config_size")
    expected_keys = identity.get("fingerprint_config_keys")
    if not isinstance(expected_size, int) or expected_size <= 2 or expected_size > 1_048_576:
        raise ContractError("browser identity config size is invalid")
    if not isinstance(expected_keys, int) or expected_keys < 8 or expected_keys > 4096:
        raise ContractError("browser identity config key count is invalid")

    config_path = generation_root / FINGERPRINT_CONFIG_FILE
    config = _load_json(config_path)
    if not isinstance(config, dict) or len(config) != expected_keys:
        raise ContractError("fingerprint config shape mismatch")
    canonical = canonical_json_bytes(config)
    if len(canonical) != expected_size:
        raise ContractError("fingerprint config size mismatch")
    if hashlib.sha256(canonical).hexdigest() != config_digest:
        raise ContractError("fingerprint config digest mismatch")
    return config, config_digest


def _verify_installed_packages(manifest: Mapping[str, Any], version_getter: Callable[[str], str]) -> None:
    packages = manifest["packages"]
    for distribution in ("camoufox", "browserforge", "playwright"):
        try:
            actual = version_getter(distribution)
        except importlib.metadata.PackageNotFoundError as error:
            raise ContractError("runtime component is missing") from error
        if actual != packages[distribution]["version"]:
            raise ContractError("runtime component version mismatch")


def _verify_browser(manifest: Mapping[str, Any]) -> None:
    try:
        from camoufox.multiversion import list_installed
    except Exception as error:
        raise ContractError("camoufox browser inventory is unavailable") from error

    active = [entry for entry in list_installed() if getattr(entry, "is_active", False)]
    if len(active) != 1:
        raise ContractError("exactly one active Camoufox browser is required")
    entry = active[0]
    expected = manifest["browser"]
    full_string = getattr(getattr(entry, "version", None), "full_string", None)
    if getattr(entry, "repo_name", None) != "official" or full_string != expected["version"]:
        raise ContractError("active browser version mismatch")
    if getattr(entry, "is_prerelease", True):
        raise ContractError("active browser channel mismatch")
    if getattr(entry, "sha256", None) != expected["artifact_sha256"]:
        raise ContractError("active browser artifact digest mismatch")


def prepare_runtime(
    environ: Mapping[str, str] | None = None,
    *,
    version_getter: Callable[[str], str] = importlib.metadata.version,
    python_version_getter: Callable[[], str] = lambda: ".".join(map(str, sys.version_info[:3])),
    verify_browser: Callable[[Mapping[str, Any]], None] = _verify_browser,
) -> PreparedRuntime:
    env = os.environ if environ is None else environ
    manifest_raw = env.get(RUNTIME_MANIFEST_ENV)
    generation_raw = env.get(GENERATION_ROOT_ENV)
    if not manifest_raw or not generation_raw:
        raise ContractError("required runtime environment is missing")
    manifest_path = Path(manifest_raw)
    generation_root = Path(generation_raw)
    _regular_dir(generation_root)
    generation_root = generation_root.resolve(strict=True)
    manifest = _runtime_manifest(manifest_path)
    if python_version_getter() != manifest["python"]["version"]:
        raise ContractError("python runtime version mismatch")
    config, digest = _identity(generation_root)
    user_data_dir = generation_root / USER_DATA_DIR
    _regular_dir(user_data_dir)
    _verify_installed_packages(manifest, version_getter)
    verify_browser(manifest)
    return PreparedRuntime(manifest, generation_root, user_data_dir, config, digest)


def launch_camoufox(prepared: PreparedRuntime):
    try:
        from camoufox.sync_api import Camoufox
    except Exception as error:
        raise ContractError("camoufox runtime import failed") from error
    try:
        manager = Camoufox(
            config=dict(prepared.fingerprint_config),
            i_know_what_im_doing=True,
            persistent_context=True,
            user_data_dir=str(prepared.user_data_dir),
            headless=False,
            enable_cache=True,
        )
        context = manager.__enter__()
    except Exception as error:
        raise ContractError("camoufox launch failed") from error
    return manager, context


def emit(frame: str) -> None:
    sys.stdout.write(frame + "\n")
    sys.stdout.flush()


def fail(kind: str) -> int:
    emit(f"error|{kind}")
    return 2


def valid_frame(raw: str) -> bool:
    return 0 < len(raw) <= MAX_FRAME_LENGTH and "\r" not in raw and "\0" not in raw and raw.endswith("\n")


def run() -> int:
    try:
        prepared = prepare_runtime()
    except (OSError, ContractError):
        return fail("preflight")

    negotiated = False
    active_session: str | None = None
    manager = None
    for raw in sys.stdin:
        if not valid_frame(raw):
            return fail("protocol")
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
                manager, _context = launch_camoufox(prepared)
            except ContractError:
                return fail("launch")
            active_session = parts[1]
            emit(f"ready|{active_session}")
            continue
        if len(parts) == 2 and parts[0] == "close" and active_session == parts[1] and manager is not None:
            try:
                manager.__exit__(None, None, None)
            except Exception:
                return fail("close")
            emit(f"closed|{active_session}|true")
            return 0
        return fail("protocol")
    return 3 if negotiated or active_session is not None else 0


if __name__ == "__main__":
    raise SystemExit(run())
