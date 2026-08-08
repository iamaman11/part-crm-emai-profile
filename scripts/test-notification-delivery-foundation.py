#!/usr/bin/env python3
"""Exercise Phase 1B notification delivery/replay/catch-up/retention D1 invariants."""

from __future__ import annotations

import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
OPERATIONS_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_notification_operations.rs"
TENANT_ID = "tenant_01_notify_test"
OWNER_ACTOR_ID = "actor_owner_notify_test"
OWNER_IDENTITY_ID = "identity_owner_notify_test"
ACTOR_ID = "actor_member_notify_test"
IDENTITY_ID = "identity_member_notify_test"
CONSUMER_ID = "consumer_notify_v1"
EVENT_A = "outbox_01_notify_a"
EVENT_B = "outbox_01_notify_b"
EVENT_C = "outbox_01_notify_c"
CLIENT_ID = "client_01_notify_test"
PROFILE_ID = "profile_01_notify_test"


def apply_migrations(connection: sqlite3.Connection) -> None:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    assert versions == list(range(1, len(files) + 1)), versions
    for path in files:
        connection.executescript(path.read_text(encoding="utf-8"))
    connection.commit()


def rust_sql(name: str) -> str:
    source = OPERATIONS_ADAPTER.read_text(encoding="utf-8")
    match = re.search(rf'const {re.escape(name)}: &str = r#"(.*?)"#;', source, re.DOTALL)
    assert match is not None, name
    return match.group(1)


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
        VALUES (?, 'notify-owner-subject', 1)
        """,
        (OWNER_IDENTITY_ID,),
    )
    connection.execute(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, 'notify-member-subject', 1)
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
        (TENANT_ID, OWNER_ACTOR_ID, OWNER_IDENTITY_ID),
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


def seed_event(
    connection: sqlite3.Connection,
    event_id: str,
    occurred_at_ms: int,
    *,
    aggregate_type: str = "tenant",
    aggregate_id: str = TENANT_ID,
    event_type: str = "tenant.owner_bootstrapped.v1",
    persist_notification: bool = False,
) -> None:
    connection.execute(
        """
        INSERT INTO outbox_events (
            tenant_id, outbox_event_id, aggregate_type, aggregate_id,
            aggregate_version, event_type, payload_json, created_at_ms
        ) VALUES (?, ?, ?, ?, 1, ?, '{}', ?)
        """,
        (TENANT_ID, event_id, aggregate_type, aggregate_id, event_type, occurred_at_ms),
    )
    if persist_notification:
        connection.execute(
            """
            INSERT INTO notification_events (
                tenant_id, outbox_event_id, envelope_version,
                aggregate_type, aggregate_id, aggregate_version,
                event_type, event_version, payload_json,
                occurred_at_ms, persisted_at_ms
            ) VALUES (?, ?, 1, ?, ?, 1, ?, 1, '{}', ?, ?)
            """,
            (
                TENANT_ID,
                event_id,
                aggregate_type,
                aggregate_id,
                event_type,
                occurred_at_ms,
                occurred_at_ms,
            ),
        )


def seed_client_profile(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO clients (
            tenant_id, client_id, kind, display_name, status, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'PERSON', 'Synthetic Notify Client', 'ACTIVE', 1, ?, ?, 2, 2)
        """,
        (TENANT_ID, CLIENT_ID, OWNER_ACTOR_ID, OWNER_ACTOR_ID),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, active_generation_id, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', NULL, 1, ?, ?, 2, 2)
        """,
        (TENANT_ID, PROFILE_ID, OWNER_ACTOR_ID, OWNER_ACTOR_ID),
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


def dead_letter_delivery(connection: sqlite3.Connection, event_id: str = EVENT_A) -> None:
    ready_delivery(connection, event_id)
    connection.execute(
        """
        UPDATE notification_deliveries
        SET delivery_state='DEAD_LETTER', attempt_count=1,
            last_attempt_at_ms=30, terminal_at_ms=30,
            failure_class='INTERNAL_FAILURE', updated_at_ms=30
        WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
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
        connection.commit()

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
            SET delivery_state='DEAD_LETTER', attempt_count=1,
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


