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


def prove_controlled_close_dispatch_contract(runtime: ModuleType) -> None:
    """Regression: stdin observer must pump real registered callbacks, not infer process death."""

    class Page:
        def __init__(self) -> None:
            self.listeners: dict[str, object] = {}

        def on(self, name: str, listener: object) -> None:
            self.listeners[name] = listener

        def fire(self, name: str) -> None:
            self.listeners[name](self)

    class Context:
        def __init__(self, pages: list[Page]) -> None:
            self.pages = pages
            self.listeners: dict[str, object] = {}
            self.next_observation = lambda: None
            self.pumps = 0

        def on(self, name: str, listener: object) -> None:
            self.listeners[name] = listener

        def wait_for_event(self, event: str, timeout: int) -> None:
            assert event == "page" and 0 < timeout <= 250
            self.pumps += 1
            self.next_observation()
            raise TimeoutError("expected bounded callback dispatch")

    first, second = Page(), Page()
    context = Context([first, second])
    witness = runtime.install_controlled_close_observation(context)
    assert not runtime.observe_controlled_close(context, witness)
    context.next_observation = lambda: first.fire("close")
    assert not runtime.observe_controlled_close(context, witness)
    context.next_observation = lambda: second.fire("close")
    assert runtime.observe_controlled_close(context, witness)
    assert context.pumps == 3

    class AbnormalBrowser:
        def safely_exited(self) -> bool:
            return False

    exited_without_normal_status = Page()
    context = Context([exited_without_normal_status])
    witness = runtime.install_controlled_close_observation(context)
    witness["browser_exit"] = AbnormalBrowser()
    context.next_observation = lambda: exited_without_normal_status.fire("close")
    assert not runtime.observe_controlled_close(context, witness), (
        "browser process failure was accepted as a controlled page-close"
    )

    crashed = Page()
    context = Context([crashed])
    witness = runtime.install_controlled_close_observation(context)
    context.next_observation = lambda: (crashed.fire("crash"), crashed.fire("close"))
    assert not runtime.observe_controlled_close(context, witness)

    unexpected = Page()
    context = Context([unexpected])
    witness = runtime.install_controlled_close_observation(context)

    def disconnect_without_close() -> None:
        raise RuntimeError("no page close witness")

    context.next_observation = disconnect_without_close
    try:
        runtime.observe_controlled_close(context, witness)
    except RuntimeError:
        pass
    else:
        raise AssertionError("browser disconnection without callback became controlled close")


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
    (root / ".profile-platform.lock").write_text(
        BRIDGE_LOCK, encoding="utf-8", newline="\n"
    )
    completed = subprocess.run(
        [str(python), str(camouhost), "--materialize-identity", str(root)],
        env=runtime_env(runtime_lock, headless),
        text=True,
        capture_output=True,
        timeout=300,
        check=False,
    )
    if completed.returncode != 0:
        raise AssertionError(
            "candidate identity materialization failed: "
            f"rc={completed.returncode} err={completed.stderr[-3000:]!r}"
        )
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise AssertionError("materialization report is not JSON") from error
    required = {
        "fingerprint_config_sha256",
        "fingerprint_policy_version",
        "profile_stable_probe_sha256",
        "runtime_lock_sha256",
    }
    if set(report) != required or any(
        not isinstance(value, str) or not value for value in report.values()
    ):
        raise AssertionError(f"unexpected materialization report: {report}")
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
            "CAMOUHOST_EXPECTED_PROBE_SHA256": report["profile_stable_probe_sha256"],
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
    try:
        process = subprocess.Popen(
            [str(python), str(camouhost)],
            env=env,
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


def assert_writer_quiescent(runtime: ModuleType, root: Path, label: str) -> None:
    if runtime.firefox_writer_active(root):
        raise AssertionError(f"Firefox writer remained active after {label}")


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
            self.send_header("Content-Length", str(len(page)))
            self.end_headers()
            self.wfile.write(page)

        def log_message(self, _format: str, *_args: object) -> None:
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
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
prefs = root / "user_data" / "user.js"
with prefs.open("a", encoding="utf-8", newline="\n") as stream:
    stream.write('\nuser_pref("browser.startup.page", 3);\n')
urls = [origin + "/prior-first", origin + "/prior-second"]

def enter():
    manager = Camoufox(**runtime.camoufox_kwargs(lock, root, config))
    with contextlib.redirect_stdout(sys.stderr):
        context = manager.__enter__()
    context.set_offline(True)
    return manager, context

manager, context = enter()
try:
    context.set_offline(False)
    first = context.pages[0] if context.pages else context.new_page()
    first.goto(urls[0], wait_until="domcontentloaded", timeout=15000)
    first.evaluate("localStorage.setItem('restore-proof', 'retained')")
    second = context.new_page()
    second.goto(urls[1], wait_until="domcontentloaded", timeout=15000)
    assert first.url == urls[0] and second.url == urls[1]

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
    runtime.close_context(manager, context, root)

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
    retained = present[urls[0]].evaluate("localStorage.getItem('restore-proof')")
    if retained != "retained":
        raise AssertionError("restored tab lost persistent origin state")
    witness = runtime.install_controlled_close_observation(context)
    assert not runtime.controlled_close_candidate(witness)
    present[urls[0]].close()
    assert not runtime.controlled_close_candidate(witness), (
        "closing one restored tab falsely acknowledged complete user close"
    )
    # The other previously restored tab remains live and managed.
    assert not present[urls[1]].is_closed()
finally:
    runtime.close_context(manager, context, root)

# INVESTIGATE-04: a tab intentionally closed before a browser-wide Quit
# must not be conflated with lost origin storage. Observe first, with no
# synthetic tab/navigation, then read the SAME origin from the surviving tab.
manager, context = enter()
try:
    initial_third_urls = [page.url for page in context.pages]
    deadline = time.monotonic() + 20
    while urls[1] not in {page.url for page in context.pages}:
        if time.monotonic() >= deadline:
            raise AssertionError(
                "surviving original-origin tab failed native SessionStore reopen: "
                f"initial={initial_third_urls!r} actual={[page.url for page in context.pages]!r}"
            )
        try:
            context.wait_for_event("page", timeout=500)
        except PlaywrightTimeoutError:
            pass
    present_third = {page.url: page for page in context.pages}
    assert urls[0] not in present_third, (
        "intentionally closed tab unexpectedly reopened; classify exact native Quit semantics"
    )
    assert present_third[urls[1]].evaluate(
        "localStorage.getItem('restore-proof')"
    ) == "retained", "closing a tab did not preserve its persistent origin storage"
    cookies_after_selective_close = {
        item["name"]: item["value"] for item in context.cookies([origin])
        if item["name"] in expected_cookies
    }
    assert cookies_after_selective_close == expected_cookies, (
        "closing a window/tab unexpectedly removed persistent origin cookies"
    )
finally:
    runtime.close_context(manager, context, root)
print(
    "two previous tabs restored; intentionally closed tab remains closed; "
    "persistent same-origin storage and cookies retained", flush=True
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
            if result != 0 or "persistent same-origin storage and cookies retained" not in output.read():
                raise AssertionError(
                    "real native Firefox tab restoration not proven: "
                    f"rc={result} diagnostic_tail={tail!r}"
                )
        assert_writer_quiescent(load_runtime(camouhost), root, "restored tabs")
        print("native_restore_proof stage=passed", flush=True)
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)


def prove_external_windows_gui_close(
    python: Path,
    camouhost: Path,
    env: dict[str, str],
    runtime: ModuleType,
    root: Path,
) -> None:
    """Post WM_CLOSE to a unique real top-level browser window, never call Playwright close.

    This is a hosted Windows fixture with a disposable profile; it does not simulate
    server-side save/commit or replace installed-device B7 evidence.
    """
    if os.name != "nt":
        return

    title = f"AR10 External Controlled Close {os.getpid()}"
    content = (f"<html><head><title>{title}</title></head>"
               "<body>Close this disposable Camoufox test window</body></html>").encode("utf-8")

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(content)))
            self.end_headers()
            self.wfile.write(content)

        def log_message(self, _format: str, *_args: object) -> None:
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    process = start_runtime(python, camouhost, env)

    from ctypes import wintypes

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    enum_windows = user32.EnumWindows
    enum_windows.argtypes = [ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM),
                             wintypes.LPARAM]
    enum_windows.restype = wintypes.BOOL
    get_text_length = user32.GetWindowTextLengthW
    get_text_length.argtypes = [wintypes.HWND]
    get_text_length.restype = ctypes.c_int
    get_text = user32.GetWindowTextW
    get_text.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    get_text.restype = ctypes.c_int
    visible = user32.IsWindowVisible
    visible.argtypes = [wintypes.HWND]
    visible.restype = wintypes.BOOL
    post_message = user32.PostMessageW
    post_message.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    post_message.restype = wintypes.BOOL

    def matching_windows() -> list[int]:
        found: list[int] = []

        @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        def visit(handle: int, _user_data: int) -> bool:
            if not visible(handle):
                return True
            count = get_text_length(handle)
            if count == 0:
                return True
            buffer = ctypes.create_unicode_buffer(count + 1)
            if get_text(handle, buffer, len(buffer)) > 0 and title in buffer.value:
                found.append(handle)
            return True

        if not enum_windows(visit, 0):
            raise AssertionError("could not enumerate real Windows browser windows")
        return found

    normal_close_proved = False
    try:
        if exchange(process, f"hello|{IPC_VERSION}") != f"hello_ack|{IPC_VERSION}":
            raise AssertionError("managed IPC negotiation failed")
        if exchange(process, f"launch|{SESSION}") != f"ready|{SESSION}":
            raise AssertionError("packaged Camoufox launch did not reach ready")
        response = exchange(process, f"observe_browser_visible|{SESSION}")
        if not response.startswith(f"browser_visible|{SESSION}|"):
            raise AssertionError("real pre-navigation browser visibility proof failed")
        url = f"http://127.0.0.1:{server.server_port}/"
        if exchange(process, f"admit_navigation|{SESSION}|{url.encode('utf-8').hex()}") != (
            f"navigation_admitted|{SESSION}"
        ):
            raise AssertionError("test navigation admission failed")
        if exchange(process, f"observe_close|{SESSION}") != f"close_observed|{SESSION}|false":
            raise AssertionError("open browser falsely reported a controlled close")

        deadline = time.monotonic() + 30
        handles: list[int] = []
        while time.monotonic() < deadline:
            handles = matching_windows()
            if handles:
                break
            time.sleep(0.2)
        if len(handles) != 1:
            raise AssertionError("no unique, real disposable browser window identified")
        if not post_message(handles[0], 0x0010, 0, 0):
            raise AssertionError("external WM_CLOSE dispatch failed")

        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            response = exchange(process, f"observe_close|{SESSION}")
            if response == f"close_observed|{SESSION}|true":
                normal_close_proved = True
                break
            if response != f"close_observed|{SESSION}|false":
                raise AssertionError(f"unexpected GUI-close IPC result: {response[:120]!r}")
            time.sleep(0.2)
        if not normal_close_proved:
            raise AssertionError("external WM_CLOSE did not produce positive callback witness")
        if exchange(process, f"close|{SESSION}") != f"closed|{SESSION}|true":
            raise AssertionError("controlled close did not release the real browser writer")
        if process.wait(timeout=90) != 0:
            raise AssertionError("real Camouhost controlled close exited with failure")
    finally:
        server.shutdown()
        server.server_close()
        server_thread.join(timeout=5)
        stop_runtime(process)
    if not normal_close_proved:
        raise AssertionError("external GUI close was never observed")
    assert_writer_quiescent(runtime, root, "external native WM_CLOSE")



