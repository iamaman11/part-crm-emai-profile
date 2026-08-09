#!/usr/bin/env python3
"""Prove Phase 2B protected client-contact D1 invariants and failure ordering."""

from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path

_SCHEMA = runpy.run_path(str(Path(__file__).with_name("test-d1-schema.py")))
CLIENT_A = _SCHEMA["CLIENT_A"]
CLIENT_B = _SCHEMA["CLIENT_B"]
OWNER_A = _SCHEMA["OWNER_A"]
OWNER_B = _SCHEMA["OWNER_B"]
TENANT_A = _SCHEMA["TENANT_A"]
TENANT_B = _SCHEMA["TENANT_B"]
apply_migrations = _SCHEMA["apply_migrations"]
open_database = _SCHEMA["open_database"]
seed_catalog = _SCHEMA["seed_catalog"]

CONTACT_A = "contact_01_phase2b_a"
CONTACT_B = "contact_01_phase2b_b"
TOKEN = bytes(range(32))
CIPHERTEXT = bytes([0xA5]) * 48
NONCE = bytes([0x5A]) * 24


def seeded_database() -> sqlite3.Connection:
    connection = open_database()
    apply_migrations(connection)
    seed_catalog(connection)
    connection.commit()
    return connection


def expect_integrity(operation, fragment: str | None = None) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        if fragment is not None and fragment not in str(error):
            raise AssertionError(
                f"expected integrity failure containing {fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError("operation unexpectedly bypassed protected-contact invariant")


def insert_contact(
    connection: sqlite3.Connection,
    *,
    tenant_id: str = TENANT_A,
    client_id: str = CLIENT_A,
    contact_point_id: str = CONTACT_A,
    actor_id: str = OWNER_A,
    status: str = "ACTIVE",
    token: bytes = TOKEN,
    encryption_key_version: int = 1,
    lookup_key_version: int = 1,
    now: int = 100,
) -> None:
    connection.execute(
        """
        INSERT INTO client_contact_points (
            tenant_id, client_id, contact_point_id, kind, status,
            normalization_version, protection_version,
            ciphertext, nonce, encryption_key_version,
            exact_lookup_token, lookup_key_version,
            created_by_actor_id, updated_by_actor_id,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'EMAIL', ?, 1, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            tenant_id,
            client_id,
            contact_point_id,
            status,
            CIPHERTEXT,
            NONCE,
            encryption_key_version,
            token,
            lookup_key_version,
            actor_id,
            actor_id,
            now,
            now,
        ),
    )


def client_version(connection: sqlite3.Connection) -> int:
    row = connection.execute(
        "SELECT version FROM clients WHERE tenant_id = ? AND client_id = ?",
        (TENANT_A, CLIENT_A),
    ).fetchone()
    assert row is not None
    return int(row["version"])


def test_schema_has_no_plaintext_contact_column() -> None:
    connection = seeded_database()
    columns = {
        row["name"]: row["type"].upper()
        for row in connection.execute("PRAGMA table_info(client_contact_points)").fetchall()
    }
    forbidden = {"value", "display_value", "email", "phone", "url", "normalized_value"}
    overlap = forbidden.intersection(columns)
    if overlap:
        raise AssertionError(f"plaintext-capable contact columns are forbidden: {sorted(overlap)}")
    if columns.get("ciphertext") != "BLOB" or columns.get("exact_lookup_token") != "BLOB":
        raise AssertionError("ciphertext and exact lookup token must use BLOB storage")

    command_columns = {
        row["name"]
        for row in connection.execute("PRAGMA table_info(client_contact_commands)").fetchall()
    }
    if {"ciphertext", "nonce", "exact_lookup_token"}.intersection(command_columns):
        raise AssertionError("command journal must not duplicate protected contact payload")
    connection.close()


def test_protected_shape_and_versions_fail_closed() -> None:
    connection = seeded_database()
    insert_contact(connection)
    row = connection.execute(
        """
        SELECT length(ciphertext) AS cipher_len,
               length(nonce) AS nonce_len,
               length(exact_lookup_token) AS token_len,
               encryption_key_version,
               lookup_key_version,
               normalization_version,
               protection_version
        FROM client_contact_points
        WHERE tenant_id = ? AND contact_point_id = ?
        """,
        (TENANT_A, CONTACT_A),
    ).fetchone()
    assert row is not None
    assert row["cipher_len"] == len(CIPHERTEXT)
    assert row["nonce_len"] == len(NONCE)
    assert row["token_len"] == 32
    assert row["encryption_key_version"] == 1
    assert row["lookup_key_version"] == 1
    assert row["normalization_version"] == 1
    assert row["protection_version"] == 1

    expect_integrity(
        lambda: insert_contact(
            connection,
            contact_point_id="contact_02_phase2b_bad_token",
            token=b"short",
        ),
        "CHECK constraint failed",
    )
    expect_integrity(
        lambda: insert_contact(
            connection,
            contact_point_id="contact_03_phase2b_bad_key",
            encryption_key_version=0,
        ),
        "CHECK constraint failed",
    )
    connection.close()


def test_tenant_and_client_scope_are_structural() -> None:
    connection = seeded_database()
    insert_contact(connection)
    expect_integrity(
        lambda: insert_contact(
            connection,
            tenant_id=TENANT_B,
            client_id=CLIENT_A,
            contact_point_id=CONTACT_B,
            actor_id=OWNER_B,
        ),
        "client_contact_client_not_active",
    )

    # The same opaque token may exist in different tenants because cryptographic tenant
    # separation is provided by the key/input domain, while every database lookup is
    # independently forced to include tenant_id.
    insert_contact(
        connection,
        tenant_id=TENANT_B,
        client_id=CLIENT_B,
        contact_point_id=CONTACT_B,
        actor_id=OWNER_B,
        token=TOKEN,
    )
    rows = connection.execute(
        """
        SELECT tenant_id, contact_point_id
        FROM client_contact_points
        WHERE tenant_id = ?
          AND kind = 'EMAIL'
          AND normalization_version = 1
          AND lookup_key_version = 1
          AND exact_lookup_token = ?
          AND status = 'ACTIVE'
        """,
        (TENANT_A, TOKEN),
    ).fetchall()
    assert [(row["tenant_id"], row["contact_point_id"]) for row in rows] == [
        (TENANT_A, CONTACT_A)
    ]
    connection.close()


def test_exact_lookup_is_index_backed() -> None:
    connection = seeded_database()
    plan = connection.execute(
        """
        EXPLAIN QUERY PLAN
        SELECT contact_point_id
        FROM client_contact_points
        WHERE tenant_id = ?
          AND kind = 'EMAIL'
          AND normalization_version = 1
          AND lookup_key_version = 1
          AND exact_lookup_token = ?
          AND status = 'ACTIVE'
        """,
        (TENANT_A, TOKEN),
    ).fetchall()
    details = "\n".join(str(row["detail"]) for row in plan)
    if "client_contact_exact_lookup" not in details:
        raise AssertionError(f"exact contact lookup is not index-backed:\n{details}")
    connection.close()


def test_archival_is_one_way_and_delete_is_forbidden() -> None:
    connection = seeded_database()
    insert_contact(connection)
    connection.execute(
        """
        UPDATE client_contact_points
        SET status = 'ARCHIVED', updated_at_ms = 110
        WHERE tenant_id = ? AND contact_point_id = ?
        """,
        (TENANT_A, CONTACT_A),
    )
    expect_integrity(
        lambda: connection.execute(
            """
            UPDATE client_contact_points
            SET status = 'ACTIVE', updated_at_ms = 120
            WHERE tenant_id = ? AND contact_point_id = ?
            """,
            (TENANT_A, CONTACT_A),
        ),
        "client_contact_archived_immutable",
    )
    expect_integrity(
        lambda: connection.execute(
            "DELETE FROM client_contact_points WHERE tenant_id = ? AND contact_point_id = ?",
            (TENANT_A, CONTACT_A),
        ),
        "client_contact_delete_forbidden",
    )
    connection.close()


def test_active_contact_requires_active_client() -> None:
    connection = seeded_database()
    connection.execute(
        "UPDATE clients SET status = 'ARCHIVED', version = version + 1 WHERE tenant_id = ? AND client_id = ?",
        (TENANT_A, CLIENT_A),
    )
    expect_integrity(
        lambda: insert_contact(
            connection,
            contact_point_id="contact_04_phase2b_archived_client",
        ),
        "client_contact_client_not_active",
    )
    connection.close()


def test_contact_guard_failure_rolls_back_client_version_and_command_intent() -> None:
    connection = seeded_database()
    assert client_version(connection) == 1
    connection.execute("BEGIN")
    try:
        connection.execute(
            """
            INSERT INTO client_contact_commands (
                tenant_id, command_id, command_actor_id, client_id, contact_point_id,
                operation, kind, expected_client_version, executed_at_ms
            ) VALUES (?, 'cmd_phase2b_rollback', ?, ?, ?, 'UPSERT', 'EMAIL', 1, 200)
            """,
            (TENANT_A, OWNER_A, CLIENT_A, CONTACT_A),
        )
        assert client_version(connection) == 2
        insert_contact(connection, token=b"invalid", now=200)
    except sqlite3.IntegrityError as error:
        if "CHECK constraint failed" not in str(error):
            raise
        connection.rollback()
    else:
        raise AssertionError("invalid protected value unexpectedly committed")

    assert client_version(connection) == 1
    assert connection.execute(
        "SELECT 1 FROM client_contact_commands WHERE tenant_id = ? AND command_id = 'cmd_phase2b_rollback'",
        (TENANT_A,),
    ).fetchone() is None
    assert connection.execute(
        "SELECT 1 FROM client_contact_points WHERE tenant_id = ? AND contact_point_id = ?",
        (TENANT_A, CONTACT_A),
    ).fetchone() is None
    connection.close()


def test_full_contact_mutation_commits_one_atomic_evidence_set() -> None:
    connection = seeded_database()
    connection.execute("BEGIN")
    connection.execute(
        """
        INSERT INTO client_contact_commands (
            tenant_id, command_id, command_actor_id, client_id, contact_point_id,
            operation, kind, expected_client_version, executed_at_ms
        ) VALUES (?, 'cmd_phase2b_success', ?, ?, ?, 'UPSERT', 'EMAIL', 1, 200)
        """,
        (TENANT_A, OWNER_A, CLIENT_A, CONTACT_A),
    )
    insert_contact(connection, now=200)
    connection.execute(
        """
        INSERT INTO idempotency_records (
            tenant_id, actor_id, idempotency_key, command_name, request_digest,
            result_code, result_reference, created_at_ms, expires_at_ms
        ) VALUES (?, ?, 'idem_phase2b_success', 'client.contact_upsert', 'digest_phase2b_success',
                  'contact_saved', ?, 200, 1000)
        """,
        (TENANT_A, OWNER_A, CONTACT_A),
    )
    connection.execute(
        """
        INSERT INTO audit_events (
            tenant_id, audit_event_id, correlation_id, actor_id, action,
            resource_type, resource_id, result_code, occurred_at_ms
        ) VALUES (?, 'audit_phase2b_success', 'corr_phase2b_success', ?, 'client.contact_upsert',
                  'client_contact', ?, 'contact_saved', 200)
        """,
        (TENANT_A, OWNER_A, CONTACT_A),
    )
    connection.execute(
        """
        INSERT INTO outbox_events (
            tenant_id, outbox_event_id, aggregate_type, aggregate_id,
            aggregate_version, event_type, payload_json, created_at_ms
        ) VALUES (?, 'outbox_phase2b_success', 'client', ?, 2, 'client.contact_saved.v1', '{}', 200)
        """,
        (TENANT_A, CLIENT_A),
    )
    connection.commit()

    assert client_version(connection) == 2
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM client_contact_commands WHERE tenant_id = ?",
        (TENANT_A,),
    ).fetchone()["value"] == 1
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM client_contact_points WHERE tenant_id = ?",
        (TENANT_A,),
    ).fetchone()["value"] == 1
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM idempotency_records WHERE idempotency_key = 'idem_phase2b_success'"
    ).fetchone()["value"] == 1
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM audit_events WHERE audit_event_id = 'audit_phase2b_success'"
    ).fetchone()["value"] == 1
    assert connection.execute(
        "SELECT COUNT(*) AS value FROM outbox_events WHERE outbox_event_id = 'outbox_phase2b_success'"
    ).fetchone()["value"] == 1
    connection.close()


def test_lifecycle_guard_is_versioned_and_archive_cascades_to_contacts() -> None:
    connection = seeded_database()
    insert_contact(connection)
    expect_integrity(
        lambda: connection.execute(
            """
            INSERT INTO client_lifecycle_commands (
                tenant_id, command_id, command_actor_id, client_id,
                operation, expected_client_version, next_display_name, executed_at_ms
            ) VALUES (?, 'cmd_phase2b_bad_version', ?, ?, 'ARCHIVE', 99, NULL, 200)
            """,
            (TENANT_A, OWNER_A, CLIENT_A),
        ),
        "client_lifecycle_version_mismatch",
    )
    assert client_version(connection) == 1

    connection.execute(
        """
        INSERT INTO client_lifecycle_commands (
            tenant_id, command_id, command_actor_id, client_id,
            operation, expected_client_version, next_display_name, executed_at_ms
        ) VALUES (?, 'cmd_phase2b_archive', ?, ?, 'ARCHIVE', 1, NULL, 200)
        """,
        (TENANT_A, OWNER_A, CLIENT_A),
    )
    client = connection.execute(
        "SELECT status, version FROM clients WHERE tenant_id = ? AND client_id = ?",
        (TENANT_A, CLIENT_A),
    ).fetchone()
    contact = connection.execute(
        "SELECT status FROM client_contact_points WHERE tenant_id = ? AND contact_point_id = ?",
        (TENANT_A, CONTACT_A),
    ).fetchone()
    assert client is not None and client["status"] == "ARCHIVED" and client["version"] == 2
    assert contact is not None and contact["status"] == "ARCHIVED"
    connection.close()


def main() -> int:
    test_schema_has_no_plaintext_contact_column()
    test_protected_shape_and_versions_fail_closed()
    test_tenant_and_client_scope_are_structural()
    test_exact_lookup_is_index_backed()
    test_archival_is_one_way_and_delete_is_forbidden()
    test_active_contact_requires_active_client()
    test_contact_guard_failure_rolls_back_client_version_and_command_intent()
    test_full_contact_mutation_commits_one_atomic_evidence_set()
    test_lifecycle_guard_is_versioned_and_archive_cascades_to_contacts()

    print("Phase 2B protected client-contact D1 invariants and failure ordering are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())