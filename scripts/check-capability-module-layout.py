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
    "coordinator_ingress", "error", "generations", "identity_acl",
    "profile_assignments", "profile_grants", "profiles",
)
CLIENT_USE_CASE_MODULES = ("client_grants", "clients", "contacts", "error", "lifecycle")
IDENTITY_USE_CASE_MODULES = ("identity_ceremonies", "identity_governance")
MAILBOX_USE_CASE_MODULES = ("browser_execution", "mailbox_jobs", "mailboxes", "scheduled")

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
    "mailboxes.rs": ("pub trait MailboxProviderPort",),
    "profiles.rs": ("pub trait ProfileRepository", "pub trait ProfileApplicationPort", "pub struct ProfileCreateWrite", "pub trait ProfileAssignmentApplicationPort", "pub struct ProfileAssignmentWrite", "pub trait ProfileGrantApplicationPort", "pub struct ProfileGrantWrite"),
    "sessions.rs": ("pub trait ProfileCoordinatorPort",),
}

MONOLITH_USE_CASE_OWNERS = {
    "coordinator_ingress.rs": ("pub struct CoordinatorIngressAccess", "pub async fn prepare_coordinator_ingress", "pub async fn execute_prepared_coordinator_ingress"),
    "error.rs": ("pub struct ApplicationError",),
    "generations.rs": ("pub async fn execute_register_generation", "pub async fn get_visible_generation", "pub async fn execute_verify_generation", "pub async fn execute_activate_generation", "pub async fn execute_deactivate_generation", "pub async fn execute_quarantine_generation"),
    "profile_assignments.rs": ("pub struct ExecuteAssignProfileCommand", "pub async fn execute_assign_profile", "pub fn authorize_profile_assignment", "pub fn next_profile_assignment_version"),
    "profile_grants.rs": ("pub struct ExecuteProfileGrantCommand", "pub async fn execute_profile_grant", "pub fn authorize_profile_grant", "pub fn next_profile_grant_version"),
    "profiles.rs": ("pub struct OpenProfileCommand", "pub fn decide_open_profile", "pub struct ExecuteCreateProfileCommand", "pub async fn execute_create_profile", "pub async fn get_visible_profile"),
}

CLIENT_USE_CASE_OWNERS = {
    "clients.rs": ("pub struct CreateClientCommand", "pub fn decide_create_client", "pub struct ExecuteCreateClientCommand", "pub async fn execute_create_client", "pub async fn get_visible_client"),
    "client_grants.rs": ("pub struct ExecuteClientGrantCommand", "pub async fn execute_client_grant", "pub fn authorize_client_grant", "pub fn next_client_grant_version"),
    "contacts.rs": ("pub struct TransientContactValue", "pub async fn prepare_protected_contact", "pub fn authorize_contact_mutation"),
    "lifecycle.rs": ("pub struct UpdateClientCommand", "pub struct ArchiveClientCommand", "pub async fn execute_update_client", "pub async fn execute_archive_client", "pub fn authorize_client_lifecycle"),
}

IDENTITY_USE_CASE_OWNERS = {
    "identity_ceremonies.rs": ("pub struct ExecuteOwnerBootstrapCommand", "pub struct ExecuteInvitationAcceptCommand", "pub async fn execute_owner_bootstrap", "pub async fn execute_invitation_accept"),
    "identity_governance.rs": ("pub struct ExecuteOwnerTransferCommand", "pub struct ExecuteInvitationCreateCommand", "pub struct ExecuteMembershipStatusCommand", "pub async fn execute_owner_transfer", "pub async fn execute_invitation_create", "pub async fn execute_membership_status", "pub fn authorize_identity_governance"),
}

MAILBOX_USE_CASE_OWNERS = {
    "browser_execution.rs": (
        "pub struct BindBrowserMailboxExecutionCommand",
        "pub struct BrowserMailboxExecutionBindingOutcome",
        "pub async fn execute_bind_browser_mailbox_execution",
    ),
    "mailbox_jobs.rs": (
        "pub async fn execute_create_mailbox_job",
        "pub async fn get_mailbox_job",
        "pub async fn execute_run_mailbox_job",
        "pub fn validate_create_mailbox_job_request",
        "pub fn validate_mailbox_job_run_version",
    ),
    "mailboxes.rs": (
        "pub async fn execute_create_mailbox_binding",
        "pub async fn execute_revoke_mailbox_binding",
        "pub async fn get_mailbox_binding",
        "pub fn authorize_mailbox_binding",
    ),
    "scheduled.rs": (
        "pub struct ProcessScheduledMailboxJobRequest",
        "pub async fn dispatch_due_mailbox_jobs",
        "pub async fn process_scheduled_mailbox_job",
    ),
}

