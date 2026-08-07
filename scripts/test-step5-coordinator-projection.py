#!/usr/bin/env python3
"""Deterministic SQLite proof for the Step 5 coordinator projection protocol."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"


def connect() -> sqlite3.Connection:
    database = sqlite3.connect(":memory:")
    database.execute("PRAGMA foreign_keys = ON")
    for migration in sorted(MIGRATIONS.glob("*.sql")):
        database.executescript(migration.read_text(encoding="utf-8"))
    return database


def seed_catalog(database: sqlite3.Connection) -> None:
    database.execute(
        "INSERT INTO tenants VALUES (?, ?, 'ACTIVE', 1, 1, 1)",
        ("tenant_step5", "Step 5 Tenant"),
    )
    database.execute(
        "INSERT INTO identities VALUES (?, ?, ?, ?)",
        ("identity_step5", "subject-step5", "owner@example.invalid", 1),
    )
    database.execute(
        "INSERT INTO memberships VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 1, 1)",
        ("tenant_step5", "actor_step5_owner", "identity_step5"),
    )
    for profile_id, generation_id in (
        ("profile_step5_a", "generation_step5_a"),
        ("profile_step5_b", "generation_step5_b"),
    ):
        database.execute(
            """
            INSERT INTO browser_profiles (
                tenant_id, profile_id, status, active_generation_id, version,
                created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
            ) VALUES (?, ?, 'READY', ?, 1, ?, ?, 1, 1)
            """,
            (
                "tenant_step5",
                profile_id,
                generation_id,
                "actor_step5_owner",
                "actor_step5_owner",
            ),
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
        "tenant_id": "tenant_step5",
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
            "tenant_step5",
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
        ("tenant_step5", "profile_step5_a"),
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
        ("tenant_step5", "profile_step5_a"),
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
        ("tenant_step5", "profile_step5_b"),
    ).fetchone() == (5,)

    expect_integrity_error(
        lambda: database.execute(
            """
            UPDATE profile_coordinator_projection_commands
            SET outcome = 'no_change'
            WHERE tenant_id = ? AND profile_id = ? AND coordinator_sequence = 5
            """,
            ("tenant_step5", "profile_step5_b"),
        ),
        "coordinator_projection_command_append_only",
    )
    database.rollback()

    assert database.execute("PRAGMA foreign_key_check").fetchall() == []
    assert database.execute("PRAGMA integrity_check").fetchone() == ("ok",)
    print("Step 5 coordinator projection invariants passed.")


if __name__ == "__main__":
    main()
