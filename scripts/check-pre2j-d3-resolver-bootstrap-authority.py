#!/usr/bin/env python3
"""Validate the one-shot Pre-2J D3 resolver/bootstrap authority."""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = Path("architecture/pre2j-d3-resolver-bootstrap-authority.json")
DOC_PATH = Path("docs/PRE2J_D3_RESOLVER_BOOTSTRAP_AUTHORITY.md")
STATUS_PATH = Path("docs/status.json")
EXPECTED_BASE = "65550585baa471c8fb3c452c85ee5db7e79d9b5b"
EXPECTED_IMPLEMENTATION_MARKER = Path(
    "architecture/pre2j-d3-resolver-bootstrap-implementation.json"
)
RESOLVER_URL = re.compile(r'https://mailbox-secret-resolver\.internal([^"\s]+)')

EXPECTED_SERVICE = {
    "binding": "MAILBOX_SECRET_RESOLVER",
    "entrypoint_root": "apps/mailbox-secret-resolver-worker",
    "deployment_config": "deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc",
    "staging_worker": "mailbox-secret-resolver-staging",
    "production_worker": "mailbox-secret-resolver-production",
    "public_route": False,
    "transport": "service_binding_http",
    "caller_authentication": "hmac_sha256_signed_request_v1",
}
EXPECTED_STORAGE = {
    "type": "dedicated_d1_application_encrypted",
    "binding": "RESOLVER_DB",
    "migration_root": "migrations/resolver-d1",
    "business_catalog_forbidden": True,
    "cipher": "AES-256-GCM",
    "nonce_bytes": 12,
    "handle_lookup": "HMAC-SHA-256",
    "staging_production_isolation_required": True,
}
EXPECTED_RESOLVER_RELEASE = {
    "source_authority": "accepted_main",
    "artifact_identity_fields": [
        "release_id",
        "source_commit_sha",
        "resolver_worker_sha256",
        "resolver_migration_manifest_sha256",
        "resolver_config_sha256",
        "build_toolchain",
    ],
    "build_once": True,
    "same_bits_staging_production": True,
    "release_workflow": ".github/workflows/mailbox-secret-resolver-release.yml",
    "promotion_workflow": ".github/workflows/mailbox-secret-resolver-promotion.yml",
    "rebuild_during_provisioning_forbidden": True,
    "unreviewed_source_deploy_forbidden": True,
}
EXPECTED_CONTROL_PLANE_SECRETS = [
    "CLIENT_CONTACT_PROTECTION_KEYRING",
    "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    "R2_GENERATION_ACCESS_KEY_ID",
    "R2_GENERATION_SECRET_ACCESS_KEY",
]
EXPECTED_RESOLVER_SECRETS = [
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    "MAILBOX_RESOLVER_ENCRYPTION_KEYRING",
    "MAILBOX_RESOLVER_HANDLE_HMAC_KEY",
    "MICROSOFT_OAUTH_CLIENT_SECRET",
]
EXPECTED_RESOLVER_VARS = [
    "GOOGLE_OAUTH_CLIENT_ID",
    "GOOGLE_OAUTH_REDIRECT_URI",
    "MICROSOFT_OAUTH_CLIENT_ID",
    "MICROSOFT_OAUTH_REDIRECT_URI",
]
EXPECTED_GITHUB_SECRETS = [
    "CLOUDFLARE_ACCESS_CLIENT_ID",
    "CLOUDFLARE_ACCESS_CLIENT_SECRET",
    "CLOUDFLARE_API_TOKEN",
    "CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON",
    "CLOUDFLARE_DEPLOY_MANIFEST_JSON",
    "CLOUDFLARE_RESOLVER_SECRETS_JSON",
]
EXPECTED_ORDER = [
    "accepted_resolver_release_artifact",
    "dedicated_resolver_d1",
    "catalog_d1_and_d3a_bootstrap",
    "profile_r2_and_queues",
    "r2_s3_credentials",
    "resolver_worker_with_secrets",
    "access_application_service_auth_policy_and_token",
    "github_staging_environment_inputs",
    "d3_control_plane_first_deploy_with_secrets_and_custom_domain",
    "d3_staging_smoke_and_attestation",
]
EXPECTED_TOKEN_PROFILES = {
    "bootstrap_permission_names": [
        "Access: Apps and Policies Write",
        "Access: Service Tokens Write",
        "API Tokens Write",
        "D1 Write",
        "Queues Write",
        "Workers R2 Storage Write",
        "Workers Routes Write",
        "Workers Scripts Write",
    ],
    "steady_state_deploy_permission_names": [
        "D1 Read",
        "Queues Read",
        "Workers R2 Storage Read",
        "Workers Routes Write",
        "Workers Scripts Write",
    ],
    "environment_resource_scope_required": True,
    "cross_environment_tokens_forbidden": True,
}
EXPECTED_RULES = {
    "authority_must_be_accepted_before_implementation": True,
    "authority_is_immutable_after_acceptance": True,
    "same_pr_implementation_forbidden": True,
    "staging_first": True,
    "control_plane_first_deploy_must_be_d3_exact_artifact": True,
    "production_forbidden_before_accepted_staging_evidence": True,
    "automatic_resource_provisioning_forbidden": True,
    "placeholder_worker_or_secret_forbidden": True,
    "bootstrap_token_as_deploy_token_forbidden": True,
    "shared_environment_credentials_forbidden": True,
    "secret_values_in_git_logs_artifacts_or_manifests_forbidden": True,
    "openapi_v1_changes_forbidden": True,
    "production_ready_must_remain_false": True,
}
REQUIRED_DOC_MARKERS = (
    "Service-binding reachability",
    "dedicated D1 database per environment",
    "`CLOUDFLARE_RESOLVER_SECRETS_JSON`",
    "`CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON`",
    "same resolver bits",
    "mode-0600 files under `$RUNNER_TEMP`",
    "`Access: Service Tokens Write`",
    "Production resource creation remains forbidden",
)


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=ROOT, check=check, text=True, capture_output=True)


