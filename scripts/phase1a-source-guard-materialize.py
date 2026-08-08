#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement target, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")


replace_once(
    "crates/cloudflare-adapters/src/d1_integration_events.rs",
    '''const PERSIST_NOTIFICATION: &str = r#"\nINSERT INTO notification_events (\n    tenant_id,\n    outbox_event_id,\n    envelope_version,\n    event_type,\n    event_version,\n    occurred_at_ms,\n    persisted_at_ms\n) VALUES (?, ?, ?, ?, ?, ?, ?)\nON CONFLICT (tenant_id, outbox_event_id) DO NOTHING\n"#;''',
    '''const PERSIST_NOTIFICATION: &str = r#"\nINSERT INTO notification_events (\n    tenant_id,\n    outbox_event_id,\n    envelope_version,\n    aggregate_type,\n    aggregate_id,\n    aggregate_version,\n    event_type,\n    event_version,\n    occurred_at_ms,\n    persisted_at_ms\n) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)\nON CONFLICT (tenant_id, outbox_event_id) DO NOTHING\n"#;''',
)

replace_once(
    "crates/cloudflare-adapters/src/d1_integration_events.rs",
    '''        let occurred_at = sqlite_integer(event.occurred_at())?;\n        let persisted_at = sqlite_integer(persisted_at)?;\n        query!(\n            &self.database,\n            PERSIST_NOTIFICATION,\n            event.tenant_id().as_str(),\n            event.event_id().as_str(),\n            i64::from(event.envelope_version()),\n            event.event_type(),\n            i64::from(event.event_version()),\n            occurred_at,\n            persisted_at\n        )''',
    '''        let aggregate_version =\n            i64::try_from(event.aggregate_version().value()).map_err(|_| integrity_failure())?;\n        let occurred_at = sqlite_integer(event.occurred_at())?;\n        let persisted_at = sqlite_integer(persisted_at)?;\n        query!(\n            &self.database,\n            PERSIST_NOTIFICATION,\n            event.tenant_id().as_str(),\n            event.event_id().as_str(),\n            i64::from(event.envelope_version()),\n            event.aggregate_type(),\n            event.aggregate_id().as_str(),\n            aggregate_version,\n            event.event_type(),\n            i64::from(event.event_version()),\n            occurred_at,\n            persisted_at\n        )''',
)

replace_once(
    "scripts/test-integration-event-foundation.py",
    '''        notification = (\n            TENANT_ID,\n            EVENT_ID,\n            1,\n            "tenant.owner_bootstrapped.v1",\n            1,\n            10,\n            19,\n        )\n        for _ in range(2):\n            connection.execute(\n                """\n                INSERT INTO notification_events (\n                    tenant_id, outbox_event_id, envelope_version,\n                    event_type, event_version, occurred_at_ms, persisted_at_ms\n                ) VALUES (?, ?, ?, ?, ?, ?, ?)\n                ON CONFLICT (tenant_id, outbox_event_id) DO NOTHING\n                """,\n                notification,\n            )''',
    '''        notification = (\n            TENANT_ID,\n            EVENT_ID,\n            1,\n            "tenant",\n            TENANT_ID,\n            1,\n            "tenant.owner_bootstrapped.v1",\n            1,\n            10,\n            19,\n        )\n        for _ in range(2):\n            connection.execute(\n                """\n                INSERT INTO notification_events (\n                    tenant_id, outbox_event_id, envelope_version,\n                    aggregate_type, aggregate_id, aggregate_version,\n                    event_type, event_version, occurred_at_ms, persisted_at_ms\n                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)\n                ON CONFLICT (tenant_id, outbox_event_id) DO NOTHING\n                """,\n                notification,\n            )''',
)

