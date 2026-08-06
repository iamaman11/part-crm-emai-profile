#!/usr/bin/env python3
"""Deterministic fake Camouhost process for repository contract evidence only."""

from __future__ import annotations

import re
import sys

IPC_VERSION = "1"
MAX_FRAME_LENGTH = 512
SESSION_PATTERN = re.compile(r"[A-Za-z0-9_-]{8,96}\Z")


def emit(frame: str) -> None:
    sys.stdout.write(frame + "\n")
    sys.stdout.flush()


def fail() -> int:
    emit("error|protocol")
    return 2


def valid_frame(raw: str) -> bool:
    return (
        0 < len(raw) <= MAX_FRAME_LENGTH
        and "\r" not in raw
        and "\0" not in raw
        and raw.endswith("\n")
    )


def run() -> int:
    negotiated = False
    active_session: str | None = None

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
            active_session = parts[1]
            emit(f"ready|{active_session}")
            continue

        if (
            len(parts) == 2
            and parts[0] == "close"
            and active_session is not None
            and parts[1] == active_session
        ):
            emit(f"closed|{active_session}|true")
            return 0

        return fail()

    return 3 if negotiated or active_session is not None else 0


if __name__ == "__main__":
    raise SystemExit(run())
