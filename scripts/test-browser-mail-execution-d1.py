#!/usr/bin/env python3
"""Prove Phase 2F BrowserFallback execution-binding D1 invariants with SQLite."""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"

TENANT = "tenant_browser_mail"
OWNER = "actor_browser_mail"
IDENTITY = "identity_browser_mail"
MEMBER = "actor_browser_mail_member"
MEMBER_IDENTITY = "identity_browser_mail_member"
PROFILE = "profile_browser_mail"
BROWSER_BINDING = "binding_browser_mail"
SECOND_BROWSER_BINDING = "binding_browser_mail_2"
IMAP_BINDING = "binding_imap_mail"

RESOLVE_ACTIVE_BINDING = """
SELECT execution.binding_id, execution.profile_id
FROM browser_mailbox_execution_bindings AS execution
JOIN mailbox_bindings AS binding
  ON binding.tenant_id = execution.tenant_id
 AND binding.binding_id = execution.binding_id
WHERE execution.tenant_id = ?
  AND execution.binding_id = ?
  AND binding.provider = 'BROWSER_FALLBACK'
  AND binding.status = 'ACTIVE'
  AND binding.execution_status = 'ACTIVE'
LIMIT 1
"""


def migration_files() -> list[Path]:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    expected = list(range(1, len(files) + 1))
    if not files or versions != expected:
        raise AssertionError(f"D1 migrations must be contiguous: {versions}; expected {expected}")
    phase2f_migration = "0021_device_generation_commit.sql"
    if phase2f_migration not in {path.name for path in files}:
        raise AssertionError(f"required Phase 2F migration is missing: {phase2f_migration}")
    return files


def database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in migration_files():
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.commit()
    return connection


def expect_integrity_error(operation: Callable[[], object]) -> None:
    try:
        operation()
    except sqlite3.IntegrityError:
        return
    raise AssertionError("operation unexpectedly bypassed a browser-mail execution invariant")


def seed(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, 'Browser Mail Tenant', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT,),
    )
    connection.execute(
        "INSERT INTO identities (identity_id, access_subject, created_at_ms) VALUES (?, ?, 10)",
        (IDENTITY, f"subject-{IDENTITY}"),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, 10, 10)
        """,
        (TENANT, OWNER, IDENTITY),
    )
    connection.execute(
        "INSERT INTO identities (identity_id, access_subject, created_at_ms) VALUES (?, ?, 11)",
        (MEMBER_IDENTITY, f"subject-{MEMBER_IDENTITY}"),
    )
    connection.execute(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, 11, 11)
        """,
        (TENANT, MEMBER, MEMBER_IDENTITY),
    )
    connection.execute(
        """
        INSERT INTO browser_profiles (
            tenant_id, profile_id, status, active_generation_id, version,
            created_by_actor_id, updated_by_actor_id, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'DRAFT', NULL, 1, ?, ?, 20, 20)
        """,
        (TENANT, PROFILE, OWNER, OWNER),
    )
    create_mailbox(connection, BROWSER_BINDING, "BROWSER_FALLBACK", 30)
    create_mailbox(connection, SECOND_BROWSER_BINDING, "BROWSER_FALLBACK", 31)
    create_mailbox(connection, IMAP_BINDING, "IMAP", 32)
    connection.commit()


def create_mailbox(
    connection: sqlite3.Connection,
    binding_id: str,
    provider: str,
    executed_at_ms: int,
) -> None:
    connection.execute(
        """
        INSERT INTO mailbox_binding_create_commands (
            tenant_id, command_id, command_actor_id, binding_id,
            provider, secret_handle, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        """,
        (
            TENANT,
            f"cmd_create_{binding_id}",
            OWNER,
            binding_id,
            provider,
            f"secret_{binding_id}",
            executed_at_ms,
        ),
    )


def bind_execution(
    connection: sqlite3.Connection,
    command_id: str,
    binding_id: str,
    profile_id: str = PROFILE,
    executed_at_ms: int = 40,
    actor_id: str = OWNER,
) -> None:
    connection.execute(
        """
        INSERT INTO browser_mailbox_execution_bind_commands (
            tenant_id, command_id, command_actor_id, binding_id, profile_id, executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?)
        """,
        (TENANT, command_id, actor_id, binding_id, profile_id, executed_at_ms),
    )


def resolved(connection: sqlite3.Connection, binding_id: str) -> tuple[str, str] | None:
    row = connection.execute(RESOLVE_ACTIVE_BINDING, (TENANT, binding_id)).fetchone()
    if row is None:
        return None
    return str(row[0]), str(row[1])


