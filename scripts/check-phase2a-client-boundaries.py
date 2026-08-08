#!/usr/bin/env python3
"""Fail closed if Phase 2A client ownership or contact-protection boundaries regress."""

from __future__ import annotations

import argparse
import tempfile
import tomllib
from pathlib import Path

DOMAIN_FILES = (
    "crates/client-domain/src/client.rs",
    "crates/client-domain/src/contact_point.rs",
    "crates/client-domain/src/assignment.rs",
    "crates/client-domain/src/merge.rs",
)
APPLICATION_FILES = (
    "crates/use-cases-clients/src/clients.rs",
    "crates/use-cases-clients/src/client_grants.rs",
    "crates/use-cases-clients/src/contacts.rs",
    "crates/use-cases-clients/src/lifecycle.rs",
)
REQUIRED = DOMAIN_FILES + APPLICATION_FILES + (
    "crates/client-domain/src/lib.rs",
    "crates/use-cases-clients/Cargo.toml",
    "crates/use-cases-clients/src/lib.rs",
    "crates/application-ports/src/clients.rs",
    "crates/primitives/src/lib.rs",
    "crates/use-cases/src/clients.rs",
    "crates/use-cases/src/client_grants.rs",
    "apps/control-plane-worker/src/clients.rs",
    "apps/control-plane-worker/Cargo.toml",
)
INNER_FORBIDDEN = (
    "cloudflare_adapters",
    "worker::",
    "D1Database",
    "D1ClientApplicationRepository",
    "web_sys",
    "wasm_bindgen",
)
PROTECTION_FORBIDDEN = (
    "raw_contact",
    "plaintext_contact",
    "plaintext_value",
    "contact_plaintext",
)
EXPECTED_CLIENT_DEPS = {"profile-platform-primitives", "zeroize"}
EXPECTED_APP_DEPS = {
    "application-ports",
    "client-domain",
    "contracts",
    "identity-access-domain",
    "profile-platform-primitives",
    "zeroize",
}


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def deps(path: Path) -> set[str]:
    with path.open("rb") as handle:
        doc = tomllib.load(handle)
    result: set[str] = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        values = doc.get(section, {})
        if isinstance(values, dict):
            result.update(str(name) for name in values)
    return result


