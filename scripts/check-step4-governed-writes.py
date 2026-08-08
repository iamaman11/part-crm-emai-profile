#!/usr/bin/env python3
"""Require Step 4 lifecycle and ACL writes to use governed command adapters."""

from __future__ import annotations

import sys
from pathlib import Path

from test_step4_error_taxonomy import main as error_taxonomy_main

ROOT = Path(__file__).resolve().parents[1]
IDENTITY_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_identity_acl.rs"
COMMAND_IDENTITY = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_command_identity.rs"
GOVERNED_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_governed_commands.rs"
GENERATION_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_profile_generations.rs"
INVITATION_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_invitation_acceptance.rs"
CLIENT_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_clients.rs"
CLIENT_USE_CASES = ROOT / "crates" / "use-cases" / "src" / "clients.rs"
CLIENT_GRANT_USE_CASES = ROOT / "crates" / "use-cases" / "src" / "client_grants.rs"
PROFILE_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_profiles.rs"
PROFILE_USE_CASES = ROOT / "crates" / "use-cases" / "src" / "profiles.rs"
PROFILE_ASSIGNMENT_USE_CASES = ROOT / "crates" / "use-cases" / "src" / "profile_assignments.rs"
PROFILE_GRANT_USE_CASES = ROOT / "crates" / "use-cases" / "src" / "profile_grants.rs"
WORKER_API = ROOT / "apps" / "control-plane-worker" / "src" / "api.rs"

LEGACY_WRITE_TOKENS = (
    "OWNER_TRANSFER_DEMOTE",
    "OWNER_TRANSFER_PROMOTE",
    "INVITATION_ACCEPT_MEMBERSHIP",
    "MEMBERSHIP_STATUS_UPDATE",
    "PROFILE_VERSION_CAS",
    "PROFILE_GRANT_UPSERT",
    "CLIENT_GRANT_UPSERT",
    "pub async fn transfer_owner(",
    "pub async fn update_membership_status(",
    "pub async fn grant_profile(",
    "pub async fn grant_client(",
)

LEGACY_WORKER_MUTATION_TOKENS = (
    "prefixed_id(",
    ".idempotency_replay(",
    'const PROFILE_ASSIGN_COMMAND: &str = "profile.assign_client";',
    "AssignProfileMutation",
    "async fn assign_profile(",
    'const PROFILE_GRANT_COMMAND: &str = "profile.grant";',
    'const PROFILE_GRANT_REVOKE_COMMAND: &str = "profile.grant_revoke";',
    "ProfileGrantMutation",
    "ProfileGrantValue",
    "async fn update_profile_grant(",
    "struct ProfileGrantRequest",
    'const CLIENT_GRANT_COMMAND: &str = "client.grant";',
    'const CLIENT_GRANT_REVOKE_COMMAND: &str = "client.grant_revoke";',
    "ClientGrantMutation",
    "ClientGrantValue",
    "async fn update_client_grant(",
    "struct ClientGrantRequest",
)

REQUIRED_WORKER_MUTATION_TOKENS = (
    "D1IdempotencyRepository",
    "IdempotencyDecision",
    "audit_event_id(scope.tenant_id(), actor_id, &idempotency_key)",
    "outbox_event_id(scope.tenant_id(), actor_id, &idempotency_key)",
    "mutation_failure_or_replay",
    'const OWNER_BOOTSTRAP_COMMAND: &str = "tenant.owner_bootstrap";',
    'const OWNER_TRANSFER_COMMAND: &str = "membership.owner_transfer";',
    'const INVITATION_CREATE_COMMAND: &str = "invitation.create";',
    'const INVITATION_ACCEPT_COMMAND: &str = "invitation.accept";',
)

REQUIRED_CLIENT_APPLICATION_TOKENS = (
    'const CLIENT_CREATE_COMMAND: &str = "client.create";',
    "decide_replay",
    "create_client",
    "ClientReplayDecision::Conflict",
)

REQUIRED_CLIENT_GRANT_APPLICATION_TOKENS = (
    'const CLIENT_GRANT_COMMAND: &str = "client.grant";',
    'const CLIENT_GRANT_REVOKE_COMMAND: &str = "client.grant_revoke";',
    "decide_client_grant_replay",
    "port.grant_client(actor, &write)",
    "port.revoke_client_grant(actor, &write)",
    "ClientGrantPortErrorClass::Conflict",
)

REQUIRED_CLIENT_ADAPTER_TOKENS = (
    "CreateClientMutation",
    "requested_display_name",
    "create_client(actor, mutation)",
    "ClientGrantMutation",
    "MutationEnvelope",
    ".grant_client(actor, mutation)",
    ".revoke_client_grant(actor, mutation)",
)

REQUIRED_PROFILE_APPLICATION_TOKENS = (
    'const PROFILE_CREATE_COMMAND: &str = "profile.create";',
    "decide_replay",
    "create_profile",
    "ProfileReplayDecision::Conflict",
)

