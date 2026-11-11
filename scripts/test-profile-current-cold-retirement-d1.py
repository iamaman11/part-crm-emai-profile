#!/usr/bin/env python3
"""#849: one CURRENT cold-restore retirement is a D1-guarded exact-key intent.

SQL-only, synthetic records. No R2 access and no permission to call DELETE.
The production Worker must independently authenticate actual CURRENT BPGC
metadata, signed device Restore and complete native-byte comparison before
inserting this receipt.
"""
from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FX = runpy.run_path(str(ROOT / "scripts/test-device-generation-commit-d1.py"))
ROOT_MIGRATIONS = ROOT / "migrations/d1-successor-v2"

COLUMNS = (
    "tenant_id", "profile_id", "generation_id", "observed_current_generation_id",
    "expected_profile_version", "object_key", "metadata_digest", "container_digest",
    "current_object_key", "current_metadata_digest", "current_container_digest",
    "sealed_current_native_sha256", "restored_current_native_sha256",
    "admitted_by_actor_id", "admitted_by_device_id", "admitted_at_ms", "not_before_at_ms",
)


def setup(*, extra_signed_put: bool = False) -> sqlite3.Connection:
    db = FX["seed"]()
    for revision in range(37, 43):
        matches = list(ROOT_MIGRATIONS.glob(f"{revision:04d}_*.sql"))
        assert len(matches) == 1, (revision, matches)
        db.executescript(matches[0].read_text(encoding="utf-8"))
    # One canonical journal; the parallel cold-current table must not exist.
    assert db.execute("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='profile_generation_current_retirement_intents'").fetchone()[0] == 0
    if extra_signed_put:
        # A signed old-generation PUT might be issued before the successor
        # CAS. Future R2 absence must wait until *its* real expiry as well.
        db.execute(
            "INSERT INTO profile_generation_upload_reservations "
            "(tenant_id, profile_id, generation_id, object_key, metadata_digest, "
            "container_digest, source_mode, source_generation_id, "
            "expected_profile_version, issued_at_ms, expires_at_ms) "
            "VALUES (?,?,?,?,?,?,'READY',?,2,500,360500)",
            (
                FX["TENANT"], FX["PROFILE"], FX["BASE"], FX["canonical_key"](FX["BASE"]),
                "a" * 64, "b" * 64, FX["BASE"],
            ),
        )
    FX["insert_device_commit"](db, FX["command_values"]())
    actual = db.execute(
        "SELECT status,version,active_generation_id FROM browser_profiles "
        "WHERE tenant_id=? AND profile_id=?",
        (FX["TENANT"], FX["PROFILE"]),
    ).fetchone()
    assert tuple(actual) == ("READY", 3, FX["CANDIDATE"])
    # CURRENT Restore authorized the *same* authenticated Windows device.
    db.execute(
        "INSERT INTO device_authorizations "
        "(tenant_id,device_id,profile_id,generation_id,version,status,"
        "evidence_reference,authorized_by_actor_id,updated_by_actor_id,"
        "authorized_at_ms,updated_at_ms,revoked_at_ms) "
        "VALUES (?,?,?,?,1,'ACTIVE','cold_restore_exact_current',?,?,150,150,NULL)",
        (FX["TENANT"], FX["DEVICE"], FX["PROFILE"], FX["CANDIDATE"],
         FX["OWNER"], FX["OWNER"]),
    )
    # The existing DO close projection has reached the idle state.
    db.execute(
        "UPDATE profile_coordinator_projections "
        "SET coordinator_status='idle', active_session_id=NULL,active_device_id=NULL,"
        "active_epoch=NULL,idle_expires_at_ms=NULL,hard_expires_at_ms=NULL,"
        "projected_at_ms=200 "
        "WHERE tenant_id=? AND profile_id=?",
        (FX["TENANT"], FX["PROFILE"]),
    )
    db.commit()
    return db


def intent(**overrides: object) -> dict[str, object]:
    data = {
        "tenant_id": FX["TENANT"],
        "profile_id": FX["PROFILE"],
        "generation_id": FX["BASE"],
        "observed_current_generation_id": FX["CANDIDATE"],
        "expected_profile_version": 3,
        "object_key": FX["canonical_key"](FX["BASE"]),
        "metadata_digest": "a" * 64,
        "container_digest": "b" * 64,
        "current_object_key": FX["canonical_key"](FX["CANDIDATE"]),
        "current_metadata_digest": FX["METADATA_DIGEST"],
        "current_container_digest": FX["CONTAINER_DIGEST"],
        "sealed_current_native_sha256": "f" * 64,
        "restored_current_native_sha256": "f" * 64,
        "admitted_by_actor_id": FX["OWNER"],
        "admitted_by_device_id": FX["DEVICE"],
        "admitted_at_ms": 500,
        "not_before_at_ms": 360500,
    }
    data.update(overrides)
    return data


