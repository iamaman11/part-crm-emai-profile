#!/usr/bin/env python3
"""Repository-owned Windows certification probe for the AR-10 Camoufox candidate."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import re
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

SHA256 = re.compile(r"[0-9a-f]{64}\Z")
EXPECTED_SCHEMA = "profile-platform-camoufox-runtime-v1"


class ProbeError(RuntimeError):
    pass


def load_manifest(path: Path) -> dict:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict) or value.get("schema") != EXPECTED_SCHEMA:
        raise ProbeError("candidate manifest schema mismatch")
    return value


def package_inventory(manifest: dict) -> dict[str, str]:
    actual = {
        name: importlib.metadata.version(name)
        for name in ("camoufox", "browserforge", "playwright")
    }
    expected = {name: manifest["packages"][name]["version"] for name in actual}
    if actual != expected:
        raise ProbeError("installed Python package inventory mismatch")
    return actual


def browser_inventory(manifest: dict) -> dict:
    from camoufox.multiversion import list_installed

    active = [entry for entry in list_installed() if entry.is_active]
    if len(active) != 1:
        raise ProbeError("expected exactly one active browser")
    entry = active[0]
    full_string = entry.version.full_string
    digest = entry.sha256
    if entry.repo_name != "official" or full_string != manifest["browser"]["version"]:
        raise ProbeError("active browser does not match candidate")
    if entry.is_prerelease:
        raise ProbeError("active browser is not from the stable release channel")
    if not isinstance(digest, str) or SHA256.fullmatch(digest) is None:
        raise ProbeError("active browser has no complete SHA-256 inventory digest")
    return {
        "repository": "daijro/camoufox",
        "channel": "official/stable",
        "version": full_string,
        "artifact_sha256": digest,
        "asset_id": entry.asset_id,
        "asset_size": entry.asset_size,
        "asset_updated_at": entry.asset_updated_at,
    }


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"<!doctype html><meta charset=utf-8><title>ar10</title>"
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        return


def persistence_smoke() -> dict[str, object]:
    from camoufox.sync_api import Camoufox

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    origin = f"http://127.0.0.1:{server.server_port}/"
    marker = "ar10-persistent-state-v1"
    try:
        with tempfile.TemporaryDirectory(prefix="ar10-camoufox-") as profile:
            with Camoufox(
                headless=True,
                persistent_context=True,
                user_data_dir=profile,
                enable_cache=True,
            ) as context:
                page = context.new_page()
                page.goto(origin, wait_until="domcontentloaded")
                page.evaluate("value => localStorage.setItem('ar10-marker', value)", marker)
                first_platform = page.evaluate("navigator.platform")
                first_ua = page.evaluate("navigator.userAgent")

            with Camoufox(
                headless=True,
                persistent_context=True,
                user_data_dir=profile,
                enable_cache=True,
            ) as context:
                page = context.new_page()
                page.goto(origin, wait_until="domcontentloaded")
                recovered = page.evaluate("localStorage.getItem('ar10-marker')")
                second_platform = page.evaluate("navigator.platform")
                second_ua = page.evaluate("navigator.userAgent")

            if recovered != marker:
                raise ProbeError("persistent localStorage marker did not survive relaunch")
            return {
                "local_storage_survived": True,
                "platform_stable": first_platform == second_platform,
                "user_agent_stable": first_ua == second_ua,
            }
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("runtime/camouhost/runtime-candidate.json"),
    )
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--skip-smoke", action="store_true")
    args = parser.parse_args()

    manifest = load_manifest(args.manifest)
    expected_python = manifest["python"]["version"]
    actual_python = ".".join(map(str, sys.version_info[:3]))
    if actual_python != expected_python:
        raise ProbeError(
            f"python version mismatch: expected {expected_python}, got {actual_python}"
        )

    evidence = {
        "schema": "profile-platform-ar10-candidate-evidence-v1",
        "candidate": True,
        "python": actual_python,
        "packages": package_inventory(manifest),
        "browser": browser_inventory(manifest),
    }
    if not args.skip_smoke:
        evidence["persistent_context_smoke"] = persistence_smoke()
    encoded = json.dumps(evidence, sort_keys=True, separators=(",", ":"))
    print(encoded)
    if args.evidence is not None:
        args.evidence.write_text(encoded + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
