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


def expect_integrity_error(operation, fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert fragment in str(error), (fragment, str(error))
    else:
        raise AssertionError("operation unexpectedly satisfied a fail-closed D1 invariant")


def test_versioned_outbox_and_duplicate_neutral_claim() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.row_factory = sqlite3.Row
    try:
        apply_migrations(connection)
        seed_tenant(connection)
        connection.execute(
            """
            INSERT INTO outbox_events (
                tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                aggregate_version, event_type, payload_json, created_at_ms
            ) VALUES (?, ?, 'tenant', ?, 1, 'tenant.owner_bootstrapped.v1', '{}', 10)
            """,
            (TENANT_ID, EVENT_ID, TENANT_ID),
        )
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

        claim = (
            TENANT_ID,
            "consumer_foundation_v1",
            EVENT_ID,
            "tenant.owner_bootstrapped.v1",
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


def test_payload_guard_rejects_prohibited_content_before_persistence() -> None:
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
            ],
            start=1,
        ):
            expect_integrity_error(
                lambda index=index, payload=payload: connection.execute(
                    """
                    INSERT INTO outbox_events (
                        tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                        aggregate_version, event_type, payload_json, created_at_ms
                    ) VALUES (?, ?, 'tenant', ?, 1, 'tenant.owner_bootstrapped.v1', ?, 10)
                    """,
                    (TENANT_ID, f"outbox_unsafe_{index:02d}", TENANT_ID, payload),
                ),
                "outbox_payload_invalid",
            )
            connection.rollback()

        assert connection.execute(
            "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?",
            (TENANT_ID,),
        ).fetchone()[0] == 0
    finally:
        connection.close()


def test_consumer_claim_requires_real_tenant_event_pair() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        apply_migrations(connection)
        seed_tenant(connection)
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO consumer_idempotency (
                    tenant_id, consumer_id, outbox_event_id,
                    event_type, event_version, consumed_at_ms
                ) VALUES (?, 'consumer_foundation_v1', 'outbox_missing_event',
                          'tenant.owner_bootstrapped.v1', 1, 20)
                """,
                (TENANT_ID,),
            ),
            "FOREIGN KEY constraint failed",
        )
    finally:
        connection.close()


def main() -> None:
    test_versioned_outbox_and_duplicate_neutral_claim()
    test_payload_guard_rejects_prohibited_content_before_persistence()
    test_consumer_claim_requires_real_tenant_event_pair()
    print("Phase 1A durable outbox, sanitizer and consumer idempotency invariants passed.")


if __name__ == "__main__":
    main()
