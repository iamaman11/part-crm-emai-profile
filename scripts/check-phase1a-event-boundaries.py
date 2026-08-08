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


def require_files(paths: list[Path]) -> None:
    missing = [path.relative_to(ROOT).as_posix() for path in paths if not path.is_file()]
    if missing:
        raise SystemExit(f"Phase 1A boundary files missing: {missing}")


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
        "retry",
        "backoff",
        "dead_letter",
        "dlq",
        "max_attempt",
    )
    violations = [marker for marker in prohibited if marker.lower() in worker.lower()]
    if violations:
        raise SystemExit(f"Worker integration-event transport owns forbidden logic: {violations}")
    required = (
        "dispatch_pending_events",
        "accept_foundation_delivery_once",
        "message.ack()",
    )
    missing = [marker for marker in required if marker not in worker]
    if missing:
        raise SystemExit(f"Worker integration-event transport missing thin composition markers: {missing}")


def enforce_phase1a_not_phase1b() -> None:
    migration = (ROOT / "migrations" / "d1" / "0012_integration_event_foundation.sql").read_text(
        encoding="utf-8"
    ).lower()
    required = (
        "alter table outbox_events",
        "envelope_version",
        "event_version",
        "create table consumer_idempotency",
        "outbox_event_payload_guard",
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
    for marker in ("max_retries", "dead_letter_queue"):
        if marker in wrangler:
            raise SystemExit(f"Phase 1B Queue policy leaked into Phase 1A: {marker}")


def registered_events() -> set[str]:
    registry = (ROOT / "crates" / "contracts" / "src" / "integration_event_registry.rs").read_text(
        encoding="utf-8"
    )
    return set(EVENT_PATTERN.findall(registry))


def producer_events() -> set[str]:
    observed: set[str] = set()
    for path in PRODUCER_FILES:
        text = path.read_text(encoding="utf-8")
        observed.update(EVENT_PATTERN.findall(text))
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
    forbid_outer_dependencies()
    enforce_thin_worker_transport()
    enforce_phase1a_not_phase1b()
    enforce_registry_covers_current_producers()
    print("Phase 1A event/outbox ownership, registry and scope boundaries passed.")


if __name__ == "__main__":
    main()
