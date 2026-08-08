#!/usr/bin/env python3
"""Fail closed if final Phase 1B notification ownership regresses."""

from __future__ import annotations

import argparse
import tempfile
import tomllib
from pathlib import Path

EXPECTED_DEPENDENCIES = {
    "notification-domain": {"profile-platform-primitives"},
    "use-cases-notifications": {
        "application-ports",
        "contracts",
        "notification-domain",
        "profile-platform-primitives",
    },
}

REQUIRED_FILES = (
    "crates/notification-domain/src/lib.rs",
    "crates/notification-domain/src/delivery.rs",
    "crates/notification-domain/src/cursor.rs",
    "crates/application-ports/src/notifications.rs",
    "crates/application-ports/src/integration_events.rs",
    "crates/use-cases-notifications/src/lib.rs",
    "crates/use-cases-notifications/src/error.rs",
    "crates/use-cases-notifications/src/integration_events.rs",
    "crates/use-cases-notifications/src/foundation_event_consumer.rs",
    "crates/use-cases-notifications/src/retry.rs",
    "crates/use-cases-notifications/src/delivery.rs",
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
)

SUPERSEDED_SHARED_FILES = (
    "crates/use-cases/src/integration_events.rs",
    "crates/use-cases/src/foundation_event_consumer.rs",
    "crates/use-cases/tests/phase1a_event_failure_order.rs",
)

APPLICATION_PORT_SYMBOLS = (
    "pub trait NotificationAuthorizationPort",
    "pub trait NotificationDeliveryRepositoryPort",
    "pub trait NotificationCursorRepositoryPort",
    "pub trait NotificationCatchUpRepositoryPort",
    "pub trait NotificationReplayRepositoryPort",
    "pub trait NotificationRetentionRepositoryPort",
    "pub trait NotificationOperationsRepositoryPort",
)
EVENT_PORT_SYMBOLS = ("pub trait IntegrationEventSourcePort",)
DELIVERY_ADAPTER_SYMBOLS = (
    "impl NotificationDeliveryRepositoryPort for D1NotificationRepository",
    "impl NotificationCursorRepositoryPort for D1NotificationRepository",
)
OPERATIONS_ADAPTER_SYMBOLS = (
    "impl NotificationAuthorizationPort for D1NotificationOperationsRepository",
    "impl NotificationCatchUpRepositoryPort for D1NotificationOperationsRepository",
    "impl NotificationReplayRepositoryPort for D1NotificationOperationsRepository",
    "impl NotificationRetentionRepositoryPort for D1NotificationOperationsRepository",
    "impl NotificationOperationsRepositoryPort for D1NotificationOperationsRepository",
)
WORKER_SYMBOLS = (
    "process_foundation_delivery",
    "dispatch_pending_replays",
    "compact_notification_state",
    "retry_with_options",
)
HTTP_WORKER_SYMBOLS = (
    "load_catch_up",
    "acknowledge_catch_up",
    "prepare_replay",
    "load_operations",
)

PROHIBITED_INNER_MARKERS = (
    "cloudflare_adapters",
    "cloudflare-adapters",
    "worker::",
    "worker_sys",
    "D1Database",
    "MessageBatch",
    "QueueIntegrationEventPublisher",
    "web_sys",
    "sqlx",
    "rusqlite",
    "rand::",
    "thread_rng",
    "getrandom",
    "Math::random",
)
OPERATIONS_ADAPTER_PROHIBITED_MARKERS = (
    "payload_json",
    "IntegrationEventPayload",
    "serde_json",
    "message_body",
    "mail_body",
    "raw_error",
    "provider_error",
)


