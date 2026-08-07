#!/usr/bin/env python3
"""Deterministic SQLite proof for the Step 5 coordinator projection protocol."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
TENANT_ID = "tenant_step5"
OWNER_ID = "actor_step5_owner"


def connect() -> sqlite3.Connection:
    database = sqlite3.connect(":memory:")
    database.execute("PRAGMA foreign_keys = ON")
    for migration in sorted(MIGRATIONS.glob("*.sql")):
        database.executescript(migration.read_text(encoding="utf-8"))
    return database


def seed_ready_profile(
    database: sqlite3.Connection,
    *,
    profile_id: str,
    generation_id: str,
    ordinal: int,
) -> None:
    database.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, active_generation_id, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', NULL, 1, ?, ?, 1, 1)
        """,
        (TENANT_ID, profile_id, OWNER_ID, OWNER_ID),
    )
    database.execute(
        """
        INSERT INTO profile_generation_register_commands (
            tenant_id, command_id, command_actor_id, profile_id,
            generation_id, object_key, metadata_digest, container_digest,
            executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT_ID,
            f"command_step5_register_{ordinal}",
            OWNER_ID,
            profile_id,
            generation_id,
            f"profiles/v1/{generation_id}.enc",
            "a" * 64,
            "b" * 64,
            10 + ordinal * 10,
        ),
    )
    database.execute(
        """
        INSERT INTO profile_generation_verify_commands (
            tenant_id, command_id, command_actor_id, profile_id,
            generation_id, expected_generation_version,
            verification_reference, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, 1, ?, ?)
        """,
        (
            TENANT_ID,
            f"command_step5_verify_{ordinal}",
            OWNER_ID,
            profile_id,
            generation_id,
            f"review:step5_{ordinal}",
            11 + ordinal * 10,
        ),
    )
    database.execute(
        """
        INSERT INTO profile_generation_activate_commands (
            tenant_id, command_id, command_actor_id, profile_id,
            generation_id, expected_profile_version, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, 1, ?)
        """,
        (
            TENANT_ID,
            f"command_step5_activate_{ordinal}",
            OWNER_ID,
            profile_id,
            generation_id,
            12 + ordinal * 10,
        ),
    )
    row = database.execute(
        """
        SELECT status, active_generation_id, version
        FROM browser_profiles
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT_ID, profile_id),
    ).fetchone()
    assert row == ("READY", generation_id, 2), row


def seed_catalog(database: sqlite3.Connection) -> None:
    database.execute(
        "INSERT INTO tenants VALUES (?, ?, 'ACTIVE', 1, 1, 1)",
        (TENANT_ID, "Step 5 Tenant"),
    )
    database.execute(
        "INSERT INTO identities VALUES (?, ?, ?, ?)",
        ("identity_step5", "subject-step5", "owner@example.invalid", 1),
    )
    database.execute(
        "INSERT INTO memberships VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 1, 1)",
        (TENANT_ID, OWNER_ID, "identity_step5"),
    )
    seed_ready_profile(
        database,
        profile_id="profile_step5_a",
        generation_id="generation_step5_a",
        ordinal=1,
    )
    seed_ready_profile(
        database,
        profile_id="profile_step5_b",
        generation_id="generation_step5_b",
        ordinal=2,
    )
    database.commit()


def projection(
    profile_id: str,
    sequence: int,
    *,
    status: str = "idle",
    next_epoch: int = 0,
) -> str:
    active = status in {"active", "draining"}
    payload = {
        "tenant_id": TENANT_ID,
        "profile_id": profile_id,
        "status": status,
        "version": sequence + 1,
        "sequence": sequence,
        "next_epoch": next_epoch,
        "active_session_id": "session_step5" if active else None,
        "active_device_id": "device_step5" if active else None,
        "active_epoch": next_epoch if active else None,
        "idle_expires_at_ms": 200 if active else None,
        "hard_expires_at_ms": 500 if active else None,
        "drain_deadline_ms": 250 if status == "draining" else None,
        "pending_launch_intent_id": None,
        "pending_intent_expires_at_ms": None,
    }
    return json.dumps(payload, separators=(",", ":"), sort_keys=True)


