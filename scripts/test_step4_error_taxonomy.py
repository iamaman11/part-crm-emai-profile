#!/usr/bin/env python3
"""Prove legacy governed commands distinguish missing aggregates from stale versions."""

from __future__ import annotations

import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
TENANT = "tenant_error_taxonomy"
OWNER = "actor_owner_error_taxonomy"
MEMBER = "actor_member_error_taxonomy"
OWNER_IDENTITY = "identity_owner_error_taxonomy"
MEMBER_IDENTITY = "identity_member_error_taxonomy"
PROFILE = "profile_error_taxonomy"
CLIENT = "client_error_taxonomy"


def database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql")):
        connection.executescript(migration.read_text(encoding="utf-8"))
    return connection


def seed(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Error Taxonomy Tenant', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT,),
    )
    connection.executemany(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, ?, 10)
        """,
        (
            (OWNER_IDENTITY, "access-owner-error-taxonomy"),
            (MEMBER_IDENTITY, "access-member-error-taxonomy"),
        ),
    )
    connection.executemany(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, 'ACTIVE', 1, 10, 10)
        """,
        (
            (TENANT, OWNER, OWNER_IDENTITY, "TENANT_OWNER"),
            (TENANT, MEMBER, MEMBER_IDENTITY, "MEMBER"),
        ),
    )
    connection.execute(
        """
        INSERT INTO clients (
            tenant_id, client_id, kind, display_name, status, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'PERSON', 'Error Taxonomy Client', 'ACTIVE', 1, ?, ?, 20, 20)
        """,
        (TENANT, CLIENT, OWNER, OWNER),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, version, created_by_actor_id,
            updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', 1, ?, ?, 20, 20)
        """,
        (TENANT, PROFILE, OWNER, OWNER),
    )
    connection.commit()


def expect_abort(connection: sqlite3.Connection, sql: str, params: tuple, expected: str) -> None:
    try:
        connection.execute(sql, params)
    except sqlite3.IntegrityError as error:
        assert expected in str(error), (expected, str(error))
        connection.rollback()
    else:
        raise AssertionError(f"expected SQLite abort containing {expected!r}")


def test_owner_transfer_taxonomy(connection: sqlite3.Connection) -> None:
    sql = """
        INSERT INTO owner_transfer_commands (
            tenant_id, command_id, current_owner_actor_id, next_owner_actor_id,
            current_owner_version, next_owner_version, executed_at_ms
        ) VALUES (?, ?, ?, ?, 1, ?, 100)
    """
    expect_abort(
        connection,
        sql,
        (TENANT, "owner_missing", OWNER, "actor_missing_error_taxonomy", 1),
        "owner_transfer_successor_mismatch",
    )
    expect_abort(
        connection,
        sql,
        (TENANT, "owner_stale", OWNER, MEMBER, 99),
        "owner_transfer_successor_version_mismatch",
    )


def test_membership_taxonomy(connection: sqlite3.Connection) -> None:
    sql = """
        INSERT INTO membership_status_commands (
            tenant_id, command_id, command_actor_id, target_actor_id,
            expected_version, next_status, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, 'SUSPENDED', 110)
    """
    expect_abort(
        connection,
        sql,
        (TENANT, "membership_missing", OWNER, "actor_missing_error_taxonomy", 1),
        "membership_status_target_missing",
    )
    expect_abort(
        connection,
        sql,
        (TENANT, "membership_stale", OWNER, MEMBER, 99),
        "membership_status_version_mismatch",
    )


def test_profile_assignment_taxonomy(connection: sqlite3.Connection) -> None:
    sql = """
        INSERT INTO profile_assignment_commands (
            tenant_id, command_id, command_actor_id, assignment_id,
            profile_id, client_id, expected_profile_version, reason, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, 'taxonomy test', 120)
    """
    expect_abort(
        connection,
        sql,
        (
            TENANT,
            "assignment_missing",
            OWNER,
            "assignment_missing_error_taxonomy",
            "profile_missing_error_taxonomy",
            CLIENT,
            1,
        ),
        "profile_assignment_profile_missing",
    )
    expect_abort(
        connection,
        sql,
        (
            TENANT,
            "assignment_stale",
            OWNER,
            "assignment_stale_error_taxonomy",
            PROFILE,
            CLIENT,
            99,
        ),
        "profile_assignment_version_mismatch",
    )


def test_profile_grant_taxonomy(connection: sqlite3.Connection) -> None:
    sql = """
        INSERT INTO profile_grant_commands (
            tenant_id, command_id, command_actor_id, target_actor_id,
            profile_id, operation, role, expected_profile_version, reason, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, 'GRANT', 'PROFILE_VIEWER', ?, 'taxonomy test', 130)
    """
    expect_abort(
        connection,
        sql,
        (TENANT, "profile_grant_missing", OWNER, MEMBER, "profile_missing_error_taxonomy", 1),
        "profile_grant_profile_missing",
    )
    expect_abort(
        connection,
        sql,
        (TENANT, "profile_grant_stale", OWNER, MEMBER, PROFILE, 99),
        "profile_grant_version_mismatch",
    )


def test_client_grant_taxonomy(connection: sqlite3.Connection) -> None:
    sql = """
        INSERT INTO client_grant_commands (
            tenant_id, command_id, command_actor_id, target_actor_id,
            client_id, operation, role, expected_client_version, reason, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, 'GRANT', 'CLIENT_VIEWER', ?, 'taxonomy test', 140)
    """
    expect_abort(
        connection,
        sql,
        (TENANT, "client_grant_missing", OWNER, MEMBER, "client_missing_error_taxonomy", 1),
        "client_grant_client_missing",
    )
    expect_abort(
        connection,
        sql,
        (TENANT, "client_grant_stale", OWNER, MEMBER, CLIENT, 99),
        "client_grant_version_mismatch",
    )


def assert_no_partial_mutation(connection: sqlite3.Connection) -> None:
    for table in (
        "owner_transfer_commands",
        "membership_status_commands",
        "profile_assignment_commands",
        "profile_grant_commands",
        "client_grant_commands",
    ):
        assert connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0] == 0, table
    assert connection.execute(
        "SELECT version FROM memberships WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, MEMBER),
    ).fetchone()[0] == 1
    assert connection.execute(
        "SELECT version FROM browser_profiles WHERE tenant_id = ? AND profile_id = ?",
        (TENANT, PROFILE),
    ).fetchone()[0] == 1
    assert connection.execute(
        "SELECT version FROM clients WHERE tenant_id = ? AND client_id = ?",
        (TENANT, CLIENT),
    ).fetchone()[0] == 1


def main() -> int:
    connection = database()
    try:
        seed(connection)
        test_owner_transfer_taxonomy(connection)
        test_membership_taxonomy(connection)
        test_profile_assignment_taxonomy(connection)
        test_profile_grant_taxonomy(connection)
        test_client_grant_taxonomy(connection)
        assert_no_partial_mutation(connection)
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()
    print("Step 4 governed error taxonomy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