def dependency_names(document: dict[str, object]) -> set[str]:
    names: set[str] = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        value = document.get(section, {})
        if isinstance(value, dict):
            names.update(str(name) for name in value)
    return names


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def validate(root: Path) -> list[str]:
    errors: list[str] = []

    for relative in REQUIRED_FILES:
        path = root / relative
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing/non-empty Phase 1B boundary file: {relative}")
    for relative in SUPERSEDED_SHARED_FILES:
        if (root / relative).exists():
            errors.append(f"superseded shared notification owner must stay removed: {relative}")

    for crate, expected in EXPECTED_DEPENDENCIES.items():
        manifest = root / "crates" / crate / "Cargo.toml"
        if not manifest.is_file():
            errors.append(f"missing Phase 1B manifest: {manifest.relative_to(root)}")
            continue
        with manifest.open("rb") as handle:
            actual = dependency_names(tomllib.load(handle))
        if actual != expected:
            errors.append(
                f"{manifest.relative_to(root)} dependency boundary drift: "
                f"expected {sorted(expected)}, found {sorted(actual)}"
            )

    for source_root in (
        root / "crates/notification-domain/src",
        root / "crates/use-cases-notifications/src",
    ):
        if source_root.is_dir():
            for path in sorted(source_root.rglob("*.rs")):
                text = read(path)
                for marker in PROHIBITED_INNER_MARKERS:
                    if marker in text:
                        errors.append(
                            f"{path.relative_to(root)} contains outer/runtime marker {marker!r}"
                        )

    ports = root / "crates/application-ports/src/notifications.rs"
    if ports.is_file():
        text = read(ports)
        for symbol in APPLICATION_PORT_SYMBOLS:
            if symbol not in text:
                errors.append(f"notification ports must own `{symbol}`")
        for marker in PROHIBITED_INNER_MARKERS:
            if marker in text:
                errors.append(f"notification ports contain outer/runtime marker {marker!r}")

    event_ports = root / "crates/application-ports/src/integration_events.rs"
    if event_ports.is_file():
        text = read(event_ports)
        for symbol in EVENT_PORT_SYMBOLS:
            if symbol not in text:
                errors.append(f"integration event ports must own `{symbol}`")

    delivery_adapter = root / "crates/cloudflare-adapters/src/d1_notifications.rs"
    if delivery_adapter.is_file():
        text = read(delivery_adapter)
        for symbol in DELIVERY_ADAPTER_SYMBOLS:
            if symbol not in text:
                errors.append(f"D1 notification delivery adapter missing `{symbol}`")

    operations_adapter = root / "crates/cloudflare-adapters/src/d1_notification_operations.rs"
    if operations_adapter.is_file():
        text = read(operations_adapter)
        for symbol in OPERATIONS_ADAPTER_SYMBOLS:
            if symbol not in text:
                errors.append(f"D1 notification operations adapter missing `{symbol}`")
        for marker in OPERATIONS_ADAPTER_PROHIBITED_MARKERS:
            if marker in text:
                errors.append(
                    "D1 notification operations adapter must remain payload/error free; "
                    f"found {marker!r}"
                )

    canonical_event = root / "crates/cloudflare-adapters/src/d1_integration_events.rs"
    if canonical_event.is_file() and "impl IntegrationEventSourcePort for D1IntegrationEventRepository" not in read(canonical_event):
        errors.append("canonical integration event adapter must implement IntegrationEventSourcePort")

    queue_worker = root / "apps/control-plane-worker/src/integration_events.rs"
    if queue_worker.is_file():
        text = read(queue_worker)
        for symbol in WORKER_SYMBOLS:
            if symbol not in text:
                errors.append(f"Worker notification schedule/queue composition missing `{symbol}`")
        for marker in ("message.attempts", ".attempts()", ".retry();"):
            if marker in text:
                errors.append(f"Worker notification composition contains forbidden `{marker}`")

    http_worker = root / "apps/control-plane-worker/src/notifications.rs"
    if http_worker.is_file():
        text = read(http_worker)
        for symbol in HTTP_WORKER_SYMBOLS:
            if symbol not in text:
                errors.append(f"notification HTTP ingress missing application call `{symbol}`")
        for marker in ("payload_json", "notification_deliveries", "notification_events"):
            if marker in text:
                errors.append(f"notification HTTP ingress contains direct storage/payload marker `{marker}`")

    worker_manifest = root / "apps/control-plane-worker/Cargo.toml"
    if worker_manifest.is_file():
        with worker_manifest.open("rb") as handle:
            dependencies = dependency_names(tomllib.load(handle))
        if "use-cases-notifications" not in dependencies:
            errors.append("Worker must compose use-cases-notifications directly")
        if "notification-domain" in dependencies:
            errors.append("Worker must not depend directly on notification-domain")

    monolith_manifest = root / "crates/use-cases/Cargo.toml"
    if monolith_manifest.is_file():
        with monolith_manifest.open("rb") as handle:
            dependencies = dependency_names(tomllib.load(handle))
        if "use-cases-notifications" in dependencies:
            errors.append("shared use-cases must not depend on extracted use-cases-notifications")
    monolith_lib = root / "crates/use-cases/src/lib.rs"
    if monolith_lib.is_file():
        text = read(monolith_lib)
        for declaration in ("pub mod integration_events;", "pub mod foundation_event_consumer;"):
            if declaration in text:
                errors.append(f"shared use-cases must not own `{declaration}` after Phase 1B")

    return errors


