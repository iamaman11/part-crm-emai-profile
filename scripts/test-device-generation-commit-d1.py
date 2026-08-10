#!/usr/bin/env python3
"""Prove Phase 2F atomic device-generation catalog commit invariants with SQLite."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"

TENANT = "tenant_device_generation_commit"
OWNER = "actor_device_generation_commit"
IDENTITY = "identity_device_generation_commit"
PROFILE = "profile_device_generation_commit"
DEVICE = "device_device_generation_commit"
BASE = "generation_device_commit_base"
CANDIDATE = "generation_device_commit_candidate"
WINNER = "generation_device_commit_winner"
JOB = "devjob_device_generation_commit"
CLAIM = "devclaim_device_generation_commit"
SESSION = "session_device_generation_commit"
TOKEN_DIGEST = "c" * 64
METADATA_DIGEST = "d" * 64
CONTAINER_DIGEST = "e" * 64


def migration_files() -> list[Path]:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    expected = list(range(1, len(files) + 1))
    if not files or versions != expected:
        raise AssertionError(f"D1 migrations must be contiguous: {versions}; expected {expected}")
    if files[-1].name != "0021_device_generation_commit.sql":
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
    raise AssertionError("operation unexpectedly bypassed a device-generation invariant")


def canonical_key(generation_id: str) -> str:
    return f"tenants/{TENANT}/profiles/{PROFILE}/generations/{generation_id}.bpgc"


def owner_register(
    connection: sqlite3.Connection,
    generation_id: str,
    at_ms: int,
    *,
    metadata_digest: str = "a" * 64,
    container_digest: str = "b" * 64,
) -> None:
    connection.execute(
        """
        INSERT INTO profile_generation_register_commands (
            tenant_id, command_id, command_actor_id, profile_id, generation_id,
            object_key, metadata_digest, container_digest, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT,
            f"cmd_register_{generation_id}",
            OWNER,
            PROFILE,
            generation_id,
            canonical_key(generation_id),
            metadata_digest,
            container_digest,
            at_ms,
        ),
    )


def owner_verify(connection: sqlite3.Connection, generation_id: str, at_ms: int) -> None:
    connection.execute(
        """
        INSERT INTO profile_generation_verify_commands (
            tenant_id, command_id, command_actor_id, profile_id, generation_id,
            expected_generation_version, verification_reference, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, 1, ?, ?)
        """,
        (
            TENANT,
            f"cmd_verify_{generation_id}",
            OWNER,
            PROFILE,
            generation_id,
            f"verify_{generation_id}",
            at_ms,
        ),
    )


def owner_activate(
    connection: sqlite3.Connection,
    generation_id: str,
    expected_profile_version: int,
    at_ms: int,
) -> None:
    connection.execute(
        """
        INSERT INTO profile_generation_activate_commands (
            tenant_id, command_id, command_actor_id, profile_id, generation_id,
            expected_profile_version, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT,
            f"cmd_activate_{generation_id}_{at_ms}",
            OWNER,
            PROFILE,
            generation_id,
            expected_profile_version,
            at_ms,
        ),
    )


def projection_payload(
    *,
    version: int,
    sequence: int,
    epoch: int,
    session_id: str = SESSION,
    device_id: str = DEVICE,
    updated_at_ms: int,
) -> str:
    return json.dumps(
        {
            "tenant_id": TENANT,
            "profile_id": PROFILE,
            "coordinator_version": version,
            "coordinator_sequence": sequence,
            "coordinator_status": "active",
            "active_session_id": session_id,
            "active_device_id": device_id,
            "active_epoch": epoch,
            "idle_expires_at_ms": 500,
            "hard_expires_at_ms": 1000,
            "drain_deadline_ms": None,
            "updated_at_ms": updated_at_ms,
        },
        separators=(",", ":"),
    )


def project_coordinator(
    connection: sqlite3.Connection,
    *,
    command_id: str,
    expected_version: int,
    version: int,
    sequence: int,
    epoch: int,
    updated_at_ms: int,
    session_id: str = SESSION,
    device_id: str = DEVICE,
) -> None:
    connection.execute(
        """
        INSERT INTO profile_coordinator_projection_commands (
            tenant_id, command_id, command_actor_id, profile_id,
            expected_coordinator_version, projection_json, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT,
            command_id,
            OWNER,
            PROFILE,
            expected_version,
            projection_payload(
                version=version,
                sequence=sequence,
                epoch=epoch,
                session_id=session_id,
                device_id=device_id,
                updated_at_ms=updated_at_ms,
            ),
            updated_at_ms,
        ),
    )


