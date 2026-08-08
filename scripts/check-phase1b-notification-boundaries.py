#!/usr/bin/env python3
"""Fail closed if final Phase 1B notification ownership regresses."""

from __future__ import annotations

import argparse
import tempfile
import tomllib
from pathlib import Path

INNER_DEPS = {
    "notification-domain": {"profile-platform-primitives"},
    "use-cases-notifications": {
        "application-ports",
        "contracts",
        "notification-domain",
        "profile-platform-primitives",
    },
}
REQUIRED = (
    "crates/notification-domain/src/delivery.rs",
    "crates/notification-domain/src/cursor.rs",
    "crates/application-ports/src/notifications.rs",
    "crates/application-ports/src/integration_events.rs",
    "crates/use-cases-notifications/src/delivery.rs",
    "crates/use-cases-notifications/src/retry.rs",
    "crates/use-cases-notifications/src/replay.rs",
    "crates/use-cases-notifications/src/catch_up.rs",
    "crates/use-cases-notifications/src/retention.rs",
    "crates/use-cases-notifications/src/operations.rs",
    "crates/use-cases-notifications/tests/phase1a_event_failure_order.rs",
    "crates/cloudflare-adapters/src/d1_notifications.rs",
    "crates/cloudflare-adapters/src/d1_notification_operations.rs",
    "crates/cloudflare-adapters/src/d1_integration_events.rs",
    "crates/control-plane-contract/src/routes/notifications.rs",
    "apps/control-plane-worker/src/integration_events.rs",
    "apps/control-plane-worker/src/notifications.rs",
    "crates/use-cases/src/lib.rs",
)
SUPERSEDED = (
    "crates/use-cases/src/integration_events.rs",
    "crates/use-cases/src/foundation_event_consumer.rs",
    "crates/use-cases/tests/phase1a_event_failure_order.rs",
)
PORTS = (
    "pub trait NotificationAuthorizationPort",
    "pub trait NotificationDeliveryRepositoryPort",
    "pub trait NotificationCursorRepositoryPort",
    "pub trait NotificationCatchUpRepositoryPort",
    "pub trait NotificationReplayRepositoryPort",
    "pub trait NotificationRetentionRepositoryPort",
    "pub trait NotificationOperationsRepositoryPort",
)
OPS_IMPLS = (
    "impl NotificationAuthorizationPort for D1NotificationOperationsRepository",
    "impl NotificationCatchUpRepositoryPort for D1NotificationOperationsRepository",
    "impl NotificationReplayRepositoryPort for D1NotificationOperationsRepository",
    "impl NotificationRetentionRepositoryPort for D1NotificationOperationsRepository",
    "impl NotificationOperationsRepositoryPort for D1NotificationOperationsRepository",
)
INNER_FORBIDDEN = (
    "cloudflare_adapters",
    "worker::",
    "D1Database",
    "MessageBatch",
    "QueueIntegrationEventPublisher",
    "rand::",
    "thread_rng",
    "getrandom",
)
OPS_FORBIDDEN = (
    "payload_json",
    "IntegrationEventPayload",
    "serde_json",
    "message_body",
    "mail_body",
    "raw_error",
    "provider_error",
)


def deps(path: Path) -> set[str]:
    with path.open("rb") as handle:
        doc = tomllib.load(handle)
    result: set[str] = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        value = doc.get(section, {})
        if isinstance(value, dict):
            result.update(str(name) for name in value)
    return result


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    for relative in REQUIRED:
        path = root / relative
        if not path.is_file() or not text(path).strip():
            errors.append(f"missing Phase 1B boundary file: {relative}")
    for relative in SUPERSEDED:
        if (root / relative).exists():
            errors.append(f"superseded shared notification owner must stay removed: {relative}")

    for crate, expected in INNER_DEPS.items():
        manifest = root / "crates" / crate / "Cargo.toml"
        if not manifest.is_file():
            errors.append(f"missing manifest: {manifest.relative_to(root)}")
        elif deps(manifest) != expected:
            errors.append(f"{crate} dependency boundary drift")

    for source_root in (
        root / "crates/notification-domain/src",
        root / "crates/use-cases-notifications/src",
    ):
        if source_root.is_dir():
            for path in source_root.rglob("*.rs"):
                for marker in INNER_FORBIDDEN:
                    if marker in text(path):
                        errors.append(f"{path.relative_to(root)} contains outer marker {marker!r}")

    ports = root / "crates/application-ports/src/notifications.rs"
    if ports.is_file():
        source = text(ports)
        for symbol in PORTS:
            if symbol not in source:
                errors.append(f"notification ports missing `{symbol}`")
        for marker in INNER_FORBIDDEN:
            if marker in source:
                errors.append(f"notification ports contain outer marker {marker!r}")

    event_ports = root / "crates/application-ports/src/integration_events.rs"
    if event_ports.is_file() and "pub trait IntegrationEventSourcePort" not in text(event_ports):
        errors.append("canonical IntegrationEventSourcePort missing")

    delivery = root / "crates/cloudflare-adapters/src/d1_notifications.rs"
    if delivery.is_file():
        source = text(delivery)
        for symbol in (
            "impl NotificationDeliveryRepositoryPort for D1NotificationRepository",
            "impl NotificationCursorRepositoryPort for D1NotificationRepository",
        ):
            if symbol not in source:
                errors.append(f"delivery adapter missing `{symbol}`")

    operations = root / "crates/cloudflare-adapters/src/d1_notification_operations.rs"
    if operations.is_file():
        source = text(operations)
        for symbol in OPS_IMPLS:
            if symbol not in source:
                errors.append(f"operations adapter missing `{symbol}`")
        for marker in OPS_FORBIDDEN:
            if marker in source:
                errors.append(f"operations adapter must stay payload/error free: {marker!r}")

    canonical = root / "crates/cloudflare-adapters/src/d1_integration_events.rs"
    if canonical.is_file() and "impl IntegrationEventSourcePort for D1IntegrationEventRepository" not in text(canonical):
        errors.append("canonical event source adapter missing")

    queue_worker = root / "apps/control-plane-worker/src/integration_events.rs"
    if queue_worker.is_file():
        source = text(queue_worker)
        for symbol in (
            "process_foundation_delivery",
            "dispatch_pending_replays",
            "compact_notification_state",
            "retry_with_options",
        ):
            if symbol not in source:
                errors.append(f"queue/schedule composition missing `{symbol}`")
        for marker in ("message.attempts", ".attempts()", ".retry();"):
            if marker in source:
                errors.append(f"queue composition contains forbidden `{marker}`")

    http_worker = root / "apps/control-plane-worker/src/notifications.rs"
    if http_worker.is_file():
        source = text(http_worker)
        for symbol in ("load_catch_up", "acknowledge_catch_up", "prepare_replay", "load_operations"):
            if symbol not in source:
                errors.append(f"notification HTTP ingress missing `{symbol}`")
        for marker in ("payload_json", "notification_deliveries", "notification_events"):
            if marker in source:
                errors.append(f"notification HTTP ingress contains storage marker `{marker}`")

    worker_manifest = root / "apps/control-plane-worker/Cargo.toml"
    if worker_manifest.is_file():
        worker_deps = deps(worker_manifest)
        if "use-cases-notifications" not in worker_deps:
            errors.append("Worker must compose use-cases-notifications")
        if "notification-domain" in worker_deps:
            errors.append("Worker must not depend directly on notification-domain")

    monolith_manifest = root / "crates/use-cases/Cargo.toml"
    if monolith_manifest.is_file() and "use-cases-notifications" in deps(monolith_manifest):
        errors.append("shared use-cases must not depend on use-cases-notifications")
    monolith_lib = root / "crates/use-cases/src/lib.rs"
    if monolith_lib.is_file():
        source = text(monolith_lib)
        if "pub mod integration_events;" in source or "pub mod foundation_event_consumer;" in source:
            errors.append("shared use-cases still owns notification modules")
    return errors


