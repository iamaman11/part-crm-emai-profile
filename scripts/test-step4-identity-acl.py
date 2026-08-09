#!/usr/bin/env python3
"""Prove Repository Step 4 identity, owner-transfer and ACL invariants."""

from __future__ import annotations

import argparse
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"

TENANT = "tenant_01_step4"
FOREIGN_TENANT = "tenant_02_step4"
OWNER = "actor_owner_step4"
MEMBER = "actor_member_step4"
INVITED_MEMBER = "actor_invited_step4"
OWNER_IDENTITY = "identity_owner_step4"
MEMBER_IDENTITY = "identity_member_step4"
INVITED_IDENTITY = "identity_invited_step4"
FOREIGN_OWNER = "actor_foreign_step4"
FOREIGN_IDENTITY = "identity_foreign_step4"
CLIENT = "client_01_step4"
FOREIGN_CLIENT = "client_02_step4"
PROFILE = "profile_01_step4"
INVITATION = "invite_01_step4"


def open_database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.row_factory = sqlite3.Row
    for migration in sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql")):
        connection.executescript(migration.read_text(encoding="utf-8"))
    return connection


def seed(connection: sqlite3.Connection) -> None:
    for tenant_id, display_name in (
        (TENANT, "Step 4 Tenant"),
        (FOREIGN_TENANT, "Foreign Step 4 Tenant"),
    ):
        connection.execute(
            """
            INSERT INTO tenants (
                tenant_id, display_name, status, version, created_at_ms, updated_at_ms
            ) VALUES (?, ?, 'ACTIVE', 1, 10, 10)
            """,
            (tenant_id, display_name),
        )

    identities = (
        (OWNER_IDENTITY, "access-owner-step4"),
        (MEMBER_IDENTITY, "access-member-step4"),
        (FOREIGN_IDENTITY, "access-foreign-step4"),
    )
    connection.executemany(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, ?, 10)
        """,
        identities,
    )
    memberships = (
        (TENANT, OWNER, OWNER_IDENTITY, "TENANT_OWNER", "ACTIVE"),
        (TENANT, MEMBER, MEMBER_IDENTITY, "MEMBER", "ACTIVE"),
        (FOREIGN_TENANT, FOREIGN_OWNER, FOREIGN_IDENTITY, "TENANT_OWNER", "ACTIVE"),
    )
    connection.executemany(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, 1, 10, 10)
        """,
        memberships,
    )
    connection.execute(
        """
        INSERT INTO clients (
            tenant_id, client_id, kind, display_name, status, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'PERSON', 'Step 4 Client', 'ACTIVE', 1, ?, ?, 20, 20)
        """,
        (TENANT, CLIENT, OWNER, OWNER),
    )
    connection.execute(
        """
        INSERT INTO clients (
            tenant_id, client_id, kind, display_name, status, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'PERSON', 'Foreign Step 4 Client', 'ACTIVE', 1, ?, ?, 20, 20)
        """,
        (FOREIGN_TENANT, FOREIGN_CLIENT, FOREIGN_OWNER, FOREIGN_OWNER),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, version, created_by_actor_id,
            updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', 1, ?, ?, 30, 30)
        """,
        (TENANT, PROFILE, OWNER, OWNER),
    )
    connection.execute(
        """
        INSERT INTO profile_assignment_commands (
            tenant_id, command_id, command_actor_id, assignment_id,
            profile_id, client_id, expected_profile_version, reason, executed_at_ms
        ) VALUES (?, 'command_assignment_01_step4', ?, 'assignment_01_step4',
                  ?, ?, 1, 'historical assignment only', 40)
        """,
        (TENANT, OWNER, PROFILE, CLIENT),
    )
    connection.commit()


def expect_integrity_error(operation, expected_fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        if expected_fragment not in str(error):
            raise AssertionError(
                f"expected integrity error containing {expected_fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError("operation unexpectedly satisfied a required invariant")


def test_invitation_acceptance(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO invitations (
            tenant_id, invitation_id, invited_contact_hmac, intended_role,
            status, expires_at_ms, created_by_actor_id, created_at_ms
        ) VALUES (?, ?, 'contact_hmac_step4_valid', 'MEMBER', 'PENDING', 500, ?, 100)
        """,
        (TENANT, INVITATION, OWNER),
    )
    connection.commit()

    with connection:
        connection.execute(
            """
            INSERT INTO identities (identity_id, access_subject, created_at_ms)
            VALUES (?, 'access-invited-step4', 200)
            """,
            (INVITED_IDENTITY,),
        )
        connection.execute(
            """
            INSERT INTO memberships (
                tenant_id, actor_id, identity_id, role, status, version,
                created_at_ms, updated_at_ms
            ) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, 200, 200)
            """,
            (TENANT, INVITED_MEMBER, INVITED_IDENTITY),
        )
        connection.execute(
            """
            INSERT INTO invitation_acceptances (
                tenant_id, invitation_id, identity_id, actor_id, accepted_at_ms
            ) VALUES (?, ?, ?, ?, 200)
            """,
            (TENANT, INVITATION, INVITED_IDENTITY, INVITED_MEMBER),
        )

    status = connection.execute(
        "SELECT status FROM invitations WHERE tenant_id = ? AND invitation_id = ?",
        (TENANT, INVITATION),
    ).fetchone()[0]
    assert status == "ACCEPTED"

    connection.execute(
        """
        INSERT INTO invitations (
            tenant_id, invitation_id, invited_contact_hmac, intended_role,
            status, expires_at_ms, created_by_actor_id, created_at_ms
        ) VALUES (?, 'invite_expired_step4', 'contact_hmac_step4_expired',
                  'MEMBER', 'PENDING', 250, ?, 100)
        """,
        (TENANT, OWNER),
    )
    connection.commit()
    try:
        with connection:
            connection.execute(
                """
                INSERT INTO identities (identity_id, access_subject, created_at_ms)
                VALUES ('identity_expired_step4', 'access-expired-step4', 300)
                """
            )
            connection.execute(
                """
                INSERT INTO memberships (
                    tenant_id, actor_id, identity_id, role, status, version,
                    created_at_ms, updated_at_ms
                ) VALUES (?, 'actor_expired_step4', 'identity_expired_step4',
                          'MEMBER', 'ACTIVE', 1, 300, 300)
                """,
                (TENANT,),
            )
            connection.execute(
                """
                INSERT INTO invitation_acceptances (
                    tenant_id, invitation_id, identity_id, actor_id, accepted_at_ms
                ) VALUES (?, 'invite_expired_step4', 'identity_expired_step4',
                          'actor_expired_step4', 300)
                """,
                (TENANT,),
            )
    except sqlite3.IntegrityError as error:
        assert "invitation_not_pending_or_expired" in str(error)
    else:
        raise AssertionError("expired invitation acceptance unexpectedly committed")

    assert (
        connection.execute(
            "SELECT COUNT(*) FROM memberships WHERE actor_id = 'actor_expired_step4'"
        ).fetchone()[0]
        == 0
    )
    assert (
        connection.execute(
            "SELECT COUNT(*) FROM identities WHERE identity_id = 'identity_expired_step4'"
        ).fetchone()[0]
        == 0
    )


