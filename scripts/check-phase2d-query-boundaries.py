#!/usr/bin/env python3
"""Enforce Phase 2D CQRS/query ownership, privacy and authorization ordering."""

from __future__ import annotations

import argparse
import re
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

QUERY_MODULES = (
    "query.rs",
    "query_clients.rs",
    "query_profiles.rs",
    "query_members.rs",
    "query_mailboxes.rs",
    "query_mail.rs",
    "query_mail_provider.rs",
    "query_global.rs",
)
FORBIDDEN_INNER = (
    "worker::",
    "web_sys::",
    "cloudflare",
    "D1Database",
    "React",
)
FORBIDDEN_DISCOVERY = (
    " LIKE ",
    " GLOB ",
    " MATCH ",
    "COUNT(",
)


def fail(message: str) -> None:
    raise AssertionError(message)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"required Phase 2D file missing: {path}")
    return path.read_text(encoding="utf-8")


def sql_literals(source: str) -> str:
    return "\n".join(re.findall(r'r#"(.*?)"#', source, flags=re.DOTALL))


def assert_no_discovery_sql(name: str, source: str) -> None:
    sql = sql_literals(source)
    for fragment in FORBIDDEN_DISCOVERY:
        if fragment.lower() in sql.lower():
            fail(f"unbounded/fuzzy query predicate is prohibited in {name}: {fragment}")


