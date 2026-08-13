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
    if match is None:
        raise AssertionError("could not extract Client Mail eligibility SQL")
    return match.group(1)


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
        """
        INSERT INTO memberships(
            tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'TENANT_OWNER', 'ACTIVE', 1, ?, ?)
        """,
        (tenant, owner, identity, NOW, NOW),
    )


def create_member(connection: sqlite3.Connection, actor: str) -> None:
    identity = f"identity_{actor}"
    connection.execute(
        "INSERT INTO identities(identity_id, access_subject, created_at_ms) VALUES (?, ?, ?)",
        (identity, f"{actor}@example.invalid", NOW),
    )
    connection.execute(
        """
        INSERT INTO memberships(
            tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, ?, ?)
        """,
        (TENANT, actor, identity, NOW, NOW),
    )


def seed(connection: sqlite3.Connection, catalog_sql: dict[str, str]) -> None:
    create_tenant(connection, TENANT, OWNER)
    create_tenant(connection, OTHER_TENANT, OTHER_OWNER)
    create_member(connection, MEMBER)
    create_member(connection, UNRELATED)

    for tenant, client, creator in (
        (TENANT, CLIENT_A, OWNER),
        (TENANT, CLIENT_B, OWNER),
        (OTHER_TENANT, OTHER_CLIENT, OTHER_OWNER),
    ):
        connection.execute(
            catalog_sql["CLIENT_CREATE"],
            (tenant, client, "PERSON", client, creator, creator, NOW, NOW),
        )

    connection.execute(
        catalog_sql["CLIENT_CREATOR_GRANT"],
        (TENANT, MEMBER, CLIENT_A, "CLIENT_VIEWER", OWNER, "Batch B member read grant", NOW),
    )

    for binding, provider in (
        (MAILBOX_A, "GMAIL_API"),
        (MAILBOX_B, "IMAP"),
        (MAILBOX_UNASSIGNED, "GMAIL_API"),
        (MAILBOX_BROWSER, "BROWSER_FALLBACK"),
        (MAILBOX_REVOKED, "GMAIL_API"),
    ):
        connection.execute(
            """
            INSERT INTO mailbox_binding_create_commands(
                tenant_id, command_id, command_actor_id, binding_id,
                provider, secret_handle, executed_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (TENANT, f"cmd_create_{binding}", OWNER, binding, provider, f"secret_{binding}", NOW),
        )

    connection.execute(
        """
        INSERT INTO mailbox_binding_revoke_commands(
            tenant_id, command_id, command_actor_id, binding_id,
            expected_binding_version, executed_at_ms
        ) VALUES (?, 'cmd_revoke_B_mailbox', ?, ?, 1, ?)
        """,
        (TENANT, OWNER, MAILBOX_REVOKED, NOW + 1),
    )
    connection.commit()


def sql_contract() -> dict[str, str]:
    association = ASSOCIATION_ADAPTER.read_text(encoding="utf-8")
    catalog = CATALOG.read_text(encoding="utf-8")
    return {
        "ASSOCIATION_COMMAND": raw_const(association, "ASSOCIATION_COMMAND"),
        "IDEMPOTENCY_CREATE": raw_const(association, "IDEMPOTENCY_CREATE"),
        "AUDIT_CREATE": raw_const(association, "AUDIT_CREATE"),
        "OUTBOX_CREATE": raw_const(association, "OUTBOX_CREATE"),
        "CLIENT_CREATE": raw_const(catalog, "CLIENT_CREATE"),
        "CLIENT_CREATOR_GRANT": raw_const(catalog, "CLIENT_CREATOR_GRANT"),
    }


def change(
    connection: sqlite3.Connection,
    sql: dict[str, str],
    *,
    binding: str,
    expected: int,
    next_version: int,
    operation: str,
    previous_client: str | None,
    next_client: str | None,
    suffix: str,
    at: int,
) -> None:
    result = {"BIND": "bound", "REBIND": "rebound", "UNBIND": "unbound"}[operation]
    action = {
        "BIND": "mailbox.client_bind",
        "REBIND": "mailbox.client_rebind",
        "UNBIND": "mailbox.client_unbind",
    }[operation]
    event = {
        "BIND": "mailbox.client_bound.v1",
        "REBIND": "mailbox.client_rebound.v1",
        "UNBIND": "mailbox.client_unbound.v1",
    }[operation]
    connection.execute(
        sql["ASSOCIATION_COMMAND"],
        (
            TENANT,
            f"cmd_assoc_{suffix}",
            OWNER,
            binding,
            expected,
            next_version,
            operation,
            previous_client,
            next_client,
            at,
        ),
    )
    connection.execute(
        sql["IDEMPOTENCY_CREATE"],
        (
            TENANT,
            OWNER,
            f"idem_assoc_{suffix}",
            "mailbox.client_association_change",
            f"digest_assoc_{suffix}_0123456789abcdef",
            result,
            binding,
            at,
            EXPIRES + at,
        ),
    )
    connection.execute(
        sql["AUDIT_CREATE"],
        (
            TENANT,
            f"audit_assoc_{suffix}",
            f"corr_assoc_{suffix}",
            OWNER,
            action,
            "mailbox_client_association",
            binding,
            result,
            at,
        ),
    )
    connection.execute(
        sql["OUTBOX_CREATE"],
        (
            TENANT,
            f"outbox_assoc_{suffix}",
            "mailbox_client_association",
            binding,
            next_version,
            event,
            "{}",
            at,
        ),
    )


def eligible(connection: sqlite3.Connection, actor: str, client: str, binding: str) -> bool:
    row = connection.execute(
        eligibility_sql(),
        (binding, TENANT, client, actor),
    ).fetchone()
    return row is not None


def state(connection: sqlite3.Connection, binding: str) -> tuple[str | None, int] | None:
    row = connection.execute(
        """
        SELECT client_id, version
        FROM mailbox_client_association_state
        WHERE tenant_id = ? AND binding_id = ?
        """,
        (TENANT, binding),
    ).fetchone()
    return None if row is None else (row[0], int(row[1]))


def history(connection: sqlite3.Connection, binding: str) -> list[tuple[object, ...]]:
    return connection.execute(
        """
        SELECT version, operation, previous_client_id, next_client_id
        FROM mailbox_client_association_history
        WHERE tenant_id = ? AND binding_id = ?
        ORDER BY version
        """,
        (TENANT, binding),
    ).fetchall()


def assert_architecture_markers() -> None:
    domain = RELATIONSHIP_DOMAIN.read_text(encoding="utf-8")
    eligibility = ELIGIBILITY_ADAPTER.read_text(encoding="utf-8")
    mail_use_case = MAIL_USE_CASE.read_text(encoding="utf-8")

    for forbidden in ("GMAIL", "IMAP", "BROWSER_FALLBACK", "D1Database", "worker::"):
        assert forbidden not in domain, f"provider/storage concern leaked into relationship domain: {forbidden}"
    for required in (
        "MailboxClientAssociationVersion",
        "MailboxClientAssociationAction::Bind",
        "MailboxClientAssociationAction::Rebind",
        "MailboxClientAssociationAction::Unbind",
        "VersionConflict",
    ):
        assert required in domain
    for required in (
        "mailbox_client_association_state",
        "client_grants",
        "requester.status = 'ACTIVE'",
        "client.status = 'ACTIVE'",
        "binding.status = 'ACTIVE'",
        "binding.execution_status = 'ACTIVE'",
        "binding.provider IN ('GMAIL_API', 'IMAP')",
        "association.client_id = client.client_id",
    ):
        assert required in eligibility, f"eligibility missing {required!r}"
    assert "profile_client_assignments" not in eligibility, "Profile assignment must never substitute for Client Mail ACL"
    assert "QueryCapability::Mail" not in mail_use_case, "Client Mail must not retain the Owner-only coarse Mail gate"
    assert mail_use_case.count("QueryCapability::Clients") == 2


def assert_relationship_and_eligibility(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection, sql)

    assert state(connection, MAILBOX_A) is None
    assert state(connection, MAILBOX_UNASSIGNED) is None
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_UNASSIGNED)

    with connection:
        change(
            connection,
            sql,
            binding=MAILBOX_A,
            expected=0,
            next_version=1,
            operation="BIND",
            previous_client=None,
            next_client=CLIENT_A,
            suffix="bind_a",
            at=1100,
        )
        change(
            connection,
            sql,
            binding=MAILBOX_B,
            expected=0,
            next_version=1,
            operation="BIND",
            previous_client=None,
            next_client=CLIENT_A,
            suffix="bind_b",
            at=1110,
        )
        change(
            connection,
            sql,
            binding=MAILBOX_BROWSER,
            expected=0,
            next_version=1,
            operation="BIND",
            previous_client=None,
            next_client=CLIENT_A,
            suffix="bind_browser",
            at=1120,
        )

    assert state(connection, MAILBOX_A) == (CLIENT_A, 1)
    assert state(connection, MAILBOX_B) == (CLIENT_A, 1)
    assert eligible(connection, MEMBER, CLIENT_A, MAILBOX_A)
    assert eligible(connection, MEMBER, CLIENT_A, MAILBOX_B)
    assert eligible(connection, OWNER, CLIENT_A, MAILBOX_A)
    assert not eligible(connection, UNRELATED, CLIENT_A, MAILBOX_A)
    assert not eligible(connection, MEMBER, CLIENT_A, MAILBOX_BROWSER)
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_REVOKED)

    with connection:
        change(
            connection,
            sql,
            binding=MAILBOX_A,
            expected=1,
            next_version=2,
            operation="REBIND",
            previous_client=CLIENT_A,
            next_client=CLIENT_B,
            suffix="rebind_a",
            at=1200,
        )
    assert state(connection, MAILBOX_A) == (CLIENT_B, 2)
    assert history(connection, MAILBOX_A) == [
        (1, "BIND", None, CLIENT_A),
        (2, "REBIND", CLIENT_A, CLIENT_B),
    ]
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_A)
    assert eligible(connection, OWNER, CLIENT_B, MAILBOX_A)
    assert not eligible(connection, MEMBER, CLIENT_B, MAILBOX_A), "Client A grant must not authorize Client B"

    connection.execute(
        "UPDATE clients SET status = 'ARCHIVED' WHERE tenant_id = ? AND client_id = ?",
        (TENANT, CLIENT_B),
    )
    connection.commit()
    assert not eligible(connection, OWNER, CLIENT_B, MAILBOX_A)
    connection.rollback()

    connection.execute(
        "UPDATE memberships SET status = 'SUSPENDED' WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, MEMBER),
    )
    connection.commit()
    assert not eligible(connection, MEMBER, CLIENT_A, MAILBOX_B)
    connection.execute(
        "UPDATE memberships SET status = 'ACTIVE' WHERE tenant_id = ? AND actor_id = ?",
        (TENANT, MEMBER),
    )
    connection.commit()

    with connection:
        change(
            connection,
            sql,
            binding=MAILBOX_B,
            expected=1,
            next_version=2,
            operation="UNBIND",
            previous_client=CLIENT_A,
            next_client=None,
            suffix="unbind_b",
            at=1300,
        )
    assert state(connection, MAILBOX_B) == (None, 2)
    assert not eligible(connection, OWNER, CLIENT_A, MAILBOX_B)
    assert history(connection, MAILBOX_B)[-1] == (2, "UNBIND", CLIENT_A, None)
    connection.close()


def assert_fail_closed_cas_cross_tenant_and_raw_writes(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection, sql)
    with connection:
        change(
            connection,
            sql,
            binding=MAILBOX_A,
            expected=0,
            next_version=1,
            operation="BIND",
            previous_client=None,
            next_client=CLIENT_A,
            suffix="baseline",
            at=1100,
        )

    for values, expected_error in (
        ((MAILBOX_A, 0, 1, "REBIND", CLIENT_A, CLIENT_B, "stale"), "version_mismatch"),
        ((MAILBOX_UNASSIGNED, 0, 1, "BIND", None, OTHER_CLIENT, "cross"), "target_not_active"),
    ):
        binding, expected, next_version, operation, previous, next_client, suffix = values
        try:
            with connection:
                change(
                    connection,
                    sql,
                    binding=binding,
                    expected=expected,
                    next_version=next_version,
                    operation=operation,
                    previous_client=previous,
                    next_client=next_client,
                    suffix=suffix,
                    at=1400,
                )
        except sqlite3.IntegrityError as exc:
            assert expected_error in str(exc), str(exc)
        else:
            raise AssertionError(f"{suffix} relationship mutation unexpectedly committed")

    try:
        connection.execute(
            """
            UPDATE mailbox_client_association_state
            SET client_id = ?, version = version + 1
            WHERE tenant_id = ? AND binding_id = ?
            """,
            (CLIENT_B, TENANT, MAILBOX_A),
        )
    except sqlite3.IntegrityError as exc:
        assert "not_governed" in str(exc)
    else:
        raise AssertionError("raw relationship state update unexpectedly succeeded")
    connection.rollback()
    assert state(connection, MAILBOX_A) == (CLIENT_A, 1)
    connection.close()


def assert_late_evidence_failure_rolls_back(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection, sql)
    connection.execute(
        """
        INSERT INTO audit_events(
            tenant_id, audit_event_id, correlation_id, actor_id, action,
            resource_type, resource_id, result_code, occurred_at_ms
        ) VALUES (?, 'audit_assoc_late', 'corr_assoc_fixture', ?, 'fixture',
                  'mailbox_client_association', ?, 'fixture', ?)
        """,
        (TENANT, OWNER, MAILBOX_A, NOW),
    )
    connection.commit()

    try:
        with connection:
            connection.execute(
                sql["ASSOCIATION_COMMAND"],
                (TENANT, "cmd_assoc_late", OWNER, MAILBOX_A, 0, 1, "BIND", None, CLIENT_A, 1500),
            )
            connection.execute(
                sql["IDEMPOTENCY_CREATE"],
                (
                    TENANT,
                    OWNER,
                    "idem_assoc_late",
                    "mailbox.client_association_change",
                    "digest_assoc_late_0123456789abcdef",
                    "bound",
                    MAILBOX_A,
                    1500,
                    EXPIRES + 1500,
                ),
            )
            connection.execute(
                sql["AUDIT_CREATE"],
                (
                    TENANT,
                    "audit_assoc_late",
                    "corr_assoc_late",
                    OWNER,
                    "mailbox.client_bind",
                    "mailbox_client_association",
                    MAILBOX_A,
                    "bound",
                    1500,
                ),
            )
    except sqlite3.IntegrityError as exc:
        assert "UNIQUE constraint failed" in str(exc)
    else:
        raise AssertionError("late evidence collision unexpectedly committed relationship state")

    assert state(connection, MAILBOX_A) is None
    assert history(connection, MAILBOX_A) == []
    assert connection.execute(
        "SELECT COUNT(*) FROM mailbox_client_association_commands WHERE tenant_id = ? AND binding_id = ?",
        (TENANT, MAILBOX_A),
    ).fetchone()[0] == 0
    assert connection.execute(
        "SELECT COUNT(*) FROM idempotency_records WHERE tenant_id = ? AND idempotency_key = 'idem_assoc_late'",
        (TENANT,),
    ).fetchone()[0] == 0
    connection.close()


def main() -> int:
    assert_architecture_markers()
    sql = sql_contract()
    assert_relationship_and_eligibility(sql)
    assert_fail_closed_cas_cross_tenant_and_raw_writes(sql)
    assert_late_evidence_failure_rolls_back(sql)
    print("Batch B mailbox Client relationship and Client Mail eligibility invariants passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
