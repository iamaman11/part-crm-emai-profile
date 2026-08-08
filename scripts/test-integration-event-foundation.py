#!/usr/bin/env python3
"""Exercise the Phase 1A D1 integration-event foundation with SQLite."""

from __future__ import annotations

import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
TENANT_ID = "tenant_01_event_test"
ACTOR_ID = "actor_owner_event_test"
IDENTITY_ID = "identity_owner_event_test"
EVENT_ID = "outbox_01_event_test"
EVENT_TYPE = "tenant.owner_bootstrapped.v1"


def apply_migrations(connection: sqlite3.Connection) -> None:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    assert versions == list(range(1, len(files) + 1)), versions
    for path in files:
        connection.executescript(path.read_text(encoding="utf-8"))
    connection.commit()


def seed_tenant(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Event Test Tenant', 'ACTIVE', 1, 1, 1)
        """,
        (TENANT_ID,),
    )
    connection.execute(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, 'event-test-subject', 1)
        """,
        (IDENTITY_ID,),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 1, 1)
        """,
        (TENANT_ID, ACTOR_ID, IDENTITY_ID),
    )
    connection.commit()


def seed_outbox(connection: sqlite3.Connection, payload_json: str = "{}") -> None:
    connection.execute(
        """
        INSERT INTO outbox_events (
            tenant_id, outbox_event_id, aggregate_type, aggregate_id,
            aggregate_version, event_type, payload_json, created_at_ms
        ) VALUES (?, ?, 'tenant', ?, 1, ?, ?, 10)
        """,
        (TENANT_ID, EVENT_ID, TENANT_ID, EVENT_TYPE, payload_json),
    )


def expect_integrity_error(operation, fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert fragment in str(error), (fragment, str(error))
    else:
        raise AssertionError("operation unexpectedly satisfied a fail-closed D1 invariant")


def notification_tuple(payload_json: str = "{}") -> tuple[object, ...]:
    return (
        TENANT_ID,
        EVENT_ID,
        1,
        "tenant",
        TENANT_ID,
        1,
        EVENT_TYPE,
        1,
        payload_json,
        10,
        19,
    )


def test_versioned_outbox_notification_and_duplicate_neutral_claim() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.row_factory = sqlite3.Row
    try:
        apply_migrations(connection)
        seed_tenant(connection)
        seed_outbox(connection)
        row = connection.execute(
            """
            SELECT envelope_version, event_version, published_at_ms
            FROM outbox_events
            WHERE tenant_id = ? AND outbox_event_id = ?
            """,
            (TENANT_ID, EVENT_ID),
        ).fetchone()
        assert (row["envelope_version"], row["event_version"], row["published_at_ms"]) == (
            1,
            1,
            None,
        )

        notification = notification_tuple()
        for _ in range(2):
            connection.execute(
                """
                INSERT INTO notification_events (
                    tenant_id, outbox_event_id, envelope_version,
                    aggregate_type, aggregate_id, aggregate_version,
                    event_type, event_version, payload_json,
                    occurred_at_ms, persisted_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT (tenant_id, outbox_event_id) DO NOTHING
                """,
                notification,
            )
        assert connection.execute(
            "SELECT COUNT(*) FROM notification_events WHERE tenant_id = ?",
            (TENANT_ID,),
        ).fetchone()[0] == 1
        notification_row = connection.execute(
            """
            SELECT aggregate_type, aggregate_id, aggregate_version,
                   event_type, event_version, payload_json,
                   occurred_at_ms, persisted_at_ms
            FROM notification_events
            WHERE tenant_id = ? AND outbox_event_id = ?
            """,
            (TENANT_ID, EVENT_ID),
        ).fetchone()
        assert tuple(notification_row) == (
            "tenant",
            TENANT_ID,
            1,
            EVENT_TYPE,
            1,
            "{}",
            10,
            19,
        )

        claim = (
            TENANT_ID,
            "consumer_foundation_v1",
            EVENT_ID,
            EVENT_TYPE,
            1,
            20,
        )
        first = connection.execute(
            """
            INSERT INTO consumer_idempotency (
                tenant_id, consumer_id, outbox_event_id,
                event_type, event_version, consumed_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (tenant_id, consumer_id, outbox_event_id) DO NOTHING
            RETURNING outbox_event_id
            """,
            claim,
        ).fetchone()
        second = connection.execute(
            """
            INSERT INTO consumer_idempotency (
                tenant_id, consumer_id, outbox_event_id,
                event_type, event_version, consumed_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (tenant_id, consumer_id, outbox_event_id) DO NOTHING
            RETURNING outbox_event_id
            """,
            claim,
        ).fetchone()
        assert first["outbox_event_id"] == EVENT_ID
        assert second is None
        assert connection.execute(
            "SELECT COUNT(*) FROM consumer_idempotency WHERE tenant_id = ?",
            (TENANT_ID,),
        ).fetchone()[0] == 1
    finally:
        connection.close()


def test_payload_guard_rejects_prohibited_keys_without_false_positive_values() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_tenant(connection)
        for index, payload in enumerate(
            [
                '{"secret_handle":"secret_01"}',
                '{"message_body":"private"}',
                '{"email":"person@example.test"}',
                '{"access_token":"token"}',
                '{"nested":{"subject":"private"}}',
            ],
            start=1,
        ):
            expect_integrity_error(
                lambda index=index, payload=payload: connection.execute(
                    """
                    INSERT INTO outbox_events (
                        tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                        aggregate_version, event_type, payload_json, created_at_ms
                    ) VALUES (?, ?, 'tenant', ?, 1, ?, ?, 10)
                    """,
                    (TENANT_ID, f"outbox_unsafe_{index:02d}", TENANT_ID, EVENT_TYPE, payload),
                ),
                "outbox_payload_invalid",
            )
            connection.rollback()

        connection.execute(
            """
            INSERT INTO outbox_events (
                tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                aggregate_version, event_type, payload_json, created_at_ms
            ) VALUES (?, 'outbox_safe_value', 'tenant', ?, 1, ?,
                      '{"note":"email is only a value here"}', 10)
            """,
            (TENANT_ID, TENANT_ID, EVENT_TYPE),
        )
        assert connection.execute(
            "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?",
            (TENANT_ID,),
        ).fetchone()[0] == 1
    finally:
        connection.close()


def test_outbox_event_type_and_version_must_agree() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_tenant(connection)
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO outbox_events (
                    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                    aggregate_version, event_type, event_version,
                    payload_json, created_at_ms
                ) VALUES (?, 'outbox_bad_version', 'tenant', ?, 1,
                          'tenant.owner_bootstrapped.v2', 1, '{}', 10)
                """,
                (TENANT_ID, TENANT_ID),
            ),
            "outbox_event_version_mismatch",
        )
    finally:
        connection.close()


def test_notification_source_guard_rejects_forged_metadata_and_payload() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_tenant(connection)
        seed_outbox(connection)

        forged_aggregate = list(notification_tuple())
        forged_aggregate[3] = "profile"
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO notification_events (
                    tenant_id, outbox_event_id, envelope_version,
                    aggregate_type, aggregate_id, aggregate_version,
                    event_type, event_version, payload_json,
                    occurred_at_ms, persisted_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                tuple(forged_aggregate),
            ),
            "notification_event_source_mismatch",
        )

        forged_payload = list(notification_tuple())
        forged_payload[8] = '{"note":"forged"}'
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO notification_events (
                    tenant_id, outbox_event_id, envelope_version,
                    aggregate_type, aggregate_id, aggregate_version,
                    event_type, event_version, payload_json,
                    occurred_at_ms, persisted_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                tuple(forged_payload),
            ),
            "notification_event_source_mismatch",
        )
        assert connection.execute(
            "SELECT COUNT(*) FROM notification_events WHERE tenant_id = ?",
            (TENANT_ID,),
        ).fetchone()[0] == 0
    finally:
        connection.close()