def admit(db: sqlite3.Connection, **changes: object) -> None:
    data = intent(**changes)
    assert set(data) == set(COLUMNS)
    db.execute(
        "INSERT INTO profile_generation_retirement_intents ("
        + ",".join(COLUMNS) + ") VALUES ("
        + ",".join("?" for _ in COLUMNS) + ")",
        tuple(data[name] for name in COLUMNS),
    )


def denied(db: sqlite3.Connection, **changes: object) -> None:
    db.execute("SAVEPOINT negative_admission")
    try:
        admit(db, **changes)
    except sqlite3.IntegrityError:
        db.execute("ROLLBACK TO negative_admission")
        db.execute("RELEASE negative_admission")
    else:
        db.execute("ROLLBACK TO negative_admission")
        db.execute("RELEASE negative_admission")
        raise AssertionError(f"unsafe CURRENT retirement accepted: {changes}")


def test_exact_current_and_one_way_receipt() -> None:
    db = setup()
    try:
        denied(db, restored_current_native_sha256="0" * 64)
        denied(db, expected_profile_version=2)
        denied(db, observed_current_generation_id=FX["BASE"])
        denied(db, current_container_digest="1" * 64)
        denied(db, metadata_digest="2" * 64)
        denied(db, not_before_at_ms=360499)
        denied(db, admitted_by_device_id="device_not_authorized_01")

        # Live coordinator is not a closed profile. Never grant a DELETE.
        db.execute(
            "UPDATE profile_coordinator_projections SET coordinator_status='active',"
            "active_session_id=?,active_device_id=?,active_epoch=1,"
            "idle_expires_at_ms=2000,hard_expires_at_ms=3000 "
            "WHERE tenant_id=? AND profile_id=?",
            (FX["SESSION"], FX["DEVICE"], FX["TENANT"], FX["PROFILE"]),
        )
        denied(db)
        db.execute(
            "UPDATE profile_coordinator_projections SET coordinator_status='idle',"
            "active_session_id=NULL,active_device_id=NULL,active_epoch=NULL,"
            "idle_expires_at_ms=NULL,hard_expires_at_ms=NULL "
            "WHERE tenant_id=? AND profile_id=?",
            (FX["TENANT"], FX["PROFILE"]),
        )

        # An offline second device still relies on the older snapshot.
        db.execute(
            "INSERT INTO device_authorizations "
            "(tenant_id,device_id,profile_id,generation_id,version,status,"
            "evidence_reference,authorized_by_actor_id,updated_by_actor_id,"
            "authorized_at_ms,updated_at_ms,revoked_at_ms) "
            "VALUES (?,?,?,?,1,'ACTIVE','other_device_restore',?,?,180,180,NULL)",
            (FX["TENANT"], "device_second_test_01", FX["PROFILE"], FX["BASE"],
             FX["OWNER"], FX["OWNER"]),
        )
        denied(db)
        db.execute(
            "DELETE FROM device_authorizations WHERE device_id='device_second_test_01'"
        )

        # Successful admission revokes ONLY the original device's old grant.
        admit(db)
        rows = db.execute(
            "SELECT generation_id,status FROM device_authorizations WHERE tenant_id=? "
            "AND profile_id=? ORDER BY generation_id",
            (FX["TENANT"], FX["PROFILE"]),
        ).fetchall()
        assert [(row["generation_id"], row["status"]) for row in rows] == sorted(
            [(FX["BASE"], "REVOKED"), (FX["CANDIDATE"], "ACTIVE")]
        )
        # Exact replay may not issue a second different retirement intent.
        denied(db)
        try:
            db.execute(
                "UPDATE profile_generation_retirement_intents "
                "SET deleted_at_ms=360499"
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("early R2 deletion receipt accepted")
        db.execute(
            "UPDATE profile_generation_retirement_intents "
            "SET deleted_at_ms=360500"
        )
        try:
            db.execute(
                "UPDATE profile_generation_retirement_intents "
                "SET deleted_at_ms=360600"
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("a completed exact R2 receipt was overwritten")
        try:
            db.execute(
                "UPDATE device_authorizations SET status='ACTIVE',revoked_at_ms=NULL "
                "WHERE generation_id=? AND profile_id=?",
                (FX["BASE"], FX["PROFILE"]),
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("retired generation reauthorized")
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
    finally:
        db.close()


def test_signed_put_expiry_prevents_early_intent() -> None:
    db = setup(extra_signed_put=True)
    try:
        denied(db, not_before_at_ms=360400)
        admit(db, not_before_at_ms=360500)
        assert db.execute(
            "SELECT not_before_at_ms,deleted_at_ms "
            "FROM profile_generation_retirement_intents"
        ).fetchone()[0] == 360500
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
    finally:
        db.close()


def main() -> int:
    test_exact_current_and_one_way_receipt()
    test_signed_put_expiry_prevents_early_intent()
    print("Cold CURRENT D1 retirement: CAS, exact key, device grants, idle "
          "coordinator, signed PUT expiry, one-way receipt and negative fixtures PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
