#!/usr/bin/env python3
"""Exercise the deterministic fake Camouhost subprocess contract."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FAKE_CAMOUHOST = ROOT / "runtime" / "camouhost" / "main.py"
SESSION_ID = "session_01JSTEP7RUNTIME"


def invoke(frames: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-I", str(FAKE_CAMOUHOST)],
        input=frames,
        text=True,
        capture_output=True,
        check=False,
        timeout=10,
    )


def main() -> None:
    lifecycle = invoke(
        f"hello|1\nlaunch|{SESSION_ID}\nclose|{SESSION_ID}\n"
    )
    assert lifecycle.returncode == 0, lifecycle
    assert lifecycle.stderr == ""
    assert lifecycle.stdout.splitlines() == [
        "hello_ack|1",
        f"ready|{SESSION_ID}",
        f"closed|{SESSION_ID}|true",
    ]

    launch_before_hello = invoke(f"launch|{SESSION_ID}\n")
    assert launch_before_hello.returncode == 2
    assert launch_before_hello.stdout == "error|protocol\n"

    unsupported_version = invoke("hello|2\n")
    assert unsupported_version.returncode == 2
    assert unsupported_version.stdout == "error|protocol\n"

    session_mismatch = invoke(
        f"hello|1\nlaunch|{SESSION_ID}\nclose|session_02JSTEP7RUNTIME\n"
    )
    assert session_mismatch.returncode == 2
    assert session_mismatch.stdout.splitlines()[-1] == "error|protocol"

    premature_eof = invoke(f"hello|1\nlaunch|{SESSION_ID}\n")
    assert premature_eof.returncode == 3
    assert premature_eof.stdout.splitlines() == [
        "hello_ack|1",
        f"ready|{SESSION_ID}",
    ]

    malformed = invoke("hello|1|extra\n")
    assert malformed.returncode == 2
    assert malformed.stdout == "error|protocol\n"

    print("Repository Step 7 fake Camouhost IPC lifecycle passed.")


if __name__ == "__main__":
    main()
