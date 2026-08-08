#!/usr/bin/env python3
"""Fail closed if capability ownership or extracted application Cargo boundaries regress."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

PORT_MODULES = (
    "audit", "clients", "clock", "commands", "coordinator_ingress", "generations",
    "identity", "identity_ceremonies", "identity_governance", "mailbox_jobs", "mailboxes",
    "profiles", "sessions",
)
MONOLITH_USE_CASE_MODULES = (
    "client_grants", "clients", "coordinator_ingress", "error", "generations", "identity_acl",
    "mailbox_jobs", "mailboxes", "profile_assignments", "profile_grants", "profiles",
)

PORT_OWNERS = {
    "audit.rs": ("pub trait AuditPort", "pub struct AuditRecord", "pub enum AuditResult"),
    "clients.rs": ("pub trait ClientRepository", "pub trait ClientApplicationPort", "pub struct ClientCreateWrite", "pub trait ClientGrantApplicationPort", "pub struct ClientGrantWrite"),
    "clock.rs": ("pub trait ClockPort",),
    "commands.rs": ("pub struct CommandExecutionEvidence",),
    "coordinator_ingress.rs": ("pub trait CoordinatorIngressApplicationPort", "pub struct CoordinatorProfileAccess", "pub struct CoordinatorRuntimeResult"),
    "generations.rs": ("pub struct GenerationObjectReference", "pub trait GenerationObjectStorePort", "pub trait GenerationApplicationPort", "pub struct GenerationReadModel", "pub struct RegisterGenerationWrite"),
    "identity.rs": ("pub trait MembershipRepository",),
    "identity_ceremonies.rs": ("pub trait IdentityCeremonyApplicationPort", "pub struct VerifiedIdentitySnapshot", "pub struct VerifiedIdentityCeremonyContext", "pub struct BootstrapOwnerWrite", "pub struct InvitationAcceptWrite"),
    "identity_governance.rs": ("pub trait ActiveOwnerGovernanceApplicationPort", "pub struct OwnerTransferWrite", "pub struct InvitationCreateWrite", "pub struct MembershipStatusWrite"),
    "mailbox_jobs.rs": ("pub trait MailboxJobApplicationPort", "pub struct MailboxJobCreateWrite", "pub struct MailboxJobPreparedRun"),
    "mailboxes.rs": ("pub struct MailboxObservation", "pub trait MailboxProviderPort"),
    "profiles.rs": ("pub trait ProfileRepository", "pub trait ProfileApplicationPort", "pub struct ProfileCreateWrite", "pub trait ProfileAssignmentApplicationPort", "pub struct ProfileAssignmentWrite", "pub trait ProfileGrantApplicationPort", "pub struct ProfileGrantWrite"),
    "sessions.rs": ("pub trait ProfileCoordinatorPort",),
}

MONOLITH_USE_CASE_OWNERS = {
    "client_grants.rs": ("pub struct ExecuteClientGrantCommand", "pub async fn execute_client_grant", "pub fn authorize_client_grant", "pub fn next_client_grant_version"),
    "clients.rs": ("pub struct CreateClientCommand", "pub fn decide_create_client", "pub struct ExecuteCreateClientCommand", "pub async fn execute_create_client", "pub async fn get_visible_client"),
    "coordinator_ingress.rs": ("pub struct CoordinatorIngressAccess", "pub async fn prepare_coordinator_ingress", "pub async fn execute_prepared_coordinator_ingress"),
    "error.rs": ("pub struct ApplicationError",),
    "generations.rs": ("pub async fn execute_register_generation", "pub async fn get_visible_generation", "pub async fn execute_verify_generation", "pub async fn execute_activate_generation", "pub async fn execute_deactivate_generation", "pub async fn execute_quarantine_generation"),
    "mailbox_jobs.rs": ("pub async fn execute_create_mailbox_job", "pub async fn get_mailbox_job", "pub async fn execute_run_mailbox_job", "pub fn validate_create_mailbox_job_request", "pub fn validate_mailbox_job_run_version"),
    "profile_assignments.rs": ("pub struct ExecuteAssignProfileCommand", "pub async fn execute_assign_profile", "pub fn authorize_profile_assignment", "pub fn next_profile_assignment_version"),
    "profile_grants.rs": ("pub struct ExecuteProfileGrantCommand", "pub async fn execute_profile_grant", "pub fn authorize_profile_grant", "pub fn next_profile_grant_version"),
    "profiles.rs": ("pub struct OpenProfileCommand", "pub fn decide_open_profile", "pub struct ExecuteCreateProfileCommand", "pub async fn execute_create_profile", "pub async fn get_visible_profile"),
}

IDENTITY_USE_CASE_OWNERS = {
    "identity_ceremonies.rs": ("pub struct ExecuteOwnerBootstrapCommand", "pub struct ExecuteInvitationAcceptCommand", "pub async fn execute_owner_bootstrap", "pub async fn execute_invitation_accept"),
    "identity_governance.rs": ("pub struct ExecuteOwnerTransferCommand", "pub struct ExecuteInvitationCreateCommand", "pub struct ExecuteMembershipStatusCommand", "pub async fn execute_owner_transfer", "pub async fn execute_invitation_create", "pub async fn execute_membership_status", "pub fn authorize_identity_governance"),
}


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    ports_dir = root / "crates/application-ports/src"
    mono_dir = root / "crates/use-cases/src"
    identity_dir = root / "crates/use-cases-identity/src"
    ports_lib = read(ports_dir / "lib.rs") if (ports_dir / "lib.rs").is_file() else ""
    mono_lib = read(mono_dir / "lib.rs") if (mono_dir / "lib.rs").is_file() else ""
    identity_lib = read(identity_dir / "lib.rs") if (identity_dir / "lib.rs").is_file() else ""

    for module in PORT_MODULES:
        declaration = f"pub mod {module};"
        path = ports_dir / f"{module}.rs"
        if declaration not in ports_lib:
            errors.append(f"application-ports facade missing `{declaration}`")
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing/non-empty application-ports module: {path.relative_to(root)}")

    for module in MONOLITH_USE_CASE_MODULES:
        declaration = f"pub mod {module};"
        path = mono_dir / f"{module}.rs"
        if declaration not in mono_lib:
            errors.append(f"use-cases facade missing `{declaration}`")
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing/non-empty monolith use-case module: {path.relative_to(root)}")

    for extracted in ("identity_ceremonies", "identity_governance"):
        if f"pub mod {extracted};" in mono_lib or (mono_dir / f"{extracted}.rs").exists():
            errors.append(f"extracted identity owner returned to monolithic use-cases: {extracted}")
        if f"pub mod {extracted};" not in identity_lib:
            errors.append(f"use-cases-identity facade missing `pub mod {extracted};`")
        path = identity_dir / f"{extracted}.rs"
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing extracted identity module: {path.relative_to(root)}")

    compatibility = "pub use use_cases_identity::{identity_ceremonies, identity_governance};"
    if compatibility not in mono_lib:
        errors.append("monolithic compatibility facade must explicitly re-export use-cases-identity")

    for filename, symbols in PORT_OWNERS.items():
        owner = read(ports_dir / filename) if (ports_dir / filename).is_file() else ""
        for symbol in symbols:
            if symbol not in owner:
                errors.append(f"{filename} must own `{symbol}`")
            if symbol in ports_lib:
                errors.append(f"application-ports facade must not own `{symbol}`")

    for filename, symbols in MONOLITH_USE_CASE_OWNERS.items():
        owner = read(mono_dir / filename) if (mono_dir / filename).is_file() else ""
        for symbol in symbols:
            if symbol not in owner:
                errors.append(f"{filename} must own `{symbol}`")
            if symbol in mono_lib:
                errors.append(f"use-cases facade must not own `{symbol}`")

    for filename, symbols in IDENTITY_USE_CASE_OWNERS.items():
        owner = read(identity_dir / filename) if (identity_dir / filename).is_file() else ""
        for symbol in symbols:
            if symbol not in owner:
                errors.append(f"use-cases-identity/{filename} must own `{symbol}`")
            if symbol in identity_lib or symbol in mono_lib:
                errors.append(f"identity application facade must not implement `{symbol}`")

    required_port_reexports = (
        "pub use audit::{AuditPort, AuditRecord, AuditResult};",
        "pub use clients::ClientRepository;",
        "pub use clock::ClockPort;",
        "pub use commands::CommandExecutionEvidence;",
        "pub use generations::{GenerationObjectReference, GenerationObjectStorePort};",
        "pub use identity::MembershipRepository;",
        "pub use mailboxes::{MailboxObservation, MailboxProviderPort};",
        "pub use profiles::ProfileRepository;",
        "pub use sessions::ProfileCoordinatorPort;",
    )
    required_mono_reexports = (
        "pub use clients::{CreateClientCommand, decide_create_client};",
        "pub use error::ApplicationError;",
        "pub use profiles::{OpenProfileCommand, OpenProfileDecision, decide_open_profile};",
    )
    for line in required_port_reexports:
        if line not in ports_lib:
            errors.append(f"application-ports facade missing compatibility re-export `{line}`")
    for line in required_mono_reexports:
        if line not in mono_lib:
            errors.append(f"use-cases facade missing compatibility re-export `{line}`")
    return errors


def write_self_test_fixture(root: Path) -> None:
    ports = root / "crates/application-ports/src"
    mono = root / "crates/use-cases/src"
    identity = root / "crates/use-cases-identity/src"
    ports.mkdir(parents=True)
    mono.mkdir(parents=True)
    identity.mkdir(parents=True)
    for module in PORT_MODULES:
        (ports / f"{module}.rs").write_text("// fixture\n", encoding="utf-8")
    for module in MONOLITH_USE_CASE_MODULES:
        (mono / f"{module}.rs").write_text("// fixture\n", encoding="utf-8")
    for module in IDENTITY_USE_CASE_OWNERS:
        (identity / module).write_text("// fixture\n", encoding="utf-8")
    (ports / "lib.rs").write_text("\n".join(f"pub mod {m};" for m in PORT_MODULES), encoding="utf-8")
    (identity / "lib.rs").write_text("pub mod identity_ceremonies;\npub mod identity_governance;\n", encoding="utf-8")
    # Deliberate regression: old owner returns to the monolith.
    (mono / "identity_governance.rs").write_text("// forbidden duplicate owner\n", encoding="utf-8")
    (mono / "lib.rs").write_text(
        "\n".join(f"pub mod {m};" for m in MONOLITH_USE_CASE_MODULES)
        + "\npub mod identity_governance;\n"
        + "pub use use_cases_identity::{identity_ceremonies, identity_governance};\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="capability-layout-") as temp_dir:
            fixture = Path(temp_dir)
            write_self_test_fixture(fixture)
            errors = validate(fixture)
            if not any("returned to monolithic" in error for error in errors):
                print("negative extracted-crate fixture unexpectedly passed")
                return 1
            print("negative extracted-crate fixture rejected as expected")
            return 0
    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("capability module and application crate layout: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
