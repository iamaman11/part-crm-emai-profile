#!/usr/bin/env python3
"""Deterministic SQLite proof for the Repository Step 6 local Bridge protocol."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "bridge"


def connect() -> sqlite3.Connection:
    database = sqlite3.connect(":memory:")
    database.execute("PRAGMA foreign_keys = ON")
    for migration in sorted(MIGRATIONS.glob("*.sql")):
        database.executescript(migration.read_text(encoding="utf-8"))
    return database


def result_payload(
    state: str,
    *,
    active_session_id: str | None = None,
    workspace_epoch: int | None = None,
) -> str:
    return json.dumps(
        {
            "state": state,
            "active_session_id": active_session_id,
            "workspace_epoch": workspace_epoch,
        },
        separators=(",", ":"),
        sort_keys=True,
    )


def insert_command(
    database: sqlite3.Connection,
    *,
    command_id: str,
    sequence: int,
    expected_version: int,
    command_type: str,
    payload_json: str,
    result_json: str,
    outbox_event_id: str,
) -> None:
    database.execute(
        """
        INSERT INTO bridge_commands (
            command_id, sequence, expected_version, command_type,
            payload_json, result_json, outbox_event_id, created_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            command_id,
            sequence,
            expected_version,
            command_type,
            payload_json,
            result_json,
            outbox_event_id,
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
        raise AssertionError(f"expected SQLite integrity error: {expected_message}")


def main() -> None:
    database = connect()
    assert database.execute(
        "SELECT version, lifecycle_state FROM bridge_state WHERE singleton = 1"
    ).fetchone() == (1, "idle")

    claim_result = result_payload("claimed")
    insert_command(
        database,
        command_id="command_step6_claim",
        sequence=1,
        expected_version=1,
        command_type="redeem_claim",
        payload_json='{"claim":"redacted"}',
        result_json=claim_result,
        outbox_event_id="outbox_step6_claim",
    )
    assert database.execute(
        "SELECT version, lifecycle_state FROM bridge_state WHERE singleton = 1"
    ).fetchone() == (2, "claimed")

    insert_command(
        database,
        command_id="command_step6_claim",
        sequence=1,
        expected_version=1,
        command_type="redeem_claim",
        payload_json='{"claim":"redacted"}',
        result_json=claim_result,
        outbox_event_id="outbox_step6_retry",
    )
    assert database.execute("SELECT COUNT(*) FROM bridge_commands").fetchone() == (1,)
    assert database.execute("SELECT COUNT(*) FROM bridge_outbox").fetchone() == (1,)

    expect_integrity_error(
        lambda: insert_command(
            database,
            command_id="command_step6_claim",
            sequence=1,
            expected_version=1,
            command_type="redeem_claim",
            payload_json='{"claim":"different"}',
            result_json=claim_result,
            outbox_event_id="outbox_step6_conflict",
        ),
        "bridge_command_conflict",
    )
    database.rollback()

    expect_integrity_error(
        lambda: insert_command(
            database,
            command_id="command_step6_stale",
            sequence=2,
            expected_version=1,
            command_type="acquire_workspace",
            payload_json="{}",
            result_json=result_payload("claimed", workspace_epoch=1),
            outbox_event_id="outbox_step6_stale",
        ),
        "bridge_command_stale_version",
    )
    database.rollback()

    expect_integrity_error(
        lambda: insert_command(
            database,
            command_id="command_step6_gap",
            sequence=3,
            expected_version=2,
            command_type="acquire_workspace",
            payload_json="{}",
            result_json=result_payload("claimed", workspace_epoch=1),
            outbox_event_id="outbox_step6_gap",
        ),
        "bridge_command_reordered",
    )
    database.rollback()

    insert_command(
        database,
        command_id="command_step6_workspace",
        sequence=2,
        expected_version=2,
        command_type="acquire_workspace",
        payload_json="{}",
        result_json=result_payload("claimed", workspace_epoch=1),
        outbox_event_id="outbox_step6_workspace",
    )
    insert_command(
        database,
        command_id="command_step6_start",
        sequence=3,
        expected_version=3,
        command_type="start_runtime",
        payload_json='{"session_id":"session_step6"}',
        result_json=result_payload(
            "starting", active_session_id="session_step6", workspace_epoch=1
        ),
        outbox_event_id="outbox_step6_start",
    )
    insert_command(
        database,
        command_id="command_step6_crash",
        sequence=4,
        expected_version=4,
        command_type="runtime_crashed",
        payload_json='{"session_id":"session_step6"}',
        result_json=result_payload("dirty", workspace_epoch=1),
        outbox_event_id="outbox_step6_crash",
    )
    assert database.execute(
        """
        SELECT version, lifecycle_state, active_session_id, workspace_epoch
        FROM bridge_state WHERE singleton = 1
        """
    ).fetchone() == (5, "dirty", None, 1)

    database.execute(
        """
        UPDATE bridge_outbox
        SET delivery_state = 'DELIVERED', attempts = attempts + 1,
            next_attempt_at_ms = NULL, delivered_at_ms = 500
        WHERE outbox_event_id = 'outbox_step6_claim'
        """
    )
    database.commit()
    assert database.execute(
        """
        SELECT delivery_state, attempts, delivered_at_ms
        FROM bridge_outbox WHERE outbox_event_id = 'outbox_step6_claim'
        """
    ).fetchone() == ("DELIVERED", 1, 500)

    expect_integrity_error(
        lambda: database.execute(
            """
            UPDATE bridge_outbox SET payload_json = '{}'
            WHERE outbox_event_id = 'outbox_step6_claim'
            """
        ),
        "bridge_outbox_payload_immutable",
    )
    database.rollback()

    expect_integrity_error(
        lambda: database.execute(
            "DELETE FROM bridge_commands WHERE command_id = 'command_step6_claim'"
        ),
        "bridge_command_append_only",
    )
    database.rollback()

    assert database.execute("PRAGMA foreign_key_check").fetchall() == []
    assert database.execute("PRAGMA integrity_check").fetchone() == ("ok",)
    print("Repository Step 6 local Bridge invariants passed.")


if __name__ == "__main__":
    main()
