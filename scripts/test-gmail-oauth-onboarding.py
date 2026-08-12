#!/usr/bin/env python3
"""Permanent fail-closed regression for Pre-2J C2 Gmail OAuth onboarding."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FRAGMENT = ROOT / "openapi/v1/fragments/mailbox-gmail-oauth.json"
PORT = ROOT / "crates/application-ports/src/gmail_oauth_onboarding.rs"
USE_CASE = ROOT / "crates/use-cases-mailboxes/src/gmail_oauth_onboarding.rs"
ADAPTER = ROOT / "crates/cloudflare-adapters/src/gmail_oauth_provisioning.rs"
WORKER = ROOT / "apps/control-plane-worker/src/mailbox_gmail_oauth.rs"
ROUTES = ROOT / "crates/control-plane-contract/src/routes/mailboxes.rs"
CONTRACT = ROOT / "crates/control-plane-contract/src/mailbox_gmail_oauth_api.rs"
MIGRATIONS = ROOT / "migrations/d1"

FORBIDDEN_PUBLIC_FIELDS = {
    "accessToken",
    "refreshToken",
    "authorizationCode",
    "pkceVerifier",
    "clientSecret",
    "secretHandle",
}
FORBIDDEN_D1_TERMS = {
    "access_token",
    "refresh_token",
    "authorization_code",
    "pkce_verifier",
    "oauth_client_secret",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def main() -> int:
    fragment = json.loads(FRAGMENT.read_text(encoding="utf-8"))
    paths = fragment.get("paths", {})
    require(
        set(paths) == {
            "/api/v1/tenants/{tenantId}/mailbox-onboardings/{onboardingId}/gmail-oauth",
            "/auth/v1/mailbox/gmail/callback",
        },
        "C2 fragment must expose only the governed start and fixed callback paths",
    )
    encoded_fragment = json.dumps(fragment, sort_keys=True)
    for field in FORBIDDEN_PUBLIC_FIELDS:
        require(field not in encoded_fragment, f"C2 public fragment leaked forbidden field {field}")
    require("gmail.send" not in encoded_fragment, "C2 contract must not pregrant Gmail send scope")

    contract = CONTRACT.read_text(encoding="utf-8")
    require("openapi_fragment()" in contract, "C2 canonical Rust OpenAPI source is missing")
    require("gmail.send" not in contract, "C2 Rust contract must not contain Gmail send scope")

    port = PORT.read_text(encoding="utf-8")
    require("Serialize" not in port, "OAuth callback state/code types must not become serializable DTOs")
    require("MAX_OAUTH_STATE_LENGTH" in port and "MAX_AUTHORIZATION_CODE_LENGTH" in port,
            "OAuth callback inputs must remain bounded")

    adapter = ADAPTER.read_text(encoding="utf-8")
    require("MAILBOX_SECRET_RESOLVER_BINDING" in adapter,
            "C2 must use the existing MAILBOX_SECRET_RESOLVER binding")
    require("https://www.googleapis.com/auth/gmail.readonly" in adapter,
            "C2 must request only Gmail read scope")
    require("https://www.googleapis.com/auth/gmail.send" not in adapter,
            "C2 must not request Gmail send scope")
    for operation in ["/oauth/start", "/oauth/inspect", "/oauth/complete", "/oauth/deny", "/discard"]:
        require(operation in adapter, f"secret-resolver C2 operation missing: {operation}")
    require("console_" not in adapter, "C2 resolver adapter must not log OAuth material")

    use_case = USE_CASE.read_text(encoding="utf-8")
    require(use_case.count("validate_onboarding(") >= 3,
            "C2 must validate C1 state/version before start and again before callback activation")
    inspect_at = use_case.find("inspect_gmail_oauth_callback")
    complete_at = use_case.find("complete_gmail_oauth_callback")
    require(inspect_at >= 0 and complete_at > inspect_at,
            "C2 callback must expose inspection before completion sequencing")
    discard_at = use_case.find(".discard(actor, &discard_handle)")
    require(discard_at >= 0, "C2 must discard provisioned credentials after failed C1 activation")
    require("MembershipRole::TenantOwner" in use_case,
            "C2 administration must remain Owner-only")
    require("MailboxProvider::GmailApi" in use_case,
            "C2 must reject non-Gmail onboarding")
    require("MailboxOnboardingStatus::Pending" in use_case and "MailboxOnboardingStatus::ReauthRequired" in use_case,
            "C2 start/activation must be bounded to PENDING or REAUTH_REQUIRED")

    worker = WORKER.read_text(encoding="utf-8")
    worker_inspect = worker.find("inspect_gmail_oauth_callback")
    actor_resolve = worker.find("resolve_active_request_actor", worker_inspect)
    worker_complete = worker.find("complete_gmail_oauth_callback", actor_resolve)
    require(worker_inspect >= 0 and actor_resolve > worker_inspect and worker_complete > actor_resolve,
            "callback must inspect opaque state, reauthorize target actor, then complete")
    require("from_oauth_callback" in worker,
            "callback activation must use deterministic ceremony-derived command evidence")
    require("cache-control\", \"no-store" in worker and "referrer-policy\", \"no-referrer" in worker,
            "OAuth responses must be no-store/no-referrer")
    require("console_" not in worker, "C2 Worker ingress must not log OAuth callback material")

    routes = ROUTES.read_text(encoding="utf-8")
    require("mailbox-onboardings" in routes and '"auth", "v1", "mailbox", "gmail", "callback"' in routes,
            "C2 routes must be exact and classified by the canonical router")

    migrations = "\n".join(path.read_text(encoding="utf-8").lower() for path in sorted(MIGRATIONS.glob("*.sql")))
    for term in FORBIDDEN_D1_TERMS:
        require(term not in migrations, f"OAuth credential material must not be persisted in D1: {term}")

    print("Pre-2J C2 Gmail OAuth onboarding fail-closed regression passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