def test_governed_binding_and_immutability(connection: sqlite3.Connection) -> None:
    bind_execution(connection, "cmd_bind_browser", BROWSER_BINDING)
    connection.commit()
    assert resolved(connection, BROWSER_BINDING) == (BROWSER_BINDING, PROFILE)

    row = connection.execute(
        """
        SELECT created_by_actor_id, created_at_ms
        FROM browser_mailbox_execution_bindings
        WHERE tenant_id = ? AND binding_id = ?
        """,
        (TENANT, BROWSER_BINDING),
    ).fetchone()
    assert tuple(row) == (OWNER, 40)

    expect_integrity_error(
        lambda: connection.execute(
            """
            INSERT INTO browser_mailbox_execution_bindings (
                tenant_id, binding_id, profile_id, created_by_actor_id, created_at_ms
            ) VALUES (?, ?, ?, ?, 41)
            """,
            (TENANT, SECOND_BROWSER_BINDING, PROFILE, OWNER),
        )
    )
    connection.rollback()

    expect_integrity_error(
        lambda: connection.execute(
            """
            UPDATE browser_mailbox_execution_bindings
            SET profile_id = ?
            WHERE tenant_id = ? AND binding_id = ?
            """,
            (PROFILE, TENANT, BROWSER_BINDING),
        )
    )
    connection.rollback()

    expect_integrity_error(
        lambda: connection.execute(
            "DELETE FROM browser_mailbox_execution_bindings WHERE tenant_id = ? AND binding_id = ?",
            (TENANT, BROWSER_BINDING),
        )
    )
    connection.rollback()

    expect_integrity_error(
        lambda: connection.execute(
            "UPDATE browser_mailbox_execution_bind_commands SET executed_at_ms = 42 WHERE tenant_id = ? AND command_id = 'cmd_bind_browser'",
            (TENANT,),
        )
    )
    connection.rollback()

    expect_integrity_error(
        lambda: connection.execute(
            "DELETE FROM browser_mailbox_execution_bind_commands WHERE tenant_id = ? AND command_id = 'cmd_bind_browser'",
            (TENANT,),
        )
    )
    connection.rollback()


def test_provider_profile_owner_and_uniqueness_fail_closed(connection: sqlite3.Connection) -> None:
    expect_integrity_error(
        lambda: bind_execution(connection, "cmd_bind_imap", IMAP_BINDING, executed_at_ms=50)
    )
    connection.rollback()
    assert resolved(connection, IMAP_BINDING) is None

    expect_integrity_error(
        lambda: bind_execution(
            connection,
            "cmd_bind_missing_profile",
            SECOND_BROWSER_BINDING,
            profile_id="profile_missing_browser_mail",
            executed_at_ms=51,
        )
    )
    connection.rollback()

    expect_integrity_error(
        lambda: bind_execution(
            connection,
            "cmd_bind_duplicate",
            BROWSER_BINDING,
            executed_at_ms=52,
        )
    )
    connection.rollback()
    assert resolved(connection, BROWSER_BINDING) == (BROWSER_BINDING, PROFILE)

    expect_integrity_error(
        lambda: bind_execution(
            connection,
            "cmd_bind_member",
            SECOND_BROWSER_BINDING,
            executed_at_ms=61,
            actor_id=MEMBER,
        )
    )
    connection.rollback()
    assert resolved(connection, SECOND_BROWSER_BINDING) is None


def test_revocation_hides_historical_binding_and_index_is_used(
    connection: sqlite3.Connection,
) -> None:
    plan = connection.execute(
        """
        EXPLAIN QUERY PLAN
        SELECT binding_id
        FROM browser_mailbox_execution_bindings
        WHERE tenant_id = ? AND profile_id = ?
        ORDER BY binding_id
        """,
        (TENANT, PROFILE),
    ).fetchall()
    plan_text = "\n".join(str(row[3]) for row in plan)
    assert "browser_mailbox_execution_profile_lookup" in plan_text, plan_text

    connection.execute(
        """
        INSERT INTO mailbox_binding_revoke_commands (
            tenant_id, command_id, command_actor_id, binding_id,
            expected_binding_version, executed_at_ms
        ) VALUES (?, 'cmd_revoke_browser_mail', ?, ?, 1, 70)
        """,
        (TENANT, OWNER, BROWSER_BINDING),
    )
    connection.commit()

    assert connection.execute(
        "SELECT COUNT(*) FROM browser_mailbox_execution_bindings WHERE tenant_id = ? AND binding_id = ?",
        (TENANT, BROWSER_BINDING),
    ).fetchone()[0] == 1
    assert resolved(connection, BROWSER_BINDING) is None


def main() -> int:
    connection = database()
    try:
        seed(connection)
        test_governed_binding_and_immutability(connection)
        test_provider_profile_owner_and_uniqueness_fail_closed(connection)
        test_revocation_hides_historical_binding_and_index_is_used(connection)
    finally:
        connection.close()
    print("Phase 2F browser mailbox execution D1 invariants are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
