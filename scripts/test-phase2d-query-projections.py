#!/usr/bin/env python3
"""Repository-local Phase 2D query projection and plan evidence."""

from __future__ import annotations

import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
D1_QUERY = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_query.rs"
D1_GLOBAL_QUERY = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_global_query.rs"
D1_CONTACT_QUERY = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_contact_query.rs"
GLOBAL_PORT = ROOT / "crates" / "application-ports" / "src" / "query_global.rs"
CONTACT_TOKEN = bytes(range(32))

CLIENT_LIST_SQL = """
SELECT client.client_id
FROM clients AS client
WHERE client.tenant_id = ?
  AND client.client_id > ?
  AND EXISTS (
      SELECT 1 FROM memberships AS membership
      WHERE membership.tenant_id = client.tenant_id
        AND membership.actor_id = ?
        AND membership.status = 'ACTIVE'
        AND (
            membership.role = 'TENANT_OWNER'
            OR (membership.role = 'MEMBER' AND EXISTS (
                SELECT 1 FROM client_grants AS grant_row
                WHERE grant_row.tenant_id = client.tenant_id
                  AND grant_row.actor_id = membership.actor_id
                  AND grant_row.client_id = client.client_id
            ))
        )
  )
ORDER BY client.client_id
LIMIT ?
"""

PROFILE_LIST_SQL = """
SELECT profile.profile_id
FROM browser_profiles AS profile
WHERE profile.tenant_id = ?
  AND profile.profile_id > ?
  AND EXISTS (
      SELECT 1 FROM memberships AS membership
      WHERE membership.tenant_id = profile.tenant_id
        AND membership.actor_id = ?
        AND membership.status = 'ACTIVE'
        AND (
            membership.role = 'TENANT_OWNER'
            OR (membership.role = 'MEMBER' AND EXISTS (
                SELECT 1 FROM profile_grants AS grant_row
                WHERE grant_row.tenant_id = profile.tenant_id
                  AND grant_row.actor_id = membership.actor_id
                  AND grant_row.profile_id = profile.profile_id
            ))
        )
  )
ORDER BY profile.profile_id
LIMIT ?
"""

MEMBER_LIST_SQL = """
SELECT member.actor_id
FROM memberships AS member
WHERE member.tenant_id = ?
  AND member.actor_id > ?
  AND EXISTS (
      SELECT 1 FROM memberships AS requester
      WHERE requester.tenant_id = member.tenant_id
        AND requester.actor_id = ?
        AND requester.status = 'ACTIVE'
        AND requester.role = 'TENANT_OWNER'
  )
ORDER BY member.actor_id
LIMIT ?
"""

MAILBOX_LIST_SQL = """
SELECT binding.binding_id
FROM mailbox_bindings AS binding
WHERE binding.tenant_id = ?
  AND binding.binding_id > ?
  AND EXISTS (
      SELECT 1 FROM memberships AS requester
      WHERE requester.tenant_id = binding.tenant_id
        AND requester.actor_id = ?
        AND requester.status = 'ACTIVE'
        AND requester.role = 'TENANT_OWNER'
  )
ORDER BY binding.binding_id
LIMIT ?
"""

CLIENT_EXACT_SQL = """
SELECT client.client_id
FROM clients AS client
WHERE client.tenant_id = ?
  AND client.client_id = ?
  AND EXISTS (
      SELECT 1 FROM memberships AS membership
      WHERE membership.tenant_id = client.tenant_id
        AND membership.actor_id = ?
        AND membership.status = 'ACTIVE'
        AND (
            membership.role = 'TENANT_OWNER'
            OR (membership.role = 'MEMBER' AND EXISTS (
                SELECT 1 FROM client_grants AS grant_row
                WHERE grant_row.tenant_id = client.tenant_id
                  AND grant_row.actor_id = membership.actor_id
                  AND grant_row.client_id = client.client_id
            ))
        )
  )
"""

