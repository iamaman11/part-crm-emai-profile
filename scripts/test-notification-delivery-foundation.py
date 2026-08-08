#!/usr/bin/env python3
"""Exercise Phase 1B notification delivery/cursor D1 invariants with SQLite."""

from __future__ import annotations

import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
TENANT_ID = "tenant_01_notify_test"
ACTOR_ID = "actor_member_notify_test"
IDENTITY_ID = "identity_member_notify_test"
CONSUMER_ID = "consumer_notify_v1"
EVENT_A = "outbox_01_notify_a"
EVENT_B = "outbox_01_notify_b"
EVENT_C = "outbox_01_notify_c"
EVENT_TYPE = "tenant.owner_bootstrapped.v1"


def apply_migrations(connection: sqlite3.Connection) -> None:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    assert versions == list(range(1, len(files) + 1)), versions
    for path in files:
        connection.executescript(path.read_text(encoding="utf-8"))
    connection.commit()


def seed_actor(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Notify Test Tenant', 'ACTIVE', 1, 1, 1)
        """,
        (TENANT_ID,),
    )
    connection.execute(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, 'notify-test-subject', 1)
        """,
        (IDENTITY_ID,),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, 1, 1)
        """,
        (TENANT_ID, ACTOR_ID, IDENTITY_ID),
    )


def seed_event(connection: sqlite3.Connection, event_id: str, occurred_at_ms: int) -> None:
    connection.execute(
        """
        INSERT INTO outbox_events (
            tenant_id, outbox_event_id, aggregate_type, aggregate_id,
            aggregate_version, event_type, payload_json, created_at_ms
        ) VALUES (?, ?, 'tenant', ?, 1, ?, '{}', ?)
        """,
        (TENANT_ID, event_id, TENANT_ID, EVENT_TYPE, occurred_at_ms),
    )


def expect_integrity_error(operation, fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert fragment in str(error), (fragment, str(error))
    else:
        raise AssertionError("operation unexpectedly satisfied a fail-closed D1 invariant")


def ready_delivery(connection: sqlite3.Connection, event_id: str = EVENT_A) -> None:
    connection.execute(
        """
        INSERT INTO notification_deliveries (
            tenant_id, consumer_id, outbox_event_id, delivery_state,
            attempt_count, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'READY', 0, 20, 20)
        """,
        (TENANT_ID, CONSUMER_ID, event_id),
    )


def test_delivery_state_shape_and_sanitization() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_actor(connection)
        seed_event(connection, EVENT_A, 10)
        ready_delivery(connection)

        columns = {
            row[1]
            for row in connection.execute("PRAGMA table_info(notification_deliveries)")
        }
        assert {"raw_error", "provider_error", "payload_json"}.isdisjoint(columns)
        assert "failure_class" in columns

        invalid_statements = (
            """
            UPDATE notification_deliveries
            SET delivery_state='RETRY_SCHEDULED', attempt_count=1,
                last_attempt_at_ms=30, next_attempt_at_ms=NULL,
                failure_class='DEPENDENCY_UNAVAILABLE', updated_at_ms=30
            WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
            """,
            """
            UPDATE notification_deliveries
            SET delivery_state='RETRY_SCHEDULED', attempt_count=1,
                last_attempt_at_ms=30, next_attempt_at_ms=30,
                failure_class='DEPENDENCY_UNAVAILABLE', updated_at_ms=30
            WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
            """,
            """
            UPDATE notification_deliveries
            SET delivery_state='DEAD_LETTER', attempt_count=3,
                last_attempt_at_ms=30, terminal_at_ms=NULL,
                failure_class='DEPENDENCY_UNAVAILABLE', updated_at_ms=30
            WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
            """,
        )
        for statement in invalid_statements:
            expect_integrity_error(
                lambda statement=statement: connection.execute(
                    statement, (TENANT_ID, CONSUMER_ID, EVENT_A)
                ),
                "CHECK constraint failed",
            )
            connection.rollback()
    finally:
        connection.close()


def test_delivery_retention_cannot_delete_canonical_event() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_actor(connection)
        seed_event(connection, EVENT_A, 10)
        ready_delivery(connection)
        connection.commit()

        expect_integrity_error(
            lambda: connection.execute(
                "DELETE FROM outbox_events WHERE tenant_id=? AND outbox_event_id=?",
                (TENANT_ID, EVENT_A),
            ),
            "FOREIGN KEY constraint failed",
        )
        connection.rollback()
        connection.execute(
            """
            DELETE FROM notification_deliveries
            WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
            """,
            (TENANT_ID, CONSUMER_ID, EVENT_A),
        )
        assert connection.execute(
            "SELECT COUNT(*) FROM outbox_events WHERE tenant_id=? AND outbox_event_id=?",
            (TENANT_ID, EVENT_A),
        ).fetchone()[0] == 1
    finally:
        connection.close()


def test_cursor_is_source_bound_monotonic_and_live_member_only() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_actor(connection)
        for event_id, occurred_at in ((EVENT_A, 10), (EVENT_B, 10), (EVENT_C, 11)):
            seed_event(connection, event_id, occurred_at)
        connection.commit()

        connection.execute(
            """
            INSERT INTO user_event_cursors (
                tenant_id, actor_id, occurred_at_ms, outbox_event_id, updated_at_ms
            ) VALUES (?, ?, 10, ?, 20)
            """,
            (TENANT_ID, ACTOR_ID, EVENT_A),
        )
        connection.execute(
            """
            UPDATE user_event_cursors
            SET occurred_at_ms=10, outbox_event_id=?, updated_at_ms=21
            WHERE tenant_id=? AND actor_id=?
            """,
            (EVENT_B, TENANT_ID, ACTOR_ID),
        )
        connection.execute(
            """
            UPDATE user_event_cursors
            SET occurred_at_ms=11, outbox_event_id=?, updated_at_ms=22
            WHERE tenant_id=? AND actor_id=?
            """,
            (EVENT_C, TENANT_ID, ACTOR_ID),
        )
        connection.commit()

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE user_event_cursors
                SET occurred_at_ms=10, outbox_event_id=?, updated_at_ms=23
                WHERE tenant_id=? AND actor_id=?
                """,
                (EVENT_B, TENANT_ID, ACTOR_ID),
            ),
            "user_event_cursor_rewind",
        )
        connection.rollback()

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE user_event_cursors
                SET occurred_at_ms=12, outbox_event_id=?, updated_at_ms=23
                WHERE tenant_id=? AND actor_id=?
                """,
                (EVENT_C, TENANT_ID, ACTOR_ID),
            ),
            "user_event_cursor_source_mismatch",
        )
        connection.rollback()

        connection.execute(
            """
            UPDATE memberships
            SET status='REVOKED', version=version+1, updated_at_ms=30
            WHERE tenant_id=? AND actor_id=?
            """,
            (TENANT_ID, ACTOR_ID),
        )
        connection.commit()
        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE user_event_cursors SET updated_at_ms=31
                WHERE tenant_id=? AND actor_id=?
                """,
                (TENANT_ID, ACTOR_ID),
            ),
            "user_event_cursor_membership_not_active",
        )
    finally:
        connection.close()


def test_delivery_source_is_fail_closed() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_actor(connection)
        seed_event(connection, EVENT_A, 10)
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO notification_deliveries (
                    tenant_id, consumer_id, outbox_event_id, delivery_state,
                    attempt_count, created_at_ms, updated_at_ms
                ) VALUES (?, ?, 'outbox_missing_notify', 'READY', 0, 20, 20)
                """,
                (TENANT_ID, CONSUMER_ID),
            ),
            "notification_delivery_source_mismatch",
        )
    finally:
        connection.close()


def main() -> int:
    tests = (
        test_delivery_state_shape_and_sanitization,
        test_delivery_retention_cannot_delete_canonical_event,
        test_cursor_is_source_bound_monotonic_and_live_member_only,
        test_delivery_source_is_fail_closed,
    )
    for test in tests:
        test()
    print(f"Phase 1B delivery/cursor D1 invariants passed ({len(tests)} tests).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
