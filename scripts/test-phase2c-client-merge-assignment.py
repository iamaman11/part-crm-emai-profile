#!/usr/bin/env python3
"""Prove Phase 2C client merge and historical assignment D1 invariants."""

from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path

_SCHEMA = runpy.run_path(str(Path(__file__).with_name("test-d1-schema.py")))
CLIENT_A = _SCHEMA["CLIENT_A"]
MEMBER_A = _SCHEMA["MEMBER_A"]
OWNER_A = _SCHEMA["OWNER_A"]
PROFILE_A = _SCHEMA["PROFILE_A"]
TENANT_A = _SCHEMA["TENANT_A"]
apply_migrations = _SCHEMA["apply_migrations"]
open_database = _SCHEMA["open_database"]
seed_catalog = _SCHEMA["seed_catalog"]

TARGET_A = "client_02_phase2c_a"
TARGET_B = "client_03_phase2c_a"
CONTACT_A = "contact_01_phase2c_merge"


def expect_integrity(operation, fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        if fragment not in str(error):
            raise AssertionError(
                f"expected integrity failure containing {fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError(f"operation unexpectedly bypassed {fragment}")


def seeded_database() -> sqlite3.Connection:
    connection = open_database()
    apply_migrations(connection)
    seed_catalog(connection)
    for client_id, display_name in (
        (TARGET_A, "Phase 2C Target A"),
        (TARGET_B, "Phase 2C Target B"),
    ):
        connection.execute(
            """
            INSERT INTO clients (
                tenant_id, client_id, kind, display_name, status, version,
                created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
            ) VALUES (?, ?, 'PERSON', ?, 'ACTIVE', 1, ?, ?, 35, 35)
            """,
            (TENANT_A, client_id, display_name, OWNER_A, OWNER_A),
        )
    connection.commit()
    return connection


def assignment_command(
    connection: sqlite3.Connection,
    *,
    command_id: str,
    assignment_id: str,
    client_id: str,
    expected_profile_version: int,
    at: int,
    reason: str,
) -> None:
    connection.execute(
        """
        INSERT INTO profile_assignment_commands (
            tenant_id, command_id, command_actor_id, assignment_id,
            profile_id, client_id, expected_profile_version, reason, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT_A,
            command_id,
            OWNER_A,
            assignment_id,
            PROFILE_A,
            client_id,
            expected_profile_version,
            reason,
            at,
        ),
    )


def merge_command(
    connection: sqlite3.Connection,
    *,
    command_id: str,
    source_client_id: str,
    target_client_id: str,
    source_version: int,
    target_version: int,
    at: int,
    reason: str = "deduplicate synthetic clients",
) -> None:
    connection.execute(
        """
        INSERT INTO client_merge_commands (
            tenant_id, command_id, command_actor_id,
            source_client_id, target_client_id,
            expected_source_version, expected_target_version,
            reason, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT_A,
            command_id,
            OWNER_A,
            source_client_id,
            target_client_id,
            source_version,
            target_version,
            reason,
            at,
        ),
    )


def client_row(connection: sqlite3.Connection, client_id: str) -> sqlite3.Row:
    row = connection.execute(
        """
        SELECT status, version, updated_by_actor_id, updated_at_ms
        FROM clients WHERE tenant_id = ? AND client_id = ?
        """,
        (TENANT_A, client_id),
    ).fetchone()
    assert row is not None
    return row


def test_assignment_history_is_one_active_primary_and_one_way() -> None:
    connection = seeded_database()
    assignment_command(
        connection,
        command_id="cmd_phase2c_assign_1",
        assignment_id="assignment_phase2c_1",
        client_id=CLIENT_A,
        expected_profile_version=1,
        at=40,
        reason="initial synthetic assignment",
    )
    connection.commit()

    active = connection.execute(
        """
        SELECT assignment_id, client_id, closed_at_ms
        FROM profile_client_assignments
        WHERE tenant_id = ? AND profile_id = ? AND closed_at_ms IS NULL
        """,
        (TENANT_A, PROFILE_A),
    ).fetchall()
    assert [(row["assignment_id"], row["client_id"]) for row in active] == [
        ("assignment_phase2c_1", CLIENT_A)
    ]

    expect_integrity(
        lambda: assignment_command(
            connection,
            command_id="cmd_phase2c_assign_same",
            assignment_id="assignment_phase2c_same",
            client_id=CLIENT_A,
            expected_profile_version=2,
            at=45,
            reason="same client must be rejected",
        ),
        "profile_assignment_same_client",
    )
    assert connection.execute(
        "SELECT version FROM browser_profiles WHERE tenant_id = ? AND profile_id = ?",
        (TENANT_A, PROFILE_A),
    ).fetchone()["version"] == 2

    assignment_command(
        connection,
        command_id="cmd_phase2c_assign_2",
        assignment_id="assignment_phase2c_2",
        client_id=TARGET_A,
        expected_profile_version=2,
        at=50,
        reason="move profile to target",
    )
    connection.commit()

    history = connection.execute(
        """
        SELECT assignment_id, client_id, assigned_at_ms, closed_at_ms
        FROM profile_client_assignments
        WHERE tenant_id = ? AND profile_id = ?
        ORDER BY assigned_at_ms
        """,
        (TENANT_A, PROFILE_A),
    ).fetchall()
    assert len(history) == 2
    assert history[0]["assignment_id"] == "assignment_phase2c_1"
    assert history[0]["client_id"] == CLIENT_A
    assert history[0]["closed_at_ms"] == 50
    assert history[1]["assignment_id"] == "assignment_phase2c_2"
    assert history[1]["client_id"] == TARGET_A
    assert history[1]["closed_at_ms"] is None

    expect_integrity(
        lambda: connection.execute(
            """
            UPDATE profile_client_assignments
            SET client_id = ?
            WHERE tenant_id = ? AND assignment_id = 'assignment_phase2c_1'
            """,
            (TARGET_B, TENANT_A),
        ),
        "profile_assignment_identity_immutable",
    )
    expect_integrity(
        lambda: connection.execute(
            """
            UPDATE profile_client_assignments
            SET closed_at_ms = NULL
            WHERE tenant_id = ? AND assignment_id = 'assignment_phase2c_1'
            """,
            (TENANT_A,),
        ),
        "profile_assignment_closed_history_immutable",
    )
    expect_integrity(
        lambda: connection.execute(
            """
            DELETE FROM profile_client_assignments
            WHERE tenant_id = ? AND assignment_id = 'assignment_phase2c_1'
            """,
            (TENANT_A,),
        ),
        "profile_assignment_delete_forbidden",
    )
    connection.close()


def test_merge_requires_assignment_reassignment_then_closes_source_capabilities() -> None:
    connection = seeded_database()
    assignment_command(
        connection,
        command_id="cmd_phase2c_assign_before_merge",
        assignment_id="assignment_phase2c_before_merge",
        client_id=CLIENT_A,
        expected_profile_version=1,
        at=40,
        reason="source still owns a profile",
    )
    connection.commit()

    expect_integrity(
        lambda: merge_command(
            connection,
            command_id="cmd_phase2c_merge_blocked",
            source_client_id=CLIENT_A,
            target_client_id=TARGET_A,
            source_version=1,
            target_version=1,
            at=50,
        ),
        "client_merge_active_assignment_requires_reassignment",
    )
    assert client_row(connection, CLIENT_A)["status"] == "ACTIVE"

    assignment_command(
        connection,
        command_id="cmd_phase2c_reassign_for_merge",
        assignment_id="assignment_phase2c_reassigned",
        client_id=TARGET_A,
        expected_profile_version=2,
        at=55,
        reason="explicit reassignment before merge",
    )
    connection.execute(
        """
        INSERT INTO client_grants (
            tenant_id, actor_id, client_id, role,
            granted_by_actor_id, reason, created_at_ms
        ) VALUES (?, ?, ?, 'CLIENT_VIEWER', ?, 'synthetic source grant', 56)
        """,
        (TENANT_A, MEMBER_A, CLIENT_A, OWNER_A),
    )
    connection.execute(
        """
        INSERT INTO client_contact_points (
            tenant_id, client_id, contact_point_id, kind, status,
            normalization_version, protection_version,
            ciphertext, nonce, encryption_key_version,
            exact_lookup_token, lookup_key_version,
            created_by_actor_id, updated_by_actor_id,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'EMAIL', 'ACTIVE', 1, 1, ?, ?, 1, ?, 1, ?, ?, 56, 56)
        """,
        (
            TENANT_A,
            CLIENT_A,
            CONTACT_A,
            bytes([0xA5]) * 48,
            bytes([0x5A]) * 24,
            bytes(range(32)),
            OWNER_A,
            OWNER_A,
        ),
    )
    connection.commit()

    merge_command(
        connection,
        command_id="cmd_phase2c_merge_success",
        source_client_id=CLIENT_A,
        target_client_id=TARGET_A,
        source_version=1,
        target_version=1,
        at=60,
    )
    connection.commit()

    source = client_row(connection, CLIENT_A)
    target = client_row(connection, TARGET_A)
    assert source["status"] == "MERGED"
    assert source["version"] == 2
    assert source["updated_by_actor_id"] == OWNER_A
    assert source["updated_at_ms"] == 60
    assert target["status"] == "ACTIVE"
    assert target["version"] == 1

    merge = connection.execute(
        """
        SELECT target_client_id, source_version_before, source_version_after,
               target_version_observed, reason
        FROM client_merges
        WHERE tenant_id = ? AND source_client_id = ?
        """,
        (TENANT_A, CLIENT_A),
    ).fetchone()
    assert merge is not None
    assert merge["target_client_id"] == TARGET_A
    assert merge["source_version_before"] == 1
    assert merge["source_version_after"] == 2
    assert merge["target_version_observed"] == 1
    assert merge["reason"] == "deduplicate synthetic clients"

    contact = connection.execute(
        """
        SELECT status, updated_at_ms FROM client_contact_points
        WHERE tenant_id = ? AND contact_point_id = ?
        """,
        (TENANT_A, CONTACT_A),
    ).fetchone()
    assert contact is not None
    assert contact["status"] == "ARCHIVED"
    assert contact["updated_at_ms"] == 60
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM client_grants WHERE tenant_id = ? AND client_id = ?",
        (TENANT_A, CLIENT_A),
    ).fetchone()["value"] == 0
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM client_grants WHERE tenant_id = ? AND client_id = ?",
        (TENANT_A, TARGET_A),
    ).fetchone()["value"] == 0

    expect_integrity(
        lambda: connection.execute(
            """
            UPDATE clients SET status = 'ACTIVE'
            WHERE tenant_id = ? AND client_id = ?
            """,
            (TENANT_A, CLIENT_A),
        ),
        "client_merged_source_immutable",
    )
    expect_integrity(
        lambda: connection.execute(
            "DELETE FROM client_merges WHERE tenant_id = ? AND source_client_id = ?",
            (TENANT_A, CLIENT_A),
        ),
        "client_merge_record_delete_forbidden",
    )
    connection.close()


