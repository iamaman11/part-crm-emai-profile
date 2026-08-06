#!/usr/bin/env python3
"""Deterministic fake Camouhost process for repository contract evidence only."""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

IPC_VERSION = "1"
MAX_FRAME_LENGTH = 512
SESSION_PATTERN = re.compile(r"[A-Za-z0-9_-]{8,96}\Z")
PROFILE_ROOT_ENV = "CAMOUHOST_SYNTHETIC_PROFILE_ROOT"
PROFILE_MARKER = ".synthetic-profile-root"
PROFILE_MARKER_CONTENT = "synthetic-profile-v1\n"
ACTIVE_STATE = ".runtime-active.json"
CLEAN_STATE = ".runtime-closed-clean.json"


def emit(frame: str) -> None:
    sys.stdout.write(frame + "\n")
    sys.stdout.flush()


def fail() -> int:
    emit("error|protocol")
    return 2


def profile_fail() -> int:
    emit("error|profile")
    return 4


def valid_frame(raw: str) -> bool:
    return (
        0 < len(raw) <= MAX_FRAME_LENGTH
        and "\r" not in raw
        and "\0" not in raw
        and raw.endswith("\n")
    )


def canonical_state(state: str, session_id: str) -> bytes:
    return (
        json.dumps(
            {"session_id": session_id, "state": state},
            ensure_ascii=True,
            separators=(",", ":"),
            sort_keys=True,
        )
        + "\n"
    ).encode("utf-8")


def synthetic_profile_root() -> Path:
    raw = os.environ.get(PROFILE_ROOT_ENV)
    if raw is None:
        raise ValueError("synthetic profile root is missing")
    candidate = Path(raw)
    if candidate.is_symlink() or not candidate.is_dir():
        raise ValueError("synthetic profile root is invalid")
    root = candidate.resolve(strict=True)
    marker = root / PROFILE_MARKER
    if marker.is_symlink() or not marker.is_file():
        raise ValueError("synthetic profile marker is missing")
    if marker.read_text(encoding="utf-8") != PROFILE_MARKER_CONTENT:
        raise ValueError("synthetic profile marker is invalid")
    if any(entry != marker for entry in root.iterdir()):
        raise ValueError("synthetic profile root is not empty")
    return root


def write_state(root: Path, name: str, state: str, session_id: str) -> Path:
    target = root / name
    temporary = root / f".{name}.tmp"
    if target.exists() or target.is_symlink() or temporary.exists() or temporary.is_symlink():
        raise ValueError("synthetic profile state already exists")
    with temporary.open("xb") as handle:
        handle.write(canonical_state(state, session_id))
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, target)
    return target


def run() -> int:
    try:
        profile_root = synthetic_profile_root()
    except (OSError, UnicodeError, ValueError):
        return profile_fail()

    negotiated = False
    active_session: str | None = None
    active_state: Path | None = None

    for raw in sys.stdin:
        if not valid_frame(raw):
            return fail()
        frame = raw[:-1]
        parts = frame.split("|")

        if parts == ["hello", IPC_VERSION] and not negotiated and active_session is None:
            negotiated = True
            emit(f"hello_ack|{IPC_VERSION}")
            continue

        if (
            len(parts) == 2
            and parts[0] == "launch"
            and negotiated
            and active_session is None
            and SESSION_PATTERN.fullmatch(parts[1]) is not None
        ):
            try:
                active_state = write_state(profile_root, ACTIVE_STATE, "active", parts[1])
            except (OSError, ValueError):
                return profile_fail()
            active_session = parts[1]
            emit(f"ready|{active_session}")
            continue

        if (
            len(parts) == 2
            and parts[0] == "close"
            and active_session is not None
            and parts[1] == active_session
            and active_state is not None
        ):
            try:
                write_state(profile_root, CLEAN_STATE, "closed_clean", active_session)
                active_state.unlink()
            except OSError:
                return profile_fail()
            emit(f"closed|{active_session}|true")
            return 0

        return fail()

    return 3 if negotiated or active_session is not None else 0


if __name__ == "__main__":
    raise SystemExit(run())
