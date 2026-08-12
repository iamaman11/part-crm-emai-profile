#!/usr/bin/env python3
"""Prove Pre-2J C1 mailbox onboarding lifecycle and atomic D1 authority."""

from __future__ import annotations

import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"
ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_mailbox_onboarding.rs"
DOMAIN = ROOT / "crates/mailbox-domain/src/onboarding.rs"

TENANT = "tenant_C1_onboarding"
OTHER_TENANT = "tenant_C1_other"
OWNER = "actor_C1_owner"
MEMBER = "actor_C1_member"
OTHER_OWNER = "actor_C1_other_owner"
NOW = 10_000
EXPIRES = 50_000


def raw_const(source: str, name: str) -> str:
    match = re.search(rf'const\s+{re.escape(name)}:\s*&str\s*=\s*r#"(.*?)"#;', source, re.DOTALL)
    if match is None:
        raise AssertionError(f"missing Rust SQL constant {name}")
    return match.group(1)


def sql_contract() -> dict[str, str]:
    source = ADAPTER.read_text(encoding="utf-8")
    return {
        name: raw_const(source, name)
        for name in ("ONBOARDING_COMMAND", "IDEMPOTENCY_CREATE", "AUDIT_CREATE", "OUTBOX_CREATE")
    }


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


def seed(connection: sqlite3.Connection) -> None:
    create_tenant(connection, TENANT, OWNER)
    create_tenant(connection, OTHER_TENANT, OTHER_OWNER)
    identity = f"identity_{MEMBER}"
    connection.execute(
        "INSERT INTO identities(identity_id, access_subject, created_at_ms) VALUES (?, ?, ?)",
        (identity, f"{MEMBER}@example.invalid", NOW),
    )
    connection.execute(
        """
        INSERT INTO memberships(
            tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, 'MEMBER', 'ACTIVE', 1, ?, ?)
        """,
        (TENANT, MEMBER, identity, NOW, NOW),
    )
    connection.commit()


def execute_change(
    connection: sqlite3.Connection,
    sql: dict[str, str],
    *,
    onboarding: str,
    provider: str,
    expected: int,
    next_version: int,
    operation: str,
    previous_status: str | None,
    next_status: str,
    previous_handle: str | None,
    next_handle: str | None,
    metadata: str | None,
    actor: str = OWNER,
    suffix: str,
    at: int,
    audit_id: str | None = None,
) -> None:
    result = {
        "START": "started",
        "ACTIVATE": "activated",
        "REQUIRE_REAUTH": "reauth_required",
        "DISABLE": "disabled",
        "CONFIG_ERROR": "config_error",
    }[operation]
    action = {
        "START": "mailbox.onboarding_start",
        "ACTIVATE": "mailbox.onboarding_activate",
        "REQUIRE_REAUTH": "mailbox.onboarding_require_reauth",
        "DISABLE": "mailbox.onboarding_disable",
        "CONFIG_ERROR": "mailbox.onboarding_config_error",
    }[operation]
    event = {
        "START": "mailbox.onboarding_started.v1",
        "ACTIVATE": "mailbox.onboarding_activated.v1",
        "REQUIRE_REAUTH": "mailbox.onboarding_reauth_required.v1",
        "DISABLE": "mailbox.onboarding_disabled.v1",
        "CONFIG_ERROR": "mailbox.onboarding_config_error.v1",
    }[operation]
    connection.execute(
        sql["ONBOARDING_COMMAND"],
        (
            TENANT,
            f"cmd_C1_{suffix}",
            actor,
            onboarding,
            provider,
            expected,
            next_version,
            operation,
            previous_status,
            next_status,
            previous_handle,
            next_handle,
            metadata,
            at,
        ),
    )
    connection.execute(
        sql["IDEMPOTENCY_CREATE"],
        (
            TENANT,
            actor,
            f"idem_C1_{suffix}",
            "mailbox.onboarding_change",
            f"digest_C1_{suffix}_0123456789abcdef",
            result,
            onboarding,
            at,
            EXPIRES + at,
        ),
    )
    connection.execute(
        sql["AUDIT_CREATE"],
        (
            TENANT,
            audit_id or f"audit_C1_{suffix}",
            f"corr_C1_{suffix}",
            actor,
            action,
            "mailbox_onboarding",
            onboarding,
            result,
            at,
        ),
    )
    connection.execute(
        sql["OUTBOX_CREATE"],
        (
            TENANT,
            f"outbox_C1_{suffix}",
            "mailbox_onboarding",
            onboarding,
            next_version,
            event,
            "{}",
            at,
        ),
    )


def state(connection: sqlite3.Connection, onboarding: str) -> tuple[object, ...] | None:
    return connection.execute(
        """
        SELECT provider, lifecycle_status, credential_handle, status_metadata, version
        FROM mailbox_onboarding_state
        WHERE tenant_id = ? AND onboarding_id = ?
        """,
        (TENANT, onboarding),
    ).fetchone()


