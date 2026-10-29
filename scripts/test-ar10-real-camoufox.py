#!/usr/bin/env python3
"""Repository-owned real Camoufox AR-10 integration evidence.

This test is intentionally local-only: it uses a loopback origin and an exact pinned
Camoufox runtime. Specialized fingerprint sites, real proxies and physical Windows
hosts remain external evidence.
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from types import ModuleType
from typing import BinaryIO, Callable, Iterator
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
CAMOUHOST = ROOT / "runtime/camouhost/real.py"
RUNTIME_LOCK = ROOT / "runtime/camouhost/runtime-lock.json"
SESSION = "session_01JAR10REALCAMOUFOX"
IPC_VERSION = "3"
HEADLESS_MODE_ENV = "AR10_REAL_CAMOUFOX_HEADLESS"
BRIDGE_LOCK_CONTENT = "profile-platform-bridge-lock-v1\ndevice_01JAR10REALCAMOUFOX\n1\n"
PAGE = b"""<!doctype html><meta charset='utf-8'><script>
const hadStorage = localStorage.getItem('ar10-marker') === 'persisted';
const hadCookie = document.cookie.includes('ar10-marker=persisted');
localStorage.setItem('ar10-marker', 'persisted');
document.cookie = 'ar10-marker=persisted; Max-Age=86400; Path=/; SameSite=Lax';
const observed = new XMLHttpRequest();
observed.open('GET', '/observed?storage=' + String(hadStorage) + '&cookie=' + String(hadCookie), false);
observed.send();
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


def requested_headless_mode() -> str:
    value = os.environ.get(HEADLESS_MODE_ENV, "virtual")
    if value not in {"false", "virtual"}:
        raise AssertionError(f"{HEADLESS_MODE_ENV} must be exactly false or virtual")
    return value


def base_env(root: Path, report: dict[str, str]) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "CAMOUHOST_PROFILE_ROOT": str(root),
            "CAMOUHOST_RUNTIME_LOCK": str(RUNTIME_LOCK),
            "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256": report["runtime_lock_sha256"],
            "CAMOUHOST_EXPECTED_CONFIG_SHA256": report["fingerprint_config_sha256"],
            "CAMOUHOST_EXPECTED_PROBE_SHA256": report["profile_stable_probe_sha256"],
            "CAMOUHOST_HEADLESS_MODE": requested_headless_mode(),
            "PYTHONUNBUFFERED": "1",
        }
    )
    return env


def materializer_env() -> dict[str, str]:
    return {
        **os.environ,
        "CAMOUHOST_RUNTIME_LOCK": str(RUNTIME_LOCK),
        "CAMOUHOST_HEADLESS_MODE": requested_headless_mode(),
    }


def write_bridge_lock(root: Path) -> None:
    (root / ".profile-platform.lock").write_text(BRIDGE_LOCK_CONTENT, encoding="utf-8")


def assert_clean_launch_lock_state(root: Path) -> None:
    bridge_lock = root / ".profile-platform.lock"
    if bridge_lock.read_text(encoding="utf-8") != BRIDGE_LOCK_CONTENT:
        raise AssertionError("candidate materialization changed Bridge writer ownership evidence")

    user_data = root / "user_data"
    if user_data.is_symlink() or (user_data.exists() and not user_data.is_dir()):
        raise AssertionError("generation-owned user_data must remain a real directory")

    for lock_path in (root / ".parentlock", root / "lock"):
        if lock_path.exists() or lock_path.is_symlink():
            raise AssertionError(
                f"unexpected root-level Firefox lock artifact: {lock_path.relative_to(root)}"
            )


def expect_materialization_requires_bridge_lock(root: Path) -> None:
    root.mkdir()
    completed = subprocess.run(
        [sys.executable, str(CAMOUHOST), "--materialize-identity", str(root)],
        cwd=ROOT,
        env=materializer_env(),
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )
    if completed.returncode != 7:
        raise AssertionError(
            "candidate materialization without Bridge ownership did not fail closed: "
            f"rc={completed.returncode} out={completed.stdout!r} err={completed.stderr[-500:]!r}"
        )
    if (root / "camoufox-config.json").exists() or (root / "user_data").exists():
        raise AssertionError("unowned candidate materialization mutated browser identity/state")


def materialize(root: Path) -> dict[str, str]:
    root.mkdir()
    write_bridge_lock(root)
    completed = subprocess.run(
        [sys.executable, str(CAMOUHOST), "--materialize-identity", str(root)],
        cwd=ROOT,
        env=materializer_env(),
        text=True,
        capture_output=True,
        timeout=180,
        check=False,
    )
    if completed.returncode != 0:
        raise AssertionError(
            f"candidate identity materialization failed: {completed.stderr[-2000:]}"
        )
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
    assert_clean_launch_lock_state(root)
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