def test_notification_and_consumer_claim_require_canonical_source_event() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_tenant(connection)
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO notification_events (
                    tenant_id, outbox_event_id, envelope_version,
                    aggregate_type, aggregate_id, aggregate_version,
                    event_type, event_version, payload_json,
                    occurred_at_ms, persisted_at_ms
                ) VALUES (?, 'outbox_missing_event', 1,
                          'tenant', ?, 1, ?, 1, '{}', 10, 20)
                """,
                (TENANT_ID, TENANT_ID, EVENT_TYPE),
            ),
            "notification_event_source_mismatch",
        )
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO consumer_idempotency (
                    tenant_id, consumer_id, outbox_event_id,
                    event_type, event_version, consumed_at_ms
                ) VALUES (?, 'consumer_foundation_v1', 'outbox_missing_event', ?, 1, 20)
                """,
                (TENANT_ID, EVENT_TYPE),
            ),
            "consumer_event_source_mismatch",
        )

        seed_outbox(connection)
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO consumer_idempotency (
                    tenant_id, consumer_id, outbox_event_id,
                    event_type, event_version, consumed_at_ms
                ) VALUES (?, 'consumer_foundation_v1', ?, 'client.created.v1', 1, 20)
                """,
                (TENANT_ID, EVENT_ID),
            ),
            "consumer_event_source_mismatch",
        )
    finally:
        connection.close()


def main() -> None:
    test_versioned_outbox_notification_and_duplicate_neutral_claim()
    test_payload_guard_rejects_prohibited_keys_without_false_positive_values()
    test_outbox_event_type_and_version_must_agree()
    test_notification_source_guard_rejects_forged_metadata_and_payload()
    test_notification_and_consumer_claim_require_canonical_source_event()
    print(
        "Phase 1A durable outbox, sanitizer, notification and consumer idempotency invariants passed."
    )


if __name__ == "__main__":
    main()
