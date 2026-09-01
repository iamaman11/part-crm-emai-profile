#!/usr/bin/env python3
"""Real browser-state effect probe for S0 EC-S0-11.

This helper does not own save/restore semantics. It only seeds or verifies browser API
state inside a generation workspace while the existing Profile Bridge writer lock is
held. The Rust P3 authoritative-generation test owns snapshot/encrypt/commit/local-loss/
download/decrypt/reopen and invokes this probe on the two sides of that lifecycle.
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

IPC_VERSION = "3"
SESSION = "session_01JAS0PORTABLESTATE"
MAX_REPORT_BYTES = 4096


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--camouhost", type=Path, required=True)
    parser.add_argument("--runtime-lock", type=Path, required=True)
    parser.add_argument("--profile-root", type=Path, required=True)
    parser.add_argument("--config-sha256", required=True)
    parser.add_argument("--probe-sha256", required=True)
    parser.add_argument("--runtime-lock-sha256", required=True)
    parser.add_argument("--headless", choices=("false", "virtual"), required=True)
    parser.add_argument("--mode", choices=("seed", "verify"), required=True)
    parser.add_argument("--port", type=int, required=True)
    return parser.parse_args()


def require_regular(path: Path, label: str) -> Path:
    resolved = path.resolve(strict=True)
    if path.is_symlink() or not resolved.is_file() or resolved.stat().st_size == 0:
        raise AssertionError(f"{label} is not a non-empty regular file")
    return resolved


def require_digest(value: str, label: str) -> str:
    if len(value) != 64 or any(ch not in "0123456789abcdef" for ch in value):
        raise AssertionError(f"{label} is not canonical lower-hex SHA-256")
    return value


def page(mode: str) -> bytes:
    encoded_mode = json.dumps(mode)
    source = f"""<!doctype html><meta charset='utf-8'><script>
const MODE = {encoded_mode};
const COOKIE_NAME = 's0_portable_cookie';
const STORAGE_KEY = 's0_portable_local_storage';
const DB_NAME = 's0-portable-indexeddb-v1';
const STORE_NAME = 'markers';
const MARKER_KEY = 'accepted';
const MARKER_VALUE = 'profile-generation-portable';

