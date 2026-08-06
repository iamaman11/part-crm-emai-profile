#!/usr/bin/env python3
"""Prove generation rows and active pointers cannot bypass command journals."""

from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
BASE = runpy.run_path(str(ROOT / "scripts" / "test-profile-generation-registry.py"))


def expect_integrity_error(operation: Callable[[], object], fragment: str) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        if fragment not in str(error):
            raise AssertionError(
                f"expected integrity error containing {fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError(f"operation unexpectedly passed; expected {fragment!r}")


def generation_snapshot(connection: sqlite3.Connection) -> tuple[object, ...] | None:
    row = BASE["generation_row"](connection)
    return None if row is None else tuple(row)


def profile_snapshot(connection: sqlite3.Connection) -> tuple[object, ...]:
    return tuple(BASE["profile_row"](connection))


def main() -> int:
    tenant = BASE["TENANT"]
    owner = BASE["OWNER"]
    profile = BASE["PROFILE"]
    generation = BASE["GENERATION"]
    object_key = BASE["OBJECT_KEY"]
    metadata_digest = BASE["METADATA_DIGEST"]
    container_digest = BASE["CONTAINER_DIGEST"]

    connection = BASE["database"]()
    try:
        BASE["seed"](connection)

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_generations (
                    tenant_id, profile_id, generation_id, object_key,
                    metadata_digest, container_digest, status, version,
                    registered_by_actor_id, created_at_ms, updated_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, 'REGISTERED', 1, ?, 100, 100)
                """,
                (
                    tenant,
                    profile,
                    generation,
                    object_key,
                    metadata_digest,
                    container_digest,
                    owner,
                ),
            ),
            "profile_generation_insert_not_governed",
        )
        connection.rollback()
        assert generation_snapshot(connection) is None

        with connection:
            BASE["register"](
                connection,
                command_id="command_register_integrity",
                actor_id=owner,
                generation_id=generation,
                object_key=object_key,
                now=100,
            )
        registered = generation_snapshot(connection)
        assert registered is not None

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE profile_generations
                SET object_key = 'profiles/v1/rewritten_generation.enc'
                WHERE tenant_id = ? AND profile_id = ? AND generation_id = ?
                """,
                (tenant, profile, generation),
            ),
            "profile_generation_identity_immutable",
        )
        connection.rollback()
        assert generation_snapshot(connection) == registered

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE profile_generations
                SET status = 'VERIFIED', version = 2,
                    verification_reference = 'review:direct_bypass',
                    verified_by_actor_id = ?, verified_at_ms = 200,
                    updated_at_ms = 200
                WHERE tenant_id = ? AND profile_id = ? AND generation_id = ?
                """,
                (owner, tenant, profile, generation),
            ),
            "profile_generation_transition_not_governed",
        )
        connection.rollback()
        assert generation_snapshot(connection) == registered

        with connection:
            connection.execute(
                """
                INSERT INTO profile_generation_verify_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_generation_version,
                    verification_reference, executed_at_ms
                ) VALUES (?, 'command_verify_integrity', ?, ?, ?, 1,
                          'review:generation_integrity', 200)
                """,
                (tenant, owner, profile, generation),
            )
        verified = generation_snapshot(connection)
        assert verified is not None and verified[0] == "VERIFIED"

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE browser_profiles
                SET active_generation_id = 'generation_missing_integrity',
                    status = 'READY', version = 2,
                    updated_by_actor_id = ?, updated_at_ms = 210
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (owner, tenant, profile),
            ),
            "active_profile_generation_not_verified",
        )
        connection.rollback()
        assert profile_snapshot(connection) == ("DRAFT", None, 1)

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE browser_profiles
                SET active_generation_id = ?, status = 'READY', version = 2,
                    updated_by_actor_id = ?, updated_at_ms = 210
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (generation, owner, tenant, profile),
            ),
            "profile_generation_activation_not_governed",
        )
        connection.rollback()
        assert profile_snapshot(connection) == ("DRAFT", None, 1)

        with connection:
            connection.execute(
                """
                INSERT INTO profile_generation_activate_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_profile_version, executed_at_ms
                ) VALUES (?, 'command_activate_integrity', ?, ?, ?, 1, 210)
                """,
                (tenant, owner, profile, generation),
            )
        assert profile_snapshot(connection) == ("READY", generation, 2)

        expect_integrity_error(
            lambda: connection.execute(
                """
                UPDATE profile_generations
                SET status = 'QUARANTINED', version = 3,
                    quarantined_by_actor_id = ?, quarantined_at_ms = 220,
                    updated_at_ms = 220
                WHERE tenant_id = ? AND profile_id = ? AND generation_id = ?
                """,
                (owner, tenant, profile, generation),
            ),
            "profile_generation_transition_not_governed",
        )
        connection.rollback()
        assert generation_snapshot(connection) == verified
        assert profile_snapshot(connection) == ("READY", generation, 2)

        with connection:
            connection.execute(
                """
                UPDATE browser_profiles
                SET status = 'SUSPENDED', active_generation_id = NULL,
                    version = 3, updated_by_actor_id = ?, updated_at_ms = 220
                WHERE tenant_id = ? AND profile_id = ?
                """,
                (owner, tenant, profile),
            )
            connection.execute(
                """
                INSERT INTO profile_generation_quarantine_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_generation_version, executed_at_ms
                ) VALUES (?, 'command_quarantine_integrity', ?, ?, ?, 2, 230)
                """,
                (tenant, owner, profile, generation),
            )
        quarantined = generation_snapshot(connection)
        assert quarantined is not None and quarantined[0] == "QUARANTINED"
        assert profile_snapshot(connection) == ("SUSPENDED", None, 3)

        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()

    print("Profile generation governed integrity guards passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
