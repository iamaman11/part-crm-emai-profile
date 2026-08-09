#!/usr/bin/env python3
"""Prove Phase 2B protected client-contact D1 invariants."""

from __future__ import annotations

import sqlite3

from test_d1_schema import (
    CLIENT_A,
    CLIENT_B,
    OWNER_A,
    OWNER_B,
    TENANT_A,
    TENANT_B,
    apply_migrations,
    open_database,
    seed_catalog,
)

CONTACT_A = "contact_01_phase2b_a"
CONTACT_B = "contact_01_phase2b_b"
TOKEN = bytes(range(32))
CIPHERTEXT = bytes([0xA5]) * 48
NONCE = bytes([0x5A]) * 24


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
        ) VALUES (?, ?, ?, 'EMAIL', ?, 1, 1, ?, ?, ?, ?, ?, ?, ?, 100, 100)
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
        ),
    )


def test_schema_has_no_plaintext_contact_column(connection: sqlite3.Connection) -> None:
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


def test_protected_shape_and_versions_fail_closed(connection: sqlite3.Connection) -> None:
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


def test_tenant_and_client_scope_are_structural(connection: sqlite3.Connection) -> None:
    expect_integrity(
        lambda: insert_contact(
            connection,
            tenant_id=TENANT_B,
            client_id=CLIENT_A,
            contact_point_id=CONTACT_B,
            actor_id=OWNER_B,
        ),
        "FOREIGN KEY constraint failed",
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


def test_exact_lookup_is_index_backed(connection: sqlite3.Connection) -> None:
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


def test_archival_is_one_way_and_delete_is_forbidden(connection: sqlite3.Connection) -> None:
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


def test_active_contact_requires_active_client(connection: sqlite3.Connection) -> None:
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


def main() -> int:
    connection = open_database()
    apply_migrations(connection)
    seed_catalog(connection)

    test_schema_has_no_plaintext_contact_column(connection)
    test_protected_shape_and_versions_fail_closed(connection)
    test_tenant_and_client_scope_are_structural(connection)
    test_exact_lookup_is_index_backed(connection)
    test_archival_is_one_way_and_delete_is_forbidden(connection)
    test_active_contact_requires_active_client(connection)

    connection.rollback()
    connection.close()
    print("Phase 2B protected client-contact D1 invariants are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
