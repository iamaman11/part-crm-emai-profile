#!/usr/bin/env python3
"""Enforce permanent Repository Step 6 Windows Bridge feasibility boundaries."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPOSITORY_REQUIRED = {
    "crates/bridge-domain/src/lib.rs": (
        'strip_prefix("profilebridge://claim/")',
        "ClaimCode([REDACTED])",
        "pub struct EnrollmentClaim",
        "DeviceRebindRejected",
        "pub struct WorkspaceLockState",
        "WriterAlreadyActive",
        "pub struct ProcessSupervisor",
        "ProcessCloseOutcome::ForcedTimeout",
        "pub enum CamouhostMessage",
        "CAMOUHOST_IPC_VERSION",
        "fn malformed_claim_uris_fail_closed",
        "fn second_workspace_writer_is_rejected",
        "fn graceful_close_and_forced_timeout_are_distinct",
    ),
    "apps/profile-bridge/src/main.rs": (
        "ClaimUri::parse",
        'Ok("claim-uri-accepted")',
        "InvalidClaimUri",
    ),
    "apps/profile-bridge/src/lib.rs": (
        "pub struct FakeDeviceIdentity",
        "pub struct FakeDeviceKeyStore",
        "pub struct FakeCamouhost",
        "requires_version_negotiation",
    ),
    "migrations/bridge/0001_local_state.sql": (
        "CREATE TABLE bridge_commands",
        "CREATE TABLE bridge_outbox",
        "bridge_command_stale_version",
        "bridge_command_reordered",
        "bridge_command_append_only",
        "bridge_outbox_payload_immutable",
    ),
    "scripts/test-step6-bridge-local.py": (
        "bridge_command_conflict",
        "bridge_command_stale_version",
        "bridge_command_reordered",
        "bridge_outbox_payload_immutable",
    ),
}

FORBIDDEN_PURE_MARKERS = (
    "std::fs",
    "std::process::Command",
    "windows::",
    "windows_sys::",
    "rusqlite",
)

DELETION_MARKERS = (
    "remove_file",
    "remove_dir_all",
    "DeleteFile",
    ".unlink(",
)

BROWSER_LOCK_MARKERS = (
    "parent.lock",
    ".parentlock",
    "SingletonLock",
)

FIXTURE_PREFIX = "tests/windows-bridge/fixtures/"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    return parser.parse_args()


def relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def main() -> int:
    root = parse_args().root.resolve()
    repository_root = (root / "Cargo.toml").exists()
    errors: list[str] = []

    pure_path = root / "crates" / "bridge-domain" / "src" / "lib.rs"
    if pure_path.exists():
        pure = pure_path.read_text(encoding="utf-8")
        for marker in FORBIDDEN_PURE_MARKERS:
            if marker in pure:
                errors.append(f"provider/runtime API escaped into Bridge domain: {marker}")

    for path in root.rglob("*"):
        if not path.is_file() or path.suffix not in {".rs", ".py"}:
            continue
        rel = relative(root, path)
        if repository_root and rel.startswith(FIXTURE_PREFIX):
            continue
        text = path.read_text(encoding="utf-8")
        if any(marker in text for marker in DELETION_MARKERS) and any(
            marker in text for marker in BROWSER_LOCK_MARKERS
        ):
            errors.append(f"automatic browser runtime lock deletion is forbidden: {rel}")

    if repository_root:
        for rel, markers in REPOSITORY_REQUIRED.items():
            path = root / rel
            if not path.exists():
                errors.append(f"missing Step 6 boundary: {rel}")
                continue
            text = path.read_text(encoding="utf-8")
            for marker in markers:
                if marker not in text:
                    errors.append(f"missing Step 6 invariant in {rel}: {marker}")

        cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
        for member in ('"apps/profile-bridge"', '"crates/bridge-domain"'):
            if member not in cargo:
                errors.append(f"Step 6 workspace member missing: {member}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Repository Step 6 Windows Bridge boundaries are enforced.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
