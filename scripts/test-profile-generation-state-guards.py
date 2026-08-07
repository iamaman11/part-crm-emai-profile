#!/usr/bin/env python3
"""Prove D1 rejects invalid active generation and live profile states."""

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
OBJECT_KEY = "profiles/v1/generation_quality_guard.enc"
METADATA_DIGEST = "a" * 64
CONTAINER_DIGEST = "b" * 64


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


def expect_integrity_error(
    operation: Callable[[], object], fragments: str | tuple[str, ...]
) -> None:
    expected = (fragments,) if isinstance(fragments, str) else fragments
    try:
        operation()
    except sqlite3.IntegrityError as error:
        if not any(fragment in str(error) for fragment in expected):
            raise AssertionError(
                f"expected integrity error containing one of {expected!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError("invalid profile state unexpectedly passed")


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


def registry_present(connection: sqlite3.Connection) -> bool:
    return (
        connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'profile_generations'"
        ).fetchone()
        is not None
    )


def activate_through_registry(connection: sqlite3.Connection) -> None:
    expect_integrity_error(
        lambda: connection.execute(
            """
            UPDATE browser_profiles
            SET status = 'READY', active_generation_id = ?,
                version = 2, updated_at_ms = 21
            WHERE tenant_id = ? AND profile_id = ?
            """,
            (GENERATION_ID, TENANT_ID, PROFILE_ID),
        ),
        "active_profile_generation_not_verified",
    )
    connection.rollback()
    assert profile_row(connection) == ("DRAFT", None, 1)

    with connection:
        connection.execute(
            """
            INSERT INTO profile_generation_register_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, object_key, metadata_digest, container_digest,
                executed_at_ms
            ) VALUES (?, 'command_quality_register', ?, ?, ?, ?, ?, ?, 21)
            """,
            (
                TENANT_ID,
                OWNER_ID,
                PROFILE_ID,
                GENERATION_ID,
                OBJECT_KEY,
                METADATA_DIGEST,
                CONTAINER_DIGEST,
            ),
        )
        connection.execute(
            """
            INSERT INTO profile_generation_verify_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_generation_version,
                verification_reference, executed_at_ms
            ) VALUES (?, 'command_quality_verify', ?, ?, ?, 1,
                      'review:quality_guard', 22)
            """,
            (TENANT_ID, OWNER_ID, PROFILE_ID, GENERATION_ID),
        )

    expect_integrity_error(
        lambda: connection.execute(
            """
            UPDATE browser_profiles
            SET status = 'READY', active_generation_id = ?,
                version = 2, updated_by_actor_id = ?, updated_at_ms = 23
            WHERE tenant_id = ? AND profile_id = ?
            """,
            (GENERATION_ID, OWNER_ID, TENANT_ID, PROFILE_ID),
        ),
        "profile_generation_activation_not_governed",
    )
    connection.rollback()

    with connection:
        connection.execute(
            """
            INSERT INTO profile_generation_activate_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_profile_version, executed_at_ms
            ) VALUES (?, 'command_quality_activate', ?, ?, ?, 1, 23)
            """,
            (TENANT_ID, OWNER_ID, PROFILE_ID, GENERATION_ID),
        )
    assert profile_row(connection) == ("READY", GENERATION_ID, 2)


def deactivate_through_registry(connection: sqlite3.Connection) -> None:
    with connection:
        connection.execute(
            """
            INSERT INTO profile_generation_deactivate_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_profile_version, executed_at_ms
            ) VALUES (?, 'command_quality_deactivate', ?, ?, ?, 2, 40)
            """,
            (TENANT_ID, OWNER_ID, PROFILE_ID, GENERATION_ID),
        )
    assert profile_row(connection) == ("SUSPENDED", None, 3)

    with connection:
        connection.execute(
            """
            UPDATE browser_profiles
            SET status = 'DRAFT', version = 4, updated_at_ms = 41
            WHERE tenant_id = ? AND profile_id = ?
            """,
            (TENANT_ID, PROFILE_ID),
        )
    assert profile_row(connection) == ("DRAFT", None, 4)


def main() -> None:
    connection = open_database()
    try:
        seed_profile(connection)

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE browser_profiles
                SET active_generation_id = 'bad/generation',
                    version = 2, updated_at_ms = 21
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (TENANT_ID, PROFILE_ID),
            ),
            "invalid_active_generation_id",
        )
        connection.rollback()
        assert profile_row(connection) == ("DRAFT", None, 1)

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE browser_profiles
                SET status = 'READY', version = 2, updated_at_ms = 22
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (TENANT_ID, PROFILE_ID),
            ),
            "live_profile_requires_active_generation",
        )
        connection.rollback()
        assert profile_row(connection) == ("DRAFT", None, 1)

        if registry_present(connection):
            activate_through_registry(connection)
        else:
            with connection:
                connection.execute(
                    """
                    UPDATE browser_profiles
                    SET status = 'READY', active_generation_id = ?,
                        version = 2, updated_at_ms = 30
                    WHERE tenant_id = ? AND profile_id = ?
                    """,
                    (GENERATION_ID, TENANT_ID, PROFILE_ID),
                )
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
            (
                "live_profile_requires_active_generation",
                "profile_generation_deactivation_not_governed",
            ),
        )
        connection.rollback()
        assert profile_row(connection) == ("READY", GENERATION_ID, 2)

        if registry_present(connection):
            deactivate_through_registry(connection)
        else:
            with connection:
                connection.execute(
                    """
                    UPDATE browser_profiles
                    SET status = 'DRAFT', active_generation_id = NULL,
                        version = 3, updated_at_ms = 40
                    WHERE tenant_id = ? AND profile_id = ?
                    """,
                    (TENANT_ID, PROFILE_ID),
                )
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

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO browser_profiles (
                    tenant_id, profile_id, status, active_generation_id, version,
                    created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
                ) VALUES (?, 'profile_quality_bad_id', 'READY', 'bad generation', 1, ?, ?, 50, 50)
                """,
                (TENANT_ID, OWNER_ID, OWNER_ID),
            ),
            "invalid_active_generation_id",
        )
        connection.rollback()

        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()

    print("D1 active generation and live profile state guards passed.")


if __name__ == "__main__":
    main()
