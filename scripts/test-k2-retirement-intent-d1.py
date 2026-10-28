#!/usr/bin/env python3
"""#802 K2: D1 retire admission + no-reactivation proof. No R2 deletes."""
from __future__ import annotations

import re
import runpy
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FX = runpy.run_path(str(ROOT / "scripts/test-device-generation-commit-d1.py"))
STREAM = ROOT / "migrations/d1-successor-v2/0037_generation_streaming_container_limit.sql"
WORKING = ROOT / "migrations/d1-successor-v2/0038_profile_generation_working_proof.sql"
RETIRE = ROOT / "migrations/d1-successor-v2/0039_profile_generation_retirement_guard.sql"


def record(conn: sqlite3.Connection, **changes: object) -> None:
    candidate = conn.execute(
        "SELECT object_key, metadata_digest, container_digest "
        "FROM profile_generations WHERE tenant_id=? AND profile_id=? AND generation_id=?",
        (FX["TENANT"], FX["PROFILE"], FX["CANDIDATE"]),
    ).fetchone()
    data = dict(
        tenant_id=FX["TENANT"], profile_id=FX["PROFILE"],
        generation_id=FX["CANDIDATE"],
        observed_current_generation_id=FX["WINNER"],
        retained_working_generation_id=FX["BASE"],
        expected_profile_version=4,
        object_key=candidate["object_key"],
        metadata_digest=candidate["metadata_digest"],
        container_digest=candidate["container_digest"],
        approved_runtime_inventory_sha256="e" * 64,
        admitted_by_actor_id=FX["OWNER"],
        admitted_by_device_id=FX["DEVICE"],
        coordinator_session_id=FX["SESSION"],
        coordinator_epoch=1, admitted_at_ms=130,
    )
    data.update(changes)
    conn.execute(
        "INSERT INTO profile_generation_retirement_intents ("
        + ", ".join(data) + ") VALUES (" + ", ".join("?" for _ in data) + ")",
        tuple(data.values()),
    )


def authorize_current_device(conn: sqlite3.Connection) -> None:
    # BASE grant alone cannot permit retirement when WINNER is CURRENT.
    conn.execute(
        """
        INSERT INTO device_authorizations (
            tenant_id, device_id, profile_id, generation_id, version, status,
            evidence_reference, authorized_by_actor_id, updated_by_actor_id,
            authorized_at_ms, updated_at_ms, revoked_at_ms
        ) VALUES (?, ?, ?, ?, 1, 'ACTIVE', 'evidence_current_maintenance',
                  ?, ?, 121, 121, NULL)
        """,
        (FX["TENANT"], FX["DEVICE"], FX["PROFILE"], FX["WINNER"],
         FX["OWNER"], FX["OWNER"]),
    )


def refused(conn: sqlite3.Connection, **change: object) -> None:
    before = conn.execute(
        "SELECT COUNT(*) FROM profile_generation_retirement_intents"
    ).fetchone()[0]
    try:
        record(conn, **change)
    except sqlite3.IntegrityError:
        conn.rollback()
    else:
        raise AssertionError(f"unsafe retirement admitted: {change}")
    assert conn.execute(
        "SELECT COUNT(*) FROM profile_generation_retirement_intents"
    ).fetchone()[0] == before


