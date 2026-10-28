#!/usr/bin/env python3
"""#802 K2: executable guarded D1 historical WORKING proof receipt.

The caller-side real Camoufox, old-root and runtime proofs are separate.
This SQL accepts only their scoped immutable metadata and grants NO prune.
"""
from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "scripts/test-device-generation-commit-d1.py"
STREAM = ROOT / "migrations/d1-successor-v2/0037_generation_streaming_container_limit.sql"
WORKING = ROOT / "migrations/d1-successor-v2/0038_profile_generation_working_proof.sql"


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
    fx = runpy.run_path(str(FIXTURE))
    conn = fx["seed"]()
    try:
        conn.executescript(STREAM.read_text(encoding="utf-8"))
        conn.executescript(WORKING.read_text(encoding="utf-8"))
        assert not conn.execute("SELECT 1 FROM profile_generation_working_proofs").fetchone()
        rejected(conn, fx)  # no committed CURRENT N+1 yet
        fx["insert_device_commit"](conn, fx["command_values"]())
        conn.commit()
        baseline = fx["catalog_snapshot"](conn)
        rejected(conn, fx, observed_current_generation_id=fx["BASE"])
        rejected(conn, fx, generation_id=fx["CANDIDATE"])
        rejected(conn, fx, container_digest="f" * 64)
        rejected(conn, fx, metadata_digest="f" * 64)
        rejected(conn, fx, tenant_id="tenant_foreign_history")
        rejected(conn, fx, observed_by_device_id="device_foreign_history")
        rejected(conn, fx, root_key_id="unsafe/root/key")
        assert fx["catalog_snapshot"](conn) == baseline
        insert(conn, fx)
        conn.commit()
        assert conn.execute("SELECT COUNT(*) FROM profile_generation_working_proofs").fetchone()[0] == 1
        # Execute the exact Rust D1 loader SQL against the real governed
        # fixture, never a test-only restatement or an age/N-1 heuristic.
        import re
        source = (ROOT / "crates/cloudflare-adapters/src/d1_profile_generation_successor.rs").read_text(encoding="utf-8")
        match = re.search(
            r'const LOAD_CERTIFIED_WORKING_RESERVE: &str = r#"(.*?)"#;',
            source,
            re.DOTALL,
        )
        assert match is not None
        working_query = match.group(1)
        def working(current: str, pinned_runtime: str = "e" * 64):
            return conn.execute(
                working_query, (fx["TENANT"], fx["PROFILE"], current, pinned_runtime)
            ).fetchone()
        saved = working(fx["CANDIDATE"])
        assert saved is not None and saved["generation_id"] == fx["BASE"]
        assert working(fx["CANDIDATE"], "f" * 64) is None, "foreign runtime must never select WORKING"
        assert working(fx["CANDIDATE"], "E" * 64) is None, "case mismatch must not bypass runtime pin"
        assert fx["catalog_snapshot"](conn) == baseline
        try:
            conn.execute("UPDATE profile_generation_working_proofs SET proven_at_ms=999")
        except sqlite3.IntegrityError:
            conn.rollback()
        else:
            raise AssertionError("WORKING evidence modified")
        try:
            conn.execute("DELETE FROM profile_generation_working_proofs")
        except sqlite3.IntegrityError:
            conn.rollback()
        else:
            raise AssertionError("WORKING evidence deleted")
        assert conn.execute("SELECT COUNT(*) FROM profile_generation_working_proofs").fetchone()[0] == 1
        # Future activation must not rewrite immutable historical evidence.
        fx["owner_register"](conn, fx["WINNER"], 110)
        fx["owner_verify"](conn, fx["WINNER"], 115)
        fx["owner_activate"](conn, fx["WINNER"], 3, 120)
        conn.commit()
        assert conn.execute("SELECT generation_id FROM profile_generation_working_proofs").fetchone()[0] == fx["BASE"]
        # The CURRENT pointer now names an independently governed generation,
        # yet the last browser-proven reserve remains N-k without inventing
        # another cold-browser proof or changing existing catalog bytes.
        selected = working(fx["WINNER"])
        assert selected is not None and selected["generation_id"] == fx["BASE"]
        assert working(fx["WINNER"], "f" * 64) is None, "N-k still needs the exact retained runtime"

        # Prune review is deliberately read-only. It refuses any candidate
        # before a WORKING reserve exists, protects both CURRENT and N-k, and
        # never selects any generation with a previous browser WORKING proof.
        review_source = re.search(
            r'const LOAD_UNPROVEN_HISTORICAL_PRUNE_REVIEW: &str = r#"(.*?)"#;',
            source,
            re.DOTALL,
        )
        assert review_source is not None
        review_query = review_source.group(1)
        def review(current: str, approved_runtime: str = "e" * 64):
            return conn.execute(
                review_query, (fx["TENANT"], fx["PROFILE"], current, approved_runtime)
            ).fetchone()
        candidate = review(fx["WINNER"])
        assert candidate is not None and candidate["generation_id"] == fx["CANDIDATE"]
        assert candidate["generation_id"] not in (fx["WINNER"], fx["BASE"])
        assert review(fx["CANDIDATE"]) is None, "stale CURRENT cannot authorize review"
        assert review(fx["WINNER"], "f" * 64) is None, "unapproved runtime blocks review"
        assert review(fx["WINNER"], "E" * 64) is None
        assert fx["catalog_snapshot"](conn) == baseline or conn.execute(
            "SELECT COUNT(*) FROM profile_generation_working_proofs"
        ).fetchone()[0] == 1
        assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
        print("K2 governed, append-only historical proof persists across CURRENT changes; no prune PASS")
    finally:
        conn.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
