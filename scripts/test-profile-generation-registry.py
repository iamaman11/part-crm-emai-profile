#!/usr/bin/env python3
"""Prove profile generation registry and activation invariants in SQLite."""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"

TENANT = "tenant_generation_registry"
FOREIGN_TENANT = "tenant_generation_foreign"
OWNER = "actor_generation_owner"
MEMBER = "actor_generation_member"
FOREIGN_OWNER = "actor_generation_foreign"
OWNER_IDENTITY = "identity_generation_owner"
MEMBER_IDENTITY = "identity_generation_member"
FOREIGN_IDENTITY = "identity_generation_foreign"
PROFILE = "profile_generation_registry"
GENERATION = "generation_registry_01"
SECOND_GENERATION = "generation_registry_02"
OBJECT_KEY = "profiles/v1/generation_registry_01.enc"
METADATA_DIGEST = "a" * 64
CONTAINER_DIGEST = "b" * 64


def migration_files() -> list[Path]:
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    if not files or versions != list(range(1, len(files) + 1)):
        raise AssertionError(f"D1 migrations must be contiguous from 0001: {versions}")
    return files


def database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in migration_files() + sorted((ROOT / "migrations" / "d1-successor-v2").glob("*.sql")):
        source = ROOT / "migrations/d1-successor/0027_pas2_payload_fingerprint_expand.sql" if migration.name.startswith("0027_") else migration
        connection.executescript(source.read_text(encoding="utf-8"))
    connection.commit()
    return connection


def seed(connection: sqlite3.Connection) -> None:
    connection.executemany(
        """
        INSERT INTO tenants (
            tenant_id, display_name, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, ?, 'ACTIVE', 1, 10, 10)
        """,
        (
            (TENANT, "Generation Registry Tenant"),
            (FOREIGN_TENANT, "Foreign Generation Tenant"),
        ),
    )
    connection.executemany(
        """
        INSERT INTO identities (identity_id, access_subject, created_at_ms)
        VALUES (?, ?, 10)
        """,
        (
            (OWNER_IDENTITY, "generation-owner-subject"),
            (MEMBER_IDENTITY, "generation-member-subject"),
            (FOREIGN_IDENTITY, "generation-foreign-subject"),
        ),
    )
    connection.executemany(
        """
        INSERT INTO memberships (
            tenant_id, actor_id, identity_id, role, status, version,
            created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, 'ACTIVE', 1, 10, 10)
        """,
        (
            (TENANT, OWNER, OWNER_IDENTITY, "TENANT_OWNER"),
            (TENANT, MEMBER, MEMBER_IDENTITY, "MEMBER"),
            (FOREIGN_TENANT, FOREIGN_OWNER, FOREIGN_IDENTITY, "TENANT_OWNER"),
        ),
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
        raise AssertionError(f"operation unexpectedly passed; expected {fragment!r}")


def command_count(connection: sqlite3.Connection, table: str, command_id: str) -> int:
    if table not in {
        "profile_generation_register_commands",
        "profile_generation_verify_commands",
        "profile_generation_activate_commands",
        "profile_generation_deactivate_commands",
        "profile_generation_quarantine_commands",
    }:
        raise AssertionError(f"unexpected command table: {table}")
    return connection.execute(
        f"SELECT COUNT(*) FROM {table} WHERE command_id = ?",  # noqa: S608
        (command_id,),
    ).fetchone()[0]


def register(
    connection: sqlite3.Connection,
    *,
    command_id: str,
    actor_id: str,
    generation_id: str,
    object_key: str,
    metadata_digest: str = METADATA_DIGEST,
    container_digest: str = CONTAINER_DIGEST,
    tenant_id: str = TENANT,
    profile_id: str = PROFILE,
    now: int = 100,
) -> None:
    connection.execute(
        """
        INSERT INTO profile_generation_register_commands (
            tenant_id, command_id, command_actor_id, profile_id,
            generation_id, object_key, metadata_digest, container_digest,
            executed_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            tenant_id,
            command_id,
            actor_id,
            profile_id,
            generation_id,
            object_key,
            metadata_digest,
            container_digest,
            now,
        ),
    )


def generation_row(
    connection: sqlite3.Connection, generation_id: str = GENERATION
) -> sqlite3.Row | None:
    return connection.execute(
        """
        SELECT status, version, object_key, metadata_digest, container_digest,
               verification_reference
        FROM profile_generations
        WHERE tenant_id = ? AND profile_id = ? AND generation_id = ?
        """,
        (TENANT, PROFILE, generation_id),
    ).fetchone()


def profile_row(connection: sqlite3.Connection) -> sqlite3.Row:
    row = connection.execute(
        """
        SELECT status, active_generation_id, version
        FROM browser_profiles
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT, PROFILE),
    ).fetchone()
    if row is None:
        raise AssertionError("profile disappeared")
    return row