def load_json(path: Path) -> dict[str, object]:
    value = json.loads((ROOT / path).read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def source_endpoints() -> list[str]:
    root = ROOT / "crates" / "cloudflare-adapters" / "src"
    endpoints = {
        match.group(1)
        for path in root.rglob("*.rs")
        for match in RESOLVER_URL.finditer(path.read_text(encoding="utf-8"))
    }
    return sorted(endpoints)


def remediation_errors(status: dict[str, object]) -> list[str]:
    current = status.get("current")
    if not isinstance(current, dict):
        return ["docs/status.json: current authority is missing"]
    remediation = current.get("pre2j_product_readiness_remediation")
    errors: list[str] = []
    if not isinstance(remediation, dict) or remediation.get("status") != "active_blocking":
        errors.append("D3 resolver authority requires active_blocking pre-2J remediation")
    elif remediation.get("tracking_issue") != 203:
        errors.append("D3 resolver authority requires umbrella blocker #203")
    phase = current.get("phase_2j")
    if (
        not isinstance(phase, dict)
        or phase.get("status") != "blocked_pending_repository_remediation"
    ):
        errors.append("Phase 2J must remain blocked")
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false")
    return errors


def authority_errors(authority: dict[str, object]) -> list[str]:
    expected_scalars = {
        "schema_version": 1,
        "status": "approved_pending_implementation",
        "decision_base": EXPECTED_BASE,
        "tracking_issue": 256,
        "parent_issue": 251,
        "umbrella_blocker_issue": 203,
        "implementation_marker": str(EXPECTED_IMPLEMENTATION_MARKER),
    }
    errors = [
        f"{AUTHORITY_PATH}: {key} must be {wanted!r}"
        for key, wanted in expected_scalars.items()
        if authority.get(key) != wanted
    ]
    exact_objects = {
        "service": EXPECTED_SERVICE,
        "resolver_storage": EXPECTED_STORAGE,
        "resolver_release": EXPECTED_RESOLVER_RELEASE,
        "control_plane_required_secrets": EXPECTED_CONTROL_PLANE_SECRETS,
        "resolver_required_secrets": EXPECTED_RESOLVER_SECRETS,
        "resolver_required_vars": EXPECTED_RESOLVER_VARS,
        "github_environment_secret_names": EXPECTED_GITHUB_SECRETS,
        "staging_creation_order": EXPECTED_ORDER,
        "token_profiles": EXPECTED_TOKEN_PROFILES,
        "rules": EXPECTED_RULES,
    }
    for key, wanted in exact_objects.items():
        if authority.get(key) != wanted:
            errors.append(f"{AUTHORITY_PATH}: {key} drifted from the accepted authority")
    if authority.get("resolver_endpoints") != source_endpoints():
        errors.append(
            f"{AUTHORITY_PATH}: resolver_endpoints must exactly match accepted adapter callers"
        )
    if set(authority) != set(expected_scalars) | set(exact_objects) | {"resolver_endpoints"}:
        errors.append(f"{AUTHORITY_PATH}: top-level key inventory drifted")
    return errors


def document_errors() -> list[str]:
    text = (ROOT / DOC_PATH).read_text(encoding="utf-8")
    return [
        f"{DOC_PATH}: missing normative marker {marker!r}"
        for marker in REQUIRED_DOC_MARKERS
        if marker not in text
    ]


def current_errors() -> list[str]:
    errors = authority_errors(load_json(AUTHORITY_PATH))
    errors.extend(document_errors())
    errors.extend(remediation_errors(load_json(STATUS_PATH)))
    return errors


def base_has(base_ref: str, path: Path) -> bool:
    return git("cat-file", "-e", f"{base_ref}:{path}", check=False).returncode == 0


def base_text(base_ref: str, path: Path) -> str:
    return git("show", f"{base_ref}:{path}").stdout


def changed_paths(base_ref: str, path: str) -> list[str]:
    return [
        line
        for line in git("diff", "--name-only", base_ref, "--", path).stdout.splitlines()
        if line
    ]


def implementation_change_errors(
    *,
    implementation_marker_present: bool,
    changed_implementation_roots: list[str],
    openapi_changes: list[str],
) -> list[str]:
    errors: list[str] = []
    if implementation_marker_present:
        errors.append(
            "authority and resolver/bootstrap implementation marker cannot land in one PR"
        )
    for path in changed_implementation_roots:
        errors.append(f"authority PR must not implement resolver/bootstrap path: {path}")
    if openapi_changes:
        errors.append("authority PR must not change openapi/v1")
    return errors


def repository_errors(base_ref: str, *, authority_only: bool) -> list[str]:
    errors = current_errors()
    accepted = base_has(base_ref, AUTHORITY_PATH)
    if accepted:
        for path in (AUTHORITY_PATH, DOC_PATH):
            if not base_has(base_ref, path) or base_text(base_ref, path) != (
                ROOT / path
            ).read_text(encoding="utf-8"):
                errors.append(f"accepted D3 resolver/bootstrap authority is immutable: {path}")
        return errors
    if authority_only:
        errors.append("D3 resolver/bootstrap authority is not yet accepted on the base branch")
        return errors
    implementation_roots = (
        EXPECTED_SERVICE["entrypoint_root"],
        EXPECTED_SERVICE["deployment_config"],
        EXPECTED_STORAGE["migration_root"],
        EXPECTED_RESOLVER_RELEASE["release_workflow"],
        EXPECTED_RESOLVER_RELEASE["promotion_workflow"],
    )
    errors.extend(
        implementation_change_errors(
            implementation_marker_present=(
                base_has(base_ref, EXPECTED_IMPLEMENTATION_MARKER)
                or (ROOT / EXPECTED_IMPLEMENTATION_MARKER).exists()
            ),
            changed_implementation_roots=[
                str(path)
                for path in implementation_roots
                if changed_paths(base_ref, str(path))
            ],
            openapi_changes=changed_paths(base_ref, "openapi/v1"),
        )
    )
    return errors


def self_test() -> None:
    valid = load_json(AUTHORITY_PATH)
    assert not authority_errors(valid)
    tampered = copy.deepcopy(valid)
    service = tampered["service"]
    assert isinstance(service, dict)
    service["public_route"] = True
    assert authority_errors(tampered)
    tampered = copy.deepcopy(valid)
    rules = tampered["rules"]
    assert isinstance(rules, dict)
    rules["placeholder_worker_or_secret_forbidden"] = False
    assert authority_errors(tampered)
    tampered = copy.deepcopy(valid)
    endpoints = tampered["resolver_endpoints"]
    assert isinstance(endpoints, list)
    endpoints.pop()
    assert authority_errors(tampered)
    tampered = copy.deepcopy(valid)
    tampered["control_plane_required_secrets"] = EXPECTED_CONTROL_PLANE_SECRETS[:-1]
    assert authority_errors(tampered)
    assert not implementation_change_errors(
        implementation_marker_present=False,
        changed_implementation_roots=[],
        openapi_changes=[],
    )
    assert implementation_change_errors(
        implementation_marker_present=True,
        changed_implementation_roots=[],
        openapi_changes=[],
    )
    assert implementation_change_errors(
        implementation_marker_present=False,
        changed_implementation_roots=[str(EXPECTED_SERVICE["entrypoint_root"])],
        openapi_changes=[],
    )
    assert implementation_change_errors(
        implementation_marker_present=False,
        changed_implementation_roots=[],
        openapi_changes=["openapi/v1/openapi.json"],
    )
    print("D3 resolver/bootstrap authority negative policy self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-ref")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--authority-only", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    errors = (
        current_errors()
        if not args.base_ref
        else repository_errors(args.base_ref, authority_only=args.authority_only)
    )
    if errors:
        raise SystemExit("\n".join(errors))
    if not args.base_ref:
        print("D3 resolver/bootstrap authority document and inventory are valid.")
    elif not base_has(args.base_ref, AUTHORITY_PATH):
        print("D3 resolver/bootstrap authority candidate is valid and unconsumed.")
    else:
        print("D3 resolver/bootstrap authority is valid and immutable.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
