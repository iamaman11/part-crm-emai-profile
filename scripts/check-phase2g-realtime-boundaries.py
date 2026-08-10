#!/usr/bin/env python3
"""Permanent Phase 2G guard for durable-first metadata-safe realtime notifications."""

from __future__ import annotations

import argparse
from pathlib import Path

CONTRACT = Path("crates/contracts/src/realtime.rs")
USE_CASE = Path("crates/use-cases-notifications/src/realtime.rs")
D1_AUTH = Path("crates/cloudflare-adapters/src/d1_realtime_notifications.rs")
HUB = Path("apps/control-plane-worker/src/realtime_notifications.rs")
FANOUT = Path("apps/control-plane-worker/src/realtime_fanout.rs")
QUEUE = Path("apps/control-plane-worker/src/integration_events.rs")
FRONTEND = Path("frontend/src/shared/realtime/NotificationRealtimeBridge.tsx")
FRONTEND_CONTRACT = Path("frontend/src/shared/realtime/notifications.ts")
FRONTEND_TEST = Path("frontend/src/shared/realtime/notifications.test.ts")
ROUTE = Path("crates/control-plane-contract/src/routes/notifications.rs")

PUBLIC_SIGNAL_FIELDS = (
    "version: u16",
    "event_id: OutboxEventId",
    "resource: RealtimeResourceKind",
    "occurred_at: UnixMillis",
)
FORBIDDEN_PUBLIC_SIGNAL_TOKENS = (
    "aggregate_id:",
    "payload:",
    "subject:",
    "body:",
    "contact:",
    "credential:",
    "secret_handle:",
)
REQUIRED_HUB_MARKERS = (
    "state.accept_web_socket",
    "serialize_attachment",
    "set_alarm",
    "NotificationCapability::CatchUp",
    "synchronize_realtime_session",
    "publish_live_invalidation",
    "headers.delete(ACCESS_TOKEN_HEADER)",
    'headers.delete("Authorization")',
    'headers.delete("Cookie")',
    "SYNC_GATE_KEY",
    "POLICY_CLOSE_CODE: u16 = 1008",
)
REQUIRED_FRONTEND_MARKERS = (
    "new WebSocket(",
    "parseRealtimeMessage",
    "RealtimeEventDeduper",
    "queryClient.invalidateQueries",
    "event.code !== POLICY_REVOKED_CLOSE_CODE",
)
FORBIDDEN_FRONTEND_MARKERS = (
    "setQueryData(",
    "localStorage",
    "sessionStorage",
    "indexedDB",
)


def read(root: Path, relative: Path) -> str:
    return (root / relative).read_text(encoding="utf-8")


