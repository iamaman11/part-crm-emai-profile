#!/usr/bin/env python3
"""Validate current mailbox-secret-resolver security and D3 transition provenance.

The historical filename is retained only until the bounded post-AR11 checker-name cleanup
because it is part of the accepted Python-estate path set. This checker never materializes,
imports, or executes retired promotion code. Historical transition provenance is delegated
to the static AR-8D successor validator.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
RESOLVER_CONFIG = Path("deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc")
RESOLVER_ROOT = Path("apps/mailbox-secret-resolver-worker")
RESOLVER_MIGRATION_ROOT = Path("migrations/resolver-d1")
ADAPTER_ROOT = Path("crates/cloudflare-adapters/src")
RELEASE_WORKFLOW = Path(".github/workflows/mailbox-secret-resolver-release.yml")
RELEASE_SCRIPT = Path("scripts/mailbox-secret-resolver-release.py")
STATUS = Path("docs/status.json")
STATIC_TRANSITION_CHECKER = Path(".github/scripts/ar8-d-secret-transport-successor.mjs")

ADAPTER_ENDPOINT_RE = re.compile(r'https://mailbox-secret-resolver\.internal([^"\s]+)')
RUNTIME_ENDPOINT_RE = re.compile(r'"(/v1/mailbox-credentials/[^"\s]+)"')
EXPECTED_RESOLVER_SECRETS = {
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    "MAILBOX_RESOLVER_ENCRYPTION_KEYRING",
    "MAILBOX_RESOLVER_HANDLE_HMAC_KEY",
    "MICROSOFT_OAUTH_CLIENT_SECRET",
}
EXPECTED_RESOLVER_VARS = {
    "GOOGLE_OAUTH_CLIENT_ID",
    "GOOGLE_OAUTH_REDIRECT_URI",
    "MICROSOFT_OAUTH_CLIENT_ID",
    "MICROSOFT_OAUTH_REDIRECT_URI",
}
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


def read(path: Path) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def load(path: Path) -> dict[str, Any]:
    value = json.loads(read(path))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def endpoint_inventory_errors(adapter_endpoints: set[str], runtime_endpoints: set[str]) -> list[str]:
    errors: list[str] = []
    if not adapter_endpoints:
        errors.append("current resolver caller endpoint inventory is empty")
    if adapter_endpoints != runtime_endpoints:
        errors.append("current resolver caller/runtime endpoint inventories differ")
    return errors


def endpoint_errors() -> list[str]:
    adapter_endpoints = {
        match.group(1)
        for path in (ROOT / ADAPTER_ROOT).glob("*.rs")
        for match in ADAPTER_ENDPOINT_RE.finditer(path.read_text(encoding="utf-8"))
    }
    model = read(RESOLVER_ROOT / "src/model.rs")
    if "pub const ALL_PATHS" not in model:
        return ["current resolver runtime path authority is missing ALL_PATHS"]
    all_paths = model.split("pub const ALL_PATHS", 1)[1].split("];", 1)[0]
    runtime_endpoints = set(RUNTIME_ENDPOINT_RE.findall(all_paths))
    return endpoint_inventory_errors(adapter_endpoints, runtime_endpoints)


def resolver_config_errors(config: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if config.get("name") != "mailbox-secret-resolver" or config.get("workers_dev") is not False:
        errors.append("current resolver base config must remain private and canonical")
    if config.get("triggers") != {"crons": ["17 * * * *"]}:
        errors.append("current resolver key-reconciliation schedule drifted")
    build = config.get("build")
    if not isinstance(build, dict) or build.get("command") != (
        "cargo install worker-build --version 0.8.5 --locked && worker-build --release"
    ):
        errors.append("current resolver build toolchain is not exactly pinned")
    environments = config.get("env")
    if not isinstance(environments, dict) or set(environments) != {"staging", "production"}:
        return errors + ["current resolver config must define exactly staging and production"]
    d1_ids: set[str] = set()
    for environment in ("staging", "production"):
        item = environments.get(environment)
        if not isinstance(item, dict):
            errors.append(f"current resolver {environment} config is missing")
            continue
        if item.get("workers_dev") is not False or item.get("routes") != []:
            errors.append(f"current resolver {environment} has public reachability")
        variables = item.get("vars")
        if not isinstance(variables, dict) or set(variables) != EXPECTED_RESOLVER_VARS:
            errors.append(f"current resolver {environment} variable inventory drifted")
        secrets = item.get("secrets")
        required = secrets.get("required") if isinstance(secrets, dict) else None
        if not isinstance(required, list) or set(required) != EXPECTED_RESOLVER_SECRETS:
            errors.append(f"current resolver {environment} secret-name inventory drifted")
        databases = item.get("d1_databases")
        if not isinstance(databases, list) or len(databases) != 1:
            errors.append(f"current resolver {environment} dedicated D1 binding is missing")
            continue
        database = databases[0]
        if (
            not isinstance(database, dict)
            or database.get("binding") != "RESOLVER_DB"
            or database.get("migrations_dir") != "../../migrations/resolver-d1"
        ):
            errors.append(f"current resolver {environment} D1 boundary drifted")
            continue
        identity = database.get("database_id")
        if not isinstance(identity, str) or not identity:
            errors.append(f"current resolver {environment} D1 identity is missing")
            continue
        d1_ids.add(identity)
    if len(d1_ids) != 2:
        errors.append("current resolver staging and production D1 identities are not isolated")
    return errors


def runtime_errors() -> list[str]:
    errors: list[str] = []
    files = {
        path.name: path.read_text(encoding="utf-8")
        for path in (ROOT / RESOLVER_ROOT / "src").glob("*.rs")
    }
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
        "lib.rs": (
            "#[event(fetch",
            "#[event(scheduled)]",
            "claim_request_nonce",
            "dispatch_operation",
        ),
    }
    for name, markers in required_markers.items():
        text = files.get(name, "")
        for marker in markers:
            if marker not in text:
                errors.append(f"current resolver runtime is missing {name} invariant {marker!r}")
    model = files.get("model.rs", "")
    if (
        "MAX_REQUEST_BYTES: usize = 32 * 1024" not in model
        or "AES_GCM_NONCE_BYTES: usize = 12" not in model
    ):
        errors.append("current resolver request or AES-GCM nonce bound drifted")
    lib = files.get("lib.rs", "")
    main_body = lib[lib.find("pub async fn main") :]
    order = [
        main_body.find(marker)
        for marker in ("authenticate_request(", "claim_request_nonce(", "dispatch_operation(")
    ]
    if any(index < 0 for index in order) or order != sorted(order):
        errors.append("current resolver must authenticate and claim replay nonce before dispatch")
    for marker in FORBIDDEN_RESOLVER_LOG_MARKERS:
        if marker in all_text:
            errors.append(f"current resolver ordinary logging is forbidden: {marker}")
    if "todo!" in all_text.lower() or "unimplemented!" in all_text.lower():
        errors.append("current resolver contains an implementation placeholder")
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
            errors.append(f"current signed resolver caller is missing {marker!r}")
    all_text = "\n".join(
        path.read_text(encoding="utf-8") for path in (ROOT / ADAPTER_ROOT).glob("*.rs")
    ).lower()
    for header in FORBIDDEN_SENSITIVE_HEADERS:
        if header in all_text:
            errors.append(f"current resolver transport exposes forbidden sensitive header: {header}")
    return errors


def migration_text_errors(text: str) -> list[str]:
    tables = set(re.findall(r"CREATE TABLE ([a-z0-9_]+)", text))
    expected = {
        "resolver_request_nonces",
        "resolver_encrypted_records",
        "resolver_idempotency_records",
        "resolver_key_rotation_runs",
    }
    errors = [] if tables == expected else ["current resolver D1 table inventory drifted"]
    for marker in (
        "key_version INTEGER NOT NULL",
        "nonce_hex TEXT NOT NULL",
        "ciphertext_hex TEXT NOT NULL",
        "CHECK (length(nonce_hex) = 24)",
        "status IN ('RUNNING', 'VERIFIED', 'FAILED')",
    ):
        if marker not in text:
            errors.append(f"current resolver migration security invariant is missing {marker!r}")
    for forbidden in (
        "password text",
        "access_token text",
        "refresh_token text",
        "authorization_code text",
    ):
        if forbidden in text.lower():
            errors.append(f"current resolver D1 persists plaintext credential field: {forbidden}")
    return errors


def migration_errors() -> list[str]:
    paths = sorted((ROOT / RESOLVER_MIGRATION_ROOT).glob("*.sql"))
    if not paths:
        return ["current resolver D1 migration set is empty"]
    return migration_text_errors("\n".join(path.read_text(encoding="utf-8") for path in paths))


def release_workflow_errors(release: str) -> list[str]:
    errors: list[str] = []
    for marker in (
        "push:",
        "- main",
        "accepted-main-release",
        "worker-build --release",
        "mailbox-secret-resolver-release.py build",
        "mailbox-secret-resolver-release.py verify-archive",
        "upload-artifact@",
    ):
        if marker not in release:
            errors.append(f"current resolver release workflow is missing {marker!r}")
    if "pull_request:" in release or "workflow_dispatch:" in release:
        errors.append("current resolver build workflow must accept only accepted-main pushes")
    if release.count("worker-build --release") != 1:
        errors.append("current resolver release workflow must build exactly once")
    for forbidden in (
        "wrangler deploy",
        "wrangler d1 create",
        "wrangler r2 bucket create",
        "wrangler queues create",
        "CLOUDFLARE_RESOLVER_SECRETS_JSON",
        "CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON",
    ):
        if forbidden in release:
            errors.append(f"current resolver release workflow contains forbidden mutation/input: {forbidden}")
    return errors


def release_script_errors() -> list[str]:
    release = read(RELEASE_SCRIPT)
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
            errors.append(f"current resolver release verifier is missing {marker!r}")
    return errors


def production_state_errors() -> list[str]:
    status = load(STATUS)
    current = status.get("current")
    errors: list[str] = []
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false")
    if not isinstance(current, dict) or current.get("architecture_complete") is not False:
        errors.append("architecture_complete must remain false")
    if not isinstance(current, dict) or current.get("production_core_gate") != "BLOCKED":
        errors.append("production_core_gate must remain BLOCKED")
    return errors


def current_errors() -> list[str]:
    errors: list[str] = []
    errors.extend(endpoint_errors())
    errors.extend(resolver_config_errors(load(RESOLVER_CONFIG)))
    errors.extend(runtime_errors())
    errors.extend(adapter_errors())
    errors.extend(migration_errors())
    errors.extend(release_workflow_errors(read(RELEASE_WORKFLOW)))
    errors.extend(release_script_errors())
    errors.extend(production_state_errors())
    return errors


def run_static_transition(*, self_test: bool = False) -> list[str]:
    command = ["node", STATIC_TRANSITION_CHECKER.as_posix()]
    if self_test:
        command.append("--self-test")
    completed = subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        text=True,
        capture_output=True,
    )
    if completed.returncode == 0:
        return []
    details = "\n".join(
        part.strip() for part in (completed.stdout, completed.stderr) if part.strip()
    )
    return [f"static D3 transition provenance failed: {details or completed.returncode}"]


def self_test() -> None:
    errors = current_errors()
    if errors:
        raise AssertionError(errors)

    config = load(RESOLVER_CONFIG)
    tampered = copy.deepcopy(config)
    tampered["env"]["staging"]["routes"] = ["staging.example.test/*"]
    assert resolver_config_errors(tampered)

    adapter = {"/v1/mailbox-credentials/resolve"}
    assert endpoint_inventory_errors(adapter, set())

    migration_text = "\n".join(
        path.read_text(encoding="utf-8")
        for path in sorted((ROOT / RESOLVER_MIGRATION_ROOT).glob("*.sql"))
    )
    assert migration_text_errors(migration_text + "\npassword TEXT\n")

    release = read(RELEASE_WORKFLOW)
    assert release_workflow_errors(release + "\nwrangler deploy --env production\n")

    transition_errors = run_static_transition(self_test=True)
    if transition_errors:
        raise AssertionError(transition_errors)
    print("Current resolver security and static transition negative fixtures passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--base-ref",
        help=(
            "deprecated compatibility input; accepted but intentionally ignored because current "
            "CI no longer replays or diffs the retired D3 implementation"
        ),
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0

    errors = current_errors()
    errors.extend(run_static_transition())
    if errors:
        raise SystemExit("\n".join(errors))
    print(
        "Current mailbox-secret-resolver security, build-once release policy, and static D3 transition provenance passed."
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Current D3 resolver security gate failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