EXTRACTED_APPLICATION_DEPENDENCIES = (
    "use-cases-clients",
    "use-cases-identity",
    "use-cases-mailboxes",
)
FORBIDDEN_MONOLITH_COMPATIBILITY = (
    "use_cases_clients::",
    "use_cases_identity::",
    "use_cases_mailboxes::",
)
FORBIDDEN_CLIENT_COMPATIBILITY_FILES = ("clients.rs", "client_grants.rs")
FORBIDDEN_WORKER_COMPATIBILITY_PATHS = (
    "use_cases::identity_ceremonies",
    "use_cases::identity_governance",
    "use_cases::browser_execution",
    "use_cases::mailbox_jobs",
    "use_cases::mailboxes",
    "use_cases::scheduled",
)


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def validate_module_set(
    root: Path,
    directory: Path,
    facade: str,
    modules: tuple[str, ...],
    label: str,
    errors: list[str],
) -> None:
    for module in modules:
        declaration = f"pub mod {module};"
        path = directory / f"{module}.rs"
        if declaration not in facade:
            errors.append(f"{label} facade missing `{declaration}`")
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing/non-empty {label} module: {path.relative_to(root)}")


def validate_owners(
    owner_dir: Path,
    facade: str,
    owners: dict[str, tuple[str, ...]],
    owner_label: str,
    facade_label: str,
    errors: list[str],
) -> None:
    for filename, symbols in owners.items():
        source = read(owner_dir / filename) if (owner_dir / filename).is_file() else ""
        for symbol in symbols:
            if symbol not in source:
                errors.append(f"{owner_label}/{filename} must own `{symbol}`")
            if symbol in facade:
                errors.append(f"{facade_label} must not implement `{symbol}`")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    ports_dir = root / "crates/application-ports/src"
    mono_dir = root / "crates/use-cases/src"
    clients_dir = root / "crates/use-cases-clients/src"
    identity_dir = root / "crates/use-cases-identity/src"
    mailboxes_dir = root / "crates/use-cases-mailboxes/src"
    worker_dir = root / "apps/control-plane-worker/src"
    ports_lib = read(ports_dir / "lib.rs") if (ports_dir / "lib.rs").is_file() else ""
    mono_lib = read(mono_dir / "lib.rs") if (mono_dir / "lib.rs").is_file() else ""
    clients_lib = read(clients_dir / "lib.rs") if (clients_dir / "lib.rs").is_file() else ""
    identity_lib = read(identity_dir / "lib.rs") if (identity_dir / "lib.rs").is_file() else ""
    mailboxes_lib = read(mailboxes_dir / "lib.rs") if (mailboxes_dir / "lib.rs").is_file() else ""

    validate_module_set(root, ports_dir, ports_lib, PORT_MODULES, "application-ports", errors)
    validate_module_set(root, mono_dir, mono_lib, MONOLITH_USE_CASE_MODULES, "monolith use-case", errors)
    validate_module_set(root, clients_dir, clients_lib, CLIENT_USE_CASE_MODULES, "use-cases-clients", errors)
    validate_module_set(root, identity_dir, identity_lib, IDENTITY_USE_CASE_MODULES, "use-cases-identity", errors)
    validate_module_set(root, mailboxes_dir, mailboxes_lib, MAILBOX_USE_CASE_MODULES, "use-cases-mailboxes", errors)

    for module in IDENTITY_USE_CASE_MODULES:
        if f"pub mod {module};" in mono_lib or (mono_dir / f"{module}.rs").exists():
            errors.append(f"extracted identity owner returned to monolithic use-cases: {module}")
    for module in MAILBOX_USE_CASE_MODULES:
        if f"pub mod {module};" in mono_lib or (mono_dir / f"{module}.rs").exists():
            errors.append(f"extracted mailbox owner returned to monolithic use-cases: {module}")
    for filename in FORBIDDEN_CLIENT_COMPATIBILITY_FILES:
        if (mono_dir / filename).exists() or f"pub mod {filename[:-3]};" in mono_lib:
            errors.append(f"historical client compatibility facade is forbidden: {filename}")

    for path in mono_dir.glob("*.rs"):
        source = read(path)
        for marker in FORBIDDEN_MONOLITH_COMPATIBILITY:
            if marker in source:
                errors.append(
                    "historical application compatibility reference is forbidden in shared "
                    f"use-cases: {path.relative_to(root)}: {marker}"
                )

    mono_manifest_path = root / "crates/use-cases/Cargo.toml"
    mono_manifest = read(mono_manifest_path) if mono_manifest_path.is_file() else ""
    for dependency in EXTRACTED_APPLICATION_DEPENDENCIES:
        if f"{dependency}.workspace" in mono_manifest or f"{dependency} =" in mono_manifest:
            errors.append(f"shared use-cases must not depend on extracted application crate: {dependency}")

    if worker_dir.is_dir():
        for path in worker_dir.rglob("*.rs"):
            source = read(path)
            for marker in FORBIDDEN_WORKER_COMPATIBILITY_PATHS:
                if marker in source:
                    errors.append(
                        "Worker must import extracted application ownership directly: "
                        f"{path.relative_to(root)}: {marker}"
                    )

    for filename, symbols in PORT_OWNERS.items():
        owner = read(ports_dir / filename) if (ports_dir / filename).is_file() else ""
        for symbol in symbols:
            if symbol not in owner:
                errors.append(f"{filename} must own `{symbol}`")
            if symbol in ports_lib:
                errors.append(f"application-ports facade must not own `{symbol}`")

    validate_owners(mono_dir, mono_lib, MONOLITH_USE_CASE_OWNERS, "use-cases", "use-cases facade", errors)
    validate_owners(clients_dir, clients_lib, CLIENT_USE_CASE_OWNERS, "use-cases-clients", "client application facade", errors)
    validate_owners(identity_dir, identity_lib, IDENTITY_USE_CASE_OWNERS, "use-cases-identity", "identity application facade", errors)
    validate_owners(mailboxes_dir, mailboxes_lib, MAILBOX_USE_CASE_OWNERS, "use-cases-mailboxes", "mailbox application facade", errors)

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
        "pub use error::ApplicationError;",
        "pub use profiles::{OpenProfileCommand, OpenProfileDecision, decide_open_profile};",
    )
    for line in required_port_reexports:
        if line not in ports_lib:
            errors.append(f"application-ports facade missing compatibility re-export `{line}`")
    for line in required_mono_reexports:
        if line not in mono_lib:
            errors.append(f"use-cases facade missing shared-owner re-export `{line}`")
    return errors


