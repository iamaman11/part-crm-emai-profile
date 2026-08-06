#!/usr/bin/env python3
"""Exercise the deterministic fake Camouhost subprocess contract."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FAKE_CAMOUHOST = ROOT / "runtime" / "camouhost" / "main.py"
SESSION_ID = "session_01JSTEP7RUNTIME"
PROFILE_MARKER = ".synthetic-profile-root"
PROFILE_MARKER_CONTENT = "synthetic-profile-v1\n"
ACTIVE_STATE = ".runtime-active.json"
CLEAN_STATE = ".runtime-closed-clean.json"


def create_profile_root(parent: Path, name: str) -> Path:
    profile = parent / name
    profile.mkdir()
    (profile / PROFILE_MARKER).write_text(PROFILE_MARKER_CONTENT, encoding="utf-8")
    return profile


def invoke(frames: str, profile_root: Path) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
    environment["CAMOUHOST_SYNTHETIC_PROFILE_ROOT"] = str(profile_root)
    return subprocess.run(
        [sys.executable, "-I", str(FAKE_CAMOUHOST)],
        input=frames,
        text=True,
        capture_output=True,
        check=False,
        timeout=10,
        env=environment,
    )


def read_state(path: Path) -> dict[str, str]:
    value = json.loads(path.read_text(encoding="utf-8"))
    assert isinstance(value, dict)
    assert set(value) == {"session_id", "state"}
    assert isinstance(value["session_id"], str)
    assert isinstance(value["state"], str)
    return value


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="step7-fake-camouhost-") as temporary:
        root = Path(temporary)

        clean_profile = create_profile_root(root, "clean-profile")
        lifecycle = invoke(
            f"hello|1\nlaunch|{SESSION_ID}\nclose|{SESSION_ID}\n",
            clean_profile,
        )
        assert lifecycle.returncode == 0, lifecycle
        assert lifecycle.stderr == ""
        assert lifecycle.stdout.splitlines() == [
            "hello_ack|1",
            f"ready|{SESSION_ID}",
            f"closed|{SESSION_ID}|true",
        ]
        assert not (clean_profile / ACTIVE_STATE).exists()
        assert read_state(clean_profile / CLEAN_STATE) == {
            "session_id": SESSION_ID,
            "state": "closed_clean",
        }

        launch_before_hello_profile = create_profile_root(root, "launch-before-hello")
        launch_before_hello = invoke(
            f"launch|{SESSION_ID}\n", launch_before_hello_profile
        )
        assert launch_before_hello.returncode == 2
        assert launch_before_hello.stdout == "error|protocol\n"
        assert not (launch_before_hello_profile / ACTIVE_STATE).exists()
        assert not (launch_before_hello_profile / CLEAN_STATE).exists()

        unsupported_profile = create_profile_root(root, "unsupported-version")
        unsupported_version = invoke("hello|2\n", unsupported_profile)
        assert unsupported_version.returncode == 2
        assert unsupported_version.stdout == "error|protocol\n"
        assert not (unsupported_profile / ACTIVE_STATE).exists()

        mismatch_profile = create_profile_root(root, "session-mismatch")
        session_mismatch = invoke(
            f"hello|1\nlaunch|{SESSION_ID}\nclose|session_02JSTEP7RUNTIME\n",
            mismatch_profile,
        )
        assert session_mismatch.returncode == 2
        assert session_mismatch.stdout.splitlines()[-1] == "error|protocol"
        assert read_state(mismatch_profile / ACTIVE_STATE) == {
            "session_id": SESSION_ID,
            "state": "active",
        }
        assert not (mismatch_profile / CLEAN_STATE).exists()

        eof_profile = create_profile_root(root, "premature-eof")
        premature_eof = invoke(f"hello|1\nlaunch|{SESSION_ID}\n", eof_profile)
        assert premature_eof.returncode == 3
        assert premature_eof.stdout.splitlines() == [
            "hello_ack|1",
            f"ready|{SESSION_ID}",
        ]
        assert read_state(eof_profile / ACTIVE_STATE) == {
            "session_id": SESSION_ID,
            "state": "active",
        }
        assert not (eof_profile / CLEAN_STATE).exists()

        malformed_profile = create_profile_root(root, "malformed")
        malformed = invoke("hello|1|extra\n", malformed_profile)
        assert malformed.returncode == 2
        assert malformed.stdout == "error|protocol\n"
        assert not (malformed_profile / ACTIVE_STATE).exists()

        nonempty_profile = create_profile_root(root, "nonempty")
        (nonempty_profile / "existing.txt").write_text("occupied", encoding="utf-8")
        nonempty = invoke("hello|1\n", nonempty_profile)
        assert nonempty.returncode == 4
        assert nonempty.stdout == "error|profile\n"

        missing_marker = root / "missing-marker"
        missing_marker.mkdir()
        missing = invoke("hello|1\n", missing_marker)
        assert missing.returncode == 4
        assert missing.stdout == "error|profile\n"

    print("Repository Step 7 fake Camouhost IPC and synthetic profile lifecycle passed.")


if __name__ == "__main__":
    main()
