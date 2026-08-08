#!/usr/bin/env python3
"""Fast, cross-platform pre-push verification for repository-local failures.

This intentionally runs only cheap deterministic checks. Full acceptance still belongs to
permanent GitHub Actions workflows on one unchanged exact head.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(label: str, command: list[str]) -> None:
    print(f"\n==> {label}")
    print("    " + " ".join(command))
    completed = subprocess.run(command, cwd=ROOT, check=False)
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--with-compile",
        action="store_true",
        help="also run the bounded native/WASM compile lane before push",
    )
    args = parser.parse_args()

    # Formatting is first because it is the cheapest high-fanout CI failure.
    run("Rust formatting", ["cargo", "fmt", "--all", "--", "--check"])

    scripts = [
        "check-architecture.py",
        "check-contract-compatibility.py",
        "check-d1-boundary.py",
        "check-step4-governed-writes.py",
        "check-step5-profile-coordinator.py",
        "check-step6-windows-bridge.py",
        "check-frontend-feature-boundaries.py",
    ]
    for script in scripts:
        run(script, [sys.executable, str(ROOT / "scripts" / script)])

    run(
        "generated frontend contract drift",
        [sys.executable, str(ROOT / "scripts" / "generate-frontend-contracts.py"), "--check"],
    )

    status_path = ROOT / "docs" / "status.json"
    print("\n==> docs/status.json syntax")
    with status_path.open("r", encoding="utf-8") as handle:
        json.load(handle)

    if args.with_compile:
        run(
            "Native workspace check",
            [
                "cargo",
                "check",
                "--locked",
                "--workspace",
                "--all-targets",
                "--exclude",
                "browser-profile-control-plane-worker",
                "--exclude",
                "cloudflare-adapters",
            ],
        )
        run(
            "Cloudflare adapter check",
            ["cargo", "check", "--locked", "-p", "cloudflare-adapters"],
        )
        run(
            "Worker native check",
            ["cargo", "check", "--locked", "-p", "browser-profile-control-plane-worker"],
        )
        run(
            "Pure crates WASM check",
            [
                "cargo",
                "check",
                "--locked",
                "--target",
                "wasm32-unknown-unknown",
                "-p",
                "profile-platform-primitives",
                "-p",
                "contracts",
                "-p",
                "control-plane-contract",
                "-p",
                "identity-access-domain",
                "-p",
                "client-domain",
                "-p",
                "profile-domain",
                "-p",
                "session-domain",
                "-p",
                "mailbox-domain",
                "-p",
                "bridge-domain",
                "-p",
                "application-ports",
                "-p",
                "use-cases",
            ],
        )

    print("\nFast preflight passed. Full exact-head permanent CI is still required.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