def test_delivery_transition_sequence_and_terminal_are_fail_closed() -> None:
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
                """
                UPDATE notification_deliveries
                SET delivery_state='DEAD_LETTER', attempt_count=3,
                    last_attempt_at_ms=30, terminal_at_ms=30,
                    failure_class='INTERNAL_FAILURE', updated_at_ms=30
                WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
                """,
                (TENANT_ID, CONSUMER_ID, EVENT_A),
            ),
            "notification_delivery_attempt_sequence_invalid",
        )
        connection.rollback()

        connection.execute(
            """
            UPDATE notification_deliveries
            SET delivery_state='DEAD_LETTER', attempt_count=1,
                last_attempt_at_ms=30, terminal_at_ms=30,
                failure_class='INTERNAL_FAILURE', updated_at_ms=30
            WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
            """,
            (TENANT_ID, CONSUMER_ID, EVENT_A),
        )
        connection.commit()

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE notification_deliveries
                SET delivery_state='READY', attempt_count=0,
                    last_attempt_at_ms=NULL, next_attempt_at_ms=NULL,
                    delivered_at_ms=NULL, terminal_at_ms=NULL,
                    failure_class=NULL, updated_at_ms=40
                WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
                """,
                (TENANT_ID, CONSUMER_ID, EVENT_A),
            ),
            "notification_delivery_terminal_immutable",
        )
    finally:
        connection.close()


def test_replay_is_owner_authorized_audited_immutable_and_reopens_only_dlq() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_actor(connection)
        seed_event(connection, EVENT_A, 10)
        dead_letter_delivery(connection)
        connection.commit()

        insert = """
            INSERT INTO notification_replay_intents (
                tenant_id, replay_id, consumer_id, outbox_event_id,
                audit_event_id, correlation_id, requested_by_actor_id,
                reason_class, terminal_attempt_count, requested_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 'OPERATOR_REMEDIATION', 1, 40)
        """
        expect_integrity_error(
            lambda: connection.execute(
                insert,
                (
                    TENANT_ID,
                    "replay_01_notify_member",
                    CONSUMER_ID,
                    EVENT_A,
                    "audit_01_notify_member",
                    "corr_01_notify_member",
                    ACTOR_ID,
                ),
            ),
            "notification_replay_owner_required",
        )
        connection.rollback()

        connection.execute(
            insert,
            (
                TENANT_ID,
                "replay_01_notify_owner",
                CONSUMER_ID,
                EVENT_A,
                "audit_01_notify_owner",
                "corr_01_notify_owner",
                OWNER_ACTOR_ID,
            ),
        )
        connection.commit()

        delivery = connection.execute(
            """
            SELECT delivery_state, attempt_count, terminal_at_ms, failure_class
            FROM notification_deliveries
            WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
            """,
            (TENANT_ID, CONSUMER_ID, EVENT_A),
        ).fetchone()
        assert delivery == ("READY", 0, None, None), delivery
        assert connection.execute(
            """
            SELECT COUNT(*) FROM audit_events
            WHERE tenant_id=? AND audit_event_id='audit_01_notify_owner'
              AND action='notification.replay.requested'
              AND resource_id=? AND result_code='prepared'
            """,
            (TENANT_ID, EVENT_A),
        ).fetchone()[0] == 1
        assert connection.execute(
            """
            SELECT dispatch_state FROM notification_replay_dispatches
            WHERE tenant_id=? AND replay_id='replay_01_notify_owner'
            """,
            (TENANT_ID,),
        ).fetchone()[0] == "PENDING"

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE notification_replay_intents SET reason_class='DEPENDENCY_RECOVERED'
                WHERE tenant_id=? AND replay_id='replay_01_notify_owner'
                """,
                (TENANT_ID,),
            ),
            "notification_replay_intent_immutable",
        )
        connection.rollback()
        expect_integrity_error(
            lambda: connection.execute(
                """
                DELETE FROM audit_events
                WHERE tenant_id=? AND audit_event_id='audit_01_notify_owner'
                """,
                (TENANT_ID,),
            ),
            "notification_replay_audit_immutable",
        )
        connection.rollback()
        assert connection.execute(
            "SELECT COUNT(*) FROM outbox_events WHERE tenant_id=? AND outbox_event_id=?",
            (TENANT_ID, EVENT_A),
        ).fetchone()[0] == 1
    finally:
        connection.close()