REQUIRED_PROFILE_ASSIGNMENT_APPLICATION_TOKENS = (
    'const PROFILE_ASSIGN_COMMAND: &str = "profile.assign_client";',
    "decide_assignment_replay",
    "assign_profile(actor, &write)",
    "ProfileAssignmentPortErrorClass::Conflict",
)

REQUIRED_PROFILE_GRANT_APPLICATION_TOKENS = (
    'const PROFILE_GRANT_COMMAND: &str = "profile.grant";',
    'const PROFILE_GRANT_REVOKE_COMMAND: &str = "profile.grant_revoke";',
    "decide_profile_grant_replay",
    "port.grant_profile(actor, &write)",
    "port.revoke_profile_grant(actor, &write)",
    "ProfileGrantPortErrorClass::Conflict",
)

REQUIRED_PROFILE_ADAPTER_TOKENS = (
    "CreateProfileMutation",
    "AssignProfileMutation",
    "ProfileGrantMutation",
    "MutationEnvelope",
    "create_profile(actor, mutation)",
    ".assign_profile(actor, mutation)",
    ".grant_profile(actor, mutation)",
    ".revoke_profile_grant(actor, mutation)",
)

REQUIRED_GOVERNED_TOKENS = (
    "owner_transfer_commands",
    "membership_status_commands",
    "invitation_create_commands",
    "profile_create_commands",
    "profile_assignment_commands",
    "profile_grant_commands",
    "client_grant_commands",
    "self.database.batch(statements).await",
)

REQUIRED_ACCEPTANCE_TOKENS = (
    "invitation_acceptances",
    "INSERT INTO memberships",
    "INSERT INTO idempotency_records",
    "INSERT INTO audit_events",
    "INSERT INTO outbox_events",
    "self.database.batch(statements).await",
)


def main() -> int:
    identity = IDENTITY_ADAPTER.read_text(encoding="utf-8")
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
    worker_api = WORKER_API.read_text(encoding="utf-8")

    errors: list[str] = []
    for token in LEGACY_WRITE_TOKENS:
        if token in identity:
            errors.append(f"legacy direct mutation token remains in d1_identity_acl.rs: {token}")
    for token in LEGACY_WORKER_MUTATION_TOKENS:
        if token in worker_api:
            errors.append(f"legacy governed Worker mutation token remains in api.rs: {token}")
    for token in REQUIRED_WORKER_MUTATION_TOKENS:
        if token not in worker_api:
            errors.append(f"governed Worker mutation envelope is missing required token: {token}")
    for token in REQUIRED_CLIENT_APPLICATION_TOKENS:
        if token not in client_use_cases:
            errors.append(f"client application orchestration is missing governed token: {token}")
    for token in REQUIRED_CLIENT_GRANT_APPLICATION_TOKENS:
        if token not in client_grant_use_cases:
            errors.append(f"client grant orchestration is missing governed token: {token}")
    for token in REQUIRED_CLIENT_ADAPTER_TOKENS:
        if token not in client_adapter:
            errors.append(f"client D1 adapter is missing atomic mutation token: {token}")
    for token in REQUIRED_PROFILE_APPLICATION_TOKENS:
        if token not in profile_use_cases:
            errors.append(f"profile application orchestration is missing governed token: {token}")
    for token in REQUIRED_PROFILE_ASSIGNMENT_APPLICATION_TOKENS:
        if token not in profile_assignment_use_cases:
            errors.append(f"profile assignment orchestration is missing governed token: {token}")
    for token in REQUIRED_PROFILE_GRANT_APPLICATION_TOKENS:
        if token not in profile_grant_use_cases:
            errors.append(f"profile grant orchestration is missing governed token: {token}")
    for token in REQUIRED_PROFILE_ADAPTER_TOKENS:
        if token not in profile_adapter:
            errors.append(f"profile D1 adapter is missing atomic mutation token: {token}")
    for token in REQUIRED_GOVERNED_TOKENS:
        if token not in governed:
            errors.append(f"governed command adapter is missing required token: {token}")
    for token in REQUIRED_ACCEPTANCE_TOKENS:
        if token not in acceptance:
            errors.append(f"invitation acceptance adapter is missing atomic envelope token: {token}")

    if "part-crm:d1-command-journal:v1" not in command_identity:
        errors.append("D1 command journal IDs are missing their domain-separated identity tag")
    if governed.count("let command_id = command_journal_id(") != 7:
        errors.append("legacy governed command tables must derive exactly seven actor-bound journal IDs")
    if governed.count("command_id.as_str(),") != 7:
        errors.append("legacy governed command inserts must use actor-bound journal IDs")
    if generation.count("let command_id = command_journal_id(") != 5:
        errors.append("profile generation command tables must derive exactly five actor-bound journal IDs")
    if generation.count("command_id.as_str(),") != 5:
        errors.append("profile generation command inserts must use actor-bound journal IDs")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    if error_taxonomy_main() != 0:
        return 1

    print(
        "Step 4 writes use governed atomic envelopes, exact replay decisions, "
        "actor-bound command/evidence identifiers, and stable error taxonomy."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
