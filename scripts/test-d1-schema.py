#!/usr/bin/env python3
"""Exercise D1-compatible SQLite catalog migrations with synthetic data."""

from __future__ import annotations

import sqlite3
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"

TENANT_A = "tenant_01_catalog"
TENANT_B = "tenant_02_catalog"
OWNER_A = "actor_owner_catalog"
MEMBER_A = "actor_member_catalog"
OWNER_B = "actor_owner_other"
IDENTITY_OWNER_A = "identity_owner_catalog"
IDENTITY_MEMBER_A = "identity_member_catalog"
IDENTITY_OWNER_B = "identity_owner_other"
CLIENT_A = "client_01_catalog"
CLIENT_B = "client_02_catalog"
PROFILE_A = "profile_01_catalog"


def migration_files() -> list[Path]:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    if not files:
        raise AssertionError("no ordered D1 migrations found")
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    if versions != list(range(1, len(files) + 1)):
        raise AssertionError(f"migration versions must be contiguous from 0001: {versions}")
    return files


def open_database(path: Path | str = ":memory:") -> sqlite3.Connection:
    connection = sqlite3.connect(path)
    connection.execute("PRAGMA foreign_keys = ON")
    connection.row_factory = sqlite3.Row
    return connection


def apply_migrations(connection: sqlite3.Connection) -> None:
    for migration in migration_files():
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.commit()


def schema_signature(connection: sqlite3.Connection) -> list[tuple[str, str, str, str]]:
    rows = connection.execute(
        """
        SELECT type, name, tbl_name, COALESCE(sql, '') AS sql
        FROM sqlite_master
        WHERE name NOT LIKE 'sqlite_%'
          AND name != 'd1_migrations'
        ORDER BY type, name
        """
    ).fetchall()
    return [(row["type"], row["name"], row["tbl_name"], row["sql"]) for row in rows]


