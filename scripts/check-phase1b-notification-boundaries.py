#!/usr/bin/env python3
"""Fail closed if Phase 1B notification domain/application ownership regresses."""

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
    "crates/use-cases-notifications/src/lib.rs",
    "crates/use-cases-notifications/src/integration_events.rs",
    "crates/use-cases-notifications/src/foundation_event_consumer.rs",
    "crates/use-cases-notifications/src/retry.rs",
    "crates/use-cases-notifications/src/delivery.rs",
    "crates/cloudflare-adapters/src/d1_notifications.rs",
    "apps/control-plane-worker/src/integration_events.rs",
    "crates/use-cases/src/integration_events.rs",
    "crates/use-cases/src/foundation_event_consumer.rs",
)

APPLICATION_PORT_SYMBOLS = (
    "pub trait NotificationDeliveryRepositoryPort",
    "pub trait NotificationCursorRepositoryPort",
    "pub trait NotificationCatchUpRepositoryPort",
    "pub trait NotificationReplayRepositoryPort",
)

ADAPTER_SYMBOLS = (
    "impl NotificationDeliveryRepositoryPort for D1NotificationRepository",
    "impl NotificationCursorRepositoryPort for D1NotificationRepository",
)

WORKER_COMPOSITION_SYMBOLS = (
    "D1NotificationRepository",
    "use_cases_notifications::delivery",
    "process_foundation_delivery",
    "retry_with_options",
)

COMPATIBILITY_FACADES = {
    "crates/use-cases/src/integration_events.rs": (
        "pub use use_cases_notifications::integration_events::*;"
    ),
    "crates/use-cases/src/foundation_event_consumer.rs": (
        "pub use use_cases_notifications::foundation_event_consumer::*;"
    ),
}

