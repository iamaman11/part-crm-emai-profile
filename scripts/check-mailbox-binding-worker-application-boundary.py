#!/usr/bin/env python3
"""Fail closed if migrated mailbox binding transport regains provider/D1 orchestration."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path


FORBIDDEN_BINDING_TRANSPORT_TOKENS = (
    "cloudflare_adapters::d1_",
    "D1MailboxRepository",
    "D1IdempotencyRepository",
    "CreateMailboxBindingMutation",
    "RevokeMailboxBindingMutation",
    "MutationEnvelope",
    "D1Database",
)

REQUIRED_BINDING_TRANSPORT_TOKENS = (
    "execute_create_mailbox_binding",
    "execute_revoke_mailbox_binding",
    "get_mailbox_binding",
    "mailbox_binding_application(env)",
)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"cannot read {path}: {exc}") from exc


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    worker = root / "apps/control-plane-worker/src"
    binding_path = worker / "mailbox_bindings.rs"
    jobs_path = worker / "mailbox_jobs.rs"
    legacy_path = worker / "mailboxes.rs"
    composition_path = worker / "composition.rs"
    lib_path = worker / "lib.rs"
    ports_path = root / "crates/application-ports/src/mailboxes.rs"
    use_cases_path = root / "crates/use-cases/src/mailboxes.rs"
    adapter_path = root / "crates/cloudflare-adapters/src/d1_mailbox_bindings.rs"

    for path in (
        binding_path,
        jobs_path,
        composition_path,
        lib_path,
        ports_path,
        use_cases_path,
        adapter_path,
    ):
        if not path.is_file():
            errors.append(f"missing mailbox application-boundary file: {path.relative_to(root)}")
    if errors:
        return errors

    if legacy_path.exists():
        errors.append(
            "legacy mixed mailbox Worker transport must be removed: "
            "apps/control-plane-worker/src/mailboxes.rs"
        )

    binding = read(binding_path)
    jobs = read(jobs_path)
    composition = read(composition_path)
    worker_lib = read(lib_path)
    ports = read(ports_path)
    use_cases = read(use_cases_path)
    adapter = read(adapter_path)

    for token in FORBIDDEN_BINDING_TRANSPORT_TOKENS:
        if token in binding:
            errors.append(f"mailbox binding Worker transport must not contain provider token `{token}`")

    for token in REQUIRED_BINDING_TRANSPORT_TOKENS:
        if token not in binding:
            errors.append(f"mailbox binding Worker transport missing application call token `{token}`")

    binding_route_fragment = (
        "RouteClass::MailboxBindingCollectionApi\n"
        "        | RouteClass::MailboxBindingResourceApi\n"
        "        | RouteClass::MailboxBindingRevokeApi"
    )
    if (
        binding_route_fragment not in worker_lib
        or "mailbox_bindings::dispatch(route, &mut request, &env).await" not in worker_lib
    ):
        errors.append("Worker root must route mailbox binding APIs to mailbox_bindings::dispatch")

    job_route_fragment = (
        "RouteClass::MailboxJobCollectionApi\n"
        "        | RouteClass::MailboxJobResourceApi\n"
        "        | RouteClass::MailboxJobRunApi"
    )
    if (
        job_route_fragment not in worker_lib
        or "mailbox_jobs::dispatch(route, &mut request, &env).await" not in worker_lib
    ):
        errors.append("Worker root must route mailbox job APIs to mailbox_jobs::dispatch")

    if "mod mailboxes;" in worker_lib:
        errors.append("Worker root must not retain legacy mixed `mailboxes` module")

    if (
        "D1MailboxBindingApplicationRepository" not in composition
        or "env.d1(D1_CATALOG_BINDING)?" not in composition
    ):
        errors.append("Worker composition root must construct the D1 mailbox binding application adapter")

    if "pub trait MailboxBindingApplicationPort" not in ports:
        errors.append("application ports must own MailboxBindingApplicationPort")
    for symbol in (
        "pub async fn execute_create_mailbox_binding",
        "pub async fn execute_revoke_mailbox_binding",
        "pub async fn get_mailbox_binding",
    ):
        if symbol not in use_cases:
            errors.append(f"mailbox binding use cases missing `{symbol}`")
    if "impl MailboxBindingApplicationPort for D1MailboxBindingApplicationRepository" not in adapter:
        errors.append("Cloudflare adapter must implement the inward mailbox binding application port")

    # The retained jobs transport is intentionally still provider-owned in this Phase 0D slice.
    # Prove it still exists rather than accidentally deleting job execution while cleaning bindings.
    for token in ("create_job", "get_job", "run_job", "MetadataMailboxProviderAdapter"):
        if token not in jobs:
            errors.append(f"retained mailbox job transport missing `{token}`")

    return errors


def write_self_test_fixture(root: Path) -> None:
    worker = root / "apps/control-plane-worker/src"
    ports = root / "crates/application-ports/src"
    use_cases = root / "crates/use-cases/src"
    adapters = root / "crates/cloudflare-adapters/src"
    for path in (worker, ports, use_cases, adapters):
        path.mkdir(parents=True, exist_ok=True)

    (worker / "mailbox_bindings.rs").write_text(
        "use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;\n"
        "fn route() { execute_create_mailbox_binding(); execute_revoke_mailbox_binding(); "
        "get_mailbox_binding(); mailbox_binding_application(env); }\n",
        encoding="utf-8",
    )
    (worker / "mailbox_jobs.rs").write_text(
        "fn create_job() {} fn get_job() {} fn run_job() {} MetadataMailboxProviderAdapter\n",
        encoding="utf-8",
    )
    (worker / "mailboxes.rs").write_text("async fn create_binding() {}\n", encoding="utf-8")
    (worker / "composition.rs").write_text(
        "D1MailboxBindingApplicationRepository env.d1(D1_CATALOG_BINDING)?\n",
        encoding="utf-8",
    )
    (worker / "lib.rs").write_text(
        "mod mailboxes;\n"
        "RouteClass::MailboxBindingCollectionApi\n"
        "        | RouteClass::MailboxBindingResourceApi\n"
        "        | RouteClass::MailboxBindingRevokeApi => "
        "mailbox_bindings::dispatch(route, &mut request, &env).await\n"
        "RouteClass::MailboxJobCollectionApi\n"
        "        | RouteClass::MailboxJobResourceApi\n"
        "        | RouteClass::MailboxJobRunApi => "
        "mailbox_jobs::dispatch(route, &mut request, &env).await\n",
        encoding="utf-8",
    )
    (ports / "mailboxes.rs").write_text("pub trait MailboxBindingApplicationPort {}\n", encoding="utf-8")
    (use_cases / "mailboxes.rs").write_text(
        "pub async fn execute_create_mailbox_binding() {}\n"
        "pub async fn execute_revoke_mailbox_binding() {}\n"
        "pub async fn get_mailbox_binding() {}\n",
        encoding="utf-8",
    )
    (adapters / "d1_mailbox_bindings.rs").write_text(
        "impl MailboxBindingApplicationPort for D1MailboxBindingApplicationRepository {}\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="mailbox-binding-boundary-") as temp_dir:
            fixture = Path(temp_dir)
            write_self_test_fixture(fixture)
            errors = validate(fixture)
            has_provider_rejection = any("provider token" in error for error in errors)
            has_legacy_rejection = any("legacy mixed" in error for error in errors)
            if not (has_provider_rejection and has_legacy_rejection):
                print("negative direct-D1 and legacy mailbox fixture unexpectedly passed")
                for error in errors:
                    print(error)
                return 1
            print("negative direct-D1 and legacy mailbox fixture rejected as expected")
            return 0

    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("mailbox binding Worker application boundary: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
