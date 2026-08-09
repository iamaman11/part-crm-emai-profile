#!/usr/bin/env python3
"""Prove mailbox catalog privacy, lifecycle, replay and atomicity invariants."""

from __future__ import annotations

import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
TENANT = "tenant_01_mailbox_slice"
OWNER = "actor_owner_mailbox_slice"
OWNER_IDENTITY = "identity_owner_mailbox_slice"
BINDING = "mailbox_01_mailbox_slice"
JOB = "mailjob_01_mailbox_slice"
DIGEST = "a" * 64


def database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    migrations = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name[:4]) for path in migrations]
    assert versions == list(range(1, len(versions) + 1)), versions
    for migration in migrations:
        connection.executescript(migration.read_text(encoding="utf-8"))
    return connection


def seed(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Mailbox Slice Tenant', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT,),
    )
    connection.execute(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, 'access-owner-mailbox-slice', 10)
        """,
        (OWNER_IDENTITY,),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT, OWNER, OWNER_IDENTITY),
    )
    connection.commit()


def expect_abort(operation, fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert fragment in str(error), (fragment, str(error))
    else:
        raise AssertionError(f"expected SQLite abort containing {fragment!r}")


def assert_metadata_only_schema(connection: sqlite3.Connection) -> None:
    for table in ("mailbox_bindings", "mailbox_jobs"):
        columns = {
            row["name"].lower()
            for row in connection.execute(f"PRAGMA table_info({table})").fetchall()
        }
        forbidden_fragments = (
            "password",
            "credential",
            "authorization",
            "access_token",
            "refresh_token",
            "message_body",
            "body_html",
            "body_text",
            "raw_message",
        )
        for fragment in forbidden_fragments:
            assert all(fragment not in column for column in columns), (table, fragment, columns)
    binding_columns = {
        row["name"] for row in connection.execute("PRAGMA table_info(mailbox_bindings)")
    }
    assert "secret_handle" in binding_columns


def insert_evidence(
    connection: sqlite3.Connection,
    *,
    key: str,
    command_name: str,
    result_code: str,
    resource_id: str,
    aggregate_type: str,
    aggregate_version: int,
    event_type: str,
    now_ms: int,
    expires_at_ms: int = 1000,
) -> None:
    connection.execute(
        """
        INSERT INTO idempotency_records (
            tenant_id, actor_id, idempotency_key, command_name, request_digest,
            result_code, result_reference, created_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT,
            OWNER,
            key,
            command_name,
            DIGEST,
            result_code,
            resource_id,
            now_ms,
            expires_at_ms,
        ),
    )
    connection.execute(
        """
        INSERT INTO audit_events (
            tenant_id, audit_event_id, correlation_id, actor_id, action,
            resource_type, resource_id, result_code, occurred_at_ms
        ) VALUES (?, ?, 'corr_mailbox_slice', ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT,
            f"audit_{key}",
            OWNER,
            command_name,
            aggregate_type,
            resource_id,
            result_code,
            now_ms,
        ),
    )
    connection.execute(
        """
        INSERT INTO outbox_events (
            tenant_id, outbox_event_id, aggregate_type, aggregate_id,
            aggregate_version, event_type, payload_json, created_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, '{}', ?)
        """,
        (
            TENANT,
            f"outbox_{key}",
            aggregate_type,
            resource_id,
            aggregate_version,
            event_type,
            now_ms,
        ),
    )


def create_binding(connection: sqlite3.Connection) -> None:
    key = "idem_mailbox_binding_create"
    with connection:
        connection.execute(
            """
            INSERT INTO mailbox_binding_create_commands (
                tenant_id, command_id, command_actor_id, binding_id,
                provider, secret_handle, executed_at_ms
            ) VALUES (?, 'command_mailbox_binding_create', ?, ?, 'IMAP',
                      'secret_handle_mailbox_slice', 20)
            """,
            (TENANT, OWNER, BINDING),
        )
        insert_evidence(
            connection,
            key=key,
            command_name="mailbox.binding_create",
            result_code="created",
            resource_id=BINDING,
            aggregate_type="mailbox_binding",
            aggregate_version=1,
            event_type="mailbox.binding_created.v1",
            now_ms=20,
        )
    row = connection.execute(
        """
        SELECT provider, secret_handle, status, version
        FROM mailbox_bindings WHERE tenant_id = ? AND binding_id = ?
        """,
        (TENANT, BINDING),
    ).fetchone()
    assert row is not None
    assert dict(row) == {
        "provider": "IMAP",
        "secret_handle": "secret_handle_mailbox_slice",
        "status": "ACTIVE",
        "version": 1,
    }

    replay = connection.execute(
        """
        SELECT command_name, request_digest, result_code, result_reference, expires_at_ms
        FROM idempotency_records
        WHERE tenant_id = ? AND actor_id = ? AND idempotency_key = ?
        """,
        (TENANT, OWNER, key),
    ).fetchone()
    assert replay is not None
    assert replay["command_name"] == "mailbox.binding_create"
    assert replay["request_digest"] == DIGEST
    assert replay["result_code"] == "created"
    assert replay["result_reference"] == BINDING
    assert replay["expires_at_ms"] == 1000

    def exact_live(command: str, digest: str, now_ms: int) -> bool:
        return (
            replay["command_name"] == command
            and replay["request_digest"] == digest
            and now_ms < replay["expires_at_ms"]
        )

    assert exact_live("mailbox.binding_create", DIGEST, 999)
    assert not exact_live("mailbox.binding_revoke", DIGEST, 999)
    assert not exact_live("mailbox.binding_create", "b" * 64, 999)
    assert not exact_live("mailbox.binding_create", DIGEST, 1000)


def create_and_run_job(connection: sqlite3.Connection) -> None:
    with connection:
        connection.execute(
            """
            INSERT INTO mailbox_job_create_commands (
                tenant_id, command_id, command_actor_id, binding_id, job_id,
                cursor, scheduled_at_ms, max_attempts, executed_at_ms
            ) VALUES (?, 'command_mailbox_job_create', ?, ?, ?, 'cursor-1', 30, 3, 30)
            """,
            (TENANT, OWNER, BINDING, JOB),
        )
        insert_evidence(
            connection,
            key="idem_mailbox_job_create",
            command_name="mailbox.job_create",
            result_code="created",
            resource_id=JOB,
            aggregate_type="mailbox_job",
            aggregate_version=1,
            event_type="mailbox.job_created.v1",
            now_ms=30,
        )

    with connection:
        connection.execute(
            """
            INSERT INTO mailbox_job_run_commands_v2 (
                tenant_id, command_id, command_actor_id, binding_id, job_id,
                expected_job_version, outcome_status, next_cursor, provider_status,
                bounded_item_count, retry_at_ms, executed_at_ms
            ) VALUES (?, 'command_mailbox_job_run', ?, ?, ?, 1, 'SUCCEEDED',
                      'cursor-2', 'SYNTHETIC_OK', 2, NULL, 40)
            """,
            (TENANT, OWNER, BINDING, JOB),
        )
        insert_evidence(
            connection,
            key="idem_mailbox_job_run",
            command_name="mailbox.job_run",
            result_code="succeeded",
            resource_id=JOB,
            aggregate_type="mailbox_job",
            aggregate_version=4,
            event_type="mailbox.job_succeeded.v1",
            now_ms=40,
        )

    row = connection.execute(
        """
        SELECT status, lifecycle_status, attempt, cursor, provider_status,
               bounded_item_count, version
        FROM mailbox_jobs
        WHERE tenant_id = ? AND binding_id = ? AND job_id = ?
        """,
        (TENANT, BINDING, JOB),
    ).fetchone()
    assert row is not None
    assert dict(row) == {
        "status": "SUCCEEDED",
        "lifecycle_status": "SUCCEEDED",
        "attempt": 1,
        "cursor": "cursor-2",
        "provider_status": "SYNTHETIC_OK",
        "bounded_item_count": 2,
        "version": 4,
    }

    expect_abort(
        lambda: connection.execute(
            """
            INSERT INTO mailbox_job_run_commands_v2 (
                tenant_id, command_id, command_actor_id, binding_id, job_id,
                expected_job_version, outcome_status, next_cursor, provider_status,
                bounded_item_count, retry_at_ms, executed_at_ms
            ) VALUES (?, 'command_mailbox_job_run_again', ?, ?, ?, 4, 'FAILED',
                      NULL, 'TERMINAL_FAILURE', 0, NULL, 41)
            """,
            (TENANT, OWNER, BINDING, JOB),
        ),
        "mailbox_job_version_mismatch",
    )
    connection.rollback()


def revoke_blocks_new_work(connection: sqlite3.Connection) -> None:
    with connection:
        connection.execute(
            """
            INSERT INTO mailbox_binding_revoke_commands (
                tenant_id, command_id, command_actor_id, binding_id,
                expected_binding_version, executed_at_ms
            ) VALUES (?, 'command_mailbox_binding_revoke', ?, ?, 1, 50)
            """,
            (TENANT, OWNER, BINDING),
        )
        insert_evidence(
            connection,
            key="idem_mailbox_binding_revoke",
            command_name="mailbox.binding_revoke",
            result_code="revoked",
            resource_id=BINDING,
            aggregate_type="mailbox_binding",
            aggregate_version=2,
            event_type="mailbox.binding_revoked.v1",
            now_ms=50,
        )

    expect_abort(
        lambda: connection.execute(
            """
            INSERT INTO mailbox_job_create_commands (
                tenant_id, command_id, command_actor_id, binding_id, job_id,
                cursor, scheduled_at_ms, max_attempts, executed_at_ms
            ) VALUES (?, 'command_after_revoke', ?, ?, 'mailjob_after_revoke',
                      NULL, 60, 3, 60)
            """,
            (TENANT, OWNER, BINDING),
        ),
        "mailbox_binding_revoked",
    )
    connection.rollback()


def late_evidence_failure_rolls_back_complete_envelope(connection: sqlite3.Connection) -> None:
    rollback_binding = "mailbox_rollback_slice"
    key = "idem_mailbox_rollback"
    try:
        with connection:
            connection.execute(
                """
                INSERT INTO mailbox_binding_create_commands (
                    tenant_id, command_id, command_actor_id, binding_id,
                    provider, secret_handle, executed_at_ms
                ) VALUES (?, 'command_mailbox_rollback', ?, ?, 'IMAP',
                          'secret_handle_rollback', 70)
                """,
                (TENANT, OWNER, rollback_binding),
            )
            connection.execute(
                """
                INSERT INTO idempotency_records (
                    tenant_id, actor_id, idempotency_key, command_name, request_digest,
                    result_code, result_reference, created_at_ms, expires_at_ms
                ) VALUES (?, ?, ?, 'mailbox.binding_create', ?, 'created', ?, 70, 1000)
                """,
                (TENANT, OWNER, key, DIGEST, rollback_binding),
            )
            connection.execute(
                """
                INSERT INTO audit_events (
                    tenant_id, audit_event_id, correlation_id, actor_id, action,
                    resource_type, resource_id, result_code, occurred_at_ms
                ) VALUES (?, 'audit_mailbox_rollback', 'corr_mailbox_rollback', ?,
                          'mailbox.binding_create', 'mailbox_binding', ?, 'created', 70)
                """,
                (TENANT, OWNER, rollback_binding),
            )
            connection.execute(
                """
                INSERT INTO outbox_events (
                    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                    aggregate_version, event_type, payload_json, created_at_ms
                ) VALUES (?, 'outbox_mailbox_rollback', 'mailbox_binding', ?, 1,
                          'mailbox.binding_created.v1', '{', 70)
                """,
                (TENANT, rollback_binding),
            )
    except sqlite3.IntegrityError as error:
        assert "CHECK constraint failed" in str(error), str(error)
    else:
        raise AssertionError("invalid late mailbox evidence unexpectedly committed")

    checks = (
        ("mailbox_binding_create_commands", "command_id", "command_mailbox_rollback"),
        ("mailbox_bindings", "binding_id", rollback_binding),
        ("idempotency_records", "idempotency_key", key),
        ("audit_events", "audit_event_id", "audit_mailbox_rollback"),
        ("outbox_events", "outbox_event_id", "outbox_mailbox_rollback"),
    )
    for table, column, value in checks:
        assert (
            connection.execute(
                f"SELECT COUNT(*) FROM {table} WHERE tenant_id = ? AND {column} = ?",
                (TENANT, value),
            ).fetchone()[0]
            == 0
        ), (table, column, value)


def main() -> int:
    connection = database()
    try:
        seed(connection)
        assert_metadata_only_schema(connection)
        late_evidence_failure_rolls_back_complete_envelope(connection)
        create_binding(connection)
        create_and_run_job(connection)
        revoke_blocks_new_work(connection)
    finally:
        connection.close()
    print("mailbox vertical-slice SQLite/privacy checks: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
