#!/usr/bin/env python3
"""Fail-closed D3 Cloudflare staging/production promotion authority.

This module consumes one D2 accepted-main release archive. It never builds product source and never
owns migration mutation policy. D4 must explicitly define migration application/fail-forward rules.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import os
import re
import shutil
import tarfile
import tempfile
import urllib.error
import urllib.request
from pathlib import Path, PurePosixPath
from types import ModuleType
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_PATH = ROOT / ".github" / "workflows" / "cloudflare-promotion.yml"
PROMOTION_CONFIG = ROOT / "deploy" / "cloudflare" / "generated" / "wrangler.promotion.json"
MATERIALIZED_MANIFEST = ROOT / "artifacts" / "cloudflare-promotion" / "release-manifest.json"
ATTESTATION_ROOT = ROOT / "artifacts" / "cloudflare-promotion" / "attestations"
CANONICAL_REPOSITORY = "iamaman11/part-crm-emai-profile"
CANONICAL_QUALITY_WORKFLOW = ".github/workflows/quality-gate.yml"
CANONICAL_QUALITY_NAME = "Quality Gate"
ENVIRONMENTS = ("staging", "production")
RELEASE_ID_RE = re.compile(r"^cloudflare-v1-sha256-[0-9a-f]{64}$")
SHA40_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
REQUIRED_WORKFLOW_MARKERS = (
    "workflow_dispatch:",
    "release_sha:",
    "quality_run_id:",
    "artifact_id:",
    "artifact_sha256:",
    "release_id:",
    "confirmation:",
    "github-preflight",
    "environment: staging",
    "environment: production",
    "needs: [preflight]",
    "needs: [staging-deploy]",
    "needs: [staging-verify]",
    "needs: [production-deploy]",
    "CLOUDFLARE_DEPLOY_MANIFEST_JSON",
    "CLOUDFLARE_API_TOKEN",
    "CLOUDFLARE_ACCESS_CLIENT_ID",
    "CLOUDFLARE_ACCESS_CLIENT_SECRET",
    "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c",
    "wrangler@4.94.0",
    "--experimental-provision=false",
    "--experimental-auto-create=false",
    "verify-remote-d1",
    "deployments status",
    "attest",
)
FORBIDDEN_WORKFLOW_MARKERS = (
    "pull_request_target",
    "wrangler@latest",
    "continue-on-error: true",
    "CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n      - name: Preflight",
)


class PromotionError(ValueError):
    """Raised when a D3 promotion boundary fails closed."""


def fail(message: str) -> None:
    raise PromotionError(message)


def canonical_document(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_module(name: str, path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        fail(f"cannot load repository authority module: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def release_module() -> ModuleType:
    return load_module("cloudflare_release_authority", ROOT / "scripts" / "cloudflare-release.py")


def config_module() -> ModuleType:
    return load_module("cloudflare_config_authority", ROOT / "scripts" / "cloudflare-deploy-config.py")


def require_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        fail(f"{label} must be a non-empty string")
    return value


def parse_positive_int(value: str, label: str) -> int:
    if not value.isdigit() or int(value) <= 0:
        fail(f"{label} must be a positive integer")
    return int(value)


def check_repository_policy() -> None:
    if not WORKFLOW_PATH.is_file():
        fail("D3 permanent promotion workflow is missing")
    workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
    missing = [marker for marker in REQUIRED_WORKFLOW_MARKERS if marker not in workflow]
    if missing:
        fail(f"D3 promotion workflow is missing required fail-closed markers: {missing}")
    present = [marker for marker in FORBIDDEN_WORKFLOW_MARKERS if marker in workflow]
    if present:
        fail(f"D3 promotion workflow contains prohibited marker(s): {present}")
    if workflow.count("environment: production") < 2:
        fail("production deploy and production verification must both cross the protected environment boundary")
    if workflow.count("environment: staging") < 2:
        fail("staging deploy and staging verification must both use the staging environment")
    if "if:" in workflow:
        fail("D3 deployment jobs must not use conditional skips as pseudo-green evidence")
    if "push:" in workflow or "pull_request:" in workflow:
        fail("D3 promotion must be explicit workflow_dispatch, never automatic merge/push deployment")

    release = release_module()
    config = config_module()
    release.check_repository_policy(ROOT)
    config.validate_template()
    print("D3 Cloudflare promotion repository policy passed.")


def validate_release_inputs(
    *,
    release_sha: str,
    quality_run_id: str,
    artifact_id: str,
    artifact_sha256: str,
    release_id: str,
    confirmation: str,
    repository: str,
    workflow_ref: str,
    workflow_sha: str,
) -> dict[str, Any]:
    if repository != CANONICAL_REPOSITORY:
        fail(f"promotion repository must be {CANONICAL_REPOSITORY}")
    if SHA40_RE.fullmatch(release_sha) is None or SHA40_RE.fullmatch(workflow_sha) is None:
        fail("release/workflow SHA must be exact lowercase 40-hex commits")
    if workflow_ref != "refs/heads/main":
        fail("promotion workflow must run from refs/heads/main")
    if workflow_sha != release_sha:
        fail("D3 currently promotes only the exact current accepted main release; D4 owns compatible rollback policy")
    run_id = parse_positive_int(quality_run_id, "Quality Gate run id")
    artifact = parse_positive_int(artifact_id, "release artifact id")
    if SHA256_RE.fullmatch(artifact_sha256) is None:
        fail("release artifact SHA-256 must be 64 lowercase hex characters")
    if RELEASE_ID_RE.fullmatch(release_id) is None:
        fail("release id is not a canonical D2 immutable release identifier")
    if confirmation != release_id:
        fail("typed production confirmation must exactly equal the immutable release id")
    return {"quality_run_id": run_id, "artifact_id": artifact}


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
    except (urllib.error.URLError, json.JSONDecodeError) as error:
        raise PromotionError(f"cannot read GitHub promotion authority {path}: {error}") from error


def validate_github_authority(
    *,
    release_sha: str,
    quality_run_id: int,
    artifact_id: int,
    artifact_sha256: str,
    release_id: str,
    main_ref: Any,
    quality_run: Any,
    artifacts: Any,
    staging_environment: Any,
    production_environment: Any,
) -> None:
    if not isinstance(main_ref, dict) or main_ref.get("object", {}).get("sha") != release_sha:
        fail("requested release is no longer exact current main")

    if not isinstance(quality_run, dict):
        fail("Quality Gate run authority is not an object")
    expected_run = {
        "id": quality_run_id,
        "name": CANONICAL_QUALITY_NAME,
        "event": "push",
        "head_branch": "main",
        "head_sha": release_sha,
        "status": "completed",
        "conclusion": "success",
        "path": CANONICAL_QUALITY_WORKFLOW,
    }
    mismatches = {key: (quality_run.get(key), expected) for key, expected in expected_run.items() if quality_run.get(key) != expected}
    if mismatches:
        fail(f"Quality Gate run is not exact accepted-main evidence: {mismatches}")

    if not isinstance(artifacts, dict) or not isinstance(artifacts.get("artifacts"), list):
        fail("Quality Gate artifact inventory is invalid")
    matching = [item for item in artifacts["artifacts"] if isinstance(item, dict) and item.get("id") == artifact_id]
    if len(matching) != 1:
        fail("exact release artifact id is not uniquely owned by the supplied Quality Gate run")
    artifact = matching[0]
    expected_name = f"{release_id}.tar"
    if artifact.get("name") != expected_name:
        fail("release artifact name does not match immutable release id")
    if artifact.get("expired") is not False:
        fail("release artifact is expired")
    if artifact.get("digest") != f"sha256:{artifact_sha256}":
        fail("GitHub artifact digest does not match requested immutable artifact digest")
    run = artifact.get("workflow_run")
    if isinstance(run, dict):
        if run.get("id") != quality_run_id or run.get("head_sha") != release_sha:
            fail("release artifact workflow ownership does not match exact accepted source")

    validate_staging_environment(staging_environment)
    validate_production_environment(production_environment)


def validate_staging_environment(environment: Any) -> None:
    if not isinstance(environment, dict) or environment.get("name") != "staging":
        fail("real GitHub staging environment must exist before D3 can deploy")


def validate_production_environment(environment: Any) -> None:
    if not isinstance(environment, dict) or environment.get("name") != "production":
        fail("real GitHub production environment must exist before D3 can promote")
    if environment.get("can_admins_bypass") is not False:
        fail("production environment must disable administrator protection bypass")
    rules = environment.get("protection_rules")
    if not isinstance(rules, list):
        fail("production environment protection rules are missing")
    required = [rule for rule in rules if isinstance(rule, dict) and rule.get("type") == "required_reviewers"]
    if len(required) != 1:
        fail("production environment must have exactly one required-reviewers protection rule")
    reviewers = required[0].get("reviewers")
    if not isinstance(reviewers, list) or not reviewers:
        fail("production environment must have at least one required reviewer")


def github_preflight(args: argparse.Namespace) -> None:
    parsed = validate_release_inputs(
        release_sha=args.release_sha,
        quality_run_id=args.quality_run_id,
        artifact_id=args.artifact_id,
        artifact_sha256=args.artifact_sha256,
        release_id=args.release_id,
        confirmation=args.confirmation,
        repository=args.repository,
        workflow_ref=args.workflow_ref,
        workflow_sha=args.workflow_sha,
    )
    if not args.token:
        fail("GitHub token is required for promotion preflight")
    repository_path = f"/repos/{args.repository}"
    main_ref = api_get(args.api_url, args.token, repository_path + "/git/ref/heads/main")
    quality_run = api_get(args.api_url, args.token, repository_path + f"/actions/runs/{parsed['quality_run_id']}")
    artifacts = api_get(args.api_url, args.token, repository_path + f"/actions/runs/{parsed['quality_run_id']}/artifacts?per_page=100")
    staging_environment = api_get(args.api_url, args.token, repository_path + "/environments/staging")
    production_environment = api_get(args.api_url, args.token, repository_path + "/environments/production")
    validate_github_authority(
        release_sha=args.release_sha,
        quality_run_id=parsed["quality_run_id"],
        artifact_id=parsed["artifact_id"],
        artifact_sha256=args.artifact_sha256,
        release_id=args.release_id,
        main_ref=main_ref,
        quality_run=quality_run,
        artifacts=artifacts,
        staging_environment=staging_environment,
        production_environment=production_environment,
    )
    print(f"D3 GitHub preflight accepted exact release {args.release_id} from {args.release_sha}.")


def load_json_file(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PromotionError(f"cannot read {label} JSON: {error}") from error


def safely_extract_release(archive: Path, destination: Path, release: ModuleType) -> Path:
    if destination.exists():
        fail(f"promotion extraction destination already exists: {destination}")
    destination.mkdir(parents=True)
    with tarfile.open(archive, mode="r:") as handle:
        members = release.safe_archive_members(handle)
        for member in members:
            target = destination / member.name
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            source = handle.extractfile(member)
            if source is None:
                fail(f"cannot extract validated release member: {member.name}")
            target.write_bytes(source.read())
    base = destination / "cloudflare-release"
    if not base.is_dir():
        fail("release archive lacks canonical cloudflare-release root")
    releases = [path for path in base.iterdir() if path.is_dir() and not path.is_symlink()]
    if len(releases) != 1:
        fail("release archive must contain exactly one immutable release directory")
    return releases[0]


def render_promotion_config(environment: str, manifest: Any, *, fixture: bool = False) -> tuple[dict[str, Any], dict[str, str]]:
    if environment not in ENVIRONMENTS:
        fail(f"unsupported promotion environment: {environment}")
    config = config_module()
    template = config.validate_template()
    validated = config.validate_manifest(environment, manifest, fixture=fixture)
    environment_template = copy.deepcopy(template["env"][environment])
    environment_config = config.substitute(environment_template, config.token_map(environment, validated))
    serialized = json.dumps(environment_config, sort_keys=True)
    if "${" in serialized:
        fail("promotion environment config retained an unresolved placeholder")

    # Exact D2 product bytes are already built. Cloudflare documents that Wrangler runs a configured
    # custom build as part of deploy, so D3 deliberately removes only the build instruction from the
    # ephemeral deploy document. The tracked canonical D1 authority remains unchanged.
    rendered = {key: copy.deepcopy(value) for key, value in template.items() if key not in {"build", "env"}}
    rendered["env"] = {environment: environment_config}
    if "build" in rendered:
        fail("promotion config must not rebuild accepted release source")
    if set(rendered["env"]) != {environment}:
        fail("promotion config must expose exactly one target environment")
    return rendered, validated


def write_github_output(values: dict[str, str]) -> None:
    target = os.environ.get("GITHUB_OUTPUT")
    if not target:
        return
    with Path(target).open("a", encoding="utf-8") as handle:
        for key, value in values.items():
            handle.write(f"{key}={value}\n")


def prepare(args: argparse.Namespace) -> None:
    archive = args.archive.resolve()
    if not archive.is_file() or archive.is_symlink():
        fail("promotion release archive must be one regular file")
    if sha256_file(archive) != args.artifact_sha256:
        fail("downloaded release archive SHA-256 differs from dispatch authority")
    release = release_module()
    manifest = release.verify_archive(
        ROOT,
        archive,
        expected_source_sha=args.release_sha,
        expected_repository=args.repository,
        expected_authority="accepted-main",
    )
    if manifest.get("release_id") != args.release_id:
        fail("downloaded release manifest id differs from dispatch authority")

    with tempfile.TemporaryDirectory(prefix="cloudflare-promotion-") as temporary:
        extracted = safely_extract_release(archive, Path(temporary) / "release", release)
        if extracted.name != args.release_id:
            fail("release directory name differs from immutable release id")
        frontend_target = ROOT / "frontend" / "dist"
        worker_target = ROOT / "apps" / "control-plane-worker" / "build"
        if frontend_target.exists() or worker_target.exists():
            fail("promotion materialization targets must start absent in a clean exact-source checkout")
        shutil.copytree(extracted / "frontend", frontend_target)
        shutil.copytree(extracted / "worker", worker_target)
        release.compare_inventory("frontend", manifest["artifacts"]["frontend"], release.inventory_directory(frontend_target))
        release.compare_inventory("worker", manifest["artifacts"]["worker"], release.worker_runtime_inventory(worker_target))

    environment_manifest = load_json_file(args.environment_manifest, f"{args.environment} deploy manifest")
    rendered, validated_environment = render_promotion_config(args.environment, environment_manifest)
    PROMOTION_CONFIG.parent.mkdir(parents=True, exist_ok=True)
    PROMOTION_CONFIG.write_bytes(canonical_document(rendered))
    MATERIALIZED_MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MATERIALIZED_MANIFEST.write_bytes(canonical_document(manifest))
    write_github_output(
        {
            "config": PROMOTION_CONFIG.relative_to(ROOT).as_posix(),
            "release_manifest": MATERIALIZED_MANIFEST.relative_to(ROOT).as_posix(),
            "custom_domain": validated_environment["custom_domain"],
            "release_id": args.release_id,
        }
    )
    print(f"Prepared exact D2 release {args.release_id} for {args.environment} without rebuilding source.")


def parse_remote_d1_names(document: Any) -> list[str]:
    if not isinstance(document, list) or len(document) != 1 or not isinstance(document[0], dict):
        fail("remote D1 query result must be one Wrangler JSON result object")
    results = document[0].get("results")
    if not isinstance(results, list):
        fail("remote D1 query lacks results array")
    names: list[str] = []
    for row in results:
        if not isinstance(row, dict) or set(row) != {"name"} or not isinstance(row["name"], str):
            fail("remote D1 migration query returned an unexpected row shape")
        names.append(row["name"])
    return names


def verify_remote_d1(args: argparse.Namespace) -> None:
    manifest = load_json_file(args.release_manifest, "release manifest")
    migrations = manifest.get("migrations") if isinstance(manifest, dict) else None
    files = migrations.get("files") if isinstance(migrations, dict) else None
    if not isinstance(files, list) or not files:
        fail("release manifest has no D1 migration inventory")
    expected: list[str] = []
    for entry in files:
        if not isinstance(entry, dict) or not isinstance(entry.get("path"), str):
            fail("release migration inventory row is invalid")
        pure = PurePosixPath(entry["path"])
        if len(pure.parts) != 3 or pure.parts[:2] != ("migrations", "d1") or not pure.name.endswith(".sql"):
            fail(f"unexpected release migration path: {entry['path']}")
        expected.append(pure.name)
    actual = parse_remote_d1_names(load_json_file(args.query_json, "remote D1 migration query"))
    if actual != expected:
        fail(f"remote D1 migration set differs from exact release: expected={expected}, actual={actual}")
    print(f"Remote D1 has the exact {len(expected)}-migration release set; D3 performs no schema mutation.")


def attestation_document(
    *,
    environment: str,
    release_manifest: dict[str, Any],
    artifact_sha256: str,
    workflow_run_id: str,
    workflow_run_attempt: str,
    deployment_status: Any,
    smoke_body_sha256: str,
    smoke_body_size: int,
) -> dict[str, Any]:
    if environment not in ENVIRONMENTS:
        fail("attestation environment is invalid")
    source = release_manifest.get("source")
    release_id = release_manifest.get("release_id")
    if not isinstance(source, dict) or source.get("authority") != "accepted-main":
        fail("promotion attestation requires an accepted-main release manifest")
    if not isinstance(release_id, str) or RELEASE_ID_RE.fullmatch(release_id) is None:
        fail("promotion attestation release id is invalid")
    if SHA256_RE.fullmatch(artifact_sha256) is None or SHA256_RE.fullmatch(smoke_body_sha256) is None:
        fail("promotion attestation digest is invalid")
    run_id = parse_positive_int(workflow_run_id, "workflow run id")
    run_attempt = parse_positive_int(workflow_run_attempt, "workflow run attempt")
    if not isinstance(deployment_status, (dict, list)):
        fail("Cloudflare deployment status must be JSON object/array")
    deployment_digest = hashlib.sha256(json.dumps(deployment_status, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {
        "schema_version": 1,
        "environment": environment,
        "release_id": release_id,
        "source": {
            "repository": source.get("repository"),
            "commit_sha": source.get("commit_sha"),
            "authority": source.get("authority"),
        },
        "release_artifact_sha256": artifact_sha256,
        "github": {"run_id": run_id, "run_attempt": run_attempt},
        "cloudflare_deployment_status_sha256": deployment_digest,
        "smoke": {"response_sha256": smoke_body_sha256, "response_size": smoke_body_size},
    }


def attest(args: argparse.Namespace) -> None:
    release_manifest = load_json_file(args.release_manifest, "release manifest")
    deployment_status = load_json_file(args.deployment_status_json, "Cloudflare deployment status")
    if not args.smoke_body.is_file() or args.smoke_body.is_symlink():
        fail("smoke response body must be a regular file")
    document = attestation_document(
        environment=args.environment,
        release_manifest=release_manifest,
        artifact_sha256=args.artifact_sha256,
        workflow_run_id=args.workflow_run_id,
        workflow_run_attempt=args.workflow_run_attempt,
        deployment_status=deployment_status,
        smoke_body_sha256=sha256_file(args.smoke_body),
        smoke_body_size=args.smoke_body.stat().st_size,
    )
    release_id = document["release_id"]
    output = ATTESTATION_ROOT / f"{args.environment}-{release_id}.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        fail(f"promotion attestation already exists: {output}")
    output.write_bytes(canonical_document(document))
    write_github_output({"attestation": output.relative_to(ROOT).as_posix()})
    print(f"Recorded {args.environment} promotion attestation for {release_id}.")


def expect_rejected(label: str, operation: Any) -> None:
    try:
        operation()
    except PromotionError:
        return
    fail(f"negative D3 fixture unexpectedly passed: {label}")


def self_test() -> None:
    release_sha = "a" * 40
    release_id = "cloudflare-v1-sha256-" + "b" * 64
    artifact_sha = "c" * 64
    run_id = 123
    artifact_id = 456
    main_ref = {"object": {"sha": release_sha}}
    run = {
        "id": run_id,
        "name": CANONICAL_QUALITY_NAME,
        "event": "push",
        "head_branch": "main",
        "head_sha": release_sha,
        "status": "completed",
        "conclusion": "success",
        "path": CANONICAL_QUALITY_WORKFLOW,
    }
    artifacts = {
        "artifacts": [
            {
                "id": artifact_id,
                "name": f"{release_id}.tar",
                "expired": False,
                "digest": f"sha256:{artifact_sha}",
                "workflow_run": {"id": run_id, "head_sha": release_sha},
            }
        ]
    }
    staging = {"name": "staging", "protection_rules": []}
    production = {
        "name": "production",
        "can_admins_bypass": False,
        "protection_rules": [{"type": "required_reviewers", "reviewers": [{"type": "User", "reviewer": {"login": "release-reviewer"}}]}],
    }
    validate_release_inputs(
        release_sha=release_sha,
        quality_run_id=str(run_id),
        artifact_id=str(artifact_id),
        artifact_sha256=artifact_sha,
        release_id=release_id,
        confirmation=release_id,
        repository=CANONICAL_REPOSITORY,
        workflow_ref="refs/heads/main",
        workflow_sha=release_sha,
    )
    validate_github_authority(
        release_sha=release_sha,
        quality_run_id=run_id,
        artifact_id=artifact_id,
        artifact_sha256=artifact_sha,
        release_id=release_id,
        main_ref=main_ref,
        quality_run=run,
        artifacts=artifacts,
        staging_environment=staging,
        production_environment=production,
    )

    negative_cases = [
        ("fork workflow", lambda: validate_release_inputs(
            release_sha=release_sha, quality_run_id=str(run_id), artifact_id=str(artifact_id), artifact_sha256=artifact_sha,
            release_id=release_id, confirmation=release_id, repository="example/fork", workflow_ref="refs/heads/main", workflow_sha=release_sha)),
        ("non-main workflow", lambda: validate_release_inputs(
            release_sha=release_sha, quality_run_id=str(run_id), artifact_id=str(artifact_id), artifact_sha256=artifact_sha,
            release_id=release_id, confirmation=release_id, repository=CANONICAL_REPOSITORY, workflow_ref="refs/heads/topic", workflow_sha=release_sha)),
        ("source mismatch", lambda: validate_release_inputs(
            release_sha=release_sha, quality_run_id=str(run_id), artifact_id=str(artifact_id), artifact_sha256=artifact_sha,
            release_id=release_id, confirmation=release_id, repository=CANONICAL_REPOSITORY, workflow_ref="refs/heads/main", workflow_sha="d" * 40)),
        ("confirmation mismatch", lambda: validate_release_inputs(
            release_sha=release_sha, quality_run_id=str(run_id), artifact_id=str(artifact_id), artifact_sha256=artifact_sha,
            release_id=release_id, confirmation="wrong", repository=CANONICAL_REPOSITORY, workflow_ref="refs/heads/main", workflow_sha=release_sha)),
    ]
    for label, operation in negative_cases:
        expect_rejected(label, operation)

    failed_run = dict(run)
    failed_run["conclusion"] = "failure"
    expect_rejected("failed Quality Gate", lambda: validate_github_authority(
        release_sha=release_sha, quality_run_id=run_id, artifact_id=artifact_id, artifact_sha256=artifact_sha,
        release_id=release_id, main_ref=main_ref, quality_run=failed_run, artifacts=artifacts,
        staging_environment=staging, production_environment=production))
    wrong_artifact = copy.deepcopy(artifacts)
    wrong_artifact["artifacts"][0]["digest"] = "sha256:" + "d" * 64
    expect_rejected("artifact substitution", lambda: validate_github_authority(
        release_sha=release_sha, quality_run_id=run_id, artifact_id=artifact_id, artifact_sha256=artifact_sha,
        release_id=release_id, main_ref=main_ref, quality_run=run, artifacts=wrong_artifact,
        staging_environment=staging, production_environment=production))
    expect_rejected("missing staging environment", lambda: validate_github_authority(
        release_sha=release_sha, quality_run_id=run_id, artifact_id=artifact_id, artifact_sha256=artifact_sha,
        release_id=release_id, main_ref=main_ref, quality_run=run, artifacts=artifacts,
        staging_environment={}, production_environment=production))
    unprotected = copy.deepcopy(production)
    unprotected["protection_rules"] = []
    expect_rejected("unprotected production", lambda: validate_github_authority(
        release_sha=release_sha, quality_run_id=run_id, artifact_id=artifact_id, artifact_sha256=artifact_sha,
        release_id=release_id, main_ref=main_ref, quality_run=run, artifacts=artifacts,
        staging_environment=staging, production_environment=unprotected))
    bypassable = copy.deepcopy(production)
    bypassable["can_admins_bypass"] = True
    expect_rejected("admin-bypass production", lambda: validate_github_authority(
        release_sha=release_sha, quality_run_id=run_id, artifact_id=artifact_id, artifact_sha256=artifact_sha,
        release_id=release_id, main_ref=main_ref, quality_run=run, artifacts=artifacts,
        staging_environment=staging, production_environment=bypassable))

    config = config_module()
    rendered, values = render_promotion_config("staging", config.fixture_manifest("staging"), fixture=True)
    if "build" in rendered or set(rendered["env"]) != {"staging"} or values["worker_name"] != "profile-control-staging":
        fail("positive single-environment promotion config fixture is invalid")
    if "${" in json.dumps(rendered):
        fail("positive promotion config retained an unresolved placeholder")

    expected_names = ["0001_first.sql", "0002_second.sql"]
    remote = [{"results": [{"name": name} for name in expected_names]}]
    if parse_remote_d1_names(remote) != expected_names:
        fail("positive remote D1 fixture did not preserve migration identity")
    expect_rejected("unexpected D1 row", lambda: parse_remote_d1_names([{"results": [{"name": "0001.sql", "extra": 1}]}]))

    attestation = attestation_document(
        environment="staging",
        release_manifest={"release_id": release_id, "source": {"repository": CANONICAL_REPOSITORY, "commit_sha": release_sha, "authority": "accepted-main"}},
        artifact_sha256=artifact_sha,
        workflow_run_id="789",
        workflow_run_attempt="1",
        deployment_status={"versions": [{"id": "synthetic-version"}]},
        smoke_body_sha256="e" * 64,
        smoke_body_size=12,
    )
    if attestation["source"]["commit_sha"] != release_sha or "deployment_status" in attestation:
        fail("promotion attestation must bind source while retaining only a digest of provider status")
    print("D3 Cloudflare promotion positive and negative self-tests passed.")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("check")
    subparsers.add_parser("self-test")

    preflight = subparsers.add_parser("github-preflight")
    preflight.add_argument("--release-sha", required=True)
    preflight.add_argument("--quality-run-id", required=True)
    preflight.add_argument("--artifact-id", required=True)
    preflight.add_argument("--artifact-sha256", required=True)
    preflight.add_argument("--release-id", required=True)
    preflight.add_argument("--confirmation", required=True)
    preflight.add_argument("--repository", required=True)
    preflight.add_argument("--workflow-ref", required=True)
    preflight.add_argument("--workflow-sha", required=True)
    preflight.add_argument("--api-url", required=True)
    preflight.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))

    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("--archive", type=Path, required=True)
    prepare_parser.add_argument("--artifact-sha256", required=True)
    prepare_parser.add_argument("--release-sha", required=True)
    prepare_parser.add_argument("--release-id", required=True)
    prepare_parser.add_argument("--repository", required=True)
    prepare_parser.add_argument("--environment", choices=ENVIRONMENTS, required=True)
    prepare_parser.add_argument("--environment-manifest", type=Path, required=True)

    d1 = subparsers.add_parser("verify-remote-d1")
    d1.add_argument("--release-manifest", type=Path, required=True)
    d1.add_argument("--query-json", type=Path, required=True)

    att = subparsers.add_parser("attest")
    att.add_argument("--environment", choices=ENVIRONMENTS, required=True)
    att.add_argument("--release-manifest", type=Path, required=True)
    att.add_argument("--artifact-sha256", required=True)
    att.add_argument("--workflow-run-id", required=True)
    att.add_argument("--workflow-run-attempt", required=True)
    att.add_argument("--deployment-status-json", type=Path, required=True)
    att.add_argument("--smoke-body", type=Path, required=True)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    if args.command == "check":
        check_repository_policy()
    elif args.command == "self-test":
        self_test()
    elif args.command == "github-preflight":
        github_preflight(args)
    elif args.command == "prepare":
        prepare(args)
    elif args.command == "verify-remote-d1":
        verify_remote_d1(args)
    elif args.command == "attest":
        attest(args)
    else:
        fail(f"unsupported command: {args.command}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except PromotionError as error:
        raise SystemExit(f"Cloudflare D3 promotion rejected: {error}") from error
