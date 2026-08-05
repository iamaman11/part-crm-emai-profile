#!/usr/bin/env python3
"""Require Step 4 lifecycle and ACL writes to use governed command adapters."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IDENTITY_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_identity_acl.rs"
GOVERNED_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_governed_commands.rs"
INVITATION_ADAPTER = ROOT / "crates" / "cloudflare-adapters" / "src" / "d1_invitation_acceptance.rs"

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
    governed = GOVERNED_ADAPTER.read_text(encoding="utf-8")
    acceptance = INVITATION_ADAPTER.read_text(encoding="utf-8")

    errors: list[str] = []
    for token in LEGACY_WRITE_TOKENS:
        if token in identity:
            errors.append(f"legacy direct mutation token remains in d1_identity_acl.rs: {token}")
    for token in REQUIRED_GOVERNED_TOKENS:
        if token not in governed:
            errors.append(f"governed command adapter is missing required token: {token}")
    for token in REQUIRED_ACCEPTANCE_TOKENS:
        if token not in acceptance:
            errors.append(f"invitation acceptance adapter is missing atomic envelope token: {token}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Step 4 writes are confined to governed atomic adapter envelopes.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