class DisposableWindowsProcessJob:
    """Kernel-owned lifetime for the CI fixture only; no shipping process supervisor.

    Unlike a PID sweep, Job Object kill-on-close covers grandchildren even
    after a parent has exited, and never mistakes a reused PID for ownership.
    """

    def __init__(self, pid: int) -> None:
        from ctypes import wintypes

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel = kernel
        self.close_handle = kernel.CloseHandle
        self.close_handle.argtypes = [wintypes.HANDLE]
        self.close_handle.restype = wintypes.BOOL

        class BasicLimits(ctypes.Structure):
            _fields_ = [
                ("PerProcessUserTimeLimit", ctypes.c_longlong),
                ("PerJobUserTimeLimit", ctypes.c_longlong),
                ("LimitFlags", wintypes.DWORD),
                ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t),
                ("ActiveProcessLimit", wintypes.DWORD),
                ("Affinity", ctypes.c_size_t),
                ("PriorityClass", wintypes.DWORD),
                ("SchedulingClass", wintypes.DWORD),
            ]

        class IoCounters(ctypes.Structure):
            _fields_ = [
                (field, ctypes.c_ulonglong)
                for field in (
                    "ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
                    "ReadTransferCount", "WriteTransferCount", "OtherTransferCount",
                )
            ]

        class ExtendedLimits(ctypes.Structure):
            _fields_ = [
                ("BasicLimitInformation", BasicLimits),
                ("IoInfo", IoCounters),
                ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t),
                ("PeakJobMemoryUsed", ctypes.c_size_t),
            ]

        create = kernel.CreateJobObjectW
        create.argtypes = [wintypes.LPVOID, wintypes.LPCWSTR]
        create.restype = wintypes.HANDLE
        set_info = kernel.SetInformationJobObject
        set_info.argtypes = [wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID, wintypes.DWORD]
        set_info.restype = wintypes.BOOL
        assign = kernel.AssignProcessToJobObject
        assign.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        assign.restype = wintypes.BOOL
        open_process = kernel.OpenProcess
        open_process.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        open_process.restype = wintypes.HANDLE

        self.handle = create(None, None)
        if not self.handle:
            raise AssertionError(f"Windows disposable Job Object unavailable: {ctypes.get_last_error()}")
        try:
            limits = ExtendedLimits()
            limits.BasicLimitInformation.LimitFlags = 0x00002000  # KILL_ON_JOB_CLOSE
            if not set_info(self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
                raise AssertionError(f"Windows fixture Job limit rejected: {ctypes.get_last_error()}")
            process = open_process(0x00000101, False, pid)  # SET_QUOTA | TERMINATE
            if not process:
                raise AssertionError(f"Windows fixture child handle unavailable: {ctypes.get_last_error()}")
            try:
                if not assign(self.handle, process):
                    raise AssertionError(f"Windows fixture Job assign rejected: {ctypes.get_last_error()}")
            finally:
                self.close_handle(process)
        except BaseException:
            self.close()
            raise

    def close(self) -> None:
        handle = getattr(self, "handle", None)
        if handle:
            self.handle = None
            if not self.close_handle(handle):
                raise AssertionError(f"Windows disposable Job close failed: {ctypes.get_last_error()}")


@contextlib.contextmanager
def crash_storage_fixture(env: dict[str, str]):
    """Disposably serve one real origin; use only the owning Camouhost agent."""
    class PageHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            body = b"<!doctype html><title>same generation crash storage proof</title>"
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, _format: str, *_args: object) -> None:
            pass

    with tempfile.TemporaryDirectory(prefix="ar10-crash-storage-") as directory:
        with ThreadingHTTPServer(("127.0.0.1", 0), PageHandler) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            origin = f"http://127.0.0.1:{server.server_address[1]}"
            try:
                yield (
                    dict(env, CAMOUHOST_AGENT_DIRECTORY=str(Path(directory).resolve())),
                    Path(directory),
                    origin,
                )
            finally:
                server.shutdown()
                thread.join(timeout=10)


