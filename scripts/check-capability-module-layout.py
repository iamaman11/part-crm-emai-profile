#!/usr/bin/env python3
"""Fail closed if application boundary crates collapse back into grab-bag lib.rs files."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path


PORT_MODULES = (
    "audit",
    "clients",
    "clock",
    "commands",
    "generations",
    "identity",
    "mailboxes",
    "profiles",
    "sessions",
)
USE_CASE_MODULES = ("clients", "error", "identity_acl", "profiles")

PORT_OWNERS = {
    "audit.rs": ("pub trait AuditPort", "pub struct AuditRecord", "pub enum AuditResult"),
    "clients.rs": (
        "pub trait ClientRepository",
        "pub trait ClientApplicationPort",
        "pub struct ClientCreateWrite",
    ),
    "clock.rs": ("pub trait ClockPort",),
    "commands.rs": ("pub struct CommandExecutionEvidence",),
    "generations.rs": ("pub struct GenerationObjectReference", "pub trait GenerationObjectStorePort"),
    "identity.rs": ("pub trait MembershipRepository",),
    "mailboxes.rs": ("pub struct MailboxObservation", "pub trait MailboxProviderPort"),
    "profiles.rs": ("pub trait ProfileRepository",),
    "sessions.rs": ("pub trait ProfileCoordinatorPort",),
}

USE_CASE_OWNERS = {
    "clients.rs": (
        "pub struct CreateClientCommand",
        "pub fn decide_create_client",
        "pub struct ExecuteCreateClientCommand",
        "pub async fn execute_create_client",
        "pub async fn get_visible_client",
    ),
    "error.rs": ("pub struct ApplicationError",),
    "profiles.rs": ("pub struct OpenProfileCommand", "pub fn decide_open_profile"),
}


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"cannot read {path}: {exc}") from exc


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    ports_dir = root / "crates/application-ports/src"
    use_cases_dir = root / "crates/use-cases/src"

    ports_lib_path = ports_dir / "lib.rs"
    use_cases_lib_path = use_cases_dir / "lib.rs"
    if not ports_lib_path.is_file():
        errors.append("missing application-ports facade lib.rs")
        ports_lib = ""
    else:
        ports_lib = read(ports_lib_path)
    if not use_cases_lib_path.is_file():
        errors.append("missing use-cases facade lib.rs")
        use_cases_lib = ""
    else:
        use_cases_lib = read(use_cases_lib_path)

    for module in PORT_MODULES:
        declaration = f"pub mod {module};"
        if declaration not in ports_lib:
            errors.append(f"application-ports facade missing `{declaration}`")
        path = ports_dir / f"{module}.rs"
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing/non-empty application-ports capability module: {path.relative_to(root)}")

    for module in USE_CASE_MODULES:
        declaration = f"pub mod {module};"
        if declaration not in use_cases_lib:
            errors.append(f"use-cases facade missing `{declaration}`")
        path = use_cases_dir / f"{module}.rs"
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing/non-empty use-cases capability module: {path.relative_to(root)}")

    for filename, symbols in PORT_OWNERS.items():
        owner_text = read(ports_dir / filename) if (ports_dir / filename).is_file() else ""
        for symbol in symbols:
            if symbol not in owner_text:
                errors.append(f"{filename} must own `{symbol}`")
            if symbol in ports_lib:
                errors.append(f"application-ports facade must not own `{symbol}`")

    for filename, symbols in USE_CASE_OWNERS.items():
        owner_text = read(use_cases_dir / filename) if (use_cases_dir / filename).is_file() else ""
        for symbol in symbols:
            if symbol not in owner_text:
                errors.append(f"{filename} must own `{symbol}`")
            if symbol in use_cases_lib:
                errors.append(f"use-cases facade must not own `{symbol}`")

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
    required_use_case_reexports = (
        "pub use clients::{CreateClientCommand, decide_create_client};",
        "pub use error::ApplicationError;",
        "pub use profiles::{OpenProfileCommand, OpenProfileDecision, decide_open_profile};",
    )
    for line in required_port_reexports:
        if line not in ports_lib:
            errors.append(f"application-ports facade missing compatibility re-export `{line}`")
    for line in required_use_case_reexports:
        if line not in use_cases_lib:
            errors.append(f"use-cases facade missing compatibility re-export `{line}`")

    return errors


def write_self_test_fixture(root: Path) -> None:
    ports = root / "crates/application-ports/src"
    use_cases = root / "crates/use-cases/src"
    ports.mkdir(parents=True)
    use_cases.mkdir(parents=True)

    for module in PORT_MODULES:
        (ports / f"{module}.rs").write_text("// fixture capability\n", encoding="utf-8")
    for module in USE_CASE_MODULES:
        (use_cases / f"{module}.rs").write_text("// fixture capability\n", encoding="utf-8")

    (ports / "lib.rs").write_text(
        "\n".join(f"pub mod {module};" for module in PORT_MODULES)
        + "\n\npub trait ClockPort {}\n",
        encoding="utf-8",
    )
    (use_cases / "lib.rs").write_text(
        "\n".join(f"pub mod {module};" for module in USE_CASE_MODULES)
        + "\n\npub struct ApplicationError;\n",
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
            if not errors:
                print("negative capability-layout fixture unexpectedly passed")
                return 1
            if not any("facade must not own" in error for error in errors):
                print("negative fixture failed, but not for facade ownership")
                for error in errors:
                    print(error)
                return 1
            print("negative capability-layout fixture rejected as expected")
            return 0

    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("capability module layout: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