function cookiePresent() {{
  return document.cookie.split(';').map(v => v.trim()).includes(COOKIE_NAME + '=1');
}}
function openDb() {{
  return new Promise((resolve, reject) => {{
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => {{
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) db.createObjectStore(STORE_NAME);
    }};
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error || new Error('indexedDB open failed'));
  }});
}}
function readDb(db) {{
  return new Promise((resolve, reject) => {{
    const tx = db.transaction(STORE_NAME, 'readonly');
    const request = tx.objectStore(STORE_NAME).get(MARKER_KEY);
    request.onsuccess = () => resolve(request.result === MARKER_VALUE);
    request.onerror = () => reject(request.error || new Error('indexedDB read failed'));
  }});
}}
function writeDb(db) {{
  return new Promise((resolve, reject) => {{
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).put(MARKER_VALUE, MARKER_KEY);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error || new Error('indexedDB write failed'));
    tx.onabort = () => reject(tx.error || new Error('indexedDB write aborted'));
  }});
}}
async function post(value) {{
  const response = await fetch('/observed', {{
    method: 'POST',
    headers: {{'Content-Type': 'application/json'}},
    body: JSON.stringify(value),
  }});
  if (!response.ok) throw new Error('observation rejected: ' + response.status);
}}
(async () => {{
  const db = await openDb();
  const before = {{
    cookie: cookiePresent(),
    local_storage: localStorage.getItem(STORAGE_KEY) === MARKER_VALUE,
    indexed_db: await readDb(db),
  }};
  if (MODE === 'seed') {{
    if (before.cookie || before.local_storage || before.indexed_db) {{
      throw new Error('new generation already contained portable-state marker');
    }}
    document.cookie = COOKIE_NAME + '=1; Path=/; SameSite=Lax';
    localStorage.setItem(STORAGE_KEY, MARKER_VALUE);
    await writeDb(db);
    const after = {{
      cookie: cookiePresent(),
      local_storage: localStorage.getItem(STORAGE_KEY) === MARKER_VALUE,
      indexed_db: await readDb(db),
    }};
    await post(after);
  }} else {{
    await post(before);
  }}
  db.close();
}})().catch(async error => {{
  try {{ await post({{error: String(error)}}); }} catch (_) {{}}
}});
</script><title>S0 portable browser state</title>"""
    return source.encode("utf-8")


class StateServer(ThreadingHTTPServer):
    allow_reuse_address = True

    def __init__(self, mode: str, port: int) -> None:
        self.page = page(mode)
        self.observations: queue.Queue[dict[str, object]] = queue.Queue()
        super().__init__(("127.0.0.1", port), Handler)


class Handler(BaseHTTPRequestHandler):
    server: StateServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if self.path != "/":
            self.send_response(404)
            self.end_headers()
            return
        payload = self.server.page
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if self.path != "/observed":
            self.send_response(404)
            self.end_headers()
            return
        try:
            length = int(self.headers.get("Content-Length") or "")
        except ValueError:
            length = -1
        if length <= 0 or length > MAX_REPORT_BYTES:
            self.send_response(413)
            self.end_headers()
            return
        payload = self.rfile.read(length)
        try:
            value = json.loads(payload.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            self.send_response(400)
            self.end_headers()
            return
        if not isinstance(value, dict):
            self.send_response(400)
            self.end_headers()
            return
        self.server.observations.put(value)
        self.send_response(204)
        self.end_headers()


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


def complete_pre_navigation_protocol(process: subprocess.Popen[str], url: str) -> None:
    response = exchange(process, f"observe_browser_visible|{SESSION}")
    prefix = f"browser_visible|{SESSION}|"
    if not response.startswith(prefix):
        raise AssertionError(f"unexpected browser-visible frame: {response[:160]!r}")
    try:
        payload = bytes.fromhex(response[len(prefix) :])
        observation = json.loads(payload.decode("utf-8"))
    except (ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AssertionError("browser-visible wire payload is invalid") from error
    if not isinstance(observation, dict) or not observation:
        raise AssertionError("browser-visible wire payload has invalid top-level shape")
    target = url.encode("utf-8").hex()
    admission = exchange(process, f"admit_navigation|{SESSION}|{target}")
    if admission != f"navigation_admitted|{SESSION}":
        raise AssertionError(f"navigation admission failed: {admission}")


def run(args: argparse.Namespace) -> dict[str, object]:
    python = require_regular(args.python, "runtime Python")
    camouhost = require_regular(args.camouhost, "Camouhost entrypoint")
    runtime_lock = require_regular(args.runtime_lock, "runtime lock")
    profile_root = args.profile_root.resolve(strict=True)
    if args.profile_root.is_symlink() or not profile_root.is_dir():
        raise AssertionError("profile root is not a regular directory")
    if not 1024 <= args.port <= 65535:
        raise AssertionError("portable browser-state origin port is out of range")

    config_sha256 = require_digest(args.config_sha256, "config digest")
    probe_sha256 = require_digest(args.probe_sha256, "probe digest")
    runtime_lock_sha256 = require_digest(args.runtime_lock_sha256, "runtime-lock digest")

    server = StateServer(args.mode, args.port)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{args.port}/"
    env = os.environ.copy()
    env.update(
        {
            "CAMOUHOST_PROFILE_ROOT": str(profile_root),
            "CAMOUHOST_RUNTIME_LOCK": str(runtime_lock),
            "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256": runtime_lock_sha256,
            "CAMOUHOST_EXPECTED_CONFIG_SHA256": config_sha256,
            "CAMOUHOST_EXPECTED_PROBE_SHA256": probe_sha256,
            "CAMOUHOST_HEADLESS_MODE": args.headless,
            "PYTHONUNBUFFERED": "1",
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
        complete_pre_navigation_protocol(process, url)
        observed = server.observations.get(timeout=30)
        if "error" in observed:
            raise AssertionError(f"browser-state page failed: {observed['error']}")
        expected = {"cookie": True, "local_storage": True, "indexed_db": True}
        if observed != expected:
            raise AssertionError(
                f"portable browser state mismatch for {args.mode}: {observed!r} != {expected!r}"
            )
        close = exchange(process, f"close|{SESSION}")
        if close != f"closed|{SESSION}|true":
            raise AssertionError(f"Camouhost clean close failed: {close}")
        if process.wait(timeout=60) != 0:
            stderr = process.stderr.read()[-3000:] if process.stderr is not None else ""
            raise AssertionError(f"Camouhost exited non-zero: {stderr}")
        return {"mode": args.mode, "origin_port": args.port, **expected}
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def main() -> int:
    report = run(parse_args())
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
