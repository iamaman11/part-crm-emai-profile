#!/usr/bin/env python3
"""Render the no-rebuild AR-11 Core Wrangler overlay from immutable release bits.

This adapter performs deterministic path/resource substitution and projects exact target identity
into the staging runtime. It does not decide release compatibility, promotion authorization, D1
policy, rollback, or production readiness; those decisions belong to native opsctl release/
promotion policy and the guarded promotion workflow.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import sys
import tarfile
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


def verified_runtime_inventory(
    release_set_path: Path, archive_path: Path, expected_release_set_id: str | None = None,
) -> str:
    """Read the exact verified immutable Release Set runtime, never a caller fingerprint."""
    expected_release_set_id = expected_release_set_id or os.environ.get("RELEASE_SET_ID")
    release = load(release_set_path, "accepted Release Set")
    release_id = release.get("release_set_id")
    if (
        release.get("schema_version") != 3
        or not isinstance(release_id, str)
        or release_id != expected_release_set_id
        or RELEASE_SET_V3_RE.fullmatch(release_id) is None
    ):
        fail("runtime inventory is not bound to the exact accepted Release Set")
    components = release.get("components")
    runtime = components.get("runtime_bundle") if isinstance(components, dict) else None
    if (
        not isinstance(runtime, dict)
        or runtime.get("component_id") != "runtime_bundle"
        or runtime.get("artifact_path") != "components/runtime-bundle.tar"
        or not isinstance(runtime.get("component_manifest_sha256"), str)
        or SHA256_RE.fullmatch(runtime["component_manifest_sha256"]) is None
        or not isinstance(runtime.get("release_id"), str)
    ):
        fail("accepted Release Set has no exact runtime component manifest identity")
    if archive_path.is_symlink() or not archive_path.is_file():
        fail("accepted runtime archive must be a regular file")
    try:
        with tarfile.open(archive_path, "r:") as archive:
            entry = archive.getmember("runtime-manifest.json")
            if not entry.isfile() or not (1 <= entry.size <= 2_000_000):
                fail("runtime manifest tar member is not a bounded regular file")
            stream = archive.extractfile(entry)
            if stream is None:
                fail("runtime manifest tar member cannot be read")
            raw = stream.read(2_000_001)
    except (tarfile.TarError, EOFError) as error:
        raise OverlayError("accepted runtime archive is invalid") from error
    if hashlib.sha256(raw).hexdigest() != runtime["component_manifest_sha256"]:
        fail("runtime manifest does not match the verified immutable Release Set")
    try:
        document = json.loads(raw)
    except (ValueError, UnicodeDecodeError) as error:
        raise OverlayError("accepted runtime manifest is malformed") from error
    if (
        not isinstance(document, dict)
        or document.get("schema_version") != 3
        or document.get("kind") != "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT"
        or document.get("release_id") != runtime["release_id"]
    ):
        fail("accepted runtime manifest identity is inconsistent")
    files = document.get("files")
    entries = files.get("files") if isinstance(files, dict) else None
    if not isinstance(entries, list) or not (1 <= len(entries) <= 10000):
        fail("accepted runtime file inventory is missing or unbounded")
    # Match Profile Bridge's runtime_bundle::inventory_digest exactly:
    # sorted entries, length/path/sha256 keys in this order, trailing newline.
    normalized = []
    seen = set()
    for entry in entries:
        if not isinstance(entry, dict):
            fail("runtime inventory entry is malformed")
        path = entry.get("path")
        size = entry.get("size_bytes")
        digest = entry.get("sha256")
        if (
            not isinstance(path, str)
            or not path
            or len(path) > 512
            or path.startswith("/")
            or "\\" in path
            or any(part in ("", ".", "..") for part in path.split("/"))
            or not path.isascii()
            or path.casefold() in seen
            or type(size) is not int
            or not (0 <= size <= 2**63 - 1)
            or not isinstance(digest, str)
            or SHA256_RE.fullmatch(digest) is None
        ):
            fail("runtime inventory contains an unsafe or invalid file identity")
        seen.add(path.casefold())
        normalized.append({"length": size, "path": path, "sha256": digest})
    normalized.sort(key=lambda item: item["path"])
    canonical = json.dumps(normalized, separators=(",", ":"), ensure_ascii=True) + "\n"
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def control_manifest(
    document: dict[str, Any], environment: str, *,
    runtime_digest: str | None = None, previous_runtime_digest: str | None = None,
) -> dict[str, str]:
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
    # Only the two cryptographically identified Release Set runtime inventories
    # can be trusted during a staged predecessor -> successor transition.
    pinned = [runtime_digest] if runtime_digest is not None else []
    if previous_runtime_digest is not None and previous_runtime_digest != runtime_digest:
        pinned.append(previous_runtime_digest)
    expected = ",".join(pinned) if pinned else None
    allowed = candidate.get("approved_runtime_inventories", expected)
    if not isinstance(allowed, str):
        fail("runtime trust requires verified immutable Release Set inventories")
    result["approved_runtime_inventories"] = bounded(allowed, "control_plane.approved_runtime_inventories")
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
    if previous_runtime_digest is not None:
        if runtime_digest is None or set(allowed.split(",")) != set(pinned):
            fail("protected runtime policy differs from the two verified Release Set inventories")
        result["approved_runtime_inventories"] = expected
    elif runtime_digest is not None and runtime_digest not in allowed.split(","):
        fail("protected runtime policy excludes the exact accepted runtime Release Set")
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
    runtime_release_set: Path,
    runtime_archive: Path,
    previous_runtime_release_set: Path | None = None,
    previous_runtime_archive: Path | None = None,
    expected_previous_release_set_id: str | None = None,
    ready_policy_evidence: Path | None = None,
    environment: str,
    output: Path,
) -> None:
    config = load(source_config, "immutable release Wrangler config")
    previous_args = (previous_runtime_release_set, previous_runtime_archive, expected_previous_release_set_id)
    if any(item is not None for item in previous_args) and not all(item is not None for item in previous_args):
        if ready_policy_evidence is None or expected_previous_release_set_id is None or any(
            item is not None for item in previous_args[:2]
        ):
            fail("known-good runtime source must be an exact release trio or sealed READY evidence")
    if ready_policy_evidence is not None and any(item is not None for item in previous_args[:2]):
        fail("sealed READY evidence and known-good runtime materialization are mutually exclusive")
    if expected_previous_release_set_id == os.environ.get("RELEASE_SET_ID"):
        fail("known-good runtime must refer to a different immutable Release Set")
    current_digest = verified_runtime_inventory(runtime_release_set, runtime_archive)
    previous_digest = None
    if all(item is not None for item in previous_args):
        previous_digest = verified_runtime_inventory(
            previous_runtime_release_set, previous_runtime_archive, expected_previous_release_set_id,
        )
    if ready_policy_evidence is not None:
        if expected_previous_release_set_id is None:
            fail("sealed READY evidence requires the exact expected previous Release Set")
        if ready_policy_evidence.is_symlink() or not ready_policy_evidence.is_file():
            fail("sealed READY runtime policy evidence must be a regular file")
        raw = ready_policy_evidence.read_bytes()
        sealed_sha = os.environ.get("READY_EVIDENCE_SHA256", "")
        if SHA256_RE.fullmatch(sealed_sha) is None or hashlib.sha256(raw).hexdigest() != sealed_sha:
            fail("READY runtime policy evidence is not bound to the one-shot authorization")
        evidence = load(ready_policy_evidence, "sealed READY runtime policy")
        policy = evidence.get("approved_runtime_inventories")
        if (
            evidence.get("kind") != "AR11_READY_TO_MUTATE"
            or evidence.get("ready") is not True
            or evidence.get("source_sha") != os.environ.get("GITHUB_SHA")
            or evidence.get("release_set_id") != os.environ.get("RELEASE_SET_ID")
            or evidence.get("expected_current_release_set_id") != expected_previous_release_set_id
            or not isinstance(policy, str)
            or APPROVED_RUNTIME_INVENTORIES_RE.fullmatch(policy) is None
            or not policy.startswith(current_digest)
            or policy.split(",")[0] != current_digest
            or evidence.get("runtime_policy_sha256") != hashlib.sha256(policy.encode("utf-8")).hexdigest()
        ):
            fail("sealed READY policy differs from exact approved source or target runtime")
        previous_digest = policy.split(",")[-1]
    manifest = control_manifest(
        load(deploy_manifest, "deploy manifest"), environment,
        runtime_digest=current_digest, previous_runtime_digest=previous_digest,
    )
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
        # Model only the two immutable component identities, not a fake runtime
        # trust value passed by a client or staging environment.
        runtime_release_id = "runtime-bundle-v3-sha256-" + "e" * 64
        fixture_files = [{"path": "browser/camoufox.exe", "size_bytes": 3, "sha256": "c" * 64}]
        runtime_document = {
            "schema_version": 3, "kind": "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT",
            "release_id": runtime_release_id, "files": {"files": fixture_files},
        }
        runtime_bytes = json.dumps(runtime_document, separators=(",", ":")).encode()
        runtime_archive = root / "runtime.tar"
        with tarfile.open(runtime_archive, "w") as archive:
            member = tarfile.TarInfo("runtime-manifest.json")
            member.size = len(runtime_bytes)
            archive.addfile(member, io.BytesIO(runtime_bytes))
        runtime_release_set = root / "release-set.json"
        runtime_release_set.write_text(json.dumps({
            "schema_version": 3, "release_set_id": release_set_id,
            "components": {"runtime_bundle": {
                "component_id": "runtime_bundle", "artifact_path": "components/runtime-bundle.tar",
                "component_manifest_sha256": hashlib.sha256(runtime_bytes).hexdigest(),
                "release_id": runtime_release_id,
            }},
        }), encoding="utf-8")
        previous_release_id = "release-set-v3-sha256-" + "7" * 64
        previous_component_id = "runtime-bundle-v3-sha256-" + "8" * 64
        previous_document = {
            "schema_version": 3, "kind": "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT",
            "release_id": previous_component_id,
            "files": {"files": [{"path": "browser/camoufox.exe", "size_bytes": 5, "sha256": "f" * 64}]},
        }
        previous_bytes = json.dumps(previous_document, separators=(",", ":")).encode()
        previous_archive = root / "previous-runtime.tar"
        with tarfile.open(previous_archive, "w") as archive:
            member = tarfile.TarInfo("runtime-manifest.json")
            member.size = len(previous_bytes)
            archive.addfile(member, io.BytesIO(previous_bytes))
        previous_release_set = root / "previous-release-set.json"
        previous_release_set.write_text(json.dumps({
            "schema_version": 3, "release_set_id": previous_release_id,
            "components": {"runtime_bundle": {
                "component_id": "runtime_bundle", "artifact_path": "components/runtime-bundle.tar",
                "component_manifest_sha256": hashlib.sha256(previous_bytes).hexdigest(),
                "release_id": previous_component_id,
            }},
        }), encoding="utf-8")
        manifest_path = root / "manifest.json"
        previous = os.environ.get("RELEASE_SET_ID")
        os.environ["RELEASE_SET_ID"] = release_set_id
        try:
            expected_pin = verified_runtime_inventory(runtime_release_set, runtime_archive)
            old_pin = verified_runtime_inventory(previous_release_set, previous_archive, previous_release_id)
            if old_pin == expected_pin:
                fail("runtime compatibility fixture did not produce two distinct inventories")
            if expected_pin != hashlib.sha256(
                (json.dumps([{"length": 3, "path": "browser/camoufox.exe", "sha256": "c" * 64}],
                            separators=(",", ":")) + "\n").encode()
            ).hexdigest():
                fail("runtime digest differs from native shipping inventory format")
            for index, contact in enumerate([None, "owner@staging.invalid"]):
                candidate = dict(manifest)
                candidate.pop("approved_runtime_inventories", None)
                if contact is not None:
                    candidate["initial_owner_email"] = contact
                manifest_path.write_text(json.dumps({"control_plane": candidate}), encoding="utf-8")
                output = root / f"overlay-{index}.json"
                render(source_config=Path(__file__).resolve().parents[1] / "deploy/cloudflare/wrangler.jsonc",
                       release_root=root, deploy_manifest=manifest_path,
                       runtime_release_set=runtime_release_set, runtime_archive=runtime_archive,
                       previous_runtime_release_set=previous_release_set,
                       previous_runtime_archive=previous_archive,
                       expected_previous_release_set_id=previous_release_id,
                       environment="staging", output=output)
                observed = json.loads(output.read_text(encoding="utf-8"))
                if observed["env"]["staging"]["vars"]["INITIAL_OWNER_EMAIL"] != (contact or ""):
                    fail("initial owner configuration was lost during canonical Core rendering")
                if observed["env"]["staging"]["vars"]["PROFILE_GENERATION_APPROVED_RUNTIME_INVENTORIES"] != expected_pin + "," + old_pin:
                    fail("staging WORKING two-release runtime policy was lost during no-rebuild Core rendering")
                if set(observed["env"]) != {"staging"} or "build" in observed:
                    fail("setup configuration expanded the promotion target/build scope")
            mismatched = dict(manifest, approved_runtime_inventories=expected_pin)
            try:
                control_manifest(
                    {"control_plane": mismatched}, "staging",
                    runtime_digest=expected_pin, previous_runtime_digest=old_pin,
                )
            except OverlayError:
                pass
            else:
                fail("protected runtime policy excluding the verified previous runtime passed")
            mismatched["approved_runtime_inventories"] = expected_pin + "," + "b" * 64
            try:
                control_manifest(
                    {"control_plane": mismatched}, "staging",
                    runtime_digest=expected_pin, previous_runtime_digest=old_pin,
                )
            except OverlayError:
                pass
            else:
                fail("unverified third-party runtime pin unexpectedly passed")
            # Exercise the actual manual/post-deploy path with an independently
            # hashed READY proof rather than fetching an older runtime in mutate.
            verified_policy = expected_pin + "," + old_pin
            ready_value = {
                "kind": "AR11_READY_TO_MUTATE", "ready": True,
                "source_sha": "1" * 40, "release_set_id": release_set_id,
                "expected_current_release_set_id": previous_release_id,
                "approved_runtime_inventories": verified_policy,
                "runtime_policy_sha256": hashlib.sha256(verified_policy.encode()).hexdigest(),
            }
            ready_file = root / "ready-to-mutate.json"
            ready_file.write_text(json.dumps(ready_value), encoding="utf-8")
            old_ready_sha = os.environ.get("READY_EVIDENCE_SHA256")
            old_git_sha = os.environ.get("GITHUB_SHA")
            os.environ["READY_EVIDENCE_SHA256"] = hashlib.sha256(ready_file.read_bytes()).hexdigest()
            os.environ["GITHUB_SHA"] = "1" * 40
            try:
                manifest_path.write_text(json.dumps({"control_plane": dict(manifest, approved_runtime_inventories=verified_policy)}), encoding="utf-8")
                proof_output = root / "overlay-ready.json"
                render(source_config=Path(__file__).resolve().parents[1] / "deploy/cloudflare/wrangler.jsonc",
                       release_root=root, deploy_manifest=manifest_path,
                       runtime_release_set=runtime_release_set, runtime_archive=runtime_archive,
                       ready_policy_evidence=ready_file, expected_previous_release_set_id=previous_release_id,
                       environment="staging", output=proof_output)
                rendered = json.loads(proof_output.read_text(encoding="utf-8"))
                if rendered["env"]["staging"]["vars"]["PROFILE_GENERATION_APPROVED_RUNTIME_INVENTORIES"] != verified_policy:
                    fail("verified READY runtime policy was not reproduced at mutation boundary")
                os.environ["READY_EVIDENCE_SHA256"] = "0" * 64
                try:
                    render(source_config=Path(__file__).resolve().parents[1] / "deploy/cloudflare/wrangler.jsonc",
                           release_root=root, deploy_manifest=manifest_path,
                           runtime_release_set=runtime_release_set, runtime_archive=runtime_archive,
                           ready_policy_evidence=ready_file, expected_previous_release_set_id=previous_release_id,
                           environment="staging", output=root / "overlay-unauthorized.json")
                except OverlayError:
                    pass
                else:
                    fail("unsealed READY runtime proof unexpectedly authorized")
            finally:
                if old_ready_sha is None:
                    os.environ.pop("READY_EVIDENCE_SHA256", None)
                else:
                    os.environ["READY_EVIDENCE_SHA256"] = old_ready_sha
                if old_git_sha is None:
                    os.environ.pop("GITHUB_SHA", None)
                else:
                    os.environ["GITHUB_SHA"] = old_git_sha
            permitted = dict(manifest, approved_runtime_inventories=old_pin + "," + expected_pin)
            if control_manifest(
                {"control_plane": permitted}, "staging",
                runtime_digest=expected_pin, previous_runtime_digest=old_pin,
            )["approved_runtime_inventories"] != expected_pin + "," + old_pin:
                fail("valid two-release policy is not canonical")
            try:
                verified_runtime_inventory(previous_release_set, previous_archive, release_set_id)
            except OverlayError:
                pass
            else:
                fail("known-good release identity substitution unexpectedly passed")
            previous_release_set.write_text(previous_release_set.read_text().replace(
                hashlib.sha256(previous_bytes).hexdigest(), "0" * 64,
            ), encoding="utf-8")
            try:
                verified_runtime_inventory(previous_release_set, previous_archive, previous_release_id)
            except OverlayError:
                pass
            else:
                fail("tampered known-good runtime manifest identity passed")
            runtime_release_set.write_text(runtime_release_set.read_text().replace(
                hashlib.sha256(runtime_bytes).hexdigest(), "f" * 64
            ), encoding="utf-8")
            try:
                verified_runtime_inventory(runtime_release_set, runtime_archive)
            except OverlayError:
                pass
            else:
                fail("tampered runtime manifest identity passed immutable verification")

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
    parser.add_argument("--runtime-release-set", type=Path)
    parser.add_argument("--runtime-archive", type=Path)
    parser.add_argument("--previous-runtime-release-set", type=Path)
    parser.add_argument("--previous-runtime-archive", type=Path)
    parser.add_argument("--expected-previous-release-set-id")
    parser.add_argument("--ready-policy-evidence", type=Path)
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
            args.runtime_release_set,
            args.runtime_archive,
            args.output,
        ):
            fail("source-config, release-root, deploy-manifest and output are required")
        render(
            source_config=args.source_config,
            release_root=args.release_root,
            deploy_manifest=args.deploy_manifest,
            runtime_release_set=args.runtime_release_set,
            runtime_archive=args.runtime_archive,
            previous_runtime_release_set=args.previous_runtime_release_set,
            previous_runtime_archive=args.previous_runtime_archive,
            expected_previous_release_set_id=args.expected_previous_release_set_id,
            ready_policy_evidence=args.ready_policy_evidence,
            environment=args.environment,
            output=args.output,
        )
        return 0
    except (OSError, OverlayError) as error:
        print(f"AR-11 Core overlay error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
