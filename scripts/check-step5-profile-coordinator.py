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
        "fn late_clean_release_becomes_uncertain",
    ),
    "crates/application-ports/src/coordinator_ingress.rs": (
        "pub trait CoordinatorIngressApplicationPort",
        "pub struct CoordinatorProfileAccess",
        "pub struct CoordinatorRuntimeResult",
        "async fn find_visible_profile",
        "async fn project",
    ),
    "crates/use-cases/src/coordinator_ingress.rs": (
        "pub async fn prepare_coordinator_ingress",
        "pub async fn execute_prepared_coordinator_ingress",
        "CoordinatorCommandInput::MarkRecovered",
        "MIN_INTENT_TTL_MS",
        "MAX_INTENT_TTL_MS",
    ),
    "crates/cloudflare-adapters/src/coordinator_ingress.rs": (
        "pub struct CloudflareCoordinatorIngressApplication",
        "durable_object(self.coordinator_binding)",
        "find_visible_profile",
        "D1ProfileCoordinatorRepository",
        "new_fencing_token",
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
    "apps/control-plane-worker/src/profile_coordinator_ingress.rs": (
        "control_plane_contract::coordinator_api",
        "CoordinatorCommandRequestDto",
        "CloudflareCoordinatorIngressApplication",
        "prepare_coordinator_ingress",
        "request.json::<CoordinatorCommandRequestDto>()",
        "execute_prepared_coordinator_ingress",
    ),
    "apps/control-plane-worker/src/profile_coordinator.rs": (
        "#[durable_object]",
        "pub struct ProfileCoordinator",
        "StoredCoordinatorDocument",
        "schedule_alarm",
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
        "must not claim atomicity across Durable Objects and D1",
    ),
}

FORBIDDEN_COORDINATOR_AUTH_MARKERS = (
    "profile_client_assignments",
    "linked_client_id",
)

FORBIDDEN_LEGACY_WORKER_ORCHESTRATION = (
    "resolve_active_request_actor",
    "D1IdentityQueryRepository",
    "D1ProfileCoordinatorRepository",
    "generate_fencing_token",
    "generate_outbox_event_id",
    "profile_is_coordinatable",
    "project_and_respond",
)

# Step 5 still rejects raw Durable Object API everywhere except explicitly governed outer adapters.
# Phase 2G adds the notification hub/fanout files to this narrow allowlist; their own architecture,
# privacy, authorization and durable-before-notify invariants are enforced by the permanent Phase 2G
# policy invoked from check-architecture.py.
ALLOWED_DURABLE_OBJECT_FILES = {
    "apps/control-plane-worker/src/lib.rs",
    "apps/control-plane-worker/src/profile_coordinator.rs",
    "apps/control-plane-worker/src/realtime_fanout.rs",
    "apps/control-plane-worker/src/realtime_notifications.rs",
    "crates/cloudflare-adapters/src/coordinator_ingress.rs",
}
FIXTURE_PREFIX = "tests/profile-coordinator/fixtures/"


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

    for path in root.rglob("*.rs"):
        rel = relative(root, path)
        if repository_root and rel.startswith(FIXTURE_PREFIX):
            continue
        text = path.read_text(encoding="utf-8")
        if path.name == "profile_coordinator.rs":
            for marker in FORBIDDEN_COORDINATOR_AUTH_MARKERS:
                if marker in text:
                    errors.append(
                        f"assignment-derived coordinator authorization is forbidden: {rel}: {marker}"
                    )
        if "durable_object(" in text and rel not in ALLOWED_DURABLE_OBJECT_FILES:
            errors.append(f"raw Durable Object API escaped outer coordinator adapters: {rel}")

    if repository_root:
        for rel, markers in REPOSITORY_REQUIRED.items():
            path = root / rel
            if not path.exists():
                errors.append(f"missing Step 5 coordinator boundary: {rel}")
                continue
            text = path.read_text(encoding="utf-8")
            for marker in markers:
                if marker not in text:
                    errors.append(f"missing Step 5 invariant in {rel}: {marker}")

        use_case_path = root / "crates/use-cases/src/coordinator_ingress.rs"
        if use_case_path.exists():
            use_case = use_case_path.read_text(encoding="utf-8")
            prepare_index = use_case.find("pub async fn prepare_coordinator_ingress")
            acl_index = use_case.find("find_visible_profile", prepare_index)
            execute_index = use_case.find("pub async fn execute_prepared_coordinator_ingress")
            runtime_candidates = (
                use_case.find(".snapshot(", execute_index),
                use_case.find(".execute(", execute_index),
            ) if execute_index >= 0 else (-1, -1)
            runtime_indexes = [index for index in runtime_candidates if index >= 0]
            runtime_index = min(runtime_indexes) if runtime_indexes else -1
            if min(prepare_index, acl_index, execute_index, runtime_index) < 0 or not (
                prepare_index <= acl_index < execute_index <= runtime_index
            ):
                errors.append(
                    "application use case must authorize visible/coordinatable profile before coordinator runtime access"
                )

        transport_path = root / "apps/control-plane-worker/src/profile_coordinator_ingress.rs"
        if transport_path.exists():
            transport = transport_path.read_text(encoding="utf-8")
            dispatch_index = transport.find("pub async fn dispatch")
            prepare_index = transport.find("prepare_coordinator_ingress(", dispatch_index)
            body_index = transport.find("request.json::<CoordinatorCommandRequestDto>()", dispatch_index)
            execute_index = transport.find("execute_prepared_coordinator_ingress(", body_index)
            if min(dispatch_index, prepare_index, body_index, execute_index) < 0 or not (
                dispatch_index < prepare_index < body_index < execute_index
            ):
                errors.append(
                    "coordinator transport must complete visibility preparation before canonical DTO parsing and execution"
                )

        do_path = root / "apps/control-plane-worker/src/profile_coordinator.rs"
        if do_path.exists():
            do_source = do_path.read_text(encoding="utf-8")
            for marker in FORBIDDEN_LEGACY_WORKER_ORCHESTRATION:
                if marker in do_source:
                    errors.append(
                        f"legacy coordinator HTTP orchestration returned to Durable Object module: {marker}"
                    )

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Repository Step 5 coordinator boundaries are enforced.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