def test_registration_guards(connection: sqlite3.Connection) -> None:
    expect_integrity_error(
        lambda: register(
            connection,
            command_id="command_register_member",
            actor_id=MEMBER,
            generation_id=GENERATION,
            object_key=OBJECT_KEY,
        ),
        "profile_generation_register_owner_required",
    )
    connection.rollback()
    assert command_count(
        connection, "profile_generation_register_commands", "command_register_member"
    ) == 0
    assert generation_row(connection) is None

    expect_integrity_error(
        lambda: register(
            connection,
            command_id="command_register_bad_key",
            actor_id=OWNER,
            generation_id=GENERATION,
            object_key="../unsafe/profile",
        ),
        "CHECK constraint failed",
    )
    connection.rollback()
    assert command_count(
        connection, "profile_generation_register_commands", "command_register_bad_key"
    ) == 0

    expect_integrity_error(
        lambda: register(
            connection,
            command_id="command_register_bad_digest",
            actor_id=OWNER,
            generation_id=GENERATION,
            object_key=OBJECT_KEY,
            metadata_digest="A" * 64,
        ),
        "CHECK constraint failed",
    )
    connection.rollback()
    assert command_count(
        connection, "profile_generation_register_commands", "command_register_bad_digest"
    ) == 0

    with connection:
        register(
            connection,
            command_id="command_register_generation",
            actor_id=OWNER,
            generation_id=GENERATION,
            object_key=OBJECT_KEY,
        )
    row = generation_row(connection)
    assert row is not None
    assert (row["status"], row["version"]) == ("REGISTERED", 1)
    assert row["object_key"] == OBJECT_KEY
    assert row["metadata_digest"] == METADATA_DIGEST
    assert row["container_digest"] == CONTAINER_DIGEST

    expect_integrity_error(
        lambda: register(
            connection,
            command_id="command_duplicate_object",
            actor_id=OWNER,
            generation_id=SECOND_GENERATION,
            object_key=OBJECT_KEY,
        ),
        "UNIQUE constraint failed",
    )
    connection.rollback()
    assert command_count(
        connection, "profile_generation_register_commands", "command_duplicate_object"
    ) == 0
    assert generation_row(connection, SECOND_GENERATION) is None

    expect_integrity_error(
        lambda: register(
            connection,
            command_id="command_foreign_registration",
            actor_id=FOREIGN_OWNER,
            generation_id=SECOND_GENERATION,
            object_key="profiles/v1/foreign_generation_02.enc",
            tenant_id=FOREIGN_TENANT,
        ),
        "profile_generation_register_profile_missing",
    )
    connection.rollback()
    assert generation_row(connection, SECOND_GENERATION) is None