def test_retention_is_bounded_to_completed_operational_rows() -> None:
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
                """
                DELETE FROM notification_deliveries
                WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
                """,
                (TENANT_ID, CONSUMER_ID, EVENT_A),
            ),
            "notification_delivery_not_compactable",
        )
        connection.rollback()

        connection.execute(
            """
            UPDATE notification_deliveries
            SET delivery_state='DELIVERED', attempt_count=1,
                last_attempt_at_ms=30, delivered_at_ms=30, updated_at_ms=30
            WHERE tenant_id=? AND consumer_id=? AND outbox_event_id=?
            """,
            (TENANT_ID, CONSUMER_ID, EVENT_A),
        )
        connection.commit()
        rows = connection.execute(rust_sql("DELETE_DELIVERED"), (31, 10)).fetchall()
        assert len(rows) == 1
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
                "DELETE FROM user_event_cursors WHERE tenant_id=? AND actor_id=?",
                (TENANT_ID, ACTOR_ID),
            ),
            "active_user_event_cursor_not_compactable",
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
        rows = connection.execute(rust_sql("DELETE_INACTIVE_CURSORS"), (31, 10)).fetchall()
        assert len(rows) == 1
    finally:
        connection.close()


def test_catch_up_sql_authorizes_before_projection_and_is_grant_aware() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_actor(connection)
        seed_client_profile(connection)
        seed_event(connection, EVENT_A, 10, persist_notification=True)
        seed_event(
            connection,
            EVENT_B,
            11,
            aggregate_type="client",
            aggregate_id=CLIENT_ID,
            event_type="client.created.v1",
            persist_notification=True,
        )
        seed_event(
            connection,
            EVENT_C,
            12,
            aggregate_type="profile",
            aggregate_id=PROFILE_ID,
            event_type="profile.created.v1",
            persist_notification=True,
        )
        connection.execute(
            """
            INSERT INTO client_grants (
                tenant_id, actor_id, client_id, role,
                granted_by_actor_id, reason, created_at_ms
            ) VALUES (?, ?, ?, 'CLIENT_VIEWER', ?, 'catch-up test', 9)
            """,
            (TENANT_ID, ACTOR_ID, CLIENT_ID, OWNER_ACTOR_ID),
        )
        connection.commit()

        sql = rust_sql("LOAD_AUTHORIZED_EVENTS")
        member_rows = connection.execute(
            sql, (ACTOR_ID, TENANT_ID, -1, -1, -1, "", 20)
        ).fetchall()
        assert [row[0] for row in member_rows] == [EVENT_B], member_rows

        owner_rows = connection.execute(
            sql, (OWNER_ACTOR_ID, TENANT_ID, -1, -1, -1, "", 20)
        ).fetchall()
        assert [row[0] for row in owner_rows] == [EVENT_A, EVENT_B, EVENT_C], owner_rows

        connection.execute(
            """
            UPDATE memberships
            SET status='REVOKED', version=version+1, updated_at_ms=30
            WHERE tenant_id=? AND actor_id=?
            """,
            (TENANT_ID, ACTOR_ID),
        )
        connection.commit()
        revoked_rows = connection.execute(
            sql, (ACTOR_ID, TENANT_ID, -1, -1, -1, "", 20)
        ).fetchall()
        assert revoked_rows == []
    finally:
        connection.close()


def test_operations_sql_is_owner_only_and_aggregate_only() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_actor(connection)
        seed_event(connection, EVENT_A, 10, persist_notification=True)
        ready_delivery(connection)
        connection.commit()

        sql = rust_sql("LOAD_OPERATIONS")
        owner_row = connection.execute(sql, (TENANT_ID, OWNER_ACTOR_ID)).fetchone()
        assert owner_row is not None
        assert len(owner_row) == 8
        member_row = connection.execute(sql, (TENANT_ID, ACTOR_ID)).fetchone()
        assert member_row is None
        lowered = sql.lower()
        for prohibited in ("payload_json", "raw_error", "provider_error", "message_body"):
            assert prohibited not in lowered
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
        test_delivery_transition_sequence_and_terminal_are_fail_closed,
        test_replay_is_owner_authorized_audited_immutable_and_reopens_only_dlq,
        test_retention_is_bounded_to_completed_operational_rows,
        test_cursor_is_source_bound_monotonic_and_live_member_only,
        test_catch_up_sql_authorizes_before_projection_and_is_grant_aware,
        test_operations_sql_is_owner_only_and_aggregate_only,
        test_delivery_source_is_fail_closed,
    )
    for test in tests:
        test()
    print(f"Phase 1B notification D1 invariants passed ({len(tests)} tests).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
