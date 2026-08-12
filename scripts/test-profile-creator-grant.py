#!/usr/bin/env python3
"""Prove A2 Profile creation + creator grant is one fail-closed D1 outcome."""

from __future__ import annotations

import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
GOVERNED = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_governed_commands.rs"
PROFILE_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_profiles.rs"
IDENTITY_QUERIES = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_identity_queries.rs"
CREATE_POLICY = ROOT / "crates" / "application-ports" / "src" / "profile_creation.rs"
MIGRATION = MIGRATIONS / "0022_profile_creator_grant.sql"

TENANT = "tenant_A2_creator"
OWNER = "actor_A2_owner"
MEMBER = "actor_A2_creator"
OTHER = "actor_A2_unrelated"
RACED = "actor_A2_raced"
NOW = 1000
EXPIRES = 2000


def raw_const(source: str, name: str) -> str:
    match = re.search(rf'const\s+{re.escape(name)}:\s*&str\s*=\s*r#"(.*?)"#;', source, re.DOTALL)
    if match is None:
        raise AssertionError(f"missing Rust SQL constant {name}")
    return match.group(1)


def visible_profile_query() -> str:
    source = IDENTITY_QUERIES.read_text(encoding="utf-8")
    method = source.split("pub async fn find_visible_profile", 1)
    if len(method) != 2:
        raise AssertionError("missing grant-safe find_visible_profile query")
    match = re.search(r'query!\(.*?r#"(.*?)"#,', method[1], re.DOTALL)
    if match is None:
        raise AssertionError("could not extract find_visible_profile SQL")
    return match.group(1)


def load_schema(connection: sqlite3.Connection) -> None:
    connection.execute("PRAGMA foreign_keys = ON")
    for path in sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql")):
        connection.executescript(path.read_text(encoding="utf-8"))
    connection.commit()


def seed(connection: sqlite3.Connection) -> None:
    connection.execute(
        "INSERT INTO tenants VALUES (?, 'A2 Creator Tenant', 'ACTIVE', 1, ?, ?)",
        (TENANT, NOW, NOW),
    )
    actors = (
        (OWNER, "a2-owner@example.invalid", "TENANT_OWNER"),
        (MEMBER, "a2-creator@example.invalid", "MEMBER"),
        (OTHER, "a2-unrelated@example.invalid", "MEMBER"),
        (RACED, "a2-raced@example.invalid", "MEMBER"),
    )
    for actor, subject, role in actors:
        identity = f"identity_{actor}"
        connection.execute(
            "INSERT INTO identities(identity_id, access_subject, created_at_ms) VALUES (?, ?, ?)",
            (identity, subject, NOW),
        )
        connection.execute(
            """
            INSERT INTO memberships(
                tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms
            ) VALUES (?, ?, ?, ?, 'ACTIVE', 1, ?, ?)
            """,
            (TENANT, actor, identity, role, NOW, NOW),
        )
    connection.commit()


def sql_contract() -> dict[str, str]:
    source = GOVERNED.read_text(encoding="utf-8")
    return {
        name: raw_const(source, name)
        for name in (
            "PROFILE_CREATE_COMMAND",
            "PROFILE_CREATOR_GRANT",
            "IDEMPOTENCY_CREATE",
            "AUDIT_CREATE",
            "OUTBOX_CREATE",
        )
    }


def execute_create(
    connection: sqlite3.Connection,
    sql: dict[str, str],
    *,
    actor: str,
    profile: str,
    command: str,
    idem: str,
    audit: str,
    outbox: str,
) -> None:
    connection.execute(
        sql["PROFILE_CREATE_COMMAND"],
        (TENANT, command, actor, profile, NOW),
    )
    connection.execute(
        sql["PROFILE_CREATOR_GRANT"],
        (TENANT, actor, profile, "PROFILE_OPERATOR", actor, "profile creator access", NOW),
    )
    connection.execute(
        sql["IDEMPOTENCY_CREATE"],
        (
            TENANT,
            actor,
            idem,
            "profile.create",
            "0123456789abcdef0123456789abcdef",
            "created",
            profile,
            NOW,
            EXPIRES,
        ),
    )
    connection.execute(
        sql["AUDIT_CREATE"],
        (TENANT, audit, "corr_A2_creator", actor, "profile.create", "profile", profile, "created", NOW),
    )
    connection.execute(
        sql["OUTBOX_CREATE"],
        (TENANT, outbox, "profile", profile, 1, "profile.created.v1", "{}", NOW),
    )