def assert_architecture_and_no_secret_fields(connection: sqlite3.Connection) -> None:
    domain = DOMAIN.read_text(encoding="utf-8")
    adapter = ADAPTER.read_text(encoding="utf-8")
    for forbidden in ("D1Database", "worker::", "access_token", "refresh_token", "authorization_code", "pkce_verifier", "password", "smtp_auth"):
        assert forbidden not in domain.lower() if forbidden.islower() else forbidden not in domain
    for required in (
        "MailboxOnboardingStatus::Pending",
        "MailboxOnboardingStatus::Active",
        "MailboxOnboardingStatus::ReauthRequired",
        "MailboxOnboardingStatus::Disabled",
        "MailboxOnboardingStatus::ConfigError",
        "MailboxOnboardingVersion",
        "CredentialHandleRequired",
    ):
        assert required in domain
    assert "MAILBOX_SECRET_RESOLVER" not in adapter

    forbidden_columns = (
        "password",
        "access_token",
        "refresh_token",
        "authorization_code",
        "pkce_verifier",
        "smtp_auth",
        "oauth_token",
    )
    for table in ("mailbox_onboarding_state", "mailbox_onboarding_history", "mailbox_onboarding_commands"):
        columns = {row[1].lower() for row in connection.execute(f"PRAGMA table_info({table})")}
        for forbidden in forbidden_columns:
            assert forbidden not in columns, (table, forbidden)
    assert "credential_handle" in {
        row[1] for row in connection.execute("PRAGMA table_info(mailbox_onboarding_state)")
    }


def assert_valid_lifecycle(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)
    onboarding = "onboarding_C1_valid"
    with connection:
        execute_change(
            connection,
            sql,
            onboarding=onboarding,
            provider="GMAIL_API",
            expected=0,
            next_version=1,
            operation="START",
            previous_status=None,
            next_status="PENDING",
            previous_handle=None,
            next_handle=None,
            metadata="ceremony.started",
            suffix="valid_start",
            at=NOW + 1,
        )
        execute_change(
            connection,
            sql,
            onboarding=onboarding,
            provider="GMAIL_API",
            expected=1,
            next_version=2,
            operation="ACTIVATE",
            previous_status="PENDING",
            next_status="ACTIVE",
            previous_handle=None,
            next_handle="secret_C1_handle",
            metadata="credential.accepted",
            suffix="valid_activate",
            at=NOW + 2,
        )
        execute_change(
            connection,
            sql,
            onboarding=onboarding,
            provider="GMAIL_API",
            expected=2,
            next_version=3,
            operation="REQUIRE_REAUTH",
            previous_status="ACTIVE",
            next_status="REAUTH_REQUIRED",
            previous_handle="secret_C1_handle",
            next_handle="secret_C1_handle",
            metadata="credential.expired",
            suffix="valid_reauth",
            at=NOW + 3,
        )
        execute_change(
            connection,
            sql,
            onboarding=onboarding,
            provider="GMAIL_API",
            expected=3,
            next_version=4,
            operation="ACTIVATE",
            previous_status="REAUTH_REQUIRED",
            next_status="ACTIVE",
            previous_handle="secret_C1_handle",
            next_handle="secret_C1_rotated",
            metadata=None,
            suffix="valid_reactivate",
            at=NOW + 4,
        )
        execute_change(
            connection,
            sql,
            onboarding=onboarding,
            provider="GMAIL_API",
            expected=4,
            next_version=5,
            operation="DISABLE",
            previous_status="ACTIVE",
            next_status="DISABLED",
            previous_handle="secret_C1_rotated",
            next_handle="secret_C1_rotated",
            metadata="operator.disabled",
            suffix="valid_disable",
            at=NOW + 5,
        )
    assert state(connection, onboarding) == (
        "GMAIL_API",
        "DISABLED",
        "secret_C1_rotated",
        "operator.disabled",
        5,
    )
    history = connection.execute(
        "SELECT operation, next_status, version FROM mailbox_onboarding_history WHERE tenant_id = ? AND onboarding_id = ? ORDER BY version",
        (TENANT, onboarding),
    ).fetchall()
    assert history == [
        ("START", "PENDING", 1),
        ("ACTIVATE", "ACTIVE", 2),
        ("REQUIRE_REAUTH", "REAUTH_REQUIRED", 3),
        ("ACTIVATE", "ACTIVE", 4),
        ("DISABLE", "DISABLED", 5),
    ]
    assert_architecture_and_no_secret_fields(connection)
    connection.close()


