#!/usr/bin/env python3
"""Empirical S0 proof for browser-visible canvas stability across portable restore.

The canonical browser identity remains owned by browser-execution-domain. This test
never promotes canvas:seed into output evidence by hashing the config key. Instead it
launches the exact pinned Camoufox runtime, observes a deterministic browser-visible
2D canvas payload, and requires byte-identical output across a cold relaunch and an
exact restored generation workspace that is rebound to a fresh Bridge writer lock.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import queue
import shutil
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

MAX_CANVAS_BYTES = 1_048_576
IPC_VERSION = "2"
SESSION = "session_01JAS0CANVASPORTABLE"
BRIDGE_LOCK = "profile-platform-bridge-lock-v1\ndevice_01JAS0CANVASPORTABLE\n1\n"
LOCK_MARKERS = (".parentlock", "parent.lock", "lock")

PAGE = b"""<!doctype html><meta charset='utf-8'><script>
const canvas = document.createElement('canvas');
canvas.width = 320;
canvas.height = 96;
const ctx = canvas.getContext('2d');
if (!ctx) throw new Error('2d canvas unavailable');
ctx.textBaseline = 'alphabetic';
ctx.font = '18px Arial';
ctx.fillStyle = '#f60';
ctx.fillRect(16, 12, 144, 48);
ctx.fillStyle = '#069';
ctx.fillText('CAP-EXEC S0 R6.5', 22, 43);
ctx.strokeStyle = 'rgba(102, 204, 0, 0.7)';
ctx.lineWidth = 3;
ctx.beginPath();
ctx.arc(222, 45, 27, 0, Math.PI * 2, true);
ctx.stroke();
ctx.fillStyle = 'rgba(255, 0, 255, 0.45)';
ctx.fillText('\u2603 portable canvas', 72, 82);
const payload = canvas.toDataURL('image/png');
const observed = new XMLHttpRequest();
observed.open('POST', '/canvas', false);
observed.setRequestHeader('Content-Type', 'text/plain;charset=UTF-8');
observed.send(payload);
if (observed.status !== 204) throw new Error('canvas observation rejected');
</script><title>S0 canvas portability proof</title>"""


class CanvasServer(ThreadingHTTPServer):
    def __init__(self) -> None:
        self.observations: queue.Queue[tuple[int, str]] = queue.Queue()
        super().__init__(("127.0.0.1", 0), Handler)


class Handler(BaseHTTPRequestHandler):
    server: CanvasServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(PAGE)))
        self.end_headers()
        self.wfile.write(PAGE)

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if self.path != "/canvas":
            self.send_response(404)
            self.end_headers()
            return
        raw_length = self.headers.get("Content-Length")
        try:
            length = int(raw_length or "")
        except ValueError:
            length = -1
        if length <= 0 or length > MAX_CANVAS_BYTES:
            self.send_response(413)
            self.end_headers()
            return
        payload = self.rfile.read(length)
        if len(payload) != length or not payload.startswith(b"data:image/png;base64,"):
            self.send_response(400)
            self.end_headers()
            return
        self.server.observations.put((length, hashlib.sha256(payload).hexdigest()))
        self.send_response(204)
        self.end_headers()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--camouhost", type=Path, required=True)
    parser.add_argument("--runtime-lock", type=Path, required=True)
    parser.add_argument("--headless", choices=("false", "virtual"), required=True)
    return parser.parse_args()


def require_regular(path: Path, label: str) -> Path:
    resolved = path.resolve(strict=True)
    if path.is_symlink() or not resolved.is_file() or resolved.stat().st_size == 0:
        raise AssertionError(f"{label} is not a non-empty regular file")
    return resolved


def write_bridge_lock(root: Path) -> None:
    (root / ".profile-platform.lock").write_text(BRIDGE_LOCK, encoding="utf-8", newline="\n")


def runtime_env(runtime_lock: Path, headless: str) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "CAMOUHOST_RUNTIME_LOCK": str(runtime_lock),
            "CAMOUHOST_HEADLESS_MODE": headless,
            "PYTHONUNBUFFERED": "1",
        }
    )
    return env


def materialize(
    python: Path,
    camouhost: Path,
    runtime_lock: Path,
    headless: str,
    root: Path,
) -> dict[str, str]:
    root.mkdir()
    write_bridge_lock(root)
    completed = subprocess.run(
        [str(python), str(camouhost), "--materialize-identity", str(root)],
        env=runtime_env(runtime_lock, headless),
        text=True,
        capture_output=True,
        timeout=240,
        check=False,
    )
    if completed.returncode != 0:
        raise AssertionError(
            "candidate identity materialization failed: "
            f"rc={completed.returncode} err={completed.stderr[-3000:]!r}"
        )
    report = json.loads(completed.stdout)
    required = {
        "fingerprint_config_sha256",
        "fingerprint_policy_version",
        "profile_stable_probe_sha256",
        "runtime_lock_sha256",
    }
    if set(report) != required or any(not isinstance(value, str) or not value for value in report.values()):
        raise AssertionError(f"unexpected materialization report: {report}")
    return report


def portable_restore(source: Path, target: Path) -> None:
    if target.exists() or target.is_symlink():
        raise AssertionError("portable restore target already exists")
    shutil.copytree(
        source,
        target,
        ignore=shutil.ignore_patterns(".profile-platform.lock"),
        symlinks=True,
    )
    for marker in LOCK_MARKERS:
        path = target / "user_data" / marker
        if path.exists() or path.is_symlink():
            path.unlink()
    for path in target.rglob("*"):
        if path.is_symlink():
            raise AssertionError(f"portable generation contains symlink: {path.relative_to(target)}")
    write_bridge_lock(target)
    source_config = (source / "camoufox-config.json").read_bytes()
    target_config = (target / "camoufox-config.json").read_bytes()
    if source_config != target_config:
        raise AssertionError("portable restore changed canonical Camoufox config bytes")


def exchange(process: subprocess.Popen[str], frame: str) -> str:
    assert process.stdin is not None
    assert process.stdout is not None
    process.stdin.write(frame + "\n")
    process.stdin.flush()
    response = process.stdout.readline().rstrip("\n")
    if not response:
        stderr = process.stderr.read()[-3000:] if process.stderr is not None else ""
        raise AssertionError(f"Camouhost produced no response for {frame!r}: {stderr}")
    return response


def observe_canvas(
    python: Path,
    camouhost: Path,
    runtime_lock: Path,
    headless: str,
    root: Path,
    report: dict[str, str],
    url: str,
    server: CanvasServer,
) -> tuple[int, str]:
    env = runtime_env(runtime_lock, headless)
    env.update(
        {
            "CAMOUHOST_PROFILE_ROOT": str(root.resolve(strict=True)),
            "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256": report["runtime_lock_sha256"],
            "CAMOUHOST_EXPECTED_CONFIG_SHA256": report["fingerprint_config_sha256"],
            "CAMOUHOST_EXPECTED_PROBE_SHA256": report["profile_stable_probe_sha256"],
            "CAMOUHOST_INITIAL_URL": url,
        }
    )
    process = subprocess.Popen(
        [str(python), str(camouhost)],
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    try:
        if exchange(process, f"hello|{IPC_VERSION}") != f"hello_ack|{IPC_VERSION}":
            raise AssertionError("Camouhost IPC negotiation failed")
        launch = exchange(process, f"launch|{SESSION}")
        if launch != f"ready|{SESSION}":
            raise AssertionError(f"Camouhost launch failed: {launch}")
        evidence = server.observations.get(timeout=30)
        close = exchange(process, f"close|{SESSION}")
        if close != f"closed|{SESSION}|true":
            raise AssertionError(f"Camouhost clean close failed: {close}")
        if process.wait(timeout=60) != 0:
            stderr = process.stderr.read()[-3000:] if process.stderr is not None else ""
            raise AssertionError(f"Camouhost exited non-zero: {stderr}")
        return evidence
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)


def main() -> int:
    args = parse_args()
    python = require_regular(args.python, "runtime Python")
    camouhost = require_regular(args.camouhost, "Camouhost entrypoint")
    runtime_lock = require_regular(args.runtime_lock, "runtime lock")

    server = CanvasServer()
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{server.server_address[1]}/"
    try:
        with tempfile.TemporaryDirectory(prefix="s0-canvas-portability-") as temporary:
            base = Path(temporary)
            host_a = base / "host-a-generation"
            host_b = base / "host-b-generation"
            report = materialize(python, camouhost, runtime_lock, args.headless, host_a)
            portable_restore(host_a, host_b)

            first = observe_canvas(
                python, camouhost, runtime_lock, args.headless, host_a, report, url, server
            )
            second = observe_canvas(
                python, camouhost, runtime_lock, args.headless, host_a, report, url, server
            )
            restored = observe_canvas(
                python, camouhost, runtime_lock, args.headless, host_b, report, url, server
            )
            if first != second:
                raise AssertionError(
                    f"browser-visible canvas drifted across cold relaunch: {first} != {second}"
                )
            if first != restored:
                raise AssertionError(
                    "browser-visible canvas drifted after exact portable restore: "
                    f"{first} != {restored}"
                )
            print(
                json.dumps(
                    {
                        "canvas_payload_bytes": first[0],
                        "canvas_payload_sha256": first[1],
                        "cold_relaunch_equal": True,
                        "portable_restore_equal": True,
                    },
                    sort_keys=True,
                    separators=(",", ":"),
                )
            )
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
