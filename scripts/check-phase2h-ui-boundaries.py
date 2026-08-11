#!/usr/bin/env python3
"""Permanent Phase 2H standalone UI, Client Mail and privacy boundary checks."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

FILES = {
    "router": Path("frontend/src/app/router.tsx"),
    "clients_route": Path("frontend/src/features/clients/route.tsx"),
    "profiles_route": Path("frontend/src/features/profiles/route.tsx"),
    "access_route": Path("frontend/src/features/access/route.tsx"),
    "mailboxes_route": Path("frontend/src/features/mailboxes/route.tsx"),
    "session_route": Path("frontend/src/features/session/route.tsx"),
    "devices_route": Path("frontend/src/features/devices/route.tsx"),
    "audit_route": Path("frontend/src/features/audit/route.tsx"),
    "settings_route": Path("frontend/src/features/settings/route.tsx"),
    "client_mail_panel": Path("frontend/src/features/clients/ClientMailPanel.tsx"),
    "client_mail_api": Path("frontend/src/shared/api/clientMail.ts"),
    "mail_html": Path("frontend/src/shared/mail/safeMailHtml.ts"),
    "mail_body": Path("frontend/src/shared/mail/SafeMailBody.tsx"),
    "worker_mail": Path("apps/control-plane-worker/src/client_mail_query.rs"),
    "eligibility": Path("crates/cloudflare-adapters/src/d1_client_mail_eligibility.rs"),
    "query_mail_contract": Path("crates/control-plane-contract/src/query_mail_api.rs"),
    "query_mail_openapi": Path("openapi/v1/fragments/query-mail.json"),
    "realtime": Path("frontend/src/shared/realtime/NotificationRealtimeBridge.tsx"),
}

ROUTE_MARKERS = (
    "path: '/clients'",
    "path: '/clients/$clientId'",
    "path: '/profiles'",
    "path: '/profiles/$profileId'",
    "path: '/users'",
    "path: '/mailboxes'",
    "path: '/sessions'",
    "path: '/devices'",
    "path: '/audit'",
    "path: '/settings'",
)

ROUTE_SOURCE_KEYS = (
    "clients_route",
    "profiles_route",
    "access_route",
    "mailboxes_route",
    "session_route",
    "devices_route",
    "audit_route",
    "settings_route",
)

MAIL_PATHS = (
    "/api/v1/tenants/{tenantId}/clients/{clientId}/mail/search",
    "/api/v1/tenants/{tenantId}/clients/{clientId}/mail/message",
)

BROWSER_PERSISTENCE_SINKS = (
    "localStorage",
    "sessionStorage",
    "indexedDB",
    "sendBeacon",
    "console.",
)


def load_sources(root: Path) -> dict[str, str]:
    sources: dict[str, str] = {}
    for key, relative in FILES.items():
        path = root / relative
        if not path.is_file():
            raise ValueError(f"missing Phase 2H governed file: {relative}")
        sources[key] = path.read_text(encoding="utf-8")
    return sources


def validate_sources(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    route_sources = "\n".join(sources[key] for key in ROUTE_SOURCE_KEYS)
    for marker in ROUTE_MARKERS:
        if marker not in route_sources:
            errors.append(f"standalone route marker missing: {marker}")

    router = sources["router"]
    for feature_import in (
        "../features/access",
        "../features/audit",
        "../features/clients",
        "../features/devices",
        "../features/mailboxes",
        "../features/profiles",
        "../features/session",
        "../features/settings",
    ):
        if feature_import not in router:
            errors.append(f"root router must compose feature public API: {feature_import}")
    for route_factory in (
        "createAccessRoute",
        "createAuditRoute",
        "createClientsRoutes",
        "createDevicesRoute",
        "createMailboxesRoute",
        "createProfilesRoutes",
        "createSessionRoute",
        "createSettingsRoute",
    ):
        if route_factory not in router:
            errors.append(f"root router route factory missing: {route_factory}")

    if "MemberDirectory" not in sources["access_route"]:
        errors.append("Users route must compose the authorized member directory")
    if "MailboxDirectory" not in sources["mailboxes_route"]:
        errors.append("Mailboxes route must compose the authorized mailbox directory")
    if "ProfileDirectory" not in sources["profiles_route"]:
        errors.append("Profiles routes must compose the authorized profile directory")

    panel = sources["client_mail_panel"]
    for marker in ("searchClientMail", "getClientMailMessage", "SafeMailBody", "listMailboxes"):
        if marker not in panel:
            errors.append(f"Client Mail executable UI marker missing: {marker}")
    for sink in BROWSER_PERSISTENCE_SINKS:
        if sink in panel:
            errors.append(f"Client Mail panel contains forbidden browser sink: {sink}")

    api = sources["client_mail_api"]
    for suffix in ("/mail/search", "/mail/message"):
        if suffix not in api:
            errors.append(f"Client Mail API route missing: {suffix}")
    if api.count("method: 'POST'") < 2 or "URLSearchParams" in api:
        errors.append("Client Mail transport must keep confidential query inputs in POST bodies")

    contract = sources["query_mail_contract"]
    for marker in ("ClientMailSearchInput", "MailboxMessageReferenceDto", "MailMessageBodyDto"):
        if marker not in contract:
            errors.append(f"Rust-owned Client Mail DTO missing: {marker}")
    for path in MAIL_PATHS:
        if path not in contract:
            errors.append(f"Rust-owned Client Mail OpenAPI path missing: {path}")
    if '"post"' not in contract or '"requestBody"' not in contract:
        errors.append("Client Mail Rust contract must expose body-based POST operations")

    try:
        openapi = json.loads(sources["query_mail_openapi"])
    except json.JSONDecodeError as error:
        errors.append(f"query-mail OpenAPI is invalid JSON: {error}")
    else:
        paths = openapi.get("paths", {})
        for path in MAIL_PATHS:
            operation = paths.get(path, {}).get("post")
            if not isinstance(operation, dict):
                errors.append(f"generated query-mail POST path missing: {path}")
            elif any(parameter.get("in") == "query" for parameter in operation.get("parameters", [])):
                errors.append(f"Client Mail confidential inputs must not be URL query parameters: {path}")

    worker = sources["worker_mail"]
    for marker in (
        "search_client_mailbox_messages",
        "get_client_mailbox_message",
        "D1ClientMailboxEligibilityRepository",
        "CloudMailboxQueryAdapter",
        "resolve_active_request_actor",
    ):
        if marker not in worker:
            errors.append(f"Client Mail Worker ingress marker missing: {marker}")
    for forbidden in ("SELECT ", "INSERT ", "UPDATE ", "DELETE FROM"):
        if forbidden in worker:
            errors.append(f"SQL must stay out of Client Mail Worker ingress: {forbidden}")

    eligibility = sources["eligibility"]
    for marker in (
        "ClientMailboxEligibilityPort",
        "client.status = 'ACTIVE'",
        "binding.status = 'ACTIVE'",
        "binding.execution_status = 'ACTIVE'",
        "requester.status = 'ACTIVE'",
        "requester.role = 'TENANT_OWNER'",
    ):
        if marker not in eligibility:
            errors.append(f"live Client Mail eligibility guard missing: {marker}")

    mail_html = sources["mail_html"]
    for marker in (
        "default-src 'none'",
        "connect-src 'none'",
        "script-src 'none'",
        "form-action 'none'",
        "img-src data: cid:",
        "SAFE_RASTER_DATA_IMAGE",
        "NAVIGATION_ATTRIBUTES",
    ):
        if marker not in mail_html:
            errors.append(f"safe mail HTML boundary missing: {marker}")
    mail_body = sources["mail_body"]
    for marker in ('sandbox=""', 'referrerPolicy="no-referrer"', "srcDoc={safeMailSrcDoc(htmlBody)}"):
        if marker not in mail_body:
            errors.append(f"safe mail body iframe boundary missing: {marker}")

    for key in ("client_mail_panel", "client_mail_api", "mail_html", "mail_body"):
        source = sources[key]
        for sink in BROWSER_PERSISTENCE_SINKS:
            if sink in source:
                errors.append(f"confidential mail source {key} contains forbidden sink: {sink}")

    realtime = sources["realtime"]
    if "invalidateQueries" not in realtime:
        errors.append("Phase 2G realtime bridge must remain invalidation-only")
    if "setQueryData" in realtime:
        errors.append("Phase 2G realtime bridge must not mutate business query data")
    return errors


def self_test(sources: dict[str, str]) -> None:
    fixtures = [
        ("route", "settings_route", "path: '/settings'", "path: '/settings-broken'"),
        ("mail execution", "client_mail_panel", "SafeMailBody", "UnsafeMailBody"),
        ("generated path", "query_mail_openapi", '"post"', '"get"'),
        ("authorization", "worker_mail", "resolve_active_request_actor", "resolve_actor_bypass"),
        ("sandbox", "mail_body", 'sandbox=""', 'sandbox="allow-scripts"'),
        ("realtime", "realtime", "invalidateQueries", "setQueryData"),
    ]
    for label, key, needle, replacement in fixtures:
        if needle not in sources[key]:
            raise ValueError(f"self-test fixture source marker missing for {label}: {needle}")
        mutated = dict(sources)
        mutated[key] = mutated[key].replace(needle, replacement)
        if not validate_sources(mutated):
            raise ValueError(f"Phase 2H negative fixture was not rejected: {label}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        sources = load_sources(args.root)
        if args.self_test:
            self_test(sources)
            print("Phase 2H negative fixtures rejected as expected.")
            return 0
        errors = validate_sources(sources)
    except (OSError, ValueError) as error:
        print(error)
        return 1
    if errors:
        for error in errors:
            print(error)
        return 1
    print("Phase 2H standalone UI, Client Mail and privacy boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
