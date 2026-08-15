#!/usr/bin/env python3
"""Validate resolver promotion inputs and render no-rebuild Wrangler configs."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import stat
import tempfile
import urllib.error
import urllib.request
from pathlib import Path
from pathlib import PurePosixPath
from typing import Any
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
RESOLVER_CONFIG = ROOT / "deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc"
CONTROL_CONFIG = ROOT / "deploy/cloudflare/wrangler.jsonc"
ENVIRONMENTS = ("staging", "production")
CANONICAL_REPOSITORY = "iamaman11/part-crm-emai-profile"
RESOLVER_RELEASE_WORKFLOW = ".github/workflows/mailbox-secret-resolver-release.yml"
CONTROL_RELEASE_WORKFLOW = ".github/workflows/quality-gate.yml"
PROMOTION_WORKFLOW = ".github/workflows/mailbox-secret-resolver-promotion.yml"
RESOLVER_RELEASE_NAME = "Mailbox Secret Resolver Release"
CONTROL_RELEASE_NAME = "Quality Gate"
PROMOTION_NAME = "Mailbox Secret Resolver Promotion"
RESOLVER_ARTIFACT_RE = re.compile(
    r"^mailbox-secret-resolver-v1-sha256-[0-9a-f]{64}\.tar$"
)
CONTROL_ARTIFACT_RE = re.compile(r"^cloudflare-v1-sha256-[0-9a-f]{64}\.tar$")
STAGING_EVIDENCE_ARTIFACT_RE = re.compile(
    r"^mailbox-secret-resolver-promotion-staging-[0-9a-f]{40}$"
)
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
ARTIFACT_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
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


def canonical_document(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"expected regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path, label: str, *, maximum_bytes: int = 64 * 1024) -> Any:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file")
    if path.stat().st_size == 0 or path.stat().st_size > maximum_bytes:
        fail(f"{label} has an invalid bounded size")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromotionError(f"{label} is not strict UTF-8 JSON") from error


def parse_positive_int(value: str, label: str) -> int:
    if not value.isdigit() or int(value) <= 0:
        fail(f"{label} must be a positive integer")
    return int(value)


def api_get(api_url: str, token: str, path: str) -> Any:
    if not api_url.startswith("https://"):
        fail("GitHub API URL must use HTTPS")
    request = urllib.request.Request(
        api_url.rstrip("/") + path,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "part-crm-d3-promotion",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.loads(response.read().decode("utf-8"))
    except (urllib.error.URLError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromotionError(f"cannot read GitHub promotion authority {path}") from error


def validate_workflow_run(
    value: Any,
    *,
    run_id: int,
    source_sha: str,
    name: str,
    path: str,
) -> None:
    run = require_object(value, f"{name} workflow run")
    expected = {
        "id": run_id,
        "name": name,
        "path": path,
        "event": "push",
        "head_branch": "main",
        "head_sha": source_sha,
        "status": "completed",
        "conclusion": "success",
    }
    mismatches = {
        field: {"actual": run.get(field), "expected": expected_value}
        for field, expected_value in expected.items()
        if run.get(field) != expected_value
    }
    if mismatches:
        fail(f"{name} workflow run is not exact accepted-main evidence: {mismatches}")


def validate_staging_promotion_run(
    value: Any, *, run_id: int, run_attempt: int, source_sha: str
) -> None:
    run = require_object(value, "staging promotion workflow run")
    expected = {
        "id": run_id,
        "name": PROMOTION_NAME,
        "path": PROMOTION_WORKFLOW,
        "event": "workflow_dispatch",
        "head_branch": "main",
        "head_sha": source_sha,
        "status": "completed",
        "conclusion": "success",
        "run_attempt": run_attempt,
    }
    mismatches = {
        field: {"actual": run.get(field), "expected": expected_value}
        for field, expected_value in expected.items()
        if run.get(field) != expected_value
    }
    if mismatches:
        fail(f"staging promotion run is not exact successful staging authority: {mismatches}")


def validate_artifact(
    value: Any,
    *,
    artifact_id: int,
    artifact_digest: str,
    source_sha: str,
    run_id: int,
    name_pattern: re.Pattern[str],
    label: str,
) -> None:
    inventory = require_object(value, f"{label} artifact inventory")
    artifacts = inventory.get("artifacts")
    if not isinstance(artifacts, list):
        fail(f"{label} artifact inventory is missing")
    if len(artifacts) != 1:
        fail(f"{label} release run must own exactly one immutable artifact")
    matching = [item for item in artifacts if isinstance(item, dict) and item.get("id") == artifact_id]
    if len(matching) != 1:
        fail(f"{label} artifact id is not uniquely owned by the supplied workflow run")
    artifact = matching[0]
    name = artifact.get("name")
    if not isinstance(name, str) or name_pattern.fullmatch(name) is None:
        fail(f"{label} artifact name is not canonical")
    if ARTIFACT_DIGEST_RE.fullmatch(artifact_digest) is None:
        fail(f"{label} artifact digest input is invalid")
    if artifact.get("digest") != artifact_digest or artifact.get("expired") is not False:
        fail(f"{label} artifact digest/retention authority does not match")
    workflow_run = artifact.get("workflow_run")
    if isinstance(workflow_run, dict):
        if workflow_run.get("id") != run_id or workflow_run.get("head_sha") != source_sha:
            fail(f"{label} artifact workflow ownership drifted")


def validate_staging_evidence_artifact(
    value: Any,
    *,
    artifact_id: int,
    artifact_digest: str,
    source_sha: str,
    run_id: int,
) -> None:
    inventory = require_object(value, "staging evidence artifact inventory")
    artifacts = inventory.get("artifacts")
    if not isinstance(artifacts, list) or len(artifacts) != 1:
        fail("staging promotion run must own exactly one immutable evidence artifact")
    matching = [item for item in artifacts if isinstance(item, dict) and item.get("id") == artifact_id]
    if len(matching) != 1:
        fail("staging evidence artifact id is not uniquely owned by the supplied staging run")
    artifact = matching[0]
    expected_name = f"mailbox-secret-resolver-promotion-staging-{source_sha}"
    if artifact.get("name") != expected_name or STAGING_EVIDENCE_ARTIFACT_RE.fullmatch(
        str(artifact.get("name"))
    ) is None:
        fail("staging evidence artifact name is not canonical for the exact source")
    if ARTIFACT_DIGEST_RE.fullmatch(artifact_digest) is None:
        fail("staging evidence artifact digest input is invalid")
    if artifact.get("digest") != artifact_digest or artifact.get("expired") is not False:
        fail("staging evidence artifact digest/retention authority does not match")
    workflow_run = artifact.get("workflow_run")
    if not isinstance(workflow_run, dict) or workflow_run.get("id") != run_id or workflow_run.get(
        "head_sha"
    ) != source_sha:
        fail("staging evidence artifact workflow ownership drifted")


def validate_environment(
    environment: Any,
    branch_policies: Any,
    *,
    expected_name: str,
) -> None:
    value = require_object(environment, f"{expected_name} environment")
    if value.get("name") != expected_name:
        fail(f"real GitHub {expected_name} environment is missing")
    policy = value.get("deployment_branch_policy")
    if policy != {"protected_branches": False, "custom_branch_policies": True}:
        fail(f"{expected_name} environment must use an exact custom deployment-branch policy")
    policies = require_object(branch_policies, f"{expected_name} branch policies").get(
        "branch_policies"
    )
    if not isinstance(policies, list):
        fail(f"{expected_name} environment branch-policy inventory is missing")
    branches = sorted(
        item.get("name")
        for item in policies
        if isinstance(item, dict)
        and item.get("type") == "branch"
        and isinstance(item.get("name"), str)
    )
    if branches != ["main"]:
        fail(f"{expected_name} environment must authorize only the exact main branch")
    if expected_name == "production":
        if value.get("can_admins_bypass") is not False:
            fail("production environment must disable administrator protection bypass")
        rules = value.get("protection_rules")
        if not isinstance(rules, list):
            fail("production environment protection rules are missing")
        reviewer_rules = [
            item for item in rules if isinstance(item, dict) and item.get("type") == "required_reviewers"
        ]
        reviewers = reviewer_rules[0].get("reviewers") if len(reviewer_rules) == 1 else None
        if not isinstance(reviewers, list) or not reviewers:
            fail("production environment must have one non-empty required-reviewers rule")


def github_preflight(args: argparse.Namespace) -> None:
    if args.repository != CANONICAL_REPOSITORY:
        fail(f"promotion repository must be {CANONICAL_REPOSITORY}")
    if COMMIT_RE.fullmatch(args.source_sha) is None:
        fail("source SHA must be exact lowercase 40-hex")
    if args.workflow_ref != "refs/heads/main" or args.workflow_sha != args.source_sha:
        fail("promotion workflow must execute from the exact accepted-main source")
    if args.confirmation != args.source_sha:
        fail("promotion confirmation must exactly equal the accepted-main source SHA")
    resolver_run_id = parse_positive_int(args.resolver_release_run_id, "resolver release run id")
    control_run_id = parse_positive_int(args.control_plane_release_run_id, "control-plane release run id")
    resolver_artifact_id = parse_positive_int(args.resolver_artifact_id, "resolver artifact id")
    control_artifact_id = parse_positive_int(args.control_plane_artifact_id, "control-plane artifact id")
    if (
        not isinstance(args.resolver_release_id, str)
        or re.fullmatch(r"mailbox-secret-resolver-v1-sha256-[0-9a-f]{64}", args.resolver_release_id)
        is None
        or SHA256_RE.fullmatch(args.resolver_worker_sha256) is None
        or not isinstance(args.control_plane_release_id, str)
        or re.fullmatch(r"cloudflare-v1-sha256-[0-9a-f]{64}", args.control_plane_release_id)
        is None
    ):
        fail("release identity inputs are malformed")
    staging_inputs = (
        args.staging_promotion_run_id,
        args.staging_evidence_artifact_id,
        args.staging_evidence_artifact_digest,
        args.staging_run_attempt,
        args.staging_evidence_confirmation,
    )
    if args.environment == "production":
        if not all(staging_inputs):
            fail("production requires the exact immutable staging evidence identity")
        if args.staging_evidence_confirmation != f"{args.source_sha}:{args.staging_evidence_artifact_digest}":
            fail("production staging-evidence confirmation must exactly bind source SHA and artifact digest")
        staging_run_id = parse_positive_int(args.staging_promotion_run_id, "staging promotion run id")
        staging_artifact_id = parse_positive_int(
            args.staging_evidence_artifact_id, "staging evidence artifact id"
        )
        staging_run_attempt = parse_positive_int(args.staging_run_attempt, "staging run attempt")
    elif any(staging_inputs):
        fail("staging promotion must not accept production-only staging evidence inputs")
    if not args.token:
        fail("GitHub token is required for promotion preflight")

    repository_path = f"/repos/{args.repository}"
    main_ref = api_get(args.api_url, args.token, repository_path + "/git/ref/heads/main")
    if not isinstance(main_ref, dict) or main_ref.get("object", {}).get("sha") != args.source_sha:
        fail("promotion source is no longer exact current main")
    resolver_run = api_get(
        args.api_url, args.token, repository_path + f"/actions/runs/{resolver_run_id}"
    )
    control_run = api_get(
        args.api_url, args.token, repository_path + f"/actions/runs/{control_run_id}"
    )
    validate_workflow_run(
        resolver_run,
        run_id=resolver_run_id,
        source_sha=args.source_sha,
        name=RESOLVER_RELEASE_NAME,
        path=RESOLVER_RELEASE_WORKFLOW,
    )
    validate_workflow_run(
        control_run,
        run_id=control_run_id,
        source_sha=args.source_sha,
        name=CONTROL_RELEASE_NAME,
        path=CONTROL_RELEASE_WORKFLOW,
    )
    resolver_artifacts = api_get(
        args.api_url,
        args.token,
        repository_path + f"/actions/runs/{resolver_run_id}/artifacts?per_page=100",
    )
    control_artifacts = api_get(
        args.api_url,
        args.token,
        repository_path + f"/actions/runs/{control_run_id}/artifacts?per_page=100",
    )
    validate_artifact(
        resolver_artifacts,
        artifact_id=resolver_artifact_id,
        artifact_digest=args.resolver_artifact_digest,
        source_sha=args.source_sha,
        run_id=resolver_run_id,
        name_pattern=RESOLVER_ARTIFACT_RE,
        label="resolver",
    )
    validate_artifact(
        control_artifacts,
        artifact_id=control_artifact_id,
        artifact_digest=args.control_plane_artifact_digest,
        source_sha=args.source_sha,
        run_id=control_run_id,
        name_pattern=CONTROL_ARTIFACT_RE,
        label="control-plane",
    )
    if args.environment == "production":
        staging_run = api_get(
            args.api_url, args.token, repository_path + f"/actions/runs/{staging_run_id}"
        )
        validate_staging_promotion_run(
            staging_run,
            run_id=staging_run_id,
            run_attempt=staging_run_attempt,
            source_sha=args.source_sha,
        )
        staging_artifacts = api_get(
            args.api_url,
            args.token,
            repository_path + f"/actions/runs/{staging_run_id}/artifacts?per_page=100",
        )
        validate_staging_evidence_artifact(
            staging_artifacts,
            artifact_id=staging_artifact_id,
            artifact_digest=args.staging_evidence_artifact_digest,
            source_sha=args.source_sha,
            run_id=staging_run_id,
        )
    environment = api_get(
        args.api_url, args.token, repository_path + f"/environments/{args.environment}"
    )
    branch_policies = api_get(
        args.api_url,
        args.token,
        repository_path
        + f"/environments/{args.environment}/deployment-branch-policies?per_page=100",
    )
    validate_environment(environment, branch_policies, expected_name=args.environment)
    print(
        f"D3 preflight accepted exact resolver/control-plane artifacts from {args.source_sha} "
        f"for protected {args.environment}."
    )


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
    config = require_object(
        load_json(release_directory / RESOLVER_CONFIG, "immutable resolver deployment config"),
        "resolver config",
    )
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
    config = require_object(
        load_json(release_directory / CONTROL_CONFIG, "immutable control-plane deployment config"),
        "control config",
    )
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


def require_deployment_closure(release_directory: Path, label: str, paths: tuple[Path, ...]) -> None:
    for relative in paths:
        path = release_directory / relative
        if path.is_symlink() or not path.is_file():
            fail(f"{label} immutable deployment closure is missing {relative.as_posix()}")


def validate_deployment_closures(resolver_release: Path, control_release: Path) -> None:
    require_deployment_closure(
        resolver_release,
        "resolver",
        (
            Path("worker/index.js"),
            Path("worker/index_bg.wasm"),
            Path("worker/worker/shim.mjs"),
            RESOLVER_CONFIG.relative_to(ROOT),
        ),
    )
    migrations = resolver_release / "migrations" / "resolver-d1"
    if migrations.is_symlink() or not migrations.is_dir() or not any(migrations.glob("*.sql")):
        fail("resolver immutable deployment closure lacks migrations")
    require_deployment_closure(
        control_release,
        "control-plane",
        (
            Path("worker/index.js"),
            Path("worker/index_bg.wasm"),
            Path("worker/worker/shim.mjs"),
            Path("frontend/index.html"),
            CONTROL_CONFIG.relative_to(ROOT),
        ),
    )
    assets = control_release / "frontend" / "assets"
    migrations = control_release / "migrations" / "d1"
    if assets.is_symlink() or not assets.is_dir() or not any(path.is_file() for path in assets.rglob("*")):
        fail("control-plane immutable deployment closure lacks static assets")
    if migrations.is_symlink() or not migrations.is_dir() or not any(migrations.glob("*.sql")):
        fail("control-plane immutable deployment closure lacks migrations")


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
    validate_deployment_closures(resolver_release, control_release)
    control, resolver = validate_deploy_manifest(deploy_manifest, environment)
    resolver_output.parent.mkdir(parents=True, exist_ok=True)
    control_output.parent.mkdir(parents=True, exist_ok=True)
    render_resolver_config(environment, resolver, resolver_release, resolver_output)
    render_control_config(environment, control, control_release, control_output)
    print(f"Prepared no-rebuild {environment} resolver and control-plane deployment configs.")


def validate_release_identities(
    *,
    source_sha: str,
    resolver_manifest_path: Path,
    control_manifest_path: Path,
    resolver_release_id: str,
    resolver_worker_sha256: str,
    control_plane_release_id: str,
) -> None:
    if COMMIT_RE.fullmatch(source_sha) is None or SHA256_RE.fullmatch(resolver_worker_sha256) is None:
        fail("expected release identity is malformed")
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
    expected = {
        "resolver_release_id": resolver_release_id,
        "resolver_source_commit_sha": source_sha,
        "resolver_worker_sha256": resolver_worker_sha256,
        "control_plane_release_id": control_plane_release_id,
        "control_plane_source_commit_sha": source_sha,
    }
    if exact != expected or source.get("authority") != "accepted-main" or source.get(
        "repository"
    ) != CANONICAL_REPOSITORY:
        fail("immutable release manifests differ from the exact requested release identities")


def validate_staging_evidence(
    *,
    evidence_path: Path,
    source_sha: str,
    resolver_release_id: str,
    resolver_worker_sha256: str,
    control_plane_release_id: str,
    staging_promotion_run_id: str,
    staging_run_attempt: str,
) -> None:
    evidence = require_object(load_json(evidence_path, "staging evidence"), "staging evidence")
    expected_fields = {
        "schema_version",
        "status",
        "environment",
        "source_commit_sha",
        "resolver",
        "control_plane",
        "smoke",
        "github",
    }
    if set(evidence) != expected_fields:
        fail("staging evidence field inventory mismatch")
    if (
        evidence["schema_version"] != 1
        or evidence["status"] != "passed"
        or evidence["environment"] != "staging"
        or evidence["source_commit_sha"] != source_sha
    ):
        fail("production requires exact passed staging evidence")
    resolver = require_object(evidence["resolver"], "staging resolver attestation")
    control = require_object(evidence["control_plane"], "staging control-plane attestation")
    smoke = require_object(evidence["smoke"], "staging smoke attestation")
    github = require_object(evidence["github"], "staging GitHub attestation")
    if set(resolver) != {"release_id", "worker_sha256", "deployment_status_sha256"}:
        fail("staging resolver attestation field inventory mismatch")
    if set(control) != {"release_id", "deployment_status_sha256"}:
        fail("staging control-plane attestation field inventory mismatch")
    if set(smoke) != {"response_sha256", "response_size"}:
        fail("staging smoke attestation field inventory mismatch")
    if set(github) != {"run_id", "run_attempt"}:
        fail("staging GitHub attestation field inventory mismatch")
    exact = {
        "resolver_release_id": resolver.get("release_id"),
        "resolver_worker_sha256": resolver.get("worker_sha256"),
        "control_plane_release_id": control.get("release_id"),
    }
    expected = {
        "resolver_release_id": resolver_release_id,
        "resolver_worker_sha256": resolver_worker_sha256,
        "control_plane_release_id": control_plane_release_id,
    }
    hashes = (
        resolver.get("deployment_status_sha256"),
        control.get("deployment_status_sha256"),
        smoke.get("response_sha256"),
    )
    if (
        exact != expected
        or any(SHA256_RE.fullmatch(str(value)) is None for value in hashes)
        or not isinstance(smoke.get("response_size"), int)
        or smoke["response_size"] <= 0
        or github.get("run_id") != parse_positive_int(staging_promotion_run_id, "staging promotion run id")
        or github.get("run_attempt") != parse_positive_int(staging_run_attempt, "staging run attempt")
    ):
        fail("production artifacts differ from exact immutable staging evidence")
    print("Production same-bits artifacts match immutable passed staging evidence.")


def parse_remote_d1_names(value: Any, label: str) -> list[str]:
    if not isinstance(value, list) or len(value) != 1 or not isinstance(value[0], dict):
        fail(f"{label} must be one Wrangler JSON result object")
    results = value[0].get("results")
    if not isinstance(results, list):
        fail(f"{label} lacks a results array")
    names: list[str] = []
    for row in results:
        if not isinstance(row, dict) or set(row) != {"name"} or not isinstance(row["name"], str):
            fail(f"{label} returned an unexpected row shape")
        names.append(row["name"])
    return names


def expected_resolver_migrations(release_directory: Path) -> list[str]:
    directory = release_directory / "migrations/resolver-d1"
    if directory.is_symlink() or not directory.is_dir():
        fail("resolver release migration directory is missing")
    names = sorted(
        path.name
        for path in directory.glob("*.sql")
        if path.is_file() and not path.is_symlink()
    )
    if not names or any(
        re.fullmatch(r"[0-9]{4}_[a-z0-9_]+\.sql", name) is None for name in names
    ):
        fail("resolver release migration inventory is invalid")
    return names


def expected_control_migrations(release_directory: Path) -> list[str]:
    manifest = require_object(
        load_json(release_directory / "release-manifest.json", "control-plane release manifest"),
        "control-plane release manifest",
    )
    migrations = require_object(manifest.get("migrations"), "control-plane migration inventory")
    files = migrations.get("files")
    if not isinstance(files, list) or not files:
        fail("control-plane release migration inventory is empty")
    names: list[str] = []
    for item in files:
        if not isinstance(item, dict) or not isinstance(item.get("path"), str):
            fail("control-plane release migration row is invalid")
        path = PurePosixPath(item["path"])
        if (
            len(path.parts) != 3
            or path.parts[:2] != ("migrations", "d1")
            or re.fullmatch(r"[0-9]{4}_[a-z0-9_]+\.sql", path.name) is None
        ):
            fail("control-plane release migration path escaped migrations/d1")
        names.append(path.name)
    if names != sorted(set(names)):
        fail("control-plane release migration inventory is not unique and ordered")
    return names


def verify_remote_d1(
    resolver_release: Path,
    control_release: Path,
    resolver_query: Path,
    control_query: Path,
) -> None:
    expected_resolver = expected_resolver_migrations(resolver_release)
    expected_control = expected_control_migrations(control_release)
    actual_resolver = parse_remote_d1_names(
        load_json(resolver_query, "remote resolver D1 query", maximum_bytes=256 * 1024),
        "remote resolver D1 query",
    )
    actual_control = parse_remote_d1_names(
        load_json(control_query, "remote catalog D1 query", maximum_bytes=256 * 1024),
        "remote catalog D1 query",
    )
    if actual_resolver != expected_resolver:
        fail(
            "remote resolver D1 migration set differs from the exact release: "
            f"expected={expected_resolver}, actual={actual_resolver}"
        )
    if actual_control != expected_control:
        fail(
            "remote catalog D1 migration set differs from the exact release: "
            f"expected={expected_control}, actual={actual_control}"
        )
    print(
        "Remote resolver/catalog D1 ledgers exactly match the immutable releases; "
        "D3 performed no schema mutation."
    )


def write_github_output(values: dict[str, str]) -> None:
    output = os.environ.get("GITHUB_OUTPUT")
    if not output:
        return
    with Path(output).open("a", encoding="utf-8") as handle:
        for name, value in values.items():
            handle.write(f"{name}={value}\n")


def attest(
    *,
    environment: str,
    resolver_manifest_path: Path,
    control_manifest_path: Path,
    resolver_status_path: Path,
    control_status_path: Path,
    smoke_body_path: Path,
    workflow_run_id: str,
    workflow_run_attempt: str,
    evidence_output: Path,
) -> None:
    if environment not in ENVIRONMENTS:
        fail("promotion evidence environment is invalid")
    run_id = parse_positive_int(workflow_run_id, "workflow run id")
    run_attempt = parse_positive_int(workflow_run_attempt, "workflow run attempt")
    resolver = require_object(
        load_json(resolver_manifest_path, "resolver release manifest"), "resolver release manifest"
    )
    control = require_object(
        load_json(control_manifest_path, "control-plane release manifest"),
        "control-plane release manifest",
    )
    control_source = require_object(control.get("source"), "control-plane release source")
    resolver_source = resolver.get("source_commit_sha")
    control_source_sha = control_source.get("commit_sha")
    if (
        COMMIT_RE.fullmatch(str(resolver_source)) is None
        or resolver_source != control_source_sha
        or control_source.get("authority") != "accepted-main"
        or control_source.get("repository") != CANONICAL_REPOSITORY
        or re.fullmatch(
            r"mailbox-secret-resolver-v1-sha256-[0-9a-f]{64}",
            str(resolver.get("release_id")),
        )
        is None
        or SHA256_RE.fullmatch(str(resolver.get("resolver_worker_sha256"))) is None
        or re.fullmatch(
            r"cloudflare-v1-sha256-[0-9a-f]{64}", str(control.get("release_id"))
        )
        is None
    ):
        fail("promotion evidence requires same-source accepted-main releases")
    resolver_status = load_json(
        resolver_status_path, "resolver deployment status", maximum_bytes=256 * 1024
    )
    control_status = load_json(
        control_status_path, "control-plane deployment status", maximum_bytes=256 * 1024
    )
    if not isinstance(resolver_status, (dict, list)) or not isinstance(control_status, (dict, list)):
        fail("deployment status must be one JSON object or array")
    if (
        smoke_body_path.is_symlink()
        or not smoke_body_path.is_file()
        or smoke_body_path.stat().st_size == 0
        or smoke_body_path.stat().st_size > 1024 * 1024
    ):
        fail("smoke response body must be one bounded non-empty regular file")
    evidence = {
        "schema_version": 1,
        "status": "passed",
        "environment": environment,
        "source_commit_sha": resolver_source,
        "resolver": {
            "release_id": resolver.get("release_id"),
            "worker_sha256": resolver.get("resolver_worker_sha256"),
            "deployment_status_sha256": hashlib.sha256(
                json.dumps(resolver_status, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        },
        "control_plane": {
            "release_id": control.get("release_id"),
            "deployment_status_sha256": hashlib.sha256(
                json.dumps(control_status, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        },
        "smoke": {
            "response_sha256": sha256_file(smoke_body_path),
            "response_size": smoke_body_path.stat().st_size,
        },
        "github": {"run_id": run_id, "run_attempt": run_attempt},
    }
    evidence_bytes = canonical_document(evidence)
    evidence_output.parent.mkdir(parents=True, exist_ok=True)
    evidence_output.write_bytes(evidence_bytes)
    write_github_output({"evidence": str(evidence_output)})
    print(f"Recorded immutable metadata-only {environment} D3 promotion evidence.")


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
        resolver_closure = root / "resolver-closure"
        control_closure = root / "control-closure"
        for release, config, migration in (
            (resolver_closure, RESOLVER_CONFIG.relative_to(ROOT), Path("migrations/resolver-d1/0001_fixture.sql")),
            (control_closure, CONTROL_CONFIG.relative_to(ROOT), Path("migrations/d1/0001_fixture.sql")),
        ):
            for relative in (Path("worker/index.js"), Path("worker/index_bg.wasm"), Path("worker/worker/shim.mjs"), config, migration):
                path = release / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture\n", encoding="utf-8")
        for relative in (Path("frontend/index.html"), Path("frontend/assets/app.js")):
            path = control_closure / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture\n", encoding="utf-8")
        validate_deployment_closures(resolver_closure, control_closure)
        (control_closure / "frontend/assets/app.js").unlink()
        expect_rejected(
            "control-plane static assets missing from immutable closure",
            lambda: validate_deployment_closures(resolver_closure, control_closure),
        )
        (control_closure / "frontend/assets/app.js").write_text("fixture\n", encoding="utf-8")
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
        source_sha = "a" * 40
        run = {
            "id": 17,
            "name": RESOLVER_RELEASE_NAME,
            "path": RESOLVER_RELEASE_WORKFLOW,
            "event": "push",
            "head_branch": "main",
            "head_sha": source_sha,
            "status": "completed",
            "conclusion": "success",
        }
        validate_workflow_run(
            run,
            run_id=17,
            source_sha=source_sha,
            name=RESOLVER_RELEASE_NAME,
            path=RESOLVER_RELEASE_WORKFLOW,
        )
        failed_run = dict(run)
        failed_run["conclusion"] = "failure"
        expect_rejected(
            "failed accepted-main release",
            lambda: validate_workflow_run(
                failed_run,
                run_id=17,
                source_sha=source_sha,
                name=RESOLVER_RELEASE_NAME,
                path=RESOLVER_RELEASE_WORKFLOW,
            ),
        )
        artifact_digest = "sha256:" + "b" * 64
        artifacts = {
            "artifacts": [
                {
                    "id": 19,
                    "name": "mailbox-secret-resolver-v1-sha256-" + "c" * 64 + ".tar",
                    "digest": artifact_digest,
                    "expired": False,
                    "workflow_run": {"id": 17, "head_sha": source_sha},
                }
            ]
        }
        validate_artifact(
            artifacts,
            artifact_id=19,
            artifact_digest=artifact_digest,
            source_sha=source_sha,
            run_id=17,
            name_pattern=RESOLVER_ARTIFACT_RE,
            label="resolver",
        )
        staging_run = {
            "id": 23,
            "name": PROMOTION_NAME,
            "path": PROMOTION_WORKFLOW,
            "event": "workflow_dispatch",
            "head_branch": "main",
            "head_sha": source_sha,
            "status": "completed",
            "conclusion": "success",
            "run_attempt": 1,
        }
        validate_staging_promotion_run(
            staging_run, run_id=23, run_attempt=1, source_sha=source_sha
        )
        failed_staging_run = dict(staging_run)
        failed_staging_run["conclusion"] = "failure"
        expect_rejected(
            "failed staging promotion",
            lambda: validate_staging_promotion_run(
                failed_staging_run, run_id=23, run_attempt=1, source_sha=source_sha
            ),
        )
        staging_artifact_digest = "sha256:" + "e" * 64
        staging_artifacts = {
            "artifacts": [
                {
                    "id": 29,
                    "name": f"mailbox-secret-resolver-promotion-staging-{source_sha}",
                    "digest": staging_artifact_digest,
                    "expired": False,
                    "workflow_run": {"id": 23, "head_sha": source_sha},
                }
            ]
        }
        validate_staging_evidence_artifact(
            staging_artifacts,
            artifact_id=29,
            artifact_digest=staging_artifact_digest,
            source_sha=source_sha,
            run_id=23,
        )
        expired_evidence = copy.deepcopy(staging_artifacts)
        expired_evidence["artifacts"][0]["expired"] = True
        expect_rejected(
            "expired staging evidence artifact",
            lambda: validate_staging_evidence_artifact(
                expired_evidence,
                artifact_id=29,
                artifact_digest=staging_artifact_digest,
                source_sha=source_sha,
                run_id=23,
            ),
        )
        substituted = copy.deepcopy(artifacts)
        substituted["artifacts"][0]["digest"] = "sha256:" + "d" * 64
        expect_rejected(
            "artifact substitution",
            lambda: validate_artifact(
                substituted,
                artifact_id=19,
                artifact_digest=artifact_digest,
                source_sha=source_sha,
                run_id=17,
                name_pattern=RESOLVER_ARTIFACT_RE,
                label="resolver",
            ),
        )
        production = {
            "name": "production",
            "deployment_branch_policy": {
                "protected_branches": False,
                "custom_branch_policies": True,
            },
            "can_admins_bypass": False,
            "protection_rules": [
                {"type": "required_reviewers", "reviewers": [{"type": "User"}]}
            ],
        }
        policies = {"branch_policies": [{"type": "branch", "name": "main"}]}
        validate_environment(production, policies, expected_name="production")
        expect_rejected(
            "wildcard deployment branch",
            lambda: validate_environment(
                production,
                {"branch_policies": [{"type": "branch", "name": "*"}]},
                expected_name="production",
            ),
        )

        resolver_release = root / "resolver-release"
        control_release = root / "control-release"
        (resolver_release / "migrations/resolver-d1").mkdir(parents=True)
        (resolver_release / "migrations/resolver-d1/0001_resolver.sql").write_text(
            "SELECT 1;\n", encoding="utf-8"
        )
        control_release.mkdir()
        resolver_manifest = {
            "release_id": "mailbox-secret-resolver-v1-sha256-" + "1" * 64,
            "source_commit_sha": source_sha,
            "resolver_worker_sha256": "2" * 64,
        }
        control_manifest = {
            "release_id": "cloudflare-v1-sha256-" + "3" * 64,
            "source": {
                "repository": CANONICAL_REPOSITORY,
                "commit_sha": source_sha,
                "authority": "accepted-main",
            },
            "migrations": {"files": [{"path": "migrations/d1/0001_catalog.sql"}]},
        }
        (resolver_release / "release-manifest.json").write_bytes(
            canonical_document(resolver_manifest)
        )
        (control_release / "release-manifest.json").write_bytes(
            canonical_document(control_manifest)
        )
        validate_release_identities(
            source_sha=source_sha,
            resolver_manifest_path=resolver_release / "release-manifest.json",
            control_manifest_path=control_release / "release-manifest.json",
            resolver_release_id=resolver_manifest["release_id"],
            resolver_worker_sha256=resolver_manifest["resolver_worker_sha256"],
            control_plane_release_id=control_manifest["release_id"],
        )
        resolver_query = root / "resolver-query.json"
        control_query = root / "control-query.json"
        resolver_query.write_text('[{"results":[{"name":"0001_resolver.sql"}]}]', encoding="utf-8")
        control_query.write_text('[{"results":[{"name":"0001_catalog.sql"}]}]', encoding="utf-8")
        verify_remote_d1(
            resolver_release, control_release, resolver_query, control_query
        )
        control_query.write_text('[{"results":[]}]', encoding="utf-8")
        expect_rejected(
            "incomplete remote D1 ledger",
            lambda: verify_remote_d1(
                resolver_release, control_release, resolver_query, control_query
            ),
        )
        control_query.write_text('[{"results":[{"name":"0001_catalog.sql"}]}]', encoding="utf-8")
        resolver_status = root / "resolver-status.json"
        control_status = root / "control-status.json"
        smoke_body = root / "smoke-body"
        resolver_status.write_text('{"versions":[{"id":"resolver-version"}]}', encoding="utf-8")
        control_status.write_text('{"versions":[{"id":"control-version"}]}', encoding="utf-8")
        smoke_body.write_text("ok", encoding="utf-8")
        evidence_output = root / "evidence.json"
        attest(
            environment="staging",
            resolver_manifest_path=resolver_release / "release-manifest.json",
            control_manifest_path=control_release / "release-manifest.json",
            resolver_status_path=resolver_status,
            control_status_path=control_status,
            smoke_body_path=smoke_body,
            workflow_run_id="23",
            workflow_run_attempt="1",
            evidence_output=evidence_output,
        )
        validate_staging_evidence(
            evidence_path=evidence_output,
            source_sha=source_sha,
            resolver_release_id=resolver_manifest["release_id"],
            resolver_worker_sha256=resolver_manifest["resolver_worker_sha256"],
            control_plane_release_id=control_manifest["release_id"],
            staging_promotion_run_id="23",
            staging_run_attempt="1",
        )
        production_evidence = require_object(load_json(evidence_output, "evidence fixture"), "evidence fixture")
        production_evidence["environment"] = "production"
        evidence_output.write_bytes(canonical_document(production_evidence))
        expect_rejected(
            "production evidence substituted for staging",
            lambda: validate_staging_evidence(
                evidence_path=evidence_output,
                source_sha=source_sha,
                resolver_release_id=resolver_manifest["release_id"],
                resolver_worker_sha256=resolver_manifest["resolver_worker_sha256"],
                control_plane_release_id=control_manifest["release_id"],
                staging_promotion_run_id="23",
                staging_run_attempt="1",
            ),
        )
    print("Mailbox resolver promotion positive and negative self-tests passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("self-test")
    preflight = commands.add_parser("github-preflight")
    preflight.add_argument("--environment", choices=ENVIRONMENTS, required=True)
    preflight.add_argument("--source-sha", required=True)
    preflight.add_argument("--resolver-release-run-id", required=True)
    preflight.add_argument("--resolver-artifact-id", required=True)
    preflight.add_argument("--resolver-artifact-digest", required=True)
    preflight.add_argument("--resolver-release-id", required=True)
    preflight.add_argument("--resolver-worker-sha256", required=True)
    preflight.add_argument("--control-plane-release-run-id", required=True)
    preflight.add_argument("--control-plane-artifact-id", required=True)
    preflight.add_argument("--control-plane-artifact-digest", required=True)
    preflight.add_argument("--control-plane-release-id", required=True)
    preflight.add_argument("--staging-promotion-run-id", default="")
    preflight.add_argument("--staging-evidence-artifact-id", default="")
    preflight.add_argument("--staging-evidence-artifact-digest", default="")
    preflight.add_argument("--staging-run-attempt", default="")
    preflight.add_argument("--staging-evidence-confirmation", default="")
    preflight.add_argument("--confirmation", required=True)
    preflight.add_argument("--repository", required=True)
    preflight.add_argument("--workflow-ref", required=True)
    preflight.add_argument("--workflow-sha", required=True)
    preflight.add_argument("--api-url", required=True)
    preflight.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
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
    evidence.add_argument("--source-sha", required=True)
    evidence.add_argument("--resolver-release-id", required=True)
    evidence.add_argument("--resolver-worker-sha256", required=True)
    evidence.add_argument("--control-plane-release-id", required=True)
    evidence.add_argument("--staging-promotion-run-id", required=True)
    evidence.add_argument("--staging-run-attempt", required=True)
    identities = commands.add_parser("validate-release-identities")
    identities.add_argument("--source-sha", required=True)
    identities.add_argument("--resolver-manifest", type=Path, required=True)
    identities.add_argument("--control-plane-manifest", type=Path, required=True)
    identities.add_argument("--resolver-release-id", required=True)
    identities.add_argument("--resolver-worker-sha256", required=True)
    identities.add_argument("--control-plane-release-id", required=True)
    d1 = commands.add_parser("verify-remote-d1")
    d1.add_argument("--resolver-release", type=Path, required=True)
    d1.add_argument("--control-plane-release", type=Path, required=True)
    d1.add_argument("--resolver-query", type=Path, required=True)
    d1.add_argument("--control-plane-query", type=Path, required=True)
    attestation = commands.add_parser("attest")
    attestation.add_argument("--environment", choices=ENVIRONMENTS, required=True)
    attestation.add_argument("--resolver-manifest", type=Path, required=True)
    attestation.add_argument("--control-plane-manifest", type=Path, required=True)
    attestation.add_argument("--resolver-status", type=Path, required=True)
    attestation.add_argument("--control-plane-status", type=Path, required=True)
    attestation.add_argument("--smoke-body", type=Path, required=True)
    attestation.add_argument("--workflow-run-id", required=True)
    attestation.add_argument("--workflow-run-attempt", required=True)
    attestation.add_argument("--evidence-output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "self-test":
        self_test()
    elif args.command == "github-preflight":
        github_preflight(args)
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
        validate_staging_evidence(
            evidence_path=args.evidence,
            source_sha=args.source_sha,
            resolver_release_id=args.resolver_release_id,
            resolver_worker_sha256=args.resolver_worker_sha256,
            control_plane_release_id=args.control_plane_release_id,
            staging_promotion_run_id=args.staging_promotion_run_id,
            staging_run_attempt=args.staging_run_attempt,
        )
    elif args.command == "validate-release-identities":
        validate_release_identities(
            source_sha=args.source_sha,
            resolver_manifest_path=args.resolver_manifest,
            control_manifest_path=args.control_plane_manifest,
            resolver_release_id=args.resolver_release_id,
            resolver_worker_sha256=args.resolver_worker_sha256,
            control_plane_release_id=args.control_plane_release_id,
        )
    elif args.command == "verify-remote-d1":
        verify_remote_d1(
            args.resolver_release,
            args.control_plane_release,
            args.resolver_query,
            args.control_plane_query,
        )
    elif args.command == "attest":
        attest(
            environment=args.environment,
            resolver_manifest_path=args.resolver_manifest,
            control_manifest_path=args.control_plane_manifest,
            resolver_status_path=args.resolver_status,
            control_status_path=args.control_plane_status,
            smoke_body_path=args.smoke_body,
            workflow_run_id=args.workflow_run_id,
            workflow_run_attempt=args.workflow_run_attempt,
            evidence_output=args.evidence_output,
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except PromotionError as error:
        raise SystemExit(f"mailbox resolver promotion rejected: {error}") from error