def expect_integrity_error(operation, expected_fragment: str | None = None) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        if expected_fragment is not None and expected_fragment not in str(error):
            raise AssertionError(
                f"expected integrity error containing {expected_fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError("operation unexpectedly satisfied a required database invariant")


def insert_tenant(
    connection: sqlite3.Connection,
    tenant_id: str,
    display_name: str,
    owner_actor_id: str,
    identity_id: str,
    access_subject: str,
) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'ACTIVE', 1, 10, 10)
        """,
        (tenant_id, display_name),
    )
    connection.execute(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, ?, 10)
        """,
        (identity_id, access_subject),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 10, 10)
        """,
        (tenant_id, owner_actor_id, identity_id),
    )


def seed_catalog(connection: sqlite3.Connection) -> None:
    insert_tenant(
        connection,
        TENANT_A,
        "Synthetic Tenant A",
        OWNER_A,
        IDENTITY_OWNER_A,
        "access-subject-owner-a",
    )
    insert_tenant(
        connection,
        TENANT_B,
        "Synthetic Tenant B",
        OWNER_B,
        IDENTITY_OWNER_B,
        "access-subject-owner-b",
    )
    connection.execute(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, 'access-subject-member-a', 10)
        """,
        (IDENTITY_MEMBER_A,),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT_A, MEMBER_A, IDENTITY_MEMBER_A),
    )
    connection.execute(
        """
        INSERT INTO clients (
            tenant_id, client_id, kind, display_name, status, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'PERSON', 'Synthetic Client A', 'ACTIVE', 1, ?, ?, 20, 20)
        """,
        (TENANT_A, CLIENT_A, OWNER_A, OWNER_A),
    )
    connection.execute(
        """
        INSERT INTO clients (
            tenant_id, client_id, kind, display_name, status, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'PERSON', 'Synthetic Client B', 'ACTIVE', 1, ?, ?, 20, 20)
        """,
        (TENANT_B, CLIENT_B, OWNER_B, OWNER_B),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, version, created_by_actor_id,
            updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', 1, ?, ?, 30, 30)
        """,
        (TENANT_A, PROFILE_A, OWNER_A, OWNER_A),
    )
    connection.commit()


def test_schema_is_deterministic() -> None:
    first = open_database()
    second = open_database()
    try:
        apply_migrations(first)
        apply_migrations(second)
        assert schema_signature(first) == schema_signature(second)
        assert first.execute("PRAGMA foreign_keys").fetchone()[0] == 1
    finally:
        first.close()
        second.close()


def test_constraints_and_concealment() -> None:
    connection = open_database()
    try:
        apply_migrations(connection)
        seed_catalog(connection)

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO memberships (
                    tenant_id, actor_id, identity_id, role, status, version,
                    created_at_ms, updated_at_ms
                ) VALUES (?, 'actor_second_owner', ?, 'TENANT_OWNER', 'ACTIVE', 1, 40, 40)
                """,
                (TENANT_A, IDENTITY_MEMBER_A),
            ),
            "UNIQUE constraint failed",
        )
        connection.rollback()

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_assignment_commands (
                    tenant_id, command_id, command_actor_id, assignment_id,
                    profile_id, client_id, expected_profile_version, reason, executed_at_ms
                ) VALUES (?, 'cmd_assignment_cross_tenant', ?, 'assignment_cross_tenant',
                          ?, ?, 1, 'invalid tenant link', 50)
                """,
                (TENANT_A, OWNER_A, PROFILE_A, CLIENT_B),
            ),
            "assignment_client_not_active",
        )
        connection.rollback()

        connection.execute(
            "UPDATE clients SET status = 'ARCHIVED', version = 2 WHERE tenant_id = ? AND client_id = ?",
            (TENANT_A, CLIENT_A),
        )
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_assignment_commands (
                    tenant_id, command_id, command_actor_id, assignment_id,
                    profile_id, client_id, expected_profile_version, reason, executed_at_ms
                ) VALUES (?, 'cmd_assignment_archived', ?, 'assignment_archived',
                          ?, ?, 1, 'archived client', 60)
                """,
                (TENANT_A, OWNER_A, PROFILE_A, CLIENT_A),
            ),
            "assignment_client_not_active",
        )
        connection.rollback()

        connection.execute(
            "UPDATE clients SET status = 'ACTIVE' WHERE tenant_id = ? AND client_id = ?",
            (TENANT_A, CLIENT_A),
        )
        connection.commit()
        connection.execute(
            """
            INSERT INTO profile_assignment_commands (
                tenant_id, command_id, command_actor_id, assignment_id,
                profile_id, client_id, expected_profile_version, reason, executed_at_ms
            ) VALUES (?, 'cmd_assignment_primary_one', ?, 'assignment_primary_one',
                      ?, ?, 1, 'primary assignment', 70)
            """,
            (TENANT_A, OWNER_A, PROFILE_A, CLIENT_A),
        )
        connection.commit()
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_assignment_commands (
                    tenant_id, command_id, command_actor_id, assignment_id,
                    profile_id, client_id, expected_profile_version, reason, executed_at_ms
                ) VALUES (?, 'cmd_assignment_primary_two', ?, 'assignment_primary_two',
                          ?, ?, 2, 'duplicate active assignment', 71)
                """,
                (TENANT_A, OWNER_A, PROFILE_A, CLIENT_A),
            ),
            "profile_assignment_same_client",
        )
        connection.rollback()
        active_assignments = connection.execute(
            """
            SELECT COUNT(*) AS value
            FROM profile_client_assignments
            WHERE tenant_id = ? AND profile_id = ? AND closed_at_ms IS NULL
            """,
            (TENANT_A, PROFILE_A),
        ).fetchone()["value"]
        assert active_assignments == 1

        assert (
            connection.execute(
                "SELECT COUNT(*) FROM profile_grants WHERE tenant_id = ? AND profile_id = ?",
                (TENANT_A, PROFILE_A),
            ).fetchone()[0]
            == 0
        ), "assignment must not grant profile access"

        connection.execute(
            "UPDATE memberships SET status = 'SUSPENDED' WHERE tenant_id = ? AND actor_id = ?",
            (TENANT_A, MEMBER_A),
        )
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_grants (
                    tenant_id, actor_id, profile_id, role, granted_by_actor_id,
                    reason, created_at_ms
                ) VALUES (?, ?, ?, 'PROFILE_VIEWER', ?, 'inactive member', 80)
                """,
                (TENANT_A, MEMBER_A, PROFILE_A, OWNER_A),
            ),
            "profile_grant_membership_not_active",
        )
        connection.rollback()

        foreign_result = connection.execute(
            "SELECT client_id FROM clients WHERE tenant_id = ? AND client_id = ?",
            (TENANT_A, CLIENT_B),
        ).fetchone()
        missing_result = connection.execute(
            "SELECT client_id FROM clients WHERE tenant_id = ? AND client_id = 'client_missing_catalog'",
            (TENANT_A,),
        ).fetchone()
        assert foreign_result is None and missing_result is None
    finally:
        connection.close()


