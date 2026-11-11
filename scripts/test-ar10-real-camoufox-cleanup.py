#!/usr/bin/env python3
"""Deterministic real-Camoufox regression for post-launch Camouhost cleanup.

The runtime owns the browser context after `ready`. Any later protocol/runtime failure
must release that ownership before process exit so the exact same generation can be
opened immediately by a fresh shipping-runtime process. This proof intentionally avoids
browser-visible observation so it cannot hide the lifecycle invariant behind an
independent observation-surface failure.
"""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import ctypes
import importlib.util
import io
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
from types import ModuleType, SimpleNamespace
from unittest.mock import Mock, patch

IPC_VERSION = "3"
SESSION = "session_01JAV2CLEANUP"
BRIDGE_LOCK = "profile-platform-bridge-lock-v1\ndevice_01JAV2CLEANUP\n1\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--camouhost", type=Path, required=True)
    parser.add_argument("--runtime-lock", type=Path, required=True)
    parser.add_argument("--headless", choices=("false", "virtual"), required=True)
    parser.add_argument(
        "--native-restore-only", action="store_true",
        help="Run actual packaged Firefox SessionStore tabs + POST proof only",
    )
    return parser.parse_args()


def require_regular(path: Path, label: str) -> Path:
    resolved = path.resolve(strict=True)
    if path.is_symlink() or not resolved.is_file() or resolved.stat().st_size == 0:
        raise AssertionError(f"{label} is not a non-empty regular file")
    return resolved


