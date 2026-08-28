#!/usr/bin/env python3
"""Require Step 4 lifecycle and ACL writes to use governed application/adapters."""

from __future__ import annotations

import re
import sqlite3
import sys
from pathlib import Path

from test_step4_error_taxonomy import main as error_taxonomy_main

ROOT = Path(__file__).resolve().parents[1]
D1_MIGRATIONS = ROOT / "migrations/d1"
IDENTITY_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_identity_acl.rs"
IDENTITY_GOVERNANCE_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_identity_governance.rs"
IDENTITY_CEREMONY_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_identity_ceremonies.rs"
IDENTITY_GOVERNANCE_USE_CASES = ROOT / "crates/use-cases-identity/src/identity_governance.rs"
IDENTITY_CEREMONY_USE_CASES = ROOT / "crates/use-cases-identity/src/identity_ceremonies.rs"
COMMAND_IDENTITY = ROOT / "crates/cloudflare-adapters/src/d1_command_identity.rs"
GOVERNED_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_governed_commands.rs"
GENERATION_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_profile_generations.rs"
INVITATION_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_invitation_acceptance.rs"
CLIENT_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_clients.rs"
CLIENT_USE_CASES = ROOT / "crates/use-cases-clients/src/clients.rs"
CLIENT_GRANT_USE_CASES = ROOT / "crates/use-cases-clients/src/client_grants.rs"
PROFILE_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_profiles.rs"
PROFILE_USE_CASES = ROOT / "crates/use-cases/src/profiles.rs"
PROFILE_ASSIGNMENT_USE_CASES = ROOT / "crates/use-cases/src/profile_assignments.rs"
PROFILE_GRANT_USE_CASES = ROOT / "crates/use-cases/src/profile_grants.rs"
QUERY_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_query.rs"
IDENTITY_QUERY_ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_identity_queries.rs"

