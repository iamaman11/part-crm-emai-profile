#!/usr/bin/env python3
"""K2 608 MiB D1 schema acceptance: exact successor-v2 history and populated upgrade."""

from __future__ import annotations

import importlib.util
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEGACY = ROOT / "migrations/d1"
V1 = ROOT / "migrations/d1-successor"
V2 = ROOT / "migrations/d1-successor-v2"
K2 = V2 / "0037_generation_streaming_container_limit.sql"


def apply(conn: sqlite3.Connection, path: Path) -> None:
    conn.executescript(path.read_text(encoding="utf-8"))


def assert_governed(conn: sqlite3.Connection) -> None:
    assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
    for name in (
        "profile_generation_successor_command_validate",
        "profile_generation_successor_command_apply",
        "profile_generation_successor_commands_are_append_only_update",
        "profile_generation_successor_commands_are_append_only_delete",
        "device_generation_commit_command_validate",
        "device_generation_commit_command_apply",
        "device_generation_commit_commands_are_append_only_update",
        "device_generation_commit_commands_are_append_only_delete",
        "profile_generation_insert_requires_register_command",
        "profile_generation_transition_requires_command",
        "profile_generation_activation_requires_command",
    ):
        assert conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='trigger' AND name=?", (name,)
        ).fetchone() is not None, name


def test_exact_successor_v2_lineage() -> None:
    conn = sqlite3.connect(":memory:")
    try:
        conn.execute("PRAGMA foreign_keys = ON")
        for path in sorted(LEGACY.glob("*.sql")):
            if int(path.name[:4]) <= 26:
                apply(conn, path)
        apply(conn, V1 / "0027_pas2_payload_fingerprint_expand.sql")
        for path in sorted(LEGACY.glob("*.sql")):
            if 28 <= int(path.name[:4]) <= 31:
                apply(conn, path)
        for path in sorted(V2.glob("*.sql")):
            if int(path.name[:4]) <= 36:
                apply(conn, path)
        before = conn.execute(
            "SELECT sql FROM sqlite_master WHERE name='profile_generation_successor_commands'"
        ).fetchone()[0]
        assert "83886080" in before
        apply(conn, K2)
        for table in (
            "profile_generation_successor_commands",
            "device_generation_commit_commands",
        ):
            definition = conn.execute(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?", (table,)
            ).fetchone()[0]
            assert "637534208" in definition
            assert "83886080" not in definition
        assert_governed(conn)
    finally:
        conn.close()


def test_populated_predecessor_preserves_exact_commands() -> None:
    source = ROOT / "scripts/test-device-generation-commit-d1.py"
    spec = importlib.util.spec_from_file_location("device_fixture", source)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    conn = module.seed()
    try:
        module.insert_device_commit(conn, module.command_values())
        conn.commit()
        historical = conn.execute(
            "SELECT * FROM device_generation_commit_commands WHERE tenant_id=? AND job_id=?",
            (module.TENANT, module.JOB),
        ).fetchone()
        assert historical is not None
        pointer = conn.execute(
            "SELECT active_generation_id FROM browser_profiles WHERE tenant_id=? AND profile_id=?",
            (module.TENANT, module.PROFILE),
        ).fetchone()[0]
        apply(conn, K2)
        copied = conn.execute(
            "SELECT * FROM device_generation_commit_commands WHERE tenant_id=? AND job_id=?",
            (module.TENANT, module.JOB),
        ).fetchone()
        assert copied is not None and tuple(copied) == tuple(historical)
        assert conn.execute(
            "SELECT active_generation_id FROM browser_profiles WHERE tenant_id=? AND profile_id=?",
            (module.TENANT, module.PROFILE),
        ).fetchone()[0] == pointer
        assert_governed(conn)
        for target in (
            "device_generation_commit_commands",
            "profile_generation_successor_commands",
        ):
            try:
                conn.execute("DELETE FROM " + target)
            except sqlite3.IntegrityError:
                pass
            else:
                raise AssertionError("append-only journal deletion accepted: " + target)
            conn.rollback()
    finally:
        conn.close()


if __name__ == "__main__":
    test_exact_successor_v2_lineage()
    test_populated_predecessor_preserves_exact_commands()
    print("K2 608MiB D1 successor-v2 schema and historical command preservation PASS")
