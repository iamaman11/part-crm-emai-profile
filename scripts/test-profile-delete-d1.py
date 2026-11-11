#!/usr/bin/env python3
"""#849: new D1 profile deletion freeze/receipts are exact and replay-safe.

This is a SQL-only test; it never requests R2 mutations. A production worker
must HEAD exact metadata, DELETE the exact key, and re-HEAD absent before
issuing a matching immutable D1 receipt.
"""
from __future__ import annotations

import runpy
import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = runpy.run_path(str(ROOT / "scripts/test-d1-schema.py"))
SUCCESSOR = ROOT / "migrations/d1-successor-v2"


def expect_denied(op) -> None:
    try:
        op()
    except sqlite3.IntegrityError:
        return
    raise AssertionError("unsafe profile deletion state transition accepted")


def make_database() -> sqlite3.Connection:
    connection = BASE["open_database"]()
    BASE["apply_migrations"](connection)
    # The accepted Catalog 0001-0031 base precedes the v2 lineage.
    # D1 0035 is a contract migration and must not be executed directly
    # against 0031. K2 uses the same physical 0037..0040 test schema.
    migrations = [
        path for path in sorted(SUCCESSOR.glob("[0-9][0-9][0-9][0-9]_*.sql"))
        if 37 <= int(path.name[:4]) <= 41
    ]
    versions = [int(path.name[:4]) for path in migrations]
    assert versions == list(range(37, 42)), versions
    for migration in migrations:
        connection.executescript(migration.read_text(encoding="utf-8"))
    BASE["seed_catalog"](connection)
    return connection


def begin(connection: sqlite3.Connection, profile: str, version: int, at_ms: int,
          actor: str | None = None) -> None:
    connection.execute(
        "INSERT INTO profile_delete_commands "
        "(tenant_id,profile_id,command_id,command_actor_id,expected_profile_version,requested_at_ms) "
        "VALUES (?,?,?,?,?,?)",
        (BASE["TENANT_A"], profile, f"command_delete_{profile}",
         actor or BASE["OWNER_A"], version, at_ms),
    )


def finish(connection: sqlite3.Connection, profile: str, version: int,
           at_ms: int) -> None:
    connection.execute(
        "INSERT INTO profile_delete_completion_commands "
        "(tenant_id,profile_id,command_actor_id,expected_profile_version,completed_at_ms) "
        "VALUES (?,?,?,?,?)",
        (BASE["TENANT_A"], profile, BASE["OWNER_A"], version, at_ms),
    )


def test_draft_direct_exact_deletion() -> None:
    connection = make_database()
    try:
        profile = BASE["PROFILE_A"]
        expect_denied(lambda: begin(connection, profile, 1, 40, BASE["MEMBER_A"]))
        expect_denied(lambda: begin(connection, profile, 3, 40))
        # A signed upload may exist in R2 before its D1 generation commit.
        # Freeze that exact key even though profile_generations is empty.
        generation = "generation_unsigned_draft_01"
        key = f"profiles/v1/{generation}.enc"
        metadata_digest, container_digest = "a" * 64, "b" * 64
        connection.execute(
            "INSERT INTO profile_generation_upload_reservations "
            "(tenant_id,profile_id,generation_id,object_key,metadata_digest,"
            "container_digest,source_mode,source_generation_id,"
            "expected_profile_version,issued_at_ms,expires_at_ms) "
            "VALUES (?,?,?,?,?,?,'DRAFT',NULL,1,35,360035)",
            (BASE["TENANT_A"],profile,generation,key,metadata_digest,container_digest),
        )
        expect_denied(lambda: connection.execute(
            "DELETE FROM profile_generation_upload_reservations "
            "WHERE tenant_id=? AND profile_id=?",
            (BASE["TENANT_A"], profile),
        ))
        begin(connection, profile, 1, 40)
        expect_denied(lambda: connection.execute(
            "INSERT INTO profile_generation_upload_reservations "
            "(tenant_id,profile_id,generation_id,object_key,metadata_digest,"
            "container_digest,source_mode,source_generation_id,"
            "expected_profile_version,issued_at_ms,expires_at_ms) "
            "VALUES (?,?,?,?,?,?,'DRAFT',NULL,1,41,360041)",
            (BASE["TENANT_A"],profile,"generation_late_draft_01",
             "profiles/v1/generation_late_draft_01.enc",
             metadata_digest,container_digest),
        ))
        assert tuple(connection.execute(
            "SELECT status,version FROM browser_profiles WHERE tenant_id=? AND profile_id=?",
            (BASE["TENANT_A"], profile),
        ).fetchone()) == ("DELETING", 2)
        assert connection.execute(
            "SELECT COUNT(*) FROM profile_delete_objects WHERE tenant_id=? AND profile_id=?",
            (BASE["TENANT_A"], profile),
        ).fetchone()[0] == 1
        assert connection.execute(
            "SELECT object_key FROM profile_delete_objects WHERE tenant_id=? AND profile_id=?",
            (BASE["TENANT_A"], profile),
        ).fetchone()[0] == key
        expect_denied(lambda: finish(connection, profile, 2, 50))
        connection.execute(
            "INSERT INTO profile_delete_object_receipts "
            "(tenant_id,profile_id,generation_id,object_key,"
            "metadata_digest,container_digest,deleted_at_ms) "
            "VALUES (?,?,?,?,?,?,360050)",
            (BASE["TENANT_A"],profile,generation,key,metadata_digest,container_digest),
        )
        finish(connection, profile, 2, 360060)
        assert connection.execute(
            "SELECT status FROM browser_profiles WHERE tenant_id=? AND profile_id=?",
            (BASE["TENANT_A"], profile),
        ).fetchone()[0] == "DELETED"
        expect_denied(lambda: connection.execute(
            "UPDATE browser_profiles SET status='READY' "
            "WHERE tenant_id=? AND profile_id=?",
            (BASE["TENANT_A"], profile),
        ))
    finally:
        connection.close()