def main() -> None:
    conn = FX["seed"]()
    try:
        for path in (STREAM, WORKING, RETIRE):
            conn.executescript(path.read_text(encoding="utf-8"))
        FX["insert_device_commit"](conn, FX["command_values"]())
        conn.commit()
        refused(conn)  # A VERIFIED predecessor is not a browser WORKING proof.
        working_test = runpy.run_path(str(ROOT / "scripts/test-k2-working-proof-d1.py"))
        working_test["insert"](conn, FX)
        conn.commit()
        refused(conn)  # Candidate is CURRENT and must remain immutable.
        FX["owner_register"](conn, FX["WINNER"], 110)
        FX["owner_verify"](conn, FX["WINNER"], 115)
        FX["owner_activate"](conn, FX["WINNER"], 3, 120)
        conn.commit()
        refused(conn)  # BASE grant cannot be borrowed for current WINNER
        authorize_current_device(conn)
        conn.commit()
        refused(conn, admitted_at_ms=120)  # grant not issued until 121
        conn.execute(
            "UPDATE device_authorizations SET status='REVOKED', "
            "revoked_at_ms=123, updated_at_ms=123 "
            "WHERE tenant_id=? AND profile_id=? AND generation_id=? AND device_id=?",
            (FX["TENANT"], FX["PROFILE"], FX["WINNER"], FX["DEVICE"]),
        )
        refused(conn)  # revocation invalidates admission even with live binding
        assert conn.execute(
            "SELECT status FROM device_authorizations "
            "WHERE tenant_id=? AND profile_id=? AND generation_id=? AND device_id=?",
            (FX["TENANT"], FX["PROFILE"], FX["WINNER"], FX["DEVICE"]),
        ).fetchone()[0] == "ACTIVE"
        refused(conn, generation_id=FX["WINNER"])
        refused(conn, generation_id=FX["BASE"])
        refused(conn, observed_current_generation_id=FX["CANDIDATE"])
        refused(conn, retained_working_generation_id=FX["CANDIDATE"])
        refused(conn, expected_profile_version=3)
        refused(conn, metadata_digest="f" * 64)
        refused(conn, container_digest="f" * 64)
        refused(conn, approved_runtime_inventory_sha256="f" * 64)
        refused(conn, approved_runtime_inventory_sha256="E" * 64)
        refused(conn, admitted_by_actor_id="other_actor")
        refused(conn, admitted_by_device_id="other_device")
        refused(conn, coordinator_epoch=0)
        record(conn)
        conn.commit()
        # Execute the actual D1 owner's insert and replay SQL, not a handwritten
        # approximation. A lost response is accepted ONLY with the exact
        # immutable original witness and timestamp.
        source = (ROOT / "crates/cloudflare-adapters/src/d1_profile_generation_successor.rs").read_text(encoding="utf-8")
        insert_sql = re.search(
            r'const INSERT_GENERATION_RETIREMENT_INTENT: &str = r#"(.*?)"#;',
            source, re.DOTALL,
        )
        replay_sql = re.search(
            r'const LOAD_EXACT_GENERATION_RETIREMENT_INTENT: &str = r#"(.*?)"#;',
            source, re.DOTALL,
        )
        assert insert_sql is not None and replay_sql is not None
        row = conn.execute(
            "SELECT * FROM profile_generation_retirement_intents"
        ).fetchone()
        values = tuple(row[name] for name in (
            "tenant_id", "profile_id", "generation_id",
            "observed_current_generation_id", "retained_working_generation_id",
            "expected_profile_version", "object_key", "metadata_digest",
            "container_digest", "approved_runtime_inventory_sha256",
            "admitted_by_actor_id", "admitted_by_device_id",
            "coordinator_session_id", "coordinator_epoch", "admitted_at_ms",
        ))
        assert conn.execute(insert_sql.group(1), values).fetchone() is None
        assert conn.execute(replay_sql.group(1), values).fetchone()[0] == FX["CANDIDATE"]
        for index in (3, 4, 5, 8, 9, 11, 12, 13, 14):
            wrong = list(values)
            wrong[index] = ("f" * 64) if index in (8, 9) else (
                int(wrong[index]) + 1 if isinstance(wrong[index], int)
                else "mismatched_witness"
            )
            assert conn.execute(replay_sql.group(1), wrong).fetchone() is None
        conn.commit()
        saved = conn.execute(
            "SELECT generation_id FROM profile_generation_retirement_intents"
        ).fetchone()
        assert saved["generation_id"] == FX["CANDIDATE"]
        refused(conn)  # Never admit a second command with a different witness.
        for statement in (
            "UPDATE profile_generation_retirement_intents SET admitted_at_ms=150",
            "DELETE FROM profile_generation_retirement_intents",
        ):
            try:
                conn.execute(statement)
            except sqlite3.IntegrityError:
                conn.rollback()
            else:
                raise AssertionError("Retirement receipt was mutable")
        try:
            FX["owner_activate"](conn, FX["CANDIDATE"], 4, 140)
        except sqlite3.IntegrityError:
            conn.rollback()
        else:
            raise AssertionError("Catalog reactivated retired history")
        try:
            conn.execute(
                "UPDATE browser_profiles SET active_generation_id=? "
                "WHERE tenant_id=? AND profile_id=?",
                (FX["CANDIDATE"], FX["TENANT"], FX["PROFILE"]),
            )
        except sqlite3.IntegrityError:
            conn.rollback()
        else:
            raise AssertionError("Direct CURRENT pointer reused retired history")
        source = (ROOT / "crates/cloudflare-adapters/src/d1_profile_generation_successor.rs").read_text(encoding="utf-8")
        query = re.search(
            r'const LOAD_UNPROVEN_HISTORICAL_PRUNE_REVIEW: &str = r#"(.*?)"#;',
            source, re.DOTALL,
        )
        assert query is not None
        assert conn.execute(
            query.group(1), (FX["TENANT"], FX["PROFILE"], FX["WINNER"], "e" * 64)
        ).fetchone() is None, "retired generation must never be reviewed twice"
        assert conn.execute(
            "SELECT active_generation_id FROM browser_profiles WHERE tenant_id=? AND profile_id=?",
            (FX["TENANT"], FX["PROFILE"]),
        ).fetchone()[0] == FX["WINNER"]
        assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
        print("K2 retirement append-only and both reactivation routes fail closed; R2 DELETE=0 PASS")
    finally:
        conn.close()


if __name__ == "__main__":
    main()
