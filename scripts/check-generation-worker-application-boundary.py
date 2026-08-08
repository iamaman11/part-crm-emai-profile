#!/usr/bin/env python3
"""Fail closed if profile generation Worker transport owns D1/idempotency orchestration."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

FORBIDDEN_TRANSPORT_TOKENS = (
    "cloudflare_adapters::d1_",
    "D1ProfileGenerationRepository",
    "D1IdempotencyRepository",
    "RegisterGenerationMutation",
    "VerifyGenerationMutation",
    "ActivateGenerationMutation",
    "DeactivateGenerationMutation",
    "QuarantineGenerationMutation",
    "MutationEnvelope",
    "D1Database",
)

REQUIRED_TRANSPORT_TOKENS = (
    "execute_register_generation",
    "get_visible_generation",
    "execute_verify_generation",
    "execute_activate_generation",
    "execute_deactivate_generation",
    "execute_quarantine_generation",
    "profile_generation_application(env)",
    "validate_generation_registration",
    "validate_generation_verification_reference",
    "next_generation_version",
)

REQUIRED_USE_CASE_TOKENS = (
    "pub async fn execute_register_generation",
    "pub async fn get_visible_generation",
    "pub async fn execute_verify_generation",
    "pub async fn execute_activate_generation",
    "pub async fn execute_deactivate_generation",
    "pub async fn execute_quarantine_generation",
    "pub fn validate_generation_registration",
    "pub fn validate_generation_verification_reference",
    "pub fn next_generation_version",
)

REQUIRED_ADAPTER_TOKENS = (
    "impl GenerationApplicationPort for D1ProfileGenerationApplicationRepository",
    "D1ProfileGenerationRepository",
    "D1IdempotencyRepository",
    "RegisterGenerationMutation",
    "VerifyGenerationMutation",
    "ActivateGenerationMutation",
    "DeactivateGenerationMutation",
    "QuarantineGenerationMutation",
)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"cannot read {path}: {exc}") from exc


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    worker = root / "apps/control-plane-worker/src"
    transport_path = worker / "profile_generations.rs"
    composition_path = worker / "composition.rs"
    lib_path = worker / "lib.rs"
    ports_path = root / "crates/application-ports/src/generations.rs"
    use_cases_path = root / "crates/use-cases/src/generations.rs"
    adapter_path = root / "crates/cloudflare-adapters/src/d1_profile_generation_application.rs"

    for path in (
        transport_path,
        composition_path,
        lib_path,
        ports_path,
        use_cases_path,
        adapter_path,
    ):
        if not path.is_file():
            errors.append(f"missing generation application-boundary file: {path.relative_to(root)}")
    if errors:
        return errors

    transport = read(transport_path)
    composition = read(composition_path)
    worker_lib = read(lib_path)
    ports = read(ports_path)
    use_cases = read(use_cases_path)
    adapter = read(adapter_path)

    for token in FORBIDDEN_TRANSPORT_TOKENS:
        if token in transport:
            errors.append(f"generation Worker transport must not contain provider token `{token}`")

    for token in REQUIRED_TRANSPORT_TOKENS:
        if token not in transport:
            errors.append(f"generation Worker transport missing application token `{token}`")

    route_tokens = (
        "RouteClass::ProfileGenerationCollectionApi",
        "RouteClass::ProfileGenerationResourceApi",
        "RouteClass::ProfileGenerationVerifyApi",
        "RouteClass::ProfileGenerationActivateApi",
        "RouteClass::ProfileGenerationDeactivateApi",
        "RouteClass::ProfileGenerationQuarantineApi",
        "profile_generations::dispatch(route, &mut request, &env).await",
    )
    for token in route_tokens:
        if token not in worker_lib:
            errors.append(f"Worker root missing generation route token `{token}`")

    if (
        "D1ProfileGenerationApplicationRepository" not in composition
        or "profile_generation_application" not in composition
        or "env.d1(D1_CATALOG_BINDING)?" not in composition
    ):
        errors.append("Worker composition root must construct the profile generation application adapter")

    if "pub trait GenerationApplicationPort" not in ports:
        errors.append("application ports must own GenerationApplicationPort")

    for token in REQUIRED_USE_CASE_TOKENS:
        if token not in use_cases:
            errors.append(f"generation use cases missing `{token}`")

    for token in REQUIRED_ADAPTER_TOKENS:
        if token not in adapter:
            errors.append(f"Cloudflare generation application adapter missing `{token}`")

    return errors


def write_self_test_fixture(root: Path) -> None:
    worker = root / "apps/control-plane-worker/src"
    ports = root / "crates/application-ports/src"
    use_cases = root / "crates/use-cases/src"
    adapters = root / "crates/cloudflare-adapters/src"
    for path in (worker, ports, use_cases, adapters):
        path.mkdir(parents=True, exist_ok=True)

    (worker / "profile_generations.rs").write_text(
        "use cloudflare_adapters::d1_profile_generations::D1ProfileGenerationRepository;\n"
        "RegisterGenerationMutation D1IdempotencyRepository\n"
        "fn route() { execute_register_generation(); get_visible_generation(); "
        "execute_verify_generation(); execute_activate_generation(); "
        "execute_deactivate_generation(); execute_quarantine_generation(); "
        "profile_generation_application(env); validate_generation_registration(); "
        "validate_generation_verification_reference(); next_generation_version(); }\n",
        encoding="utf-8",
    )
    (worker / "composition.rs").write_text(
        "D1ProfileGenerationApplicationRepository profile_generation_application "
        "env.d1(D1_CATALOG_BINDING)?\n",
        encoding="utf-8",
    )
    (worker / "lib.rs").write_text(
        "RouteClass::ProfileGenerationCollectionApi ProfileGenerationResourceApi "
        "ProfileGenerationVerifyApi ProfileGenerationActivateApi "
        "ProfileGenerationDeactivateApi ProfileGenerationQuarantineApi "
        "profile_generations::dispatch(route, &mut request, &env).await\n",
        encoding="utf-8",
    )
    (ports / "generations.rs").write_text(
        "pub trait GenerationApplicationPort {}\n",
        encoding="utf-8",
    )
    (use_cases / "generations.rs").write_text(
        "pub async fn execute_register_generation() {}\n"
        "pub async fn get_visible_generation() {}\n"
        "pub async fn execute_verify_generation() {}\n"
        "pub async fn execute_activate_generation() {}\n"
        "pub async fn execute_deactivate_generation() {}\n"
        "pub async fn execute_quarantine_generation() {}\n"
        "pub fn validate_generation_registration() {}\n"
        "pub fn validate_generation_verification_reference() {}\n"
        "pub fn next_generation_version() {}\n",
        encoding="utf-8",
    )
    (adapters / "d1_profile_generation_application.rs").write_text(
        "impl GenerationApplicationPort for D1ProfileGenerationApplicationRepository {}\n"
        "D1ProfileGenerationRepository D1IdempotencyRepository RegisterGenerationMutation "
        "VerifyGenerationMutation ActivateGenerationMutation DeactivateGenerationMutation "
        "QuarantineGenerationMutation\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="generation-boundary-") as temp_dir:
            fixture = Path(temp_dir)
            write_self_test_fixture(fixture)
            errors = validate(fixture)
            direct_d1 = any("D1ProfileGenerationRepository" in error for error in errors)
            mutation = any("RegisterGenerationMutation" in error for error in errors)
            idempotency = any("D1IdempotencyRepository" in error for error in errors)
            if not (direct_d1 and mutation and idempotency):
                print("negative direct-D1 generation fixture unexpectedly passed")
                for error in errors:
                    print(error)
                return 1
            print("negative direct-D1 generation fixture rejected as expected")
            return 0

    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("profile generation Worker application boundary: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
