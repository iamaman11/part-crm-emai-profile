#!/usr/bin/env python3
"""Fail closed if accepted Phase 2C client merge/assignment boundaries regress."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

MIGRATION = Path("migrations/d1/0015_client_merge_assignment_history.sql")
MERGE_DOMAIN = Path("crates/client-domain/src/merge.rs")
ASSIGNMENT_DOMAIN = Path("crates/client-domain/src/assignment.rs")
MERGE_PORT = Path("crates/application-ports/src/client_merge.rs")
MERGE_USE_CASE = Path("crates/use-cases-clients/src/merge.rs")
MERGE_ADAPTER = Path("crates/cloudflare-adapters/src/d1_client_merge.rs")
IDENTITY_ACL = Path("crates/use-cases/src/identity_acl.rs")
D1_PROOF = Path("scripts/test-phase2c-client-merge-assignment.py")

REQUIRED_MIGRATION = (
    "CREATE TABLE client_merges",
    "CREATE TABLE client_merge_commands",
    "client_merge_owner_required",
    "client_merge_source_version_or_state_mismatch",
    "client_merge_target_version_or_state_mismatch",
    "client_merge_active_assignment_requires_reassignment",
    "client_merge_status_requires_command",
    "client_merged_source_cannot_resurrect",
    "client_merge_record_immutable_update",
    "client_merge_record_immutable_delete",
    "client_merge_command_apply",
    "UPDATE client_contact_points",
    "DELETE FROM client_grants",
    "profile_assignment_same_client",
    "profile_assignment_history_insert_guard",
    "profile_assignment_identity_immutable",
    "profile_assignment_closed_history_immutable",
    "profile_assignment_delete_forbidden",
)
REQUIRED_MERGE_DOMAIN = (
    "pub struct ClientMergePlan",
    "pub fn merge_clients",
    "ClientMergeError::TenantMismatch",
    "ClientMergeError::SelfMerge",
    "ClientMergeError::SourceVersionConflict",
    "ClientMergeError::TargetVersionConflict",
    "ClientMergeError::SourceAlreadyMerged",
    "ClientMergeError::MergeCycle",
    "source.mark_merged()",
)
REQUIRED_ASSIGNMENT_DOMAIN = (
    "AssignmentId",
    "AssignmentRole::Primary",
    "pub struct PrimaryAssignmentTransition",
    "pub fn plan_primary_reassignment",
    "AssignmentError::CurrentScopeMismatch",
    "AssignmentError::CurrentNotActivePrimary",
    "AssignmentError::AlreadyPrimaryClient",
)
REQUIRED_PORT = (
    "pub trait ClientMergeApplicationPort",
    "load_client_for_merge",
    "source_has_active_assignment",
    "decide_client_merge_replay",
    "persist_client_merge",
)
REQUIRED_USE_CASE = (
    "pub async fn execute_merge_client",
    "authorize_merge(role)?",
    "decide_replay(actor, port",
    "source_has_active_assignment",
    "merge_clients(",
    "persist_client_merge",
    "decide_client_merge_replay",
)
REQUIRED_ADAPTER = (
    "impl ClientMergeApplicationPort for D1ClientMergeRepository",
    "INSERT INTO client_merge_commands",
    "INSERT INTO idempotency_records",
    "INSERT INTO audit_events",
    "INSERT INTO outbox_events",
    "self.database.batch(statements)",
)
FORBIDDEN_MIGRATION = (
    "INSERT INTO client_grants",
    "UPDATE client_grants SET client_id",
)
FORBIDDEN_ADAPTER = (
    "UPDATE clients",
    "UPDATE client_contact_points",
    "DELETE FROM client_grants",
    "INSERT INTO client_merges",
)
FORBIDDEN_MERGE_DOMAIN = (
    "identity_access_domain",
    "Membership",
    "ClientGrant",
    "ProfileGrant",
)


def read(root: Path, path: Path) -> str:
    target = root / path
    if not target.is_file():
        return ""
    return target.read_text(encoding="utf-8")


def require_markers(errors: list[str], source: str, markers: tuple[str, ...], label: str) -> None:
    for marker in markers:
        if marker not in source:
            errors.append(f"{label} missing required marker `{marker}`")


def reject_markers(errors: list[str], source: str, markers: tuple[str, ...], label: str) -> None:
    for marker in markers:
        if marker in source:
            errors.append(f"{label} must not contain `{marker}`")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    migration = read(root, MIGRATION)
    merge_domain = read(root, MERGE_DOMAIN)
    assignment_domain = read(root, ASSIGNMENT_DOMAIN)
    merge_port = read(root, MERGE_PORT)
    merge_use_case = read(root, MERGE_USE_CASE)
    merge_adapter = read(root, MERGE_ADAPTER)
    identity_acl = read(root, IDENTITY_ACL)
    d1_proof = read(root, D1_PROOF)

    for path, source in (
        (MIGRATION, migration),
        (MERGE_DOMAIN, merge_domain),
        (ASSIGNMENT_DOMAIN, assignment_domain),
        (MERGE_PORT, merge_port),
        (MERGE_USE_CASE, merge_use_case),
        (MERGE_ADAPTER, merge_adapter),
        (IDENTITY_ACL, identity_acl),
        (D1_PROOF, d1_proof),
    ):
        if not source.strip():
            errors.append(f"missing Phase 2C boundary source: {path}")

    require_markers(errors, migration, REQUIRED_MIGRATION, "Phase 2C migration")
    reject_markers(errors, migration, FORBIDDEN_MIGRATION, "Phase 2C migration")
    require_markers(errors, merge_domain, REQUIRED_MERGE_DOMAIN, "merge domain")
    reject_markers(errors, merge_domain, FORBIDDEN_MERGE_DOMAIN, "merge domain")
    require_markers(errors, assignment_domain, REQUIRED_ASSIGNMENT_DOMAIN, "assignment domain")
    require_markers(errors, merge_port, REQUIRED_PORT, "merge application port")
    require_markers(errors, merge_use_case, REQUIRED_USE_CASE, "merge use case")
    require_markers(errors, merge_adapter, REQUIRED_ADAPTER, "merge D1 adapter")
    reject_markers(errors, merge_adapter, FORBIDDEN_ADAPTER, "merge D1 adapter")

    if "assignment_never_grants_client_or_profile_access" not in identity_acl:
        errors.append("assignment-as-non-ACL regression proof is missing")
    if "query_profile(" not in identity_acl or "query_client(" not in identity_acl:
        errors.append("neutral grant-safe client/profile query boundary is missing")

    for marker in (
        "test_assignment_history_is_one_active_primary_and_one_way",
        "test_merge_requires_assignment_reassignment_then_closes_source_capabilities",
        "test_merge_version_failure_and_downstream_failure_roll_back_everything",
    ):
        if marker not in d1_proof:
            errors.append(f"Phase 2C D1 proof missing `{marker}`")

    # Sequencing matters: authorization/replay must precede reads and writes.
    try:
        authorize_at = merge_use_case.index("authorize_merge(role)?")
        replay_at = merge_use_case.index("decide_replay(actor, port")
        load_at = merge_use_case.index("load_client_for_merge")
        write_at = merge_use_case.index("persist_client_merge")
        if not (authorize_at < replay_at < load_at < write_at):
            errors.append("merge use case must sequence auth -> replay -> load -> governed write")
    except ValueError:
        pass

    # D1 command trigger is the canonical mutator; adapter only emits command + evidence.
    if "source_client_id = NEW.source_client_id" not in migration:
        errors.append("merge trigger must scope source mutations to the commanded source client")
    if "client_id = NEW.source_client_id" not in migration:
        errors.append("merge trigger must scope capability cleanup to the commanded source client")
    if "target_client_id" not in migration:
        errors.append("merge history must retain the explicit target client")

    return errors


def write_fixture(root: Path) -> None:
    for path in (
        MIGRATION,
        MERGE_DOMAIN,
        ASSIGNMENT_DOMAIN,
        MERGE_PORT,
        MERGE_USE_CASE,
        MERGE_ADAPTER,
        IDENTITY_ACL,
        D1_PROOF,
    ):
        (root / path).parent.mkdir(parents=True, exist_ok=True)

    (root / MIGRATION).write_text(
        "\n".join(REQUIRED_MIGRATION)
        + "\nsource_client_id = NEW.source_client_id\nclient_id = NEW.source_client_id\ntarget_client_id\n",
        encoding="utf-8",
    )
    (root / MERGE_DOMAIN).write_text("\n".join(REQUIRED_MERGE_DOMAIN), encoding="utf-8")
    (root / ASSIGNMENT_DOMAIN).write_text("\n".join(REQUIRED_ASSIGNMENT_DOMAIN), encoding="utf-8")
    (root / MERGE_PORT).write_text("\n".join(REQUIRED_PORT), encoding="utf-8")
    (root / MERGE_USE_CASE).write_text(
        "\n".join(
            (
                "authorize_merge(role)?",
                "decide_replay(actor, port",
                "load_client_for_merge",
                "source_has_active_assignment",
                "merge_clients(",
                "persist_client_merge",
                "decide_client_merge_replay",
                "pub async fn execute_merge_client",
            )
        ),
        encoding="utf-8",
    )
    (root / MERGE_ADAPTER).write_text("\n".join(REQUIRED_ADAPTER), encoding="utf-8")
    (root / IDENTITY_ACL).write_text(
        "assignment_never_grants_client_or_profile_access\nquery_profile(\nquery_client(\n",
        encoding="utf-8",
    )
    (root / D1_PROOF).write_text(
        "test_assignment_history_is_one_active_primary_and_one_way\n"
        "test_merge_requires_assignment_reassignment_then_closes_source_capabilities\n"
        "test_merge_version_failure_and_downstream_failure_roll_back_everything\n",
        encoding="utf-8",
    )


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="phase2c-client-boundary-") as temp:
        root = Path(temp)
        write_fixture(root)
        baseline = validate(root)
        if baseline:
            print("invalid Phase 2C boundary fixture baseline")
            print("\n".join(baseline))
            return 1

        write_fixture(root)
        path = root / MIGRATION
        path.write_text(
            read(root, MIGRATION).replace(
                "client_merge_active_assignment_requires_reassignment", "guard_removed", 1
            ),
            encoding="utf-8",
        )
        if not any("active_assignment" in error for error in validate(root)):
            print("missing active-assignment merge guard fixture unexpectedly passed")
            return 1

        write_fixture(root)
        path = root / MIGRATION
        path.write_text(read(root, MIGRATION) + "\nINSERT INTO client_grants\n", encoding="utf-8")
        if not any("INSERT INTO client_grants" in error for error in validate(root)):
            print("grant-transfer merge fixture unexpectedly passed")
            return 1

        write_fixture(root)
        path = root / MIGRATION
        path.write_text(
            read(root, MIGRATION).replace("client_merged_source_cannot_resurrect", "removed", 1),
            encoding="utf-8",
        )
        if not any("cannot_resurrect" in error for error in validate(root)):
            print("merge resurrection fixture unexpectedly passed")
            return 1

        write_fixture(root)
        path = root / MERGE_ADAPTER
        path.write_text(read(root, MERGE_ADAPTER) + "\nUPDATE clients\n", encoding="utf-8")
        if not any("UPDATE clients" in error for error in validate(root)):
            print("raw merge mutation adapter fixture unexpectedly passed")
            return 1

        write_fixture(root)
        path = root / MERGE_DOMAIN
        path.write_text(read(root, MERGE_DOMAIN) + "\nClientGrant\n", encoding="utf-8")
        if not any("ClientGrant" in error for error in validate(root)):
            print("grant-aware merge domain fixture unexpectedly passed")
            return 1

    print("Phase 2C merge/assignment negative fixtures rejected as expected.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    errors = validate(args.root.resolve())
    if errors:
        print("\n".join(errors))
        return 1
    print("Phase 2C client merge/assignment boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
