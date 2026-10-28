#!/usr/bin/env python3
"""Render the no-rebuild AR-11 Core Wrangler overlay from immutable release bits.

This adapter performs deterministic path/resource substitution and projects exact target identity
into the staging runtime. It does not decide release compatibility, promotion authorization, D1
policy, rollback, or production readiness; those decisions belong to native opsctl release/
promotion policy and the guarded promotion workflow.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from pathlib import Path
from urllib.parse import urlparse
from typing import Any

RESOURCE_RE = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$")
ACCOUNT_RE = re.compile(r"^[0-9a-f]{32}$")
D1_RE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
AUDIENCE_RE = re.compile(r"^[A-Za-z0-9_-]{16,128}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
APPROVED_RUNTIME_INVENTORIES_RE = re.compile(r"^[0-9a-f]{64}(?:,[0-9a-f]{64})?$")
RELEASE_SET_V3_RE = re.compile(r"^release-set-v3-sha256-[0-9a-f]{64}$")
TARGET_AUTHORIZATION_VAR = "TARGET_AUTHORIZATION_OBSERVATION"
TARGET_AUTHORIZATION_SCHEMA = "target-v1"
REQUIRED_CONTROL_FIELDS = {
    "worker_name",
    "account_id",
    "custom_domain",
    "access_issuer",
    "access_audience",
    "approved_runtime_inventories",
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
    result = {
        field: bounded(candidate[field], f"control_plane.{field}")
        for field in REQUIRED_CONTROL_FIELDS
    }
    owner = candidate.get("initial_owner_email", "")
    if not isinstance(owner, str) or len(owner) > 254 or any(ord(ch) < 32 or ord(ch) == 127 for ch in owner):
        fail("control_plane.initial_owner_email must be a bounded single-line string")
    result["initial_owner_email"] = owner
    for field in (
        "worker_name",
        "d1_database_name",
        "r2_bucket_name",
        "integration_events_queue",
    ):
        if RESOURCE_RE.fullmatch(result[field]) is None:
            fail(f"control_plane.{field} is not a bounded Cloudflare resource name")
    if ACCOUNT_RE.fullmatch(result["account_id"]) is None:
        fail("control_plane.account_id is not a Cloudflare account id")
    if D1_RE.fullmatch(result["d1_database_id"]) is None:
        fail("control_plane.d1_database_id is not a D1 UUID")
    if AUDIENCE_RE.fullmatch(result["access_audience"]) is None:
        fail("control_plane.access_audience shape is invalid")
    allowed = result["approved_runtime_inventories"]
    if APPROVED_RUNTIME_INVENTORIES_RE.fullmatch(allowed) is None:
        fail("control_plane.approved_runtime_inventories must be one or two exact lowercase SHA-256s")
    if "," in allowed and allowed.split(",")[0] == allowed.split(",")[1]:
        fail("control_plane.approved_runtime_inventories must not duplicate a runtime")
    issuer = urlparse(result["access_issuer"])
    if (
        issuer.scheme != "https"
        or not issuer.hostname
        or issuer.path not in ("", "/")
        or issuer.query
        or issuer.fragment
    ):
        fail("control_plane.access_issuer must be one HTTPS origin")
    domain = result["custom_domain"]
    if (
        "/" in domain
        or ":" in domain
        or "." not in domain
        or domain.endswith(".workers.dev")
    ):
        fail("control_plane.custom_domain is invalid")
    return result


def target_authorization_observation(
    vars_value: dict[str, Any],
    release_set_id: Any,
) -> str:
    environment = bounded(vars_value.get("CANONICAL_ENVIRONMENT"), "CANONICAL_ENVIRONMENT")
    profile_id = bounded(vars_value.get("CAPABILITY_PROFILE_ID"), "CAPABILITY_PROFILE_ID")
    profile_digest = bounded(
        vars_value.get("CAPABILITY_PROFILE_DIGEST"), "CAPABILITY_PROFILE_DIGEST"
    )
    release_set_id = bounded(release_set_id, "RELEASE_SET_ID")
    if environment != "staging" or profile_id != "rehearsal-core-v2":
        fail("target authorization projection is restricted to the rehearsal Core staging target")
    if SHA256_RE.fullmatch(profile_digest) is None:
        fail("CAPABILITY_PROFILE_DIGEST must be a lowercase SHA-256 digest")
    if RELEASE_SET_V3_RE.fullmatch(release_set_id) is None:
        fail("RELEASE_SET_ID must be an immutable Release Set v3 identity")
    return "|".join(
        [
            TARGET_AUTHORIZATION_SCHEMA,
            environment,
            profile_id,
            profile_digest,
            release_set_id,
        ]
    )


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
    if TARGET_AUTHORIZATION_VAR in vars_value:
        fail("immutable Wrangler config must not author target authorization")
    if (
        vars_value.get("CANONICAL_ENVIRONMENT") != "staging"
        or vars_value.get("CAPABILITY_PROFILE_ID") != "rehearsal-core-v2"
    ):
        fail("selected immutable config is not the AR-11 rehearsal Core profile")

    replacements = {
        "${STAGING_WORKER_NAME}": manifest["worker_name"],
        "${STAGING_ACCOUNT_ID}": manifest["account_id"],
        "${STAGING_CUSTOM_DOMAIN}": manifest["custom_domain"],
        "${STAGING_ACCESS_ISSUER}": manifest["access_issuer"],
        "${STAGING_ACCESS_AUDIENCE}": manifest["access_audience"],
        "${STAGING_APPROVED_RUNTIME_INVENTORIES}": manifest["approved_runtime_inventories"],
        "${STAGING_INITIAL_OWNER_EMAIL}": manifest["initial_owner_email"],
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
    if not isinstance(selected, dict):
        fail("rendered selected Core environment must remain an object")
    rendered_vars = selected.get("vars")
    if not isinstance(rendered_vars, dict):
        fail("rendered selected environment vars are missing")
    rendered_vars[TARGET_AUTHORIZATION_VAR] = target_authorization_observation(
        rendered_vars,
        os.environ.get("RELEASE_SET_ID"),
    )
    # `secrets.required` is repository policy metadata, not Wrangler deployment input.
    # Secret values are never transported here; remote binding names are observed with
    # `wrangler secret list` and validated before the exact-bits deploy.
    selected.pop("secrets", None)
    serialized_selected = json.dumps(selected, sort_keys=True)
    if "${STAGING_" in serialized_selected:
        fail("selected Core environment still contains unresolved staging placeholders")
    if (
        "MAILBOX_JOBS" in serialized_selected
        or "MAILBOX_SECRET_RESOLVER" in serialized_selected
    ):
        fail("Mail operational dependency unexpectedly entered Core deployment closure")

    worker_entry = release_root / "worker" / "worker" / "shim.mjs"
    frontend = release_root / "frontend"
    if worker_entry.is_symlink() or not worker_entry.is_file():
        fail("immutable release Worker entrypoint is missing")
    if (
        frontend.is_symlink()
        or not frontend.is_dir()
        or not (frontend / "index.html").is_file()
    ):
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
    output.write_text(
        json.dumps(rendered, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )


def self_test() -> None:
    try:
        control_manifest({"worker_name": "x"}, "production")
    except OverlayError:
        pass
    else:
        fail("production overlay negative fixture unexpectedly passed")

    digest = "a" * 64
    release_set_id = f"release-set-v3-sha256-{'b' * 64}"
    vars_value = {
        "CANONICAL_ENVIRONMENT": "staging",
        "CAPABILITY_PROFILE_ID": "rehearsal-core-v2",
        "CAPABILITY_PROFILE_DIGEST": digest,
    }
    expected = f"target-v1|staging|rehearsal-core-v2|{digest}|{release_set_id}"
    if target_authorization_observation(vars_value, release_set_id) != expected:
        fail("target authorization projection is not deterministic")
    try:
        target_authorization_observation(vars_value, "release-set-v2-sha256-" + "b" * 64)
    except OverlayError:
        pass
    else:
        fail("non-v3 Release Set target unexpectedly passed")

    # Exercise the actual no-rebuild renderer so setup authorization survives the
    # same target-config path used by canonical promotion, without provider effects.
    manifest = {
        "worker_name": "profile-staging", "account_id": "c" * 32,
        "custom_domain": "staging.example.invalid",
        "access_issuer": "https://team.cloudflareaccess.com", "access_audience": "d" * 32,
        "approved_runtime_inventories": "a" * 64,
        "d1_database_name": "catalog-staging", "d1_database_id": "11111111-1111-1111-1111-111111111111",
        "r2_bucket_name": "profiles-staging", "integration_events_queue": "events-staging",
    }
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "worker/worker").mkdir(parents=True)
        (root / "worker/worker/shim.mjs").write_text("// immutable test entrypoint\n", encoding="utf-8")
        (root / "frontend").mkdir()
        (root / "frontend/index.html").write_text("<!doctype html>", encoding="utf-8")
        manifest_path = root / "manifest.json"
        previous = os.environ.get("RELEASE_SET_ID")
        os.environ["RELEASE_SET_ID"] = release_set_id
        try:
            for index, contact in enumerate([None, "owner@staging.invalid"]):
                candidate = dict(manifest)
                if contact is not None:
                    candidate["initial_owner_email"] = contact
                manifest_path.write_text(json.dumps({"control_plane": candidate}), encoding="utf-8")
                output = root / f"overlay-{index}.json"
                render(source_config=Path(__file__).resolve().parents[1] / "deploy/cloudflare/wrangler.jsonc",
                       release_root=root, deploy_manifest=manifest_path, environment="staging", output=output)
                observed = json.loads(output.read_text(encoding="utf-8"))
                if observed["env"]["staging"]["vars"]["INITIAL_OWNER_EMAIL"] != (contact or ""):
                    fail("initial owner configuration was lost during canonical Core rendering")
                if observed["env"]["staging"]["vars"]["PROFILE_GENERATION_APPROVED_RUNTIME_INVENTORIES"] != candidate["approved_runtime_inventories"]:
                    fail("staging WORKING runtime pin was lost during the canonical no-rebuild Core rendering")
                if set(observed["env"]) != {"staging"} or "build" in observed:
                    fail("setup configuration expanded the promotion target/build scope")
        finally:
            if previous is None:
                os.environ.pop("RELEASE_SET_ID", None)
            else:
                os.environ["RELEASE_SET_ID"] = previous

    print("AR-11 Core no-rebuild overlay self-test passed.")


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
        if None in (
            args.source_config,
            args.release_root,
            args.deploy_manifest,
            args.output,
        ):
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
