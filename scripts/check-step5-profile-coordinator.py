#!/usr/bin/env python3
"""Enforce the permanent Repository Step 5 coordinator boundaries."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPOSITORY_REQUIRED = {
    "crates/session-domain/src/coordinator.rs": (
        "pub struct ProfileCoordinatorState",
        "CoordinatorCommand::Claim",
        "CoordinatorCommand::Heartbeat",
        "CoordinatorError::StaleWriter",
        "fn delayed_writer_is_rejected_after_turnover",
        "fn reordered_commands_and_key_reuse_are_rejected",
        "fn idle_timeout_preserves_uncertain_state_until_recovery",
    ),
    "crates/cloudflare-adapters/src/profile_coordinator.rs": (
        "pub struct StoredCoordinatorDocument",
        "pub fn replay",
        "JournalCapacityExceeded",
        "fn persisted_turnover_fences_old_writer",
    ),
    "crates/cloudflare-adapters/src/d1_profile_coordinator.rs": (
        "pub struct D1ProfileCoordinatorRepository",
        "profile_coordinator_projection_commands",
        "pub async fn projected_sequence",
    ),
    "apps/control-plane-worker/src/profile_coordinator.rs": (
        "resolve_active_request_actor",
        "find_visible_profile",
        "coordinator_object_name(&profile_id)",
        "generate_fencing_token",
        "D1ProfileCoordinatorRepository",
        "profile_is_coordinatable",
    ),
    "migrations/d1/0004_profile_coordinator_projection.sql": (
        "CREATE TABLE profile_coordinator_projection_commands",
        "CREATE TABLE profile_coordinator_projections",
        "profile_coordinator_projection_command_apply",
        "one_profile_coordinator_outbox_per_version",
    ),
    "docs/PROFILE_COORDINATOR.md": (
        "Durable Object storage is authoritative",
        "neutral disclosure response",
        "no cross-service transaction",
    ),
}

FORBIDDEN_COORDINATOR_AUTH_MARKERS = (
    "profile_client_assignments",
    "linked_client_id",
)

ALLOWED_DURABLE_OBJECT_FILES = {
    "apps/control-plane-worker/src/lib.rs",
    "apps/control-plane-worker/src/profile_coordinator.rs",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    return parser.parse_args()


def relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def main() -> int:
    root = parse_args().root.resolve()
    errors: list[str] = []

    for path in root.rglob("*.rs"):
        rel = relative(root, path)
        text = path.read_text(encoding="utf-8")
        if path.name == "profile_coordinator.rs":
            for marker in FORBIDDEN_COORDINATOR_AUTH_MARKERS:
                if marker in text:
                    errors.append(
                        f"assignment-derived coordinator authorization is forbidden: {rel}: {marker}"
                    )
        if "durable_object(" in text and rel not in ALLOWED_DURABLE_OBJECT_FILES:
            errors.append(f"raw Durable Object API escaped Worker composition: {rel}")

    if (root / "Cargo.toml").exists():
        for rel, markers in REPOSITORY_REQUIRED.items():
            path = root / rel
            if not path.exists():
                errors.append(f"missing Step 5 coordinator boundary: {rel}")
                continue
            text = path.read_text(encoding="utf-8")
            for marker in markers:
                if marker not in text:
                    errors.append(f"missing Step 5 invariant in {rel}: {marker}")

        worker_path = root / "apps/control-plane-worker/src/profile_coordinator.rs"
        if worker_path.exists():
            worker = worker_path.read_text(encoding="utf-8")
            actor_index = worker.find("resolve_active_request_actor")
            acl_index = worker.find("find_visible_profile")
            object_index = worker.find("env.durable_object")
            if min(actor_index, acl_index, object_index) < 0 or not (
                actor_index < acl_index < object_index
            ):
                errors.append(
                    "Worker must resolve active actor and explicit profile ACL before Durable Object access"
                )

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Repository Step 5 coordinator boundaries are enforced.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