def test_verification_and_activation(connection: sqlite3.Connection) -> None:
    expect_integrity_error(
        lambda: connection.execute(
            """
            INSERT INTO profile_generation_verify_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_generation_version,
                verification_reference, executed_at_ms
            ) VALUES (?, 'command_verify_stale', ?, ?, ?, 9, 'review:stale_01', 200)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        ),
        "profile_generation_verify_state_mismatch",
    )
    connection.rollback()
    assert generation_row(connection)["status"] == "REGISTERED"

    with connection:
        connection.execute(
            """
            INSERT INTO profile_generation_verify_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_generation_version,
                verification_reference, executed_at_ms
            ) VALUES (?, 'command_verify_generation', ?, ?, ?, 1,
                      'review:generation_01', 210)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        )
    row = generation_row(connection)
    assert (row["status"], row["version"], row["verification_reference"]) == (
        "VERIFIED",
        2,
        "review:generation_01",
    )

    with connection:
        register(
            connection,
            command_id="command_register_second",
            actor_id=OWNER,
            generation_id=SECOND_GENERATION,
            object_key="profiles/v1/generation_registry_02.enc",
            metadata_digest="c" * 64,
            container_digest="d" * 64,
            now=220,
        )

    expect_integrity_error(
        lambda: connection.execute(
            """
            INSERT INTO profile_generation_activate_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_profile_version, executed_at_ms
            ) VALUES (?, 'command_activate_unverified', ?, ?, ?, 1, 230)
            """,
            (TENANT, OWNER, PROFILE, SECOND_GENERATION),
        ),
        "profile_generation_not_verified",
    )
    connection.rollback()
    assert tuple(profile_row(connection)) == ("DRAFT", None, 1)

    with connection:
        connection.execute(
            """
            INSERT INTO profile_generation_activate_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_profile_version, executed_at_ms
            ) VALUES (?, 'command_activate_generation', ?, ?, ?, 1, 240)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        )
    assert tuple(profile_row(connection)) == ("READY", GENERATION, 2)

    expect_integrity_error(
        lambda: connection.execute(
            """
            INSERT INTO profile_generation_activate_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_profile_version, executed_at_ms
            ) VALUES (?, 'command_activate_stale', ?, ?, ?, 1, 250)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        ),
        "profile_generation_activate_profile_state_mismatch",
    )
    connection.rollback()
    assert tuple(profile_row(connection)) == ("READY", GENERATION, 2)