def write_valid_fixture(root: Path) -> None:
    for relative in REQUIRED_FILES:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("// fixture\n", encoding="utf-8")
    (root / "crates/notification-domain/Cargo.toml").write_text(
        "[package]\nname='notification-domain'\nversion='0.1.0'\n"
        "[dependencies]\nprofile-platform-primitives = {}\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-notifications/Cargo.toml").write_text(
        "[package]\nname='use-cases-notifications'\nversion='0.1.0'\n"
        "[dependencies]\napplication-ports = {}\ncontracts = {}\n"
        "notification-domain = {}\nprofile-platform-primitives = {}\n",
        encoding="utf-8",
    )
    (root / "apps/control-plane-worker/Cargo.toml").write_text(
        "[package]\nname='worker-fixture'\nversion='0.1.0'\n"
        "[dependencies]\nuse-cases-notifications = {}\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases/Cargo.toml").write_text(
        "[package]\nname='use-cases'\nversion='0.1.0'\n[dependencies]\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases/src/lib.rs").parent.mkdir(parents=True, exist_ok=True)
    (root / "crates/use-cases/src/lib.rs").write_text("#![forbid(unsafe_code)]\n", encoding="utf-8")
    (root / "crates/application-ports/src/notifications.rs").write_text(
        "\n".join(APPLICATION_PORT_SYMBOLS) + "\n", encoding="utf-8"
    )
    (root / "crates/application-ports/src/integration_events.rs").write_text(
        "\n".join(EVENT_PORT_SYMBOLS) + "\n", encoding="utf-8"
    )
    (root / "crates/cloudflare-adapters/src/d1_notifications.rs").write_text(
        "\n".join(DELIVERY_ADAPTER_SYMBOLS) + "\n", encoding="utf-8"
    )
    (root / "crates/cloudflare-adapters/src/d1_notification_operations.rs").write_text(
        "\n".join(OPERATIONS_ADAPTER_SYMBOLS) + "\n", encoding="utf-8"
    )
    (root / "crates/cloudflare-adapters/src/d1_integration_events.rs").write_text(
        "impl IntegrationEventSourcePort for D1IntegrationEventRepository {}\n", encoding="utf-8"
    )
    (root / "apps/control-plane-worker/src/integration_events.rs").write_text(
        "\n".join(WORKER_SYMBOLS) + "\n", encoding="utf-8"
    )
    (root / "apps/control-plane-worker/src/notifications.rs").write_text(
        "\n".join(HTTP_WORKER_SYMBOLS) + "\n", encoding="utf-8"
    )


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="phase1b-notification-boundary-") as temp_dir:
        root = Path(temp_dir)
        write_valid_fixture(root)
        baseline = validate(root)
        if baseline:
            print(f"invalid self-test baseline: {baseline}")
            return 1

        (root / "crates/use-cases-notifications/src/delivery.rs").write_text(
            "use worker::Env;\n", encoding="utf-8"
        )
        if not any("outer/runtime marker" in error for error in validate(root)):
            print("provider/runtime dependency fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        (root / "crates/cloudflare-adapters/src/d1_notification_operations.rs").write_text(
            "\n".join(OPERATIONS_ADAPTER_SYMBOLS) + "\nconst BAD: &str = \"payload_json\";\n",
            encoding="utf-8",
        )
        if not any("payload/error free" in error for error in validate(root)):
            print("payload notification adapter fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        superseded = root / SUPERSEDED_SHARED_FILES[0]
        superseded.parent.mkdir(parents=True, exist_ok=True)
        superseded.write_text("pub use use_cases_notifications::*;\n", encoding="utf-8")
        if not any("superseded shared" in error for error in validate(root)):
            print("superseded shared owner fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        manifest = root / "crates/use-cases/Cargo.toml"
        manifest.write_text(
            "[package]\nname='use-cases'\nversion='0.1.0'\n"
            "[dependencies]\nuse-cases-notifications = {}\n",
            encoding="utf-8",
        )
        if not any("must not depend" in error for error in validate(root)):
            print("shared use-cases dependency fixture unexpectedly passed")
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
        for error in errors:
            print(error)
        return 1
    print("Phase 1B final notification ownership boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