_CRASH_STORAGE_JS = """
async ({mode}) => {
    const recent = "crash-dirty-local-v1";
    const baseline = "confirmed-native-close-v1";
    if (mode === "baseline") {
        document.cookie = "b7_baseline_cookie=" + baseline +
            "; Path=/; Max-Age=3600; SameSite=Lax";
        localStorage.setItem("b7_baseline_local", baseline);
    }
    if (mode === "write") {
        document.cookie = "b7_crash_cookie=" + recent +
            "; Path=/; Max-Age=3600; SameSite=Lax";
        localStorage.setItem("b7_crash_local", recent);
    }
    const req = indexedDB.open("b7-crash-storage", 1);
    const db = await new Promise((resolve, reject) => {
        req.onupgradeneeded = () => {
            if (mode === "read") {
                req.transaction.abort();
                return;
            }
            req.result.createObjectStore("samples");
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(new Error("indexeddb open failed or missing"));
    });
    let indexed, baselineIndexed;
    try {
        const state = await new Promise((resolve, reject) => {
            const txn = db.transaction("samples", mode === "read" ? "readonly" : "readwrite");
            const store = txn.objectStore("samples");
            if (mode === "baseline") store.put(baseline, "baseline");
            if (mode === "write") store.put(recent, "state");
            const get = store.get("state");
            const getBaseline = store.get("baseline");
            let observed, observedBaseline;
            get.onsuccess = () => { observed = get.result; };
            getBaseline.onsuccess = () => { observedBaseline = getBaseline.result; };
            txn.oncomplete = () => resolve({recent: observed, baseline: observedBaseline});
            txn.onerror = () => reject(new Error("indexeddb transaction failed"));
            txn.onabort = () => reject(new Error("indexeddb transaction aborted"));
        });
        indexed = state.recent;
        baselineIndexed = state.baseline;
    } finally {
        db.close();
    }
    return {
        cookie: document.cookie,
        local: localStorage.getItem("b7_crash_local"),
        indexed,
        baselineLocal: localStorage.getItem("b7_baseline_local"),
        baselineIndexed,
    };
}
"""


