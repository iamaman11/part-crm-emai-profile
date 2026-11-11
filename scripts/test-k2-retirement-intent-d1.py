#!/usr/bin/env python3
"""#802 K2: exact D1 retirement and one-way R2 effect receipts."""
from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FX = runpy.run_path(str(ROOT / "scripts/test-device-generation-commit-d1.py"))
STREAM = ROOT / "migrations/d1-successor-v2/0037_generation_streaming_container_limit.sql"
WORKING = ROOT / "migrations/d1-successor-v2/0038_profile_generation_working_proof.sql"
RETIRE = ROOT / "migrations/d1-successor-v2/0039_profile_generation_retirement_guard.sql"
NATIVE = ROOT / "migrations/d1-successor-v2/0040_native_browser_recovery_proof.sql"


def record(
    conn: sqlite3.Connection,
    *,
    candidate_id: str = FX["CANDIDATE"],
    **changes: object,
) -> None:
    candidate = conn.execute(
        "SELECT object_key, metadata_digest, container_digest "
        "FROM profile_generations WHERE tenant_id=? AND profile_id=? AND generation_id=?",
        (FX["TENANT"], FX["PROFILE"], candidate_id),
    ).fetchone()
    data = dict(
        tenant_id=FX["TENANT"], profile_id=FX["PROFILE"],
        generation_id=candidate_id,
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
        for path in (STREAM, WORKING, RETIRE, NATIVE):
            conn.executescript(path.read_text(encoding="utf-8"))
        FX["insert_device_commit"](conn, FX["command_values"]())
        conn.commit()
        refused(conn)  # A VERIFIED predecessor is not a browser WORKING proof.
        working_test = runpy.run_path(str(ROOT / "scripts/test-k2-working-proof-d1.py"))
        working_test["insert"](conn, FX, native_snapshot_sha256="a" * 64, native_browser_witness_sha256="a" * 64)
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
        # A historical device can retain its own authorized generation.
        # The job foreign key requires the exact old-generation grant.
        conn.execute(
            """
            INSERT INTO device_authorizations (
                tenant_id, device_id, profile_id, generation_id, version, status,
                evidence_reference, authorized_by_actor_id, updated_by_actor_id,
                authorized_at_ms, updated_at_ms, revoked_at_ms
            ) VALUES (?, ?, ?, ?, 1, 'ACTIVE', 'evidence_old_maintenance',
                      ?, ?, 124, 124, NULL)
            """,
            (FX["TENANT"], FX["DEVICE"], FX["PROFILE"], FX["CANDIDATE"],
             FX["OWNER"], FX["OWNER"]),
        )
        conn.commit()
        refused(conn)  # ACTIVE historical grant may belong to offline device
        # The existing D1 owner's retirement admission must reject any
        # unresolved device job on the historical candidate. It is not proof
        # of clean local Firefox bytes, even if CURRENT/WORKING are valid.
        conn.execute(
            """
            INSERT INTO device_jobs (
                tenant_id, job_id, device_id, profile_id, generation_id,
                aggregate_version, status, attempt, max_attempts, last_fence,
                current_claim_id, claim_fence, claimed_at_ms,
                claim_heartbeat_at_ms, claim_lease_expires_at_ms, retry_at_ms,
                updated_at_ms
            ) VALUES (?, 'job_retirement_pending_01', ?, ?, ?, 1,
                      'PENDING_DEVICE', 0, 3, 0, NULL, NULL, NULL, NULL,
                      NULL, NULL, 125)
            """,
            (FX["TENANT"], FX["DEVICE"], FX["PROFILE"], FX["CANDIDATE"]),
        )
        conn.commit()
        refused(conn)  # active historical grant AND pending job
        conn.execute(
            "UPDATE device_authorizations SET status='REVOKED', "
            "revoked_at_ms=126, updated_at_ms=126 "
            "WHERE tenant_id=? AND profile_id=? AND generation_id=? AND device_id=?",
            (FX["TENANT"], FX["PROFILE"], FX["CANDIDATE"], FX["DEVICE"]),
        )
        conn.commit()
        refused(conn)  # revoked grant alone is NOT enough: job still pending
        assert conn.execute(
            "SELECT status FROM device_jobs WHERE job_id='job_retirement_pending_01'"
        ).fetchone()[0] == "PENDING_DEVICE"
        conn.execute(
            "DELETE FROM device_jobs WHERE job_id='job_retirement_pending_01'"
        )
        conn.commit()
        refused(conn, admitted_by_actor_id="other_actor")
        refused(conn, admitted_by_device_id="other_device")
        refused(conn, coordinator_epoch=0)
        record(conn)
        conn.commit()
        # This is historical schema0039/0040, not today's executable
        # retirement path; immutable old receipts still fail closed.
        saved = conn.execute(
            "SELECT generation_id FROM profile_generation_retirement_intents"
        ).fetchone()
        assert saved["generation_id"] == FX["CANDIDATE"]
        refused(conn)
        for statement in (
            "UPDATE profile_generation_retirement_intents SET admitted_at_ms=150",
            "DELETE FROM profile_generation_retirement_intents",
        ):
            try:
                conn.execute(statement)
            except sqlite3.IntegrityError:
                conn.rollback()
            else:
                raise AssertionError("historical retirement receipt was mutated")

        # The new cutover may not silently delete a pending historical intent.
        # No side effects are allowed before schema0042's explicit empty check.
        migration41 = ROOT / "migrations/d1-successor-v2/0041_profile_delete_exact_r2_receipts.sql"
        migration42 = ROOT / "migrations/d1-successor-v2/0042_profile_current_cold_retirement.sql"
        conn.executescript(migration41.read_text(encoding="utf-8"))
        try:
            conn.executescript(migration42.read_text(encoding="utf-8"))
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("schema0042 discarded an existing retirement receipt")
        assert conn.execute(
            "SELECT generation_id FROM profile_generation_retirement_intents"
        ).fetchone()[0] == FX["CANDIDATE"]
        assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
        print("Historical retirement CAS/ACL/grants/jobs immutable; 0042 fail-closed cutover PASS")
    finally:
        conn.close()


if __name__ == "__main__":
    main()
    runpy.run_path(str(ROOT / "scripts/test-profile-delete-d1.py"))["main"]()