def seed() -> sqlite3.Connection:
    connection = database()
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Device Generation Commit', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT,),
    )
    connection.execute(
        "INSERT INTO identities (identity_id, access_subject, created_at_ms) VALUES (?, ?, 10)",
        (IDENTITY, f"subject-{IDENTITY}"),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT, OWNER, IDENTITY),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, active_generation_id, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', NULL, 1, ?, ?, 20, 20)
        """,
        (TENANT, PROFILE, OWNER, OWNER),
    )

    owner_register(connection, BASE, 30)
    owner_verify(connection, BASE, 35)
    owner_activate(connection, BASE, 1, 40)

    connection.execute(
        """
        INSERT INTO device_authorizations (
            tenant_id, device_id, profile_id, generation_id, version, status,
            evidence_reference, authorized_by_actor_id, updated_by_actor_id,
            authorized_at_ms, updated_at_ms, revoked_at_ms
        ) VALUES (?, ?, ?, ?, 1, 'ACTIVE', 'evidence_device_generation_commit',
                  ?, ?, 50, 50, NULL)
        """,
        (TENANT, DEVICE, PROFILE, BASE, OWNER, OWNER),
    )
    connection.execute(
        """
        INSERT INTO device_actor_bindings (
            tenant_id, actor_id, device_id, version, status, evidence_reference,
            bound_at_ms, updated_at_ms, revoked_at_ms
        ) VALUES (?, ?, ?, 1, 'ACTIVE', 'binding_device_generation_commit', 50, 50, NULL)
        """,
        (TENANT, OWNER, DEVICE),
    )
    connection.execute(
        """
        INSERT INTO device_jobs (
            tenant_id, job_id, device_id, profile_id, generation_id,
            aggregate_version, status, attempt, max_attempts, last_fence,
            current_claim_id, claim_fence, claimed_at_ms, claim_heartbeat_at_ms,
            claim_lease_expires_at_ms, retry_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, 2, 'RUNNING', 1, 3, 1,
                  ?, 1, 70, 70, 200, NULL, 70)
        """,
        (TENANT, JOB, DEVICE, PROFILE, BASE, CLAIM),
    )
    project_coordinator(
        connection,
        command_id="cmd_projection_initial",
        expected_version=0,
        version=1,
        sequence=0,
        epoch=1,
        updated_at_ms=60,
    )
    connection.commit()
    return connection


def command_values(
    *,
    generation_id: str = CANDIDATE,
    object_key: str | None = None,
    metadata_digest: str = METADATA_DIGEST,
    container_digest: str = CONTAINER_DIGEST,
    container_bytes: int = 4096,
    expected_job_version: int = 2,
    claim_id: str = CLAIM,
    claim_fence: int = 1,
    expected_profile_version: int = 2,
    coordinator_session_id: str = SESSION,
    coordinator_epoch: int = 1,
    coordinator_version: int = 1,
    coordinator_sequence: int = 0,
    executed_at_ms: int = 100,
    base_generation_id: str = BASE,
) -> tuple[object, ...]:
    return (
        TENANT,
        JOB,
        OWNER,
        DEVICE,
        PROFILE,
        base_generation_id,
        generation_id,
        object_key or canonical_key(generation_id),
        metadata_digest,
        container_digest,
        container_bytes,
        expected_job_version,
        claim_id,
        claim_fence,
        expected_profile_version,
        coordinator_session_id,
        TOKEN_DIGEST,
        coordinator_epoch,
        coordinator_version,
        coordinator_sequence,
        executed_at_ms,
    )


def insert_device_commit(connection: sqlite3.Connection, values: tuple[object, ...]) -> None:
    connection.execute(
        """
        INSERT INTO device_generation_commit_commands (
            tenant_id, job_id, command_actor_id, device_id, profile_id,
            base_generation_id, generation_id, object_key, metadata_digest,
            container_digest, container_bytes, expected_job_version, claim_id,
            claim_fence, expected_profile_version, coordinator_session_id,
            coordinator_fencing_token_digest, coordinator_epoch,
            coordinator_version, coordinator_sequence, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        values,
    )