PROFILE_EXACT_SQL = """
SELECT profile.profile_id
FROM browser_profiles AS profile
WHERE profile.tenant_id = ?
  AND profile.profile_id = ?
  AND EXISTS (
      SELECT 1 FROM memberships AS membership
      WHERE membership.tenant_id = profile.tenant_id
        AND membership.actor_id = ?
        AND membership.status = 'ACTIVE'
        AND (
            membership.role = 'TENANT_OWNER'
            OR (membership.role = 'MEMBER' AND EXISTS (
                SELECT 1 FROM profile_grants AS grant_row
                WHERE grant_row.tenant_id = profile.tenant_id
                  AND grant_row.actor_id = membership.actor_id
                  AND grant_row.profile_id = profile.profile_id
            ))
        )
  )
"""

CONTACT_EXACT_SQL = """
SELECT contact.client_id, contact.contact_point_id
FROM client_contact_points AS contact
WHERE contact.tenant_id = ?
  AND contact.kind = 'EMAIL'
  AND contact.normalization_version = 1
  AND contact.lookup_key_version = 1
  AND contact.exact_lookup_token = ?
  AND contact.status = 'ACTIVE'
  AND EXISTS (
      SELECT 1 FROM clients AS client
      WHERE client.tenant_id = contact.tenant_id
        AND client.client_id = contact.client_id
        AND client.status = 'ACTIVE'
  )
  AND EXISTS (
      SELECT 1 FROM memberships AS membership
      WHERE membership.tenant_id = contact.tenant_id
        AND membership.actor_id = ?
        AND membership.status = 'ACTIVE'
        AND (
            membership.role = 'TENANT_OWNER'
            OR (membership.role = 'MEMBER' AND EXISTS (
                SELECT 1 FROM client_grants AS grant_row
                WHERE grant_row.tenant_id = contact.tenant_id
                  AND grant_row.actor_id = membership.actor_id
                  AND grant_row.client_id = contact.client_id
            ))
        )
  )
ORDER BY contact.contact_point_id
LIMIT ?
"""


def apply_real_migrations(connection: sqlite3.Connection) -> None:
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted(MIGRATIONS.glob("*.sql")):
        connection.executescript(migration.read_text(encoding="utf-8"))


def plan(connection: sqlite3.Connection, sql: str, args: tuple[object, ...]) -> str:
    rows = connection.execute("EXPLAIN QUERY PLAN " + sql, args).fetchall()
    return "\n".join(str(row[3]) for row in rows)


def assert_indexed_real_schema() -> None:
    connection = sqlite3.connect(":memory:")
    apply_real_migrations(connection)
    cases = [
        ("clients", CLIENT_LIST_SQL, ("tenant_01", "", "actor_01", 26), "client"),
        ("profiles", PROFILE_LIST_SQL, ("tenant_01", "", "actor_01", 26), "profile"),
        ("members", MEMBER_LIST_SQL, ("tenant_01", "", "actor_01", 26), "member"),
        ("mailboxes", MAILBOX_LIST_SQL, ("tenant_01", "", "actor_01", 26), "binding"),
        ("client exact", CLIENT_EXACT_SQL, ("tenant_01", "client_01", "actor_01"), "client"),
        ("profile exact", PROFILE_EXACT_SQL, ("tenant_01", "profile_01", "actor_01"), "profile"),
    ]
    for label, sql, args, alias in cases:
        details = plan(connection, sql, args)
        upper = details.upper()
        if f"SCAN {alias.upper()}" in upper and "USING" not in upper:
            raise AssertionError(f"{label} query is not index-backed:\n{details}")
        if "SEARCH" not in upper:
            raise AssertionError(f"{label} query plan has no indexed SEARCH:\n{details}")

    contact_details = plan(
        connection,
        CONTACT_EXACT_SQL,
        ("tenant_01", CONTACT_TOKEN, "actor_01", 20),
    )
    if "client_contact_exact_lookup" not in contact_details:
        raise AssertionError(
            "exact contact query did not use Phase 2B HMAC index:\n" + contact_details
        )
    connection.close()