def crash_storage_agent_readback(
    agent_directory: Path, camouhost_pid: int, origin: str, phase: str,
) -> None:
    """Read original browser persistence; distinguish confirmed from volatile."""
    assert phase in ("baseline", "write", "read")
    source = (
        f"origin = {origin!r}\n"
        "initial_urls = [p.url for p in context.pages]\n"
        "page = context.pages[0] if context.pages else context.new_page()\n"
        "initial_cookies = {c['name']: c['value'] for c in context.cookies([origin])}\n"
        "page.goto(origin, wait_until='domcontentloaded')\n"
        f"observed = page.evaluate({_CRASH_STORAGE_JS!r}, {{'mode': {phase!r}}})\n"
        "assert observed['baselineLocal'] == 'confirmed-native-close-v1', observed\n"
        "assert observed['baselineIndexed'] == 'confirmed-native-close-v1', observed\n"
        "assert 'b7_baseline_cookie=confirmed-native-close-v1' in observed['cookie'], observed\n"
        "assert any(c['name'] == 'b7_baseline_cookie' and "
        "c['value'] == 'confirmed-native-close-v1' for c in context.cookies([origin]))\n"
        "if " + repr(phase) + " != 'baseline':\n"
        "    assert observed['indexed'] == 'crash-dirty-local-v1', observed\n"
        "    assert 'b7_crash_cookie=crash-dirty-local-v1' in observed['cookie'], observed\n"
        "if " + repr(phase) + " == 'write':\n"
        "    assert observed['local'] == 'crash-dirty-local-v1', observed\n"
        "if " + repr(phase) + " == 'read':\n"
        "    assert initial_cookies.get('b7_crash_cookie') == 'crash-dirty-local-v1'\n"
        # This is a documented native page-unload milestone, not an artificial
        # timer or external manipulation of SQLite or WAL.
        "if " + repr(phase) + " == 'write':\n"
        "    page.goto('about:blank', wait_until='load')\n"
        "result = {'phase': " + repr(phase) + ", 'initial_urls': initial_urls, "
        "'confirmed_native_close': True, "
        "'recent_cookie_durable': observed['indexed'] == 'crash-dirty-local-v1' "
        "and 'b7_crash_cookie=crash-dirty-local-v1' in observed['cookie'], "
        "'recent_local_durable': observed['local'] == 'crash-dirty-local-v1'}\n"
    )
    script = agent_directory / f"crash-storage-{phase}.py"
    script.write_text(source, encoding="utf-8")
    command = subprocess.run(
        [
            sys.executable, str(Path(__file__).with_name("camouhost-agent.py")),
            "--directory", str(agent_directory), "--pid", str(camouhost_pid),
            "--script", str(script), "--timeout", "90",
        ],
        capture_output=True, text=True, timeout=110,
    )
    assert command.returncode == 0, (
        f"same-owner real-browser storage {phase} failed: "
        f"status={command.returncode}; stderr_tail={command.stderr[-1000:]!r}"
    )
    assert '"confirmed_native_close": true' in command.stdout
    if phase == "read":
        recent = (
            "DURABLE"
            if '"recent_local_durable": true' in command.stdout
            else "NOT_DURABLY_CONFIRMED"
        )
        print(
            "native_windows_crash_storage stage=read "
            "confirmed_previous_close_cookie_local_indexed=PASS "
            "unpublished_cookie_indexed=PASS "
            f"unpublished_localStorage={recent}; no_false_saved=1",
            flush=True,
        )
    else:
        print(f"native_windows_crash_storage stage={phase} verified PASS", flush=True)