def write_self_test_fixture(root: Path) -> None:
    ports = root / "crates/application-ports/src"
    mono = root / "crates/use-cases/src"
    clients = root / "crates/use-cases-clients/src"
    identity = root / "crates/use-cases-identity/src"
    mailboxes = root / "crates/use-cases-mailboxes/src"
    worker = root / "apps/control-plane-worker/src"
    for directory in (ports, mono, clients, identity, mailboxes, worker):
        directory.mkdir(parents=True)

    for module in PORT_MODULES:
        (ports / f"{module}.rs").write_text("// fixture\n", encoding="utf-8")
    for module in MONOLITH_USE_CASE_MODULES:
        (mono / f"{module}.rs").write_text("// fixture\n", encoding="utf-8")
    for module in CLIENT_USE_CASE_MODULES:
        (clients / f"{module}.rs").write_text("// fixture\n", encoding="utf-8")
    for module in IDENTITY_USE_CASE_MODULES:
        (identity / f"{module}.rs").write_text("// fixture\n", encoding="utf-8")
    for module in MAILBOX_USE_CASE_MODULES:
        (mailboxes / f"{module}.rs").write_text("// fixture\n", encoding="utf-8")

    (ports / "lib.rs").write_text("\n".join(f"pub mod {m};" for m in PORT_MODULES), encoding="utf-8")
    (clients / "lib.rs").write_text("\n".join(f"pub mod {m};" for m in CLIENT_USE_CASE_MODULES), encoding="utf-8")
    (identity / "lib.rs").write_text("\n".join(f"pub mod {m};" for m in IDENTITY_USE_CASE_MODULES), encoding="utf-8")
    (mailboxes / "lib.rs").write_text("\n".join(f"pub mod {m};" for m in MAILBOX_USE_CASE_MODULES), encoding="utf-8")
    (mono / "lib.rs").write_text(
        "\n".join(f"pub mod {m};" for m in MONOLITH_USE_CASE_MODULES)
        + "\npub use use_cases_identity::{identity_ceremonies, identity_governance};\n",
        encoding="utf-8",
    )
    (mono.parent / "Cargo.toml").write_text(
        "[package]\nname = \"use-cases\"\nversion = \"0.1.0\"\n"
        "[dependencies]\nuse-cases-mailboxes.workspace = true\n",
        encoding="utf-8",
    )
    (worker / "legacy.rs").write_text(
        "use use_cases::mailboxes::execute_create_mailbox_binding;\n",
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
            if not any("historical application compatibility reference is forbidden" in error for error in errors):
                print("negative compatibility re-export fixture unexpectedly passed")
                return 1
            if not any("shared use-cases must not depend on extracted application crate" in error for error in errors):
                print("negative extracted dependency fixture unexpectedly passed")
                return 1
            if not any("Worker must import extracted application ownership directly" in error for error in errors):
                print("negative Worker compatibility import fixture unexpectedly passed")
                return 1
            print("negative compatibility/dependency/Worker import fixtures rejected as expected")
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
