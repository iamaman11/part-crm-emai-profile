#!/usr/bin/env python3
"""Stable-origin browser-state fixture for S0 EC-S0-11.

This process never launches Camouhost or Camoufox and owns no save/restore semantics.
It serves one loopback origin for the existing managed Profile Bridge runtime. The page
seeds bounded non-secret markers on its first visit and verifies their exact values on
the authoritative reopened visit. Stdout contains only bounded booleans/visit metadata.
"""

from __future__ import annotations

import argparse
import json
import secrets
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_REPORT_BYTES = 1024


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    return parser.parse_args()


def page(visit: int, nonce: str) -> bytes:
    source = f"""<!doctype html><meta charset='utf-8'><script>
const VISIT = {visit};
const PROOF_NONCE = '{nonce}';
const COOKIE_NAME = 's0_portable_cookie';
const COOKIE_VALUE = 'v1';
const STORAGE_KEY = 's0_portable_local_storage';
const DB_NAME = 's0-portable-indexeddb-v1';
const STORE_NAME = 'markers';
const MARKER_KEY = 'accepted';
const MARKER_VALUE = 'profile-generation-portable-v1';

function observedCookieValue() {{
  const prefix = COOKIE_NAME + '=';
  const cookie = document.cookie.split(';').map(value => value.trim())
    .find(value => value.startsWith(prefix));
  return cookie ? cookie.slice(prefix.length) : null;
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
    request.onsuccess = () => resolve(request.result ?? null);
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
async function snapshot(db) {{
  const cookieValue = observedCookieValue();
  const localValue = localStorage.getItem(STORAGE_KEY);
  const indexedValue = await readDb(db);
  // The exact same native browser values must survive encrypted Save, a
  // missing-local cold R2 restore, and the independently cold N-1 open.
  // Never send raw cookie/LS/IDB values outside this disposable test origin.
  // Per-run nonce prevents matching a digest reused by an earlier test run.
  const encoded = new TextEncoder().encode(
    JSON.stringify([PROOF_NONCE, location.origin, cookieValue, localValue, indexedValue])
  );
  const sha = await crypto.subtle.digest('SHA-256', encoded);
  const stateDigest = Array.from(new Uint8Array(sha))
    .map(value => value.toString(16).padStart(2, '0')).join('');
  return {{
    cookie: cookieValue === COOKIE_VALUE,
    local_storage: localValue === MARKER_VALUE,
    indexed_db: indexedValue === MARKER_VALUE,
    state_digest: stateDigest,
  }};
}}
async function report(phase, state) {{
  await fetch('/observed', {{
    method: 'POST',
    headers: {{'Content-Type': 'application/json'}},
    body: JSON.stringify({{
      phase,
      visit: VISIT,
      cookie: state.cookie === true,
      local_storage: state.local_storage === true,
      indexed_db: state.indexed_db === true,
      state_digest: state.state_digest,
    }}),
  }});
}}
(async () => {{
  const db = await openDb();
  const before = await snapshot(db);
  let phase = 'error';
  let observed = before;
  if (!before.cookie && !before.local_storage && !before.indexed_db) {{
    document.cookie = COOKIE_NAME + '=' + COOKIE_VALUE
      + '; Path=/; Max-Age=31536000; SameSite=Lax';
    localStorage.setItem(STORAGE_KEY, MARKER_VALUE);
    await writeDb(db);
    observed = await snapshot(db);
    phase = observed.cookie && observed.local_storage && observed.indexed_db ? 'seed' : 'error';
  }} else if (before.cookie && before.local_storage && before.indexed_db) {{
    phase = 'verify';
  }}
  await report(phase, observed);
  db.close();
}})().catch(async () => {{
  try {{
    await report('error', {{cookie: false, local_storage: false, indexed_db: false, state_digest: '0'.repeat(64)}});
  }} catch (_) {{}}
}});
</script><title>S0 portable browser state</title>"""
    return source.encode("utf-8")


class StateServer(ThreadingHTTPServer):
    allow_reuse_address = False

    def __init__(self, port: int) -> None:
        self._visit_lock = threading.Lock()
        self._visits = 0
        self.proof_nonce = secrets.token_hex(16)
        super().__init__(("127.0.0.1", port), Handler)

    def next_visit(self) -> int:
        with self._visit_lock:
            self._visits += 1
            return self._visits


class Handler(BaseHTTPRequestHandler):
    server: StateServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if self.path != "/":
            self.send_response(404)
            self.end_headers()
            return
        payload = page(self.server.next_visit(), self.server.proof_nonce)
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
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
        try:
            value = json.loads(self.rfile.read(length).decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            self.send_response(400)
            self.end_headers()
            return
        if (
            not isinstance(value, dict)
            or set(value) != {
                "phase",
                "visit",
                "cookie",
                "local_storage",
                "indexed_db",
                "state_digest",
            }
            or value["phase"] not in {"seed", "verify", "error"}
            or isinstance(value["visit"], bool)
            or not isinstance(value["visit"], int)
            or value["visit"] <= 0
            or any(
                not isinstance(value[name], bool)
                for name in ("cookie", "local_storage", "indexed_db")
            )
            or not isinstance(value["state_digest"], str)
            or len(value["state_digest"]) != 64
            or any(char not in "0123456789abcdef" for char in value["state_digest"])
        ):
            self.send_response(400)
            self.end_headers()
            return
        print(
            json.dumps(value, sort_keys=True, separators=(",", ":")),
            flush=True,
        )
        self.send_response(204)
        self.end_headers()


def main() -> int:
    args = parse_args()
    if not 0 <= args.port <= 65535:
        raise SystemExit("portable browser-state port is out of range")
    server = StateServer(args.port)
    actual_port = server.server_address[1]
    print(
        json.dumps(
            {"origin_port": actual_port, "ready": True},
            sort_keys=True,
            separators=(",", ":"),
        ),
        flush=True,
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