def test_owner_transfer_and_last_owner(connection: sqlite3.Connection) -> None:
    expect_integrity_error(
        lambda: connection.execute(
            "UPDATE memberships SET status = 'SUSPENDED' WHERE tenant_id = ? AND actor_id = ?",
            (TENANT, OWNER),
        ),
        "last_active_owner",
    )
    connection.rollback()

    try:
        with connection:
            demoted = connection.execute(
                """
                UPDATE memberships
                SET role = 'MEMBER', version = version + 1, updated_at_ms = 400
                WHERE tenant_id = ? AND actor_id = ? AND role = 'TENANT_OWNER'
                  AND status = 'ACTIVE' AND version = 1
                """,
                (TENANT, OWNER),
            )
            assert demoted.rowcount == 1
            raise sqlite3.IntegrityError("forced_transfer_failure")
    except sqlite3.IntegrityError as error:
        assert "forced_transfer_failure" in str(error)
    else:
        raise AssertionError("forced owner-transfer failure unexpectedly committed")

    still_owner = connection.execute(
        "SELECT role, version FROM memberships WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, OWNER),
    ).fetchone()
    assert (still_owner["role"], still_owner["version"]) == ("TENANT_OWNER", 1)

    with connection:
        demoted = connection.execute(
            """
            UPDATE memberships
            SET role = 'MEMBER', version = version + 1, updated_at_ms = 410
            WHERE tenant_id = ? AND actor_id = ? AND role = 'TENANT_OWNER'
              AND status = 'ACTIVE' AND version = 1
            """,
            (TENANT, OWNER),
        )
        assert demoted.rowcount == 1
        promoted = connection.execute(
            """
            UPDATE memberships
            SET role = 'TENANT_OWNER', version = version + 1, updated_at_ms = 410
            WHERE tenant_id = ? AND actor_id = ? AND role = 'MEMBER'
              AND status = 'ACTIVE' AND version = 1
            """,
            (TENANT, MEMBER),
        )
        assert promoted.rowcount == 1
        connection.execute(
            """
            INSERT INTO idempotency_records (
                tenant_id, actor_id, idempotency_key, command_name, request_digest,
                result_code, result_reference, created_at_ms, expires_at_ms
            ) VALUES (?, ?, 'idem_transfer_step4', 'membership.owner_transfer',
                      '0123456789abcdef', 'transferred', ?, 410, 1000)
            """,
            (TENANT, OWNER, MEMBER),
        )
        connection.execute(
            """
            INSERT INTO audit_events (
                tenant_id, audit_event_id, correlation_id, actor_id, action,
                resource_type, resource_id, result_code, occurred_at_ms
            ) VALUES (?, 'audit_transfer_step4', 'corr_transfer_step4', ?,
                      'membership.owner_transfer', 'membership', ?, 'transferred', 410)
            """,
            (TENANT, OWNER, MEMBER),
        )
        connection.execute(
            """
            INSERT INTO outbox_events (
                tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                aggregate_version, event_type, payload_json, created_at_ms
            ) VALUES (?, 'outbox_transfer_step4', 'membership', ?, 2,
                      'membership.owner_transferred.v1', '{}', 410)
            """,
            (TENANT, MEMBER),
        )

    owners = connection.execute(
        """
        SELECT actor_id FROM memberships
        WHERE tenant_id = ? AND role = 'TENANT_OWNER' AND status = 'ACTIVE'
        """,
        (TENANT,),
    ).fetchall()
    assert [row["actor_id"] for row in owners] == [MEMBER]


