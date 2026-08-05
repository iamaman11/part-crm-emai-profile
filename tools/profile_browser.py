#!/usr/bin/env python3
"""Launch one isolated persistent Camoufox profile with a fixed fingerprint."""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.metadata
import json
import os
import time
from pathlib import Path
from typing import Any

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
  timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
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
  canvas: (() => {
    const canvas = document.createElement('canvas');
    canvas.width = 220;
    canvas.height = 40;
    const context = canvas.getContext('2d');
    context.font = '16px sans-serif';
    context.fillText('profile-stability-probe', 4, 24);
    return canvas.toDataURL();
  })(),
})
"""


def atomic_write_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(f"{path.suffix}.tmp")
    temporary.write_text(
        json.dumps(data, indent=2, sort_keys=True),
        encoding="utf-8",
    )
    os.replace(temporary, path)


def browser_environment() -> dict[str, str]:
    allowed = {
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "GDK_BACKEND",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOGNAME",
        "PATH",
        "SHELL",
        "USER",
        "WAYLAND_DISPLAY",
        "WSL_DISTRO_NAME",
        "WSL_INTEROP",
        "XDG_RUNTIME_DIR",
    }
    return {key: value for key, value in os.environ.items() if key in allowed}


def browser_process_uses_profile(user_data_dir: Path) -> bool:
    expected = str(user_data_dir.resolve()).encode()
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            command = (entry / "cmdline").read_bytes()
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue
        if expected in command and (b"camoufox" in command or b"-profile" in command):
            return True
    return False


def extract_camoufox_config(options: dict[str, Any]) -> dict[str, Any]:
    chunks = [
        (int(name.rsplit("_", 1)[1]), str(value))
        for name, value in options.get("env", {}).items()
        if name.startswith("CAMOU_CONFIG_")
    ]
    if not chunks:
        raise RuntimeError("Camoufox did not materialize CAMOU_CONFIG")
    return json.loads("".join(value for _, value in sorted(chunks)))


def load_or_create_profile(workspace: Path, profile_id: str) -> dict[str, Any]:
    from camoufox.pkgman import installed_verstr
    from camoufox.utils import launch_options

    metadata_path = workspace / "profile.json"
    if metadata_path.exists():
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        if metadata.get("profile_id") != profile_id:
            raise ValueError("profile ID does not match workspace metadata")
        return metadata

    options = launch_options(
        os="windows",
        locale="ru-RU",
        geoip=True,
        headless=False,
        enable_cache=True,
        env=browser_environment(),
    )
    config = extract_camoufox_config(options)
    config_bytes = json.dumps(config, sort_keys=True, separators=(",", ":")).encode()
    metadata = {
        "schema_version": 1,
        "profile_id": profile_id,
        "fingerprint_policy": "browserforge-fixed-v1",
        "fingerprint_config_sha256": hashlib.sha256(config_bytes).hexdigest(),
        "fingerprint_config": config,
        "runtime": {
            "camoufox": importlib.metadata.version("camoufox"),
            "browserforge": importlib.metadata.version("browserforge"),
            "playwright": importlib.metadata.version("playwright"),
            "browser": installed_verstr(),
        },
    }
    atomic_write_json(metadata_path, metadata)
    return metadata


def run_browser(
    workspace: Path,
    profile_id: str,
    target_url: str,
    auto_close_seconds: float | None,
) -> dict[str, Any]:
    from camoufox import Camoufox
    from playwright._impl._errors import TargetClosedError

    workspace.mkdir(parents=True, exist_ok=True)
    user_data_dir = workspace / "user_data"
    user_data_dir.mkdir(parents=True, exist_ok=True)
    metadata = load_or_create_profile(workspace, profile_id)

    try:
        with Camoufox(
            config=copy.deepcopy(metadata["fingerprint_config"]),
            i_know_what_im_doing=True,
            persistent_context=True,
            user_data_dir=str(user_data_dir),
            headless=False,
            enable_cache=True,
            env=browser_environment(),
        ) as context:
            page = context.pages[0] if context.pages else context.new_page()
            probe = page.evaluate(FINGERPRINT_PROBE)
            probe_digest = hashlib.sha256(
                json.dumps(probe, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            baseline = metadata.get("fingerprint_probe_sha256")
            metadata["fingerprint_probe_sha256"] = baseline or probe_digest
            atomic_write_json(workspace / "profile.json", metadata)

            try:
                page.goto(target_url, wait_until="domcontentloaded", timeout=90_000)
            except Exception as error:
                print(f"Navigation warning: {type(error).__name__}", flush=True)

            print(
                json.dumps(
                    {
                        "browser_ready": True,
                        "profile_id": profile_id,
                        "fingerprint_stable": baseline in {None, probe_digest},
                        "close_window_to_sync": True,
                    },
                    separators=(",", ":"),
                ),
                flush=True,
            )
            browser_process_seen = browser_process_uses_profile(user_data_dir)
            opened_at = time.monotonic()
            while True:
                time.sleep(1)
                try:
                    browser_process_running = browser_process_uses_profile(user_data_dir)
                    browser_process_seen = browser_process_seen or browser_process_running
                    if browser_process_seen and not browser_process_running:
                        break
                    if page.is_closed() or not context.pages:
                        break
                    if auto_close_seconds and time.monotonic() - opened_at >= auto_close_seconds:
                        context.close()
                        break
                except TargetClosedError:
                    break
    except TargetClosedError:
        pass

    return {
        "profile_id": profile_id,
        "fingerprint_probe_sha256": probe_digest,
        "fingerprint_stable": baseline in {None, probe_digest},
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--profile-id", required=True)
    parser.add_argument("--url", default="https://e.mail.ru/inbox/")
    parser.add_argument("--auto-close-seconds", type=float)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    report = run_browser(
        args.workspace.resolve(),
        args.profile_id,
        args.url,
        args.auto_close_seconds,
    )
    print(json.dumps({"browser_closed": True, **report}, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
