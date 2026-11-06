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
    # Existing identity-only probe must never certify. Only the explicitly
    # selected staging cold-probe path may present an original encrypted
    # native snapshot SHA plus the validated real-browser witness.
    operator = (ROOT / "apps/profile-bridge/src/operator_flow.rs").read_text(
        encoding="utf-8"
    )
    assert "pub fn probe_and_certify_verified_predecessor_once" in operator
    assert "certify_verified_predecessor_working(" in operator
    assert "native_witness.is_none()" in operator
    shipping = (ROOT / "apps/profile-bridge/src/shipping_composition.rs").read_text(
        encoding="utf-8"
    )
    assert "R2_PRUNE_AUTHORITY=STAGING_CANARY_ONLY" in shipping
    worker = (
        ROOT / "apps/control-plane-worker/src/profile_generation_successor_ingress.rs"
    ).read_text(encoding="utf-8")
    assert "staging_r2_prune_canary_scope(" in worker
    assert "body.native_snapshot_sha256() != original_snapshot" in worker
    assert "lower_sha256(body.native_browser_witness_sha256())" in worker
    route_start = worker.index("async fn dispatch_working_proof(")
    route_end = worker.index("async fn retire_one_old_generation(", route_start)
    handler = worker[route_start:route_end]
    assert handler.index("historical_browser_content_certification_implemented") < handler.index(
        "request.json::<BridgeGenerationWorkingProofRequest>()"
    )
    assert handler.index("body.native_snapshot_sha256() != original_snapshot") < handler.index(
        ".record_historical_working_proof("
    )
    d1 = (
        ROOT / "crates/cloudflare-adapters/src/d1_profile_generation_successor.rs"
    ).read_text(encoding="utf-8")
    assert "native_snapshot_sha256 IS NOT NULL" in d1
    assert "native_browser_witness_sha256 IS NOT NULL" in d1
    assert "valid_native_proof_digest(native_snapshot_sha256)" in d1
    assert "valid_native_proof_digest(native_browser_witness_sha256)" in d1
    assert "JOIN profile_generation_working_proofs AS proof" in d1
    # Schema0040 old rows must be NULL, always excluded from reserve/prune.
    migration = NATIVE.read_text(encoding="utf-8")
    assert "retirement_requires_native_cold_recovery_proof" in migration
    assert "native_snapshot_sha256 IS NULL" in migration

    # The SQL fixture below tests historical schema integrity only. A direct
    # synthetic insert is NOT a working-browser certificate or pruning permit.
    fx = runpy.run_path(str(FIXTURE))
    conn = fx["seed"]()
    try:
        conn.executescript(STREAM.read_text(encoding="utf-8"))
        conn.executescript(WORKING.read_text(encoding="utf-8"))
        conn.executescript(RETIREMENT.read_text(encoding="utf-8"))
        conn.executescript(NATIVE.read_text(encoding="utf-8"))
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
        rejected(conn, fx, native_snapshot_sha256="BAD", native_browser_witness_sha256="b" * 64)
        rejected(conn, fx, native_snapshot_sha256="a" * 64, native_browser_witness_sha256="UPPER" * 13)
        insert(conn, fx, native_snapshot_sha256="a" * 64, native_browser_witness_sha256="b" * 64)
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
        before_review = (
            conn.execute("SELECT COUNT(*) FROM profile_generations").fetchone()[0],
            conn.execute("SELECT COUNT(*) FROM profile_generation_working_proofs").fetchone()[0],
            conn.execute("SELECT COUNT(*) FROM profile_generation_successor_commands").fetchone()[0],
        )
        candidate = review(fx["WINNER"])
        assert candidate is not None and candidate["generation_id"] == fx["CANDIDATE"]
        assert candidate["generation_id"] not in (fx["WINNER"], fx["BASE"])
        assert review(fx["CANDIDATE"]) is None, "stale CURRENT cannot authorize review"
        assert review(fx["WINNER"], "f" * 64) is None, "unapproved runtime blocks review"
        assert review(fx["WINNER"], "E" * 64) is None
        assert (
            conn.execute("SELECT COUNT(*) FROM profile_generations").fetchone()[0],
            conn.execute("SELECT COUNT(*) FROM profile_generation_working_proofs").fetchone()[0],
            conn.execute("SELECT COUNT(*) FROM profile_generation_successor_commands").fetchone()[0],
        ) == before_review, "prune review must never mutate D1"
        assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
        # A metadata-only schema0038 receipt in a fresh fixture must remain
        # permanently ineligible even when CURRENT, object digests and ACL pass.
        legacy = fx["seed"]()
        try:
            for migration_file in (STREAM, WORKING, RETIREMENT, NATIVE):
                legacy.executescript(migration_file.read_text(encoding="utf-8"))
            fx["insert_device_commit"](legacy, fx["command_values"]())
            legacy.commit()
            insert(legacy, fx)  # Original schema0038 shaped metadata-only row
            legacy.commit()
            assert legacy.execute(
                working_query,
                (fx["TENANT"], fx["PROFILE"], fx["CANDIDATE"], "e" * 64),
            ).fetchone() is None
        finally:
            legacy.close()
        print("K2 native C09 proof excludes legacy metadata-only receipts; no unsafe prune PASS")
    finally:
        conn.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