def enforce(root: Path) -> None:
    ports = root / "crates" / "application-ports" / "src"
    for name in QUERY_MODULES:
        source = read(ports / name)
        for fragment in FORBIDDEN_INNER:
            if fragment.lower() in source.lower():
                fail(f"provider/runtime leakage in application query port {name}: {fragment}")

    query_root = root / "crates" / "use-cases-query" / "src"
    application = read(query_root / "lib.rs")
    contact_application = read(query_root / "contact.rs")
    mail_application = read(query_root / "mail.rs")
    for name, source in (
        ("lib.rs", application),
        ("contact.rs", contact_application),
        ("mail.rs", mail_application),
    ):
        for fragment in FORBIDDEN_INNER:
            if fragment.lower() in source.lower():
                fail(f"provider/runtime leakage in use-cases-query/{name}: {fragment}")

    required_functions = (
        "list_clients",
        "list_profiles",
        "list_members",
        "list_mailboxes",
        "search_global_exact",
        "list_client_mail_eligibility",
    )
    for function in required_functions:
        marker = f"pub async fn {function}"
        if marker not in application:
            fail(f"missing application-owned query function: {function}")

    global_start = application.index("pub async fn search_global_exact")
    global_end = application.index("pub async fn list_client_mail_eligibility", global_start)
    global_body = application[global_start:global_end]
    authorize_at = global_body.find("authorize(")
    project_at = global_body.find(".search_exact(")
    if authorize_at < 0 or project_at < 0 or authorize_at >= project_at:
        fail("global search must authorize before projection")

    list_pairs = (
        ("list_clients", ".list_clients("),
        ("list_profiles", ".list_profiles("),
        ("list_members", ".list_members("),
        ("list_mailboxes", ".list_mailboxes("),
        ("list_client_mail_eligibility", ".list_eligible_mailboxes_for_client("),
    )
    for function, projection_marker in list_pairs:
        start = application.index(f"pub async fn {function}")
        next_fn = application.find("\npub async fn ", start + 1)
        body = application[start : next_fn if next_fn >= 0 else len(application)]
        authorize_at = body.find("authorize(")
        project_at = body.find(projection_marker)
        if authorize_at < 0 or project_at < 0 or authorize_at >= project_at:
            fail(f"{function} must authorize before projection")

    contact_auth = contact_application.find("authorize(")
    contact_hmac = contact_application.find("derive_exact_lookup_candidates(")
    contact_project = contact_application.find(".find_visible_clients_by_exact_contact(")
    if min(contact_auth, contact_hmac, contact_project) < 0 or not (
        contact_auth < contact_hmac < contact_project
    ):
        fail("exact contact query must authorize before HMAC derivation and projection")
    if "MAX_LOOKUP_KEY_CANDIDATES" not in contact_application or "MAX_EXACT_CONTACT_MATCHES" not in contact_application:
        fail("exact contact query must bound key rotation and result cardinality")

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
            fail(f"{function} must authorize and prove eligibility before provider fetch")

    global_port = read(ports / "query_global.rs")
    for typed_id in ("ClientId", "ProfileId", "ActorId", "MailboxBindingId"):
        if typed_id not in global_port:
            fail(f"global search is missing typed opaque identity {typed_id}")
    if "String" in global_port.split("pub enum GlobalSearchKey", 1)[1].split("}", 1)[0]:
        fail("global search key must not accept arbitrary String variants")

    mail_port = read(ports / "query_mail_provider.rs")
    for sensitive_type in ("MailMessageSummary", "MailMessageBody", "MailboxMessageReference"):
        marker = f"pub struct {sensitive_type}"
        start = mail_port.find(marker)
        if start < 0:
            fail(f"missing provider-neutral mail contract: {sensitive_type}")
        derive_start = mail_port.rfind("#[derive(", 0, start)
        derive_end = mail_port.find(")]", derive_start, start) if derive_start >= 0 else -1
        if derive_start >= 0 and derive_end >= 0 and "Debug" in mail_port[derive_start:derive_end]:
            fail(f"confidential mail type must not derive Debug: {sensitive_type}")
    if "MAX_MAIL_BODY_BYTES" not in mail_port:
        fail("mail body must have an explicit byte bound")

    adapters = root / "crates" / "cloudflare-adapters" / "src"
    d1_query = read(adapters / "d1_query.rs")
    d1_global = read(adapters / "d1_global_query.rs")
    d1_contact = read(adapters / "d1_contact_query.rs")
    for name, source in (
        ("d1_query.rs", d1_query),
        ("d1_global_query.rs", d1_global),
        ("d1_contact_query.rs", d1_contact),
    ):
        assert_no_discovery_sql(name, source)
    combined = d1_query + "\n" + d1_global + "\n" + d1_contact
    if "secret_handle" in combined:
        fail("query projection must never read mailbox secret handles")
    if "client_contact_points" in d1_global:
        fail("global search must not scan contact storage")

    contact_sql = sql_literals(d1_contact)
    for required in (
        "client_contact_points",
        "exact_lookup_token",
        "lookup_key_version",
        "client_grants",
        "membership.status = 'ACTIVE'",
        "LIMIT ?",
    ):
        if required not in contact_sql:
            fail(f"exact contact D1 query missing grant-safe indexed predicate: {required}")

    if "profile_client_assignments" not in d1_query:
        fail("profile read projection must preserve assignment linkage")
    profile_sql = d1_query.split("impl ProfileReadModelPort", 1)[1].split("impl MemberReadModelPort", 1)[0]
    if "profile_grants" not in profile_sql:
        fail("profile read projection must authorize through profile grants")
    if "client_grants" in profile_sql:
        fail("profile assignment/client grants must not become profile authorization")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ports = root / "crates" / "application-ports" / "src"
        ports.mkdir(parents=True)
        for name in QUERY_MODULES:
            (ports / name).write_text("pub trait Safe {}\n", encoding="utf-8")
        (ports / "query_global.rs").write_text(
            "pub enum GlobalSearchKey { Raw(String) }\nClientId ProfileId ActorId MailboxBindingId\n",
            encoding="utf-8",
        )
        query_root = root / "crates" / "use-cases-query" / "src"
        query_root.mkdir(parents=True)
        query_root.joinpath("lib.rs").write_text("worker::Env\n", encoding="utf-8")
        query_root.joinpath("contact.rs").write_text("pub async fn unsafe_contact() {}\n", encoding="utf-8")
        query_root.joinpath("mail.rs").write_text("pub async fn unsafe_mail() {}\n", encoding="utf-8")
        adapters = root / "crates" / "cloudflare-adapters" / "src"
        adapters.mkdir(parents=True)
        adapters.joinpath("d1_query.rs").write_text(
            'let sql = r#" SELECT COUNT(*) FROM clients "#;', encoding="utf-8"
        )
        adapters.joinpath("d1_global_query.rs").write_text(
            'let sql = r#" SELECT secret_handle FROM mailbox_bindings "#;', encoding="utf-8"
        )
        adapters.joinpath("d1_contact_query.rs").write_text(
            'let sql = r#" SELECT value FROM client_contact_points WHERE value LIKE ? "#;', encoding="utf-8"
        )
        try:
            enforce(root)
        except AssertionError:
            return
        raise AssertionError("Phase 2D negative fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Phase 2D query negative fixtures rejected as expected.")
        return 0
    enforce(args.root)
    print("Phase 2D query ownership, privacy and authorization boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
