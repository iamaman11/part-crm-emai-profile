#!/usr/bin/env python3
"""Fail-closed permanent checks for the Pre-2J D3 resolver implementation."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = Path("architecture/pre2j-d3-resolver-bootstrap-authority.json")
MARKER = Path("architecture/pre2j-d3-resolver-bootstrap-implementation.json")
ACCEPTED_MAIN = "6a7dad9f74a25ccfd77cdd1a76216d8a46694e10"
AUTHORITY_SHA256 = "e19007fdcb0533001313463a070ce675e4fb636c04507b89f47a87feca3c610e"
RELEASE_WORKFLOW = Path(".github/workflows/mailbox-secret-resolver-release.yml")
PROMOTION_WORKFLOW = Path(".github/workflows/mailbox-secret-resolver-promotion.yml")
QUALITY_WORKFLOW = Path(".github/workflows/quality-gate.yml")
RESOLVER_CONFIG = Path("deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc")
RESOLVER_ROOT = Path("apps/mailbox-secret-resolver-worker")
RESOLVER_MIGRATION_ROOT = Path("migrations/resolver-d1")
ADAPTER_ROOT = Path("crates/cloudflare-adapters/src")
ADAPTER_ENDPOINT_RE = re.compile(r'https://mailbox-secret-resolver\.internal([^"\s]+)')
RUNTIME_ENDPOINT_RE = re.compile(r'"(/v1/mailbox-credentials/[^"\s]+)"')
EXPECTED_MARKER = {
    "schema_version": 1,
    "status": "implemented_pending_acceptance",
    "authority_commit": ACCEPTED_MAIN,
    "authority_sha256": AUTHORITY_SHA256,
    "authority_pr": 257,
    "tracking_issue": 258,
    "implementation_base": ACCEPTED_MAIN,
    "production_ready": False,
    "external_mutations": False,
    "resolver_endpoint_count": 23,
    "implementation_roots": [
        "apps/mailbox-secret-resolver-worker",
        "crates/cloudflare-adapters",
        "deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc",
        "migrations/resolver-d1",
        "scripts/mailbox-secret-resolver-release.py",
        "scripts/mailbox-secret-resolver-promotion.py",
        ".github/workflows/mailbox-secret-resolver-release.yml",
        ".github/workflows/mailbox-secret-resolver-promotion.yml",
    ],
    "release_workflow": str(RELEASE_WORKFLOW),
    "promotion_workflow": str(PROMOTION_WORKFLOW),
    "same_bits_staging_production": True,
    "first_control_plane_deploy_uses_secrets_file": True,
    "openapi_v1_changed": False,
}
EXPECTED_RESOLVER_SECRETS = {
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    "MAILBOX_RESOLVER_ENCRYPTION_KEYRING",
    "MAILBOX_RESOLVER_HANDLE_HMAC_KEY",
    "MICROSOFT_OAUTH_CLIENT_SECRET",
}
EXPECTED_CONTROL_SECRETS = {
    "CLIENT_CONTACT_PROTECTION_KEYRING",
    "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    "R2_GENERATION_ACCESS_KEY_ID",
    "R2_GENERATION_SECRET_ACCESS_KEY",
}
EXPECTED_RESOLVER_VARS = {
    "GOOGLE_OAUTH_CLIENT_ID",
    "GOOGLE_OAUTH_REDIRECT_URI",
    "MICROSOFT_OAUTH_CLIENT_ID",
    "MICROSOFT_OAUTH_REDIRECT_URI",
}
ALLOWED_IMPLEMENTATION_PATHS = {
    "Cargo.lock",
    "Cargo.toml",
    ".github/workflows/mailbox-secret-resolver-release.yml",
    ".github/workflows/mailbox-secret-resolver-promotion.yml",
    ".github/workflows/quality-gate.yml",
    "architecture/inventory.json",
    "architecture/pre2j-d3-resolver-bootstrap-implementation.json",
    "deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc",
    "deploy/cloudflare/wrangler.jsonc",
    "scripts/check-pre2j-d3-resolver-bootstrap-implementation.py",
    "scripts/check-d1-boundary.py",
    "scripts/check-cloudflare-runtime-bindings.py",
    "scripts/cloudflare-deploy-config.py",
    "scripts/check-phase2i-release-freeze.sh",
    "scripts/check-phase2e-mailbox-boundaries.py",
    "scripts/mailbox-secret-resolver-promotion.py",
    "scripts/mailbox-secret-resolver-release.py",
    "scripts/verify-fast.py",
}
ALLOWED_IMPLEMENTATION_PREFIXES = (
    "apps/mailbox-secret-resolver-worker/",
    "crates/cloudflare-adapters/src/",
    "migrations/resolver-d1/",
)
FORBIDDEN_RESOLVER_LOG_MARKERS = (
    "console_log!",
    "console_error!",
    "dbg!(",
    "println!(",
    "tracing::",
)
FORBIDDEN_SENSITIVE_HEADERS = (
    "x-profile-access-token",
    "x-profile-authorization-code",
    "x-profile-password",
    "x-profile-refresh-token",
    "x-profile-tenant-id",
)


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=ROOT, check=check, text=True, capture_output=True)


def read(path: Path) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def load(path: Path) -> dict[str, Any]:
    value = json.loads(read(path))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256((ROOT / path).read_bytes()).hexdigest()


def marker_errors(marker: dict[str, Any]) -> list[str]:
    return [] if marker == EXPECTED_MARKER else [f"{MARKER}: exact implementation marker drifted"]


def endpoint_inventory_errors(
    authority_endpoints: Any, adapter_endpoints: set[str], runtime_endpoints: set[str]
) -> list[str]:
    if not isinstance(authority_endpoints, list):
        return ["accepted resolver endpoint authority is not a list"]
    accepted = set(authority_endpoints)
    errors: list[str] = []
    if len(authority_endpoints) != 23 or len(accepted) != 23:
        errors.append("accepted resolver endpoint inventory must contain exactly 23 unique paths")
    if adapter_endpoints != accepted:
        errors.append("adapter resolver endpoint inventory drifted from accepted authority")
    if runtime_endpoints != accepted:
        errors.append("runtime resolver endpoint inventory drifted from accepted authority")
    return errors


def endpoint_errors(authority: dict[str, Any]) -> list[str]:
    adapter_endpoints = {
        match.group(1)
        for path in (ROOT / ADAPTER_ROOT).glob("*.rs")
        for match in ADAPTER_ENDPOINT_RE.finditer(path.read_text(encoding="utf-8"))
    }
    model = read(RESOLVER_ROOT / "src/model.rs")
    all_paths = model.split("pub const ALL_PATHS", 1)[1].split("];", 1)[0]
    runtime_endpoints = set(RUNTIME_ENDPOINT_RE.findall(all_paths))
    return endpoint_inventory_errors(
        authority.get("resolver_endpoints"), adapter_endpoints, runtime_endpoints
    )


def resolver_config_errors(config: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if config.get("name") != "mailbox-secret-resolver" or config.get("workers_dev") is not False:
        errors.append("resolver base config must remain private and canonical")
    if config.get("triggers") != {"crons": ["17 * * * *"]}:
        errors.append("resolver bounded key-reconciliation schedule drifted")
    build = config.get("build")
    if not isinstance(build, dict) or build.get("command") != (
        "cargo install worker-build --version 0.8.5 --locked && worker-build --release"
    ):
        errors.append("resolver canonical build is not exactly pinned")
    environments = config.get("env")
    if not isinstance(environments, dict) or set(environments) != {"staging", "production"}:
        return errors + ["resolver config must define exactly staging and production"]
    d1_ids: set[str] = set()
    for environment in ("staging", "production"):
        item = environments.get(environment)
        if not isinstance(item, dict):
            errors.append(f"resolver {environment} config is missing")
            continue
        if item.get("workers_dev") is not False or item.get("routes") != []:
            errors.append(f"resolver {environment} has public reachability")
        variables = item.get("vars")
        if not isinstance(variables, dict) or set(variables) != EXPECTED_RESOLVER_VARS:
            errors.append(f"resolver {environment} variable inventory drifted")
        secrets = item.get("secrets")
        required = secrets.get("required") if isinstance(secrets, dict) else None
        if not isinstance(required, list) or set(required) != EXPECTED_RESOLVER_SECRETS:
            errors.append(f"resolver {environment} secret-name inventory drifted")
        databases = item.get("d1_databases")
        if not isinstance(databases, list) or len(databases) != 1:
            errors.append(f"resolver {environment} dedicated D1 binding is missing")
            continue
        database = databases[0]
        if not isinstance(database, dict) or database.get("binding") != "RESOLVER_DB":
            errors.append(f"resolver {environment} D1 binding drifted")
            continue
        d1_ids.add(str(database.get("database_id")))
    if len(d1_ids) != 2:
        errors.append("resolver staging and production D1 bindings are not isolated")
    return errors


def runtime_errors() -> list[str]:
    errors: list[str] = []
    files = {path.name: path.read_text(encoding="utf-8") for path in (ROOT / RESOLVER_ROOT / "src").glob("*.rs")}
    all_text = "\n".join(files.values())
    required_markers = {
        "crypto.rs": (
            "Aes256Gcm",
            "Hmac<Sha256>",
            "active_version",
            "keys.len() > 4",
            "AuthenticatedContext",
            "nonce_hex",
        ),
        "ingress.rs": (
            "MAX_REQUEST_BYTES",
            "x-resolver-signature-version",
            "x-resolver-body-sha256",
            "x-resolver-timestamp-ms",
            "x-resolver-nonce",
            "x-resolver-signature",
            "CrossTenantState",
        ),
        "storage.rs": (
            "reconcile_key_rotation",
            "resolver_key_rotation_runs",
            "status = 'VERIFIED'",
            "key_version <> ?",
            "discarded_at_ms IS NULL",
        ),
        "lib.rs": ("#[event(fetch", "#[event(scheduled)]", "claim_request_nonce", "dispatch_operation"),
    }
    for name, markers in required_markers.items():
        text = files.get(name, "")
        for marker in markers:
            if marker not in text:
                errors.append(f"resolver runtime is missing {name} marker {marker!r}")
    model = files.get("model.rs", "")
    if "MAX_REQUEST_BYTES: usize = 32 * 1024" not in model or "AES_GCM_NONCE_BYTES: usize = 12" not in model:
        errors.append("resolver body or AES-GCM nonce bound drifted")
    lib = files.get("lib.rs", "")
    main_body = lib[lib.find("pub async fn main") :]
    order = [
        main_body.find(marker)
        for marker in ("authenticate_request(", "claim_request_nonce(", "dispatch_operation(")
    ]
    if any(index < 0 for index in order) or order != sorted(order):
        errors.append("resolver must authenticate and claim replay nonce before provider dispatch")
    for marker in FORBIDDEN_RESOLVER_LOG_MARKERS:
        if marker in all_text:
            errors.append(f"resolver ordinary logging is forbidden: {marker}")
    if "todo!" in all_text.lower() or "unimplemented!" in all_text.lower():
        errors.append("resolver contains an implementation placeholder")
    return errors


def adapter_errors() -> list[str]:
    errors: list[str] = []
    helper = read(ADAPTER_ROOT / "resolver_request.rs")
    for marker in (
        "Hmac<Sha256>",
        "x-resolver-signature-version",
        "x-resolver-body-sha256",
        "x-resolver-timestamp-ms",
        "x-resolver-nonce",
        "x-resolver-signature",
        "serde_json::to_string",
        "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    ):
        if marker not in helper:
            errors.append(f"signed resolver caller helper is missing {marker!r}")
    all_text = "\n".join(
        path.read_text(encoding="utf-8") for path in (ROOT / ADAPTER_ROOT).glob("*.rs")
    ).lower()
    for header in FORBIDDEN_SENSITIVE_HEADERS:
        if header in all_text:
            errors.append(f"sensitive resolver header transport remains: {header}")
    return errors


def migration_errors() -> list[str]:
    paths = sorted((ROOT / RESOLVER_MIGRATION_ROOT).glob("*.sql"))
    if not paths:
        return ["resolver D1 migration set is empty"]
    text = "\n".join(path.read_text(encoding="utf-8") for path in paths)
    tables = set(re.findall(r"CREATE TABLE ([a-z0-9_]+)", text))
    expected = {
        "resolver_request_nonces",
        "resolver_encrypted_records",
        "resolver_idempotency_records",
        "resolver_key_rotation_runs",
    }
    errors = [] if tables == expected else ["resolver D1 table inventory drifted"]
    for marker in (
        "key_version INTEGER NOT NULL",
        "nonce_hex TEXT NOT NULL",
        "ciphertext_hex TEXT NOT NULL",
        "CHECK (length(nonce_hex) = 24)",
        "status IN ('RUNNING', 'VERIFIED', 'FAILED')",
    ):
        if marker not in text:
            errors.append(f"resolver migration is missing security marker {marker!r}")
    for forbidden in ("password text", "access_token text", "refresh_token text", "authorization_code text"):
        if forbidden in text.lower():
            errors.append(f"resolver D1 persists plaintext credential field: {forbidden}")
    return errors


def workflow_errors(release: str, promotion: str) -> list[str]:
    errors: list[str] = []
    for marker in (
        "push:",
        "- main",
        "accepted-main",
        "worker-build --release",
        "mailbox-secret-resolver-release.py build",
        "upload-artifact@",
    ):
        if marker not in release:
            errors.append(f"resolver release workflow is missing {marker!r}")
    if "pull_request:" in release or "workflow_dispatch:" in release:
        errors.append("resolver build workflow must accept only accepted-main pushes")
    if release.count("worker-build --release") != 1:
        errors.append("resolver release workflow must build exactly once")
    for marker in (
        "workflow_dispatch:",
        "resolver_artifact_id:",
        "resolver_artifact_digest:",
        "control_plane_artifact_id:",
        "control_plane_artifact_digest:",
        "resolver_release_id:",
        "resolver_worker_sha256:",
        "control_plane_release_id:",
        "staging_promotion_run_id:",
        "staging_evidence_artifact_id:",
        "staging_evidence_artifact_digest:",
        "staging_run_attempt:",
        "staging_evidence_confirmation:",
        "confirmation:",
        "github-preflight",
        "download-artifact@",
        "artifact-ids:",
        "deploy --dry-run",
        "--strict",
        "--experimental-autoconfig=false",
        "validate-secrets",
        "validate-release-identities",
        "validate-staging-evidence",
        "verify-remote-d1",
        "deployments status",
        "attest",
        "umask 077",
        "chmod 0600",
        "trap 'rm -f",
        "CLOUDFLARE_RESOLVER_SECRETS_JSON",
        "CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON",
    ):
        if marker not in promotion:
            errors.append(f"resolver promotion workflow is missing {marker!r}")
    if "--no-bundle" in promotion:
        errors.append(
            "promotion must let pinned Wrangler package the immutable Worker module closure; "
            "--no-bundle rejects worker-build's shim module imports"
        )
    if "staging_evidence_path" in promotion or "docs/evidence/" in promotion:
        errors.append("production must accept immutable staging evidence artifacts, never tracked Git evidence")
    if promotion.count("--secrets-file") != 4:
        errors.append("both immutable artifact dry-runs and both Worker deploys must each use --secrets-file")
    if promotion.count("artifact-ids:") != 5:
        errors.append("preflight and deployment must download exact release artifacts, plus production staging evidence, by GitHub artifact id")
    if promotion.count("--strict") != 4:
        errors.append("both immutable artifact dry-runs and both Worker deployments must use strict validation")
    if promotion.count("--experimental-autoconfig=false") != 4:
        errors.append("both immutable artifact dry-runs and both Worker deployments must disable Wrangler automatic configuration")
    if promotion.count("deploy --dry-run") != 2:
        errors.append("resolver and control-plane immutable artifacts must each pass pinned Wrangler dry-run")
    preflight_job = promotion.find("\n  preflight:")
    deploy_job = promotion.find("\n  promote-same-bits:")
    if (
        preflight_job < 0
        or deploy_job < 0
        or preflight_job >= deploy_job
        or "needs: [preflight]" not in promotion
    ):
        errors.append("environment-secret deployment must depend on exact accepted-main preflight")
    resolver_deploy = promotion.find('--config "$resolver_config"')
    control_deploy = promotion.find('--config "$control_config"')
    if resolver_deploy < 0 or control_deploy < 0 or resolver_deploy >= control_deploy:
        errors.append("promotion must deploy resolver before the exact control plane")
    for forbidden in (
        "worker-build --release",
        "cargo build",
        "wrangler d1 create",
        "wrangler r2 bucket create",
        "wrangler queues create",
        "BOOTSTRAP_API_TOKEN",
    ):
        if forbidden in promotion:
            errors.append(f"promotion contains forbidden rebuild/provisioning authority: {forbidden}")
    if promotion.count("validate-release-identities") != 2:
        errors.append("both preflight and deployment must bind manifests to exact release identities")
    if promotion.count("validate-staging-evidence") != 1:
        errors.append("production must validate one downloaded immutable staging evidence artifact")
    return errors


def quality_workflow_errors(quality: str) -> list[str]:
    try:
        rust_linux = quality.split("\n  rust-linux:", 1)[1].split("\n  d1-catalog:", 1)[0]
    except IndexError:
        return ["quality workflow Rust Linux job is unavailable"]
    errors: list[str] = []
    if "fetch-depth: 0" not in rust_linux:
        errors.append("quality workflow must fetch full history for the accepted implementation base")
    for marker in (
        "github.event.pull_request.base.sha || github.event.before",
        '--base-ref "$IMPLEMENTATION_BASE"',
    ):
        if marker not in rust_linux:
            errors.append(f"quality workflow implementation base interlock is missing {marker!r}")
    return errors


def release_script_errors() -> list[str]:
    release = read(Path("scripts/mailbox-secret-resolver-release.py"))
    promotion = read(Path("scripts/mailbox-secret-resolver-promotion.py"))
    errors: list[str] = []
    for marker in (
        "IDENTITY_FIELDS",
        "source_commit_sha",
        "resolver_worker_sha256",
        "resolver_migration_manifest_sha256",
        "resolver_config_sha256",
        "build_toolchain",
        "GENERATED_METADATA_FILES",
        "deterministic_tar",
        "safe_extract",
    ):
        if marker not in release:
            errors.append(f"resolver release verifier is missing {marker!r}")
    for marker in (
        "require_mode_0600",
        "cross-environment-identical secret documents are forbidden",
        "caller-auth secret must match both Workers",
        "render_resolver_config",
        "render_control_config",
        "validate_release_identities",
        "validate_staging_evidence_artifact",
        "validate_deployment_closures",
        "Production same-bits artifacts match immutable passed staging evidence",
    ):
        if marker not in promotion:
            errors.append(f"resolver promotion verifier is missing {marker!r}")
    return errors


def status_errors() -> list[str]:
    status = load(Path("docs/status.json"))
    return [] if status.get("production_ready") is False else ["production_ready must remain false"]


def current_errors() -> list[str]:
    errors: list[str] = []
    if sha256(AUTHORITY) != AUTHORITY_SHA256:
        errors.append("accepted resolver/bootstrap authority is not immutable")
    authority = load(AUTHORITY)
    errors.extend(marker_errors(load(MARKER)))
    errors.extend(endpoint_errors(authority))
    errors.extend(resolver_config_errors(load(RESOLVER_CONFIG)))
    errors.extend(runtime_errors())
    errors.extend(adapter_errors())
    errors.extend(migration_errors())
    errors.extend(workflow_errors(read(RELEASE_WORKFLOW), read(PROMOTION_WORKFLOW)))
    errors.extend(quality_workflow_errors(read(QUALITY_WORKFLOW)))
    errors.extend(release_script_errors())
    errors.extend(status_errors())
    control_config = load(Path("deploy/cloudflare/wrangler.jsonc"))
    environments = control_config.get("env")
    if not isinstance(environments, dict):
        errors.append("control-plane environments are missing")
    else:
        for environment in ("staging", "production"):
            item = environments.get(environment)
            secrets = item.get("secrets") if isinstance(item, dict) else None
            required = secrets.get("required") if isinstance(secrets, dict) else None
            if not isinstance(required, list) or set(required) != EXPECTED_CONTROL_SECRETS:
                errors.append(f"control-plane {environment} secret-name inventory drifted")
    cloudflare_release = read(Path("scripts/cloudflare-release.py"))
    for marker in (
        '"deployment_config"',
        "copy_tree_exact(root / \"migrations\" / \"d1\"",
        "migration_inventory(release_directory)",
    ):
        if marker not in cloudflare_release:
            errors.append(f"control-plane immutable release closure is missing {marker!r}")
    return errors


def diff_errors(base_ref: str) -> list[str]:
    errors: list[str] = []
    if git("cat-file", "-e", f"{base_ref}^{{commit}}", check=False).returncode != 0:
        return [f"implementation base ref is unavailable: {base_ref}"]
    changed = {
        path
        for path in git("diff", "--name-only", base_ref, "--").stdout.splitlines()
        if path
    }
    base_has_marker = git("cat-file", "-e", f"{base_ref}:{MARKER}", check=False).returncode == 0
    if base_has_marker and str(MARKER) not in changed:
        return []
    accepted_authority = git("show", f"{base_ref}:{AUTHORITY}", check=False)
    if accepted_authority.returncode != 0 or accepted_authority.stdout != read(AUTHORITY):
        errors.append("accepted resolver/bootstrap authority changed in the implementation unit")
    if any(path.startswith("openapi/v1/") for path in changed):
        errors.append("resolver implementation unit must not change openapi/v1")
    unexpected = sorted(
        path
        for path in changed
        if path not in ALLOWED_IMPLEMENTATION_PATHS
        and not any(path.startswith(prefix) for prefix in ALLOWED_IMPLEMENTATION_PREFIXES)
    )
    if unexpected:
        errors.append(f"resolver implementation diff escaped its bounded inventory: {unexpected}")
    return errors


def self_test() -> None:
    errors = current_errors()
    if errors:
        raise AssertionError(errors)
    marker = copy.deepcopy(EXPECTED_MARKER)
    marker["production_ready"] = True
    assert marker_errors(marker)
    authority = load(AUTHORITY)
    accepted = set(authority["resolver_endpoints"])
    missing = set(accepted)
    missing.pop()
    assert endpoint_inventory_errors(authority["resolver_endpoints"], missing, accepted)
    release = read(RELEASE_WORKFLOW)
    promotion = read(PROMOTION_WORKFLOW)
    quality = read(QUALITY_WORKFLOW)
    assert workflow_errors(release, promotion + "\nworker-build --release")
    assert workflow_errors(release, promotion.replace("--secrets-file", "--secret-file"))
    assert workflow_errors(release, promotion.replace("artifact-ids:", "pattern:", 1))
    assert workflow_errors(release, promotion.replace("github-preflight", "unchecked-preflight", 1))
    assert workflow_errors(release, promotion + "\n--no-bundle")
    assert workflow_errors(release, promotion.replace("staging_evidence_artifact_id", "tracked_evidence_path", 1))
    assert quality_workflow_errors(quality.replace("fetch-depth: 0", "fetch-depth: 1", 1))
    config = load(RESOLVER_CONFIG)
    tampered = copy.deepcopy(config)
    tampered["env"]["staging"]["routes"] = ["staging.example.test/*"]
    assert resolver_config_errors(tampered)
    print("D3 resolver implementation negative policy self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-ref")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    errors = current_errors()
    if args.base_ref:
        errors.extend(diff_errors(args.base_ref))
    if errors:
        raise SystemExit("\n".join(errors))
    print("D3 resolver/bootstrap implementation and no-rebuild ceremony are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
