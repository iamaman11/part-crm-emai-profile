#!/usr/bin/env python3
"""#802 K2: immutable historical metadata receipts and safe runtime gate.

The synthetic D1 SQL fixture exercises schema integrity and historical
selection semantics. Production cannot select these metadata-only records
as LKG or R2 pruning authority until a reviewed native data proof exists.
"""
from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "scripts/test-device-generation-commit-d1.py"
STREAM = ROOT / "migrations/d1-successor-v2/0037_generation_streaming_container_limit.sql"
WORKING = ROOT / "migrations/d1-successor-v2/0038_profile_generation_working_proof.sql"
RETIREMENT = ROOT / "migrations/d1-successor-v2/0039_profile_generation_retirement_guard.sql"
NATIVE = ROOT / "migrations/d1-successor-v2/0040_native_browser_recovery_proof.sql"


def insert(conn: sqlite3.Connection, fx: dict, **changes: object) -> None:
    data = dict(
        tenant_id=fx["TENANT"],
        profile_id=fx["PROFILE"],
        generation_id=fx["BASE"],
        observed_current_generation_id=fx["CANDIDATE"],
        metadata_digest="a" * 64,
        container_digest="b" * 64,
        root_key_id="profile-generation-root-v1-1",
        runtime_inventory_sha256="e" * 64,
        observed_by_actor_id=fx["OWNER"],
        observed_by_device_id=fx["DEVICE"],
        coordinator_session_id=fx["SESSION"],
        coordinator_epoch=1,
        proven_at_ms=105,
    )
    data.update(changes)
    columns = ", ".join(data)
    placeholders = ", ".join("?" for _ in data)
    conn.execute(
        f"INSERT INTO profile_generation_working_proofs ({columns}) VALUES ({placeholders})",
        tuple(data.values()),
    )


def rejected(conn: sqlite3.Connection, fx: dict, **changes: object) -> None:
    try:
        insert(conn, fx, **changes)
    except sqlite3.IntegrityError:
        conn.rollback()
    else:
        raise AssertionError(f"unsafe working proof was admitted: {sorted(changes)}")
    assert conn.execute("SELECT COUNT(*) FROM profile_generation_working_proofs").fetchone()[0] == 0


def main() -> int:
    # The old WORKING/LKG SQL lineage remains an immutable migration fixture,
    # never an executable app retirement route after schema0042 cutover.
    worker = (
        ROOT / "apps/control-plane-worker/src/profile_generation_successor_ingress.rs"
    ).read_text(encoding="utf-8")
    adapter = (
        ROOT / "crates/cloudflare-adapters/src/d1_profile_generation_successor.rs"
    ).read_text(encoding="utf-8")
    unified = (
        ROOT / "crates/cloudflare-adapters/src/d1_profile_generation_successor/current_retirement.rs"
    ).read_text(encoding="utf-8")
    assert "async fn dispatch_working_proof(" not in worker
    assert "async fn retire_one_old_generation(" not in worker
    assert "historical_predecessor_reopen_descriptor(" not in worker
    assert "record_historical_working_proof(" not in adapter
    assert "load_certified_working_reserve(" not in adapter
    assert "profile_generation_retirement_intents" in unified
    assert "profile_generation_current_retirement_intents" not in unified

    fx = runpy.run_path(str(FIXTURE))
    conn = fx["seed"]()
    try:
        for migration in (STREAM, WORKING, RETIREMENT, NATIVE):
            conn.executescript(migration.read_text(encoding="utf-8"))
        rejected(conn, fx)  # missing CURRENT
        fx["insert_device_commit"](conn, fx["command_values"]())
        conn.commit()
        rejected(conn, fx, native_snapshot_sha256="a" * 64,
                 native_browser_witness_sha256="b" * 64)
        rejected(conn, fx, native_snapshot_sha256="A" * 64,
                 native_browser_witness_sha256="A" * 64)
        rejected(conn, fx, generation_id=fx["CANDIDATE"])
        rejected(conn, fx, metadata_digest="f" * 64)
        insert(conn, fx, native_snapshot_sha256="a" * 64,
               native_browser_witness_sha256="a" * 64)
        conn.commit()
        assert conn.execute(
            "SELECT COUNT(*) FROM profile_generation_working_proofs"
        ).fetchone()[0] == 1
        for statement in (
            "UPDATE profile_generation_working_proofs SET proven_at_ms=999",
            "DELETE FROM profile_generation_working_proofs",
        ):
            try:
                conn.execute(statement)
            except sqlite3.IntegrityError:
                conn.rollback()
            else:
                raise AssertionError("immutable historical WORKING receipt changed")
        assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
    finally:
        conn.close()
    print("Historical 0039/0040 integrity is retained; production WORKING/LKG path absent PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