LEGACY_WRITE_TOKENS = (
    "OWNER_TRANSFER_DEMOTE", "OWNER_TRANSFER_PROMOTE", "INVITATION_ACCEPT_MEMBERSHIP",
    "MEMBERSHIP_STATUS_UPDATE", "PROFILE_VERSION_CAS", "PROFILE_GRANT_UPSERT",
    "CLIENT_GRANT_UPSERT", "pub async fn transfer_owner(",
    "pub async fn update_membership_status(", "pub async fn grant_profile(",
    "pub async fn grant_client(",
)
REQUIRED_IDENTITY_GOVERNANCE_APPLICATION_TOKENS = (
    'const OWNER_TRANSFER_COMMAND: &str = "membership.owner_transfer";',
    'const INVITATION_CREATE_COMMAND: &str = "invitation.create";',
    '"membership.activate"', '"membership.suspend"', '"membership.revoke"',
    "authorize_identity_governance", "decide_identity_replay", "port.transfer_owner(actor, &write)",
    "port.create_invitation(actor, &write)", "port.update_membership_status(actor, &write)",
    "IdentityGovernancePortErrorClass::Conflict",
)
REQUIRED_IDENTITY_CEREMONY_APPLICATION_TOKENS = (
    'const OWNER_BOOTSTRAP_COMMAND: &str = "tenant.owner_bootstrap";',
    'const INVITATION_ACCEPT_COMMAND: &str = "invitation.accept";',
    "find_active_identity_binding", "tenant_identity_boundary", "decide_ceremony_replay",
    "port.bootstrap_owner(&context, &write)", "port.accept_invitation(&context, &write)",
    "IdentityGovernancePortErrorClass::Conflict",
)
REQUIRED_IDENTITY_GOVERNANCE_ADAPTER_TOKENS = (
    "D1IdempotencyRepository", "OwnerTransferMutation", "CreateInvitationMutation",
    "MembershipStatusMutation", ".transfer_owner(", ".create_invitation(",
    ".update_membership_status(", "mutation_envelope(write.evidence(), write.event_payload_json())",
)
REQUIRED_IDENTITY_CEREMONY_ADAPTER_TOKENS = (
    "D1IdentityAclRepository", "D1IdempotencyRepository", "D1InvitationAcceptanceRepository",
    "BootstrapOwnerMutation", "AcceptInvitationMutation", "VerifiedBootstrapContext::from_verified_identity",
    ".bootstrap_owner(", ".accept(",
)
REQUIRED_CLIENT_APPLICATION_TOKENS = (
    'const CLIENT_CREATE_COMMAND: &str = "client.create";', "decide_replay", "create_client",
    "ClientReplayDecision::Conflict",
)
REQUIRED_CLIENT_GRANT_APPLICATION_TOKENS = (
    'const CLIENT_GRANT_COMMAND: &str = "client.grant";',
    'const CLIENT_GRANT_REVOKE_COMMAND: &str = "client.grant_revoke";',
    "decide_client_grant_replay", "port.grant_client(actor, &write)",
    "port.revoke_client_grant(actor, &write)", "ClientGrantPortErrorClass::Conflict",
)
REQUIRED_CLIENT_ADAPTER_TOKENS = (
    "CreateClientMutation", "requested_display_name", "create_client(actor, mutation)",
    "ClientGrantMutation", "MutationEnvelope", ".grant_client(actor, mutation)",
    ".revoke_client_grant(actor, mutation)",
)
REQUIRED_PROFILE_APPLICATION_TOKENS = (
    'const PROFILE_CREATE_COMMAND: &str = "profile.create";', "decide_replay", "create_profile",
    "ProfileReplayDecision::Conflict",
)
REQUIRED_PROFILE_ASSIGNMENT_APPLICATION_TOKENS = (
    'const PROFILE_ASSIGN_COMMAND: &str = "profile.assign_client";',
    'const PROFILE_DETACH_COMMAND: &str = "profile.detach_client";',
    "decide_assignment_replay", "load_profile_assignment_context", "load_profile_detachment_context",
    "port.assign_profile(actor, &write)", "port.detach_profile(actor, &write)",
    "ProfileAssignmentPortErrorClass::Conflict",
)
REQUIRED_PROFILE_GRANT_APPLICATION_TOKENS = (
    'const PROFILE_GRANT_COMMAND: &str = "profile.grant";',
    'const PROFILE_GRANT_REVOKE_COMMAND: &str = "profile.grant_revoke";',
    "decide_profile_grant_replay", "port.grant_profile(actor, &write)",
    "port.revoke_profile_grant(actor, &write)", "ProfileGrantPortErrorClass::Conflict",
)
REQUIRED_PROFILE_ADAPTER_TOKENS = (
    "CreateProfileMutation", "ProfileAssignmentMutation", "ProfileGrantMutation", "MutationEnvelope",
    "ProfileCreateGrantSpec", "map_profile_grant_role(write.creator_grant_role())",
    "write.creator_grant_reason()", ".create_profile(", ".assign_profile(actor, mutation)",
    ".detach_profile(actor, mutation)", ".grant_profile(actor, mutation)",
    ".revoke_profile_grant(actor, mutation)",
)
REQUIRED_GOVERNED_TOKENS = (
    "owner_transfer_commands", "membership_status_commands", "invitation_create_commands",
    "profile_create_commands", "PROFILE_CREATOR_GRANT", "creator_grant_role.database_value()",
    "creator_grant_reason", "profile_assignment_commands", '"profile.assign_client"',
    '"profile.detach_client"', '"profile.client_assigned.v1"', '"profile.client_detached.v1"',
    "profile_grant_commands", "client_grant_commands", "self.database.batch(statements).await",
)
REQUIRED_ACCEPTANCE_TOKENS = (
    "invitation_acceptances", "INSERT INTO memberships", "INSERT INTO idempotency_records",
    "INSERT INTO audit_events", "INSERT INTO outbox_events", "self.database.batch(statements).await",
)


def require_tokens(text: str, tokens: tuple[str, ...], label: str, errors: list[str]) -> None:
    for token in tokens:
        if token not in text:
            errors.append(f"{label} is missing required token: {token}")


def extract_rust_raw_sql(path: Path, scope_marker: str, contains: str) -> str:
    source = path.read_text(encoding="utf-8")
    if scope_marker not in source:
        raise AssertionError(f"missing Rust SQL scope marker {scope_marker!r} in {path}")
    scope = source.split(scope_marker, 1)[1]
    for match in re.finditer(r'r#"(.*?)"#', scope, re.DOTALL):
        sql = match.group(1).strip()
        if contains in sql:
            return sql
    raise AssertionError(f"missing production SQL containing {contains!r} in {path}")