def test_optimistic_version_and_mutation_envelope() -> None:
    connection = open_database()
    try:
        apply_migrations(connection)
        seed_catalog(connection)

        fresh = connection.execute(
            """
            UPDATE clients
            SET display_name = 'Updated Client', version = version + 1,
                updated_by_actor_id = ?, updated_at_ms = 100
            WHERE tenant_id = ? AND client_id = ? AND version = 1
            """,
            (OWNER_A, TENANT_A, CLIENT_A),
        )
        assert fresh.rowcount == 1
        stale = connection.execute(
            """
            UPDATE clients
            SET display_name = 'Stale Overwrite', version = version + 1,
                updated_by_actor_id = ?, updated_at_ms = 101
            WHERE tenant_id = ? AND client_id = ? AND version = 1
            """,
            (OWNER_A, TENANT_A, CLIENT_A),
        )
        assert stale.rowcount == 0
        updated = connection.execute(
            "SELECT display_name, version FROM clients WHERE tenant_id = ? AND client_id = ?",
            (TENANT_A, CLIENT_A),
        ).fetchone()
        assert (updated["display_name"], updated["version"]) == ("Updated Client", 2)
        connection.commit()

        try:
            connection.execute("BEGIN")
            rolled_back_update = connection.execute(
                """
                UPDATE clients
                SET display_name = 'Rollback Candidate', version = version + 1,
                    updated_by_actor_id = ?, updated_at_ms = 110
                WHERE tenant_id = ? AND client_id = ? AND version = 2
                """,
                (OWNER_A, TENANT_A, CLIENT_A),
            )
            assert rolled_back_update.rowcount == 1
            connection.execute(
                """
                INSERT INTO idempotency_records (
                    tenant_id, actor_id, idempotency_key, command_name, request_digest,
                    result_code, result_reference, created_at_ms, expires_at_ms
                ) VALUES (?, ?, 'idem_rollback_catalog', 'client.update',
                          '0123456789abcdef', 'updated', ?, 110, 1000)
                """,
                (TENANT_A, OWNER_A, CLIENT_A),
            )
            connection.execute(
                """
                INSERT INTO audit_events (
                    tenant_id, audit_event_id, correlation_id, actor_id, action,
                    resource_type, resource_id, result_code, occurred_at_ms
                ) VALUES (?, 'audit_rollback_catalog', 'corr_rollback_catalog', ?,
                          'client.update', 'client', ?, 'updated', 110)
                """,
                (TENANT_A, OWNER_A, CLIENT_A),
            )
            connection.execute(
                """
                INSERT INTO outbox_events (
                    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                    aggregate_version, event_type, payload_json, created_at_ms
                ) VALUES (?, 'outbox_rollback_catalog', 'client', ?, 3,
                          'client.updated.v1', '{}', 110)
                """,
                (TENANT_A, CLIENT_A),
            )
            connection.execute(
                """
                INSERT INTO audit_events (
                    tenant_id, audit_event_id, correlation_id, actor_id, action,
                    resource_type, resource_id, result_code, occurred_at_ms
                ) VALUES (?, 'audit_rollback_catalog', 'corr_duplicate_catalog', ?,
                          'forced.failure', 'client', ?, 'failed', 111)
                """,
                (TENANT_A, OWNER_A, CLIENT_A),
            )
        except sqlite3.IntegrityError:
            connection.rollback()
        else:
            raise AssertionError("forced mutation envelope failure unexpectedly committed")

        rolled_back_client = connection.execute(
            "SELECT display_name, version FROM clients WHERE tenant_id = ? AND client_id = ?",
            (TENANT_A, CLIENT_A),
        ).fetchone()
        assert (rolled_back_client["display_name"], rolled_back_client["version"]) == (
            "Updated Client",
            2,
        )
        for table, column, value in (
            ("idempotency_records", "idempotency_key", "idem_rollback_catalog"),
            ("audit_events", "audit_event_id", "audit_rollback_catalog"),
            ("outbox_events", "outbox_event_id", "outbox_rollback_catalog"),
        ):
            count = connection.execute(
                f"SELECT COUNT(*) FROM {table} WHERE tenant_id = ? AND {column} = ?",
                (TENANT_A, value),
            ).fetchone()[0]
            assert count == 0, f"{table} escaped transaction rollback"

        with connection:
            committed_update = connection.execute(
                """
                UPDATE clients
                SET display_name = 'Committed Client', version = version + 1,
                    updated_by_actor_id = ?, updated_at_ms = 120
                WHERE tenant_id = ? AND client_id = ? AND version = 2
                """,
                (OWNER_A, TENANT_A, CLIENT_A),
            )
            assert committed_update.rowcount == 1
            connection.execute(
                """
                INSERT INTO idempotency_records (
                    tenant_id, actor_id, idempotency_key, command_name, request_digest,
                    result_code, result_reference, created_at_ms, expires_at_ms
                ) VALUES (?, ?, 'idem_committed_catalog', 'client.update',
                          'fedcba9876543210', 'updated', ?, 120, 1000)
                """,
                (TENANT_A, OWNER_A, CLIENT_A),
            )
            connection.execute(
                """
                INSERT INTO audit_events (
                    tenant_id, audit_event_id, correlation_id, actor_id, action,
                    resource_type, resource_id, result_code, occurred_at_ms
                ) VALUES (?, 'audit_committed_catalog', 'corr_committed_catalog', ?,
                          'client.update', 'client', ?, 'updated', 120)
                """,
                (TENANT_A, OWNER_A, CLIENT_A),
            )
            connection.execute(
                """
                INSERT INTO outbox_events (
                    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                    aggregate_version, event_type, payload_json, created_at_ms
                ) VALUES (?, 'outbox_committed_catalog', 'client', ?, 3,
                          'client.updated.v1', '{}', 120)
                """,
                (TENANT_A, CLIENT_A),
            )

        committed_client = connection.execute(
            "SELECT display_name, version FROM clients WHERE tenant_id = ? AND client_id = ?",
            (TENANT_A, CLIENT_A),
        ).fetchone()
        assert (committed_client["display_name"], committed_client["version"]) == (
            "Committed Client",
            3,
        )
        assert connection.execute(
            "SELECT COUNT(*) FROM idempotency_records WHERE tenant_id = ? AND idempotency_key = 'idem_committed_catalog'",
            (TENANT_A,),
        ).fetchone()[0] == 1
        assert connection.execute(
            "SELECT COUNT(*) FROM audit_events WHERE tenant_id = ? AND audit_event_id = 'audit_committed_catalog'",
            (TENANT_A,),
        ).fetchone()[0] == 1
        assert connection.execute(
            "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ? AND outbox_event_id = 'outbox_committed_catalog'",
            (TENANT_A,),
        ).fetchone()[0] == 1
    finally:
        connection.close()


def test_file_backed_restore_shape() -> None:
    with tempfile.TemporaryDirectory() as directory:
        database_path = Path(directory) / "catalog.sqlite3"
        first = open_database(database_path)
        apply_migrations(first)
        seed_catalog(first)
        first.close()

        restored = open_database(database_path)
        try:
            assert restored.execute(
                "SELECT display_name FROM tenants WHERE tenant_id = ?", (TENANT_A,)
            ).fetchone()[0] == "Synthetic Tenant A"
            assert restored.execute("PRAGMA foreign_key_check").fetchall() == []
            assert restored.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        finally:
            restored.close()


def main() -> None:
    test_schema_is_deterministic()
    test_constraints_and_concealment()
    test_optimistic_version_and_mutation_envelope()
    test_file_backed_restore_shape()
    print("D1 catalog schema, isolation, CAS and transaction invariants passed.")


if __name__ == "__main__":
    main()