def test_ready_profile_requires_all_exact_object_receipts() -> None:
    connection = make_database()
    try:
        tenant, owner = BASE["TENANT_A"], BASE["OWNER_A"]
        profile = "profile_delete_ready_01"
        generation = "generation_delete_ready_01"
        key = f"profiles/v1/{generation}.enc"
        metadata_digest, container_digest = "a" * 64, "b" * 64
        connection.execute(
            "INSERT INTO browser_profiles "
            "(tenant_id,profile_id,status,active_generation_id,version,"
            "created_by_actor_id,updated_by_actor_id,created_at_ms,updated_at_ms) "
            "VALUES (?,?, 'DRAFT',NULL,1,?,?,30,30)",
            (tenant, profile, owner, owner),
        )
        connection.execute(
            "INSERT INTO profile_generation_register_commands "
            "(tenant_id,command_id,command_actor_id,profile_id,generation_id,"
            "object_key,metadata_digest,container_digest,executed_at_ms) "
            "VALUES (?,?,?,?,?,?,?,?,40)",
            (tenant,"command_register_delete_01",owner,profile,generation,
             key,metadata_digest,container_digest),
        )
        connection.execute(
            "INSERT INTO profile_generation_verify_commands "
            "(tenant_id,command_id,command_actor_id,profile_id,generation_id,"
            "expected_generation_version,verification_reference,executed_at_ms) "
            "VALUES (?,?,?,?,?,1,?,45)",
            (tenant,"command_verify_delete_01",owner,profile,generation,
             "verify_exact_object_delete"),
        )
        connection.execute(
            "INSERT INTO profile_generation_activate_commands "
            "(tenant_id,command_id,command_actor_id,profile_id,generation_id,"
            "expected_profile_version,executed_at_ms) VALUES (?,?,?,?,?,1,50)",
            (tenant,"command_activate_delete_01",owner,profile,generation),
        )
        unsigned_generation = "generation_ready_uncommitted_01"
        unsigned_key = f"profiles/v1/{unsigned_generation}.enc"
        connection.execute(
            "INSERT INTO profile_generation_upload_reservations "
            "(tenant_id,profile_id,generation_id,object_key,metadata_digest,"
            "container_digest,source_mode,source_generation_id,"
            "expected_profile_version,issued_at_ms,expires_at_ms) "
            "VALUES (?,?,?,?,?,?,'READY',?,2,55,360055)",
            (tenant, profile, generation, key, metadata_digest, container_digest, generation),
        )
        connection.execute(
            "INSERT INTO profile_generation_upload_reservations "
            "(tenant_id,profile_id,generation_id,object_key,metadata_digest,"
            "container_digest,source_mode,source_generation_id,"
            "expected_profile_version,issued_at_ms,expires_at_ms) "
            "VALUES (?,?,?,?,?,?,'READY',?,2,58,360058)",
            (tenant,profile,unsigned_generation,unsigned_key,"c"*64,"d"*64,generation),
        )
        expect_denied(lambda: begin(connection, profile, 1, 60))
        begin(connection, profile, 2, 60)
        # Prove the production scheduled D1 query selects only expired,
        # already-frozen commands, not new Delete authorizations.
        d1_source = (
            ROOT / "crates/cloudflare-adapters/src/d1_profile_deletion.rs"
        ).read_text(encoding="utf-8")
        due_match = re.search(
            r'const DUE_FROZEN_DELETES: &str = r#"(.*?)"#;',
            d1_source, re.DOTALL,
        )
        assert due_match is not None
        due_sql = due_match.group(1)
        assert connection.execute(due_sql, (360059,)).fetchall() == []
        due = connection.execute(due_sql, (360060,)).fetchall()
        assert len(due) == 1
        assert tuple(due[0]) == (tenant, profile, owner, 2)
        expect_denied(lambda: connection.execute(
            "UPDATE profile_generation_upload_reservations "
            "SET issued_at_ms=70,expires_at_ms=360070 "
            "WHERE tenant_id=? AND profile_id=?",
            (tenant, profile),
        ))
        expect_denied(lambda: connection.execute(
            "INSERT INTO profile_generation_upload_reservations "
            "(tenant_id,profile_id,generation_id,object_key,metadata_digest,"
            "container_digest,source_mode,source_generation_id,"
            "expected_profile_version,issued_at_ms,expires_at_ms) "
            "VALUES (?,?,?,?,?,?,'READY',?,2,70,360070) "
            "ON CONFLICT(tenant_id,profile_id,generation_id) DO UPDATE SET "
            "issued_at_ms=excluded.issued_at_ms,expires_at_ms=excluded.expires_at_ms",
            (tenant,profile,unsigned_generation,unsigned_key,"c"*64,"d"*64,generation),
        ))
        # The frozen R2 inventory must remain stable even if a stale/offline
        # writer tries to register another generation or move CURRENT.
        expect_denied(lambda: connection.execute(
            "UPDATE browser_profiles SET active_generation_id=NULL "
            "WHERE tenant_id=? AND profile_id=?",
            (tenant, profile),
        ))
        expect_denied(lambda: connection.execute(
            "INSERT INTO profile_generation_register_commands "
            "(tenant_id,command_id,command_actor_id,profile_id,generation_id,"
            "object_key,metadata_digest,container_digest,executed_at_ms) "
            "VALUES (?,?,?,?,?,?,?,?,65)",
            (tenant, "command_register_after_delete_01", owner, profile,
             "generation_after_delete_01",
             "profiles/v1/generation_after_delete_01.enc",
             metadata_digest, container_digest),
        ))
        assert connection.execute(
            "SELECT COUNT(*) FROM profile_generations WHERE tenant_id=? AND profile_id=?",
            (tenant, profile),
        ).fetchone()[0] == 1
        row = connection.execute(
            "SELECT object_key,metadata_digest,container_digest "
            "FROM profile_delete_objects WHERE tenant_id=? AND profile_id=? AND generation_id=?",
            (tenant,profile,generation),
        ).fetchone()
        assert tuple(row) == (key,metadata_digest,container_digest)
        assert connection.execute(
            "SELECT object_key FROM profile_delete_objects "
            "WHERE tenant_id=? AND profile_id=? AND generation_id=?",
            (tenant,profile,unsigned_generation),
        ).fetchone()[0] == unsigned_key
        expect_denied(lambda: finish(connection, profile, 3, 65))
        # DELETE must not receipt a key while an old signed PUT can recreate it.
        expect_denied(lambda: connection.execute(
            "INSERT INTO profile_delete_object_receipts "
            "(tenant_id,profile_id,generation_id,object_key,"
            "metadata_digest,container_digest,deleted_at_ms) "
            "VALUES (?,?,?,?,?,?,360059)",
            (tenant,profile,generation,key,metadata_digest,container_digest),
        ))
        expect_denied(lambda: connection.execute(
            "INSERT INTO profile_delete_object_receipts "
            "(tenant_id,profile_id,generation_id,object_key,"
            "metadata_digest,container_digest,deleted_at_ms) "
            "VALUES (?,?,?,?,?,?,360070)",
            (tenant,profile,generation,key,metadata_digest,"c"*64),
        ))
        connection.execute(
            "INSERT INTO profile_delete_object_receipts "
            "(tenant_id,profile_id,generation_id,object_key,"
            "metadata_digest,container_digest,deleted_at_ms) "
            "VALUES (?,?,?,?,?,?,360070)",
            (tenant,profile,generation,key,metadata_digest,container_digest),
        )
        expect_denied(lambda: connection.execute(
            "UPDATE profile_delete_object_receipts SET deleted_at_ms=999 "
            "WHERE tenant_id=? AND profile_id=?",
            (tenant,profile),
        ))
        expect_denied(lambda: finish(connection,profile,3,360080))
        connection.execute(
            "INSERT INTO profile_delete_object_receipts "
            "(tenant_id,profile_id,generation_id,object_key,"
            "metadata_digest,container_digest,deleted_at_ms) "
            "VALUES (?,?,?,?,?,?,360080)",
            (tenant,profile,unsigned_generation,unsigned_key,"c"*64,"d"*64),
        )
        finish(connection,profile,3,360090)
        assert connection.execute(
            "SELECT status FROM browser_profiles WHERE tenant_id=? AND profile_id=?",
            (tenant,profile),
        ).fetchone()[0] == "DELETED"
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
    finally:
        connection.close()


def main() -> int:
    test_draft_direct_exact_deletion()
    test_ready_profile_requires_all_exact_object_receipts()
    print("profile DELETE D1 frozen object inventory, exact receipts, terminal status PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