def section(source: str, start: str, end: str) -> str:
    start_index = source.find(start)
    if start_index < 0:
        return ""
    end_index = source.find(end, start_index + len(start))
    if end_index < 0:
        return source[start_index:]
    return source[start_index:end_index]


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    for relative in REQUIRED:
        path = root / relative
        if not path.is_file() or not read(path).strip():
            errors.append(f"missing Phase 2A boundary file: {relative}")
    if errors:
        return errors

    client_manifest = root / "crates/client-domain/Cargo.toml"
    app_manifest = root / "crates/use-cases-clients/Cargo.toml"
    if deps(client_manifest) != EXPECTED_CLIENT_DEPS:
        errors.append("client-domain dependency boundary drift")
    if deps(app_manifest) != EXPECTED_APP_DEPS:
        errors.append("use-cases-clients dependency boundary drift")

    domain_lib = read(root / "crates/client-domain/src/lib.rs")
    for token in ("pub struct ", "pub enum ", "pub trait ", "impl "):
        if token in domain_lib:
            errors.append(f"client-domain facade must stay thin; found `{token.strip()}`")

    app_lib = read(root / "crates/use-cases-clients/src/lib.rs")
    for token in ("pub struct ", "pub enum ", "pub trait ", "impl "):
        if token in app_lib:
            errors.append(f"use-cases-clients facade must stay thin; found `{token.strip()}`")

    for relative in DOMAIN_FILES + APPLICATION_FILES:
        source = read(root / relative)
        for marker in INNER_FORBIDDEN:
            if marker in source:
                errors.append(f"{relative} contains outer marker {marker!r}")

    primitives = read(root / "crates/primitives/src/lib.rs")
    if "define_typed_id!(ContactPointId);" not in primitives:
        errors.append("opaque ContactPointId is missing from primitives")

    client = read(root / "crates/client-domain/src/client.rs")
    for marker in (
        'Self::Person => "PERSON"',
        'Self::Organization => "ORGANIZATION"',
        'Self::Active => "ACTIVE"',
        'Self::Archived => "ARCHIVED"',
        'Self::Merged => "MERGED"',
        "pub fn rename(&mut self",
        "pub fn archive(&mut self",
    ):
        if marker not in client:
            errors.append(f"client lifecycle vocabulary/invariant missing `{marker}`")

    contact = read(root / "crates/client-domain/src/contact_point.rs")
    for marker in (
        "pub enum ContactKind",
        "pub enum ContactStatus",
        "pub enum ContactNormalizationVersion",
        "pub enum ContactProtectionVersion",
        "pub struct ProtectedContactPoint",
        "pub struct EncryptedContactValue",
        "pub struct ExactLookupToken",
        "client-contact-exact-lookup\\0v1\\0",
        "impl Drop for NormalizedContactValue",
        "impl Drop for ExactLookupHmacInput",
    ):
        if marker not in contact:
            errors.append(f"contact protection contract missing `{marker}`")
    protected_model = section(contact, "pub struct ProtectedContactPoint", "impl ProtectedContactPoint")
    for marker in ("String", "NormalizedContactValue", "ExactLookupHmacInput"):
        if marker in protected_model:
            errors.append(f"persistable protected contact carries transient/plaintext marker `{marker}`")

    ports = read(root / "crates/application-ports/src/clients.rs")
    for marker in (
        "pub trait ClientLifecycleApplicationPort",
        "pub trait ContactProtectionPort",
        "pub trait ProtectedClientContactRepositoryPort",
        "ContactEncryptionKeyDomain::ClientContactDisplay",
        "ContactLookupKeyDomain::TenantExactLookup",
        "pub struct ProtectedContactWrite",
    ):
        if marker not in ports:
            errors.append(f"client ports missing `{marker}`")
    protected_write = section(ports, "pub struct ProtectedContactWrite", "impl ProtectedContactWrite")
    for marker in ("String", "NormalizedContactValue", "ExactLookupHmacInput"):
        if marker in protected_write:
            errors.append(f"persistence port accepts transient/plaintext marker `{marker}`")
    for marker in PROTECTION_FORBIDDEN:
        if marker in ports.lower():
            errors.append(f"client ports contain plaintext persistence marker `{marker}`")

    contacts = read(root / "crates/use-cases-clients/src/contacts.rs")
    for marker in (
        "pub struct TransientContactValue",
        "impl Drop for TransientContactValue",
        "normalize_contact_value",
        "exact_lookup_hmac_input",
        "encrypt_contact_display",
        "derive_exact_lookup_token",
        "ProtectedContactWrite::new",
    ):
        if marker not in contacts:
            errors.append(f"contact application boundary missing `{marker}`")
    if contacts.find("authorize_contact_mutation(role)?") > contacts.find("normalize_contact_value"):
        errors.append("contact authorization must precede plaintext normalization/protection")

    lifecycle = read(root / "crates/use-cases-clients/src/lifecycle.rs")
    for marker in (
        "execute_update_client",
        "execute_archive_client",
        "authorize_client_lifecycle(role)?",
        "load_exact_version",
        "decide_client_lifecycle_replay",
        "persist_client_lifecycle",
    ):
        if marker not in lifecycle:
            errors.append(f"client lifecycle application contract missing `{marker}`")

    shared_clients = read(root / "crates/use-cases/src/clients.rs")
    shared_grants = read(root / "crates/use-cases/src/client_grants.rs")
    if "pub use use_cases_clients::clients::*;" not in shared_clients or "pub async fn" in shared_clients:
        errors.append("shared use-cases must be compatibility-only for client create/query")
    if (
        "pub use use_cases_clients::client_grants::*;" not in shared_grants
        or "pub async fn" in shared_grants
    ):
        errors.append("shared use-cases must be compatibility-only for client grants")

    worker = read(root / "apps/control-plane-worker/src/clients.rs")
    if "use use_cases_clients::clients" not in worker or "use use_cases_clients::client_grants" not in worker:
        errors.append("Worker must compose use-cases-clients directly")
    for marker in ("use use_cases::clients", "use use_cases::client_grants"):
        if marker in worker:
            errors.append(f"Worker still composes superseded shared client owner `{marker}`")

    worker_manifest = deps(root / "apps/control-plane-worker/Cargo.toml")
    if "use-cases-clients" not in worker_manifest:
        errors.append("Worker manifest must depend on use-cases-clients")

    adapters = root / "crates/cloudflare-adapters/src"
    if adapters.is_dir():
        adapter_source = "\n".join(read(path) for path in adapters.rglob("*.rs"))
        if "impl ProtectedClientContactRepositoryPort" in adapter_source:
            errors.append("Phase 2A must not implement D1 protected-contact persistence before Phase 2B")

    migrations = root / "migrations/d1"
    if migrations.is_dir():
        migration_source = "\n".join(read(path).lower() for path in migrations.rglob("*.sql"))
        for marker in ("contact_point", "contact_points", "contact_ciphertext", "lookup_hmac"):
            if marker in migration_source:
                errors.append(f"Phase 2A must not add D1 contact persistence marker `{marker}`")

    return errors