replace_once(
    "scripts/test-integration-event-foundation.py",
    '''        notification_row = connection.execute(\n            """\n            SELECT event_type, event_version, occurred_at_ms, persisted_at_ms\n            FROM notification_events\n            WHERE tenant_id = ? AND outbox_event_id = ?\n            """,\n            (TENANT_ID, EVENT_ID),\n        ).fetchone()\n        assert tuple(notification_row) == (\n            "tenant.owner_bootstrapped.v1",\n            1,\n            10,\n            19,\n        )''',
    '''        notification_row = connection.execute(\n            """\n            SELECT aggregate_type, aggregate_id, aggregate_version,\n                   event_type, event_version, occurred_at_ms, persisted_at_ms\n            FROM notification_events\n            WHERE tenant_id = ? AND outbox_event_id = ?\n            """,\n            (TENANT_ID, EVENT_ID),\n        ).fetchone()\n        assert tuple(notification_row) == (\n            "tenant",\n            TENANT_ID,\n            1,\n            "tenant.owner_bootstrapped.v1",\n            1,\n            10,\n            19,\n        )''',
)

replace_once(
    "scripts/test-integration-event-foundation.py",
    '''def test_notification_and_consumer_claim_require_real_tenant_event_pair() -> None:\n''',
    '''def test_notification_source_guard_rejects_forged_metadata() -> None:\n    connection = sqlite3.connect(":memory:")\n    connection.execute("PRAGMA foreign_keys = ON")\n    try:\n        apply_migrations(connection)\n        seed_tenant(connection)\n        seed_outbox(connection)\n        expect_integrity_error(\n            lambda: connection.execute(\n                """\n                INSERT INTO notification_events (\n                    tenant_id, outbox_event_id, envelope_version,\n                    aggregate_type, aggregate_id, aggregate_version,\n                    event_type, event_version, occurred_at_ms, persisted_at_ms\n                ) VALUES (?, ?, 1, 'profile', ?, 1,\n                          'tenant.owner_bootstrapped.v1', 1, 10, 20)\n                """,\n                (TENANT_ID, EVENT_ID, TENANT_ID),\n            ),\n            "notification_event_source_mismatch",\n        )\n        assert connection.execute(\n            "SELECT COUNT(*) FROM notification_events WHERE tenant_id = ?",\n            (TENANT_ID,),\n        ).fetchone()[0] == 0\n    finally:\n        connection.close()\n\n\ndef test_notification_and_consumer_claim_require_real_tenant_event_pair() -> None:\n''',
)

replace_once(
    "scripts/test-integration-event-foundation.py",
    '''                INSERT INTO notification_events (\n                    tenant_id, outbox_event_id, envelope_version,\n                    event_type, event_version, occurred_at_ms, persisted_at_ms\n                ) VALUES (?, 'outbox_missing_event', 1,\n                          'tenant.owner_bootstrapped.v1', 1, 10, 20)\n''',
    '''                INSERT INTO notification_events (\n                    tenant_id, outbox_event_id, envelope_version,\n                    aggregate_type, aggregate_id, aggregate_version,\n                    event_type, event_version, occurred_at_ms, persisted_at_ms\n                ) VALUES (?, 'outbox_missing_event', 1,\n                          'tenant', ?, 1,\n                          'tenant.owner_bootstrapped.v1', 1, 10, 20)\n''',
)

replace_once(
    "scripts/test-integration-event-foundation.py",
    '''                (TENANT_ID,),\n            ),\n            "FOREIGN KEY constraint failed",\n        )\n        expect_integrity_error(\n''',
    '''                (TENANT_ID, TENANT_ID),\n            ),\n            "notification_event_source_mismatch",\n        )\n        expect_integrity_error(\n''',
)

replace_once(
    "scripts/test-integration-event-foundation.py",
    '''    test_payload_guard_rejects_prohibited_content_before_persistence()\n    test_notification_and_consumer_claim_require_real_tenant_event_pair()\n''',
    '''    test_payload_guard_rejects_prohibited_content_before_persistence()\n    test_notification_source_guard_rejects_forged_metadata()\n    test_notification_and_consumer_claim_require_real_tenant_event_pair()\n''',
)

replace_once(
    "scripts/check-phase1a-event-boundaries.py",
    '''        "create table notification_events",\n        "create table consumer_idempotency",\n        "outbox_event_payload_guard",\n''',
    '''        "create table notification_events",\n        "notification_event_source_guard",\n        "create table consumer_idempotency",\n        "outbox_event_payload_guard",\n''',
)

print("Phase 1A notification source guard materialized")
