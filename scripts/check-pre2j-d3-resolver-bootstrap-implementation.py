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
AR11_RELEASE_AUTHORITY = Path("architecture/release-architecture-ar11.json")
PROMOTION_WORKFLOW = Path(".github/workflows/mailbox-secret-resolver-promotion.yml")
PROMOTION_SCRIPT = Path("scripts/mailbox-secret-resolver-promotion.py")
CONTROL_PLANE_CONFIG = Path("deploy/cloudflare/wrangler.jsonc")
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


def predecessor_metadata() -> tuple[str, str]:
    authority = json.loads((ROOT / SUCCESSOR_AUTHORITY).read_text(encoding="utf-8"))
    predecessor = authority.get("predecessor")
    if not isinstance(predecessor, dict):
        raise ValueError("AR-8D successor predecessor metadata is missing")
    ref = predecessor.get("transition_base_main")
    workflow = predecessor.get("promotion_workflow")
    if not isinstance(ref, str) or not isinstance(workflow, str):
        raise ValueError("AR-8D successor predecessor workflow identity is malformed")
    return ref, workflow


def historical_file(ref: str, relative: Path | str) -> bytes:
    result = run(["git", "show", f"{ref}:{relative}"], capture=True)
    if result.returncode != 0 or not result.stdout:
        details = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError(
            f"governed historical D3 file is unavailable: {relative}"
            + (f": {details}" if details else "")
        )
    return result.stdout


def restore_optional_file(path: Path, current: bytes | None) -> None:
    if current is None:
        path.unlink(missing_ok=True)
    else:
        path.write_bytes(current)


def run_historical(args: list[str]) -> int:
    """Replay immutable D3 checks against the exact retired executable surface.

    AR-11 intentionally removes mailbox-only deployment authority from the current
    Core closure. Historical proof therefore materializes the exact predecessor
    promotion workflow, promotion helper and control-plane config only for the
    duration of the D3 check, then restores the current tree byte-for-byte.
    """

    ref, workflow = predecessor_metadata()
    promotion_path = ROOT / PROMOTION_WORKFLOW
    promotion_script_path = ROOT / PROMOTION_SCRIPT
    control_config_path = ROOT / CONTROL_PLANE_CONFIG

    current_promotion = promotion_path.read_bytes() if promotion_path.is_file() else None
    current_promotion_script = (
        promotion_script_path.read_bytes() if promotion_script_path.is_file() else None
    )
    current_control_config = control_config_path.read_bytes()

    predecessor_promotion = historical_file(ref, workflow)
    predecessor_promotion_script = historical_file(ref, PROMOTION_SCRIPT)
    predecessor_control_config = historical_file(ref, CONTROL_PLANE_CONFIG)

    try:
        promotion_path.parent.mkdir(parents=True, exist_ok=True)
        promotion_script_path.parent.mkdir(parents=True, exist_ok=True)
        promotion_path.write_bytes(predecessor_promotion)
        promotion_script_path.write_bytes(predecessor_promotion_script)
        control_config_path.write_bytes(predecessor_control_config)
        return run([sys.executable, str(HISTORICAL_CHECKER), *args]).returncode
    finally:
        restore_optional_file(promotion_path, current_promotion)
        restore_optional_file(promotion_script_path, current_promotion_script)
        control_config_path.write_bytes(current_control_config)


def main() -> int:
    args = sys.argv[1:]
    promotion_path = ROOT / PROMOTION_WORKFLOW
    promotion = promotion_path.read_text(encoding="utf-8") if promotion_path.is_file() else ""
    successor_active = (
        (ROOT / SUCCESSOR_AUTHORITY).is_file()
        and (not promotion_path.is_file() or HISTORICAL_MARKER not in promotion)
    )
    ar11_profile_aware_closure = (ROOT / AR11_RELEASE_AUTHORITY).is_file()

    if not successor_active:
        if ar11_profile_aware_closure:
            return run_historical(args)
        return run([sys.executable, str(HISTORICAL_CHECKER), *args]).returncode

    successor = run(
        ["node", SUCCESSOR_CHECKER.as_posix(), *successor_args(args)],
        capture=True,
    )
    if successor.returncode != 0:
        return fail_from(successor, "AR-8D successor policy")

    return run_historical(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"D3 -> AR-8D transition gate failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
