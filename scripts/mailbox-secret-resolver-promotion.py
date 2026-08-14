#!/usr/bin/env python3
"""Validate resolver promotion inputs and render no-rebuild Wrangler configs."""

from __future__ import annotations

import argparse
import copy
import json
import os
import re
import stat
import tempfile
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
RESOLVER_CONFIG = ROOT / "deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc"
CONTROL_CONFIG = ROOT / "deploy/cloudflare/wrangler.jsonc"
ENVIRONMENTS = ("staging", "production")
RESOLVER_SECRET_NAMES = {
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    "MAILBOX_RESOLVER_ENCRYPTION_KEYRING",
    "MAILBOX_RESOLVER_HANDLE_HMAC_KEY",
    "MICROSOFT_OAUTH_CLIENT_SECRET",
}
CONTROL_SECRET_NAMES = {
    "CLIENT_CONTACT_PROTECTION_KEYRING",
    "MAILBOX_RESOLVER_CALLER_AUTH_KEY",
    "R2_GENERATION_ACCESS_KEY_ID",
    "R2_GENERATION_SECRET_ACCESS_KEY",
}
CONTROL_FIELDS = {
    "worker_name",
    "account_id",
    "custom_domain",
    "access_issuer",
    "access_audience",
    "d1_database_name",
    "d1_database_id",
    "r2_bucket_name",
    "generation_verification_queue",
    "integration_events_queue",
    "mailbox_jobs_queue",
    "mailbox_jobs_dlq",
    "mailbox_secret_resolver_service",
}
RESOLVER_FIELDS = {
    "worker_name",
    "account_id",
    "d1_database_name",
    "d1_database_id",
    "google_oauth_client_id",
    "google_oauth_redirect_uri",
    "microsoft_oauth_client_id",
    "microsoft_oauth_redirect_uri",
}
RESOURCE_RE = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$")
ACCOUNT_RE = re.compile(r"^[0-9a-f]{32}$")
D1_RE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
AUDIENCE_RE = re.compile(r"^[A-Za-z0-9_-]{16,128}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
FORBIDDEN_VALUE_MARKERS = (
    "${",
    "changeme",
    "dummy",
    "example",
    "placeholder",
    "replace_with",
    "secret-value",
    "todo",
)


class PromotionError(ValueError):
    """Fail-closed promotion-input validation error."""


def fail(message: str) -> None:
    raise PromotionError(message)


def load_json(path: Path, label: str, *, maximum_bytes: int = 64 * 1024) -> Any:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file")
    if path.stat().st_size == 0 or path.stat().st_size > maximum_bytes:
        fail(f"{label} has an invalid bounded size")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromotionError(f"{label} is not strict UTF-8 JSON") from error


def require_object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    return value


def require_exact_string_object(value: Any, names: set[str], label: str) -> dict[str, str]:
    document = require_object(value, label)
    if set(document) != names:
        fail(f"{label} name inventory mismatch")
    result: dict[str, str] = {}
    for name in sorted(names):
        item = document[name]
        if not isinstance(item, str) or not item.strip() or len(item.encode()) > 16 * 1024:
            fail(f"{label}.{name} must be one bounded non-empty string")
        lowered = item.lower()
        if any(marker in lowered for marker in FORBIDDEN_VALUE_MARKERS):
            fail(f"{label}.{name} contains a forbidden placeholder marker")
        result[name] = item
    return result


def require_mode_0600(path: Path, label: str) -> None:
    mode = stat.S_IMODE(path.stat().st_mode)
    if mode != 0o600:
        fail(f"{label} must have mode 0600")


def validate_encryption_keyring(value: str) -> set[int]:
    try:
        document = json.loads(value)
    except json.JSONDecodeError as error:
        raise PromotionError("resolver encryption keyring must be strict JSON") from error
    keyring = require_object(document, "resolver encryption keyring")
    if set(keyring) != {"activeVersion", "keys"}:
        fail("resolver encryption keyring field inventory drifted")
    active = keyring["activeVersion"]
    keys = keyring["keys"]
    if not isinstance(active, int) or active <= 0 or not isinstance(keys, list) or not 1 <= len(keys) <= 4:
        fail("resolver encryption keyring active/retained bounds are invalid")
    versions: set[int] = set()
    for item in keys:
        entry = require_object(item, "resolver encryption key")
        if set(entry) != {"version", "keyHex"}:
            fail("resolver encryption key field inventory drifted")
        version = entry["version"]
        key_hex = entry["keyHex"]
        if (
            not isinstance(version, int)
            or version <= 0
            or version in versions
            or not isinstance(key_hex, str)
            or re.fullmatch(r"[0-9a-fA-F]{64}", key_hex) is None
        ):
            fail("resolver encryption key version/material shape is invalid")
        versions.add(version)
    if active not in versions:
        fail("resolver active encryption key is not retained")
    return versions


def validate_secret_documents(
    resolver_path: Path,
    control_path: Path,
    *,
    peer_resolver_path: Path | None = None,
    peer_control_path: Path | None = None,
) -> None:
    require_mode_0600(resolver_path, "resolver secret document")
    require_mode_0600(control_path, "control-plane secret document")
    resolver = require_exact_string_object(
        load_json(resolver_path, "resolver secret document", maximum_bytes=32 * 1024),
        RESOLVER_SECRET_NAMES,
        "resolver secret document",
    )
    control = require_exact_string_object(
        load_json(control_path, "control-plane secret document", maximum_bytes=32 * 1024),
        CONTROL_SECRET_NAMES,
        "control-plane secret document",
    )
    caller_key = resolver["MAILBOX_RESOLVER_CALLER_AUTH_KEY"]
    if caller_key != control["MAILBOX_RESOLVER_CALLER_AUTH_KEY"] or not 32 <= len(caller_key) <= 128:
        fail("caller-auth secret must match both Workers within one environment")
    handle_key = resolver["MAILBOX_RESOLVER_HANDLE_HMAC_KEY"]
    if not 32 <= len(handle_key) <= 128:
        fail("resolver handle-HMAC secret length is invalid")
    validate_encryption_keyring(resolver["MAILBOX_RESOLVER_ENCRYPTION_KEYRING"])

    if (peer_resolver_path is None) != (peer_control_path is None):
        fail("cross-environment validation requires both peer secret documents")
    if peer_resolver_path is None or peer_control_path is None:
        return
    require_mode_0600(peer_resolver_path, "peer resolver secret document")
    require_mode_0600(peer_control_path, "peer control-plane secret document")
    peer_resolver = require_exact_string_object(
        load_json(peer_resolver_path, "peer resolver secret document", maximum_bytes=32 * 1024),
        RESOLVER_SECRET_NAMES,
        "peer resolver secret document",
    )
    peer_control = require_exact_string_object(
        load_json(peer_control_path, "peer control-plane secret document", maximum_bytes=32 * 1024),
        CONTROL_SECRET_NAMES,
        "peer control-plane secret document",
    )
    if resolver == peer_resolver or control == peer_control:
        fail("cross-environment-identical secret documents are forbidden")
    reused = {
        name
        for name in RESOLVER_SECRET_NAMES & set(peer_resolver)
        if resolver[name] == peer_resolver[name]
    } | {
        name
        for name in CONTROL_SECRET_NAMES & set(peer_control)
        if control[name] == peer_control[name]
    }
    if reused:
        fail(f"cross-environment secret-value reuse is forbidden: {sorted(reused)}")


def bounded_string(document: dict[str, Any], name: str, label: str) -> str:
    value = document.get(name)
    if not isinstance(value, str) or not value or len(value) > 512:
        fail(f"{label}.{name} must be one bounded non-empty string")
    lowered = value.lower()
    if any(marker in lowered for marker in FORBIDDEN_VALUE_MARKERS):
        fail(f"{label}.{name} contains a forbidden placeholder")
    return value


def validate_control_manifest(value: Any, environment: str) -> dict[str, str]:
    document = require_object(value, "control-plane deploy manifest")
    if set(document) != CONTROL_FIELDS:
        fail("control-plane deploy manifest field inventory mismatch")
    result = {name: bounded_string(document, name, "control_plane") for name in CONTROL_FIELDS}
    for name in (
        "worker_name",
        "d1_database_name",
        "r2_bucket_name",
        "generation_verification_queue",
        "integration_events_queue",
        "mailbox_jobs_queue",
        "mailbox_jobs_dlq",
        "mailbox_secret_resolver_service",
    ):
        if RESOURCE_RE.fullmatch(result[name]) is None:
            fail(f"control_plane.{name} is not a bounded resource name")
    if ACCOUNT_RE.fullmatch(result["account_id"]) is None or D1_RE.fullmatch(result["d1_database_id"]) is None:
        fail("control-plane account or D1 identity has an invalid shape")
    if AUDIENCE_RE.fullmatch(result["access_audience"]) is None:
        fail("control-plane Access audience has an invalid shape")
    issuer = urlparse(result["access_issuer"])
    if issuer.scheme != "https" or not issuer.hostname or issuer.path not in ("", "/") or issuer.query or issuer.fragment:
        fail("control-plane Access issuer must be one HTTPS origin")
    custom_domain = result["custom_domain"]
    if "/" in custom_domain or ":" in custom_domain or "." not in custom_domain or custom_domain.endswith(".workers.dev"):
        fail("control-plane custom domain is invalid")
    queues = {
        result["generation_verification_queue"],
        result["integration_events_queue"],
        result["mailbox_jobs_queue"],
        result["mailbox_jobs_dlq"],
    }
    if len(queues) != 4 or environment not in result["worker_name"]:
        fail("control-plane environment resources are not isolated")
    return result


def validate_resolver_manifest(
    value: Any, environment: str, control: dict[str, str]
) -> dict[str, str]:
    document = require_object(value, "resolver deploy manifest")
    if set(document) != RESOLVER_FIELDS:
        fail("resolver deploy manifest field inventory mismatch")
    result = {name: bounded_string(document, name, "resolver") for name in RESOLVER_FIELDS}
    expected_name = f"mailbox-secret-resolver-{environment}"
    if result["worker_name"] != expected_name or result["worker_name"] != control["mailbox_secret_resolver_service"]:
        fail("resolver Worker identity differs from the accepted service binding")
    if result["account_id"] != control["account_id"] or ACCOUNT_RE.fullmatch(result["account_id"]) is None:
        fail("resolver and control plane must target the same selected account")
    if RESOURCE_RE.fullmatch(result["d1_database_name"]) is None or D1_RE.fullmatch(result["d1_database_id"]) is None:
        fail("resolver D1 identity has an invalid shape")
    if result["d1_database_id"] == control["d1_database_id"] or result["d1_database_name"] == control["d1_database_name"]:
        fail("resolver D1 must be isolated from the business/catalog D1")
    for provider in ("google", "microsoft"):
        redirect = urlparse(result[f"{provider}_oauth_redirect_uri"])
        if (
            redirect.scheme != "https"
            or redirect.hostname != control["custom_domain"]
            or not redirect.path.startswith("/")
            or redirect.path == "/"
            or redirect.fragment
        ):
            fail(f"{provider} OAuth redirect must terminate on the selected control-plane origin")
    return result


def validate_deploy_manifest(path: Path, environment: str) -> tuple[dict[str, str], dict[str, str]]:
    value = require_object(load_json(path, "deploy manifest"), "deploy manifest")
    if set(value) != {"schema_version", "environment", "control_plane", "resolver"}:
        fail("deploy manifest top-level field inventory mismatch")
    if value["schema_version"] != 1 or value["environment"] != environment:
        fail("deploy manifest environment/schema mismatch")
    control = validate_control_manifest(value["control_plane"], environment)
    resolver = validate_resolver_manifest(value["resolver"], environment, control)
    return control, resolver


def substitute(value: Any, replacements: dict[str, str]) -> Any:
    if isinstance(value, dict):
        return {name: substitute(item, replacements) for name, item in value.items()}
    if isinstance(value, list):
        return [substitute(item, replacements) for item in value]
    if isinstance(value, str):
        return replacements.get(value, value)
    return value


def relative_to_output(target: Path, output: Path) -> str:
    return Path(os.path.relpath(target.resolve(), output.parent.resolve())).as_posix()


def render_resolver_config(
    environment: str, manifest: dict[str, str], release_directory: Path, output: Path
) -> None:
    config = require_object(load_json(RESOLVER_CONFIG, "canonical resolver config"), "resolver config")
    selected = copy.deepcopy(require_object(config["env"][environment], f"resolver env.{environment}"))
    prefix = environment.upper()
    replacements = {
        f"${{{prefix}_ACCOUNT_ID}}": manifest["account_id"],
        f"${{{prefix}_RESOLVER_D1_DATABASE_NAME}}": manifest["d1_database_name"],
        f"${{{prefix}_RESOLVER_D1_DATABASE_ID}}": manifest["d1_database_id"],
        f"${{{prefix}_GOOGLE_OAUTH_CLIENT_ID}}": manifest["google_oauth_client_id"],
        f"${{{prefix}_GOOGLE_OAUTH_REDIRECT_URI}}": manifest["google_oauth_redirect_uri"],
        f"${{{prefix}_MICROSOFT_OAUTH_CLIENT_ID}}": manifest["microsoft_oauth_client_id"],
        f"${{{prefix}_MICROSOFT_OAUTH_REDIRECT_URI}}": manifest["microsoft_oauth_redirect_uri"],
    }
    selected = require_object(substitute(selected, replacements), "rendered resolver environment")
    selected.pop("secrets", None)
    selected["d1_databases"][0]["migrations_dir"] = relative_to_output(
        release_directory / "migrations/resolver-d1", output
    )
    rendered = {name: copy.deepcopy(value) for name, value in config.items() if name not in {"build", "env"}}
    rendered["main"] = relative_to_output(release_directory / "worker/worker/shim.mjs", output)
    rendered["env"] = {environment: selected}
    serialized = json.dumps(rendered, indent=2) + "\n"
    if "${" in serialized or '"build"' in serialized:
        fail("rendered resolver config retained a placeholder or rebuild command")
    output.write_text(serialized, encoding="utf-8")


def control_replacements(environment: str, manifest: dict[str, str]) -> dict[str, str]:
    prefix = environment.upper()
    mapping = {
        "WORKER_NAME": "worker_name",
        "ACCOUNT_ID": "account_id",
        "CUSTOM_DOMAIN": "custom_domain",
        "ACCESS_ISSUER": "access_issuer",
        "ACCESS_AUDIENCE": "access_audience",
        "D1_DATABASE_NAME": "d1_database_name",
        "D1_DATABASE_ID": "d1_database_id",
        "R2_BUCKET_NAME": "r2_bucket_name",
        "GENERATION_VERIFICATION_QUEUE": "generation_verification_queue",
        "INTEGRATION_EVENTS_QUEUE": "integration_events_queue",
        "MAILBOX_JOBS_QUEUE": "mailbox_jobs_queue",
        "MAILBOX_JOBS_DLQ": "mailbox_jobs_dlq",
        "MAILBOX_SECRET_RESOLVER_SERVICE": "mailbox_secret_resolver_service",
    }
    return {f"${{{prefix}_{token}}}": manifest[name] for token, name in mapping.items()}


def render_control_config(
    environment: str, manifest: dict[str, str], release_directory: Path, output: Path
) -> None:
    config = require_object(load_json(CONTROL_CONFIG, "canonical control-plane config"), "control config")
    selected = copy.deepcopy(require_object(config["env"][environment], f"control env.{environment}"))
    selected = require_object(
        substitute(selected, control_replacements(environment, manifest)),
        "rendered control-plane environment",
    )
    selected.pop("secrets", None)
    rendered = {name: copy.deepcopy(value) for name, value in config.items() if name not in {"build", "env"}}
    rendered["main"] = relative_to_output(release_directory / "worker/worker/shim.mjs", output)
    rendered["assets"]["directory"] = relative_to_output(release_directory / "frontend", output)
    rendered["env"] = {environment: selected}
    serialized = json.dumps(rendered, indent=2) + "\n"
    if "${" in serialized or '"build"' in serialized:
        fail("rendered control-plane config retained a placeholder or rebuild command")
    output.write_text(serialized, encoding="utf-8")


def prepare(
    environment: str,
    deploy_manifest: Path,
    resolver_release: Path,
    control_release: Path,
    resolver_output: Path,
    control_output: Path,
) -> None:
    if environment not in ENVIRONMENTS:
        fail("promotion environment must be staging or production")
    for directory, label in (
        (resolver_release, "resolver release directory"),
        (control_release, "control-plane release directory"),
    ):
        if directory.is_symlink() or not directory.is_dir():
            fail(f"{label} is missing")
    control, resolver = validate_deploy_manifest(deploy_manifest, environment)
    resolver_output.parent.mkdir(parents=True, exist_ok=True)
    control_output.parent.mkdir(parents=True, exist_ok=True)
    render_resolver_config(environment, resolver, resolver_release, resolver_output)
    render_control_config(environment, control, control_release, control_output)
    print(f"Prepared no-rebuild {environment} resolver and control-plane deployment configs.")


def validate_staging_evidence(
    evidence_path: Path,
    resolver_manifest_path: Path,
    control_manifest_path: Path,
) -> None:
    evidence = require_object(load_json(evidence_path, "accepted staging evidence"), "staging evidence")
    expected_fields = {
        "schema_version",
        "status",
        "environment",
        "resolver_release_id",
        "resolver_source_commit_sha",
        "resolver_worker_sha256",
        "control_plane_release_id",
        "control_plane_source_commit_sha",
        "deployment_evidence_sha256",
    }
    if set(evidence) != expected_fields:
        fail("accepted staging evidence field inventory mismatch")
    if evidence["schema_version"] != 1 or evidence["status"] != "accepted" or evidence["environment"] != "staging":
        fail("production requires accepted staging evidence")
    resolver = require_object(load_json(resolver_manifest_path, "resolver release manifest"), "resolver manifest")
    control = require_object(load_json(control_manifest_path, "control-plane release manifest"), "control manifest")
    source = require_object(control.get("source"), "control-plane release source")
    exact = {
        "resolver_release_id": resolver.get("release_id"),
        "resolver_source_commit_sha": resolver.get("source_commit_sha"),
        "resolver_worker_sha256": resolver.get("resolver_worker_sha256"),
        "control_plane_release_id": control.get("release_id"),
        "control_plane_source_commit_sha": source.get("commit_sha"),
    }
    if any(evidence[name] != value for name, value in exact.items()):
        fail("production artifacts differ from accepted staging evidence")
    if COMMIT_RE.fullmatch(str(exact["resolver_source_commit_sha"])) is None or SHA256_RE.fullmatch(
        str(evidence["deployment_evidence_sha256"])
    ) is None:
        fail("accepted staging evidence digest/source shape is invalid")
    print("Production same-bits artifacts match accepted staging evidence.")


def fixture_deploy_manifest(environment: str) -> dict[str, Any]:
    digit = "1" if environment == "staging" else "2"
    resolver_name = f"mailbox-secret-resolver-{environment}"
    return {
        "schema_version": 1,
        "environment": environment,
        "control_plane": {
            "worker_name": f"profile-control-{environment}",
            "account_id": digit * 32,
            "custom_domain": f"{environment}.crm.invalid",
            "access_issuer": f"https://{environment}.cloudflareaccess.invalid",
            "access_audience": ("a" if environment == "staging" else "b") * 32,
            "d1_database_name": f"catalog-{environment}",
            "d1_database_id": f"{digit * 8}-{digit * 4}-{digit * 4}-{digit * 4}-{digit * 12}",
            "r2_bucket_name": f"profiles-{environment}",
            "generation_verification_queue": f"generation-verification-{environment}",
            "integration_events_queue": f"integration-events-{environment}",
            "mailbox_jobs_queue": f"mailbox-jobs-{environment}",
            "mailbox_jobs_dlq": f"mailbox-jobs-dlq-{environment}",
            "mailbox_secret_resolver_service": resolver_name,
        },
        "resolver": {
            "worker_name": resolver_name,
            "account_id": digit * 32,
            "d1_database_name": f"resolver-{environment}",
            "d1_database_id": f"{digit * 8}-{digit * 4}-{digit * 4}-{digit * 4}-{'3' * 12}",
            "google_oauth_client_id": f"google-client-{environment}",
            "google_oauth_redirect_uri": f"https://{environment}.crm.invalid/oauth/google/callback",
            "microsoft_oauth_client_id": f"microsoft-client-{environment}",
            "microsoft_oauth_redirect_uri": f"https://{environment}.crm.invalid/oauth/microsoft/callback",
        },
    }


def fixture_secrets(seed: str) -> tuple[dict[str, str], dict[str, str]]:
    caller = (seed + "-caller-auth-") * 3
    resolver = {
        "GOOGLE_OAUTH_CLIENT_SECRET": seed + "-google-oauth-secret",
        "MAILBOX_RESOLVER_CALLER_AUTH_KEY": caller,
        "MAILBOX_RESOLVER_ENCRYPTION_KEYRING": json.dumps(
            {"activeVersion": 1, "keys": [{"version": 1, "keyHex": seed[0] * 64}]}
        ),
        "MAILBOX_RESOLVER_HANDLE_HMAC_KEY": (seed + "-handle-hmac-") * 3,
        "MICROSOFT_OAUTH_CLIENT_SECRET": seed + "-microsoft-oauth-secret",
    }
    control = {
        "CLIENT_CONTACT_PROTECTION_KEYRING": seed + "-contact-keyring-material",
        "MAILBOX_RESOLVER_CALLER_AUTH_KEY": caller,
        "R2_GENERATION_ACCESS_KEY_ID": seed + "-r2-access-key",
        "R2_GENERATION_SECRET_ACCESS_KEY": seed + "-r2-secret-key",
    }
    return resolver, control


def write_0600(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value), encoding="utf-8")
    path.chmod(0o600)


