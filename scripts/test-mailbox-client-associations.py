#!/usr/bin/env python3
"""Prove Batch B mailbox-to-Client authority and Client Mail eligibility."""

from __future__ import annotations

import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
ASSOCIATION_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_mailbox_client_associations.rs"
ELIGIBILITY_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_client_mail_eligibility.rs"
CATALOG = ROOT / "crates/cloudflare-adapters/src/d1_catalog.rs"
MAIL_USE_CASE = ROOT / "crates/use-cases-query/src/mail.rs"
RELATIONSHIP_DOMAIN = ROOT / "crates/mailbox-domain/src/client_association.rs"

TENANT = "tenant_B_mail_client"
OTHER_TENANT = "tenant_B_other"
OWNER = "actor_B_owner"
MEMBER = "actor_B_member"
UNRELATED = "actor_B_unrelated"
OTHER_OWNER = "actor_B_other_owner"
CLIENT_A = "client_B_alpha"
CLIENT_B = "client_B_beta"
OTHER_CLIENT = "client_B_other"
MAILBOX_A = "mailbox_B_gmail"
MAILBOX_B = "mailbox_B_imap"
MAILBOX_UNASSIGNED = "mailbox_B_unassigned"
MAILBOX_BROWSER = "mailbox_B_browser"
MAILBOX_REVOKED = "mailbox_B_revoked"
NOW = 1000
EXPIRES = 5000


def raw_const(source: str, name: str) -> str:
    match = re.search(rf'const\s+{re.escape(name)}:\s*&str\s*=\s*r#"(.*?)"#;', source, re.DOTALL)
    if match is None:
        raise AssertionError(f"missing Rust SQL constant {name}")
    return match.group(1)


def eligibility_sql() -> str:
    source = ELIGIBILITY_ADAPTER.read_text(encoding="utf-8")
    match = re.search(r'query!\(\s*&self\.database,\s*r#"(.*?)"#,', source, re.DOTALL)
    if match is not None:
        return match.group(1)
    return raw_const(source, "CLIENT_MAILBOX_ACCESS")


def load_schema(connection: sqlite3.Connection) -> None:
    connection.execute("PRAGMA foreign_keys = ON")
    files = sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    versions = [int(path.name.split("_", 1)[0]) for path in files]
    assert versions == list(range(1, len(files) + 1)), versions
    for path in files:
        connection.executescript(path.read_text(encoding="utf-8"))
    connection.commit()


def create_tenant(connection: sqlite3.Connection, tenant: str, owner: str) -> None:
    identity = f"identity_{owner}"
    connection.execute(
        "INSERT INTO tenants VALUES (?, ?, 'ACTIVE', 1, ?, ?)",
        (tenant, tenant, NOW, NOW),
    )
    connection.execute(
        "INSERT INTO identities(identity_id, access_subject, created_at_ms) VALUES (?, ?, ?)",
        (identity, f"{owner}@example.invalid", NOW),
    )
    connection.execute(
        "INSERT INTO tenant_owners VALUES (?, ?, ?, 1, ?, ?)",
        (tenant, owner, identity, NOW, NOW),
    )
    connection.execute(
        "INSERT INTO memberships VALUES (?, ?, 'TENANT_OWNER', 'ACTIVE', 1, ?, ?, ?)",
        (tenant, owner, identity, NOW, NOW, NOW),
    )


def create_member(connection: sqlite3.Connection, actor: str) -> None:
    identity = f"identity_{actor}"
    connection.execute(
        "INSERT INTO identities(identity_id, access_subject, created_at_ms) VALUES (?, ?, ?)",
        (identity, f"{actor}@example.invalid", NOW),
    )
    connection.execute(
        "INSERT INTO memberships VALUES (?, ?, 'MEMBER', 'ACTIVE', 1, ?, ?, ?)",
        (TENANT, actor, identity, NOW, NOW, NOW),
    )


def create_client(connection: sqlite3.Connection, tenant: str, client: str) -> None:
    connection.execute(
        "INSERT INTO clients VALUES (?, ?, ?, 'ACTIVE', 1, ?, ?)",
        (tenant, client, client, NOW, NOW),
    )


def create_mailbox(
    connection: sqlite3.Connection,
    tenant: str,
    mailbox: str,
    provider: str,
    status: str = "ACTIVE",
) -> None:
    connection.execute(
        """
        INSERT INTO mailbox_bindings(
            tenant_id, binding_id, provider, profile_id, status, version,
            secret_handle, secret_fingerprint, execution_status, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, NULL, ?, 1, ?, ?, 'ACTIVE', ?, ?)
        """,
        (
            tenant,
            mailbox,
            provider,
            status,
            f"secret://{mailbox}",
            f"fingerprint_{mailbox}",
            NOW,
            NOW,
        ),
    )


def grant_client(connection: sqlite3.Connection, actor: str, client: str) -> None:
    connection.execute(
        "INSERT INTO client_grants VALUES (?, ?, ?, ?, ?)",
        (TENANT, client, actor, NOW, OWNER),
    )


def seed(connection: sqlite3.Connection) -> None:
    create_tenant(connection, TENANT, OWNER)
    create_tenant(connection, OTHER_TENANT, OTHER_OWNER)
    create_member(connection, MEMBER)
    create_member(connection, UNRELATED)
    create_client(connection, TENANT, CLIENT_A)
    create_client(connection, TENANT, CLIENT_B)
    create_client(connection, OTHER_TENANT, OTHER_CLIENT)
    create_mailbox(connection, TENANT, MAILBOX_A, "GMAIL_API")
    create_mailbox(connection, TENANT, MAILBOX_B, "IMAP")
    create_mailbox(connection, TENANT, MAILBOX_UNASSIGNED, "GMAIL_API")
    create_mailbox(connection, TENANT, MAILBOX_BROWSER, "BROWSER_FALLBACK")
    create_mailbox(connection, TENANT, MAILBOX_REVOKED, "IMAP", "REVOKED")
    grant_client(connection, MEMBER, CLIENT_A)


