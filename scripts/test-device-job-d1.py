#!/usr/bin/env python3
"""Prove Phase 2F device authorization/job D1 invariants with SQLite."""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"

TENANT_A = "tenant_device_jobs_a"
TENANT_B = "tenant_device_jobs_b"
OWNER_A = "actor_device_jobs_a"
OWNER_B = "actor_device_jobs_b"
IDENTITY_A = "identity_device_jobs_a"
IDENTITY_B = "identity_device_jobs_b"
PROFILE_A = "profile_device_jobs_a"
PROFILE_B = "profile_device_jobs_b"
GENERATION_A = "generation_device_jobs_a"
GENERATION_B = "generation_device_jobs_b"
DEVICE_A = "device_device_jobs_a"
DEVICE_B = "device_device_jobs_b"
JOB_A = "devjob_device_jobs_a"


def migration_files() -> list[Path]:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    expected = list(range(1, len(files) + 1))
    if not files or versions != expected:
        raise AssertionError(f"D1 migrations must be contiguous: {versions}; expected {expected}")
    if files[-1].name != "0018_device_authorizations_and_jobs.sql":
        raise AssertionError(f"unexpected Phase 2F migration tail: {files[-1].name}")
    return files


def database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in migration_files():
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.commit()
    return connection


def expect_integrity_error(operation: Callable[[], object]) -> None:
    try:
        operation()
    except sqlite3.IntegrityError:
        return
    raise AssertionError("operation unexpectedly bypassed a required D1 invariant")


