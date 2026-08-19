#!/usr/bin/env python3
"""Render the no-rebuild AR-11 Core Wrangler overlay from immutable release bits.

This adapter performs deterministic path/resource substitution only. It does not decide
release compatibility, promotion authorization, D1 policy, rollback, or production
readiness; those decisions belong to native opsctl release/promotion policy.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from urllib.parse import urlparse
from typing import Any

RESOURCE_RE = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$")
ACCOUNT_RE = re.compile(r"^[0-9a-f]{32}$")
D1_RE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
AUDIENCE_RE = re.compile(r"^[A-Za-z0-9_-]{16,128}$")
REQUIRED_CONTROL_FIELDS = {
    "worker_name",
    "account_id",
    "custom_domain",
    "access_issuer",
    "access_audience",
    "d1_database_name",
    "d1_database_id",
    "r2_bucket_name",
    "integration_events_queue",
}
FORBIDDEN_MARKERS = ("${", "changeme", "dummy", "placeholder", "replace_with", "secret-value")


class OverlayError(ValueError):
    pass


def fail(message: str) -> None:
    raise OverlayError(message)


def load(path: Path, label: str) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise OverlayError(f"{label} is invalid JSON: {error}") from error
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def bounded(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value or len(value) > 512:
        fail(f"{label} must be a bounded non-empty string")
    lowered = value.lower()
    if any(marker in lowered for marker in FORBIDDEN_MARKERS):
        fail(f"{label} contains a forbidden placeholder marker")
    return value


def control_manifest(document: dict[str, Any], environment: str) -> dict[str, str]:
    if environment != "staging":
        fail("AR-11 Core overlay adapter is staging-only; production execution is forbidden")
    if "control_plane" in document:
        candidate = document["control_plane"]
    else:
        candidate = document
    if not isinstance(candidate, dict):
        fail("control_plane manifest must be a JSON object")
    missing = REQUIRED_CONTROL_FIELDS - set(candidate)
    if missing:
        fail(f"control_plane manifest is missing Core fields: {sorted(missing)}")
    result = {field: bounded(candidate[field], f"control_plane.{field}") for field in REQUIRED_CONTROL_FIELDS}
    for field in ("worker_name", "d1_database_name", "r2_bucket_name", "integration_events_queue"):
        if RESOURCE_RE.fullmatch(result[field]) is None:
            fail(f"control_plane.{field} is not a bounded Cloudflare resource name")
    if ACCOUNT_RE.fullmatch(result["account_id"]) is None:
        fail("control_plane.account_id is not a Cloudflare account id")
    if D1_RE.fullmatch(result["d1_database_id"]) is None:
        fail("control_plane.d1_database_id is not a D1 UUID")
    if AUDIENCE_RE.fullmatch(result["access_audience"]) is None:
        fail("control_plane.access_audience shape is invalid")
    issuer = urlparse(result["access_issuer"])
    if issuer.scheme != "https" or not issuer.hostname or issuer.path not in ("", "/") or issuer.query or issuer.fragment:
        fail("control_plane.access_issuer must be one HTTPS origin")
    domain = result["custom_domain"]
    if "/" in domain or ":" in domain or "." not in domain or domain.endswith(".workers.dev"):
        fail("control_plane.custom_domain is invalid")
    return result


def relative_path(target: Path, output: Path) -> str:
    return Path(os.path.relpath(target.resolve(), output.parent.resolve())).as_posix()


def render(
    *,
    source_config: Path,
    release_root: Path,
    deploy_manifest: Path,
    environment: str,
    output: Path,
) -> None:
    config = load(source_config, "immutable release Wrangler config")
    manifest = control_manifest(load(deploy_manifest, "deploy manifest"), environment)
    envs = config.get("env")
    if not isinstance(envs, dict) or set(envs) != {"staging", "production"}:
        fail("immutable Wrangler config must contain exactly staging and production templates")
    selected = envs.get(environment)
    if not isinstance(selected, dict):
        fail(f"immutable Wrangler config has no {environment} environment")
    vars_value = selected.get("vars")
    if not isinstance(vars_value, dict):
        fail("selected environment vars are missing")
    if vars_value.get("CANONICAL_ENVIRONMENT") != "staging" or vars_value.get("CAPABILITY_PROFILE_ID") != "rehearsal-core-v1":
        fail("selected immutable config is not the AR-11 rehearsal Core profile")

    replacements = {
        "${STAGING_WORKER_NAME}": manifest["worker_name"],
        "${STAGING_ACCOUNT_ID}": manifest["account_id"],
        "${STAGING_CUSTOM_DOMAIN}": manifest["custom_domain"],
        "${STAGING_ACCESS_ISSUER}": manifest["access_issuer"],
        "${STAGING_ACCESS_AUDIENCE}": manifest["access_audience"],
        "${STAGING_D1_DATABASE_NAME}": manifest["d1_database_name"],
        "${STAGING_D1_DATABASE_ID}": manifest["d1_database_id"],
        "${STAGING_R2_BUCKET_NAME}": manifest["r2_bucket_name"],
        "${STAGING_INTEGRATION_EVENTS_QUEUE}": manifest["integration_events_queue"],
    }

    def substitute(value: Any) -> Any:
        if isinstance(value, dict):
            return {key: substitute(child) for key, child in value.items()}
        if isinstance(value, list):
            return [substitute(child) for child in value]
        if isinstance(value, str):
            return replacements.get(value, value)
        return value

    selected = substitute(selected)
    serialized_selected = json.dumps(selected, sort_keys=True)
    if "${STAGING_" in serialized_selected:
        fail("selected Core environment still contains unresolved staging placeholders")
    if "MAILBOX_JOBS" in serialized_selected or "MAILBOX_SECRET_RESOLVER" in serialized_selected:
        fail("Mail operational dependency unexpectedly entered Core deployment closure")

    worker_entry = release_root / "worker" / "worker" / "shim.mjs"
    frontend = release_root / "frontend"
    if worker_entry.is_symlink() or not worker_entry.is_file():
        fail("immutable release Worker entrypoint is missing")
    if frontend.is_symlink() or not frontend.is_dir() or not (frontend / "index.html").is_file():
        fail("immutable release frontend assets are missing")

    rendered = dict(config)
    rendered.pop("build", None)
    rendered["main"] = relative_path(worker_entry, output)
    assets = rendered.get("assets")
    if not isinstance(assets, dict):
        fail("immutable release assets config is missing")
    assets = dict(assets)
    assets["directory"] = relative_path(frontend, output)
    rendered["assets"] = assets
    rendered["env"] = {environment: selected}

    if "build" in rendered:
        fail("promotion overlay must never contain a build command")
    if output.exists():
        fail(f"promotion overlay already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(rendered, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def self_test() -> None:
    try:
        control_manifest({"worker_name": "x"}, "production")
    except OverlayError:
        print("AR-11 Core no-rebuild overlay self-test passed.")
        return
    fail("production overlay negative fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-config", type=Path)
    parser.add_argument("--release-root", type=Path)
    parser.add_argument("--deploy-manifest", type=Path)
    parser.add_argument("--environment", default="staging")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        if None in (args.source_config, args.release_root, args.deploy_manifest, args.output):
            fail("source-config, release-root, deploy-manifest and output are required")
        render(
            source_config=args.source_config,
            release_root=args.release_root,
            deploy_manifest=args.deploy_manifest,
            environment=args.environment,
            output=args.output,
        )
        return 0
    except (OSError, OverlayError) as error:
        print(f"AR-11 Core overlay error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
