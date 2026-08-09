#!/usr/bin/env python3
"""Enforce Phase 2D CQRS/query ownership, privacy and authorization ordering."""

from __future__ import annotations

import argparse
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


def enforce(root: Path) -> None:
    ports = root / "crates" / "application-ports" / "src"
    for name in QUERY_MODULES:
        source = read(ports / name)
        for fragment in FORBIDDEN_INNER:
            if fragment.lower() in source.lower():
                fail(f"provider/runtime leakage in application query port {name}: {fragment}")

    application = read(root / "crates" / "use-cases-query" / "src" / "lib.rs")
    for fragment in FORBIDDEN_INNER:
        if fragment.lower() in application.lower():
            fail(f"provider/runtime leakage in use-cases-query: {fragment}")

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

    global_port = read(ports / "query_global.rs")
    for typed_id in ("ClientId", "ProfileId", "ActorId", "MailboxBindingId"):
        if typed_id not in global_port:
            fail(f"global search is missing typed opaque identity {typed_id}")
    if "String" in global_port.split("pub enum GlobalSearchKey", 1)[1].split("}", 1)[0]:
        fail("global search key must not accept arbitrary String variants")

    d1_query = read(root / "crates" / "cloudflare-adapters" / "src" / "d1_query.rs")
    d1_global = read(root / "crates" / "cloudflare-adapters" / "src" / "d1_global_query.rs")
    combined = d1_query + "\n" + d1_global
    for fragment in FORBIDDEN_DISCOVERY:
        if fragment.lower() in combined.lower():
            fail(f"unbounded/fuzzy query predicate is prohibited: {fragment}")
    if "secret_handle" in combined:
        fail("query projection must never read mailbox secret handles")
    if "client_contact_points" in d1_global:
        fail("global search must not scan contact storage")

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
        use_cases = root / "crates" / "use-cases-query" / "src"
        use_cases.mkdir(parents=True)
        use_cases.joinpath("lib.rs").write_text("worker::Env\n", encoding="utf-8")
        adapters = root / "crates" / "cloudflare-adapters" / "src"
        adapters.mkdir(parents=True)
        adapters.joinpath("d1_query.rs").write_text(" SELECT COUNT(*) FROM clients ", encoding="utf-8")
        adapters.joinpath("d1_global_query.rs").write_text(" SELECT secret_handle FROM mailbox_bindings ", encoding="utf-8")
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