def catalog_snapshot(connection: sqlite3.Connection, generation_id: str = CANDIDATE) -> tuple[object, ...]:
    profile = connection.execute(
        """
        SELECT active_generation_id, status, version, updated_at_ms
        FROM browser_profiles
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT, PROFILE),
    ).fetchone()
    generation = connection.execute(
        """
        SELECT status, version, verification_reference, metadata_digest, container_digest
        FROM profile_generations
        WHERE tenant_id = ? AND profile_id = ? AND generation_id = ?
        """,
        (TENANT, PROFILE, generation_id),
    ).fetchone()
    commands = connection.execute(
        "SELECT COUNT(*) FROM device_generation_commit_commands WHERE tenant_id = ?",
        (TENANT,),
    ).fetchone()[0]
    return (
        tuple(profile),
        None if generation is None else tuple(generation),
        int(commands),
    )


def assert_failed_without_catalog_mutation(
    connection: sqlite3.Connection,
    operation: Callable[[], object],
) -> None:
    before = catalog_snapshot(connection)
    expect_integrity_error(operation)
    connection.rollback()
    assert catalog_snapshot(connection) == before


def test_schema_is_metadata_only() -> None:
    connection = database()
    try:
        columns = {
            str(row[1]) for row in connection.execute("PRAGMA table_info(device_generation_commit_commands)")
        }
        assert "coordinator_fencing_token_digest" in columns
        assert "coordinator_fencing_token" not in columns
        forbidden = {"container", "ciphertext", "profile_bytes", "key", "secret", "mail_body"}
        assert columns.isdisjoint(forbidden)
    finally:
        connection.close()


def test_exact_command_atomically_verifies_and_activates() -> None:
    connection = seed()
    try:
        insert_device_commit(connection, command_values())
        connection.commit()
        profile, generation, commands = catalog_snapshot(connection)
        assert profile == (CANDIDATE, "READY", 3, 100)
        assert generation == (
            "VERIFIED",
            2,
            f"r2sha256:{CONTAINER_DIGEST}",
            METADATA_DIGEST,
            CONTAINER_DIGEST,
        )
        assert commands == 1

        before = catalog_snapshot(connection)
        expect_integrity_error(lambda: insert_device_commit(connection, command_values()))
        connection.rollback()
        assert catalog_snapshot(connection) == before

        expect_integrity_error(
            lambda: connection.execute(
                "UPDATE device_generation_commit_commands SET executed_at_ms = 101 WHERE tenant_id = ? AND job_id = ?",
                (TENANT, JOB),
            )
        )
        connection.rollback()
        expect_integrity_error(
            lambda: connection.execute(
                "DELETE FROM device_generation_commit_commands WHERE tenant_id = ? AND job_id = ?",
                (TENANT, JOB),
            )
        )
        connection.rollback()
    finally:
        connection.close()


def test_stale_claim_fence_and_expired_claim_fail_before_catalog_mutation() -> None:
    for values in (
        command_values(claim_fence=2),
        command_values(claim_id="devclaim_stale_generation_commit"),
        command_values(executed_at_ms=200),
        command_values(expected_job_version=1),
    ):
        connection = seed()
        try:
            assert_failed_without_catalog_mutation(
                connection, lambda values=values: insert_device_commit(connection, values)
            )
        finally:
            connection.close()


def test_revoked_binding_and_authorization_fail_closed() -> None:
    connection = seed()
    try:
        connection.execute(
            """
            UPDATE device_actor_bindings
            SET status = 'REVOKED', revoked_at_ms = 90, updated_at_ms = 90
            WHERE tenant_id = ? AND actor_id = ? AND device_id = ?
            """,
            (TENANT, OWNER, DEVICE),
        )
        connection.commit()
        assert_failed_without_catalog_mutation(
            connection, lambda: insert_device_commit(connection, command_values())
        )
    finally:
        connection.close()

    connection = seed()
    try:
        connection.execute(
            """
            UPDATE device_authorizations
            SET status = 'REVOKED', version = 2, revoked_at_ms = 90,
                updated_at_ms = 90, updated_by_actor_id = ?
            WHERE tenant_id = ? AND device_id = ? AND profile_id = ? AND generation_id = ?
            """,
            (OWNER, TENANT, DEVICE, PROFILE, BASE),
        )
        connection.commit()
        assert_failed_without_catalog_mutation(
            connection, lambda: insert_device_commit(connection, command_values())
        )
    finally:
        connection.close()


def test_competing_generation_keeps_loser_non_authoritative() -> None:
    connection = seed()
    try:
        owner_register(connection, WINNER, 80, metadata_digest="1" * 64, container_digest="2" * 64)
        owner_verify(connection, WINNER, 85)
        owner_activate(connection, WINNER, 2, 90)
        connection.commit()
        assert catalog_snapshot(connection)[0] == (WINNER, "READY", 3, 90)

        assert_failed_without_catalog_mutation(
            connection, lambda: insert_device_commit(connection, command_values())
        )
        assert connection.execute(
            """
            SELECT COUNT(*) FROM profile_generations
            WHERE tenant_id = ? AND profile_id = ? AND generation_id = ?
            """,
            (TENANT, PROFILE, CANDIDATE),
        ).fetchone()[0] == 0
        assert catalog_snapshot(connection)[0] == (WINNER, "READY", 3, 90)
    finally:
        connection.close()


def test_stale_coordinator_witness_fails_closed() -> None:
    connection = seed()
    try:
        project_coordinator(
            connection,
            command_id="cmd_projection_advanced",
            expected_version=1,
            version=2,
            sequence=1,
            epoch=2,
            session_id="session_device_generation_new",
            updated_at_ms=80,
        )
        connection.commit()
        assert_failed_without_catalog_mutation(
            connection, lambda: insert_device_commit(connection, command_values())
        )
    finally:
        connection.close()


def test_late_activation_failure_rolls_back_journal_and_candidate() -> None:
    connection = seed()
    try:
        connection.executescript(
            f"""
            CREATE TRIGGER force_device_generation_activation_failure
            BEFORE UPDATE OF active_generation_id ON browser_profiles
            FOR EACH ROW
            WHEN NEW.active_generation_id = '{CANDIDATE}'
            BEGIN
                SELECT RAISE(ABORT, 'forced_device_generation_activation_failure');
            END;
            """
        )
        connection.commit()
        assert_failed_without_catalog_mutation(
            connection, lambda: insert_device_commit(connection, command_values())
        )
        assert catalog_snapshot(connection) == ((BASE, "READY", 2, 40), None, 0)
    finally:
        connection.close()


def test_direct_generation_mutations_remain_governed() -> None:
    connection = seed()
    try:
        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_generations (
                    tenant_id, profile_id, generation_id, object_key, metadata_digest,
                    container_digest, status, version, registered_by_actor_id,
                    created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, 'REGISTERED', 1, ?, 80, 80)
                """,
                (
                    TENANT,
                    PROFILE,
                    CANDIDATE,
                    canonical_key(CANDIDATE),
                    METADATA_DIGEST,
                    CONTAINER_DIGEST,
                    OWNER,
                ),
            )
        )
        connection.rollback()

        owner_register(connection, CANDIDATE, 80)
        connection.commit()
        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE profile_generations
                SET status = 'VERIFIED', version = 2,
                    verification_reference = 'raw_verify_candidate',
                    verified_by_actor_id = ?, verified_at_ms = 90, updated_at_ms = 90
                WHERE tenant_id = ? AND profile_id = ? AND generation_id = ?
                """,
                (OWNER, TENANT, PROFILE, CANDIDATE),
            )
        )
        connection.rollback()

        owner_verify(connection, CANDIDATE, 90)
        connection.commit()
        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE browser_profiles
                SET active_generation_id = ?, version = version + 1,
                    updated_by_actor_id = ?, updated_at_ms = 95
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (CANDIDATE, OWNER, TENANT, PROFILE),
            )
        )
        connection.rollback()
    finally:
        connection.close()


def test_malformed_command_rows_fail_closed() -> None:
    cases = (
        command_values(generation_id=BASE, base_generation_id=BASE),
        command_values(object_key="profiles/not-canonical.enc"),
        command_values(metadata_digest="A" * 64),
        command_values(container_digest="g" * 64),
        command_values(container_bytes=0),
        command_values(container_bytes=83_886_081),
        command_values(coordinator_version=2, coordinator_sequence=0),
    )
    for values in cases:
        connection = seed()
        try:
            assert_failed_without_catalog_mutation(
                connection, lambda values=values: insert_device_commit(connection, values)
            )
        finally:
            connection.close()


def main() -> int:
    test_schema_is_metadata_only()
    test_exact_command_atomically_verifies_and_activates()
    test_stale_claim_fence_and_expired_claim_fail_before_catalog_mutation()
    test_revoked_binding_and_authorization_fail_closed()
    test_competing_generation_keeps_loser_non_authoritative()
    test_stale_coordinator_witness_fails_closed()
    test_late_activation_failure_rolls_back_journal_and_candidate()
    test_direct_generation_mutations_remain_governed()
    test_malformed_command_rows_fail_closed()
    print("Phase 2F device-generation atomic D1 commit invariants are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
