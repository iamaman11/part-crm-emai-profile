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
    process.stdin.write(frame + "\n")
    process.stdin.flush()
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
finally:
    runtime.close_context(manager, context, root)

manager, context = enter()
try:
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
print("two previous Firefox tabs restored and tracked; partial close not complete", flush=True)
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
            if result != 0 or "two previous Firefox tabs restored and tracked" not in output.read():
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
        prove_preexisting_tabs_restored(
            python, camouhost, runtime_lock, args.headless, Path(temporary) / "restore-tabs"
        )
        if os.name == "nt" and args.headless == "false":
            prove_external_windows_gui_close(python, camouhost, env, runtime, root)

    print(
        "AR-10 real Camoufox post-launch cleanup, Firefox native prior-tabs restore and close, immediate relaunch and GUI-close evidence passed."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