PROHIBITED_SOURCE_MARKERS = (
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

    for crate, expected in EXPECTED_DEPENDENCIES.items():
        manifest = root / "crates" / crate / "Cargo.toml"
        if not manifest.is_file():
            errors.append(f"missing Phase 1B manifest: {manifest.relative_to(root)}")
            continue
        with manifest.open("rb") as handle:
            document = tomllib.load(handle)
        actual = dependency_names(document)
        if actual != expected:
            errors.append(
                f"{manifest.relative_to(root)} dependency boundary drift: "
                f"expected {sorted(expected)}, found {sorted(actual)}"
            )

    pure_sources = (
        root / "crates" / "notification-domain" / "src",
        root / "crates" / "use-cases-notifications" / "src",
    )
    for source_root in pure_sources:
        if not source_root.is_dir():
            continue
        for path in sorted(source_root.rglob("*.rs")):
            text = read(path)
            for marker in PROHIBITED_SOURCE_MARKERS:
                if marker in text:
                    errors.append(
                        f"{path.relative_to(root)} contains outer/runtime marker {marker!r}"
                    )

    notification_ports = root / "crates/application-ports/src/notifications.rs"
    if notification_ports.is_file():
        ports = read(notification_ports)
        for symbol in APPLICATION_PORT_SYMBOLS:
            if symbol not in ports:
                errors.append(f"application-ports/notifications.rs must own `{symbol}`")
        for marker in PROHIBITED_SOURCE_MARKERS:
            if marker in ports:
                errors.append(
                    "crates/application-ports/src/notifications.rs contains "
                    f"outer/runtime marker {marker!r}"
                )

    d1_adapter = root / "crates/cloudflare-adapters/src/d1_notifications.rs"
    if d1_adapter.is_file():
        adapter = read(d1_adapter)
        for symbol in ADAPTER_SYMBOLS:
            if symbol not in adapter:
                errors.append(f"D1 notification adapter missing `{symbol}`")

    worker_composition = root / "apps/control-plane-worker/src/integration_events.rs"
    if worker_composition.is_file():
        worker_source = read(worker_composition)
        for symbol in WORKER_COMPOSITION_SYMBOLS:
            if symbol not in worker_source:
                errors.append(f"Worker notification composition missing `{symbol}`")
        for marker in (
            "use_cases::foundation_event_consumer",
            "message.attempts",
            ".attempts()",
            ".retry();",
        ):
            if marker in worker_source:
                errors.append(f"Worker notification composition contains forbidden `{marker}`")

    ports_lib = root / "crates/application-ports/src/lib.rs"
    if ports_lib.is_file() and "pub mod notifications;" not in read(ports_lib):
        errors.append("application-ports facade missing `pub mod notifications;`")

    ports_manifest = root / "crates/application-ports/Cargo.toml"
    if ports_manifest.is_file():
        with ports_manifest.open("rb") as handle:
            ports_document = tomllib.load(handle)
        if "notification-domain" not in dependency_names(ports_document):
            errors.append("application-ports must depend inward on notification-domain")

    adapter_manifest = root / "crates/cloudflare-adapters/Cargo.toml"
    if adapter_manifest.is_file():
        with adapter_manifest.open("rb") as handle:
            adapter_document = tomllib.load(handle)
        adapter_dependencies = dependency_names(adapter_document)
        for dependency in ("application-ports", "notification-domain"):
            if dependency not in adapter_dependencies:
                errors.append(
                    f"cloudflare-adapters must depend inward on {dependency} for Phase 1B"
                )

    worker_manifest = root / "apps/control-plane-worker/Cargo.toml"
    if worker_manifest.is_file():
        with worker_manifest.open("rb") as handle:
            worker_document = tomllib.load(handle)
        worker_dependencies = dependency_names(worker_document)
        if "use-cases-notifications" not in worker_dependencies:
            errors.append("Worker must compose use-cases-notifications directly")
        if "notification-domain" in worker_dependencies:
            errors.append("Worker must not depend directly on notification-domain")

    for relative, expected in COMPATIBILITY_FACADES.items():
        path = root / relative
        if path.is_file() and read(path).strip() != expected:
            errors.append(
                f"{relative} must remain a thin temporary compatibility re-export"
            )

    notification_lib = root / "crates/use-cases-notifications/src/lib.rs"
    if notification_lib.is_file():
        lib = read(notification_lib)
        for declaration in (
            "pub mod delivery;",
            "pub mod foundation_event_consumer;",
            "pub mod integration_events;",
            "pub mod retry;",
        ):
            if declaration not in lib:
                errors.append(f"use-cases-notifications facade missing `{declaration}`")

    monolith_manifest = root / "crates/use-cases/Cargo.toml"
    if monolith_manifest.is_file():
        with monolith_manifest.open("rb") as handle:
            monolith = tomllib.load(handle)
        if "use-cases-notifications" not in dependency_names(monolith):
            errors.append(
                "temporary monolithic compatibility facade must depend on use-cases-notifications"
            )

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
    (root / "crates/application-ports/Cargo.toml").write_text(
        "[package]\nname='application-ports'\nversion='0.1.0'\n"
        "[dependencies]\nnotification-domain = {}\n",
        encoding="utf-8",
    )
    (root / "crates/application-ports/src/lib.rs").write_text(
        "pub mod notifications;\n",
        encoding="utf-8",
    )
    (root / "crates/application-ports/src/notifications.rs").write_text(
        "\n".join(APPLICATION_PORT_SYMBOLS) + "\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-notifications/Cargo.toml").write_text(
        "[package]\nname='use-cases-notifications'\nversion='0.1.0'\n"
        "[dependencies]\napplication-ports = {}\ncontracts = {}\n"
        "notification-domain = {}\nprofile-platform-primitives = {}\n",
        encoding="utf-8",
    )
    (root / "crates/cloudflare-adapters/Cargo.toml").write_text(
        "[package]\nname='cloudflare-adapters'\nversion='0.1.0'\n"
        "[dependencies]\napplication-ports = {}\nnotification-domain = {}\nworker = {}\n",
        encoding="utf-8",
    )
    (root / "crates/cloudflare-adapters/src/d1_notifications.rs").write_text(
        "\n".join(ADAPTER_SYMBOLS) + "\n",
        encoding="utf-8",
    )
    (root / "apps/control-plane-worker/Cargo.toml").write_text(
        "[package]\nname='worker-fixture'\nversion='0.1.0'\n"
        "[dependencies]\nuse-cases-notifications = {}\nworker = {}\n",
        encoding="utf-8",
    )
    (root / "apps/control-plane-worker/src/integration_events.rs").write_text(
        "\n".join(WORKER_COMPOSITION_SYMBOLS) + "\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases/Cargo.toml").write_text(
        "[package]\nname='use-cases'\nversion='0.1.0'\n"
        "[dependencies]\nuse-cases-notifications = {}\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-notifications/src/lib.rs").write_text(
        "pub mod delivery;\npub mod foundation_event_consumer;\n"
        "pub mod integration_events;\npub mod retry;\n",
        encoding="utf-8",
    )
    for relative, content in COMPATIBILITY_FACADES.items():
        (root / relative).write_text(content + "\n", encoding="utf-8")


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="phase1b-notification-boundary-") as temp_dir:
        root = Path(temp_dir)
        write_valid_fixture(root)
        baseline = validate(root)
        if baseline:
            print(f"invalid self-test baseline: {baseline}")
            return 1

        application = root / "crates/use-cases-notifications/src/delivery.rs"
        application.write_text("use worker::Env;\n", encoding="utf-8")
        errors = validate(root)
        if not any("outer/runtime marker" in error for error in errors):
            print("provider/runtime dependency fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        retry = root / "crates/use-cases-notifications/src/retry.rs"
        retry.write_text("let _ = rand::thread_rng();\n", encoding="utf-8")
        errors = validate(root)
        if not any("outer/runtime marker" in error for error in errors):
            print("runtime randomness in deterministic retry fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        facade = root / "crates/use-cases/src/integration_events.rs"
        facade.write_text("pub fn dispatch_pending_events() {}\n", encoding="utf-8")
        errors = validate(root)
        if not any("thin temporary compatibility" in error for error in errors):
            print("duplicate monolithic owner fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        ports = root / "crates/application-ports/src/notifications.rs"
        ports.write_text("use worker::Env;\n", encoding="utf-8")
        errors = validate(root)
        if not any("application-ports/src/notifications.rs" in error for error in errors):
            print("provider leakage into notification ports fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        adapter = root / "crates/cloudflare-adapters/src/d1_notifications.rs"
        adapter.write_text("// missing port implementations\n", encoding="utf-8")
        errors = validate(root)
        if not any("D1 notification adapter missing" in error for error in errors):
            print("missing D1 notification port implementation fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        worker_manifest = root / "apps/control-plane-worker/Cargo.toml"
        worker_manifest.write_text(
            "[package]\nname='worker-fixture'\nversion='0.1.0'\n"
            "[dependencies]\nuse-cases-notifications = {}\nnotification-domain = {}\n",
            encoding="utf-8",
        )
        errors = validate(root)
        if not any("must not depend directly on notification-domain" in error for error in errors):
            print("Worker direct domain dependency fixture unexpectedly passed")
            return 1

        write_valid_fixture(root)
        worker = root / "apps/control-plane-worker/src/integration_events.rs"
        worker.write_text("message.retry();\nmessage.attempts();\n", encoding="utf-8")
        errors = validate(root)
        if not any("Worker notification composition" in error for error in errors):
            print("implicit/platform-attempt Worker fixture unexpectedly passed")
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

    print("Phase 1B notification domain/application boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
