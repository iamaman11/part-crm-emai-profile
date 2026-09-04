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
import importlib.util
import json
import os
import subprocess
import tempfile
from pathlib import Path
from types import ModuleType

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


def main() -> int:
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
