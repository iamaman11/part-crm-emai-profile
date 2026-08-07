#!/usr/bin/env python3
"""Fail closed if the migrated profile Worker transport regains D1 orchestration."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path


FORBIDDEN_PROFILE_TRANSPORT_TOKENS = (
    "cloudflare_adapters::d1_",
    "D1GovernedCommandRepository",
    "D1IdentityQueryRepository",
    "D1IdempotencyRepository",
    "CreateProfileMutation",
    "D1Database",
)

REQUIRED_PROFILE_TRANSPORT_TOKENS = (
    "execute_create_profile",
    "get_visible_profile",
    "profile_application(env)",
)

LEGACY_PROFILE_API_TOKENS = (
    "async fn create_profile(",
    "async fn get_profile(",
    "struct ProfileCreateRequest",
    "struct ProfileResponse",
)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"cannot read {path}: {exc}") from exc


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    worker = root / "apps/control-plane-worker/src"
    profile_path = worker / "profiles.rs"
    composition_path = worker / "composition.rs"
    lib_path = worker / "lib.rs"
    legacy_api_path = worker / "api.rs"
    ports_path = root / "crates/application-ports/src/profiles.rs"
    use_cases_path = root / "crates/use-cases/src/profiles.rs"
    adapter_path = root / "crates/cloudflare-adapters/src/d1_profiles.rs"

    required_paths = (
        profile_path,
        composition_path,
        lib_path,
        legacy_api_path,
        ports_path,
        use_cases_path,
        adapter_path,
    )
    for path in required_paths:
        if not path.is_file():
            errors.append(f"missing profile application-boundary file: {path.relative_to(root)}")
    if errors:
        return errors

    transport = read(profile_path)
    composition = read(composition_path)
    worker_lib = read(lib_path)
    legacy_api = read(legacy_api_path)
    ports = read(ports_path)
    use_cases = read(use_cases_path)
    adapter = read(adapter_path)

    for token in FORBIDDEN_PROFILE_TRANSPORT_TOKENS:
        if token in transport:
            errors.append(f"profile Worker transport must not contain provider token `{token}`")

    for token in REQUIRED_PROFILE_TRANSPORT_TOKENS:
        if token not in transport:
            errors.append(f"profile Worker transport missing application call token `{token}`")

    route_fragment = "RouteClass::ProfileCollectionApi | RouteClass::ProfileResourceApi"
    if route_fragment not in worker_lib or "profiles::dispatch(route, &mut request, &env).await" not in worker_lib:
        errors.append("Worker composition root must route profile collection/resource APIs to profiles::dispatch")

    if "D1ProfileApplicationRepository" not in composition or "env.d1(D1_CATALOG_BINDING)?" not in composition:
        errors.append("Worker composition root must construct the D1 profile application adapter")

    if "pub trait ProfileApplicationPort" not in ports:
        errors.append("application ports must own ProfileApplicationPort")
    if "pub async fn execute_create_profile" not in use_cases or "pub async fn get_visible_profile" not in use_cases:
        errors.append("profile use cases must own create/query orchestration")
    if "impl ProfileApplicationPort for D1ProfileApplicationRepository" not in adapter:
        errors.append("Cloudflare adapter must implement the inward profile application port")

    for token in LEGACY_PROFILE_API_TOKENS:
        if token in legacy_api:
            errors.append(f"legacy profile create/query implementation remains in api.rs: `{token}`")

    return errors


def write_self_test_fixture(root: Path) -> None:
    worker = root / "apps/control-plane-worker/src"
    ports = root / "crates/application-ports/src"
    use_cases = root / "crates/use-cases/src"
    adapters = root / "crates/cloudflare-adapters/src"
    for path in (worker, ports, use_cases, adapters):
        path.mkdir(parents=True, exist_ok=True)

    (worker / "profiles.rs").write_text(
        "use cloudflare_adapters::d1_identity_queries::D1IdentityQueryRepository;\n"
        "fn route() { execute_create_profile(); get_visible_profile(); profile_application(env); }\n",
        encoding="utf-8",
    )
    (worker / "composition.rs").write_text(
        "D1ProfileApplicationRepository env.d1(D1_CATALOG_BINDING)?\n",
        encoding="utf-8",
    )
    (worker / "lib.rs").write_text(
        "RouteClass::ProfileCollectionApi | RouteClass::ProfileResourceApi => "
        "profiles::dispatch(route, &mut request, &env).await\n",
        encoding="utf-8",
    )
    (worker / "api.rs").write_text("async fn create_profile() {}\n", encoding="utf-8")
    (ports / "profiles.rs").write_text("pub trait ProfileApplicationPort {}\n", encoding="utf-8")
    (use_cases / "profiles.rs").write_text(
        "pub async fn execute_create_profile() {}\npub async fn get_visible_profile() {}\n",
        encoding="utf-8",
    )
    (adapters / "d1_profiles.rs").write_text(
        "impl ProfileApplicationPort for D1ProfileApplicationRepository {}\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="profile-worker-app-boundary-") as temp_dir:
            fixture = Path(temp_dir)
            write_self_test_fixture(fixture)
            errors = validate(fixture)
            has_provider_rejection = any("provider token" in error for error in errors)
            has_legacy_rejection = any("legacy profile" in error for error in errors)
            if not (has_provider_rejection and has_legacy_rejection):
                print("negative direct-D1 and legacy profile fixtures unexpectedly passed")
                for error in errors:
                    print(error)
                return 1
            print("negative direct-D1 and legacy profile fixtures rejected as expected")
            return 0

    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("profile Worker application boundary: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