def assert_fail_closed(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)
    onboarding = "onboarding_C1_guarded"
    with connection:
        execute_change(
            connection,
            sql,
            onboarding=onboarding,
            provider="IMAP",
            expected=0,
            next_version=1,
            operation="START",
            previous_status=None,
            next_status="PENDING",
            previous_handle=None,
            next_handle=None,
            metadata=None,
            suffix="guard_start",
            at=NOW + 10,
        )

    cases = [
        dict(expected=0, next_version=1, operation="ACTIVATE", previous_status="PENDING", next_status="ACTIVE", previous_handle=None, next_handle="secret_stale", actor=OWNER, suffix="stale", error="version_mismatch"),
        dict(expected=1, next_version=2, operation="REQUIRE_REAUTH", previous_status="PENDING", next_status="REAUTH_REQUIRED", previous_handle=None, next_handle=None, actor=OWNER, suffix="invalid", error="invalid_transition"),
        dict(expected=1, next_version=2, operation="ACTIVATE", previous_status="PENDING", next_status="ACTIVE", previous_handle=None, next_handle="secret_member", actor=MEMBER, suffix="member", error="owner_required"),
    ]
    for case in cases:
        before = state(connection, onboarding)
        try:
            with connection:
                execute_change(
                    connection,
                    sql,
                    onboarding=onboarding,
                    provider="IMAP",
                    metadata=None,
                    at=NOW + 20,
                    **{key: value for key, value in case.items() if key != "error"},
                )
        except sqlite3.IntegrityError as error:
            assert case["error"] in str(error), (case["error"], error)
        else:
            raise AssertionError(f"fail-closed case unexpectedly passed: {case['suffix']}")
        assert state(connection, onboarding) == before

    try:
        connection.execute(
            "UPDATE mailbox_onboarding_state SET lifecycle_status = 'ACTIVE' WHERE tenant_id = ? AND onboarding_id = ?",
            (TENANT, onboarding),
        )
    except sqlite3.IntegrityError as error:
        assert "not_governed" in str(error)
        connection.rollback()
    else:
        raise AssertionError("raw onboarding state update unexpectedly passed")

    try:
        connection.execute(
            "INSERT INTO mailbox_onboarding_history(tenant_id,onboarding_id,version,operation,provider,previous_status,next_status,changed_by_actor_id,changed_at_ms) VALUES (?, ?, 99, 'DISABLE', 'IMAP', 'PENDING', 'DISABLED', ?, ?)",
            (TENANT, onboarding, OWNER, NOW + 30),
        )
    except sqlite3.IntegrityError as error:
        assert "not_governed" in str(error)
        connection.rollback()
    else:
        raise AssertionError("raw onboarding history insert unexpectedly passed")

    other = "onboarding_C1_cross_tenant"
    try:
        with connection:
            connection.execute(
                sql["ONBOARDING_COMMAND"],
                (TENANT, "cmd_C1_cross", OTHER_OWNER, other, "IMAP", 0, 1, "START", None, "PENDING", None, None, None, NOW + 31),
            )
    except sqlite3.IntegrityError as error:
        assert "owner_required" in str(error) or "FOREIGN KEY" in str(error)
    else:
        raise AssertionError("cross-tenant actor unexpectedly administered onboarding")
    assert state(connection, other) is None
    connection.close()


def assert_late_evidence_rolls_back(sql: dict[str, str]) -> None:
    connection = sqlite3.connect(":memory:")
    load_schema(connection)
    seed(connection)
    with connection:
        execute_change(
            connection,
            sql,
            onboarding="onboarding_C1_audit_seed",
            provider="IMAP",
            expected=0,
            next_version=1,
            operation="START",
            previous_status=None,
            next_status="PENDING",
            previous_handle=None,
            next_handle=None,
            metadata=None,
            suffix="audit_seed",
            at=NOW + 40,
            audit_id="audit_C1_collision",
        )

    failed = "onboarding_C1_late_failure"
    try:
        with connection:
            execute_change(
                connection,
                sql,
                onboarding=failed,
                provider="GMAIL_API",
                expected=0,
                next_version=1,
                operation="START",
                previous_status=None,
                next_status="PENDING",
                previous_handle=None,
                next_handle=None,
                metadata="ceremony.started",
                suffix="late_failure",
                at=NOW + 41,
                audit_id="audit_C1_collision",
            )
    except sqlite3.IntegrityError as error:
        assert "UNIQUE constraint failed" in str(error)
    else:
        raise AssertionError("late audit collision unexpectedly committed")

    assert state(connection, failed) is None
    for table, column in (
        ("mailbox_onboarding_commands", "onboarding_id"),
        ("mailbox_onboarding_history", "onboarding_id"),
        ("idempotency_records", "result_reference"),
        ("outbox_events", "aggregate_id"),
    ):
        count = connection.execute(
            f"SELECT COUNT(*) FROM {table} WHERE tenant_id = ? AND {column} = ?",
            (TENANT, failed),
        ).fetchone()[0]
        assert count == 0, (table, count)
    connection.close()


def main() -> None:
    sql = sql_contract()
    assert_valid_lifecycle(sql)
    assert_fail_closed(sql)
    assert_late_evidence_rolls_back(sql)
    print("C1 mailbox onboarding lifecycle authority checks passed")


if __name__ == "__main__":
    main()
