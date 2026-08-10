#!/usr/bin/env python3
"""Enforce the Phase 2F coordinator-authoritative device-generation commit boundary."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

COORDINATOR_INGRESS = Path("crates/cloudflare-adapters/src/coordinator_ingress.rs")
D1_JOURNAL = Path("crates/cloudflare-adapters/src/d1_device_generation_commit.rs")
RUNTIME_CONTRACT = Path("crates/cloudflare-adapters/src/device_generation_commit_runtime.rs")
WORKER_COORDINATOR = Path("apps/control-plane-worker/src/profile_coordinator.rs")
WORKER_ENDPOINT = Path("apps/control-plane-worker/src/device_generation_commit.rs")
WORKER_COMPOSITION = Path("apps/control-plane-worker/src/composition.rs")
DEVICE_ROUTES = Path("crates/control-plane-contract/src/routes/devices.rs")
MIGRATION = Path("migrations/d1/0021_device_generation_commit.sql")


def production(source: str) -> str:
    return source.split("#[cfg(test)]", 1)[0]


def read(root: Path, relative: Path) -> str:
    return (root / relative).read_text(encoding="utf-8")


def struct_body(source: str, marker: str, impl_marker: str) -> str:
    start = source.find(marker)
    if start < 0:
        return ""
    remainder = source[start + len(marker) :]
    end = remainder.find(impl_marker)
    return remainder if end < 0 else remainder[:end]


def transport_struct(source: str) -> str:
    return struct_body(
        source,
        "pub struct DeviceGenerationCommitInternalRequest {",
        "\n}\n\nimpl DeviceGenerationCommitInternalRequest",
    )


def endpoint_body_struct(source: str) -> str:
    return struct_body(
        source,
        "struct DeviceGenerationCommitBody {",
        "\n}\n\nimpl DeviceGenerationCommitBody",
    )


def generation_commit_gate_release_helper(source: str) -> str:
    marker = "fn generation_commit_failure_releases_gate("
    start = source.find(marker)
    if start < 0:
        return ""
    end = source.find("\nfn generation_commit_error(", start)
    return source[start:] if end < 0 else source[start:end]


def errors(root: Path) -> list[str]:
    result: list[str] = []
    ingress = production(read(root, COORDINATOR_INGRESS))
    journal = production(read(root, D1_JOURNAL))
    runtime = production(read(root, RUNTIME_CONTRACT))
    coordinator = production(read(root, WORKER_COORDINATOR))
    endpoint = production(read(root, WORKER_ENDPOINT))
    composition = production(read(root, WORKER_COMPOSITION))
    routes = read(root, DEVICE_ROUTES)
    migration = read(root, MIGRATION)

    if "impl DeviceGenerationCommitPort for CloudflareDeviceGenerationCommitPort" not in ingress:
        result.append("final DeviceGenerationCommitPort must be the coordinator-ingress DO client")
    if "durable_object(self.coordinator_binding)" not in ingress:
        result.append("final generation commit port must address the profile Durable Object")
    if "impl DeviceGenerationCommitPort" in journal:
        result.append("raw D1 journal must never implement the final generation commit port")

    gate_put = coordinator.find(".put(GENERATION_COMMIT_GATE_KEY, &gate)")
    journal_apply = coordinator.find("journal.apply(&actor, &commit).await")
    if gate_put < 0 or journal_apply < 0 or gate_put > journal_apply:
        result.append("Durable Object must persist the authority gate before external D1 commit")
    if coordinator.count("load_generation_commit_gate().await?") < 3:
        result.append("command, alarm and generation commit paths must all observe the commit gate")
    if "lease.accepts_writer(" not in coordinator:
        result.append("Durable Object must validate exact session/epoch/raw fencing token authority")
    if "coordinator.version().value() != request.coordinator().coordinator_version()" not in coordinator:
        result.append("Durable Object must validate exact coordinator version")
    if "coordinator.last_sequence() != request.coordinator().coordinator_sequence()" not in coordinator:
        result.append("Durable Object must validate exact coordinator sequence")

    release_helper = generation_commit_gate_release_helper(coordinator)
    if not release_helper:
        result.append("generation commit failures must have an explicit gate-release policy")
    else:
        for required_class in ["StaleAuthority", "VersionConflict"]:
            if f"DeviceGenerationCommitErrorClass::{required_class}" not in release_helper:
                result.append(f"proved {required_class} failures must release the generation commit gate")
        for retained_class in ["IntegrityFailure", "DependencyUnavailable"]:
            if f"DeviceGenerationCommitErrorClass::{retained_class}" in release_helper:
                result.append(
                    f"uncertain {retained_class} failures must retain the generation commit gate"
                )
    if "generation_commit_failure_releases_gate(error.class())" not in coordinator:
        result.append("generation commit error handling must apply the explicit gate-release policy")
    if "schedule_gate_retry_alarm(&self.state).await?" not in coordinator:
        result.append("retained generation commit reservations must keep the retry alarm alive")

    if "coordinator_fencing_token_digest TEXT NOT NULL" not in migration:
        result.append("D1 commit journal must persist only the fencing-token digest")
    if "coordinator_fencing_token TEXT" in migration:
        result.append("raw coordinator fencing token must never be persisted in D1")

    transport = transport_struct(runtime)
    if not transport:
        result.append("strict internal generation commit transport is missing")
    elif "observed_at" in transport or "observedAt" in transport or "executed_at" in transport:
        result.append("internal generation commit transport must not accept a client clock")
    if "DeviceGenerationCommitInternalRequest::from_domain" not in ingress:
        result.append("outer DO client must use the strict internal generation commit DTO")

    endpoint_body = endpoint_body_struct(endpoint)
    if not endpoint_body:
        result.append("strict metadata-only device generation commit body is missing")
    for forbidden_field in [
        "device_id",
        "tenant_id",
        "observed_at",
        "executed_at",
        "expected_job_version",
        "expected_profile_version",
        "coordinator_version",
        "coordinator_sequence",
    ]:
        if forbidden_field in endpoint_body:
            result.append(
                f"public generation commit body must not accept server authority field: {forbidden_field}"
            )

    for required in [
        "deny_unknown_fields",
        "load_device_job(actor.tenant_scope().tenant_id(), &job_id)",
        "job.version()",
        "claim.claim_id() != &claim_id",
        "claim.fence() != body.fence",
        "job.last_fence() != body.fence",
        "claim.is_expired(now)",
        "load_active_profile_version(actor, &profile_id, &base_generation_id)",
        "coordinator_ingress_application(env)",
        ".snapshot(actor.tenant_scope(), &profile_id)",
        "projection.active_session_id() != Some(&session_id)",
        "projection.active_device_id() != Some(&device_id)",
        "projection.active_epoch() != Some(body.coordinator_epoch)",
        "snapshot.version().value()",
        "snapshot.sequence()",
        "execute_commit_dirty_generation",
        "generation_object_verifier(env)",
        "device_generation_commit(env)",
    ]:
        if required not in endpoint:
            result.append(f"metadata-only Worker endpoint missing required boundary: {required}")
    for forbidden in [
        "cloudflare_adapters::r2_",
        "R2GenerationObjects",
        "R2_PROFILES_BINDING",
        "ciphertext",
        "profile_bytes",
        "container: Vec",
        "observed_at_ms",
    ]:
        if forbidden in endpoint:
            result.append(f"generation commit endpoint must remain provider-free/metadata-only: {forbidden}")

    for required in [
        "pub fn generation_object_verifier",
        "R2GenerationObjects::new",
        "env.bucket(R2_PROFILES_BINDING)?",
        "pub fn coordinator_ingress_application",
        "CloudflareCoordinatorIngressApplication::new",
        "pub fn device_generation_commit",
        "CloudflareDeviceGenerationCommitPort::new",
    ]:
        if required not in composition:
            result.append(f"Worker composition missing generation commit provider wiring: {required}")

    if '"generation-commit"' not in routes or "DeviceGenerationCommitApi" not in routes:
        result.append("authenticated device generation commit route is missing")

    return result


def self_test() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        for relative in [
            COORDINATOR_INGRESS,
            D1_JOURNAL,
            RUNTIME_CONTRACT,
            WORKER_COORDINATOR,
            WORKER_ENDPOINT,
            WORKER_COMPOSITION,
            DEVICE_ROUTES,
            MIGRATION,
        ]:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(read(ROOT, relative), encoding="utf-8")

        coordinator_path = root / WORKER_COORDINATOR
        original = coordinator_path.read_text(encoding="utf-8")
        coordinator_path.write_text(
            original.replace(
                ".put(GENERATION_COMMIT_GATE_KEY, &gate)",
                ".put(\"unsafe-generation-commit-gate\", &gate)",
                1,
            ),
            encoding="utf-8",
        )
        detected = errors(root)
        if not any("persist the authority gate before external D1 commit" in item for item in detected):
            raise AssertionError("missing authority reservation negative fixture unexpectedly passed")

        coordinator_path.write_text(
            original.replace(
                "DeviceGenerationCommitErrorClass::VersionConflict\n    )",
                "DeviceGenerationCommitErrorClass::VersionConflict\n            | DeviceGenerationCommitErrorClass::DependencyUnavailable\n    )",
                1,
            ),
            encoding="utf-8",
        )
        detected = errors(root)
        if not any("DependencyUnavailable" in item and "retain" in item for item in detected):
            raise AssertionError("uncertain dependency failure gate-release fixture unexpectedly passed")

    print("Phase 2F generation-commit authority negative fixtures rejected as expected.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    detected = errors(ROOT)
    if detected:
        for item in detected:
            print(item)
        return 1
    print("Phase 2F coordinator-authoritative generation commit boundary passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
