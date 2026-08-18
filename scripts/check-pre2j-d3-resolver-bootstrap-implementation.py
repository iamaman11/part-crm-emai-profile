#!/usr/bin/env python3
"""Governed AR-8D dispatcher for the immutable historical Pre-2J D3 implementation gate."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HISTORICAL_CHECKER = Path(
    "scripts/check-pre2j-d3-resolver-bootstrap-implementation-historical"
)
SUCCESSOR_AUTHORITY = Path("architecture/ar8-d-secret-transport-successor.json")
SUCCESSOR_CHECKER = Path(".github/scripts/ar8-d-secret-transport-successor.mjs")
PROMOTION_WORKFLOW = Path(".github/workflows/mailbox-secret-resolver-promotion.yml")
HISTORICAL_MARKER = "validate-secrets"


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        capture_output=capture,
    )


def fail_from(result: subprocess.CompletedProcess[bytes], label: str) -> int:
    if result.returncode == 0:
        return 0
    if result.stdout:
        sys.stdout.buffer.write(result.stdout)
    if result.stderr:
        sys.stderr.buffer.write(result.stderr)
    if not result.stdout and not result.stderr:
        print(f"{label} failed with exit code {result.returncode}", file=sys.stderr)
    return result.returncode


def successor_args(args: list[str]) -> list[str]:
    if "--self-test" in args:
        return ["--self-test"]
    if "--base-ref" in args:
        index = args.index("--base-ref")
        if index + 1 >= len(args):
            raise ValueError("--base-ref requires a value")
        return ["--base-ref", args[index + 1]]
    return []


def historical_workflow() -> bytes:
    authority = json.loads((ROOT / SUCCESSOR_AUTHORITY).read_text(encoding="utf-8"))
    predecessor = authority.get("predecessor")
    if not isinstance(predecessor, dict):
        raise ValueError("AR-8D successor predecessor metadata is missing")
    ref = predecessor.get("transition_base_main")
    workflow = predecessor.get("promotion_workflow")
    if not isinstance(ref, str) or not isinstance(workflow, str):
        raise ValueError("AR-8D successor predecessor workflow identity is malformed")
    result = run(["git", "show", f"{ref}:{workflow}"], capture=True)
    if result.returncode != 0 or not result.stdout:
        details = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError(
            "governed historical D3 promotion workflow is unavailable"
            + (f": {details}" if details else "")
        )
    return result.stdout


def main() -> int:
    args = sys.argv[1:]
    promotion_path = ROOT / PROMOTION_WORKFLOW
    promotion = promotion_path.read_text(encoding="utf-8")
    successor_active = (ROOT / SUCCESSOR_AUTHORITY).is_file() and HISTORICAL_MARKER not in promotion

    if not successor_active:
        return run([sys.executable, str(HISTORICAL_CHECKER), *args]).returncode

    successor = run(
        ["node", SUCCESSOR_CHECKER.as_posix(), *successor_args(args)],
        capture=True,
    )
    if successor.returncode != 0:
        return fail_from(successor, "AR-8D successor policy")

    current_workflow = promotion_path.read_bytes()
    predecessor_workflow = historical_workflow()
    try:
        promotion_path.write_bytes(predecessor_workflow)
        historical = run([sys.executable, str(HISTORICAL_CHECKER), *args])
        return historical.returncode
    finally:
        promotion_path.write_bytes(current_workflow)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"D3 -> AR-8D transition gate failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
