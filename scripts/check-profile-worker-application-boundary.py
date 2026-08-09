#!/usr/bin/env python3
"""Fail closed if migrated profile Worker transports regain D1 orchestration."""

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
    "AssignProfileMutation",
    "ProfileGrantMutation",
    "D1Database",
)

REQUIRED_PROFILE_TRANSPORT_TOKENS = (
    "execute_create_profile",
    "get_visible_profile",
    "execute_assign_profile",
    "authorize_profile_assignment",
    "execute_profile_grant",
    "authorize_profile_grant",
    "profile_application(env)",
)

LEGACY_PROFILE_API_TOKENS = (
    "RouteClass::ProfileAssignmentApi",
    "RouteClass::ProfileGrantApi",
    "async fn create_profile(",
    "async fn get_profile(",
    "async fn assign_profile(",
    "async fn update_profile_grant(",
    "struct ProfileCreateRequest",
    "struct ProfileResponse",
    "struct AssignmentRequest",
    "struct ProfileGrantRequest",
    "AssignProfileMutation",
    "ProfileGrantMutation",
    "ProfileGrantValue",
    "PROFILE_GRANT_COMMAND",
    "PROFILE_GRANT_REVOKE_COMMAND",
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
    context_ports_path = root / "crates/application-ports/src/profile_assignment_context.rs"
    use_cases_path = root / "crates/use-cases/src/profiles.rs"
    assignment_use_cases_path = root / "crates/use-cases/src/profile_assignments.rs"
    grant_use_cases_path = root / "crates/use-cases/src/profile_grants.rs"
    adapter_path = root / "crates/cloudflare-adapters/src/d1_profiles.rs"
    bundle_adapter_path = root / "crates/cloudflare-adapters/src/d1_profile_application.rs"

    required_paths = (
        profile_path,
        composition_path,
        lib_path,
        ports_path,
        context_ports_path,
        use_cases_path,
        assignment_use_cases_path,
        grant_use_cases_path,
        adapter_path,
        bundle_adapter_path,
    )
    for path in required_paths:
        if not path.is_file():
            errors.append(f"missing profile application-boundary file: {path.relative_to(root)}")
    if errors:
        return errors

    transport = read(profile_path)
    composition = read(composition_path)
    worker_lib = read(lib_path)
    legacy_api = read(legacy_api_path) if legacy_api_path.is_file() else ""
    ports = read(ports_path)
    context_ports = read(context_ports_path)
    use_cases = read(use_cases_path)
    assignment_use_cases = read(assignment_use_cases_path)
    grant_use_cases = read(grant_use_cases_path)
    adapter = read(adapter_path)
    bundle_adapter = read(bundle_adapter_path)

    for token in FORBIDDEN_PROFILE_TRANSPORT_TOKENS:
        if token in transport:
            errors.append(f"profile Worker transport must not contain provider token `{token}`")

    for token in REQUIRED_PROFILE_TRANSPORT_TOKENS:
        if token not in transport:
            errors.append(f"profile Worker transport missing application call token `{token}`")

    for route_token in (
        "RouteClass::ProfileCollectionApi",
        "RouteClass::ProfileResourceApi",
        "RouteClass::ProfileAssignmentApi",
        "RouteClass::ProfileGrantApi",
        "profiles::dispatch(route, &mut request, &env).await",
    ):
        if route_token not in worker_lib:
            errors.append(f"Worker composition root missing profile route token `{route_token}`")

    if (
        "D1ProfileApplicationBundle" not in composition
        or "env.d1(D1_CATALOG_BINDING)?" not in composition
    ):
        errors.append("Worker composition root must construct the composed D1 profile application adapter")

    for token in (
        "pub trait ProfileApplicationPort",
        "pub trait ProfileAssignmentApplicationPort",
        "pub struct ProfileAssignmentWrite",
        "pub trait ProfileGrantApplicationPort",
        "pub struct ProfileGrantWrite",
    ):
        if token not in ports:
            errors.append(f"application profile ports missing `{token}`")
    for token in (
        "pub trait ProfileAssignmentContextPort",
        "pub struct ProfileAssignmentContext",
        "pub struct CurrentProfileAssignmentSnapshot",
    ):
        if token not in context_ports:
            errors.append(f"application assignment-context port missing `{token}`")

    if (
        "pub async fn execute_create_profile" not in use_cases
        or "pub async fn get_visible_profile" not in use_cases
    ):
        errors.append("profile use cases must own create/query orchestration")
    for token in (
        "pub async fn execute_assign_profile",
        "pub fn authorize_profile_assignment",
        "pub fn next_profile_assignment_version",
        "decide_assignment_replay",
        "load_profile_assignment_context",
        "plan_primary_reassignment",
    ):
        if token not in assignment_use_cases:
            errors.append(f"profile assignment use cases missing `{token}`")
    for token in (
        "pub async fn execute_profile_grant",
        "pub fn authorize_profile_grant",
        "pub fn next_profile_grant_version",
        "decide_profile_grant_replay",
    ):
        if token not in grant_use_cases:
            errors.append(f"profile grant use cases missing `{token}`")

    if "impl ProfileApplicationPort for D1ProfileApplicationRepository" not in adapter:
        errors.append("Cloudflare adapter must implement the inward profile application port")
    if "impl ProfileAssignmentApplicationPort for D1ProfileApplicationRepository" not in adapter:
        errors.append("Cloudflare adapter must implement the inward profile assignment port")
    if "impl ProfileGrantApplicationPort for D1ProfileApplicationRepository" not in adapter:
        errors.append("Cloudflare adapter must implement the inward profile grant port")
    if "AssignProfileMutation" not in adapter or ".assign_profile(actor, mutation)" not in adapter:
        errors.append("Cloudflare profile adapter must own the atomic assignment mutation mapping")
    for token in (
        "ProfileGrantMutation",
        ".grant_profile(actor, mutation)",
        ".revoke_profile_grant(actor, mutation)",
    ):
        if token not in adapter:
            errors.append(f"Cloudflare profile adapter missing grant mapping token `{token}`")

    for token in (
        "impl ProfileApplicationPort for D1ProfileApplicationBundle",
        "impl ProfileAssignmentApplicationPort for D1ProfileApplicationBundle",
        "impl ProfileGrantApplicationPort for D1ProfileApplicationBundle",
        "impl ProfileAssignmentContextPort for D1ProfileApplicationBundle",
        "profile.tenant_id = ?",
        "assignment.closed_at_ms IS NULL",
        "JOIN clients AS target",
        "LEFT JOIN clients AS current_client",
    ):
        if token not in bundle_adapter:
            errors.append(f"composed profile adapter missing assignment-context token `{token}`")
    if "profile_grants" in bundle_adapter or "client_grants" in bundle_adapter:
        errors.append("assignment-context query must not derive authorization from grants")

    for token in LEGACY_PROFILE_API_TOKENS:
        if token in legacy_api:
            errors.append(f"legacy migrated profile implementation remains in api.rs: `{token}`")

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
        "fn route() { execute_create_profile(); get_visible_profile(); execute_assign_profile(); "
        "authorize_profile_assignment(); execute_profile_grant(); authorize_profile_grant(); "
        "profile_application(env); ProfileGrantMutation; }\n",
        encoding="utf-8",
    )
    (worker / "composition.rs").write_text(
        "D1ProfileApplicationBundle env.d1(D1_CATALOG_BINDING)?\n",
        encoding="utf-8",
    )
    (worker / "lib.rs").write_text(
        "RouteClass::ProfileCollectionApi RouteClass::ProfileResourceApi "
        "RouteClass::ProfileAssignmentApi RouteClass::ProfileGrantApi "
        "profiles::dispatch(route, &mut request, &env).await\n",
        encoding="utf-8",
    )
    (worker / "api.rs").write_text(
        "RouteClass::ProfileGrantApi async fn update_profile_grant() {} "
        "struct ProfileGrantRequest; ProfileGrantMutation; PROFILE_GRANT_COMMAND;\n",
        encoding="utf-8",
    )
    (ports / "profiles.rs").write_text(
        "pub trait ProfileApplicationPort {}\n"
        "pub trait ProfileAssignmentApplicationPort {}\n"
        "pub struct ProfileAssignmentWrite;\n"
        "pub trait ProfileGrantApplicationPort {}\n"
        "pub struct ProfileGrantWrite;\n",
        encoding="utf-8",
    )
    (ports / "profile_assignment_context.rs").write_text(
        "pub trait ProfileAssignmentContextPort {}\n"
        "pub struct ProfileAssignmentContext;\n"
        "pub struct CurrentProfileAssignmentSnapshot;\n",
        encoding="utf-8",
    )
    (use_cases / "profiles.rs").write_text(
        "pub async fn execute_create_profile() {}\npub async fn get_visible_profile() {}\n",
        encoding="utf-8",
    )
    (use_cases / "profile_assignments.rs").write_text(
        "pub async fn execute_assign_profile() { decide_assignment_replay(); "
        "load_profile_assignment_context(); plan_primary_reassignment(); }\n"
        "pub fn authorize_profile_assignment() {}\n"
        "pub fn next_profile_assignment_version() {}\n",
        encoding="utf-8",
    )
    (use_cases / "profile_grants.rs").write_text(
        "pub async fn execute_profile_grant() {}\n"
        "pub fn authorize_profile_grant() {}\n"
        "pub fn next_profile_grant_version() {}\n"
        "fn replay() { decide_profile_grant_replay(); }\n",
        encoding="utf-8",
    )
    (adapters / "d1_profiles.rs").write_text(
        "impl ProfileApplicationPort for D1ProfileApplicationRepository {}\n"
        "impl ProfileAssignmentApplicationPort for D1ProfileApplicationRepository {}\n"
        "impl ProfileGrantApplicationPort for D1ProfileApplicationRepository {}\n"
        "fn write() { AssignProfileMutation; repo.assign_profile(actor, mutation); "
        "ProfileGrantMutation; repo.grant_profile(actor, mutation); "
        "repo.revoke_profile_grant(actor, mutation); }\n",
        encoding="utf-8",
    )
    (adapters / "d1_profile_application.rs").write_text(
        "impl ProfileApplicationPort for D1ProfileApplicationBundle {}\n"
        "impl ProfileAssignmentApplicationPort for D1ProfileApplicationBundle {}\n"
        "impl ProfileGrantApplicationPort for D1ProfileApplicationBundle {}\n"
        "impl ProfileAssignmentContextPort for D1ProfileApplicationBundle {}\n"
        "JOIN clients AS target LEFT JOIN clients AS current_client "
        "profile.tenant_id = ? assignment.closed_at_ms IS NULL profile_grants\n",
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
            has_legacy_rejection = any("legacy migrated profile" in error for error in errors)
            has_assignment_acl_rejection = any(
                "must not derive authorization from grants" in error for error in errors
            )
            if not (
                has_provider_rejection
                and has_legacy_rejection
                and has_assignment_acl_rejection
            ):
                print("negative direct-D1, legacy or assignment-ACL profile fixtures unexpectedly passed")
                for error in errors:
                    print(error)
                return 1
            print("negative direct-D1, legacy and assignment-ACL profile fixtures rejected as expected")
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
