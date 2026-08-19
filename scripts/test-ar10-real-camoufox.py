#!/usr/bin/env python3
"""Repository-owned real Camoufox AR-10 integration evidence.

This test is intentionally local-only: it uses a loopback origin and an exact pinned
Camoufox runtime. Specialized fingerprint sites, real proxies and physical Windows
hosts remain external evidence.
"""

from __future__ import annotations

import hashlib
import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CAMOUHOST = ROOT / "runtime/camouhost/real.py"
RUNTIME_LOCK = ROOT / "runtime/camouhost/runtime-lock.json"
SESSION = "session_01JAR10REALCAMOUFOX"

PAGE = b"""<!doctype html><meta charset='utf-8'><script>
const hadStorage = localStorage.getItem('ar10-marker') === 'persisted';
const hadCookie = document.cookie.includes('ar10-marker=persisted');
fetch('/observed?storage=' + String(hadStorage) + '&cookie=' + String(hadCookie));
localStorage.setItem('ar10-marker', 'persisted');
document.cookie = 'ar10-marker=persisted; Path=/; SameSite=Lax';
</script><title>AR-10 local persistence proof</title>"""


class ObservationServer(ThreadingHTTPServer):
    def __init__(self) -> None:
        self.observations: queue.Queue[tuple[bool, bool]] = queue.Queue()
        super().__init__(("127.0.0.1", 0), Handler)


class Handler(BaseHTTPRequestHandler):
    server: ObservationServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if self.path.startswith("/observed?"):
            query = self.path.split("?", 1)[1]
            values = dict(part.split("=", 1) for part in query.split("&"))
            self.server.observations.put(
                (values.get("storage") == "true", values.get("cookie") == "true")
            )
            self.send_response(204)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(PAGE)))
        self.end_headers()
        self.wfile.write(PAGE)


def canonical_lock_digest() -> str:
    raw = RUNTIME_LOCK.read_bytes()
    parsed = json.loads(raw.decode("utf-8"))
    canonical = (
        json.dumps(parsed, ensure_ascii=True, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode("utf-8")
    if raw != canonical:
        raise AssertionError("runtime lock is not canonical JSON")
    return hashlib.sha256(raw).hexdigest()


def base_env(root: Path, report: dict[str, str], url: str) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "CAMOUHOST_PROFILE_ROOT": str(root),
            "CAMOUHOST_RUNTIME_LOCK": str(RUNTIME_LOCK),
            "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256": report["runtime_lock_sha256"],
            "CAMOUHOST_EXPECTED_CONFIG_SHA256": report["fingerprint_config_sha256"],
            "CAMOUHOST_EXPECTED_PROBE_SHA256": report["profile_stable_probe_sha256"],
            "CAMOUHOST_HEADLESS_MODE": "virtual",
            "CAMOUHOST_INITIAL_URL": url,
            "PYTHONUNBUFFERED": "1",
        }
    )
    return env


def materialize(root: Path) -> dict[str, str]:
    root.mkdir()
    completed = subprocess.run(
        [sys.executable, str(CAMOUHOST), "--materialize-identity", str(root)],
        cwd=ROOT,
        env={**os.environ, "CAMOUHOST_RUNTIME_LOCK": str(RUNTIME_LOCK), "CAMOUHOST_HEADLESS_MODE": "virtual"},
        text=True,
        capture_output=True,
        timeout=180,
        check=False,
    )
    if completed.returncode != 0:
        raise AssertionError(f"candidate identity materialization failed: {completed.stderr[-2000:]}")
    report = json.loads(completed.stdout)
    if set(report) != {
        "fingerprint_config_sha256",
        "fingerprint_policy_version",
        "profile_stable_probe_sha256",
        "runtime_lock_sha256",
    }:
        raise AssertionError(f"unexpected materialization report: {report}")
    if report["runtime_lock_sha256"] != canonical_lock_digest():
        raise AssertionError("materialized runtime lock identity drifted")
    (root / ".profile-platform.lock").write_text(
        "profile-platform-bridge-lock-v1\ndevice_01JAR10REALCAMOUFOX\n1\n",
        encoding="utf-8",
    )
    return report