def test_quarantine(connection: sqlite3.Connection) -> None:
    expect_integrity_error(
        lambda: connection.execute(
            """
            INSERT INTO profile_generation_quarantine_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_generation_version, executed_at_ms
            ) VALUES (?, 'command_quarantine_active', ?, ?, ?, 2, 300)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        ),
        "active_profile_generation_cannot_be_quarantined",
    )
    connection.rollback()
    assert generation_row(connection)["status"] == "VERIFIED"
    assert tuple(profile_row(connection)) == ("READY", GENERATION, 2)

    expect_integrity_error(
        lambda: connection.execute(
            """
            UPDATE browser_profiles
            SET status = 'SUSPENDED', active_generation_id = NULL,
                version = 3, updated_by_actor_id = ?, updated_at_ms = 310
            WHERE tenant_id = ? AND profile_id = ?
            """,
            (OWNER, TENANT, PROFILE),
        ),
        "profile_generation_deactivation_not_governed",
    )
    connection.rollback()
    assert tuple(profile_row(connection)) == ("READY", GENERATION, 2)

    with connection:
        connection.execute(
            """
            INSERT INTO profile_generation_deactivate_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_profile_version, executed_at_ms
            ) VALUES (?, 'command_deactivate_generation', ?, ?, ?, 2, 310)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        )
        connection.execute(
            """
            INSERT INTO profile_generation_quarantine_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_generation_version, executed_at_ms
            ) VALUES (?, 'command_quarantine_generation', ?, ?, ?, 2, 320)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        )
    assert command_count(
        connection,
        "profile_generation_deactivate_commands",
        "command_deactivate_generation",
    ) == 1
    row = generation_row(connection)
    assert (row["status"], row["version"]) == ("QUARANTINED", 3)
    assert tuple(profile_row(connection)) == ("SUSPENDED", None, 3)

    expect_integrity_error(
        lambda: connection.execute(
            """
            INSERT INTO profile_generation_quarantine_commands (
                tenant_id, command_id, command_actor_id, profile_id,
                generation_id, expected_generation_version, executed_at_ms
            ) VALUES (?, 'command_quarantine_again', ?, ?, ?, 3, 330)
            """,
            (TENANT, OWNER, PROFILE, GENERATION),
        ),
        "profile_generation_quarantine_state_mismatch",
    )
    connection.rollback()
    assert generation_row(connection)["status"] == "QUARANTINED"



def initial_database(role: str = "PROFILE_OPERATOR") -> sqlite3.Connection:
    connection = database()
    seed(connection)
    connection.execute("INSERT INTO profile_grants (tenant_id, profile_id, actor_id, role, granted_by_actor_id, reason, created_at_ms) VALUES (?, ?, ?, ?, ?, 'self service', 20)", (TENANT, PROFILE, MEMBER, role, OWNER))
    connection.execute("INSERT INTO device_public_key_binding_commands (tenant_id, pairing_digest, actor_id, device_id, public_key_spki_der_hex, expected_previous_version, next_version, executed_at_ms) VALUES (?, ?, ?, 'device_initial_member', ?, NULL, 1, 30)", (TENANT, "c" * 64, MEMBER, "a" * 182))
    connection.commit()
    return connection


def initialize(connection: sqlite3.Connection, device: str = "device_initial_member", version: int = 1, fail_late: bool = False) -> None:
    # Production SQL comes from the existing D1 repository, not a second initializer.
    import re
    source = (ROOT / "crates/cloudflare-adapters/src/d1_profile_generations/initialization.rs").read_text()
    repository = (ROOT / "crates/cloudflare-adapters/src/d1_profile_generations.rs").read_text()
    def sql(name: str, text: str = source) -> str:
        match = re.search(r'const ' + name + r': &str = r#"(.*?)"#;', text, re.S)
        assert match, name
        return match.group(1)
    with connection:
        connection.execute(sql("INITIAL_REGISTER_COMMAND"), (TENANT, "initial_test", MEMBER, PROFILE, GENERATION, OBJECT_KEY, METADATA_DIGEST, CONTAINER_DIGEST, 40, device, version))
        connection.execute(sql("VERIFY_COMMAND", repository), (TENANT, "initial_test", MEMBER, PROFILE, GENERATION, 1, "r2sha256:" + CONTAINER_DIGEST, 40))
        connection.execute(sql("INITIAL_DEVICE_ADMISSION"), (TENANT, MEMBER, device, PROFILE, GENERATION, "r2sha256:" + CONTAINER_DIGEST, 40, version))
        connection.execute(sql("ACTIVATE_COMMAND", repository), (TENANT, "initial_test", MEMBER, PROFILE, GENERATION, version, 40))
        if fail_late:
            connection.execute("INSERT INTO profile_generation_activate_commands SELECT * FROM profile_generation_activate_commands")


def test_initial_self_service() -> None:
    with initial_database() as connection:
        initialize(connection)
        assert tuple(profile_row(connection)) == ("READY", GENERATION, 2)
        assert generation_row(connection)["status"] == "VERIFIED"
        admission = connection.execute("SELECT device_id, generation_id, status FROM device_authorizations").fetchall()
        assert [tuple(row) for row in admission] == [("device_initial_member", GENERATION, "ACTIVE")]
        # Old technical APIs remain owner-only for the same MEMBER.
        expect_integrity_error(lambda: register(connection, actor_id=MEMBER, command_id="member_admin_denied", generation_id=SECOND_GENERATION, object_key="profiles/v1/second.enc"), "owner_required")
        connection.rollback()
    for role, device, version, late in [("PROFILE_VIEWER", "device_initial_member", 1, False), ("PROFILE_OPERATOR", "device_foreign", 1, False), ("PROFILE_OPERATOR", "device_initial_member", 2, False), ("PROFILE_OPERATOR", "device_initial_member", 1, True)]:
        with initial_database(role) as connection:
            expect_integrity_error(lambda: initialize(connection, device, version, late), "")
            assert tuple(profile_row(connection)) == ("DRAFT", None, 1)
            assert connection.execute("SELECT COUNT(*) FROM profile_generations").fetchone()[0] == 0
            assert connection.execute("SELECT COUNT(*) FROM device_authorizations").fetchone()[0] == 0
    with initial_database() as connection:
        connection.execute("DELETE FROM profile_grants WHERE actor_id = ?", (MEMBER,))
        connection.commit()
        expect_integrity_error(lambda: initialize(connection), "access_required")
        assert tuple(profile_row(connection)) == ("DRAFT", None, 1)


def test_user_access_on_registered_computers() -> None:
    import re
    source = (ROOT / "crates/cloudflare-adapters/src/d1_device_authorization.rs").read_text()
    match = re.search(r'const ADMIT_PROFILE_DEVICE: &str = r#"(.*?)"#;', source, re.S)
    assert match
    admission_sql = match.group(1)
    with initial_database() as connection:
        initialize(connection)
        # Same user, second computer: pairing does not revoke the first.
        connection.execute("INSERT INTO device_public_key_binding_commands VALUES (?, ?, ?, ?, ?, 1, 2, 50)", (TENANT, "d" * 64, MEMBER, "device_member_second", "b" * 182))
        assert connection.execute("SELECT COUNT(*) FROM device_actor_bindings WHERE actor_id = ? AND status = 'ACTIVE'", (MEMBER,)).fetchone()[0] == 2
        # Another authorized user also opens the same profile on their own machine.
        connection.execute("INSERT INTO device_public_key_binding_commands VALUES (?, ?, ?, ?, ?, NULL, 1, 50)", (TENANT, "e" * 64, OWNER, "device_owner_second", "c" * 182))
        for actor, device in [(MEMBER, "device_member_second"), (OWNER, "device_owner_second")]:
            connection.execute(admission_sql, (TENANT, actor, PROFILE, 60, device))
            assert connection.execute("SELECT COUNT(*) FROM device_authorizations WHERE device_id = ? AND generation_id = ? AND status = 'ACTIVE'", (device, GENERATION)).fetchone()[0] == 1
        # A locator for someone else's device grants nothing, even for the tenant owner.
        connection.execute("DELETE FROM device_authorizations WHERE device_id = 'device_member_second'")
        connection.execute(admission_sql, (TENANT, OWNER, PROFILE, 60, "device_member_second"))
        assert connection.execute("SELECT COUNT(*) FROM device_authorizations WHERE device_id = 'device_member_second'").fetchone()[0] == 0
        # Revoking the first computer by exact binding version leaves the second active.
        connection.execute("INSERT INTO device_binding_revoke_commands VALUES (?, 'revoke_first', ?, ?, 1, 70)", (TENANT, OWNER, MEMBER))
        assert connection.execute("SELECT status FROM device_actor_bindings WHERE actor_id = ? AND version = 1", (MEMBER,)).fetchone()[0] == "REVOKED"
        assert connection.execute("SELECT status FROM device_actor_bindings WHERE actor_id = ? AND version = 2", (MEMBER,)).fetchone()[0] == "ACTIVE"
        # ACL revocation prevents admission to the current generation, despite ACTIVE device state.
        connection.execute("DELETE FROM profile_grants WHERE actor_id = ?", (MEMBER,))
        connection.execute(admission_sql, (TENANT, MEMBER, PROFILE, 80, "device_member_second"))
        assert connection.execute("SELECT COUNT(*) FROM device_authorizations WHERE device_id = 'device_member_second'").fetchone()[0] == 0
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []


def main() -> int:
    test_initial_self_service()
    test_user_access_on_registered_computers()
    connection = database()
    try:
        seed(connection)
        test_registration_guards(connection)
        test_verification_and_activation(connection)
        test_quarantine(connection)
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        connection.close()
    print("Profile generation registry and activation guards passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
