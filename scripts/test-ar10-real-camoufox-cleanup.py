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
import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
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
    return subprocess.Popen(
        [str(python), str(camouhost)],
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )


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


def stderr_tail(process: subprocess.Popen[str]) -> str:
    return process.stderr.read()[-3000:] if process.stderr is not None else ""


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
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)
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
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)
    assert_writer_quiescent(runtime, root, "post-launch EOF")


def prove_failure_observation_contract() -> None:
    """Cheap fault injection for the same runtime/proof owners used below with real browsers."""
    source_root = Path(__file__).resolve().parents[1]
    runtime = load_runtime(source_root / "runtime/camouhost/real.py")
    proof = load_runtime(source_root / "scripts/test-ar10-real-camoufox.py")
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
            assert stderr.getvalue() == ""
            close.assert_called_once()
            continue
        expected_failure = f"closed|{SESSION}|false" if phase == "close" else "error|runtime"
        assert stdout.buffer.getvalue().decode("ascii").splitlines()[-1] == expected_failure
        assert f"camouhost_failure phase={phase} category=timeout\n" in stderr.getvalue()
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
            patch.object(proof, "complete_pre_navigation_protocol", side_effect=primary):
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

    print(
        "AR-10 real Camoufox post-launch protocol/EOF cleanup and immediate relaunch evidence passed."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
