#!/usr/bin/env python3
"""Prove D1 rejects live profile rows without an active generation."""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"

TENANT_ID = "tenant_quality_guard"
OWNER_ID = "actor_quality_guard"
IDENTITY_ID = "identity_quality_guard"
PROFILE_ID = "profile_quality_guard"
GENERATION_ID = "generation_quality_guard"


def migration_files() -> list[Path]:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    if not files or versions != list(range(1, len(files) + 1)):
        raise AssertionError(f"D1 migrations must be contiguous from 0001: {versions}")
    return files


def open_database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.row_factory = sqlite3.Row
    for migration in migration_files():
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.commit()
    return connection


def seed_profile(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Quality Guard Tenant', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT_ID,),
    )
    connection.execute(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, 'quality-guard-subject', 10)
        """,
        (IDENTITY_ID,),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT_ID, OWNER_ID, IDENTITY_ID),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, active_generation_id, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', NULL, 1, ?, ?, 20, 20)
        """,
        (TENANT_ID, PROFILE_ID, OWNER_ID, OWNER_ID),
    )
    connection.commit()


def expect_integrity_error(operation: Callable[[], object], fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        if fragment not in str(error):
            raise AssertionError(
                f"expected integrity error containing {fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError("invalid live profile state unexpectedly passed")


def profile_row(connection: sqlite3.Connection) -> tuple[str, str | None, int]:
    row = connection.execute(
        """
        SELECT status, active_generation_id, version
        FROM browser_profiles
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT_ID, PROFILE_ID),
    ).fetchone()
    if row is None:
        raise AssertionError("seed profile disappeared")
    return row["status"], row["active_generation_id"], row["version"]


def main() -> None:
    connection = open_database()
    try:
        seed_profile(connection)

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE browser_profiles
                SET status = 'READY', version = 2, updated_at_ms = 30
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (TENANT_ID, PROFILE_ID),
            ),
            "live_profile_requires_active_generation",
        )
        connection.rollback()
        assert profile_row(connection) == ("DRAFT", None, 1)

        connection.execute(
            """
            UPDATE browser_profiles
            SET status = 'READY', active_generation_id = ?,
                version = 2, updated_at_ms = 30
            WHERE tenant_id = ? AND profile_id = ?
            """,
            (GENERATION_ID, TENANT_ID, PROFILE_ID),
        )
        connection.commit()
        assert profile_row(connection) == ("READY", GENERATION_ID, 2)

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE browser_profiles
                SET active_generation_id = NULL, version = 3, updated_at_ms = 40
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (TENANT_ID, PROFILE_ID),
            ),
            "live_profile_requires_active_generation",
        )
        connection.rollback()
        assert profile_row(connection) == ("READY", GENERATION_ID, 2)

        connection.execute(
            """
            UPDATE browser_profiles
            SET status = 'DRAFT', active_generation_id = NULL,
                version = 3, updated_at_ms = 40
            WHERE tenant_id = ? AND profile_id = ?
            """,
            (TENANT_ID, PROFILE_ID),
        )
        connection.commit()
        assert profile_row(connection) == ("DRAFT", None, 3)

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO browser_profiles (
                    tenant_id, profile_id, status, active_generation_id, version,
                    created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
                ) VALUES (?, 'profile_quality_insert', 'IN_USE', NULL, 1, ?, ?, 50, 50)
                """,
                (TENANT_ID, OWNER_ID, OWNER_ID),
            ),
            "live_profile_requires_active_generation",
        )
        connection.rollback()

        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()

    print("D1 live profile generation state guards passed.")


if __name__ == "__main__":
    main()