def write_fixture(root: Path) -> None:
    for relative in REQUIRED:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("// fixture\n", encoding="utf-8")
    (root / "crates/client-domain/Cargo.toml").write_text(
        "[package]\nname='client-domain'\nversion='0.1.0'\n[dependencies]\n"
        "profile-platform-primitives={}\nzeroize={}\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-clients/Cargo.toml").write_text(
        "[package]\nname='use-cases-clients'\nversion='0.1.0'\n[dependencies]\n"
        "application-ports={}\nclient-domain={}\ncontracts={}\nidentity-access-domain={}\n"
        "profile-platform-primitives={}\nzeroize={}\n",
        encoding="utf-8",
    )
    (root / "crates/client-domain/src/lib.rs").write_text(
        "mod client; mod contact_point; mod assignment; mod merge; pub use client::*;\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-clients/src/lib.rs").write_text(
        "pub mod clients; pub mod client_grants; pub mod contacts; pub mod lifecycle;\n",
        encoding="utf-8",
    )
    (root / "crates/primitives/src/lib.rs").write_text(
        "define_typed_id!(ContactPointId);\n", encoding="utf-8"
    )
    (root / "crates/client-domain/src/client.rs").write_text(
        'Self::Person => "PERSON" Self::Organization => "ORGANIZATION" '
        'Self::Active => "ACTIVE" Self::Archived => "ARCHIVED" Self::Merged => "MERGED" '
        "pub fn rename(&mut self pub fn archive(&mut self\n",
        encoding="utf-8",
    )
    (root / "crates/client-domain/src/contact_point.rs").write_text(
        "pub enum ContactKind {} pub enum ContactStatus {} pub enum ContactNormalizationVersion {} "
        "pub enum ContactProtectionVersion {} pub struct EncryptedContactValue { ciphertext: Vec<u8> } "
        "pub struct ExactLookupToken { bytes: [u8;32] } "
        "pub struct ProtectedContactPoint { display_value: EncryptedContactValue, exact_lookup: ExactLookupToken } "
        "impl ProtectedContactPoint {} client-contact-exact-lookup\\0v1\\0 "
        "impl Drop for NormalizedContactValue {} impl Drop for ExactLookupHmacInput {}\n",
        encoding="utf-8",
    )
    (root / "crates/application-ports/src/clients.rs").write_text(
        "pub trait ClientLifecycleApplicationPort {} pub trait ContactProtectionPort {} "
        "pub trait ProtectedClientContactRepositoryPort {} "
        "ContactEncryptionKeyDomain::ClientContactDisplay ContactLookupKeyDomain::TenantExactLookup "
        "pub struct ProtectedContactWrite { contact: ProtectedContactPoint } impl ProtectedContactWrite {}\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-clients/src/contacts.rs").write_text(
        "pub struct TransientContactValue {} impl Drop for TransientContactValue {} "
        "fn x(){ authorize_contact_mutation(role)?; normalize_contact_value(); exact_lookup_hmac_input(); "
        "encrypt_contact_display(); derive_exact_lookup_token(); ProtectedContactWrite::new(); }\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases-clients/src/lifecycle.rs").write_text(
        "execute_update_client execute_archive_client authorize_client_lifecycle(role)? load_exact_version "
        "decide_client_lifecycle_replay persist_client_lifecycle\n",
        encoding="utf-8",
    )
    (root / "crates/use-cases/src/clients.rs").write_text(
        "pub use use_cases_clients::clients::*;\n", encoding="utf-8"
    )
    (root / "crates/use-cases/src/client_grants.rs").write_text(
        "pub use use_cases_clients::client_grants::*;\n", encoding="utf-8"
    )
    (root / "apps/control-plane-worker/src/clients.rs").write_text(
        "use use_cases_clients::clients; use use_cases_clients::client_grants;\n",
        encoding="utf-8",
    )
    (root / "apps/control-plane-worker/Cargo.toml").write_text(
        "[package]\nname='worker'\nversion='0.1.0'\n[dependencies]\nuse-cases-clients={}\n",
        encoding="utf-8",
    )


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="phase2a-client-boundary-") as temp:
        root = Path(temp)
        write_fixture(root)
        baseline = validate(root)
        if baseline:
            print("invalid Phase 2A boundary fixture baseline")
            print("\n".join(baseline))
            return 1

        write_fixture(root)
        ports = root / "crates/application-ports/src/clients.rs"
        ports.write_text(
            read(ports).replace(
                "pub struct ProtectedContactWrite { contact: ProtectedContactPoint }",
                "pub struct ProtectedContactWrite { plaintext_value: String }",
            ),
            encoding="utf-8",
        )
        if not any("plaintext" in error or "transient" in error for error in validate(root)):
            print("plaintext persistence fixture unexpectedly passed")
            return 1

        write_fixture(root)
        domain = root / "crates/client-domain/src/contact_point.rs"
        domain.write_text(read(domain) + "\nuse worker::Env;\n", encoding="utf-8")
        if not any("outer marker" in error for error in validate(root)):
            print("provider leakage fixture unexpectedly passed")
            return 1

        write_fixture(root)
        shared = root / "crates/use-cases/src/clients.rs"
        shared.write_text(read(shared) + "\npub async fn execute_create_client() {}\n", encoding="utf-8")
        if not any("compatibility-only" in error for error in validate(root)):
            print("duplicate shared ownership fixture unexpectedly passed")
            return 1

        write_fixture(root)
        adapter = root / "crates/cloudflare-adapters/src/d1_clients.rs"
        adapter.parent.mkdir(parents=True, exist_ok=True)
        adapter.write_text("impl ProtectedClientContactRepositoryPort for D1Contacts {}\n", encoding="utf-8")
        if not any("before Phase 2B" in error for error in validate(root)):
            print("premature D1 contact persistence fixture unexpectedly passed")
            return 1

    print("Phase 2A negative ownership/protection fixtures rejected as expected.")
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
    print("Phase 2A client ownership and contact-protection boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
