#!/usr/bin/env python3
"""Prove A1 Client creation + creator grant is one fail-closed D1 outcome."""

from __future__ import annotations

import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
CATALOG = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_catalog.rs"
CLIENT_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_clients.rs"
IDENTITY_QUERIES = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_identity_queries.rs"
CREATE_POLICY = ROOT / "crates" / "application-ports" / "src" / "client_creation.rs"

TENANT = "tenant_A1_creator"
MEMBER = "actor_A1_creator"
OTHER = "actor_A1_unrelated"
RACED = "actor_A1_raced"
NOW = 1000
EXPIRES = 2000


def raw_const(source: str, name: str) -> str:
    match = re.search(rf'const\s+{re.escape(name)}:\s*&str\s*=\s*r#"(.*?)"#;', source, re.DOTALL)
    if match is None:
        raise AssertionError(f"missing Rust SQL constant {name}")
    return match.group(1)


def visible_client_query() -> str:
    source = IDENTITY_QUERIES.read_text(encoding="utf-8")
    method = source.split("pub async fn find_visible_client", 1)
    if len(method) != 2:
        raise AssertionError("missing grant-safe find_visible_client query")
    match = re.search(r'query!\(.*?r#"(.*?)"#,', method[1], re.DOTALL)
    if match is None:
        raise AssertionError("could not extract find_visible_client SQL")
    return match.group(1)


def load_schema(connection: sqlite3.Connection) -> None:
    connection.execute("PRAGMA foreign_keys = ON")
    for path in sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql")):
        connection.executescript(path.read_text(encoding="utf-8"))
    connection.commit()


def seed(connection: sqlite3.Connection) -> None:
    connection.execute(
        "INSERT INTO tenants VALUES (?, 'A1 Creator Tenant', 'ACTIVE', 1, ?, ?)",
        (TENANT, NOW, NOW),
    )
    for actor, subject in (
        (MEMBER, "a1-creator@example.invalid"),
        (OTHER, "a1-unrelated@example.invalid"),
        (RACED, "a1-raced@example.invalid"),
    ):
        identity = f"identity_{actor}"
        connection.execute(
            "INSERT INTO identities(identity_id, access_subject, created_at_ms) VALUES (?, ?, ?)",
            (identity, subject, NOW),
        )
        connection.execute(
            """
            INSERT INTO memberships(
                tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms
            ) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, ?, ?)
            """,
            (TENANT, actor, identity, NOW, NOW),
        )
    connection.commit()


