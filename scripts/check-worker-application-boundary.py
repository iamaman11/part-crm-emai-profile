#!/usr/bin/env python3
"""Fail closed if migrated Worker surfaces regain provider/D1 or compatibility coupling."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

FORBIDDEN_CLIENT_TRANSPORT_TOKENS = (
    "cloudflare_adapters::d1_",
    "D1CatalogRepository",
    "D1GovernedCommandRepository",
    "D1IdentityQueryRepository",
    "D1IdempotencyRepository",
    "CreateClientMutation",
    "ClientGrantMutation",
    "ClientGrantValue",
    "D1Database",
)

FORBIDDEN_IDENTITY_TRANSPORT_TOKENS = (
    "cloudflare_adapters::d1_",
    "D1GovernedCommandRepository",
    "D1IdempotencyRepository",
    "D1IdentityAclRepository",
    "D1InvitationAcceptanceRepository",
    "OwnerTransferMutation",
    "CreateInvitationMutation",
    "MembershipStatusMutation",
    "BootstrapOwnerMutation",
    "AcceptInvitationMutation",
    "D1Database",
)

REQUIRED_CLIENT_TRANSPORT_TOKENS = (
    "use use_cases_clients::client_grants",
    "use use_cases_clients::clients",
    "execute_create_client",
    "get_visible_client",
    "execute_client_grant",
    "authorize_client_grant",
    "RouteClass::ClientGrantApi",
    "client_application(env)",
)

REQUIRED_IDENTITY_TRANSPORT_TOKENS = (
    "use use_cases_identity::identity_ceremonies",
    "use use_cases_identity::identity_governance",
    "RouteClass::OwnerBootstrapApi",
    "RouteClass::OwnerTransferApi",
    "RouteClass::InvitationCollectionApi",
    "RouteClass::InvitationAcceptApi",
    "RouteClass::MembershipStatusApi",
    "execute_owner_bootstrap",
    "execute_owner_transfer",
    "execute_invitation_create",
    "execute_invitation_accept",
    "execute_membership_status",
    "authorize_identity_governance",
    "identity_governance_application(env)",
    "identity_ceremony_application(env, verified.identity().clone())",
)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"cannot read {path}: {exc}") from exc


def require_tokens(text: str, tokens: tuple[str, ...], label: str, errors: list[str]) -> None:
    for token in tokens:
        if token not in text:
            errors.append(f"{label} missing `{token}`")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    worker = root / "apps/control-plane-worker/src"
    client_path = worker / "clients.rs"
    identity_path = worker / "identity.rs"
    composition_path = worker / "composition.rs"
    lib_path = worker / "lib.rs"
    legacy_api_path = worker / "api.rs"
    client_ports_path = root / "crates/application-ports/src/clients.rs"
    client_use_cases_path = root / "crates/use-cases-clients/src/clients.rs"
    client_grant_use_cases_path = root / "crates/use-cases-clients/src/client_grants.rs"
    shared_client_facade_path = root / "crates/use-cases/src/clients.rs"
    shared_grant_facade_path = root / "crates/use-cases/src/client_grants.rs"
    client_adapter_path = root / "crates/cloudflare-adapters/src/d1_clients.rs"
    identity_governance_ports_path = root / "crates/application-ports/src/identity_governance.rs"
    identity_ceremony_ports_path = root / "crates/application-ports/src/identity_ceremonies.rs"
    identity_governance_use_cases_path = root / "crates/use-cases-identity/src/identity_governance.rs"
    identity_ceremony_use_cases_path = root / "crates/use-cases-identity/src/identity_ceremonies.rs"
    identity_governance_adapter_path = root / "crates/cloudflare-adapters/src/d1_identity_governance.rs"
    identity_ceremony_adapter_path = root / "crates/cloudflare-adapters/src/d1_identity_ceremonies.rs"

    required_paths = (
        client_path,
        identity_path,
        composition_path,
        lib_path,
        client_ports_path,
        client_use_cases_path,
        client_grant_use_cases_path,
        client_adapter_path,
        identity_governance_ports_path,
        identity_ceremony_ports_path,
        identity_governance_use_cases_path,
        identity_ceremony_use_cases_path,
        identity_governance_adapter_path,
        identity_ceremony_adapter_path,
    )
    for path in required_paths:
        if not path.is_file():
            errors.append(f"missing application-boundary file: {path.relative_to(root)}")
    if errors:
        return errors

    if legacy_api_path.exists():
        errors.append("legacy api.rs must remain removed after identity application-boundary migration")
    for path in (shared_client_facade_path, shared_grant_facade_path):
        if path.exists():
            errors.append(
                "shared use-cases compatibility facade must remain removed: "
                f"{path.relative_to(root)}"
            )

    client = read(client_path)
    identity = read(identity_path)
    composition = read(composition_path)
    worker_lib = read(lib_path)
    client_ports = read(client_ports_path)
    client_use_cases = read(client_use_cases_path)
    client_grant_use_cases = read(client_grant_use_cases_path)
    client_adapter = read(client_adapter_path)
    identity_governance_ports = read(identity_governance_ports_path)
    identity_ceremony_ports = read(identity_ceremony_ports_path)
    identity_governance_use_cases = read(identity_governance_use_cases_path)
    identity_ceremony_use_cases = read(identity_ceremony_use_cases_path)
    identity_governance_adapter = read(identity_governance_adapter_path)
    identity_ceremony_adapter = read(identity_ceremony_adapter_path)

    for token in FORBIDDEN_CLIENT_TRANSPORT_TOKENS:
        if token in client:
            errors.append(f"client Worker transport must not contain provider token `{token}`")
    for token in FORBIDDEN_IDENTITY_TRANSPORT_TOKENS:
        if token in identity:
            errors.append(f"identity Worker transport must not contain provider token `{token}`")

    require_tokens(client, REQUIRED_CLIENT_TRANSPORT_TOKENS, "client Worker transport", errors)
    require_tokens(identity, REQUIRED_IDENTITY_TRANSPORT_TOKENS, "identity Worker transport", errors)

    for route_token in (
        "RouteClass::ClientCollectionApi",
        "RouteClass::ClientResourceApi",
        "RouteClass::ClientGrantApi",
        "clients::dispatch(route, &mut request, &env).await",
        "RouteClass::OwnerBootstrapApi",
        "RouteClass::OwnerTransferApi",
        "RouteClass::InvitationCollectionApi",
        "RouteClass::InvitationAcceptApi",
        "RouteClass::MembershipStatusApi",
        "identity::dispatch(route, &mut request, &env).await",
    ):
        if route_token not in worker_lib:
            errors.append(f"Worker composition root missing migrated route token `{route_token}`")

    require_tokens(
        composition,
        (
            "D1ClientApplicationRepository",
            "D1IdentityGovernanceApplicationRepository",
            "D1IdentityCeremonyApplicationRepository",
            "pub fn identity_governance_application(",
            "pub fn identity_ceremony_application(",
            "env.d1(D1_CATALOG_BINDING)?",
        ),
        "Worker composition root",
        errors,
    )

    require_tokens(
        client_ports,
        (
            "pub trait ClientApplicationPort",
            "pub trait ClientGrantApplicationPort",
            "pub struct ClientGrantWrite",
        ),
        "client application ports",
        errors,
    )
    require_tokens(
        client_use_cases,
        ("pub async fn execute_create_client", "pub async fn get_visible_client"),
        "extracted client use cases",
        errors,
    )
    require_tokens(
        client_grant_use_cases,
        (
            "pub async fn execute_client_grant",
            "pub fn authorize_client_grant",
            "pub fn next_client_grant_version",
            "decide_client_grant_replay",
        ),
        "extracted client grant use cases",
        errors,
    )
    require_tokens(
        client_adapter,
        (
            "impl ClientApplicationPort for D1ClientApplicationRepository",
            "impl ClientGrantApplicationPort for D1ClientApplicationRepository",
            "ClientGrantMutation",
            ".grant_client(actor, mutation)",
            ".revoke_client_grant(actor, mutation)",
        ),
        "Cloudflare client adapter",
        errors,
    )

    require_tokens(
        identity_governance_ports,
        (
            "pub trait ActiveOwnerGovernanceApplicationPort",
            "pub struct OwnerTransferWrite",
            "pub struct InvitationCreateWrite",
            "pub struct MembershipStatusWrite",
        ),
        "identity governance ports",
        errors,
    )
    require_tokens(
        identity_ceremony_ports,
        (
            "pub trait IdentityCeremonyApplicationPort",
            "pub struct VerifiedIdentitySnapshot",
            "pub struct BootstrapOwnerWrite",
            "pub struct InvitationAcceptWrite",
        ),
        "identity ceremony ports",
        errors,
    )
    require_tokens(
        identity_governance_use_cases,
        (
            "pub async fn execute_owner_transfer",
            "pub async fn execute_invitation_create",
            "pub async fn execute_membership_status",
            "pub fn authorize_identity_governance",
        ),
        "identity governance use cases",
        errors,
    )
    require_tokens(
        identity_ceremony_use_cases,
        (
            "pub async fn execute_owner_bootstrap",
            "pub async fn execute_invitation_accept",
            'const OWNER_BOOTSTRAP_COMMAND: &str = "tenant.owner_bootstrap";',
            'const INVITATION_ACCEPT_COMMAND: &str = "invitation.accept";',
        ),
        "identity ceremony use cases",
        errors,
    )
    require_tokens(
        identity_governance_adapter,
        (
            "impl ActiveOwnerGovernanceApplicationPort for D1IdentityGovernanceApplicationRepository",
            ".transfer_owner(",
            ".create_invitation(",
            ".update_membership_status(",
            "D1IdempotencyRepository",
        ),
        "identity governance D1 adapter",
        errors,
    )
    require_tokens(
        identity_ceremony_adapter,
        (
            "impl IdentityCeremonyApplicationPort for D1IdentityCeremonyApplicationRepository",
            ".bootstrap_owner(",
            ".accept(",
            "VerifiedBootstrapContext::from_verified_identity",
            "D1IdempotencyRepository",
        ),
        "identity ceremony D1 adapter",
        errors,
    )

    return errors


def write_self_test_fixture(root: Path) -> None:
    worker = root / "apps/control-plane-worker/src"
    ports = root / "crates/application-ports/src"
    shared_use_cases = root / "crates/use-cases/src"
    client_use_cases = root / "crates/use-cases-clients/src"
    identity_use_cases = root / "crates/use-cases-identity/src"
    adapters = root / "crates/cloudflare-adapters/src"
    for path in (worker, ports, shared_use_cases, client_use_cases, identity_use_cases, adapters):
        path.mkdir(parents=True, exist_ok=True)

    (worker / "clients.rs").write_text(
        "use cloudflare_adapters::d1_catalog::D1CatalogRepository;\n"
        "use use_cases_clients::client_grants; use use_cases_clients::clients;\n"
        "fn route() { execute_create_client(); get_visible_client(); execute_client_grant(); "
        "authorize_client_grant(); RouteClass::ClientGrantApi; client_application(env); }\n",
        encoding="utf-8",
    )
    (worker / "identity.rs").write_text(
        "use cloudflare_adapters::d1_identity_acl::D1IdentityAclRepository;\n"
        "use use_cases_identity::identity_ceremonies; use use_cases_identity::identity_governance;\n"
        "fn route() { RouteClass::OwnerBootstrapApi; RouteClass::OwnerTransferApi; "
        "RouteClass::InvitationCollectionApi; RouteClass::InvitationAcceptApi; "
        "RouteClass::MembershipStatusApi; execute_owner_bootstrap(); execute_owner_transfer(); "
        "execute_invitation_create(); execute_invitation_accept(); execute_membership_status(); "
        "authorize_identity_governance(); identity_governance_application(env); "
        "identity_ceremony_application(env, verified.identity().clone()); }\n",
        encoding="utf-8",
    )
    (worker / "composition.rs").write_text(
        "D1ClientApplicationRepository D1IdentityGovernanceApplicationRepository "
        "D1IdentityCeremonyApplicationRepository pub fn identity_governance_application( "
        "pub fn identity_ceremony_application( env.d1(D1_CATALOG_BINDING)?\n",
        encoding="utf-8",
    )
    (worker / "lib.rs").write_text(
        "RouteClass::ClientCollectionApi RouteClass::ClientResourceApi RouteClass::ClientGrantApi "
        "clients::dispatch(route, &mut request, &env).await RouteClass::OwnerBootstrapApi "
        "RouteClass::OwnerTransferApi RouteClass::InvitationCollectionApi "
        "RouteClass::InvitationAcceptApi RouteClass::MembershipStatusApi "
        "identity::dispatch(route, &mut request, &env).await\n",
        encoding="utf-8",
    )
    (worker / "api.rs").write_text("async fn bootstrap_owner() {}\n", encoding="utf-8")
    (ports / "clients.rs").write_text(
        "pub trait ClientApplicationPort {}\npub trait ClientGrantApplicationPort {}\n"
        "pub struct ClientGrantWrite;\n",
        encoding="utf-8",
    )
    (client_use_cases / "clients.rs").write_text(
        "pub async fn execute_create_client() {}\npub async fn get_visible_client() {}\n",
        encoding="utf-8",
    )
    (client_use_cases / "client_grants.rs").write_text(
        "pub async fn execute_client_grant() {}\npub fn authorize_client_grant() {}\n"
        "pub fn next_client_grant_version() {}\nfn replay() { decide_client_grant_replay(); }\n",
        encoding="utf-8",
    )
    (shared_use_cases / "clients.rs").write_text(
        "pub use use_cases_clients::clients::*;\n", encoding="utf-8"
    )
    (shared_use_cases / "client_grants.rs").write_text(
        "pub use use_cases_clients::client_grants::*;\n", encoding="utf-8"
    )
    (adapters / "d1_clients.rs").write_text(
        "impl ClientApplicationPort for D1ClientApplicationRepository {}\n"
        "impl ClientGrantApplicationPort for D1ClientApplicationRepository {}\n"
        "fn grant() { ClientGrantMutation; repo.grant_client(actor, mutation); "
        "repo.revoke_client_grant(actor, mutation); }\n",
        encoding="utf-8",
    )
    (ports / "identity_governance.rs").write_text(
        "pub trait ActiveOwnerGovernanceApplicationPort {}\npub struct OwnerTransferWrite;\n"
        "pub struct InvitationCreateWrite;\npub struct MembershipStatusWrite;\n",
        encoding="utf-8",
    )
    (ports / "identity_ceremonies.rs").write_text(
        "pub trait IdentityCeremonyApplicationPort {}\npub struct VerifiedIdentitySnapshot;\n"
        "pub struct BootstrapOwnerWrite;\npub struct InvitationAcceptWrite;\n",
        encoding="utf-8",
    )
    (identity_use_cases / "identity_governance.rs").write_text(
        "pub async fn execute_owner_transfer() {}\npub async fn execute_invitation_create() {}\n"
        "pub async fn execute_membership_status() {}\npub fn authorize_identity_governance() {}\n",
        encoding="utf-8",
    )
    (identity_use_cases / "identity_ceremonies.rs").write_text(
        'const OWNER_BOOTSTRAP_COMMAND: &str = "tenant.owner_bootstrap";\n'
        'const INVITATION_ACCEPT_COMMAND: &str = "invitation.accept";\n'
        "pub async fn execute_owner_bootstrap() {}\npub async fn execute_invitation_accept() {}\n",
        encoding="utf-8",
    )
    (adapters / "d1_identity_governance.rs").write_text(
        "D1IdempotencyRepository impl ActiveOwnerGovernanceApplicationPort for "
        "D1IdentityGovernanceApplicationRepository { fn x(){ repo.transfer_owner( ); "
        "repo.create_invitation( ); repo.update_membership_status( ); } }\n",
        encoding="utf-8",
    )
    (adapters / "d1_identity_ceremonies.rs").write_text(
        "D1IdempotencyRepository VerifiedBootstrapContext::from_verified_identity "
        "impl IdentityCeremonyApplicationPort for D1IdentityCeremonyApplicationRepository "
        "{ fn x(){ repo.bootstrap_owner( ); repo.accept( ); } }\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="worker-app-boundary-") as temp_dir:
            fixture = Path(temp_dir)
            write_self_test_fixture(fixture)
            errors = validate(fixture)
            provider_rejected = any("provider token" in error for error in errors)
            legacy_rejected = any("legacy api.rs" in error for error in errors)
            compatibility_rejected = any("compatibility facade" in error for error in errors)
            if not provider_rejected or not legacy_rejected or not compatibility_rejected:
                print("negative Worker application-boundary fixture unexpectedly passed")
                for error in errors:
                    print(error)
                return 1
            print("negative provider, legacy and compatibility fixtures rejected as expected")
            return 0

    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("client and identity Worker application boundaries: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