def insert_command(
    database: sqlite3.Connection,
    *,
    profile_id: str,
    sequence: int,
    outcome: str,
    payload: str,
    event_id: str,
) -> None:
    database.execute(
        """
        INSERT OR IGNORE INTO profile_coordinator_projection_commands (
            tenant_id, profile_id, coordinator_sequence, coordinator_version,
            outbox_event_id, outcome, projection_json, projected_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT_ID,
            profile_id,
            sequence,
            sequence + 1,
            event_id,
            outcome,
            payload,
            100 + sequence,
        ),
    )
    database.commit()


def expect_integrity_error(operation, expected_message: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert expected_message in str(error), (expected_message, str(error))
    else:
        raise AssertionError(f"expected sqlite integrity error: {expected_message}")


def main() -> None:
    database = connect()
    seed_catalog(database)

    initial = projection("profile_step5_a", 0)
    insert_command(
        database,
        profile_id="profile_step5_a",
        sequence=0,
        outcome="snapshot",
        payload=initial,
        event_id="outbox_step5_0000",
    )
    row = database.execute(
        """
        SELECT coordinator_sequence, coordinator_version, coordinator_status
        FROM profile_coordinator_projections
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT_ID, "profile_step5_a"),
    ).fetchone()
    assert row == (0, 1, "idle"), row
    assert database.execute(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_type = 'profile_coordinator'"
    ).fetchone() == (1,)

    insert_command(
        database,
        profile_id="profile_step5_a",
        sequence=0,
        outcome="snapshot",
        payload=initial,
        event_id="outbox_step5_retry",
    )
    assert database.execute(
        "SELECT COUNT(*) FROM profile_coordinator_projection_commands"
    ).fetchone() == (1,)
    assert database.execute(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_type = 'profile_coordinator'"
    ).fetchone() == (1,)

    expect_integrity_error(
        lambda: insert_command(
            database,
            profile_id="profile_step5_a",
            sequence=0,
            outcome="no_change",
            payload=initial,
            event_id="outbox_step5_conflict",
        ),
        "coordinator_projection_conflict",
    )
    database.rollback()

    active = projection("profile_step5_a", 2, status="active", next_epoch=1)
    insert_command(
        database,
        profile_id="profile_step5_a",
        sequence=2,
        outcome="lease_claimed",
        payload=active,
        event_id="outbox_step5_0002",
    )
    assert database.execute(
        """
        SELECT coordinator_sequence, active_epoch, active_session_id
        FROM profile_coordinator_projections
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT_ID, "profile_step5_a"),
    ).fetchone() == (2, 1, "session_step5")

    expect_integrity_error(
        lambda: insert_command(
            database,
            profile_id="profile_step5_a",
            sequence=1,
            outcome="launch_intent_issued",
            payload=projection("profile_step5_a", 1),
            event_id="outbox_step5_stale",
        ),
        "coordinator_projection_stale",
    )
    database.rollback()

    repaired = projection("profile_step5_b", 5)
    insert_command(
        database,
        profile_id="profile_step5_b",
        sequence=5,
        outcome="snapshot",
        payload=repaired,
        event_id="outbox_step5_repair",
    )
    assert database.execute(
        """
        SELECT coordinator_sequence
        FROM profile_coordinator_projections
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT_ID, "profile_step5_b"),
    ).fetchone() == (5,)

    expect_integrity_error(
        lambda: database.execute(
            """
            UPDATE profile_coordinator_projection_commands
            SET outcome = 'no_change'
            WHERE tenant_id = ? AND profile_id = ? AND coordinator_sequence = 5
            """,
            (TENANT_ID, "profile_step5_b"),
        ),
        "coordinator_projection_command_append_only",
    )
    database.rollback()

    assert database.execute("PRAGMA foreign_key_check").fetchall() == []
    assert database.execute("PRAGMA integrity_check").fetchone() == ("ok",)
    print("Step 5 coordinator projection invariants passed.")


if __name__ == "__main__":
    main()