def seed_tenant(
    connection: sqlite3.Connection,
    *,
    tenant: str,
    owner: str,
    identity: str,
    profile: str,
    generation: str,
    device: str,
) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'ACTIVE', 1, 10, 10)
        """,
        (tenant, tenant),
    )
    connection.execute(
        "INSERT INTO identities (identity_id, access_subject, created_at_ms) VALUES (?, ?, 10)",
        (identity, f"subject-{identity}"),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 10, 10)
        """,
        (tenant, owner, identity),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, active_generation_id, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', NULL, 1, ?, ?, 20, 20)
        """,
        (tenant, profile, owner, owner),
    )
    connection.execute(
        """
        INSERT INTO profile_generation_register_commands (
            tenant_id, command_id, command_actor_id, profile_id, generation_id,
            object_key, metadata_digest, container_digest, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 30)
        """,
        (
            tenant,
            f"cmd_register_{generation}",
            owner,
            profile,
            generation,
            f"profiles/v1/{generation}.enc",
            "a" * 64,
            "b" * 64,
        ),
    )
    connection.execute(
        """
        INSERT INTO profile_generation_verify_commands (
            tenant_id, command_id, command_actor_id, profile_id, generation_id,
            expected_generation_version, verification_reference, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, 1, ?, 40)
        """,
        (
            tenant,
            f"cmd_verify_{generation}",
            owner,
            profile,
            generation,
            f"verify_{generation}",
        ),
    )
    connection.execute(
        """
        INSERT INTO device_authorizations (
            tenant_id, device_id, profile_id, generation_id, version, status,
            evidence_reference, authorized_by_actor_id, updated_by_actor_id,
            authorized_at_ms, updated_at_ms, revoked_at_ms
        ) VALUES (?, ?, ?, ?, 1, 'ACTIVE', ?, ?, ?, 50, 50, NULL)
        """,
        (
            tenant,
            device,
            profile,
            generation,
            f"evidence_{device}",
            owner,
            owner,
        ),
    )


def insert_pending_job(connection: sqlite3.Connection, *, tenant: str = TENANT_A) -> None:
    connection.execute(
        """
        INSERT INTO device_jobs (
            tenant_id, job_id, device_id, profile_id, generation_id,
            aggregate_version, status, attempt, max_attempts, last_fence,
            current_claim_id, claim_fence, claimed_at_ms, claim_heartbeat_at_ms,
            claim_lease_expires_at_ms, retry_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, 1, 'PENDING_DEVICE', 0, 3, 0,
                  NULL, NULL, NULL, NULL, NULL, NULL, 60)
        """,
        (tenant, JOB_A, DEVICE_A, PROFILE_A, GENERATION_A),
    )


def test_schema_and_tenant_binding(connection: sqlite3.Connection) -> None:
    seed_tenant(
        connection,
        tenant=TENANT_A,
        owner=OWNER_A,
        identity=IDENTITY_A,
        profile=PROFILE_A,
        generation=GENERATION_A,
        device=DEVICE_A,
    )
    seed_tenant(
        connection,
        tenant=TENANT_B,
        owner=OWNER_B,
        identity=IDENTITY_B,
        profile=PROFILE_B,
        generation=GENERATION_B,
        device=DEVICE_B,
    )
    insert_pending_job(connection)
    connection.commit()

    assert connection.execute(
        "SELECT COUNT(*) FROM device_jobs WHERE tenant_id = ? AND job_id = ?",
        (TENANT_B, JOB_A),
    ).fetchone()[0] == 0

    expect_integrity_error(
        lambda: connection.execute(
            """
            INSERT INTO device_jobs (
                tenant_id, job_id, device_id, profile_id, generation_id,
                aggregate_version, status, attempt, max_attempts, last_fence, updated_at_ms
            ) VALUES (?, 'devjob_cross_tenant', ?, ?, ?, 1,
                      'PENDING_DEVICE', 0, 3, 0, 60)
            """,
            (TENANT_B, DEVICE_A, PROFILE_A, GENERATION_A),
        )
    )
    connection.rollback()


def test_claim_shape_and_stale_cas(connection: sqlite3.Connection) -> None:
    running = connection.execute(
        """
        UPDATE device_jobs
        SET aggregate_version = 2,
            status = 'RUNNING',
            attempt = 1,
            last_fence = 1,
            current_claim_id = 'devclaim_device_jobs_1',
            claim_fence = 1,
            claimed_at_ms = 70,
            claim_heartbeat_at_ms = 70,
            claim_lease_expires_at_ms = 100,
            updated_at_ms = 70
        WHERE tenant_id = ? AND job_id = ? AND aggregate_version = 1
        """,
        (TENANT_A, JOB_A),
    )
    assert running.rowcount == 1
    connection.commit()

    expect_integrity_error(
        lambda: connection.execute(
            """
            UPDATE device_jobs
            SET claim_lease_expires_at_ms = NULL
            WHERE tenant_id = ? AND job_id = ?
            """,
            (TENANT_A, JOB_A),
        )
    )
    connection.rollback()

    retry = connection.execute(
        """
        UPDATE device_jobs
        SET aggregate_version = 3,
            status = 'RETRY_SCHEDULED',
            current_claim_id = NULL,
            claim_fence = NULL,
            claimed_at_ms = NULL,
            claim_heartbeat_at_ms = NULL,
            claim_lease_expires_at_ms = NULL,
            retry_at_ms = 120,
            updated_at_ms = 80
        WHERE tenant_id = ? AND job_id = ? AND aggregate_version = 2
        """,
        (TENANT_A, JOB_A),
    )
    assert retry.rowcount == 1
    second_claim = connection.execute(
        """
        UPDATE device_jobs
        SET aggregate_version = 4,
            status = 'RUNNING',
            attempt = 2,
            last_fence = 2,
            current_claim_id = 'devclaim_device_jobs_2',
            claim_fence = 2,
            claimed_at_ms = 120,
            claim_heartbeat_at_ms = 120,
            claim_lease_expires_at_ms = 180,
            retry_at_ms = NULL,
            updated_at_ms = 120
        WHERE tenant_id = ? AND job_id = ? AND aggregate_version = 3
        """,
        (TENANT_A, JOB_A),
    )
    assert second_claim.rowcount == 1
    connection.commit()

    stale_result = connection.execute(
        """
        UPDATE device_jobs
        SET aggregate_version = 3, status = 'SUCCEEDED',
            current_claim_id = NULL, claim_fence = NULL, claimed_at_ms = NULL,
            claim_heartbeat_at_ms = NULL, claim_lease_expires_at_ms = NULL,
            retry_at_ms = NULL, updated_at_ms = 130
        WHERE tenant_id = ? AND job_id = ? AND aggregate_version = 2
        """,
        (TENANT_A, JOB_A),
    )
    assert stale_result.rowcount == 0
    current = connection.execute(
        """
        SELECT aggregate_version, status, attempt, last_fence, current_claim_id
        FROM device_jobs WHERE tenant_id = ? AND job_id = ?
        """,
        (TENANT_A, JOB_A),
    ).fetchone()
    assert tuple(current) == (4, "RUNNING", 2, 2, "devclaim_device_jobs_2")


def test_authorization_version_and_revocation_shape(connection: sqlite3.Connection) -> None:
    fresh = connection.execute(
        """
        UPDATE device_authorizations
        SET version = version + 1,
            status = 'REVOKED',
            updated_by_actor_id = ?,
            updated_at_ms = 140,
            revoked_at_ms = 140
        WHERE tenant_id = ? AND device_id = ? AND profile_id = ?
          AND generation_id = ? AND version = 1 AND status = 'ACTIVE'
        """,
        (OWNER_A, TENANT_A, DEVICE_A, PROFILE_A, GENERATION_A),
    )
    assert fresh.rowcount == 1
    stale = connection.execute(
        """
        UPDATE device_authorizations
        SET version = version + 1
        WHERE tenant_id = ? AND device_id = ? AND profile_id = ?
          AND generation_id = ? AND version = 1
        """,
        (TENANT_A, DEVICE_A, PROFILE_A, GENERATION_A),
    )
    assert stale.rowcount == 0
    row = connection.execute(
        """
        SELECT version, status, revoked_at_ms
        FROM device_authorizations
        WHERE tenant_id = ? AND device_id = ? AND profile_id = ? AND generation_id = ?
        """,
        (TENANT_A, DEVICE_A, PROFILE_A, GENERATION_A),
    ).fetchone()
    assert tuple(row) == (2, "REVOKED", 140)


def main() -> int:
    connection = database()
    try:
        test_schema_and_tenant_binding(connection)
        test_claim_shape_and_stale_cas(connection)
        test_authorization_version_and_revocation_shape(connection)
    finally:
        connection.close()
    print("Phase 2F device authorization/job D1 invariants are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