def failures_for_sources(
    contract: str,
    use_case: str,
    d1_auth: str,
    hub: str,
    fanout: str,
    queue: str,
    frontend: str,
    frontend_contract: str,
    frontend_test: str,
    route: str,
) -> list[str]:
    failures: list[str] = []

    signal_start = contract.find("pub struct RealtimeInvalidationSignal")
    signal_end = contract.find("impl RealtimeInvalidationSignal", signal_start)
    signal = contract[signal_start:signal_end] if signal_start >= 0 and signal_end >= 0 else ""
    for field in PUBLIC_SIGNAL_FIELDS:
        if field not in signal:
            failures.append(f"canonical realtime signal missing field: {field}")
    for token in FORBIDDEN_PUBLIC_SIGNAL_TOKENS:
        if token in signal.lower():
            failures.append(f"canonical realtime signal exposes forbidden field: {token}")
    if "REALTIME_INVALIDATION_VERSION" not in contract or "canonical_json" not in contract:
        failures.append("canonical realtime signal must be explicitly versioned and wire-encoded")

    catch_up = use_case.find("pub async fn synchronize_realtime_session")
    publish = use_case.find("sink.publish_invalidation", catch_up)
    commit = use_case.find("commit_catch_up", publish)
    if min(catch_up, publish, commit) < 0 or not catch_up < publish < commit:
        failures.append("reconnect must publish durable catch-up before advancing its cursor")
    if "publish_live_invalidation" not in use_case or "is_event_authorized" not in use_case:
        failures.append("live delivery must reauthorize the exact event before publish")
    live_start = use_case.find("pub async fn publish_live_invalidation")
    live_body = use_case[live_start:] if live_start >= 0 else ""
    if "commit_catch_up" in live_body:
        failures.append("live overlay must not advance the durable catch-up cursor")
    if "MAX_REALTIME_AUDIENCE_PAGE_SIZE" not in use_case or "strictly_ordered_after" not in use_case:
        failures.append("realtime audience fanout must be bounded and stable-key paged")

    for marker in ("membership.status = 'ACTIVE'", "client_grants", "profile_grants"):
        if marker not in d1_auth:
            failures.append(f"realtime D1 authorization missing current ACL marker: {marker}")
    production_auth = d1_auth.split("#[cfg(test)]", 1)[0].lower()
    for forbidden_acl_source in ("profile_assignments", "client_assignments", "assignment_id"):
        if forbidden_acl_source in production_auth:
            failures.append(
                f"realtime D1 authorization must never treat assignment state as ACL: {forbidden_acl_source}"
            )
    if "ORDER BY membership.actor_id ASC" not in d1_auth or "membership.actor_id > ?" not in d1_auth:
        failures.append("realtime audience query must use stable actor-id keyset paging")

    for marker in REQUIRED_HUB_MARKERS:
        if marker not in hub:
            failures.append(f"per-user notification hub missing invariant: {marker}")
    if "websocket_message" not in hub:
        failures.append("hibernatable WebSocket message handler is required")
    if "RealtimeInternalEvent" not in hub or "canonical_json" not in hub:
        failures.append("hub must bridge only typed internal events to canonical invalidation signals")

    delivered = queue.find("Ok(DeliveryProcessingOutcome::Delivered")
    realtime = queue.find("publish_durable_event(&event, env)", delivered)
    dead_letter = queue.find("Ok(DeliveryProcessingOutcome::DeadLetter)", delivered)
    if min(delivered, realtime, dead_letter) < 0 or not delivered < realtime < dead_letter:
        failures.append("realtime fanout must occur only after durable Delivered outcome")
    if "RetrySynchronizationRace" not in queue or "message.ack()" not in queue:
        failures.append("queue must distinguish reconnect synchronization race from durable acceptance")
    if "load_realtime_audience_page" not in fanout or "INTERNAL_PUBLISH_PATH" not in fanout:
        failures.append("live fanout must use bounded authorized audience and typed internal DO path")

    route_shape = '["api", "v1", "tenants", _, "notifications", "realtime"]'
    if route_shape not in route or 'method == "GET"' not in route:
        failures.append("public realtime upgrade route must be explicit GET-only notification ingress")

    for marker in REQUIRED_FRONTEND_MARKERS:
        if marker not in frontend:
            failures.append(f"frontend realtime bridge missing invariant: {marker}")
    for marker in FORBIDDEN_FRONTEND_MARKERS:
        if marker in frontend:
            failures.append(f"frontend realtime bridge must not persist or synthesize state via {marker}")
    if "SIGNAL_KEYS" not in frontend_contract or "keys.length !== SIGNAL_KEYS.length" not in frontend_contract:
        failures.append("frontend must parse the canonical realtime shape strictly")
    for forbidden_fixture in ("aggregateId", "payload", "body"):
        if forbidden_fixture not in frontend_test:
            failures.append(f"frontend metadata-safety negative fixture missing: {forbidden_fixture}")
    if "deduper.accept('outbox_a')" not in frontend_test:
        failures.append("frontend duplicate-suppression evidence is missing")

    return failures


def load_sources(root: Path) -> tuple[str, ...]:
    return tuple(
        read(root, path)
        for path in (
            CONTRACT,
            USE_CASE,
            D1_AUTH,
            HUB,
            FANOUT,
            QUEUE,
            FRONTEND,
            FRONTEND_CONTRACT,
            FRONTEND_TEST,
            ROUTE,
        )
    )


def check(root: Path) -> list[str]:
    try:
        return failures_for_sources(*load_sources(root))
    except OSError as error:
        return [f"could not read Phase 2G realtime source: {error}"]


def self_test(root: Path) -> list[str]:
    try:
        sources = list(load_sources(root))
    except OSError as error:
        return [f"could not read Phase 2G realtime source: {error}"]

    contract = sources[0]
    fixture = contract.replace(
        "occurred_at: UnixMillis,",
        "occurred_at: UnixMillis,\n    payload: String,",
        1,
    )
    rejected = failures_for_sources(fixture, *sources[1:])
    if not any("forbidden field" in failure for failure in rejected):
        return ["realtime confidential-payload fixture unexpectedly passed"]

    queue = sources[5]
    fixture_queue = queue.replace("publish_durable_event(&event, env)", "removed_realtime_publish(&event, env)", 1)
    fixture_sources = sources.copy()
    fixture_sources[5] = fixture_queue
    rejected = failures_for_sources(*fixture_sources)
    if not any("after durable Delivered" in failure for failure in rejected):
        return ["realtime durable-before-notify fixture unexpectedly passed"]

    frontend = sources[6]
    fixture_frontend = frontend.replace(
        "void queryClient.invalidateQueries({",
        "queryClient.setQueryData(['unsafe'], signal);\n        void queryClient.invalidateQueries({",
        1,
    )
    fixture_sources = sources.copy()
    fixture_sources[6] = fixture_frontend
    rejected = failures_for_sources(*fixture_sources)
    if not any("setQueryData" in failure for failure in rejected):
        return ["realtime frontend authority fixture unexpectedly passed"]

    return []


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()

    failures = self_test(root) if args.self_test else check(root)
    if failures:
        for failure in failures:
            print(f"ERROR: {failure}")
        return 1
    if args.self_test:
        print("Phase 2G realtime negative fixtures were rejected.")
    else:
        print("Phase 2G durable realtime architecture and privacy policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