def count(connection: sqlite3.Connection, table: str, column: str, value: str) -> int:
    row = connection.execute(
        f"SELECT COUNT(*) FROM {table} WHERE tenant_id = ? AND {column} = ?",
        (TENANT, value),
    ).fetchone()
    assert row is not None
    return int(row[0])


def assert_application_policy_is_inner_owned() -> None:
    policy = CREATE_POLICY.read_text(encoding="utf-8")
    adapter = PROFILE_ADAPTER.read_text(encoding="utf-8")
    governed = GOVERNED.read_text(encoding="utf-8")
    migration = MIGRATION.read_text(encoding="utf-8")

    for marker in (
        "pub trait ProfileCreateGrantSpec",
        "ProfileGrantRole::Operator",
        'PROFILE_CREATOR_GRANT_REASON: &str = "profile creator access"',
        "must persist the Profile, this creator grant and command evidence",
    ):
        assert marker in policy, f"application-owned creator policy missing {marker!r}"

    for marker in (
        "write.creator_grant_role()",
        "write.creator_grant_reason()",
        "map_profile_grant_role(write.creator_grant_role())",
    ):
        assert marker in adapter, f"D1 adapter is not consuming inner creator policy: {marker!r}"

    create_block = adapter.split("async fn create_profile", 1)[1].split("async fn find_visible_profile", 1)[0]
    assert "ProfileGrantRole::Operator" not in create_block, "D1 create adapter must not choose creator ACL policy"
    assert "creator_grant_role.database_value()" in governed
    assert "creator_grant_reason" in governed

    assert "DROP TRIGGER profile_create_command_validate" in migration
    assert "profile_create_membership_not_active" in migration
    assert "status = 'ACTIVE'" in migration
    assert "role = 'TENANT_OWNER'" not in migration