def semantic_schema() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.executescript(
        """
        CREATE TABLE memberships (
            tenant_id TEXT NOT NULL, actor_id TEXT NOT NULL, role TEXT NOT NULL,
            status TEXT NOT NULL, PRIMARY KEY (tenant_id, actor_id)
        );
        CREATE TABLE clients (
            tenant_id TEXT NOT NULL, client_id TEXT NOT NULL, status TEXT NOT NULL,
            PRIMARY KEY (tenant_id, client_id)
        );
        CREATE TABLE client_grants (
            tenant_id TEXT NOT NULL, actor_id TEXT NOT NULL, client_id TEXT NOT NULL,
            PRIMARY KEY (tenant_id, actor_id, client_id)
        );
        CREATE TABLE client_contact_points (
            tenant_id TEXT NOT NULL, client_id TEXT NOT NULL, contact_point_id TEXT NOT NULL,
            kind TEXT NOT NULL, normalization_version INTEGER NOT NULL,
            lookup_key_version INTEGER NOT NULL, exact_lookup_token BLOB NOT NULL,
            status TEXT NOT NULL, PRIMARY KEY (tenant_id, contact_point_id)
        );
        CREATE TABLE browser_profiles (
            tenant_id TEXT NOT NULL, profile_id TEXT NOT NULL,
            PRIMARY KEY (tenant_id, profile_id)
        );
        CREATE TABLE profile_grants (
            tenant_id TEXT NOT NULL, actor_id TEXT NOT NULL, profile_id TEXT NOT NULL,
            PRIMARY KEY (tenant_id, actor_id, profile_id)
        );
        CREATE TABLE profile_client_assignments (
            tenant_id TEXT NOT NULL, assignment_id TEXT NOT NULL, profile_id TEXT NOT NULL,
            client_id TEXT NOT NULL, closed_at_ms INTEGER,
            PRIMARY KEY (tenant_id, assignment_id)
        );
        CREATE TABLE mailbox_bindings (
            tenant_id TEXT NOT NULL, binding_id TEXT NOT NULL,
            PRIMARY KEY (tenant_id, binding_id)
        );
        """
    )
    connection.executemany(
        "INSERT INTO memberships VALUES (?, ?, ?, ?)",
        [
            ("tenant_a", "actor_owner", "TENANT_OWNER", "ACTIVE"),
            ("tenant_a", "actor_member", "MEMBER", "ACTIVE"),
            ("tenant_a", "actor_other", "MEMBER", "ACTIVE"),
            ("tenant_b", "actor_b", "TENANT_OWNER", "ACTIVE"),
        ],
    )
    connection.executemany(
        "INSERT INTO clients VALUES (?, ?, 'ACTIVE')",
        [("tenant_a", "client_a"), ("tenant_b", "client_b")],
    )
    connection.execute(
        "INSERT INTO client_grants VALUES ('tenant_a', 'actor_member', 'client_a')"
    )
    connection.execute(
        "INSERT INTO client_contact_points VALUES "
        "('tenant_a', 'client_a', 'contact_a', 'EMAIL', 1, 1, ?, 'ACTIVE')",
        (CONTACT_TOKEN,),
    )
    connection.executemany(
        "INSERT INTO browser_profiles VALUES (?, ?)",
        [("tenant_a", "profile_a"), ("tenant_b", "profile_b")],
    )
    connection.execute(
        "INSERT INTO profile_client_assignments VALUES "
        "('tenant_a', 'assignment_a', 'profile_a', 'client_a', NULL)"
    )
    connection.executemany(
        "INSERT INTO mailbox_bindings VALUES (?, ?)",
        [("tenant_a", "binding_a"), ("tenant_b", "binding_b")],
    )
    return connection


def values(connection: sqlite3.Connection, sql: str, args: tuple[object, ...]) -> list[str]:
    return [str(row[0]) for row in connection.execute(sql, args).fetchall()]


