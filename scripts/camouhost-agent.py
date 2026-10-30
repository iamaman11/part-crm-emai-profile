#!/usr/bin/env python3
"""Drive the Playwright BrowserContext already owned by an installed Camouhost.

Local, explicit test-mode command. There is no CDP listener, Firefox profile attach,
new browser writer, privileged cloud control call, or profile/content restriction.

Usage:
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --list
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --script test.py
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --pid 12345 --script test.py
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --pid 12345 --timeout 0 --script test.py

A test-mode Bridge must be launched with PROFILE_BRIDGE_AGENT_DIRECTORY pointing
to the pre-existing directory. Run it as the SAME interactive Windows account.
"""
from __future__ import annotations

import argparse
import json
import os
import secrets
import sys
import time
from pathlib import Path


def sessions(directory: Path) -> list[dict[str, object]]:
    active: list[dict[str, object]] = []
    for entry in sorted(directory.glob("active-*.json")):
        if entry.is_symlink() or not entry.is_file():
            continue
        try:
            details = json.loads(entry.read_text(encoding="utf-8"))
            run = Path(details["run"])
            if not run.is_absolute() or run.is_symlink() or not run.is_dir():
                continue
            if run.parent != directory:
                continue
            if not (run / "requests").is_dir() or not (run / "results").is_dir():
                continue
            active.append(details)
        except (OSError, ValueError, TypeError, KeyError):
            continue
    return active


def run() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--pid", type=int)
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--script", type=Path)
    parser.add_argument("--timeout", type=float, default=180.0,
                        help="Wait for completion in seconds; 0 means no time limit")
    arguments = parser.parse_args()
    directory = arguments.directory.resolve(strict=True)
    if not directory.is_dir() or directory.is_symlink():
        parser.error("agent directory must be an existing regular directory")
    available = sessions(directory)
    if arguments.list:
        for item in available:
            print(json.dumps(item, ensure_ascii=False))
        return 0
    if arguments.script is None or arguments.timeout < 0:
        parser.error("--script is required and --timeout must be nonnegative")
    candidates = [x for x in available
                  if arguments.pid is None or x.get("pid") == arguments.pid]
    if len(candidates) != 1:
        parser.error(f"expected one live Camouhost selection; found {len(candidates)}; use --pid")
    run_dir = Path(candidates[0]["run"])
    script = arguments.script.read_bytes()  # No artificial source/content cap for test profiles.
    job_id = secrets.token_hex(16)
    requests = run_dir / "requests"
    results = run_dir / "results"
    staging = requests / f".{job_id}.part"
    committed = requests / f"{job_id}.py"
    with staging.open("xb") as output:
        output.write(script)
        output.flush()
        os.fsync(output.fileno())
    os.replace(staging, committed)
    print(f"PLAYWRIGHT_AGENT_JOB={job_id}", flush=True)
    status = results / f"{job_id}.json"
    descriptor = directory / f"active-{candidates[0]['pid']}.json"
    deadline = None if arguments.timeout == 0 else time.monotonic() + arguments.timeout
    while not status.is_file():
        if not descriptor.is_file():
            print(
                "PLAYWRIGHT_AGENT_UNCONFIRMED: session ended without a result; "
                "do not replay this job automatically",
                file=sys.stderr,
            )
            return 125
        if deadline is not None and time.monotonic() >= deadline:
            claimed = requests / f"{job_id}.running"
            stage = "RUNNING" if claimed.exists() else "QUEUED"
            print(
                f"PLAYWRIGHT_AGENT_PENDING_{stage}: no confirmed result; "
                "do not replay automatically",
                file=sys.stderr,
            )
            return 124
        time.sleep(0.1)
    details = json.loads(status.read_text(encoding="utf-8"))
    for extension, target in (("stdout.txt", sys.stdout), ("stderr.txt", sys.stderr)):
        log = results / f"{job_id}.{extension}"
        if log.is_file():
            target.write(log.read_text(encoding="utf-8"))
    print(json.dumps(details, ensure_ascii=False, default=str), flush=True)
    return 0 if details.get("status") == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(run())
