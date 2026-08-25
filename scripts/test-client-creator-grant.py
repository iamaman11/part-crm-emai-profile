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
    match = re.search(rf'const\\s+{re.escape(name)}:\\s*&str\\s*=\\s*r#"(.*?)"#;', source, re.DOTALL)
    if match is None:
        raise AssertionError(f"missing Rust SQL constant {name}")
    return match.group(1)


def visible_client_query() -> str:
    source = IDENTITY_QUERIES.read_text(encoding="utf-8")
    method = source.split("pub async fn find_visible_client", 1)
    if len(method) != 2:
        raise AssertionError("missing grant-safe find_visible_client query")
    match = re.search(r'query!\\(.*?r#"(.*?)"#,', method[1], re.DOTALL)
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
        (TENANT, actor, idem, client, NOW, EXPIRES),
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