def prove_forced_windows_browser_termination_is_not_controlled(
    python: Path,
    camouhost: Path,
    env: dict[str, str],
    runtime: ModuleType,
    root: Path,
) -> None:
    """Main-PID crash must fail closed; native Job covers disposable descendants."""
    if os.name != "nt":
        return
    from ctypes import wintypes

    with crash_storage_fixture(env) as (crash_env, agent_base, origin):
        # First establish a REAL, previously confirmed Firefox working copy.
        # Only normal native Close confirms the browser's own flush lifecycle;
        # it is NOT a cloud Save or permission to discard unsynced evidence.
        baseline_process = start_runtime(python, camouhost, crash_env)
        try:
            baseline_job = DisposableWindowsProcessJob(baseline_process.pid)
        except BaseException:
            stop_runtime(baseline_process)
            raise
        try:
            assert exchange(baseline_process, f"hello|{IPC_VERSION}") == (
                f"hello_ack|{IPC_VERSION}"
            )
            assert exchange(baseline_process, f"launch|{SESSION}") == f"ready|{SESSION}"
            assert exchange(baseline_process, f"observe_browser_visible|{SESSION}").startswith(
                f"browser_visible|{SESSION}|"
            )
            assert exchange(baseline_process, f"admit_navigation|{SESSION}|") == (
                f"navigation_admitted|{SESSION}"
            )
            crash_storage_agent_readback(agent_base, baseline_process.pid, origin, "baseline")
            assert exchange(baseline_process, f"close|{SESSION}", 45) == (
                f"closed|{SESSION}|true"
            )
            assert baseline_process.wait(timeout=30) == 0
        finally:
            try:
                stop_runtime(baseline_process)
            finally:
                baseline_job.close()
        assert_writer_quiescent(runtime, root, "pre-crash confirmed native close")
        process = start_runtime(python, camouhost, crash_env)
        # Attach BEFORE browser launch. This OS kernel boundary, not a best-effort
        # PID scan, guarantees fixture cleanup if the browser root dies first.
        try:
            job = DisposableWindowsProcessJob(process.pid)
        except BaseException:
            stop_runtime(process)
            raise
        witness = None
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        open_process = kernel.OpenProcess
        open_process.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        open_process.restype = wintypes.HANDLE
        terminate = kernel.TerminateProcess
        terminate.argtypes = [wintypes.HANDLE, wintypes.UINT]
        terminate.restype = wintypes.BOOL
        wait = kernel.WaitForSingleObject
        wait.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        wait.restype = wintypes.DWORD
        close = kernel.CloseHandle
        close.argtypes = [wintypes.HANDLE]
        close.restype = wintypes.BOOL
        try:
            assert exchange(process, f"hello|{IPC_VERSION}") == f"hello_ack|{IPC_VERSION}"
            assert exchange(process, f"launch|{SESSION}") == f"ready|{SESSION}"
            assert exchange(process, f"observe_browser_visible|{SESSION}").startswith(
                f"browser_visible|{SESSION}|"
            )
            assert exchange(process, f"admit_navigation|{SESSION}|") == (
                f"navigation_admitted|{SESSION}"
            )
            crash_storage_agent_readback(agent_base, process.pid, origin, "write")
            witness = runtime.WindowsBrowserExitWitness(process.pid)
            assert len(witness.pids) == 1, "browser main process identity is ambiguous"
            pid = witness.pids[0]
            handle = open_process(0x00100001, False, pid)  # SYNCHRONIZE | TERMINATE
            assert handle, "cannot open exact browser-main handle for negative proof"
            try:
                assert terminate(handle, 19), "Windows refused main-PID fault injection"
                assert wait(handle, 12000) == 0, "browser-main death not observed"
            finally:
                close(handle)
            assert not witness.safely_exited(), "forced browser death looked like normal exit zero"
            observed = None
            for _ in range(4):
                try:
                    observed = exchange(process, f"observe_close|{SESSION}", 10)
                except AssertionError:
                    assert process.poll() is not None, "Camouhost stalled after browser main kill"
                    break
                assert observed != f"close_observed|{SESSION}|true", (
                    "forced browser-main termination was mistaken for controlled Close"
                )
                if observed == "error|runtime":
                    break
                time.sleep(0.2)
            assert observed != f"close_observed|{SESSION}|true"
        finally:
            try:
                stop_runtime(process)
            finally:
                # The Windows kernel, not a race-prone hand-coded PID walk, closes
                # all remaining Firefox/driver children of this disposable job.
                job.close()
                if witness is not None:
                    witness.close()

        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and runtime.firefox_writer_active(root):
            time.sleep(0.1)
        assert_writer_quiescent(runtime, root, "forced Windows browser main death")

        # FIX-01 native half of the crash recovery composition. Reuse the SAME
        # on-disk Firefox user_data after its main PID was killed; no fresh profile,
        # lock deletion, browser preference reset or alternate runtime permitted.
        restarted = start_runtime(python, camouhost, crash_env)
        try:
            # This disposable process has the same kernel-managed child-lifetime
            # guarantee as the fault-injection half. No PID sweeps or manual lock
            # deletion if the second launch fails partway through.
            restarted_job = DisposableWindowsProcessJob(restarted.pid)
        except BaseException:
            stop_runtime(restarted)
            raise
        try:
            assert exchange(restarted, f"hello|{IPC_VERSION}") == f"hello_ack|{IPC_VERSION}"
            assert exchange(restarted, f"launch|{SESSION}") == f"ready|{SESSION}", (
                "same Firefox workspace could not reach native Ready after main-PID death"
            )
            assert exchange(restarted, f"observe_browser_visible|{SESSION}").startswith(
                f"browser_visible|{SESSION}|"
            ), "native same-workspace post-crash browser-visible readiness not proven"
            assert exchange(restarted, f"admit_navigation|{SESSION}|") == (
                f"navigation_admitted|{SESSION}"
            )
            crash_storage_agent_readback(agent_base, restarted.pid, origin, "read")
            # The prior fixture erroneously called stop_runtime(), which KILLS
            # Camouhost and may orphan its Firefox processes while SQLite is
            # still open. This is normal controlled fixture teardown, not a
            # second crash. Require the real IPC close and observed child exit.
            assert exchange(restarted, f"close|{SESSION}", 45) == (
                f"closed|{SESSION}|true"
            ), "native post-crash reopen did not support ordinary close"
            assert restarted.wait(timeout=30) == 0
        finally:
            try:
                stop_runtime(restarted)
            finally:
                restarted_job.close()
        assert_writer_quiescent(runtime, root, "post-crash same-workspace native reopen")
        print(
            "native_windows_main_kill fails_closed_disposable_job_released_"
            "and_same_workspace_native_reopen_ready PASS",
            flush=True,
        )



