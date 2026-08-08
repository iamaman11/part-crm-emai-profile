#!/usr/bin/env python3
"""Fail closed if the migrated client Worker surface regains D1 orchestration."""

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

FORBIDDEN_LEGACY_API_TOKENS = (
    "CreateClientMutation",
    "CLIENT_CREATE_COMMAND",
    "async fn create_client(",
    "async fn get_client(",
    "struct ClientCreateRequest",
    "struct ClientResponse",
    "RouteClass::ClientGrantApi",
    "ClientGrantMutation",
    "ClientGrantValue",
    "CLIENT_GRANT_COMMAND",
    "CLIENT_GRANT_REVOKE_COMMAND",
    "async fn update_client_grant(",
    "struct ClientGrantRequest",
)

REQUIRED_CLIENT_TRANSPORT_TOKENS = (
    "execute_create_client",
    "get_visible_client",
    "execute_client_grant",
    "authorize_client_grant",
    "RouteClass::ClientGrantApi",
    "client_application(env)",
)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"cannot read {path}: {exc}") from exc


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    worker = root / "apps/control-plane-worker/src"
    client_path = worker / "clients.rs"
    composition_path = worker / "composition.rs"
    lib_path = worker / "lib.rs"
    legacy_api_path = worker / "api.rs"
    ports_path = root / "crates/application-ports/src/clients.rs"
    use_cases_path = root / "crates/use-cases/src/clients.rs"
    grant_use_cases_path = root / "crates/use-cases/src/client_grants.rs"
    adapter_path = root / "crates/cloudflare-adapters/src/d1_clients.rs"

    required_paths = (
        client_path,
        composition_path,
        lib_path,
        legacy_api_path,
        ports_path,
        use_cases_path,
        grant_use_cases_path,
        adapter_path,
    )
    for path in required_paths:
        if not path.is_file():
            errors.append(f"missing application-boundary file: {path.relative_to(root)}")
    if errors:
        return errors

    client = read(client_path)
    composition = read(composition_path)
    worker_lib = read(lib_path)
    legacy_api = read(legacy_api_path)
    ports = read(ports_path)
    use_cases = read(use_cases_path)
    grant_use_cases = read(grant_use_cases_path)
    adapter = read(adapter_path)

    for token in FORBIDDEN_CLIENT_TRANSPORT_TOKENS:
        if token in client:
            errors.append(f"client Worker transport must not contain provider token `{token}`")

    for token in FORBIDDEN_LEGACY_API_TOKENS:
        if token in legacy_api:
            errors.append(f"legacy api.rs must not contain migrated client token `{token}`")

    for token in REQUIRED_CLIENT_TRANSPORT_TOKENS:
        if token not in client:
            errors.append(f"client Worker transport missing application call token `{token}`")

    for route_token in (
        "RouteClass::ClientCollectionApi",
        "RouteClass::ClientResourceApi",
        "RouteClass::ClientGrantApi",
        "clients::dispatch(route, &mut request, &env).await",
    ):
        if route_token not in worker_lib:
            errors.append(f"Worker composition root missing client route token `{route_token}`")

    if (
        "D1ClientApplicationRepository" not in composition
        or "env.d1(D1_CATALOG_BINDING)?" not in composition
    ):
        errors.append("Worker composition root must construct the D1 client application adapter")

    for token in (
        "pub trait ClientApplicationPort",
        "pub trait ClientGrantApplicationPort",
        "pub struct ClientGrantWrite",
    ):
        if token not in ports:
            errors.append(f"application client ports missing `{token}`")

    if (
        "pub async fn execute_create_client" not in use_cases
        or "pub async fn get_visible_client" not in use_cases
    ):
        errors.append("client use cases must own create/query orchestration")
    for token in (
        "pub async fn execute_client_grant",
        "pub fn authorize_client_grant",
        "pub fn next_client_grant_version",
        "decide_client_grant_replay",
    ):
        if token not in grant_use_cases:
            errors.append(f"client grant use cases missing `{token}`")

    if "impl ClientApplicationPort for D1ClientApplicationRepository" not in adapter:
        errors.append("Cloudflare adapter must implement the inward client application port")
    if "impl ClientGrantApplicationPort for D1ClientApplicationRepository" not in adapter:
        errors.append("Cloudflare adapter must implement the inward client grant port")
    for token in (
        "ClientGrantMutation",
        ".grant_client(actor, mutation)",
        ".revoke_client_grant(actor, mutation)",
    ):
        if token not in adapter:
            errors.append(f"Cloudflare client adapter missing grant mapping token `{token}`")

    return errors


def write_self_test_fixture(root: Path) -> None:
    worker = root / "apps/control-plane-worker/src"
    ports = root / "crates/application-ports/src"
    use_cases = root / "crates/use-cases/src"
    adapters = root / "crates/cloudflare-adapters/src"
    for path in (worker, ports, use_cases, adapters):
        path.mkdir(parents=True, exist_ok=True)

    (worker / "clients.rs").write_text(
        "use cloudflare_adapters::d1_catalog::D1CatalogRepository;\n"
        "fn route() { execute_create_client(); get_visible_client(); execute_client_grant(); "
        "authorize_client_grant(); RouteClass::ClientGrantApi; client_application(env); "
        "ClientGrantMutation; }\n",
        encoding="utf-8",
    )
    (worker / "composition.rs").write_text(
        "D1ClientApplicationRepository env.d1(D1_CATALOG_BINDING)?\n",
        encoding="utf-8",
    )
    (worker / "lib.rs").write_text(
        "RouteClass::ClientCollectionApi RouteClass::ClientResourceApi "
        "RouteClass::ClientGrantApi clients::dispatch(route, &mut request, &env).await\n",
        encoding="utf-8",
    )
    (worker / "api.rs").write_text(
        "RouteClass::ClientGrantApi async fn update_client_grant() {} "
        "struct ClientGrantRequest; ClientGrantMutation; CLIENT_GRANT_COMMAND;\n",
        encoding="utf-8",
    )
    (ports / "clients.rs").write_text(
        "pub trait ClientApplicationPort {}\n"
        "pub trait ClientGrantApplicationPort {}\n"
        "pub struct ClientGrantWrite;\n",
        encoding="utf-8",
    )
    (use_cases / "clients.rs").write_text(
        "pub async fn execute_create_client() {}\npub async fn get_visible_client() {}\n",
        encoding="utf-8",
    )
    (use_cases / "client_grants.rs").write_text(
        "pub async fn execute_client_grant() {}\n"
        "pub fn authorize_client_grant() {}\n"
        "pub fn next_client_grant_version() {}\n"
        "fn replay() { decide_client_grant_replay(); }\n",
        encoding="utf-8",
    )
    (adapters / "d1_clients.rs").write_text(
        "impl ClientApplicationPort for D1ClientApplicationRepository {}\n"
        "impl ClientGrantApplicationPort for D1ClientApplicationRepository {}\n"
        "fn grant() { ClientGrantMutation; repo.grant_client(actor, mutation); "
        "repo.revoke_client_grant(actor, mutation); }\n",
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
            if not provider_rejected or not legacy_rejected:
                print("negative Worker application-boundary fixture unexpectedly passed")
                for error in errors:
                    print(error)
                return 1
            print("negative direct-D1 and legacy Worker fixtures rejected as expected")
            return 0

    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("client Worker application boundary: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