def expect_integrity_failure(
    database: sqlite3.Connection,
    statement: str,
    parameters: tuple[object, ...],
    expected_reason: str,
    label: str,
    errors: list[str],
) -> None:
    before = database.total_changes
    try:
        database.execute(statement, parameters)
    except sqlite3.IntegrityError as error:
        if expected_reason not in str(error):
            errors.append(f"{label} failed with unexpected reason: {error}")
    else:
        errors.append(f"{label} unexpectedly succeeded")
    if database.total_changes != before:
        errors.append(f"{label} mutated D1 state before failing")


def seed_profile_relationship_fixture(database: sqlite3.Connection) -> None:
    database.execute(
        "INSERT INTO tenants VALUES (?, ?, ?, ?, ?, ?)",
        ("tenant_p1_fixture", "P1 fixture", "ACTIVE", 1, 0, 0),
    )
    database.execute(
        "INSERT INTO identities VALUES (?, ?, ?, ?)",
        ("identity_p1_fixture", "p1-fixture-subject", None, 0),
    )
    database.execute(
        "INSERT INTO memberships VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        (
            "tenant_p1_fixture",
            "actor_owner_p1_fixture",
            "identity_p1_fixture",
            "TENANT_OWNER",
            "ACTIVE",
            1,
            0,
            0,
        ),
    )
    for suffix in ("profile", "profile_only", "client_only", "no_client"):
        database.execute(
            "INSERT INTO identities VALUES (?, ?, ?, ?)",
            (
                f"identity_member_{suffix}_p1",
                f"p1-member-{suffix}-subject",
                None,
                0,
            ),
        )
        database.execute(
            "INSERT INTO memberships VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "tenant_p1_fixture",
                f"actor_member_{suffix}_p1",
                f"identity_member_{suffix}_p1",
                "MEMBER",
                "ACTIVE",
                1,
                0,
                0,
            ),
        )
    for client_id, display_name in (
        ("client_a_p1_fixture", "Client A"),
        ("client_b_p1_fixture", "Client B"),
    ):
        database.execute(
            "INSERT INTO clients VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "tenant_p1_fixture",
                client_id,
                "PERSON",
                display_name,
                "ACTIVE",
                1,
                "actor_owner_p1_fixture",
                "actor_owner_p1_fixture",
                0,
                0,
            ),
        )
    for profile_id in ("profile_p1_fixture", "profile_concurrent_p1_fixture"):
        database.execute(
            "INSERT INTO browser_profiles VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "tenant_p1_fixture",
                profile_id,
                "READY",
                None,
                1,
                "actor_owner_p1_fixture",
                "actor_owner_p1_fixture",
                0,
                0,
            ),
        )
    for actor_id in ("actor_member_profile_p1", "actor_member_client_only_p1"):
        database.execute(
            """
            INSERT INTO client_grants (
                tenant_id, actor_id, client_id, role,
                granted_by_actor_id, reason, created_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                "tenant_p1_fixture",
                actor_id,
                "client_b_p1_fixture",
                "CLIENT_VIEWER",
                "actor_owner_p1_fixture",
                "P1 inverse query client visibility proof",
                0,
            ),
        )
    for actor_id in ("actor_member_profile_p1", "actor_member_profile_only_p1"):
        database.execute(
            """
            INSERT INTO profile_grants (
                tenant_id, actor_id, profile_id, role,
                granted_by_actor_id, reason, created_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                "tenant_p1_fixture",
                actor_id,
                "profile_p1_fixture",
                "PROFILE_VIEWER",
                "actor_owner_p1_fixture",
                "P1 inverse query profile visibility proof",
                0,
            ),
        )


def relationship_rows(database: sqlite3.Connection) -> list[tuple[str, str, int, int | None]]:
    return database.execute(
        """
        SELECT assignment_id, client_id, assigned_at_ms, closed_at_ms
        FROM profile_client_assignments
        WHERE tenant_id = 'tenant_p1_fixture' AND profile_id = 'profile_p1_fixture'
        ORDER BY assigned_at_ms, assignment_id
        """
    ).fetchall()


def profile_version(database: sqlite3.Connection) -> int:
    row = database.execute(
        "SELECT version FROM browser_profiles WHERE tenant_id = ? AND profile_id = ?",
        ("tenant_p1_fixture", "profile_p1_fixture"),
    ).fetchone()
    if row is None:
        raise AssertionError("P1 fixture profile disappeared")
    return int(row[0])