def fixture(root: Path) -> None:
    for relative in REQUIRED:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("// fixture\n", encoding="utf-8")
    (root / "crates/notification-domain/Cargo.toml").write_text(
        "[package]\nname='notification-domain'\nversion='0.1.0'\n[dependencies]\nprofile-platform-primitives={}\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-notifications/Cargo.toml").write_text(
        "[package]\nname='use-cases-notifications'\nversion='0.1.0'\n[dependencies]\n"
        "application-ports={}\ncontracts={}\nnotification-domain={}\nprofile-platform-primitives={}\n",
        encoding="utf-8",
    )
    (root / "crates/application-ports/src/notifications.rs").write_text("\n".join(PORTS), encoding="utf-8")
    (root / "crates/application-ports/src/integration_events.rs").write_text("pub trait IntegrationEventSourcePort {}\n", encoding="utf-8")
    (root / "crates/cloudflare-adapters/src/d1_notifications.rs").write_text(
        "impl NotificationDeliveryRepositoryPort for D1NotificationRepository {}\n"
        "impl NotificationCursorRepositoryPort for D1NotificationRepository {}\n",
        encoding="utf-8",
    )
    (root / "crates/cloudflare-adapters/src/d1_notification_operations.rs").write_text("\n".join(OPS_IMPLS), encoding="utf-8")
    (root / "crates/cloudflare-adapters/src/d1_integration_events.rs").write_text("impl IntegrationEventSourcePort for D1IntegrationEventRepository {}\n", encoding="utf-8")
    (root / "apps/control-plane-worker/Cargo.toml").write_text(
        "[package]\nname='worker-fixture'\nversion='0.1.0'\n[dependencies]\nuse-cases-notifications={}\n",
        encoding="utf-8",
    )
    (root / "apps/control-plane-worker/src/integration_events.rs").write_text(
        "process_foundation_delivery dispatch_pending_replays compact_notification_state retry_with_options\n",
        encoding="utf-8",
    )
    (root / "apps/control-plane-worker/src/notifications.rs").write_text(
        "load_catch_up acknowledge_catch_up prepare_replay load_operations\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases/Cargo.toml").write_text(
        "[package]\nname='use-cases'\nversion='0.1.0'\n[dependencies]\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases/src/lib.rs").write_text("#![forbid(unsafe_code)]\n", encoding="utf-8")


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="phase1b-boundary-") as temp:
        root = Path(temp)
        fixture(root)
        if validate(root):
            print(f"invalid baseline: {validate(root)}")
            return 1
        bad = root / SUPERSEDED[0]
        bad.parent.mkdir(parents=True, exist_ok=True)
        bad.write_text("pub use use_cases_notifications::*;\n", encoding="utf-8")
        if not any("superseded shared" in error for error in validate(root)):
            return 1
        fixture(root)
        ops = root / "crates/cloudflare-adapters/src/d1_notification_operations.rs"
        ops.write_text("\n".join(OPS_IMPLS) + "\npayload_json\n", encoding="utf-8")
        if not any("payload/error free" in error for error in validate(root)):
            return 1
        fixture(root)
        (root / "crates/use-cases-notifications/src/retry.rs").write_text("use worker::Env;\n", encoding="utf-8")
        if not any("outer marker" in error for error in validate(root)):
            return 1
    print("Phase 1B notification negative fixtures rejected as expected.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    errors = validate(args.root.resolve())
    if errors:
        print("\n".join(errors))
        return 1
    print("Phase 1B final notification ownership boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