def assert_security_semantics() -> None:
    connection = semantic_schema()
    assert values(connection, CLIENT_LIST_SQL, ("tenant_a", "", "actor_member", 26)) == ["client_a"]
    assert values(connection, CLIENT_LIST_SQL, ("tenant_a", "", "actor_b", 26)) == []
    assert values(connection, CLIENT_EXACT_SQL, ("tenant_a", "client_b", "actor_member")) == []
    assert values(connection, CONTACT_EXACT_SQL, ("tenant_a", CONTACT_TOKEN, "actor_member", 20)) == ["client_a"]
    assert values(connection, CONTACT_EXACT_SQL, ("tenant_b", CONTACT_TOKEN, "actor_member", 20)) == []

    connection.execute(
        "DELETE FROM client_grants WHERE tenant_id='tenant_a' AND actor_id='actor_member'"
    )
    assert values(connection, CLIENT_LIST_SQL, ("tenant_a", "", "actor_member", 26)) == []
    assert values(connection, CLIENT_EXACT_SQL, ("tenant_a", "client_a", "actor_member")) == []
    assert values(connection, CONTACT_EXACT_SQL, ("tenant_a", CONTACT_TOKEN, "actor_member", 20)) == []

    assert values(connection, PROFILE_LIST_SQL, ("tenant_a", "", "actor_member", 26)) == []
    assert values(connection, PROFILE_EXACT_SQL, ("tenant_a", "profile_a", "actor_member")) == []
    connection.execute("INSERT INTO profile_grants VALUES ('tenant_a', 'actor_member', 'profile_a')")
    assert values(connection, PROFILE_LIST_SQL, ("tenant_a", "", "actor_member", 26)) == ["profile_a"]
    connection.execute("DELETE FROM profile_grants WHERE tenant_id='tenant_a' AND actor_id='actor_member'")
    assert values(connection, PROFILE_LIST_SQL, ("tenant_a", "", "actor_member", 26)) == []

    assert values(connection, MEMBER_LIST_SQL, ("tenant_a", "", "actor_member", 26)) == []
    assert values(connection, MEMBER_LIST_SQL, ("tenant_a", "", "actor_owner", 26)) == [
        "actor_member", "actor_other", "actor_owner"
    ]
    assert values(connection, MAILBOX_LIST_SQL, ("tenant_a", "", "actor_member", 26)) == []
    assert values(connection, MAILBOX_LIST_SQL, ("tenant_a", "", "actor_owner", 26)) == ["binding_a"]
    connection.execute(
        "UPDATE memberships SET status='REVOKED' WHERE tenant_id='tenant_a' AND actor_id='actor_owner'"
    )
    assert values(connection, MAILBOX_LIST_SQL, ("tenant_a", "", "actor_owner", 26)) == []
    assert values(connection, CONTACT_EXACT_SQL, ("tenant_a", CONTACT_TOKEN, "actor_owner", 20)) == []
    connection.close()


def sql_literals(source: str) -> str:
    return "\n".join(re.findall(r'r#"(.*?)"#', source, flags=re.DOTALL))


def assert_source_privacy_boundaries() -> None:
    d1_query = D1_QUERY.read_text(encoding="utf-8")
    d1_global = D1_GLOBAL_QUERY.read_text(encoding="utf-8")
    d1_contact = D1_CONTACT_QUERY.read_text(encoding="utf-8")
    global_port = GLOBAL_PORT.read_text(encoding="utf-8")
    combined_sql = "\n".join(sql_literals(source) for source in (d1_query, d1_global, d1_contact))
    for fragment in [" LIKE ", " GLOB ", " MATCH ", "COUNT("]:
        if fragment.lower() in combined_sql.lower():
            raise AssertionError(f"forbidden Phase 2D query SQL present: {fragment}")
    if "secret_handle" in d1_query + d1_global + d1_contact:
        raise AssertionError("query projections must never read mailbox secret handles")
    if "alice@example.com" not in global_port:
        raise AssertionError("global-search negative PII fixture is missing")
    for prefix in ("client_", "profile_", "actor_", "binding_"):
        if prefix not in global_port:
            raise AssertionError(f"opaque global-search prefix missing: {prefix}")
    if "client_contact_points" in d1_global:
        raise AssertionError("global exact search must not become a contact scan")


def main() -> int:
    assert_indexed_real_schema()
    assert_security_semantics()
    assert_source_privacy_boundaries()
    print("Phase 2D grant-safe bounded query projections and query plans passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
