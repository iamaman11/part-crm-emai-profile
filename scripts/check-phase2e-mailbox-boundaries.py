#!/usr/bin/env python3
"""Enforce Phase 2E cloud mailbox ownership, runtime and privacy boundaries."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SENSITIVE_TERMS = (
    "access_token",
    "refresh_token",
    "password",
    "authorization",
    "message_body",
    "body_html",
    "body_text",
    "raw_message",
    "subject",
    "sender",
    "recipient",
)
CONFIDENTIAL_SINKS = (
    "AuditPort",
    "audit_events",
    "outbox_events",
    "IntegrationEvent",
    "console_",
    "println!(",
    "eprintln!(",
    "dbg!(",
    "tracing::",
    "log::",
    "telemetry",
)
INNER_RUNTIME_FRAGMENTS = (
    "worker::",
    "D1Database",
    "cloudflare",
    "gmail.googleapis.com",
    "Socket::",
    "MAILBOX_SECRET_RESOLVER",
)


def fail(message: str) -> None:
    raise AssertionError(message)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"required Phase 2E file missing: {path}")
    return path.read_text(encoding="utf-8")


def production_source(source: str) -> str:
    """Exclude test-only negative markers from production privacy scans."""
    return source.split("#[cfg(test)]", 1)[0]


def assert_contains(name: str, source: str, fragments: tuple[str, ...]) -> None:
    for fragment in fragments:
        if fragment not in source:
            fail(f"{name} missing required Phase 2E invariant: {fragment}")


def assert_absent(name: str, source: str, fragments: tuple[str, ...]) -> None:
    lowered = source.lower()
    for fragment in fragments:
        if fragment.lower() in lowered:
            fail(f"{name} contains prohibited Phase 2E fragment: {fragment}")


def assert_metadata_only(name: str, source: str) -> None:
    lowered = production_source(source).lower()
    for term in SENSITIVE_TERMS:
        if term in lowered:
            fail(f"{name} must remain metadata-only; found {term}")


def enforce(root: Path) -> None:
    ports = root / "crates" / "application-ports" / "src"
    use_cases = root / "crates" / "use-cases-mailboxes" / "src"
    adapters = root / "crates" / "cloudflare-adapters" / "src"
    worker = root / "apps" / "control-plane-worker" / "src"

    inner_files = (
        root / "crates" / "mailbox-domain" / "src" / "binding.rs",
        root / "crates" / "mailbox-domain" / "src" / "job.rs",
        ports / "mailbox_jobs.rs",
        ports / "mailbox_scheduling.rs",
        ports / "mailboxes.rs",
        use_cases / "mailbox_jobs.rs",
        use_cases / "scheduled.rs",
    )
    for path in inner_files:
        assert_absent(str(path.relative_to(root)), read(path), INNER_RUNTIME_FRAGMENTS)

    scheduling_port = read(ports / "mailbox_scheduling.rs")
    assert_contains(
        "application-ports/mailbox_scheduling.rs",
        scheduling_port,
        (
            "pub struct MailboxJobDispatch",
            "pub trait MailboxSchedulingRepositoryPort",
            "pub trait MailboxDispatchPublisherPort",
            "MailboxExecutionClaimOutcome",
            "MailboxExecutionCompletionOutcome",
        ),
    )

    scheduling_use_case = read(use_cases / "scheduled.rs")
    assert_contains(
        "use-cases-mailboxes/scheduled.rs",
        scheduling_use_case,
        (
            "acquire_execution(",
            "execute_run_mailbox_job(",
            "complete_execution(",
            "MailboxExecutionClaimOutcome::InFlight",
            "MailboxExecutionClaimOutcome::Completed",
            "MailboxExecutionClaimOutcome::Stale",
            "EXECUTION_LEASE_MS",
            "MAX_DISPATCH_BATCH",
        ),
    )
    acquire_at = scheduling_use_case.find(".acquire_execution(")
    provider_at = scheduling_use_case.find("execute_run_mailbox_job(")
    if acquire_at < 0 or provider_at < 0 or acquire_at >= provider_at:
        fail("scheduled mailbox execution must durably claim before provider I/O")

    queue = read(adapters / "mailbox_job_queue.rs")
    assert_contains(
        "mailbox_job_queue.rs",
        queue,
        (
            "MAILBOX_JOB_QUEUE_ENVELOPE_VERSION",
            "tenant_id",
            "actor_id",
            "binding_id",
            "job_id",
            "expected_job_version",
            "due_at_ms",
        ),
    )
    assert_metadata_only("mailbox_job_queue.rs", queue)

    migration = read(root / "migrations" / "d1" / "0017_mailbox_queue_coordination.sql")
    assert_contains(
        "0017_mailbox_queue_coordination.sql",
        migration,
        (
            "mailbox_job_queue_dispatches",
            "mailbox_job_execution_leases",
            "expected_job_version",
            "fence",
            "lease_expires_at_ms",
        ),
    )
    assert_metadata_only("0017_mailbox_queue_coordination.sql", migration)

    scheduling_adapter = read(adapters / "d1_mailbox_scheduling.rs")
    assert_contains(
        "d1_mailbox_scheduling.rs",
        scheduling_adapter,
        (
            "job.lifecycle_status IN ('SCHEDULED', 'RETRY_PENDING')",
            "binding.execution_status = 'ACTIVE'",
            "job.attempt < job.max_attempts",
            "ON CONFLICT (tenant_id, binding_id, job_id, expected_job_version) DO NOTHING",
            "fence = fence + 1",
            "AND fence = ?",
        ),
    )
    assert_absent(
        "d1_mailbox_scheduling.rs",
        production_source(scheduling_adapter),
        ("message_body", "body_html", "body_text", "raw_message", "access_token", "password"),
    )

    secret_resolver = read(adapters / "cloud_mailbox_secrets.rs")
    assert_contains(
        "cloud_mailbox_secrets.rs",
        secret_resolver,
        (
            'MAILBOX_SECRET_RESOLVER_BINDING: &str = "MAILBOX_SECRET_RESOLVER"',
            ".service(MAILBOX_SECRET_RESOLVER_BINDING)",
            '"x-profile-tenant-id"',
            '"x-profile-mailbox-secret-handle"',
            '"x-profile-mailbox-provider"',
            "MAX_SECRET_DOCUMENT_BYTES",
            ".zeroize()",
        ),
    )
    assert_absent(
        "cloud_mailbox_secrets.rs",
        secret_resolver,
        ("secret_store(handle", "env.secret(handle", "env.var(handle"),
    )

    cloud_query = read(adapters / "cloud_mail_query.rs")
    assert_contains(
        "cloud_mail_query.rs",
        cloud_query,
        (
            "impl ClientMailProviderQueryPort for CloudMailboxQueryAdapter",
            "load_executable_binding",
            "MailboxBinding::is_executable",
            "resolve_mailbox_credential",
            "search_gmail_messages",
            "search_imap_messages",
            "get_gmail_message",
            "get_imap_message",
        ),
    )
    assert_absent("cloud_mail_query.rs", production_source(cloud_query), CONFIDENTIAL_SINKS)

    gmail_query = read(adapters / "gmail_mail_query.rs")
    assert_contains(
        "gmail_mail_query.rs",
        gmail_query,
        (
            'GMAIL_CURSOR_PREFIX: &str = "gmail:"',
            'GMAIL_REFERENCE_PREFIX: &str = "gmail:"',
            "MAX_GMAIL_QUERY_PAGE_SIZE",
            "MAX_GMAIL_MESSAGE_RESPONSE_BYTES",
            "MAX_MAIL_BODY_BYTES",
            '"metadata"',
            '"full"',
            ".zeroize()",
        ),
    )
    assert_absent("gmail_mail_query.rs", production_source(gmail_query), CONFIDENTIAL_SINKS)

    imap_query = read(adapters / "imap_query.rs")
    assert_contains(
        "imap_query.rs",
        imap_query,
        (
            'IMAP_CURSOR_PREFIX: &str = "imap:"',
            'IMAP_REFERENCE_PREFIX: &str = "imap:"',
            "UIDVALIDITY",
            "UID SEARCH",
            "UID FETCH",
            "MAX_IMAP_SEARCH_WINDOWS",
            "MAX_MAIL_BODY_BYTES",
            "MAX_MIME_PARTS",
            "MAX_MIME_DEPTH",
            'command.push_str("CHARSET UTF-8 ")',
            "term.as_str().is_ascii()",
            ".execute_with_literal(",
        ),
    )
    assert_absent("imap_query.rs", production_source(imap_query), CONFIDENTIAL_SINKS)

    imap_session = read(adapters / "imap_session.rs")
    assert_contains(
        "imap_session.rs",
        imap_session,
        (
            "SecureTransport::On",
            "SecureTransport::StartTls",
            "session.socket = session.socket.start_tls();",
            "login.zeroize();",
            "maximum_response_bytes",
            "MAX_COMMAND_LITERAL_BYTES",
            "execute_with_literal(",
            "literal_command_prefix(",
            "continuation_requested(",
        ),
    )

    mail_application = read(root / "crates" / "use-cases-query" / "src" / "mail.rs")
    for function, provider_marker in (
        ("search_client_mailbox_messages", ".search_messages("),
        ("get_client_mailbox_message", ".get_message("),
    ):
        start = mail_application.index(f"pub async fn {function}")
        next_fn = mail_application.find("\npub async fn ", start + 1)
        body = mail_application[start : next_fn if next_fn >= 0 else len(mail_application)]
        authorize_at = body.find("authorize(")
        eligibility_at = body.find(".is_mailbox_eligible(")
        provider_at = body.find(provider_marker)
        if min(authorize_at, eligibility_at, provider_at) < 0 or not (
            authorize_at < eligibility_at < provider_at
        ):
            fail(f"{function} must preserve auth -> eligibility -> provider ordering")

    worker_lib = read(worker / "lib.rs")
    if "CloudMailboxQueryAdapter" in worker_lib:
        fail("real Client Mail provider must not be called directly from the Worker transport router")

    wrangler = read(root / "deploy" / "cloudflare" / "wrangler.example.toml")
    assert_contains(
        "wrangler.example.toml",
        wrangler,
        (
            'binding = "MAILBOX_JOBS"',
            "max_retries = 6",
            "dead_letter_queue",
            'binding = "MAILBOX_SECRET_RESOLVER"',
        ),
    )

    for path in (
        worker / "mailbox_queue_evidence.rs",
        worker / "mailbox_scheduling.rs",
        adapters / "mailbox_job_queue.rs",
        adapters / "d1_mailbox_scheduling.rs",
    ):
        source = production_source(read(path))
        assert_absent(
            str(path.relative_to(root)),
            source,
            (
                "MailMessageBody",
                "MailMessageSummary",
                "access_token",
                "refresh_token",
                "password",
            ),
        )


def self_test() -> None:
    try:
        assert_absent("negative-inner-runtime", "pub fn x(_: worker::Env) {}", INNER_RUNTIME_FRAGMENTS)
    except AssertionError:
        pass
    else:
        fail("Phase 2E inner-runtime negative fixture unexpectedly passed")

    try:
        assert_metadata_only("negative-queue", "struct Envelope { subject: String }")
    except AssertionError:
        return
    fail("Phase 2E privacy negative fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Phase 2E mailbox negative fixtures rejected as expected.")
        return 0
    enforce(args.root)
    print("Phase 2E cloud mailbox runtime, Queue and privacy boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