def bind(
    connection: sqlite3.Connection,
    mailbox: str,
    client: str,
    actor: str = OWNER,
    association_id: str | None = None,
) -> str:
    association_id = association_id or f"assoc_{mailbox}_{client}"
    connection.execute(
        """
        INSERT INTO mailbox_client_associations(
            tenant_id, association_id, binding_id, client_id, state, version,
            bound_at_ms, bound_by_actor_id, released_at_ms, released_by_actor_id
        ) VALUES (?, ?, ?, ?, 'ACTIVE', 1, ?, ?, NULL, NULL)
        """,
        (TENANT, association_id, mailbox, client, NOW, actor),
    )
    return association_id


def release(connection: sqlite3.Connection, association_id: str, version: int = 1) -> None:
    connection.execute(
        """
        UPDATE mailbox_client_associations
        SET state = 'RELEASED', version = ?, released_at_ms = ?, released_by_actor_id = ?
        WHERE tenant_id = ? AND association_id = ? AND version = ?
        """,
        (version + 1, NOW + version, OWNER, TENANT, association_id, version),
    )


def eligible(connection: sqlite3.Connection, actor: str, client: str, mailbox: str) -> bool:
    row = connection.execute(
        eligibility_sql(),
        (mailbox, TENANT, client, actor),
    ).fetchone()
    return row is not None


def assert_relationship_and_eligibility(sql: str) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)

    association_a = bind(connection, MAILBOX_A, CLIENT_A)
    assert eligible(connection, OWNER, CLIENT_A, MAILBOX_A)
    assert eligible(connection, MEMBER, CLIENT_A, MAILBOX_A)
    assert not eligible(connection, UNRELATED, CLIENT_A, MAILBOX_A)
    assert not eligible(connection, OWNER, CLIENT_B, MAILBOX_A)
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_UNASSIGNED)
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_BROWSER)
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_REVOKED)

    connection.execute(
        "UPDATE mailbox_bindings SET execution_status = 'BLOCKED' WHERE tenant_id = ? AND binding_id = ?",
        (TENANT, MAILBOX_A),
    )
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_A)
    connection.execute(
        "UPDATE mailbox_bindings SET execution_status = 'ACTIVE' WHERE tenant_id = ? AND binding_id = ?",
        (TENANT, MAILBOX_A),
    )

    release(connection, association_a)
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_A)
    association_b = bind(connection, MAILBOX_A, CLIENT_B, association_id="assoc_rebound")
    assert eligible(connection, OWNER, CLIENT_B, MAILBOX_A)
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_A)

    try:
        bind(connection, MAILBOX_A, CLIENT_A, association_id="assoc_conflict")
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("one active Client per mailbox invariant was not enforced")

    try:
        connection.execute(
            """
            INSERT INTO mailbox_client_associations(
                tenant_id, association_id, binding_id, client_id, state, version,
                bound_at_ms, bound_by_actor_id, released_at_ms, released_by_actor_id
            ) VALUES (?, 'assoc_cross_tenant', ?, ?, 'ACTIVE', 1, ?, ?, NULL, NULL)
            """,
            (OTHER_TENANT, MAILBOX_B, OTHER_CLIENT, NOW, OTHER_OWNER),
        )
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("cross-tenant mailbox association was accepted")

    release(connection, association_b)
    assert not eligible(connection, OWNER, CLIENT_B, MAILBOX_A)
    assert "mailbox_client_association_state" in sql
    assert "CREATE UNIQUE INDEX mailbox_client_one_active_client" in sql
    connection.close()


def assert_history_and_version_guards() -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)
    association_id = bind(connection, MAILBOX_A, CLIENT_A)
    release(connection, association_id)
    row = connection.execute(
        "SELECT state, version, released_at_ms FROM mailbox_client_associations WHERE association_id = ?",
        (association_id,),
    ).fetchone()
    assert row is not None
    assert row[0] == "RELEASED"
    assert row[1] == 2
    assert row[2] is not None

    try:
        connection.execute(
            "DELETE FROM mailbox_client_associations WHERE tenant_id = ? AND association_id = ?",
            (TENANT, association_id),
        )
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("association history deletion was accepted")

    connection.close()


def assert_adapter_and_domain_boundaries() -> None:
    association_adapter = ASSOCIATION_ADAPTER.read_text(encoding="utf-8")
    eligibility_adapter = ELIGIBILITY_ADAPTER.read_text(encoding="utf-8")
    mail_use_case = MAIL_USE_CASE.read_text(encoding="utf-8")
    relationship_domain = RELATIONSHIP_DOMAIN.read_text(encoding="utf-8")

    assert "MailboxClientAssociationPort" in association_adapter
    assert "mailbox_client_association_state" in eligibility_adapter
    assert "ClientMailboxEligibilityPort" in eligibility_adapter
    assert "is_mailbox_eligible" in mail_use_case
    assert "profile_assignment" not in eligibility_adapter.lower()
    assert "mailbox_client" in relationship_domain.lower()


def main() -> int:
    migration_sql = "\n".join(
        path.read_text(encoding="utf-8")
        for path in sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    )
    assert_relationship_and_eligibility(migration_sql)
    assert_history_and_version_guards()
    assert_adapter_and_domain_boundaries()
    print("Batch B mailbox Client relationship, authorization, history and eligibility invariants passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