def client_state(database: sqlite3.Connection, client_id: str) -> tuple[str, int]:
    row = database.execute(
        "SELECT status, version FROM clients WHERE tenant_id = ? AND client_id = ?",
        ("tenant_p1_fixture", client_id),
    ).fetchone()
    if row is None:
        raise AssertionError(f"P1 fixture Client disappeared: {client_id}")
    return str(row[0]), int(row[1])


def insert_relationship_command(
    database: sqlite3.Connection,
    *,
    command_id: str,
    assignment_id: str,
    client_id: str,
    expected_version: int,
    executed_at_ms: int,
    operation: str | None,
    profile_id: str = "profile_p1_fixture",
) -> None:
    columns = """
        tenant_id, command_id, command_actor_id, assignment_id, profile_id,
        client_id, expected_profile_version, reason, executed_at_ms
    """
    values: tuple[object, ...] = (
        "tenant_p1_fixture",
        command_id,
        "actor_owner_p1_fixture",
        assignment_id,
        profile_id,
        client_id,
        expected_version,
        "P1 relationship proof",
        executed_at_ms,
    )
    if operation is None:
        database.execute(
            f"INSERT INTO profile_assignment_commands ({columns}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            values,
        )
        return
    database.execute(
        f"INSERT INTO profile_assignment_commands ({columns}, operation) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        values + (operation,),
    )


def validate_inverse_relationship_acl(database: sqlite3.Connection, errors: list[str]) -> None:
    client_sql = extract_rust_raw_sql(
        IDENTITY_QUERY_ADAPTER,
        "pub async fn find_visible_client(",
        "FROM clients AS client",
    )
    profiles_sql = extract_rust_raw_sql(
        QUERY_ADAPTER,
        "impl ClientProfileReadModelPort for D1QueryRepository",
        "FROM browser_profiles AS profile",
    )
    tenant_id = "tenant_p1_fixture"
    client_id = "client_b_p1_fixture"

    def visible_client(actor_id: str, owner: int) -> bool:
        return database.execute(
            client_sql,
            (tenant_id, client_id, owner, actor_id),
        ).fetchone() is not None

    if not visible_client("actor_owner_p1_fixture", 1):
        errors.append("TenantOwner lost Client visibility in P1 inverse relationship proof")
    if not visible_client("actor_member_profile_p1", 0):
        errors.append("member with explicit Client grant cannot enter P1 Client relationship view")
    if not visible_client("actor_member_client_only_p1", 0):
        errors.append("member with Client-only grant cannot enter visible Client card for P1 proof")
    if visible_client("actor_member_profile_only_p1", 0):
        errors.append("member with Profile grant but without Client grant can enter nested Client relationship view")
    if visible_client("actor_member_no_client_p1", 0):
        errors.append("member without Client grant can see Client relationship surface")

    def visible_profiles(actor_id: str) -> list[tuple[object, ...]]:
        return database.execute(
            profiles_sql,
            (client_id, tenant_id, "", actor_id, 26),
        ).fetchall()

    if len(visible_profiles("actor_owner_p1_fixture")) != 1:
        errors.append("TenantOwner inverse Client->Profiles projection did not return the active relationship")
    if len(visible_profiles("actor_member_profile_p1")) != 1:
        errors.append("member with independent Client and Profile grants cannot see attached Profile")
    if len(visible_profiles("actor_member_profile_only_p1")) != 1:
        errors.append("independent Profile grant should remain valid even when nested Client visibility fails closed")
    if visible_profiles("actor_member_client_only_p1"):
        errors.append("Client relationship leaked attached Profile without an independent Profile grant")
    if visible_profiles("actor_member_no_client_p1"):
        errors.append("relationship exposed attached Profile to a member with no resource grants")