def assert_success(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)

    with connection:
        execute_create(
            connection,
            sql,
            actor=MEMBER,
            profile="profile_A2_member",
            command="command_A2_member",
            idem="idem_A2_member",
            audit="audit_A2_member",
            outbox="outbox_A2_member",
        )
        execute_create(
            connection,
            sql,
            actor=OWNER,
            profile="profile_A2_owner",
            command="command_A2_owner",
            idem="idem_A2_owner",
            audit="audit_A2_owner",
            outbox="outbox_A2_owner",
        )

    member_grant = connection.execute(
        """
        SELECT actor_id, role, granted_by_actor_id, reason
        FROM profile_grants
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT, "profile_A2_member"),
    ).fetchall()
    assert member_grant == [(MEMBER, "PROFILE_OPERATOR", MEMBER, "profile creator access")]

    owner_grant = connection.execute(
        """
        SELECT actor_id, role, granted_by_actor_id, reason
        FROM profile_grants
        WHERE tenant_id = ? AND profile_id = ?
        """,
        (TENANT, "profile_A2_owner"),
    ).fetchall()
    assert owner_grant == [(OWNER, "PROFILE_OPERATOR", OWNER, "profile creator access")]

    assert count(connection, "profile_grants", "actor_id", OTHER) == 0
    assert count(connection, "browser_profiles", "profile_id", "profile_A2_member") == 1
    assert count(connection, "browser_profiles", "profile_id", "profile_A2_owner") == 1
    assert count(connection, "idempotency_records", "idempotency_key", "idem_A2_member") == 1
    assert count(connection, "audit_events", "audit_event_id", "audit_A2_member") == 1
    assert count(connection, "outbox_events", "outbox_event_id", "outbox_A2_member") == 1

    visibility = visible_profile_query()
    creator_row = connection.execute(
        visibility,
        (TENANT, "profile_A2_member", 0, MEMBER),
    ).fetchone()
    unrelated_row = connection.execute(
        visibility,
        (TENANT, "profile_A2_member", 0, OTHER),
    ).fetchone()
    assert creator_row is not None, "creator grant must make the new Profile immediately visible"
    assert unrelated_row is None, "same-tenant unrelated Member must not see creator-owned Profile"
    connection.close()


def assert_membership_race_rolls_back(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)
    connection.execute(
        "UPDATE memberships SET status = 'SUSPENDED', version = 2, updated_at_ms = ? WHERE tenant_id = ? AND actor_id = ?",
        (NOW + 1, TENANT, RACED),
    )
    connection.commit()

    try:
        with connection:
            execute_create(
                connection,
                sql,
                actor=RACED,
                profile="profile_A2_raced",
                command="command_A2_raced",
                idem="idem_A2_raced",
                audit="audit_A2_raced",
                outbox="outbox_A2_raced",
            )
    except sqlite3.IntegrityError as exc:
        assert "profile_create_membership_not_active" in str(exc)
    else:
        raise AssertionError("suspended creator unexpectedly received a Profile/grant")

    assert count(connection, "profile_create_commands", "profile_id", "profile_A2_raced") == 0
    assert count(connection, "browser_profiles", "profile_id", "profile_A2_raced") == 0
    assert count(connection, "profile_grants", "profile_id", "profile_A2_raced") == 0
    assert count(connection, "idempotency_records", "idempotency_key", "idem_A2_raced") == 0
    assert count(connection, "audit_events", "audit_event_id", "audit_A2_raced") == 0
    assert count(connection, "outbox_events", "outbox_event_id", "outbox_A2_raced") == 0
    connection.close()


def assert_late_failure_rolls_back(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)
    connection.execute(
        """
        INSERT INTO audit_events(
            tenant_id, audit_event_id, correlation_id, actor_id, action,
            resource_type, resource_id, result_code, occurred_at_ms
        ) VALUES (?, ?, ?, ?, 'fixture', 'profile', ?, 'fixture', ?)
        """,
        (TENANT, "audit_A2_latefail", "corr_A2_fixture", MEMBER, "profile_A2_fixture", NOW),
    )
    connection.commit()

    try:
        with connection:
            execute_create(
                connection,
                sql,
                actor=MEMBER,
                profile="profile_A2_latefail",
                command="command_A2_latefail",
                idem="idem_A2_latefail",
                audit="audit_A2_latefail",
                outbox="outbox_A2_latefail",
            )
    except sqlite3.IntegrityError as exc:
        assert "UNIQUE constraint failed" in str(exc)
    else:
        raise AssertionError("late audit collision unexpectedly committed Profile/grant half-state")

    assert count(connection, "profile_create_commands", "profile_id", "profile_A2_latefail") == 0
    assert count(connection, "browser_profiles", "profile_id", "profile_A2_latefail") == 0
    assert count(connection, "profile_grants", "profile_id", "profile_A2_latefail") == 0
    assert count(connection, "idempotency_records", "idempotency_key", "idem_A2_latefail") == 0
    assert count(connection, "outbox_events", "outbox_event_id", "outbox_A2_latefail") == 0
    assert count(connection, "audit_events", "audit_event_id", "audit_A2_latefail") == 1
    connection.close()


def main() -> int:
    assert_application_policy_is_inner_owned()
    sql = sql_contract()
    assert_success(sql)
    assert_membership_race_rolls_back(sql)
    assert_late_failure_rolls_back(sql)
    print("A2 Profile creator-grant policy, visibility and atomic D1 invariants passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
