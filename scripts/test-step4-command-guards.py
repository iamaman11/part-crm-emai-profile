#!/usr/bin/env python3
"""Prove Step 4 governed command triggers are transaction-fatal and atomic."""

from __future__ import annotations

import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
TENANT = "tenant_01_commands"
OWNER = "actor_owner_commands"
MEMBER = "actor_member_commands"
OWNER_IDENTITY = "identity_owner_commands"
MEMBER_IDENTITY = "identity_member_commands"
CLIENT = "client_01_commands"
PROFILE = "profile_01_commands"


def database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql")):
        connection.executescript(migration.read_text(encoding="utf-8"))
    return connection


def seed(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Commands Tenant', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT,),
    )
    connection.executemany(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, ?, 10)
        """,
        (
            (OWNER_IDENTITY, "access-owner-commands"),
            (MEMBER_IDENTITY, "access-member-commands"),
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
        ) VALUES (?, ?, 'PERSON', 'Commands Client', 'ACTIVE', 1, ?, ?, 20, 20)
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


def expect_abort(operation, fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert fragment in str(error), (fragment, str(error))
    else:
        raise AssertionError(f"expected SQLite abort containing {fragment!r}")


def owner_roles(connection: sqlite3.Connection) -> dict[str, tuple[str, int]]:
    rows = connection.execute(
        "SELECT actor_id, role, version FROM memberships WHERE tenant_id = ?",
        (TENANT,),
    ).fetchall()
    return {row["actor_id"]: (row["role"], row["version"]) for row in rows}


def test_owner_transfer_guard(connection: sqlite3.Connection) -> None:
    expect_abort(
        lambda: connection.execute(
            """
            INSERT INTO owner_transfer_commands (
                tenant_id, command_id, current_owner_actor_id, next_owner_actor_id,
                current_owner_version, next_owner_version, executed_at_ms
            ) VALUES (?, 'command_stale_transfer', ?, ?, 99, 1, 100)
            """,
            (TENANT, OWNER, MEMBER),
        ),
        "owner_transfer_current_owner_mismatch",
    )
    connection.rollback()
    assert owner_roles(connection) == {
        OWNER: ("TENANT_OWNER", 1),
        MEMBER: ("MEMBER", 1),
    }

    try:
        with connection:
            connection.execute(
                """
                INSERT INTO owner_transfer_commands (
                    tenant_id, command_id, current_owner_actor_id, next_owner_actor_id,
                    current_owner_version, next_owner_version, executed_at_ms
                ) VALUES (?, 'command_rollback_transfer', ?, ?, 1, 1, 110)
                """,
                (TENANT, OWNER, MEMBER),
            )
            connection.execute(
                "INSERT INTO audit_events (tenant_id) VALUES (?)",
                (TENANT,),
            )
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("forced envelope failure unexpectedly committed")
    assert owner_roles(connection) == {
        OWNER: ("TENANT_OWNER", 1),
        MEMBER: ("MEMBER", 1),
    }
    assert (
        connection.execute(
            "SELECT COUNT(*) FROM owner_transfer_commands WHERE command_id = 'command_rollback_transfer'"
        ).fetchone()[0]
        == 0
    )

    with connection:
        connection.execute(
            """
            INSERT INTO owner_transfer_commands (
                tenant_id, command_id, current_owner_actor_id, next_owner_actor_id,
                current_owner_version, next_owner_version, executed_at_ms
            ) VALUES (?, 'command_commit_transfer', ?, ?, 1, 1, 120)
            """,
            (TENANT, OWNER, MEMBER),
        )
    assert owner_roles(connection) == {
        OWNER: ("MEMBER", 2),
        MEMBER: ("TENANT_OWNER", 2),
    }


def test_last_owner_and_membership_version(connection: sqlite3.Connection) -> None:
    expect_abort(
        lambda: connection.execute(
            """
            INSERT INTO membership_status_commands (
                tenant_id, command_id, command_actor_id, target_actor_id,
                expected_version, next_status, executed_at_ms
            ) VALUES (?, 'command_remove_last_owner', ?, ?, 2, 'REVOKED', 130)
            """,
            (TENANT, MEMBER, MEMBER),
        ),
        "last_active_owner",
    )
    connection.rollback()
    expect_abort(
        lambda: connection.execute(
            """
            INSERT INTO membership_status_commands (
                tenant_id, command_id, command_actor_id, target_actor_id,
                expected_version, next_status, executed_at_ms
            ) VALUES (?, 'command_stale_member', ?, ?, 99, 'SUSPENDED', 130)
            """,
            (TENANT, MEMBER, OWNER),
        ),
        "membership_status_version_mismatch",
    )
    connection.rollback()


def test_invitation_and_resource_guards(connection: sqlite3.Connection) -> None:
    expect_abort(
        lambda: connection.execute(
            """
            INSERT INTO invitation_create_commands (
                tenant_id, command_id, command_actor_id, invitation_id,
                invited_contact_hmac, expires_at_ms, expected_tenant_version,
                executed_at_ms
            ) VALUES (?, 'command_stale_invite', ?, 'invite_stale_commands',
                      'contact_hmac_stale_commands', 1000, 99, 200)
            """,
            (TENANT, MEMBER),
        ),
        "invitation_create_tenant_version_mismatch",
    )
    connection.rollback()
    with connection:
        connection.execute(
            """
            INSERT INTO invitation_create_commands (
                tenant_id, command_id, command_actor_id, invitation_id,
                invited_contact_hmac, expires_at_ms, expected_tenant_version,
                executed_at_ms
            ) VALUES (?, 'command_invite', ?, 'invite_01_commands',
                      'contact_hmac_valid_commands', 1000, 1, 200)
            """,
            (TENANT, MEMBER),
        )
    assert (
        connection.execute(
            "SELECT status FROM invitations WHERE invitation_id = 'invite_01_commands'"
        ).fetchone()[0]
        == "PENDING"
    )
    assert connection.execute(
        "SELECT version FROM tenants WHERE tenant_id = ?", (TENANT,)
    ).fetchone()[0] == 2

    expect_abort(
        lambda: connection.execute(
            """
            INSERT INTO profile_grant_commands (
                tenant_id, command_id, command_actor_id, target_actor_id,
                profile_id, operation, role, expected_profile_version,
                reason, executed_at_ms
            ) VALUES (?, 'command_stale_profile_grant', ?, ?, ?, 'GRANT',
                      'PROFILE_VIEWER', 99, 'stale version', 210)
            """,
            (TENANT, MEMBER, OWNER, PROFILE),
        ),
        "profile_grant_target_not_active_member",
    )
    connection.rollback()

    with connection:
        connection.execute(
            """
            INSERT INTO profile_assignment_commands (
                tenant_id, command_id, command_actor_id, assignment_id,
                profile_id, client_id, expected_profile_version,
                reason, executed_at_ms
            ) VALUES (?, 'command_assignment', ?, 'assignment_01_commands',
                      ?, ?, 1, 'historical association', 220)
            """,
            (TENANT, MEMBER, PROFILE, CLIENT),
        )
    assert connection.execute(
        "SELECT version FROM browser_profiles WHERE tenant_id = ? AND profile_id = ?",
        (TENANT, PROFILE),
    ).fetchone()[0] == 2
    assert connection.execute(
        "SELECT COUNT(*) FROM profile_grants WHERE tenant_id = ? AND actor_id = ? AND profile_id = ?",
        (TENANT, OWNER, PROFILE),
    ).fetchone()[0] == 0

    with connection:
        connection.execute(
            """
            INSERT INTO profile_grant_commands (
                tenant_id, command_id, command_actor_id, target_actor_id,
                profile_id, operation, role, expected_profile_version,
                reason, executed_at_ms
            ) VALUES (?, 'command_profile_grant', ?, ?, ?, 'GRANT',
                      'PROFILE_OPERATOR', 2, 'explicit authorization', 230)
            """,
            (TENANT, MEMBER, OWNER, PROFILE),
        )
    assert connection.execute(
        "SELECT role FROM profile_grants WHERE tenant_id = ? AND actor_id = ? AND profile_id = ?",
        (TENANT, OWNER, PROFILE),
    ).fetchone()[0] == "PROFILE_OPERATOR"
    assert connection.execute(
        "SELECT version FROM browser_profiles WHERE tenant_id = ? AND profile_id = ?",
        (TENANT, PROFILE),
    ).fetchone()[0] == 3


def main() -> int:
    connection = database()
    try:
        seed(connection)
        test_owner_transfer_guard(connection)
        test_last_owner_and_membership_version(connection)
        test_invitation_and_resource_guards(connection)
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()
    print("Step 4 governed command guards passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