def validate_profile_relationship_d1_behavior(errors: list[str]) -> None:
    database = sqlite3.connect(":memory:")
    database.execute("PRAGMA foreign_keys = ON")
    try:
        migrations = sorted(D1_MIGRATIONS.glob("*.sql"))
        if not migrations or migrations[0].name != "0001_catalog.sql" or migrations[-1].name != "0028_profile_assignment_detach.sql":
            errors.append("P1 D1 behavior proof requires the exact current Catalog migration range 0001..0028")
            return

        for migration in migrations:
            database.executescript(migration.read_text(encoding="utf-8"))
            if migration.name == "0001_catalog.sql":
                seed_profile_relationship_fixture(database)

        # Legacy callers omit operation; the additive migration must preserve ASSIGN as the default.
        insert_relationship_command(
            database,
            command_id="command_attach_p1_fixture",
            assignment_id="assignment_a_p1_fixture",
            client_id="client_a_p1_fixture",
            expected_version=1,
            executed_at_ms=10,
            operation=None,
        )
        if relationship_rows(database) != [("assignment_a_p1_fixture", "client_a_p1_fixture", 10, None)]:
            errors.append("legacy ASSIGN no longer creates exactly one active Client/Profile relationship")
        if profile_version(database) != 2:
            errors.append("legacy ASSIGN must bump Profile version exactly once")

        # A failure after command + idempotency + audit + outbox inside one transaction must roll
        # the complete governed mutation envelope back, modelling the production D1 batch boundary.
        database.commit()
        database.execute("BEGIN")
        try:
            insert_relationship_command(
                database,
                command_id="command_reassign_rollback_p1_fixture",
                assignment_id="assignment_b_rollback_p1_fixture",
                client_id="client_b_p1_fixture",
                expected_version=2,
                executed_at_ms=20,
                operation="ASSIGN",
            )
            database.execute(
                """
                INSERT INTO idempotency_records (
                    tenant_id, actor_id, idempotency_key, command_name, payload_fingerprint,
                    result_code, result_reference, created_at_ms, expires_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    "tenant_p1_fixture", "actor_owner_p1_fixture", "idem_rollback_p1_fixture",
                    "profile.assign_client", "a" * 64, "assigned",
                    "assignment_b_rollback_p1_fixture", 20, 200,
                ),
            )
            database.execute(
                """
                INSERT INTO audit_events (
                    tenant_id, audit_event_id, correlation_id, actor_id, action,
                    resource_type, resource_id, result_code, occurred_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    "tenant_p1_fixture", "audit_rollback_p1_fixture", "corr_rollback_p1_fixture",
                    "actor_owner_p1_fixture", "profile.assign_client", "profile",
                    "profile_p1_fixture", "assigned", 20,
                ),
            )
            database.execute(
                """
                INSERT INTO outbox_events (
                    tenant_id, outbox_event_id, aggregate_type, aggregate_id,
                    aggregate_version, event_type, payload_json, created_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    "tenant_p1_fixture", "outbox_rollback_p1_fixture", "profile",
                    "profile_p1_fixture", 3, "profile.client_assigned.v1", "{}", 20,
                ),
            )
            database.execute("INSERT INTO p1_forced_failure_missing_table VALUES (1)")
        except sqlite3.DatabaseError:
            database.rollback()
        else:
            errors.append("P1 D1 rollback proof did not force the intended transaction failure")
            database.rollback()
        if relationship_rows(database) != [("assignment_a_p1_fixture", "client_a_p1_fixture", 10, None)]:
            errors.append("failed reassign transaction changed relationship history")
        if profile_version(database) != 2:
            errors.append("failed reassign transaction changed Profile version")
        if database.execute(
            "SELECT COUNT(*) FROM profile_assignment_commands WHERE command_id = ?",
            ("command_reassign_rollback_p1_fixture",),
        ).fetchone()[0] != 0:
            errors.append("failed reassign transaction retained its governed command row")
        rollback_envelope_count = sum(
            database.execute(statement, parameters).fetchone()[0]
            for statement, parameters in (
                (
                    "SELECT COUNT(*) FROM idempotency_records WHERE idempotency_key = ?",
                    ("idem_rollback_p1_fixture",),
                ),
                (
                    "SELECT COUNT(*) FROM audit_events WHERE audit_event_id = ?",
                    ("audit_rollback_p1_fixture",),
                ),
                (
                    "SELECT COUNT(*) FROM outbox_events WHERE outbox_event_id = ?",
                    ("outbox_rollback_p1_fixture",),
                ),
            )
        )
        if rollback_envelope_count != 0:
            errors.append("failed relationship transaction retained partial idempotency/audit/outbox state")

        insert_relationship_command(
            database,
            command_id="command_reassign_p1_fixture",
            assignment_id="assignment_b_p1_fixture",
            client_id="client_b_p1_fixture",
            expected_version=2,
            executed_at_ms=20,
            operation="ASSIGN",
        )
        if relationship_rows(database) != [
            ("assignment_a_p1_fixture", "client_a_p1_fixture", 10, 20),
            ("assignment_b_p1_fixture", "client_b_p1_fixture", 20, None),
        ]:
            errors.append("atomic reassign must close exactly the previous row and leave one active successor")
        if profile_version(database) != 3:
            errors.append("atomic reassign must bump Profile version exactly once")

        # Execute the exact production SQL from both read adapters. Relationship is only
        # projection data: Client visibility and Profile visibility remain independently granted.
        validate_inverse_relationship_acl(database, errors)

        client_lifecycle_insert = """
            INSERT INTO client_lifecycle_commands (
                tenant_id, command_id, command_actor_id, client_id,
                operation, expected_client_version, next_display_name, executed_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """
        expect_integrity_failure(
            database,
            client_lifecycle_insert,
            (
                "tenant_p1_fixture",
                "command_archive_blocked_p1_fixture",
                "actor_owner_p1_fixture",
                "client_b_p1_fixture",
                "ARCHIVE",
                1,
                None,
                25,
            ),
            "client_archive_active_assignment_identity_mismatch",
            "archive Client with active Profile relationship",
            errors,
        )
        if client_state(database, "client_b_p1_fixture") != ("ACTIVE", 1):
            errors.append("blocked Client archive changed Client state while relationship remained active")

        command_insert = """
            INSERT INTO profile_assignment_commands (
                tenant_id, command_id, command_actor_id, assignment_id, profile_id,
                client_id, expected_profile_version, reason, executed_at_ms, operation
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """

        # Two commands racing on one optimistic Profile version serialize so at most one can win.
        insert_relationship_command(
            database,
            command_id="command_concurrent_first_p1_fixture",
            assignment_id="assignment_concurrent_a_p1_fixture",
            client_id="client_a_p1_fixture",
            expected_version=1,
            executed_at_ms=22,
            operation="ASSIGN",
            profile_id="profile_concurrent_p1_fixture",
        )
        expect_integrity_failure(
            database,
            command_insert,
            (
                "tenant_p1_fixture", "command_concurrent_second_p1_fixture",
                "actor_owner_p1_fixture", "assignment_concurrent_b_p1_fixture",
                "profile_concurrent_p1_fixture", "client_b_p1_fixture", 1,
                "P1 negative proof", 23, "ASSIGN",
            ),
            "profile_assignment_version_mismatch",
            "second concurrent assign with the same expected Profile version",
            errors,
        )
        expect_integrity_failure(
            database,
            command_insert,
            (
                "tenant_p1_fixture", "command_concurrent_stale_detach_p1_fixture",
                "actor_owner_p1_fixture", "assignment_concurrent_a_p1_fixture",
                "profile_concurrent_p1_fixture", "client_a_p1_fixture", 1,
                "P1 negative proof", 24, "DETACH",
            ),
            "profile_assignment_version_mismatch",
            "detach after concurrent Profile version change",
            errors,
        )
        concurrent_rows = database.execute(
            """
            SELECT assignment_id, client_id, assigned_at_ms, closed_at_ms
            FROM profile_client_assignments
            WHERE tenant_id = ? AND profile_id = ?
            ORDER BY assigned_at_ms, assignment_id
            """,
            ("tenant_p1_fixture", "profile_concurrent_p1_fixture"),
        ).fetchall()
        concurrent_version = database.execute(
            "SELECT version FROM browser_profiles WHERE tenant_id = ? AND profile_id = ?",
            ("tenant_p1_fixture", "profile_concurrent_p1_fixture"),
        ).fetchone()
        if concurrent_rows != [
            ("assignment_concurrent_a_p1_fixture", "client_a_p1_fixture", 22, None)
        ] or concurrent_version != (2,):
            errors.append("optimistic concurrency proof did not leave exactly one committed assignment and one version bump")

        base = (
            "tenant_p1_fixture",
            "actor_owner_p1_fixture",
            "profile_p1_fixture",
            "P1 negative proof",
        )
        expect_integrity_failure(
            database,
            command_insert,
            (
                base[0], "command_stale_detach_p1_fixture", base[1], "assignment_b_p1_fixture",
                base[2], "client_b_p1_fixture", 2, base[3], 30, "DETACH",
            ),
            "profile_assignment_version_mismatch",
            "stale-version detach",
            errors,
        )
        expect_integrity_failure(
            database,
            command_insert,
            (
                base[0], "command_wrong_detach_p1_fixture", base[1], "assignment_a_p1_fixture",
                base[2], "client_b_p1_fixture", 3, base[3], 30, "DETACH",
            ),
            "profile_assignment_active_assignment_missing",
            "wrong-assignment detach",
            errors,
        )
        expect_integrity_failure(
            database,
            command_insert,
            (
                base[0], "command_wrong_client_detach_p1_fixture", base[1], "assignment_b_p1_fixture",
                base[2], "client_a_p1_fixture", 3, base[3], 30, "DETACH",
            ),
            "profile_assignment_active_assignment_missing",
            "wrong-Client detach",
            errors,
        )
        expect_integrity_failure(
            database,
            command_insert,
            (
                base[0], "command_time_regression_p1_fixture", base[1], "assignment_b_p1_fixture",
                base[2], "client_b_p1_fixture", 3, base[3], 19, "DETACH",
            ),
            "profile_assignment_time_regression",
            "time-regressing detach",
            errors,
        )
        if relationship_rows(database) != [
            ("assignment_a_p1_fixture", "client_a_p1_fixture", 10, 20),
            ("assignment_b_p1_fixture", "client_b_p1_fixture", 20, None),
        ] or profile_version(database) != 3:
            errors.append("failed detach preconditions changed the previous valid relationship")

        insert_relationship_command(
            database,
            command_id="command_detach_p1_fixture",
            assignment_id="assignment_b_p1_fixture",
            client_id="client_b_p1_fixture",
            expected_version=3,
            executed_at_ms=30,
            operation="DETACH",
        )
        if relationship_rows(database) != [
            ("assignment_a_p1_fixture", "client_a_p1_fixture", 10, 20),
            ("assignment_b_p1_fixture", "client_b_p1_fixture", 20, 30),
        ]:
            errors.append("DETACH must close only the exact active relationship and insert no successor")
        if profile_version(database) != 4:
            errors.append("DETACH must bump Profile version exactly once")

        expect_integrity_failure(
            database,
            """
            INSERT INTO profile_client_assignments (
                tenant_id, assignment_id, profile_id, client_id, assigned_by_actor_id,
                assigned_at_ms, closed_at_ms, reason
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                "tenant_p1_fixture", "assignment_detach_successor_p1_fixture", "profile_p1_fixture",
                "client_b_p1_fixture", "actor_owner_p1_fixture", 30, None, "P1 negative proof",
            ),
            "profile_assignment_not_governed",
            "DETACH authority cannot insert a successor relationship",
            errors,
        )

        # Once the relationship is resolved, the existing Client archive command is valid again.
        database.execute(
            client_lifecycle_insert,
            (
                "tenant_p1_fixture",
                "command_archive_after_detach_p1_fixture",
                "actor_owner_p1_fixture",
                "client_b_p1_fixture",
                "ARCHIVE",
                1,
                None,
                35,
            ),
        )
        if client_state(database, "client_b_p1_fixture") != ("ARCHIVED", 2):
            errors.append("Client archive did not resume after the active Profile relationship was detached")

        # A completed command cannot be reused as durable authority for a later direct history write.
        expect_integrity_failure(
            database,
            "UPDATE profile_client_assignments SET closed_at_ms = ? WHERE tenant_id = ? AND assignment_id = ?",
            (20, "tenant_p1_fixture", "assignment_b_p1_fixture"),
            "profile_assignment_closed_history_immutable",
            "closed history mutation",
            errors,
        )

        insert_relationship_command(
            database,
            command_id="command_attach_again_p1_fixture",
            assignment_id="assignment_c_p1_fixture",
            client_id="client_a_p1_fixture",
            expected_version=4,
            executed_at_ms=40,
            operation="ASSIGN",
        )
        expect_integrity_failure(
            database,
            "UPDATE profile_client_assignments SET closed_at_ms = ? WHERE tenant_id = ? AND assignment_id = ?",
            (40, "tenant_p1_fixture", "assignment_c_p1_fixture"),
            "profile_assignment_close_not_governed",
            "consumed ASSIGN cannot become standalone DETACH authority",
            errors,
        )
        if relationship_rows(database)[-1] != (
            "assignment_c_p1_fixture",
            "client_a_p1_fixture",
            40,
            None,
        ) or profile_version(database) != 5:
            errors.append("history-guard negative proof corrupted the active relationship")
    except (AssertionError, OSError, sqlite3.DatabaseError) as error:
        errors.append(f"P1 governed relationship D1 behavior proof failed: {error}")
    finally:
        database.close()


def main() -> int:
    identity = IDENTITY_ADAPTER.read_text(encoding="utf-8")
    governance_adapter = IDENTITY_GOVERNANCE_ADAPTER.read_text(encoding="utf-8")
    ceremony_adapter = IDENTITY_CEREMONY_ADAPTER.read_text(encoding="utf-8")
    governance_use_cases = IDENTITY_GOVERNANCE_USE_CASES.read_text(encoding="utf-8")
    ceremony_use_cases = IDENTITY_CEREMONY_USE_CASES.read_text(encoding="utf-8")
    command_identity = COMMAND_IDENTITY.read_text(encoding="utf-8")
    governed = GOVERNED_ADAPTER.read_text(encoding="utf-8")
    generation = GENERATION_ADAPTER.read_text(encoding="utf-8")
    acceptance = INVITATION_ADAPTER.read_text(encoding="utf-8")
    client_adapter = CLIENT_ADAPTER.read_text(encoding="utf-8")
    client_use_cases = CLIENT_USE_CASES.read_text(encoding="utf-8")
    client_grant_use_cases = CLIENT_GRANT_USE_CASES.read_text(encoding="utf-8")
    profile_adapter = PROFILE_ADAPTER.read_text(encoding="utf-8")
    profile_use_cases = PROFILE_USE_CASES.read_text(encoding="utf-8")
    profile_assignment_use_cases = PROFILE_ASSIGNMENT_USE_CASES.read_text(encoding="utf-8")
    profile_grant_use_cases = PROFILE_GRANT_USE_CASES.read_text(encoding="utf-8")
    errors: list[str] = []

    for token in LEGACY_WRITE_TOKENS:
        if token in identity:
            errors.append(f"legacy direct mutation token remains in d1_identity_acl.rs: {token}")
    require_tokens(governance_use_cases, REQUIRED_IDENTITY_GOVERNANCE_APPLICATION_TOKENS, "identity governance application orchestration", errors)
    require_tokens(ceremony_use_cases, REQUIRED_IDENTITY_CEREMONY_APPLICATION_TOKENS, "identity ceremony application orchestration", errors)
    require_tokens(governance_adapter, REQUIRED_IDENTITY_GOVERNANCE_ADAPTER_TOKENS, "identity governance D1 adapter", errors)
    require_tokens(ceremony_adapter, REQUIRED_IDENTITY_CEREMONY_ADAPTER_TOKENS, "identity ceremony D1 adapter", errors)
    require_tokens(client_use_cases, REQUIRED_CLIENT_APPLICATION_TOKENS, "client application orchestration", errors)
    require_tokens(client_grant_use_cases, REQUIRED_CLIENT_GRANT_APPLICATION_TOKENS, "client grant orchestration", errors)
    require_tokens(client_adapter, REQUIRED_CLIENT_ADAPTER_TOKENS, "client D1 adapter", errors)
    require_tokens(profile_use_cases, REQUIRED_PROFILE_APPLICATION_TOKENS, "profile application orchestration", errors)
    require_tokens(profile_assignment_use_cases, REQUIRED_PROFILE_ASSIGNMENT_APPLICATION_TOKENS, "profile assignment orchestration", errors)
    require_tokens(profile_grant_use_cases, REQUIRED_PROFILE_GRANT_APPLICATION_TOKENS, "profile grant orchestration", errors)
    require_tokens(profile_adapter, REQUIRED_PROFILE_ADAPTER_TOKENS, "profile D1 adapter", errors)
    require_tokens(governed, REQUIRED_GOVERNED_TOKENS, "governed command adapter", errors)
    require_tokens(acceptance, REQUIRED_ACCEPTANCE_TOKENS, "invitation acceptance adapter", errors)

    if "part-crm:d1-command-journal:v1" not in command_identity:
        errors.append("D1 command journal IDs are missing their domain-separated identity tag")
    if governed.count("let command_id = command_journal_id(") != 7 or governed.count("command_id.as_str(),") != 7:
        errors.append("legacy governed command tables must use exactly seven actor-bound journal IDs")
    if generation.count("let command_id = command_journal_id(") != 5 or generation.count("command_id.as_str(),") != 5:
        errors.append("profile generation command tables must use exactly five actor-bound journal IDs")

    validate_profile_relationship_d1_behavior(errors)

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    if error_taxonomy_main() != 0:
        return 1
    print("Step 4 writes preserve application-owned authorization/replay sequencing and governed atomic adapters.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