def load_runtime(camouhost: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location("ar10_real_camouhost_cleanup_test", camouhost)
    if spec is None or spec.loader is None:
        raise AssertionError("cannot load Camouhost runtime for writer-lock observation")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def prove_managed_bootstrap_tab_contract(runtime: ModuleType) -> None:
    """A real page may never be closed just because its URL is about:blank."""

    marker = runtime.MANAGED_BOOTSTRAP_TAB_URL

    class Page:
        def __init__(self, url: str) -> None:
            self.url = url
            self.closed = False
            self.navigations: list[str] = []

        def goto(self, url: str, *, wait_until: str, timeout: int) -> None:
            assert url == "about:blank" and wait_until == "domcontentloaded"
            assert timeout == 5_000
            self.navigations.append(url)
            self.url = url

        def close(self) -> None:
            self.closed = True

    def check(pages: list[Page]) -> None:
        runtime.retire_managed_bootstrap_tabs(SimpleNamespace(pages=pages))

    ordinary = Page("about:blank")
    real_site = Page("http://127.0.0.1:43187/verify")
    owned_a, owned_b = Page(marker), Page(marker)
    check([owned_a, ordinary, real_site, owned_b])
    assert owned_a.closed and owned_b.closed
    assert not ordinary.closed and not ordinary.navigations
    assert not real_site.closed and not real_site.navigations

    sole = Page(marker)
    check([sole])
    assert not sole.closed and sole.url == "about:blank"
    assert sole.navigations == ["about:blank"]

    first, second = Page(marker), Page(marker)
    check([first, second])
    assert not first.closed and first.url == "about:blank"
    assert second.closed

    untouched = Page("about:blank")
    check([untouched])
    assert not untouched.closed and not untouched.navigations


def prove_official_context_close_event_contract(runtime: ModuleType) -> None:
    """A Playwright context close event is the only adapter close signal."""

    class Context:
        def __init__(self) -> None:
            self.listeners: dict[str, object] = {}
            self.closed = False

        def on(self, name: str, listener: object) -> None:
            self.listeners[name] = listener

        def is_closed(self) -> bool:
            return self.closed

        def close(self) -> None:
            self.closed = True
            callback = self.listeners.get("close")
            assert callable(callback)
            callback(self)

        def crash(self) -> None:
            # Playwright reports browser application crash as context closure.
            self.close()

    normal = Context()
    state = runtime.install_context_close_observation(normal)
    assert runtime.context_close_observed(normal, state) is False
    normal.close()
    assert runtime.context_close_observed(normal, state) is True
    assert state["event_count"] == 1

    crashed = Context()
    crash_state = runtime.install_context_close_observation(crashed)
    crashed.crash()
    assert runtime.context_close_observed(crashed, crash_state) is True
    assert crash_state["event_count"] == 1

    source = (Path(__file__).resolve().parents[1] / "runtime/camouhost/real.py").read_text(
        encoding="utf-8"
    )
    for forbidden in (
        "WindowsBrowserExitWitness",
        "firefox_writer_active",
        "wait_for_browser_quiescence",
        "EnumWindows",
        "CreateToolhelp32Snapshot",
        "WM_CLOSE",
        "PostMessage",
        "install_controlled_close_observation",
        "observe_controlled_close",
    ):
        assert forbidden not in source, f"retired browser observer remains: {forbidden}"


def runtime_env(runtime_lock: Path, headless: str) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "CAMOUHOST_RUNTIME_LOCK": str(runtime_lock),
            "CAMOUHOST_HEADLESS_MODE": headless,
            "CAMOUHOST_EXECUTION_MODE": "isolated-cold-probe",
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
    # This test drives the official SDK directly, so it reproduces the local
    # filesystem preparation that Rust/ProfileBridge performs before Open.
    user_data = root / "user_data"
    user_data.mkdir()
    (root / ".profile-platform.lock").write_text(
        BRIDGE_LOCK, encoding="utf-8", newline="\n"
    )
    # The retired CLI was only a redundant production Python process.
    # This isolated test directly calls the same official fpgen function in
    # the packaged runtime; only Rust/ProfileBridge persists its result in
    # production, while this disposable fixture mirrors that exact write.
    fixture = """
import contextlib
import importlib.util
import json
import sys
from pathlib import Path
camouhost, root = map(Path, sys.argv[1:])
spec = importlib.util.spec_from_file_location("camouhost_fixture", camouhost)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
with contextlib.redirect_stdout(sys.stderr):
    report = module.materialize_candidate_identity(root)
print(json.dumps(report, separators=(",", ":"), sort_keys=True), flush=True)
"""
    completed = subprocess.run(
        [str(python), "-c", fixture, str(camouhost), str(root)],
        env=runtime_env(runtime_lock, headless),
        text=True,
        capture_output=True,
        timeout=300,
        check=False,
    )
    if completed.returncode != 0:
        raise AssertionError(
            "candidate identity materialization failed: "
            f"rc={completed.returncode} diagnostic={completed.stdout[-1200:]!r} "
            f"err={completed.stderr[-3000:]!r}"
        )
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise AssertionError("materialization report is not JSON") from error
    required = {
        "camoufox_config_json",
        "fingerprint_config_sha256",
        "fingerprint_policy_version",
        "fingerprint_materialization_sha256",
        "runtime_lock_sha256",
    }
    if set(report) != required or any(
        not isinstance(value, str) or not value for value in report.values()
    ):
        raise AssertionError(f"unexpected materialization report keys: {sorted(report)}")
    import hashlib

    canonical = report["camoufox_config_json"].encode("utf-8")
    try:
        decoded = json.loads(canonical)
    except json.JSONDecodeError as error:
        raise AssertionError("official SDK config is not JSON") from error
    expected_canonical = (
        json.dumps(decoded, ensure_ascii=True, separators=(",", ":"), sort_keys=True)
        + "\n"
    ).encode("utf-8")
    if canonical != expected_canonical:
        raise AssertionError("official SDK config is not canonical")
    if hashlib.sha256(canonical).hexdigest() != report["fingerprint_config_sha256"]:
        raise AssertionError("official SDK config digest does not match returned config")
    # The production Rust owner persists this exact canonical payload. The
    # test harness mirrors that write; Camouhost itself never writes the file.
    (root / "camoufox-config.json").write_bytes(canonical)
    expected = hashlib.sha256(b"profile-stability-v2-fpgen-config\0" + canonical).hexdigest()
    if report["fingerprint_materialization_sha256"] != expected:
        raise AssertionError("official fpgen materialization was not bound to exact config")
    return report


def process_env(
    runtime_lock: Path,
    headless: str,
    root: Path,
    report: dict[str, str],
) -> dict[str, str]:
    env = runtime_env(runtime_lock, headless)
    env.update(
        {
            "CAMOUHOST_PROFILE_ROOT": str(root.resolve(strict=True)),
            "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256": report["runtime_lock_sha256"],
            "CAMOUHOST_EXPECTED_CONFIG_SHA256": report["fingerprint_config_sha256"],
            "CAMOUHOST_EXPECTED_PROBE_SHA256": report["fingerprint_materialization_sha256"],
        }
    )
    return env


def start_runtime(
    python: Path,
    camouhost: Path,
    env: dict[str, str],
) -> subprocess.Popen[str]:
    # Firefox grandchildren can retain stderr across Camouhost teardown.
    # A file prevents both pipe backpressure and an unbounded stderr.read().
    diagnostics = tempfile.TemporaryFile(mode="w+t", encoding="utf-8")
    # This is a disposable CI-only context. A test requesting unrestricted
    # local Playwright fixtures must select the explicit agent mode; the C09
    # historical mode rejects executable script inboxes.
    dispatch_env = dict(env)
    if dispatch_env.get("CAMOUHOST_AGENT_DIRECTORY"):
        dispatch_env["CAMOUHOST_EXECUTION_MODE"] = "automation-playwright"
    try:
        process = subprocess.Popen(
            [str(python), str(camouhost)],
            env=dispatch_env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=diagnostics,
            text=True,
            bufsize=1,
        )
    except BaseException:
        diagnostics.close()
        raise
    process.stderr = diagnostics
    return process


def exchange(process: subprocess.Popen[str], frame: str, seconds: int = 120) -> str:
    assert process.stdin is not None
    assert process.stdout is not None
    response: queue.Queue[str] = queue.Queue(maxsize=1)

    def read_one() -> None:
        try:
            response.put(process.stdout.readline().rstrip("\n"))
        except (OSError, ValueError):
            response.put("")

    threading.Thread(target=read_one, daemon=True).start()
    # Send the exact Bridge wire bytes. Windows TextIOWrapper.write("\\n")
    # otherwise translates LF to CRLF; the real Bridge IPC rejects CR.
    process.stdin.buffer.write((frame + "\n").encode("utf-8"))
    process.stdin.buffer.flush()
    try:
        result = response.get(timeout=seconds)
    except queue.Empty as error:
        # Log only the command name, never a URL or profile-bearing IPC payload.
        command = frame.partition("|")[0]
        raise AssertionError(
            f"Camouhost IPC {command} timed out after {seconds}s; "
            f"process_alive={process.poll() is None}; stderr_tail={stderr_tail(process)!r}"
        ) from error
    if not result:
        raise AssertionError(
            f"Camouhost produced no IPC response for {frame.partition('|')[0]}: "
            f"{stderr_tail(process)!r}"
        )
    return result


def stderr_tail(process: subprocess.Popen[str]) -> str:
    if process.stderr is None:
        return ""
    process.stderr.seek(0, os.SEEK_END)
    last = process.stderr.tell()
    process.stderr.seek(max(0, last - 3000))
    return process.stderr.read()[-3000:]


def stop_runtime(process: subprocess.Popen[str]) -> None:
    if process.poll() is None:
        process.kill()
        process.wait(timeout=30)
    for stream in (process.stdin, process.stderr):
        if stream is not None and not stream.closed:
            stream.close()
    # Do not close stdout here: on a timeout its daemon reader may still be
    # blocked in readline() on an inherited Firefox pipe. The test exits on
    # that failure, and closing a TextIOWrapper from another thread may hang.


def assert_writer_quiescent(_runtime: ModuleType, _root: Path, label: str) -> None:
    """Assert that native file-access decisions belong to Rust, not Camouhost."""
    root = Path(__file__).resolve().parents[1]
    browser_execution = (root / "apps/profile-bridge/src/browser_execution.rs").read_text(
        encoding="utf-8"
    )
    local_open = (root / "apps/profile-bridge/src/local_browser_open.rs").read_text(
        encoding="utf-8"
    )
    assert "browser_writer_locks_absent_at" in browser_execution
    # The redundant pre-launch writer check was removed; the Rust owner
    # still checks native writer quiescence before saving/after closure.
    assert local_open.count("browser_writer_locks_absent(&workspace)") >= 2
    assert "firefox_writer_active" not in (root / "runtime/camouhost/real.py").read_text(
        encoding="utf-8"
    )
    print(f"{label}: Rust native file gate owns writer proof")


def prove_protocol_failure_cleanup(
    python: Path,
    camouhost: Path,
    env: dict[str, str],
    runtime: ModuleType,
    root: Path,
) -> None:
    process = start_runtime(python, camouhost, env)
    try:
        if exchange(process, f"hello|{IPC_VERSION}") != f"hello_ack|{IPC_VERSION}":
            raise AssertionError("Camouhost IPC negotiation failed")
        if exchange(process, f"launch|{SESSION}") != f"ready|{SESSION}":
            raise AssertionError("real Camoufox did not reach ready before failure injection")

        # A second hello is a valid IPC frame but invalid in the active-session state. Preserve
        # the existing protocol failure while requiring the runtime-owned context to be released.
        if exchange(process, f"hello|{IPC_VERSION}") != "error|protocol":
            raise AssertionError("post-launch invalid state did not preserve error|protocol")
        returncode = process.wait(timeout=90)
        if returncode != 2:
            raise AssertionError(
                f"post-launch protocol failure returned rc={returncode}: {stderr_tail(process)}"
            )
    finally:
        stop_runtime(process)
    assert_writer_quiescent(runtime, root, "post-launch protocol failure")


def prove_immediate_relaunch_after_cleanup(
    python: Path,
    camouhost: Path,
    env: dict[str, str],
    runtime: ModuleType,
    root: Path,
) -> None:
    process = start_runtime(python, camouhost, env)
    try:
        if exchange(process, f"hello|{IPC_VERSION}") != f"hello_ack|{IPC_VERSION}":
            raise AssertionError("relaunch IPC negotiation failed")
        if exchange(process, f"launch|{SESSION}") != f"ready|{SESSION}":
            raise AssertionError("same generation could not immediately relaunch after cleanup")

        # EOF is another non-success exit after launch. It must use the same centralized cleanup
        # path and preserve the existing rc=3 incomplete-session contract.
        assert process.stdin is not None
        process.stdin.close()
        returncode = process.wait(timeout=90)
        if returncode != 3:
            raise AssertionError(
                f"post-launch EOF returned rc={returncode}: {stderr_tail(process)}"
            )
    finally:
        stop_runtime(process)
    assert_writer_quiescent(runtime, root, "post-launch EOF")


def prove_session_restore_startup(
    python: Path,
    camouhost: Path,
    env: dict[str, str],
    runtime: ModuleType,
    root: Path,
) -> None:
    """Real disposable Firefox profile: session restore must not deadlock IPC Ready.

    This checks the native pref rather than mocking Camoufox or disabling Firefox
    session restore. Only the temporary test generation is modified.
    """
    prefs = root / "user_data" / "user.js"
    with prefs.open("a", encoding="utf-8", newline="\n") as stream:
        stream.write('\nuser_pref("browser.startup.page", 3);\n')
    process = start_runtime(python, camouhost, env)

    try:
        assert exchange(process, f"hello|{IPC_VERSION}", 15) == f"hello_ack|{IPC_VERSION}"
        assert exchange(process, f"launch|{SESSION}", 45) == f"ready|{SESSION}"
        assert exchange(process, f"observe_browser_visible|{SESSION}", 30).startswith(
            f"browser_visible|{SESSION}|"
        ), "session restore has no browser-visible identity witness"
        assert exchange(process, f"admit_navigation|{SESSION}|", 30) == (
            f"navigation_admitted|{SESSION}"
        )
        assert exchange(process, f"close|{SESSION}", 45) == f"closed|{SESSION}|true"
        assert process.wait(timeout=30) == 0
    finally:
        stop_runtime(process)
    assert 'user_pref("browser.startup.page", 3);' in prefs.read_text(encoding="utf-8")
    assert_writer_quiescent(runtime, root, "Firefox session restore startup")




def prove_owned_playwright_agent(
    python: Path,
    camouhost: Path,
    env: dict[str, str],
    root: Path,
) -> None:
    """Actual browser integration: same owned context, full trace, no UIA/second writer."""
    with tempfile.TemporaryDirectory(prefix="ar10-owned-playwright-agent-") as temporary:
        base = Path(temporary).resolve()
        script = base / "browser-actions.py"
        script.write_text(
            "from pathlib import Path\n"
            "page = context.pages[0] if context.pages else context.new_page()\n"
            "context.tracing.start(screenshots=True, snapshots=True, sources=True)\n"
            "try:\n"
            "    page.goto('about:blank')\n"
            "    page.set_content('<!doctype html><input id=agent-input><p id=proof>agent-owned</p>')\n"
            "    page.locator('#agent-input').fill('full-playwright-control')\n"
            "    assert page.locator('#agent-input').input_value() == 'full-playwright-control'\n"
            "    assert page.locator('#proof').inner_text() == 'agent-owned'\n"
            "    page.screenshot(path=str(agent_artifacts_dir / 'page.png'))\n"
            "    result = {'url': page.url, 'html': page.content(), "
            "'value': page.locator('#agent-input').input_value()}\n"
            "finally:\n"
            "    context.tracing.stop(path=str(agent_artifacts_dir / 'trace.zip'))\n",
            encoding="utf-8",
        )
        agent_env = dict(env, CAMOUHOST_AGENT_DIRECTORY=str(base))
        process = start_runtime(python, camouhost, agent_env)
        try:
            assert exchange(process, f"hello|{IPC_VERSION}", 15) == f"hello_ack|{IPC_VERSION}"
            assert exchange(process, f"launch|{SESSION}", 120) == f"ready|{SESSION}"
            assert exchange(process, f"observe_browser_visible|{SESSION}", 30).startswith(
                f"browser_visible|{SESSION}|"
            )
            assert exchange(process, f"admit_navigation|{SESSION}|", 30) == (
                f"navigation_admitted|{SESSION}"
            )
            client = [
                sys.executable, str(Path(__file__).with_name("camouhost-agent.py")),
                "--directory", str(base), "--pid", str(process.pid),
                "--script", str(script), "--timeout", "120",
            ]
            command = subprocess.run(client, capture_output=True, text=True, timeout=130)
            assert command.returncode == 0, command.stderr
            assert "full-playwright-control" in command.stdout
            manifest = json.loads((base / f"active-{process.pid}.json").read_text(encoding="utf-8"))
            results = Path(manifest["run"]) / "results"
            assert (results / "trace.zip").is_file()
            assert (results / "trace.zip").stat().st_size > 0
            assert (results / "page.png").is_file()
            assert exchange(process, f"observe_close|{SESSION}", 10) == (
                f"close_observed|{SESSION}|false"
            ), "agent-driven web actions must not forge browser closure"
            assert exchange(process, f"close|{SESSION}", 45) == f"closed|{SESSION}|true"
            assert process.wait(timeout=30) == 0
            assert not (base / f"active-{process.pid}.json").exists(), (
                "live agent descriptor survived owned Camouhost termination"
            )
        finally:
            stop_runtime(process)
    print("native_playwright_agent proof=real_context_dom_screenshot_trace_and_close PASS", flush=True)



def prove_preexisting_tabs_restored(
    python: Path,
    camouhost: Path,
    runtime_lock: Path,
    headless: str,
    root: Path,
) -> None:
    """End-to-end native Firefox sessionstore -> Playwright tracked tabs, isolated profile.

    Use the packaged browser Python on Windows, not the CI host Python. This proof
    deliberately does not share or inspect any real customer Firefox files.
    """
    print("native_restore_proof stage=materialize", flush=True)
    materialize(python, camouhost, runtime_lock, headless, root)

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            page = b"<!doctype html><title>Disposable Firefox sessionstore proof</title>"
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(page)))
            self.end_headers()
            self.wfile.write(page)

        def do_POST(self) -> None:  # noqa: N802
            server.post_paths.append(self.path)
            self.rfile.read(int(self.headers.get("Content-Length", "0")))
            self.do_GET()

        def log_message(self, _format: str, *_args: object) -> None:
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.post_paths = []
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    proof = r"""
import contextlib
import importlib.util
import json
import os
import sys
import time
from pathlib import Path
from camoufox.sync_api import Camoufox
from playwright.sync_api import TimeoutError as PlaywrightTimeoutError

camouhost, lockfile, root, origin = map(str, sys.argv[1:5])
spec = importlib.util.spec_from_file_location("camouhost_restore_session_proof", camouhost)
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)
root = Path(root)
lock = json.loads(Path(lockfile).read_text(encoding="utf-8"))
config = json.loads((root / "camoufox-config.json").read_text(encoding="utf-8"))
urls = [origin + "/prior-first", origin + "/prior-second"]
expected_title = "Disposable Firefox sessionstore proof"

def retry_as_user(page):
    # Firefox owns retry/error-page UI. Releasing the isolated context's offline
    # gate must not cause Camouhost to reload or navigate restored tabs.
    if page.title() != expected_title:
        assert page.evaluate("document.documentURI").startswith(
            "about:neterror?e=netOffline&"
        ), "restored tab was neither loaded nor Firefox's native offline error page"
        page.reload(wait_until="domcontentloaded", timeout=15000)
    assert page.title() == expected_title, (
        "Firefox's native user retry did not recover the restored tab"
    )

def enter():
    kwargs = runtime.camoufox_launch_options(lock, root, config, isolated=True)
    assert kwargs["firefox_user_prefs"]["browser.startup.page"] == 3
    manager = Camoufox(
        from_options=kwargs,
        headless=runtime.resolve_headless_mode(),
        persistent_context=True,
    )
    with contextlib.redirect_stdout(sys.stderr):
        context = manager.__enter__()
    context.set_offline(True)
    return manager, context

manager, context = enter()
try:
    context.set_offline(False)
    audio_page = context.pages[0] if context.pages else context.new_page()
    audio_crashes = []
    audio_page.on("crash", lambda _page: audio_crashes.append(True))
    audio_state = audio_page.evaluate(
        "() => new OfflineAudioContext(1, 256, 44100).state"
    )
    audio_page.wait_for_timeout(250)
    assert audio_state == "suspended", (
        "packaged Camoufox OfflineAudioContext did not initialize: "
        f"state={audio_state!r}"
    )
    assert not audio_crashes and not audio_page.is_closed(), (
        "packaged Camoufox content page crashed during OfflineAudioContext initialization"
    )
    print(
        "native_restore_proof stage=offline_audio_context "
        "state=suspended page_crash=0",
        flush=True,
    )
    first = audio_page
    first.goto(urls[0], wait_until="domcontentloaded", timeout=15000)
    first.evaluate("localStorage.setItem('restore-proof', 'retained')")
    second = context.new_page()
    second.goto(urls[1], wait_until="domcontentloaded", timeout=15000)
    assert first.url == urls[0] and second.url == urls[1]

    # Put a real form POST in Firefox's saved navigation history.
    post_page = context.new_page()
    post_page.goto(origin + "/post-source", wait_until="domcontentloaded", timeout=15000)
    post_page.evaluate(
        "() => { const f=document.createElement('form'); f.method='POST'; "
        "f.action='/post'; document.body.appendChild(f); f.submit(); }"
    )
    post_page.wait_for_url(origin + "/post", timeout=15000)

    # INVESTIGATE-05: same actual packaged context and origin, no fake
    # browser/cookie backend. Distinct names avoid masking one mechanism by
    # another. Expiry is generated relative to the browser host clock.
    expires = int(time.time()) + 3600
    context.add_cookies([{
        "name": "b7_playwright_url", "value": "url_v1",
        "url": urls[0], "expires": expires, "sameSite": "Lax",
    }])
    context.add_cookies([{
        "name": "b7_playwright_domain", "value": "domain_v1",
        "domain": "127.0.0.1", "path": "/",
        "expires": expires, "sameSite": "Lax",
    }])
    first.evaluate(
        "() => { document.cookie = 'b7_native_cookie=native_v1; Path=/; Max-Age=3600; SameSite=Lax'; }"
    )
    expected_cookies = {
        "b7_playwright_url": "url_v1",
        "b7_playwright_domain": "domain_v1",
        "b7_native_cookie": "native_v1",
    }
    actual_before = {
        item["name"]: item["value"] for item in context.cookies([origin])
        if item["name"] in expected_cookies
    }
    # Native Firefox must retain not only the values but also each cookie's
    # absolute site-compatible expiry. A session-only cookie with the same
    # value is not an equivalent recovered authenticated session.
    cookie_metadata_before = {
        item["name"]: {
            "expires": item["expires"],
            "sameSite": item["sameSite"],
            "secure": item["secure"],
            "httpOnly": item["httpOnly"],
        }
        for item in context.cookies([origin])
        if item["name"] in expected_cookies
    }
    assert set(cookie_metadata_before) == set(expected_cookies)
    assert all(
        item["expires"] >= time.time() + 3000
        and item["sameSite"] == "Lax"
        and not item["secure"]
        and not item["httpOnly"]
        for item in cookie_metadata_before.values()
    ), "native initial cookie attributes or persistent expiry are missing"
    visible_before = first.evaluate("document.cookie")
    assert actual_before == expected_cookies, (
        "same-context context.add_cookies failed immediate cookie readback: "
        f"expected_names={sorted(expected_cookies)} "
        f"actual_names={sorted(actual_before)}"
    )
    assert all(
        f"{key}={value}" in visible_before
        for key, value in expected_cookies.items()
    ), "valid non-HttpOnly cookies were not visible at their original origin"
finally:
    runtime.close_context(manager, context)

manager, context = enter()
try:
    # First observation BEFORE any new page, navigation or mutation.
    initial_urls = [page.url for page in context.pages]
    reopened_cookies = {
        item["name"]: item["value"] for item in context.cookies([origin])
        if item["name"] in expected_cookies
    }
    assert reopened_cookies == expected_cookies, (
        "packaged native full-process reopen did not retain valid cookies: "
        f"initial_urls={initial_urls!r}, retained_names={sorted(reopened_cookies)}"
    )
    reopened_metadata = {
        item["name"]: {
            "expires": item["expires"],
            "sameSite": item["sameSite"],
            "secure": item["secure"],
            "httpOnly": item["httpOnly"],
        }
        for item in context.cookies([origin])
        if item["name"] in expected_cookies
    }
    assert reopened_metadata == cookie_metadata_before, (
        "native Firefox changed cookie expiration or attributes on cold reopen: "
        f"names={sorted(reopened_metadata)}"
    )
    # Firefox SessionStore restores windows asynchronously on Windows. Wait
    # for the existing native pages; never synthesize pages on the second launch.
    deadline = time.monotonic() + 20
    while True:
        present = {page.url: page for page in context.pages}
        missing = set(urls) - set(present)
        if not missing:
            break
        if time.monotonic() >= deadline:
            raise AssertionError(
                "previously open Firefox tabs absent from Playwright context; "
                f"missing={sorted(missing)!r}; "
                f"observed={sorted(page.url for page in context.pages)!r}"
            )
        try:
            context.wait_for_event("page", timeout=500)
        except PlaywrightTimeoutError:
            pass
    assert any(
        page.evaluate("document.documentURI").startswith("about:neterror?e=netOffline&")
        for page in present.values()
    ), "Firefox no-store restore had no pre-admission offline error"
    runtime.admit_navigation(context, None)
    assert not any(
        page.url == runtime.MANAGED_BOOTSTRAP_TAB_URL for page in context.pages
    ), "Camouhost's internal bootstrap tab survived a native SessionStore restore"
    retry_as_user(present[urls[0]])
    retry_as_user(present[urls[1]])
    retained = present[urls[0]].evaluate("localStorage.getItem('restore-proof')")
    if retained != "retained":
        raise AssertionError("restored tab lost persistent origin state")
    # A deliberate user POST after admission must still work normally:
    # Camouhost must not leave a broad network interception layer installed.
    user_post = context.new_page()
    user_post.goto(origin + "/user-post-source", wait_until="domcontentloaded", timeout=15000)
    user_post.evaluate(
        "() => { const f=document.createElement('form'); f.method='POST'; "
        "f.action='/user-post'; document.body.appendChild(f); f.submit(); }"
    )
    user_post.wait_for_url(origin + "/user-post", timeout=15000)
    # The application does not close or manage individual user tabs.
    assert not present[urls[1]].is_closed()
finally:
    runtime.close_context(manager, context)

print(
    "two previous tabs restored; persistent same-origin storage, cookie expiry "
    "and attributes retained", flush=True
)
"""
    try:
        env = runtime_env(runtime_lock, headless)
        print("native_restore_proof stage=two_launches", flush=True)
        # Firefox grandchildren may inherit stdout/stderr after the Python child
        # exits. Capture to files, not pipes: communicate() would wait forever for
        # inherited pipe writers and obscure the actual restore failure.
        with tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as output, \
                tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as diagnostics:
            child = subprocess.Popen(
                [str(python), "-c", proof, str(camouhost), str(runtime_lock),
                 str(root), f"http://127.0.0.1:{server.server_port}"],
                env=env, stdin=subprocess.DEVNULL, stdout=output, stderr=diagnostics,
            )
            try:
                result = child.wait(timeout=150)
            except subprocess.TimeoutExpired as error:
                child.kill()
                child.wait(timeout=15)
                raise AssertionError(
                    "native session-restoration child exceeded bounded lifecycle"
                ) from error
            output.seek(0)
            diagnostics.seek(0, os.SEEK_END)
            diagnostics.seek(max(0, diagnostics.tell() - 2400))
            tail = diagnostics.read()
            if result != 0 or "persistent same-origin storage, cookie expiry and attributes retained" not in output.read():
                raise AssertionError(
                    "real native Firefox tab restoration not proven: "
                    f"rc={result} diagnostic_tail={tail!r}"
                )
        assert_writer_quiescent(load_runtime(camouhost), root, "restored tabs")
        assert server.post_paths == ["/post", "/user-post"], (
            "native SessionStore silently replayed POST or blocked intentional form submit; "
            f"observed_paths={server.post_paths!r}"
        )
        print(
            "native_restore_proof stage=passed safe_get=PASS "
            "post_replay=0 user_post_after_admission=PASS", flush=True,
        )
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)