def exchange(process: subprocess.Popen[str], frame: str) -> str:
    assert process.stdin is not None
    assert process.stdout is not None
    process.stdin.write(frame + "\n")
    process.stdin.flush()
    response = process.stdout.readline().rstrip("\n")
    if not response:
        stderr = ""
        if process.stderr is not None:
            stderr = process.stderr.read()[-2000:]
        raise AssertionError(f"Camouhost produced no response for {frame!r}: {stderr}")
    return response


def run_cold_launch(root: Path, report: dict[str, str], url: str) -> None:
    process = subprocess.Popen(
        [sys.executable, str(CAMOUHOST)],
        cwd=ROOT,
        env=base_env(root, report, url),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    try:
        assert exchange(process, "hello|1") == "hello_ack|1"
        assert exchange(process, f"launch|{SESSION}") == f"ready|{SESSION}"
        assert exchange(process, f"close|{SESSION}") == f"closed|{SESSION}|true"
        returncode = process.wait(timeout=60)
        if returncode != 0:
            stderr = process.stderr.read()[-2000:] if process.stderr is not None else ""
            raise AssertionError(f"Camouhost clean close returned {returncode}: {stderr}")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)


def expect_prelaunch_identity_rejection(root: Path, report: dict[str, str], url: str) -> None:
    env = base_env(root, report, url)
    env["CAMOUHOST_EXPECTED_CONFIG_SHA256"] = "0" * 64
    completed = subprocess.run(
        [sys.executable, str(CAMOUHOST)],
        cwd=ROOT,
        env=env,
        input="hello|1\n",
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )
    if completed.returncode != 4 or completed.stdout != "error|identity\n":
        raise AssertionError(
            f"config mismatch did not fail before launch: rc={completed.returncode} out={completed.stdout!r}"
        )


def expect_probe_drift_rejection(root: Path, report: dict[str, str], url: str) -> None:
    env = base_env(root, report, url)
    env["CAMOUHOST_EXPECTED_PROBE_SHA256"] = "0" * 64
    process = subprocess.Popen(
        [sys.executable, str(CAMOUHOST)],
        cwd=ROOT,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    try:
        assert exchange(process, "hello|1") == "hello_ack|1"
        assert exchange(process, f"launch|{SESSION}") == "error|runtime"
        if process.wait(timeout=60) != 5:
            raise AssertionError("profile-stable probe drift did not fail closed")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)


def main() -> int:
    server = ObservationServer()
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{server.server_address[1]}/"
    try:
        with tempfile.TemporaryDirectory(prefix="ar10-real-camoufox-") as temporary:
            base = Path(temporary)
            first_root = base / "generation-a"
            second_root = base / "generation-b"
            first = materialize(first_root)
            second = materialize(second_root)

            if first["fingerprint_config_sha256"] == second["fingerprint_config_sha256"]:
                raise AssertionError("independent candidate generations unexpectedly share exact fingerprint config")

            run_cold_launch(first_root, first, url)
            observed_first = server.observations.get(timeout=15)
            if observed_first != (False, False):
                raise AssertionError(f"first generation was contaminated before first launch: {observed_first}")

            run_cold_launch(first_root, first, url)
            observed_second = server.observations.get(timeout=15)
            if observed_second != (True, True):
                raise AssertionError(f"cookie/localStorage did not survive clean cold relaunch: {observed_second}")

            run_cold_launch(second_root, second, url)
            observed_other = server.observations.get(timeout=15)
            if observed_other != (False, False):
                raise AssertionError(f"browser state crossed generation boundary: {observed_other}")

            expect_prelaunch_identity_rejection(first_root, first, url)
            expect_probe_drift_rejection(first_root, first, url)

        print("AR-10 real Camoufox cold-launch, identity and persistence evidence passed.")
        return 0
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