def sql_contract() -> dict[str, str]:
    source = CATALOG.read_text(encoding="utf-8")
    return {
        name: raw_const(source, name)
        for name in (
            "CLIENT_CREATE",
            "CLIENT_CREATOR_GRANT",
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
    client: str,
    idem: str,
    audit: str,
    outbox: str,
) -> None:
    connection.execute(
        sql["CLIENT_CREATE"],
        (TENANT, client, "PERSON", "A1 Client", actor, actor, NOW, NOW),
    )
    connection.execute(
        sql["CLIENT_CREATOR_GRANT"],
        (TENANT, actor, client, "CLIENT_EDITOR", actor, "client creator access", NOW),
    )
    connection.execute(
        sql["IDEMPOTENCY_CREATE"],
        (TENANT, actor, idem, "0123456789abcdef0123456789abcdef", client, NOW, EXPIRES),
    )
    connection.execute(
        sql["AUDIT_CREATE"],
        (TENANT, audit, "corr_A1_creator", actor, client, NOW),
    )
    connection.execute(sql["OUTBOX_CREATE"], (TENANT, outbox, client, "{}", NOW))


def count(connection: sqlite3.Connection, table: str, column: str, value: str) -> int:
    row = connection.execute(
        f"SELECT COUNT(*) FROM {table} WHERE tenant_id = ? AND {column} = ?",
        (TENANT, value),
    ).fetchone()
    assert row is not None
    return int(row[0])


def assert_application_policy_is_inner_owned() -> None:
    policy = CREATE_POLICY.read_text(encoding="utf-8")
    adapter = CLIENT_ADAPTER.read_text(encoding="utf-8")
    for marker in (
        "pub trait ClientCreateGrantSpec",
        "ClientGrantRole::Editor",
        'CLIENT_CREATOR_GRANT_REASON: &str = "client creator access"',
        "must persist the Client, this creator grant and command evidence",
    ):
        assert marker in policy, f"application-owned creator policy missing {marker!r}"
    for marker in (
        "write.creator_grant_role()",
        "write.creator_grant_reason()",
        "catalog_creator_grant_role(write.creator_grant_role())",
    ):
        assert marker in adapter, f"D1 adapter is not consuming inner creator policy: {marker!r}"
    create_block = adapter.split("async fn create_client", 1)[1].split("async fn find_visible_client", 1)[0]
    assert "CatalogClientGrantRole::Editor" not in create_block, "D1 create path must not choose creator ACL policy"


def assert_success(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)
    with connection:
        execute_create(
            connection,
            sql,
            actor=MEMBER,
            client="client_A1_success",
            idem="idem_A1_success",
            audit="audit_A1_success",
            outbox="outbox_A1_success",
        )

    grant = connection.execute(
        """
        SELECT actor_id, role, granted_by_actor_id, reason
        FROM client_grants
        WHERE tenant_id = ? AND client_id = ?
        """,
        (TENANT, "client_A1_success"),
    ).fetchall()
    assert grant == [(MEMBER, "CLIENT_EDITOR", MEMBER, "client creator access")]
    assert count(connection, "client_grants", "actor_id", OTHER) == 0
    assert count(connection, "clients", "client_id", "client_A1_success") == 1
    assert count(connection, "idempotency_records", "idempotency_key", "idem_A1_success") == 1
    assert count(connection, "audit_events", "audit_event_id", "audit_A1_success") == 1
    assert count(connection, "outbox_events", "outbox_event_id", "outbox_A1_success") == 1

    visibility = visible_client_query()
    creator_row = connection.execute(
        visibility,
        (TENANT, "client_A1_success", 0, MEMBER),
    ).fetchone()
    unrelated_row = connection.execute(
        visibility,
        (TENANT, "client_A1_success", 0, OTHER),
    ).fetchone()
    assert creator_row is not None, "creator grant must make the new Client immediately visible"
    assert unrelated_row is None, "same-tenant unrelated Member must not see creator-owned Client"
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
                client="client_A1_raced",
                idem="idem_A1_raced",
                audit="audit_A1_raced",
                outbox="outbox_A1_raced",
            )
    except sqlite3.IntegrityError as exc:
        assert "client_grant_membership_not_active" in str(exc)
    else:
        raise AssertionError("suspended creator unexpectedly received a Client/grant")

    assert count(connection, "clients", "client_id", "client_A1_raced") == 0
    assert count(connection, "client_grants", "client_id", "client_A1_raced") == 0
    assert count(connection, "idempotency_records", "idempotency_key", "idem_A1_raced") == 0
    assert count(connection, "audit_events", "audit_event_id", "audit_A1_raced") == 0
    assert count(connection, "outbox_events", "outbox_event_id", "outbox_A1_raced") == 0
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
        ) VALUES (?, ?, ?, ?, 'fixture', 'client', ?, 'fixture', ?)
        """,
        (TENANT, "audit_A1_latefail", "corr_A1_fixture", MEMBER, "client_A1_fixture", NOW),
    )
    connection.commit()

    try:
        with connection:
            execute_create(
                connection,
                sql,
                actor=MEMBER,
                client="client_A1_latefail",
                idem="idem_A1_latefail",
                audit="audit_A1_latefail",
                outbox="outbox_A1_latefail",
            )
    except sqlite3.IntegrityError as exc:
        assert "UNIQUE constraint failed" in str(exc)
    else:
        raise AssertionError("late audit collision unexpectedly committed Client/grant half-state")

    assert count(connection, "clients", "client_id", "client_A1_latefail") == 0
    assert count(connection, "client_grants", "client_id", "client_A1_latefail") == 0
    assert count(connection, "idempotency_records", "idempotency_key", "idem_A1_latefail") == 0
    assert count(connection, "outbox_events", "outbox_event_id", "outbox_A1_latefail") == 0
    assert count(connection, "audit_events", "audit_event_id", "audit_A1_latefail") == 1
    connection.close()


def main() -> int:
    assert_application_policy_is_inner_owned()
    sql = sql_contract()
    assert_success(sql)
    assert_membership_race_rolls_back(sql)
    assert_late_failure_rolls_back(sql)
    print("A1 Client creator-grant policy, visibility and atomic D1 invariants passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
