#!/usr/bin/env python3
"""Enforce permanent Repository Step 6 and Phase 2F Bridge/device boundaries."""

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
        "pub struct FakeProcessControl",
        "ProcessAction::ForceTerminate",
        "requires_version_negotiation",
    ),
    "apps/profile-bridge/src/browser_execution.rs": (
        "BRIDGE_LOCK_SCHEMA",
        "expected_workspace_epoch",
        "verify_bridge_lock",
        "NetworkIdentityDecision::Accepted",
        "BrowserWriterDecision::RecoveryRequired",
    ),
    "apps/profile-bridge/src/browser_preflight.rs": (
        "pub struct BoundBrowserLaunchPreflight",
        "BrowserRuntimeObservationPort",
        "runtime_inventory_sha256()",
        ".observe(workspace, device_id)",
        "evaluate_browser_launch(",
    ),
    "apps/profile-bridge/src/operator_flow.rs": (
        "pub trait BrowserLaunchPreflightPort",
        "browser_preflight: B",
        ".evaluate_before_launch(",
        "OperatorFailureStage::BrowserPreflight",
        "RuntimeSessionOrchestrator::launch(",
        "browser_preflight_failure_prevents_runtime_spawn_and_releases_ownership",
    ),
    "apps/profile-bridge/src/bin/profile-bridge-synthetic.rs": (
        "BoundBrowserLaunchPreflight",
        "persist_materialization_binding",
        "synthetic_browser_preflight",
    ),
    "crates/application-ports/src/device_jobs.rs": (
        "pub trait AuthenticatedDevicePort",
        "authenticated_device_id",
        "DeviceJobPortErrorClass::AuthenticationFailed",
        "pub trait DeviceJobQueryPort",
        "list_claimable_device_jobs",
    ),
    "crates/application-ports/src/lib.rs": (
        "AuthenticatedDevicePort",
        "DeviceJobQueryPort",
    ),
    "crates/use-cases-devices/src/jobs.rs": (
        "execute_claim_device_job",
        "execute_heartbeat_device_job",
        "execute_apply_device_job_outcome",
        "ensure_authenticated_device",
        "DeviceJobPortErrorClass::AuthenticationFailed => DeviceJobOperationError::Forbidden",
    ),
    "crates/use-cases-devices/src/queries.rs": (
        "MAX_CLAIMABLE_DEVICE_JOBS",
        "execute_list_claimable_device_jobs",
        ".authenticated_device_id(actor)",
        ".list_claimable_device_jobs(actor, &device_id",
        "DeviceJobCapability::Claim",
        "DeviceExecutionReadiness::Ready",
        "DeviceJobPortErrorClass::AuthenticationFailed => DeviceJobQueryError::Forbidden",
        "DeviceJobQueryError::IntegrityFailure",
        "foreign_device_row_is_integrity_failure_before_projection",
    ),
    "crates/use-cases-devices/tests/job_orchestration.rs": (
        "foreign_authenticated_device_cannot_claim_target",
        "assert_eq!(authorization.calls.get(), 0)",
        "assert_eq!(repository.loads.get(), 0)",
        "assert_eq!(repository.writes.get(), 0)",
    ),
    "crates/cloudflare-adapters/src/d1_authenticated_device.rs": (
        "impl AuthenticatedDevicePort for D1AuthenticatedDevice",
        "FROM device_actor_bindings AS binding",
        "membership.status = 'ACTIVE'",
        "binding.status = 'ACTIVE'",
        "LIMIT 2",
        "Err(authentication_failed())",
        "Err(integrity_failure())",
    ),
    "crates/cloudflare-adapters/src/d1_device_jobs.rs": (
        "LIST_CLAIMABLE_DEVICE_JOBS",
        "job.tenant_id = ?",
        "job.device_id = ?",
        "authorization.status = 'ACTIVE'",
        "membership.status = 'ACTIVE'",
        "profile_grants",
        "job.status = 'PENDING_DEVICE'",
        "job.status IN ('PROFILE_BUSY', 'RETRY_SCHEDULED')",
        "job.retry_at_ms <= ?",
        "LIMIT ?",
        "impl DeviceJobQueryPort for D1DeviceJobRepository",
        "claimable_query_is_live_grant_device_authorization_and_due_scoped",
    ),
    "migrations/d1/0018_device_authorizations_and_jobs.sql": (
        "CREATE INDEX device_jobs_claimable_device_lookup",
        "tenant_id, device_id, status, retry_at_ms, updated_at_ms, job_id",
    ),
    "migrations/d1/0019_device_actor_bindings.sql": (
        "CREATE TABLE device_actor_bindings",
        "PRIMARY KEY (tenant_id, actor_id, version)",
        "CREATE UNIQUE INDEX device_actor_bindings_one_active_actor",
        "WHERE status = 'ACTIVE'",
        "REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT",
    ),
    "scripts/test-device-job-d1.py": (
        "AUTHENTICATED_DEVICE_QUERY",
        "test_actor_device_binding_is_unique_revocable_and_membership_scoped",
        "device_actor_bindings_one_active_actor",
        "CLAIMABLE_QUERY",
        "test_claimable_due_query_and_index",
        "device_jobs_claimable_device_lookup",
        "EXPLAIN QUERY PLAN",
        "assert claimable_ids(connection, 500) == []",
    ),
    "crates/control-plane-contract/src/routes/devices.rs": (
        "DeviceJobClaimableApi",
        "DeviceJobClaimApi",
        "DeviceJobHeartbeatApi",
        "DeviceJobOutcomeApi",
    ),
    "apps/control-plane-worker/src/composition.rs": (
        "pub fn authenticated_device",
        "pub fn device_job_authorization",
        "pub fn device_execution_preconditions",
        "pub fn device_job_repository",
    ),
    "apps/control-plane-worker/src/device_jobs.rs": (
        "resolve_active_request_actor",
        "authenticated_device(env)?",
        "authenticated_device_id(actor)",
        "ResolvedAuthenticatedDevice",
        "execute_list_claimable_device_jobs",
        "execute_claim_device_job",
        "execute_heartbeat_device_job",
        "execute_apply_device_job_outcome",
        "DEVICE_CLAIM_LEASE_MS",
        "MAX_DEVICE_RETRY_DELAY_MS",
        "Date::now().as_millis()",
        "checked_future",
        "deny_unknown_fields",
        "transport_rejects_device_time_and_lease_substitution_fields",
        "heartbeat_and_outcome_are_strict_and_retry_is_relative_only",
    ),
    "apps/control-plane-worker/src/lib.rs": (
        "mod device_jobs;",
        "RouteClass::DeviceJobClaimableApi",
        "RouteClass::DeviceJobClaimApi",
        "RouteClass::DeviceJobHeartbeatApi",
        "RouteClass::DeviceJobOutcomeApi",
        "device_jobs::dispatch(route, &mut request, &env).await",
    ),
    "apps/profile-bridge/src/windows_native.rs": (
        "std::os::windows::ffi::OsStrExt",
        "pub fn encode_wide_argument",
        "windows_argument_encoding_is_nul_terminated_without_unsafe_code",
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

FORBIDDEN_PHASE2F_PATHS = (
    "scripts/phase2f-materialize-lease.py",
)

FIXTURE_PREFIX = "tests/windows-bridge/fixtures/"
POLICY_PATH = "scripts/check-step6-windows-bridge.py"
BRIDGE_SOURCE_ROOTS = (
    "apps/profile-bridge",
    "crates/bridge-domain",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    return parser.parse_args()


def relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def source_files(root: Path, repository_root: bool):
    roots = [root / value for value in BRIDGE_SOURCE_ROOTS] if repository_root else [root]
    for scan_root in roots:
        if not scan_root.exists():
            continue
        for path in scan_root.rglob("*"):
            if path.is_file() and path.suffix in {".rs", ".py"}:
                yield path


def function_body(source: str, function_name: str) -> str:
    marker = f"pub async fn {function_name}"
    start = source.find(marker)
    if start < 0:
        return ""
    next_start = source.find("\npub async fn ", start + len(marker))
    return source[start : next_start if next_start >= 0 else len(source)]


def struct_body(source: str, struct_name: str) -> str:
    marker = f"struct {struct_name} {{"
    start = source.find(marker)
    if start < 0:
        return ""
    end = source.find("\n}", start + len(marker))
    return source[start : end + 2 if end >= 0 else len(source)]


def require_order(
    errors: list[str],
    source: str,
    earlier: str,
    later: str,
    label: str,
) -> None:
    earlier_at = source.find(earlier)
    later_at = source.find(later)
    if earlier_at < 0 or later_at < 0 or earlier_at >= later_at:
        errors.append(f"Phase 2F ordering violated for {label}: {earlier} must precede {later}")


def enforce_phase2f_ordering(root: Path, errors: list[str]) -> None:
    operator = (root / "apps/profile-bridge/src/operator_flow.rs").read_text(encoding="utf-8")
    require_order(
        errors,
        operator,
        ".evaluate_before_launch(",
        "RuntimeSessionOrchestrator::launch(",
        "browser preflight before runtime launch",
    )

    device_jobs = (root / "crates/use-cases-devices/src/jobs.rs").read_text(encoding="utf-8")
    for function_name in (
        "execute_claim_device_job",
        "execute_heartbeat_device_job",
        "execute_apply_device_job_outcome",
    ):
        body = function_body(device_jobs, function_name)
        if not body:
            errors.append(f"missing Phase 2F device operation: {function_name}")
            continue
        require_order(
            errors,
            body,
            "ensure_authenticated_device(",
            "authorize(",
            f"{function_name} authenticated device before authorization",
        )
        require_order(
            errors,
            body,
            "ensure_authenticated_device(",
            "load_exact_job(",
            f"{function_name} authenticated device before repository access",
        )

    device_queries = (root / "crates/use-cases-devices/src/queries.rs").read_text(encoding="utf-8")
    query_body = function_body(device_queries, "execute_list_claimable_device_jobs")
    if not query_body:
        errors.append("missing Phase 2F claimable device-job query operation")
    else:
        require_order(
            errors,
            query_body,
            ".authenticated_device_id(actor)",
            ".list_claimable_device_jobs(actor, &device_id",
            "claimable query authenticates device before D1/provider query",
        )
        require_order(
            errors,
            query_body,
            ".is_device_job_authorized(actor, target, DeviceJobCapability::Claim)",
            ".evaluate_device_execution(actor, target)",
            "claimable query authorization before execution precondition projection",
        )

    identity_adapter = (
        root / "crates/cloudflare-adapters/src/d1_authenticated_device.rs"
    ).read_text(encoding="utf-8")
    for forbidden in ("X-Device-Id", "X-Device-ID", "x-device-id"):
        if forbidden in identity_adapter:
            errors.append(
                "trusted device identity must come from verified actor binding, not a request header"
            )

    worker_ingress = (root / "apps/control-plane-worker/src/device_jobs.rs").read_text(
        encoding="utf-8"
    )
    dispatch_body = function_body(worker_ingress, "dispatch")
    require_order(
        errors,
        dispatch_body,
        "resolve_active_request_actor(",
        "authenticated_device(env)?",
        "Worker resolves verified actor before trusted device principal",
    )
    require_order(
        errors,
        dispatch_body,
        "authenticated_device_id(actor)",
        "execute_list_claimable_device_jobs(",
        "Worker resolves trusted device before any device use-case dispatch",
    )
    if ".headers()" in worker_ingress or "X-Device-Id" in worker_ingress:
        errors.append("device Worker ingress must not derive device identity from request headers")
    for request_struct in (
        "ClaimDeviceJobRequest",
        "HeartbeatDeviceJobRequest",
        "ApplyDeviceJobOutcomeRequest",
    ):
        body = struct_body(worker_ingress, request_struct)
        if not body:
            errors.append(f"missing strict Phase 2F Worker DTO: {request_struct}")
            continue
        for forbidden in (
            "device_id",
            "tenant_id",
            "observed_at",
            "lease_expires_at",
            "retry_at",
        ):
            if forbidden in body:
                errors.append(
                    f"{request_struct} must not accept trusted/server-owned field: {forbidden}"
                )

    synthetic = (root / "apps/profile-bridge/src/bin/profile-bridge-synthetic.rs").read_text(
        encoding="utf-8"
    )
    if "AllowBrowserPreflight" in synthetic:
        errors.append("synthetic executable must use the bound browser preflight, not an allow stub")


def phase2f_ordering_self_test(errors: list[str]) -> None:
    probe: list[str] = []
    require_order(
        probe,
        "RuntimeSessionOrchestrator::launch(); .evaluate_before_launch();",
        ".evaluate_before_launch(",
        "RuntimeSessionOrchestrator::launch(",
        "negative preflight bypass fixture",
    )
    if not probe:
        errors.append("Phase 2F negative preflight-order fixture unexpectedly passed")

    probe = []
    require_order(
        probe,
        "authorize(); ensure_authenticated_device(); load_exact_job();",
        "ensure_authenticated_device(",
        "authorize(",
        "negative device-auth ordering fixture",
    )
    if not probe:
        errors.append("Phase 2F negative device-auth-order fixture unexpectedly passed")

    probe = []
    require_order(
        probe,
        ".list_claimable_device_jobs(actor, &device_id); .authenticated_device_id(actor);",
        ".authenticated_device_id(actor)",
        ".list_claimable_device_jobs(actor, &device_id",
        "negative claimable-query device-auth fixture",
    )
    if not probe:
        errors.append("Phase 2F negative claimable-query device-auth fixture unexpectedly passed")

    probe = []
    require_order(
        probe,
        "authenticated_device(env)?; resolve_active_request_actor();",
        "resolve_active_request_actor(",
        "authenticated_device(env)?",
        "negative Worker actor/device ordering fixture",
    )
    if not probe:
        errors.append("Phase 2F negative Worker actor/device-order fixture unexpectedly passed")


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

    for path in source_files(root, repository_root):
        rel = relative(root, path)
        if rel == POLICY_PATH or (repository_root and rel.startswith(FIXTURE_PREFIX)):
            continue
        text = path.read_text(encoding="utf-8")
        if any(marker in text for marker in DELETION_MARKERS) and any(
            marker in text for marker in BROWSER_LOCK_MARKERS
        ):
            errors.append(f"automatic browser runtime lock deletion is forbidden: {rel}")

    if repository_root:
        for forbidden in FORBIDDEN_PHASE2F_PATHS:
            if (root / forbidden).exists():
                errors.append(f"temporary Phase 2F materializer artifact remains: {forbidden}")

        for rel, markers in REPOSITORY_REQUIRED.items():
            path = root / rel
            if not path.exists():
                errors.append(f"missing Step 6 / Phase 2F boundary: {rel}")
                continue
            text = path.read_text(encoding="utf-8")
            for marker in markers:
                if marker not in text:
                    errors.append(f"missing Step 6 / Phase 2F invariant in {rel}: {marker}")

        enforce_phase2f_ordering(root, errors)
        phase2f_ordering_self_test(errors)

        cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
        for member in (
            '"apps/profile-bridge"',
            '"crates/bridge-domain"',
            '"crates/device-domain"',
            '"crates/use-cases-devices"',
        ):
            if member not in cargo:
                errors.append(f"Step 6 / Phase 2F workspace member missing: {member}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Repository Step 6 and Phase 2F Bridge/device boundaries are enforced.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
