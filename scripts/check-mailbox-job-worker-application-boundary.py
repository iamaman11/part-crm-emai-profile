#!/usr/bin/env python3
"""Fail closed if mailbox job transport regains D1 or application-owned provider decisions."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

FORBIDDEN_JOB_TRANSPORT_TOKENS = (
    "cloudflare_adapters::d1_",
    "D1MailboxRepository",
    "D1IdempotencyRepository",
    "CreateMailboxJobMutation",
    "RunMailboxJobMutation",
    "MutationEnvelope",
    "MetadataMailboxProviderAdapter",
    "decide_mailbox_run",
    "D1Database",
)

REQUIRED_JOB_TRANSPORT_TOKENS = (
    "execute_create_mailbox_job",
    "get_mailbox_job",
    "execute_run_mailbox_job",
    "mailbox_job_application(env)",
    "validate_create_mailbox_job_request",
    "validate_mailbox_job_run_version",
    "CloudMailboxProviderRouter::new(env)",
)

REQUIRED_USE_CASE_TOKENS = (
    "pub async fn execute_create_mailbox_job",
    "pub async fn get_mailbox_job",
    "pub async fn execute_run_mailbox_job",
    "pub fn validate_create_mailbox_job_request",
    "pub fn validate_mailbox_job_run_version",
)

REQUIRED_ADAPTER_TOKENS = (
    "impl MailboxJobApplicationPort for D1MailboxJobApplicationRepository",
    "CreateMailboxJobMutation",
    "RunMailboxJobMutation",
)

FORBIDDEN_ADAPTER_TOKENS = (
    "MetadataMailboxProviderAdapter",
    "decide_mailbox_run",
    "type RunDecision",
)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"cannot read {path}: {exc}") from exc


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    worker = root / "apps/control-plane-worker/src"
    job_path = worker / "mailbox_jobs.rs"
    composition_path = worker / "composition.rs"
    lib_path = worker / "lib.rs"
    ports_path = root / "crates/application-ports/src/mailbox_jobs.rs"
    use_cases_path = root / "crates/use-cases-mailboxes/src/mailbox_jobs.rs"
    adapter_path = root / "crates/cloudflare-adapters/src/d1_mailbox_jobs.rs"

    for path in (job_path, composition_path, lib_path, ports_path, use_cases_path, adapter_path):
        if not path.is_file():
            errors.append(f"missing mailbox job application-boundary file: {path.relative_to(root)}")
    if errors:
        return errors

    job = read(job_path)
    composition = read(composition_path)
    worker_lib = read(lib_path)
    ports = read(ports_path)
    use_cases = read(use_cases_path)
    adapter = read(adapter_path)

    for token in FORBIDDEN_JOB_TRANSPORT_TOKENS:
        if token in job:
            errors.append(f"mailbox job Worker transport must not contain provider token `{token}`")

    for token in REQUIRED_JOB_TRANSPORT_TOKENS:
        if token not in job:
            errors.append(f"mailbox job Worker transport missing application token `{token}`")

    route_fragment = (
        "RouteClass::MailboxJobCollectionApi\n"
        "        | RouteClass::MailboxJobResourceApi\n"
        "        | RouteClass::MailboxJobRunApi"
    )
    if (
        route_fragment not in worker_lib
        or "mailbox_jobs::dispatch(route, &mut request, &env).await" not in worker_lib
    ):
        errors.append("Worker root must route mailbox job APIs to mailbox_jobs::dispatch")

    if (
        "D1MailboxJobApplicationRepository" not in composition
        or "mailbox_job_application" not in composition
        or "env.d1(D1_CATALOG_BINDING)?" not in composition
    ):
        errors.append("Worker composition root must construct the D1 mailbox job application adapter")

    if "pub trait MailboxJobApplicationPort" not in ports:
        errors.append("application ports must own MailboxJobApplicationPort")
    if "MailboxJobPreparedRun" not in ports:
        errors.append("mailbox job application port must expose only the prepared canonical run write")
    if "type RunDecision" in ports:
        errors.append("provider run decisions must not be owned by the persistence application port")

    for token in REQUIRED_USE_CASE_TOKENS:
        if token not in use_cases:
            errors.append(f"extracted mailbox job use cases missing `{token}`")

    for token in REQUIRED_ADAPTER_TOKENS:
        if token not in adapter:
            errors.append(f"Cloudflare mailbox job persistence adapter missing `{token}`")
    for token in FORBIDDEN_ADAPTER_TOKENS:
        if token in adapter:
            errors.append(
                f"Cloudflare mailbox job persistence adapter must not decide provider outcome `{token}`"
            )

    return errors


def write_self_test_fixture(root: Path) -> None:
    worker = root / "apps/control-plane-worker/src"
    ports = root / "crates/application-ports/src"
    use_cases = root / "crates/use-cases-mailboxes/src"
    adapters = root / "crates/cloudflare-adapters/src"
    for path in (worker, ports, use_cases, adapters):
        path.mkdir(parents=True, exist_ok=True)

    (worker / "mailbox_jobs.rs").write_text(
        "use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;\n"
        "MetadataMailboxProviderAdapter decide_mailbox_run\n"
        "fn route() { execute_create_mailbox_job(); get_mailbox_job(); execute_run_mailbox_job(); "
        "mailbox_job_application(env); validate_create_mailbox_job_request(); "
        "validate_mailbox_job_run_version(); CloudMailboxProviderRouter::new(env); }\n",
        encoding="utf-8",
    )
    (worker / "composition.rs").write_text(
        "D1MailboxJobApplicationRepository mailbox_job_application env.d1(D1_CATALOG_BINDING)?\n",
        encoding="utf-8",
    )
    (worker / "lib.rs").write_text(
        "RouteClass::MailboxJobCollectionApi\n"
        "        | RouteClass::MailboxJobResourceApi\n"
        "        | RouteClass::MailboxJobRunApi => "
        "mailbox_jobs::dispatch(route, &mut request, &env).await\n",
        encoding="utf-8",
    )
    (ports / "mailbox_jobs.rs").write_text(
        "pub struct MailboxJobPreparedRun;\n"
        "pub trait MailboxJobApplicationPort {}\n",
        encoding="utf-8",
    )
    (use_cases / "mailbox_jobs.rs").write_text(
        "pub async fn execute_create_mailbox_job() {}\n"
        "pub async fn get_mailbox_job() {}\n"
        "pub async fn execute_run_mailbox_job() {}\n"
        "pub fn validate_create_mailbox_job_request() {}\n"
        "pub fn validate_mailbox_job_run_version() {}\n",
        encoding="utf-8",
    )
    (adapters / "d1_mailbox_jobs.rs").write_text(
        "impl MailboxJobApplicationPort for D1MailboxJobApplicationRepository {}\n"
        "CreateMailboxJobMutation RunMailboxJobMutation\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="mailbox-job-boundary-") as temp_dir:
            fixture = Path(temp_dir)
            write_self_test_fixture(fixture)
            errors = validate(fixture)
            direct_d1 = any("D1MailboxRepository" in error for error in errors)
            provider = any("MetadataMailboxProviderAdapter" in error for error in errors)
            if not (direct_d1 and provider):
                print("negative direct-D1/provider mailbox job fixture unexpectedly passed")
                for error in errors:
                    print(error)
                return 1
            print("negative direct-D1/provider mailbox job fixture rejected as expected")
            return 0

    errors = validate(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("mailbox job Worker application boundary: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
