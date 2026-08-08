#!/usr/bin/env python3
"""Fail-closed ownership checks for the Phase 1A event/outbox foundation."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

PURE_FILES = [
    ROOT / "crates" / "contracts" / "src" / "integration_events.rs",
    ROOT / "crates" / "contracts" / "src" / "integration_event_registry.rs",
    ROOT / "crates" / "application-ports" / "src" / "integration_events.rs",
    ROOT / "crates" / "use-cases" / "src" / "integration_events.rs",
    ROOT / "crates" / "use-cases" / "src" / "foundation_event_consumer.rs",
]

PRODUCER_FILES = [
    ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_catalog.rs",
    ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_governed_commands.rs",
    ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_identity_acl.rs",
    ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_invitation_acceptance.rs",
    ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_mailboxes.rs",
    ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_profile_generations.rs",
    ROOT / "migrations" / "d1" / "0004_profile_coordinator_projection.sql",
]

EVENT_PATTERN = re.compile(r"[a-z][a-z0-9_.-]+\.v[0-9]+")
COORDINATOR_OUTCOME_BLOCK = re.compile(
    r"outcome\s+TEXT\s+NOT\s+NULL\s+CHECK\s*\(\s*outcome\s+IN\s*\((.*?)\)\s*\)",
    re.IGNORECASE | re.DOTALL,
)
COORDINATOR_DYNAMIC_EVENT = "'profile_coordinator.' || NEW.outcome || '.v1'"


def require_files(paths: list[Path]) -> None:
    missing = [path.relative_to(ROOT).as_posix() for path in paths if not path.is_file()]
    if missing:
        raise SystemExit(f"Phase 1A boundary files missing: {missing}")


def forbid_temporary_materializers() -> None:
    temporary = ROOT / "scripts" / "phase1a-source-guard-materialize.py"
    if temporary.exists():
        raise SystemExit("temporary Phase 1A source-guard materializer must not be tracked")


def forbid_outer_dependencies() -> None:
    prohibited = (
        "cloudflare",
        "worker::",
        "D1Database",
        "MessageBatch",
        "QueueIntegrationEventPublisher",
        "serde_json",
        "web_sys",
    )
    violations: list[str] = []
    for path in PURE_FILES:
        text = path.read_text(encoding="utf-8")
        for marker in prohibited:
            if marker in text:
                violations.append(f"{path.relative_to(ROOT)} contains outer marker {marker!r}")
    if violations:
        raise SystemExit("\n".join(violations))


def enforce_thin_worker_transport() -> None:
    worker = (ROOT / "apps" / "control-plane-worker" / "src" / "integration_events.rs").read_text(
        encoding="utf-8"
    )
    prohibited = (
        "INSERT INTO",
        "UPDATE outbox_events",
        "query!(",
        "accept_foundation_delivery_once(",
        "message.attempts",
    )
    violations = [marker for marker in prohibited if marker.lower() in worker.lower()]
    if violations:
        raise SystemExit(f"Worker integration-event transport owns forbidden logic: {violations}")
    required = (
        "dispatch_pending_events",
        "process_foundation_delivery",
        "message.ack()",
    )
    missing = [marker for marker in required if marker not in worker]
    if missing:
        raise SystemExit(f"Worker integration-event transport missing thin composition markers: {missing}")


def sql_surface(path: Path) -> str:
    lines = path.read_text(encoding="utf-8").splitlines()
    return "\n".join(line.split("--", 1)[0] for line in lines).lower()


def enforce_phase1a_not_phase1b() -> None:
    migration = sql_surface(ROOT / "migrations" / "d1" / "0012_integration_event_foundation.sql")
    required = (
        "alter table outbox_events",
        "envelope_version",
        "event_version",
        "outbox_event_payload_guard",
        "outbox_event_version_guard",
        "create table notification_events",
        "notification_event_source_guard",
        "create table consumer_idempotency",
        "consumer_idempotency_source_guard",
    )
    missing = [marker for marker in required if marker not in migration]
    if missing:
        raise SystemExit(f"Phase 1A migration missing required foundation: {missing}")
    prohibited = (
        "dead_letter",
        "dlq",
        "retry_after",
        "next_attempt",
        "max_attempt",
        "user_event_cursors",
        "notification_deliveries",
    )
    present = [marker for marker in prohibited if marker in migration]
    if present:
        raise SystemExit(f"Phase 1A migration contains Phase 1B surfaces: {present}")

    wrangler = (ROOT / "deploy" / "cloudflare" / "wrangler.example.toml").read_text(
        encoding="utf-8"
    ).lower()
    if "integration_events" not in wrangler or "queues.consumers" not in wrangler:
        raise SystemExit("Cloudflare example is missing the Integration Events producer/consumer")


def registered_events() -> set[str]:
    registry = (ROOT / "crates" / "contracts" / "src" / "integration_event_registry.rs").read_text(
        encoding="utf-8"
    )
    return set(EVENT_PATTERN.findall(registry))


def dynamic_coordinator_events(text: str) -> set[str]:
    if COORDINATOR_DYNAMIC_EVENT not in text:
        return set()
    match = COORDINATOR_OUTCOME_BLOCK.search(text)
    if match is None:
        raise SystemExit("coordinator event generator exists without a bounded outcome CHECK")
    outcomes = re.findall(r"'([a-z][a-z0-9_]*)'", match.group(1))
    if not outcomes:
        raise SystemExit("coordinator outcome CHECK contains no observable outcomes")
    return {f"profile_coordinator.{outcome}.v1" for outcome in outcomes}


def producer_events() -> set[str]:
    observed: set[str] = set()
    for path in PRODUCER_FILES:
        text = path.read_text(encoding="utf-8")
        observed.update(EVENT_PATTERN.findall(text))
        if path.name == "0004_profile_coordinator_projection.sql":
            observed.update(dynamic_coordinator_events(text))
    return observed


def enforce_registry_covers_current_producers() -> None:
    registered = registered_events()
    observed = producer_events()
    required = {
        "client.created.v1",
        "tenant.owner_bootstrapped.v1",
        "profile_coordinator.snapshot.v1",
        "mailbox.job_succeeded.v1",
        "profile_generation.registered.v1",
    }
    if not required.issubset(observed):
        raise SystemExit(f"expected current event producers are not observable: {sorted(required - observed)}")
    missing = observed - registered
    if missing:
        raise SystemExit(f"current durable producers are absent from foundation registry: {sorted(missing)}")


def main() -> None:
    require_files(PURE_FILES + PRODUCER_FILES)
    forbid_temporary_materializers()
    forbid_outer_dependencies()
    enforce_thin_worker_transport()
    enforce_phase1a_not_phase1b()
    enforce_registry_covers_current_producers()
    print("Phase 1A event/outbox ownership, registry and scope boundaries passed.")


if __name__ == "__main__":
    main()