def prove_three_independent_native_profiles_two_roundtrips(
    python: Path,
    camouhost: Path,
    runtime_lock: Path,
    headless: str,
    runtime: ModuleType,
    temporary: Path,
) -> None:
    """Hosted Windows packaged Camoufox: three isolated workspaces, two real native closes.

    This proves actual Firefox cookie/localStorage/IndexedDB persistence, with
    a first read-only observer on EACH Open. It deliberately does not fake a
    server Coordinator, R2 PUT/GET, a frontend claim, or cloud Saved.
    """
    with crash_storage_fixture(runtime_env(runtime_lock, headless)) as (agent_env, agent_base, origin):
        observer = Path(__file__).with_name("b7-playwright-observe.py")
        cli = Path(__file__).with_name("camouhost-agent.py")
        assert observer.is_file() and cli.is_file()
        for profile_index in range(3):
            profile_root = temporary / f"native-profile-{profile_index}"
            report = materialize(python, camouhost, runtime_lock, headless, profile_root)
            # Reuse the proven same-owner inbox root from the real Windows
            # browser-main crash fixture. An extra nested directory here is
            # not a separate browser profile and only adds a different ACL/
            # descriptor discovery path. Firefox user_data remains isolated.
            inbox = agent_base
            environment = process_env(runtime_lock, headless, profile_root, report)
            environment["CAMOUHOST_AGENT_DIRECTORY"] = agent_env["CAMOUHOST_AGENT_DIRECTORY"]
            previous = None

            # cycle 0 -> cycle 1 -> cycle 2: two clean native Close/Reopen
            # roundtrips for EACH of the three independent Firefox profiles.
            for cycle in range(3):
                process = start_runtime(python, camouhost, environment)
                # This disposable Windows test owns all descendants through
                # an existing kernel Job, including if a new assertion fails
                # before the controlled native Close is reached.
                try:
                    job = DisposableWindowsProcessJob(process.pid)
                except BaseException:
                    stop_runtime(process)
                    raise
                clean_close = False
                navigation_admitted = False
                try:
                    assert exchange(process, f"hello|{IPC_VERSION}") == f"hello_ack|{IPC_VERSION}"
                    assert exchange(process, f"launch|{SESSION}") == f"ready|{SESSION}"
                    assert exchange(process, f"observe_browser_visible|{SESSION}").startswith(
                        f"browser_visible|{SESSION}|"
                    )
                    assert exchange(process, f"admit_navigation|{SESSION}|") == (
                        f"navigation_admitted|{SESSION}"
                    )
                    navigation_admitted = True

                    # The real Camouhost must have published its own
                    # session-scoped descriptor BEFORE any test script. Never
                    # synthesize or repair it in a fixture; on failure include
                    # only safe process/inbox diagnostics.
                    descriptor = inbox / f"active-{process.pid}.json"
                    assert descriptor.is_file(), (
                        f"native profile={profile_index} cycle={cycle} "
                        f"missing owner descriptor; process_alive={process.poll() is None}; "
                        f"stderr_tail={stderr_tail(process)!r}"
                    )
                    descriptor_data = json.loads(descriptor.read_text(encoding="utf-8"))
                    assert descriptor_data["pid"] == process.pid and (
                        Path(descriptor_data["run"]).parent.samefile(inbox)
                    ), "native Camouhost descriptor does not belong to its selected inbox"

                    def same_owner(script: Path) -> str:
                        done = subprocess.run(
                            [sys.executable, str(cli), "--directory", str(inbox),
                             "--pid", str(process.pid), "--script", str(script),
                             "--timeout", "120"],
                            capture_output=True, text=True, timeout=135,
                        )
                        assert done.returncode == 0, (
                            f"native profile={profile_index} cycle={cycle} "
                            f"agent={script.name} exit={done.returncode} "
                            f"stderr_tail={done.stderr[-1200:]!r}"
                        )
                        return done.stdout

                    # The first operation in the actual context is read-only:
                    # no goto/new_page/close/marker write is permitted here.
                    first = same_owner(observer)
                    assert '"status": "ok"' in first, first[-1000:]

                    next_value = (
                        f"native-profile-{profile_index}-cycle-{cycle}"
                        if cycle < 2 else None
                    )
                    script = inbox / f"storage-cycle-{cycle}.py"
                    source = (
                        f"origin = {origin!r}\n"
                        f"previous = {previous!r}\n"
                        f"next_value = {next_value!r}\n"
                        "page = context.pages[0] if context.pages else context.new_page()\n"
                        "page.goto(origin, wait_until='domcontentloaded', timeout=30000)\n"
                        "observed = page.evaluate(" + repr(_TWO_CYCLE_STORAGE_JS) +
                        ", {'nextValue': next_value})\n"
                        "assert observed['priorLocal'] == previous, observed\n"
                        "assert observed['priorIndexed'] == previous, observed\n"
                        "assert (previous is None or 'b7_native_cycle=' + previous in observed['priorCookie']), observed\n"
                        "assert (previous is not None or 'b7_native_cycle=' not in observed['priorCookie']), observed\n"
                        "if next_value is not None:\n"
                        "    assert observed['afterLocal'] == next_value, observed\n"
                        "    assert observed['afterIndexed'] == next_value, observed\n"
                        "    assert 'b7_native_cycle=' + next_value in observed['afterCookie'], observed\n"
                        # INVESTIGATE-05: prove BOTH supported Playwright cookie
                        # injection forms, not only document.cookie. The
                        # resulting cookies must survive a clean native Close
                        # and a new managed browser process without re-injection.
                        "import time\n"
                        "expected_playwright_cookies = {\n"
                        "    'b7_playwright_url': 'url-injected-native',\n"
                        "    'b7_playwright_domain': 'domain-injected-native',\n"
                        "}\n"
                        "if " + repr(cycle) + " == 0:\n"
                        "    context.add_cookies([\n"
                        "        {'name': 'b7_playwright_url', 'value': 'url-injected-native',\n"
                        "         'url': origin, 'sameSite': 'Lax', 'expires': time.time() + 7200},\n"
                        "        {'name': 'b7_playwright_domain', 'value': 'domain-injected-native',\n"
                        "         'domain': '127.0.0.1', 'path': '/', 'sameSite': 'Lax',\n"
                        "         'expires': time.time() + 7200},\n"
                        "    ])\n"
                        "native_cookies = {c['name']: c for c in context.cookies([origin])}\n"
                        "for name, value in expected_playwright_cookies.items():\n"
                        "    assert name in native_cookies, {'missing': name, 'cookies': list(native_cookies)}\n"
                        "    assert native_cookies[name]['value'] == value, native_cookies[name]\n"
                        "    assert native_cookies[name]['sameSite'] == 'Lax', native_cookies[name]\n"
                        "    assert native_cookies[name]['expires'] > time.time(), native_cookies[name]\n"
                        "    assert not native_cookies[name]['secure'], native_cookies[name]\n"
                        "    assert name + '=' + value in page.evaluate('document.cookie'), name\n"
                        "result = {'profile': " + repr(profile_index) +
                        ", 'cycle': " + repr(cycle) +
                        ", 'native_storage_roundtrip': True, 'playwright_cookie_reopen': True}\n"
                    )
                    script.write_text(source, encoding="utf-8")
                    actual = same_owner(script)
                    assert '"native_storage_roundtrip": true' in actual, actual[-1200:]
                    assert '"playwright_cookie_reopen": true' in actual, actual[-1200:]
                    assert exchange(process, f"close|{SESSION}", 45) == (
                        f"closed|{SESSION}|true"
                    ), "native controlled Close did not confirm Firefox shutdown"
                    assert process.wait(timeout=30) == 0
                    clean_close = True
                    previous = next_value if next_value is not None else previous
                finally:
                    if not clean_close and navigation_admitted and process.poll() is None:
                        # Best-effort *native* teardown on an assertion failure;
                        # preserve the primary failure and never claim Saved.
                        try:
                            if exchange(process, f"close|{SESSION}", 45) == (
                                f"closed|{SESSION}|true"
                            ):
                                process.wait(timeout=30)
                        except (AssertionError, OSError, subprocess.TimeoutExpired):
                            pass
                    try:
                        stop_runtime(process)
                        if clean_close:
                            # A normal Close must make the native Firefox
                            # writer quiescent BEFORE the disposable Job is
                            # closed. The test must not manufacture durability
                            # by silently killing a lingering browser.
                            deadline = time.monotonic() + 15
                            while runtime.firefox_writer_active(profile_root):
                                if time.monotonic() >= deadline:
                                    raise AssertionError(
                                        "controlled native Close left a live Firefox writer"
                                    )
                                time.sleep(0.1)
                    finally:
                        job.close()
                if clean_close:
                    assert_writer_quiescent(
                        runtime, profile_root,
                        f"profile {profile_index} native cycle {cycle}",
                    )
            print(
                f"native_three_profile_two_roundtrips profile={profile_index} "
                "close_reopen=2 cookie_localStorage_indexedDB=PASS",
                flush=True,
            )