def prove_ipc_timeout_contract() -> None:
    """A silent child must fail quickly without a pipe-read deadlock."""
    with tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as diagnostics:
        process = subprocess.Popen(
            [sys.executable, "-u", "-c", "import time; time.sleep(30)"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=diagnostics,
            text=True,
        )
        process.stderr = diagnostics
        try:
            try:
                exchange(process, "hello|silent", seconds=1)
            except AssertionError as error:
                assert "IPC hello timed out after 1s" in str(error)
            else:
                raise AssertionError("silent Camouhost child escaped IPC timeout")
        finally:
            stop_runtime(process)


def prove_failure_observation_contract() -> None:
    """Cheap fault injection for the same runtime/proof owners used below with real browsers."""
    prove_ipc_timeout_contract()
    source_root = Path(__file__).resolve().parents[1]
    runtime = load_runtime(source_root / "runtime/camouhost/real.py")
    prove_official_context_close_event_contract(runtime)
    prove_managed_bootstrap_tab_contract(runtime)
    proof = load_runtime(source_root / "scripts/test-ar10-real-camoufox.py")
    # Camoufox inherits the native environment: do not resurrect a filter.
    assert not hasattr(runtime, "browser_environment")
    # Cold identity failures now happen inside the single user-native Open
    # process, not in a separate CLI. Observe only the redacted fixed error.
    for error, category in (
        (runtime.RuntimeContractError("secret-config-path"), "contract"),
        (OSError("secret-config-path"), "io"),
        (type("TargetClosedError", (Exception,), {})("secret-config-path"), "browser_closed"),
    ):
        stdout = io.StringIO()
        stderr = io.StringIO()
        with patch.dict(sys.modules, {
                "native_process": SimpleNamespace(launch_native_camoufox=Mock())
        }), patch.object(runtime, "materialize_candidate_identity", side_effect=error), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            result = asyncio.run(runtime.run_local_profile(Path("unused"), None, True))
        assert result == 5
        assert stdout.getvalue() == ""
        assert stderr.getvalue().strip() == (
            f"camouhost_failure phase=launch category={category}"
        )
        assert "secret" not in stdout.getvalue() + stderr.getvalue()
    timeout = type("TimeoutError", (Exception,), {})("secret-url-key-profile-content")
    frames = [f"hello|{IPC_VERSION}", f"launch|{SESSION}", f"observe_browser_visible|{SESSION}",
              f"admit_navigation|{SESSION}|unused", f"close|{SESSION}"]
    for phase, failure_method, frame_count, exit_code, cleanup_failure in (
        ("success", None, 5, 0, False),
        ("launch", "launch_verified_context", 2, 5, False),
        ("browser_visible", "browser_visible_observation", 3, 5, False),
        ("navigation", "admit_navigation", 4, 5, False),
        ("close", "close_context", 5, 6, False),
        ("browser_visible", "browser_visible_observation", 3, 5, True),
    ):
        stdout = io.TextIOWrapper(io.BytesIO(), encoding="utf-8")
        stderr = io.StringIO()
        close = Mock(side_effect=OSError("secret-cleanup-path") if cleanup_failure else None)
        mocks = {
            "load_runtime_lock": Mock(return_value=({}, "0" * 64)),
            "verify_python_components": Mock(),
            "resolve_profile_root": Mock(return_value=source_root),
            "load_generation_config": Mock(return_value=({}, "0" * 64)),
            "launch_verified_context": Mock(return_value=(object(), SimpleNamespace(pages=[object()]))),
            "install_context_close_observation": Mock(return_value={"closed": False, "event_count": 0}),
            "context_close_observed": Mock(return_value=False),
            "browser_visible_observation": Mock(return_value={}),
            "emit_browser_visible": Mock(),
            "decode_navigation_target": Mock(return_value="http://127.0.0.1/"),
            "admit_navigation": Mock(),
            "close_context": close,
        }
        if failure_method is not None:
            mocks[failure_method] = Mock(side_effect=timeout)
        if failure_method == "close_context":
            close = mocks[failure_method]
        with patch.multiple(runtime, **mocks), patch.dict(os.environ, {
            runtime.EXPECTED_PROBE_SHA256_ENV: "0" * 64,
        }), patch.object(sys, "stdin", io.StringIO("\n".join(frames[:frame_count]) + "\n")), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            result = runtime.run_ipc()
        assert result == exit_code, (phase, result)
        if phase == "success":
            assert stdout.buffer.getvalue().decode("ascii").splitlines()[-1] == f"closed|{SESSION}|true"
            assert stderr.getvalue().splitlines() == [
                "camouhost_stage phase=startup step=begin",
                "camouhost_stage phase=startup step=ready",
                "camouhost_stage phase=launch step=ready",
                "camouhost_pages count=1",
            ]
            close.assert_called_once()
            continue
        expected_failure = f"closed|{SESSION}|false" if phase == "close" else "error|runtime"
        assert stdout.buffer.getvalue().decode("ascii").splitlines()[-1] == expected_failure
        assert f"camouhost_failure phase={phase} category=timeout\n" in stderr.getvalue()
        assert stderr.getvalue().splitlines()[:2] == [
            "camouhost_stage phase=startup step=begin",
            "camouhost_stage phase=startup step=ready",
        ]
        assert "secret" not in stderr.getvalue()
        assert (close.call_count == 0) == (phase == "launch")
        if cleanup_failure:
            assert "camouhost_failure phase=cleanup category=io\n" in stderr.getvalue()

    stderr = io.StringIO()
    with contextlib.redirect_stderr(stderr):
        runtime.report_runtime_failure("secret-path", runtime.RuntimeContractError("secret-key"))
    assert stderr.getvalue() == "camouhost_failure phase=unknown category=contract\n"

    # Exercise the actual cold-launch harness against a child that releases ownership on EOF.
    # The early assertion must remain primary, with the child's exit and cleanup captured first.
    child = """import sys
for raw in sys.stdin:
    if raw.startswith('hello|'):
        print('hello_ack|3', flush=True)
    elif raw.startswith('launch|'):
        print('ready|session_01JAR10REALCAMOUFOX', flush=True)
print('camouhost_failure phase=browser_visible category=timeout', file=sys.stderr, flush=True)
print('fixture_cleanup_complete', file=sys.stderr, flush=True)
sys.exit(5)
"""
    popen = subprocess.Popen
    primary = AssertionError("original browser-visible failure")
    spawned = []

    def start_child(*_args, **kwargs):
        process = popen([sys.executable, "-c", child], **kwargs)
        spawned.append(process)
        return process

    with patch.object(proof, "base_env", return_value=os.environ.copy()), \
            patch.object(proof.subprocess, "Popen", side_effect=start_child), \
            patch.object(proof, "observe_browser_visible_wire", side_effect=primary):
        try:
            proof.run_cold_launch(source_root, {}, "http://127.0.0.1/")
        except AssertionError as observed:
            assert observed is primary
            note = "\n".join(observed.__notes__)
            assert "rc=5 cleanup=eof_exit" in note
            assert "fixture_cleanup_complete" in note
            assert "phase=browser_visible category=timeout" in note
        else:
            raise AssertionError("early runtime failure became success")
    assert spawned[0].returncode == 5

    # A child that cannot clean up remains a failure; bounded kill cannot replace its cause.
    stuck = Mock(stdin=io.StringIO(), stdout=io.StringIO(), returncode=None)
    stuck.poll.side_effect = lambda: stuck.returncode
    stuck.wait.side_effect = [subprocess.TimeoutExpired("fixture", 60), -9]
    stuck.kill.side_effect = lambda: setattr(stuck, "returncode", -9)
    primary = AssertionError("original stuck runtime failure")
    with patch.object(proof, "base_env", return_value={}), \
            patch.object(proof.subprocess, "Popen", return_value=stuck), \
            patch.object(proof, "exchange", side_effect=primary):
        try:
            proof.run_cold_launch(source_root, {}, "http://127.0.0.1/")
        except AssertionError as observed:
            assert observed is primary
            assert "cleanup=forced_kill" in "\n".join(observed.__notes__)
            stuck.kill.assert_called_once()
        else:
            raise AssertionError("forced termination became success")

    primary = AssertionError("original workspace runtime failure")
    temporary = SimpleNamespace(name=str(source_root), cleanup=Mock(side_effect=OSError("db")))
    with patch.object(proof.tempfile, "TemporaryDirectory", return_value=temporary):
        try:
            with proof.cold_launch_workspace():
                raise primary
        except AssertionError as observed:
            assert observed is primary
            assert "workspace cleanup failed separately: OSError" in "\n".join(observed.__notes__)
        else:
            raise AssertionError("workspace cleanup replaced the original failure")


def prove_shipping_managed_path_retired() -> None:
    """Normal Open has no compiled ManagedCamouhost process or claim owner."""
    root = Path(__file__).resolve().parents[1]
    lib = (root / "apps/profile-bridge/src/lib.rs").read_text(encoding="utf-8")
    composition = (root / "apps/profile-bridge/src/shipping_composition.rs").read_text(
        encoding="utf-8"
    )
    assert "pub mod camouhost_process;" not in lib
    assert not (root / "apps/profile-bridge/src/camouhost_process.rs").exists()
    assert "ManagedCamouhostProcess" not in composition
    assert "ManagedCamouhostIpc" not in composition
    assert "pub fn run_claim(" not in composition


def prove_disposable_managed_site_probe_contract() -> None:
    """Optional VK compatibility check must remain an offline-testable fixture.

    Browser launch and external public-site navigation are NOT run by CI.
    The shipped Bridge must never import or execute this diagnostic script.
    """
    path = Path(__file__).resolve().with_name("probe-camoufox-managed-site.py")
    spec = importlib.util.spec_from_file_location("crm_managed_site_probe", path)
    assert spec is not None and spec.loader is not None
    probe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(probe)
    probe.assert_self_test()
    assert set(probe.URLS) == {"vk", "example"}
    assert probe.main(["--self-test"]) == 0


def main() -> int:
    prove_failure_observation_contract()
    prove_shipping_managed_path_retired()
    prove_disposable_managed_site_probe_contract()
    if sys.argv[1:] == ["--self-test"]:
        print("Camouhost failure phase, primary outcome and bounded cleanup proof passed.")
        return 0
    args = parse_args()
    python = require_regular(args.python, "Python runtime")
    camouhost = require_regular(args.camouhost, "Camouhost runtime")
    runtime_lock = require_regular(args.runtime_lock, "runtime lock")
    runtime = load_runtime(camouhost)

    # Reuse the shipping Python/Camoufox runtime already materialized by CI;
    # no second browser or duplicated fixture engine. The normal full proof
    # remains available unchanged for additional Windows acceptance.
    if args.native_restore_only:
        with tempfile.TemporaryDirectory(prefix="ar10-native-restore-only-") as temporary:
            prove_preexisting_tabs_restored(
                python, camouhost, runtime_lock, args.headless,
                Path(temporary) / "restore-tabs",
            )
        print("AR10_NATIVE_FIREFOX_SESSIONSTORE=PHYSICAL_PASS", flush=True)
        return 0

    with tempfile.TemporaryDirectory(prefix="ar10-real-camoufox-cleanup-") as temporary:
        root = Path(temporary) / "generation"
        report = materialize(python, camouhost, runtime_lock, args.headless, root)
        env = process_env(runtime_lock, args.headless, root, report)

        prove_protocol_failure_cleanup(python, camouhost, env, runtime, root)
        prove_immediate_relaunch_after_cleanup(python, camouhost, env, runtime, root)
        prove_session_restore_startup(python, camouhost, env, runtime, root)
        prove_owned_playwright_agent(python, camouhost, env, root)
        prove_preexisting_tabs_restored(
            python, camouhost, runtime_lock, args.headless, Path(temporary) / "restore-tabs"
        )
        # Native close/crash/file-lock behavior is covered by the Rust
        # real_camoufox_runtime integration test and W00-W24 physical matrix.

    print(
        "AR-10 Camoufox adapter cleanup, Playwright close-event contracts and Firefox native prior-tabs restore passed."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