def authorized_profile(
    connection: sqlite3.Connection,
    tenant_id: str,
    actor_id: str,
    profile_id: str,
) -> sqlite3.Row | None:
    return connection.execute(
        """
        SELECT profile.profile_id
        FROM browser_profiles AS profile
        JOIN memberships AS membership
          ON membership.tenant_id = profile.tenant_id
         AND membership.actor_id = ?
         AND membership.status = 'ACTIVE'
        LEFT JOIN profile_grants AS grant
          ON grant.tenant_id = profile.tenant_id
         AND grant.actor_id = membership.actor_id
         AND grant.profile_id = profile.profile_id
        WHERE profile.tenant_id = ?
          AND profile.profile_id = ?
          AND (membership.role = 'TENANT_OWNER' OR grant.profile_id IS NOT NULL)
        """,
        (actor_id, tenant_id, profile_id),
    ).fetchone()


def test_endpoint_acl_and_concealment(
    connection: sqlite3.Connection, negative_fixture: Path | None
) -> None:
    assert authorized_profile(connection, TENANT, OWNER, PROFILE) is None
    assert authorized_profile(connection, TENANT, MEMBER, PROFILE) is not None

    connection.execute(
        "UPDATE memberships SET role = 'MEMBER' WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, MEMBER),
    )
    connection.execute(
        "UPDATE memberships SET role = 'TENANT_OWNER' WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, OWNER),
    )
    connection.commit()

    assert authorized_profile(connection, TENANT, MEMBER, PROFILE) is None
    connection.execute(
        """
        INSERT INTO profile_grants (
            tenant_id, actor_id, profile_id, role, granted_by_actor_id,
            reason, created_at_ms
        ) VALUES (?, ?, ?, 'PROFILE_VIEWER', ?, 'explicit test grant', 500)
        """,
        (TENANT, MEMBER, PROFILE, OWNER),
    )
    connection.commit()
    assert authorized_profile(connection, TENANT, MEMBER, PROFILE) is not None

    connection.execute(
        "UPDATE memberships SET status = 'SUSPENDED' WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, MEMBER),
    )
    connection.commit()
    assert authorized_profile(connection, TENANT, MEMBER, PROFILE) is None
    connection.execute(
        "UPDATE memberships SET status = 'REVOKED' WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, MEMBER),
    )
    connection.commit()
    assert authorized_profile(connection, TENANT, MEMBER, PROFILE) is None

    foreign = connection.execute(
        "SELECT client_id FROM clients WHERE tenant_id = ? AND client_id = ?",
        (TENANT, FOREIGN_CLIENT),
    ).fetchone()
    missing = connection.execute(
        "SELECT client_id FROM clients WHERE tenant_id = ? AND client_id = 'client_missing_step4'",
        (TENANT,),
    ).fetchone()
    assert foreign is None and missing is None

    if negative_fixture is not None:
        connection.executescript(negative_fixture.read_text(encoding="utf-8"))
        leaked = connection.execute(
            """
            SELECT profile_id FROM fixture_profile_access
            WHERE tenant_id = ? AND actor_id = ? AND profile_id = ?
            """,
            (TENANT, MEMBER, PROFILE),
        ).fetchone()
        if leaked is not None:
            raise AssertionError("assignment-only fixture incorrectly grants profile access")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--negative-fixture", type=Path)
    args = parser.parse_args()

    connection = open_database()
    try:
        seed(connection)
        test_invitation_acceptance(connection)
        test_owner_transfer_and_last_owner(connection)
        test_endpoint_acl_and_concealment(connection, args.negative_fixture)
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()

    print("Step 4 identity, owner-transfer and ACL invariants passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
