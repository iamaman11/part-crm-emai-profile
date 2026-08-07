#!/usr/bin/env python3
"""Exercise generation registry rollback and monotonic-time edge cases."""

from __future__ import annotations

import runpy
import sqlite3
from pathlib import Path
from typing import Any, Callable

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


def command_count(connection: sqlite3.Connection, table: str, command_id: str) -> int:
    allowed = {
        "profile_generation_register_commands",
        "profile_generation_verify_commands",
        "profile_generation_activate_commands",
        "profile_generation_deactivate_commands",
        "profile_generation_quarantine_commands",
    }
    if table not in allowed:
        raise AssertionError(f"unexpected command table: {table}")
    return connection.execute(
        f"SELECT COUNT(*) FROM {table} WHERE command_id = ?",  # noqa: S608
        (command_id,),
    ).fetchone()[0]


def values() -> dict[str, Any]:
    keys = (
        "TENANT",
        "OWNER",
        "PROFILE",
        "GENERATION",
        "OBJECT_KEY",
        "METADATA_DIGEST",
        "CONTAINER_DIGEST",
    )
    return {key: BASE[key] for key in keys}


def main() -> int:
    value = values()
    connection = BASE["database"]()
    try:
        BASE["seed"](connection)

        expect_integrity_error(
            lambda: BASE["register"](
                connection,
                command_id="command_register_time_regression",
                actor_id=value["OWNER"],
                generation_id=value["GENERATION"],
                object_key=value["OBJECT_KEY"],
                now=19,
            ),
            "profile_generation_time_regression",
        )
        connection.rollback()
        assert command_count(
            connection,
            "profile_generation_register_commands",
            "command_register_time_regression",
        ) == 0

        with connection:
            BASE["register"](
                connection,
                command_id="command_register_edge",
                actor_id=value["OWNER"],
                generation_id=value["GENERATION"],
                object_key=value["OBJECT_KEY"],
                now=100,
            )

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_generation_verify_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_generation_version,
                    verification_reference, executed_at_ms
                ) VALUES (?, 'command_verify_time_regression', ?, ?, ?, 1,
                          'review:generation_edge', 99)
                """,
                (
                    value["TENANT"],
                    value["OWNER"],
                    value["PROFILE"],
                    value["GENERATION"],
                ),
            ),
            "profile_generation_time_regression",
        )
        connection.rollback()
        assert command_count(
            connection,
            "profile_generation_verify_commands",
            "command_verify_time_regression",
        ) == 0

        with connection:
            connection.execute(
                """
                INSERT INTO profile_generation_verify_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_generation_version,
                    verification_reference, executed_at_ms
                ) VALUES (?, 'command_verify_edge', ?, ?, ?, 1,
                          'review:generation_edge', 200)
                """,
                (
                    value["TENANT"],
                    value["OWNER"],
                    value["PROFILE"],
                    value["GENERATION"],
                ),
            )

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_generation_activate_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_profile_version, executed_at_ms
                ) VALUES (?, 'command_activate_time_regression', ?, ?, ?, 1, 199)
                """,
                (
                    value["TENANT"],
                    value["OWNER"],
                    value["PROFILE"],
                    value["GENERATION"],
                ),
            ),
            "profile_generation_time_regression",
        )
        connection.rollback()
        assert command_count(
            connection,
            "profile_generation_activate_commands",
            "command_activate_time_regression",
        ) == 0

        with connection:
            connection.execute(
                """
                INSERT INTO profile_generation_activate_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_profile_version, executed_at_ms
                ) VALUES (?, 'command_activate_edge', ?, ?, ?, 1, 210)
                """,
                (
                    value["TENANT"],
                    value["OWNER"],
                    value["PROFILE"],
                    value["GENERATION"],
                ),
            )

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_generation_deactivate_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_profile_version, executed_at_ms
                ) VALUES (?, 'command_deactivate_time_regression', ?, ?, ?, 2, 209)
                """,
                (
                    value["TENANT"],
                    value["OWNER"],
                    value["PROFILE"],
                    value["GENERATION"],
                ),
            ),
            "profile_generation_time_regression",
        )
        connection.rollback()
        assert command_count(
            connection,
            "profile_generation_deactivate_commands",
            "command_deactivate_time_regression",
        ) == 0
        assert tuple(BASE["profile_row"](connection)) == (
            "READY",
            value["GENERATION"],
            2,
        )

        with connection:
            connection.execute(
                """
                INSERT INTO profile_generation_deactivate_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_profile_version, executed_at_ms
                ) VALUES (?, 'command_deactivate_edge', ?, ?, ?, 2, 220)
                """,
                (
                    value["TENANT"],
                    value["OWNER"],
                    value["PROFILE"],
                    value["GENERATION"],
                ),
            )
        assert tuple(BASE["profile_row"](connection)) == ("SUSPENDED", None, 3)

        expect_integrity_error(
            lambda: connection.execute(
                """
                INSERT INTO profile_generation_quarantine_commands (
                    tenant_id, command_id, command_actor_id, profile_id,
                    generation_id, expected_generation_version, executed_at_ms
                ) VALUES (?, 'command_quarantine_time_regression', ?, ?, ?, 2, 199)
                """,
                (
                    value["TENANT"],
                    value["OWNER"],
                    value["PROFILE"],
                    value["GENERATION"],
                ),
            ),
            "profile_generation_time_regression",
        )
        connection.rollback()
        assert command_count(
            connection,
            "profile_generation_quarantine_commands",
            "command_quarantine_time_regression",
        ) == 0
        assert tuple(BASE["profile_row"](connection)) == ("SUSPENDED", None, 3)

        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()

    print("Profile generation monotonic-time and rollback edge cases passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