_TWO_CYCLE_STORAGE_JS = r"""
async ({nextValue}) => {
    const open = indexedDB.open("b7-native-two-cycle", 1);
    open.onupgradeneeded = () => open.result.createObjectStore("samples");
    const database = await new Promise((resolve, reject) => {
        open.onsuccess = () => resolve(open.result);
        open.onerror = () => reject(new Error("native IndexedDB open failed"));
    });
    try {
        const transaction = database.transaction("samples", "readonly");
        const read = transaction.objectStore("samples").get("state");
        const priorIndexed = await new Promise((resolve, reject) => {
            read.onsuccess = () => resolve(read.result ?? null);
            read.onerror = () => reject(new Error("native IndexedDB read failed"));
        });
        const priorLocal = localStorage.getItem("b7_native_cycle");
        const priorCookie = document.cookie;
        if (nextValue !== null) {
            const write = database.transaction("samples", "readwrite");
            write.objectStore("samples").put(nextValue, "state");
            await new Promise((resolve, reject) => {
                write.oncomplete = resolve;
                write.onerror = () => reject(new Error("native IndexedDB write failed"));
                write.onabort = () => reject(new Error("native IndexedDB write aborted"));
            });
            localStorage.setItem("b7_native_cycle", nextValue);
            document.cookie = "b7_native_cycle=" + nextValue +
                "; Path=/; Max-Age=7200; SameSite=Lax";
        }
        const verification = database.transaction("samples", "readonly");
        const after = verification.objectStore("samples").get("state");
        const afterIndexed = await new Promise((resolve, reject) => {
            after.onsuccess = () => resolve(after.result ?? null);
            after.onerror = () => reject(new Error("native IndexedDB readback failed"));
        });
        return {
            priorLocal, priorIndexed, priorCookie,
            afterLocal: localStorage.getItem("b7_native_cycle"),
            afterIndexed, afterCookie: document.cookie,
        };
    } finally {
        database.close();
    }
}
"""


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
    prove_controlled_close_dispatch_contract(runtime)
    proof = load_runtime(source_root / "scripts/test-ar10-real-camoufox.py")
    proof.prove_managed_environment_projection(runtime)
    for error, category in (
        (runtime.RuntimeContractError("secret-config-path"), "contract"),
        (OSError("secret-config-path"), "io"),
        (type("TargetClosedError", (Exception,), {})("secret-config-path"), "browser_closed"),
    ):
        stdout = io.StringIO()
        stderr = io.StringIO()
        with patch.object(sys, "argv", ["real.py", "--materialize-identity", "unused"]), \
                patch.object(runtime, "materialize_candidate_identity", side_effect=error), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            result = runtime.main()
        report = json.loads(stdout.getvalue())
        assert result == 7 and report["failure"]["category"] == category
        assert isinstance(report["failure"]["line"], int)
        assert len(stdout.getvalue().splitlines()) == 1
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
            "firefox_writer_active": Mock(return_value=False),
            "load_generation_config": Mock(return_value=({}, "0" * 64)),
            "launch_verified_context": Mock(return_value=(object(), SimpleNamespace(pages=[object()]))),
            "install_controlled_close_observation": Mock(return_value={}),
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


def main() -> int:
    prove_failure_observation_contract()
    if sys.argv[1:] == ["--self-test"]:
        print("Camouhost failure phase, primary outcome and bounded cleanup proof passed.")
        return 0
    args = parse_args()
    python = require_regular(args.python, "Python runtime")
    camouhost = require_regular(args.camouhost, "Camouhost runtime")
    runtime_lock = require_regular(args.runtime_lock, "runtime lock")
    runtime = load_runtime(camouhost)

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
        if os.name == "nt" and args.headless == "false":
            prove_three_independent_native_profiles_two_roundtrips(
                python, camouhost, runtime_lock, args.headless, runtime,
                Path(temporary),
            )
            prove_external_windows_gui_close(python, camouhost, env, runtime, root)
            prove_forced_windows_browser_termination_is_not_controlled(
                python, camouhost, env, runtime, root
            )

    print(
        "AR-10 real Camoufox post-launch cleanup, Firefox native prior-tabs restore and close, immediate relaunch and GUI-close evidence passed."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