def expect_rejected(label: str, operation: Any) -> None:
    try:
        operation()
    except PromotionError:
        return
    fail(f"negative promotion fixture unexpectedly passed: {label}")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="resolver-promotion-self-test-") as temporary:
        root = Path(temporary)
        resolver, control = fixture_secrets("a")
        peer_resolver, peer_control = fixture_secrets("b")
        paths = [root / name for name in ("resolver.json", "control.json", "peer-r.json", "peer-c.json")]
        for path, value in zip(paths, (resolver, control, peer_resolver, peer_control), strict=True):
            write_0600(path, value)
        validate_secret_documents(paths[0], paths[1], peer_resolver_path=paths[2], peer_control_path=paths[3])
        missing = dict(resolver)
        missing.pop("GOOGLE_OAUTH_CLIENT_SECRET")
        write_0600(paths[0], missing)
        expect_rejected("missing resolver secret", lambda: validate_secret_documents(paths[0], paths[1]))
        write_0600(paths[0], resolver)
        write_0600(paths[2], resolver)
        write_0600(paths[3], control)
        expect_rejected(
            "cross-environment identical documents",
            lambda: validate_secret_documents(
                paths[0], paths[1], peer_resolver_path=paths[2], peer_control_path=paths[3]
            ),
        )
        paths[0].chmod(0o644)
        expect_rejected("non-0600 secret file", lambda: validate_secret_documents(paths[0], paths[1]))
        manifest = fixture_deploy_manifest("staging")
        validate_control_manifest(manifest["control_plane"], "staging")
        validate_resolver_manifest(manifest["resolver"], "staging", manifest["control_plane"])
        manifest["resolver"]["d1_database_id"] = manifest["control_plane"]["d1_database_id"]
        expect_rejected(
            "business D1 reuse",
            lambda: validate_resolver_manifest(
                manifest["resolver"], "staging", manifest["control_plane"]
            ),
        )
    print("Mailbox resolver promotion positive and negative self-tests passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("self-test")
    secrets = commands.add_parser("validate-secrets")
    secrets.add_argument("--resolver", type=Path, required=True)
    secrets.add_argument("--control-plane", type=Path, required=True)
    secrets.add_argument("--peer-resolver", type=Path)
    secrets.add_argument("--peer-control-plane", type=Path)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("--environment", choices=ENVIRONMENTS, required=True)
    prepare_parser.add_argument("--deploy-manifest", type=Path, required=True)
    prepare_parser.add_argument("--resolver-release", type=Path, required=True)
    prepare_parser.add_argument("--control-plane-release", type=Path, required=True)
    prepare_parser.add_argument("--resolver-output", type=Path, required=True)
    prepare_parser.add_argument("--control-plane-output", type=Path, required=True)
    evidence = commands.add_parser("validate-staging-evidence")
    evidence.add_argument("--evidence", type=Path, required=True)
    evidence.add_argument("--resolver-manifest", type=Path, required=True)
    evidence.add_argument("--control-plane-manifest", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "self-test":
        self_test()
    elif args.command == "validate-secrets":
        validate_secret_documents(
            args.resolver,
            args.control_plane,
            peer_resolver_path=args.peer_resolver,
            peer_control_path=args.peer_control_plane,
        )
        print("Validated exact resolver/control-plane secret-name inventories.")
    elif args.command == "prepare":
        prepare(
            args.environment,
            args.deploy_manifest,
            args.resolver_release,
            args.control_plane_release,
            args.resolver_output,
            args.control_plane_output,
        )
    elif args.command == "validate-staging-evidence":
        validate_staging_evidence(args.evidence, args.resolver_manifest, args.control_plane_manifest)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except PromotionError as error:
        raise SystemExit(f"mailbox resolver promotion rejected: {error}") from error