def observe_browser_visible_wire(process: subprocess.Popen[str]) -> None:
    response = exchange(process, f"observe_browser_visible|{SESSION}")
    prefix = f"browser_visible|{SESSION}|"
    if not response.startswith(prefix):
        raise AssertionError(f"unexpected browser-visible frame: {response[:160]!r}")
    payload_hex = response[len(prefix) :]
    try:
        payload = bytes.fromhex(payload_hex)
        observation = json.loads(payload.decode("utf-8"))
    except (ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AssertionError("browser-visible wire payload is not bounded JSON evidence") from error
    if not isinstance(observation, dict) or not observation:
        raise AssertionError("browser-visible wire payload has invalid top-level shape")


def admit_navigation(process: subprocess.Popen[str], url: str | None) -> None:
    target = "" if url is None else url.encode("utf-8").hex()
    response = exchange(process, f"admit_navigation|{SESSION}|{target}")
    if response != f"navigation_admitted|{SESSION}":
        raise AssertionError(f"navigation admission failed: {response!r}")


def complete_pre_navigation_protocol(
    process: subprocess.Popen[str],
    url: str | None,
) -> None:
    observe_browser_visible_wire(process)
    admit_navigation(process, url)


def prove_managed_environment_projection(runtime: ModuleType) -> None:
    expected = {
        "SYSTEMDRIVE": "C:",
        "SYSTEMROOT": r"C:\Windows",
        "WINDIR": r"C:\Windows",
        "PROGRAMDATA": r"C:\ProgramData",
        "ALLUSERSPROFILE": r"C:\ProgramData",
        "APPDATA": r"C:\Users\Test\AppData\Roaming",
        "LOCALAPPDATA": r"C:\Users\Test\AppData\Local",
    }
    injected = {
        "CLOUDFLARE_API_TOKEN": "must-not-cross",
        "HTTP_PROXY": "must-not-cross",
        "CAMOU_CONFIG_1": "must-not-cross",
    }
    with patch.dict(os.environ, expected | injected, clear=True):
        assert runtime.browser_environment() == expected


def run_cold_launch(
    root: Path, report: dict[str, str], url: str, on_ready: Callable[[], None] | None = None
) -> None:
    # A file avoids a full stderr pipe blocking runtime-owned browser cleanup. Read only
    # a bounded tail after process exit; do not block on stderr while the child is alive.
    with tempfile.TemporaryFile() as diagnostics:
        run_cold_launch_observed(root, report, url, diagnostics, on_ready)


def run_cold_launch_observed(
    root: Path, report: dict[str, str], url: str, diagnostics: BinaryIO,
    on_ready: Callable[[], None] | None = None,
) -> None:
    process = subprocess.Popen(
        [sys.executable, str(CAMOUHOST)],
        cwd=ROOT,
        env=base_env(root, report),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=diagnostics,
        text=True,
        bufsize=1,
    )
    primary = None
    cleanup = "normal_exit"
    try:
        assert exchange(process, f"hello|{IPC_VERSION}") == f"hello_ack|{IPC_VERSION}"
        assert exchange(process, f"launch|{SESSION}") == f"ready|{SESSION}"
        if on_ready is not None:
            on_ready()
        complete_pre_navigation_protocol(process, url)
        assert exchange(process, f"close|{SESSION}") == f"closed|{SESSION}|true"
        returncode = process.wait(timeout=60)
        if returncode != 0:
            raise AssertionError(f"Camouhost clean close returned {returncode}")
    except BaseException as error:
        primary = error
    finally:
        if process.poll() is None:
            # EOF lets run_ipc finish its finally block even before navigation admission.
            # Sending `close` here is invalid in some of the failed protocol states.
            cleanup = "eof_exit"
            try:
                if process.stdin is not None:
                    process.stdin.close()
            except (BrokenPipeError, OSError):
                pass
            try:
                process.wait(timeout=60)
            except subprocess.TimeoutExpired:
                cleanup = "forced_kill"
                try:
                    process.kill()
                    process.wait(timeout=30)
                except (OSError, subprocess.TimeoutExpired):
                    cleanup = "forced_kill_failed"
        for pipe in (process.stdin, process.stdout):
            if pipe is not None:
                try:
                    pipe.close()
                except OSError:
                    cleanup += "_pipe_close_failed"
        try:
            diagnostics.flush()
            diagnostics.seek(0, os.SEEK_END)
            diagnostics.seek(max(0, diagnostics.tell() - 4096))
            tail = diagnostics.read().decode("utf-8", errors="replace")
        except OSError:
            tail = "diagnostic_capture_failed"
            cleanup += "_diagnostic_capture_failed"
        evidence = f"Camouhost rc={process.poll()} cleanup={cleanup}; stderr_tail={tail!r}"
    if primary is not None:
        primary.add_note(evidence)
        raise primary
    if cleanup != "normal_exit":
        raise AssertionError(evidence)


@contextlib.contextmanager
def cold_launch_workspace() -> Iterator[Path]:
    temporary = tempfile.TemporaryDirectory(prefix="ar10-real-camoufox-")
    try:
        yield Path(temporary.name)
    except BaseException as primary:
        try:
            temporary.cleanup()
        except OSError as cleanup:
            primary.add_note(f"Camoufox workspace cleanup failed separately: {type(cleanup).__name__}")
        raise
    else:
        temporary.cleanup()


def expect_prelaunch_identity_rejection(root: Path, report: dict[str, str]) -> None:
    env = base_env(root, report)
    env["CAMOUHOST_EXPECTED_CONFIG_SHA256"] = "0" * 64
    completed = subprocess.run(
        [sys.executable, str(CAMOUHOST)],
        cwd=ROOT,
        env=env,
        input=f"hello|{IPC_VERSION}\n",
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )
    if completed.returncode != 4 or completed.stdout != "error|identity\n":
        raise AssertionError(
            f"config mismatch did not fail before launch: rc={completed.returncode} out={completed.stdout!r}"
        )


def expect_probe_digest_non_authoritative(root: Path, report: dict[str, str]) -> None:
    env = base_env(root, report)
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
        assert exchange(process, f"hello|{IPC_VERSION}") == f"hello_ack|{IPC_VERSION}"
        assert exchange(process, f"launch|{SESSION}") == f"ready|{SESSION}"
        complete_pre_navigation_protocol(process, None)
        assert exchange(process, f"close|{SESSION}") == f"closed|{SESSION}|true"
        if process.wait(timeout=60) != 0:
            stderr = process.stderr.read()[-2000:] if process.stderr is not None else ""
            raise AssertionError(
                "aggregate probe evidence unexpectedly became an admission veto: " + stderr
            )
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
        with cold_launch_workspace() as base:
            expect_materialization_requires_bridge_lock(base / "generation-unowned")
            first_root = base / "generation-a"
            second_root = base / "generation-b"
            first = materialize(first_root)
            second = materialize(second_root)

            if first["fingerprint_config_sha256"] == second["fingerprint_config_sha256"]:
                raise AssertionError(
                    "independent candidate generations unexpectedly share exact fingerprint config"
                )

            run_cold_launch(first_root, first, url)
            observed_first = server.observations.get(timeout=15)
            if observed_first != (False, False):
                raise AssertionError(
                    f"first generation was contaminated before first launch: {observed_first}"
                )

            assert_clean_launch_lock_state(first_root)
            # The prior browser has exited. Prove its requests are completely
            # consumed before attributing any new observation to restored tabs.
            if not server.observations.empty():
                raise AssertionError(
                    "prior browser left queued network observations before session restore"
                )
            # Restore a *previously browsed* tab on the next cold launch. The
            # test copy retains Firefox's own session setting and files.
            with (first_root / "user_data" / "user.js").open(
                "a", encoding="utf-8", newline="\n"
            ) as prefs:
                prefs.write('\nuser_pref("browser.startup.page", 3);\n')

            def require_no_early_network() -> None:
                if not server.observations.empty():
                    raise AssertionError(
                        "restored tab accessed the network before navigation admission"
                    )

            run_cold_launch(first_root, first, url, require_no_early_network)
            observed_second = server.observations.get(timeout=15)
            if observed_second != (True, True):
                raise AssertionError(
                    "durable cookie/localStorage did not survive clean cold relaunch: "
                    f"{observed_second}"
                )
            # A restored foreground tab may reload after admission alongside
            # the explicit navigation; neither may contaminate another profile.
            while not server.observations.empty():
                if server.observations.get_nowait() != (True, True):
                    raise AssertionError("restored tab lost its persisted origin state")

            assert_clean_launch_lock_state(first_root)
            run_cold_launch(second_root, second, url)
            observed_other = server.observations.get(timeout=15)
            if observed_other != (False, False):
                raise AssertionError(
                    f"browser state crossed generation boundary: {observed_other}"
                )

            assert_clean_launch_lock_state(second_root)
            expect_prelaunch_identity_rejection(first_root, first)
            expect_probe_digest_non_authoritative(first_root, first)

        print(
            "AR-10 real Camoufox cold-launch, identity, Bridge ownership, "
            "typed admission authority and persistence evidence passed."
        )
        return 0
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