def test_merge_version_failure_and_downstream_failure_roll_back_everything() -> None:
    connection = seeded_database()
    expect_integrity(
        lambda: merge_command(
            connection,
            command_id="cmd_phase2c_bad_version",
            source_client_id=CLIENT_A,
            target_client_id=TARGET_A,
            source_version=99,
            target_version=1,
            at=50,
        ),
        "client_merge_source_version_or_state_mismatch",
    )
    assert client_row(connection, CLIENT_A)["status"] == "ACTIVE"
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM client_merges WHERE tenant_id = ?",
        (TENANT_A,),
    ).fetchone()["value"] == 0

    connection.execute(
        """
        INSERT INTO idempotency_records (
            tenant_id, actor_id, idempotency_key, command_name, request_digest,
            result_code, result_reference, created_at_ms, expires_at_ms
        ) VALUES (?, ?, 'idem_phase2c_duplicate', 'synthetic.existing', 'digest-existing',
                  'existing', NULL, 20, 1000)
        """,
        (TENANT_A, OWNER_A),
    )
    connection.commit()

    connection.execute("BEGIN")
    try:
        merge_command(
            connection,
            command_id="cmd_phase2c_rollback",
            source_client_id=CLIENT_A,
            target_client_id=TARGET_A,
            source_version=1,
            target_version=1,
            at=60,
        )
        assert client_row(connection, CLIENT_A)["status"] == "MERGED"
        connection.execute(
            """
            INSERT INTO idempotency_records (
                tenant_id, actor_id, idempotency_key, command_name, request_digest,
                result_code, result_reference, created_at_ms, expires_at_ms
            ) VALUES (?, ?, 'idem_phase2c_duplicate', 'client.merge', 'digest-merge',
                      'merged', ?, 60, 1000)
            """,
            (TENANT_A, OWNER_A, CLIENT_A),
        )
    except sqlite3.IntegrityError:
        connection.rollback()
    else:
        raise AssertionError("downstream duplicate unexpectedly committed")

    source = client_row(connection, CLIENT_A)
    assert source["status"] == "ACTIVE"
    assert source["version"] == 1
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM client_merge_commands WHERE tenant_id = ?",
        (TENANT_A,),
    ).fetchone()["value"] == 0
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM client_merges WHERE tenant_id = ?",
        (TENANT_A,),
    ).fetchone()["value"] == 0
    connection.close()


def main() -> int:
    test_assignment_history_is_one_active_primary_and_one_way()
    test_merge_requires_assignment_reassignment_then_closes_source_capabilities()
    test_merge_version_failure_and_downstream_failure_roll_back_everything()
    print("Phase 2C client merge and assignment D1 invariants are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
