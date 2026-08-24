#!/usr/bin/env python3
"""Validate the single current credential composition root.

The accepted AR-8B credential registry is immutable provenance. Current additive
metadata is supplied only through explicitly named overlays owned by
architecture/credential-authority.json; overlays never contain secret values.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
CURRENT_AUTHORITY = "architecture/credential-authority.json"
EXPECTED_REGISTRY = "architecture/credential-authority-ar8b.json"
EXPECTED_REGISTRY_OVERLAY = "architecture/credential-authority-ar11.json"
EXPECTED_LIFECYCLE = "architecture/credential-lifecycle.json"
EXPECTED_PROFILE_SECURITY = "architecture/profile-security.json"
EXPECTED_SECRET_TRANSPORT_SUCCESSOR = "architecture/ar8-d-secret-transport-successor.json"

CANONICAL_ENVIRONMENTS = {"rehearsal", "staging", "production"}
REQUIRED_FIELDS = {
    "id", "class", "provider_system", "environment_scope", "owner", "consumers",
    "bindings", "protected_value_authority", "legitimate_mutable_authority",
    "version_state_source", "automation_class", "externally_issued",
    "rotation_recovery_policy", "future_cutover",
}
FORBIDDEN_VALUE_FIELDS = {
    "value", "secret_value", "plaintext", "plaintext_value", "private_key",
    "password", "token", "token_value", "credential_value", "key_material",
    "raw_secret", "raw_token",
}
MATERIAL_PATTERNS = (
    re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
)
WORKFLOW_SECRET = re.compile(r"\bsecrets\.([A-Z][A-Z0-9_]*)\b")
WRANGLER_REQUIRED = re.compile(r'"required"\s*:\s*\[(.*?)\]', re.DOTALL)
QUOTED_IDENTIFIER = re.compile(r'"([A-Z][A-Z0-9_]*)"')
RUST_WORKER_SECRET = re.compile(r'\.secret\(\s*"([A-Z][A-Z0-9_]*)"\s*\)')
CREDENTIAL_NAME = r"([A-Z][A-Z0-9_]*(?:SECRET|TOKEN|PASSWORD|PRIVATE_KEY|API_KEY|AUTH_KEY|KEYRING)[A-Z0-9_]*)"
PY_ENV_PATTERNS = (
    re.compile(rf"os\.environ\[\s*[\"']{CREDENTIAL_NAME}[\"']\s*\]"),
    re.compile(rf"os\.environ\.get\(\s*[\"']{CREDENTIAL_NAME}[\"']"),
    re.compile(rf"os\.getenv\(\s*[\"']{CREDENTIAL_NAME}[\"']"),
)
JS_ENV_LOOKUP = re.compile(rf"(?:process\.env\.|env\.){CREDENTIAL_NAME}")
ENVIRONMENT_BOUND_SURFACES = {"github_environment_secret", "cloudflare_worker_secret"}
SCAN_EXCLUSIONS = {"scripts/check-tracked-secrets.sh"}
LEGACY_BUNDLE_OWNERS = {
    "CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON": "cloudflare.control-plane-secret-bundle-transport",
    "CLOUDFLARE_RESOLVER_SECRETS_JSON": "cloudflare.resolver-secret-bundle-transport",
}


@dataclass(frozen=True)
class State:
    composition: dict[str, Any]
    registry: dict[str, Any]
    overlay: dict[str, Any]
    lifecycle: dict[str, Any]
    successor: dict[str, Any]
    detected: dict[str, set[str]]


def read_json(root: Path, relative: str) -> dict[str, Any]:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"credential authority source missing/not regular: {relative}")
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"credential authority source must be one object: {relative}")
    return value


def repo_path(value: object, field: str) -> str:
    if not isinstance(value, str) or not value or value.startswith(("/", "\\")):
        raise ValueError(f"{field} must be a repository-relative path")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"{field} must stay inside the repository")
    return path.as_posix()


def validate_composition(root: Path, value: dict[str, Any]) -> tuple[str, str, str]:
    if (
        value.get("schema_version") != 1
        or value.get("kind") != "CURRENT_CREDENTIAL_AUTHORITY"
        or value.get("status") != "current"
    ):
        raise ValueError("current credential authority identity/version/status drifted")
    if value.get("canonical_inventory") != "architecture/inventory.json":
        raise ValueError("current credential authority must project through architecture/inventory.json")
    sources = {
        "registry_source": EXPECTED_REGISTRY,
        "credential_lifecycle_source": EXPECTED_LIFECYCLE,
        "profile_security_source": EXPECTED_PROFILE_SECURITY,
    }
    for field, expected in sources.items():
        if repo_path(value.get(field), field) != expected:
            raise ValueError(f"current credential authority source ownership drifted: {field}")
    if "operator_contract_source" in value:
        raise ValueError("retired operator-contract predecessor may not remain a credential composition source")
    if value.get("registry_source_role") != "IMMUTABLE_ACCEPTED_PROVENANCE_DATASET":
        raise ValueError("accepted credential registry provenance role drifted")
    overlays = value.get("registry_overlay_sources")
    if not isinstance(overlays, list) or overlays != [EXPECTED_REGISTRY_OVERLAY]:
        raise ValueError("current credential registry overlay set drifted")
    if value.get("registry_overlay_role") != "CURRENT_ADDITIVE_METADATA_AUTHORITY":
        raise ValueError("current credential registry overlay role drifted")
    overlay_path = repo_path(overlays[0], "registry_overlay_sources[0]")
    provenance = value.get("historical_provenance")
    if (
        not isinstance(provenance, dict)
        or provenance.get("accepted_ar8b_authority") != EXPECTED_REGISTRY
        or provenance.get("accepted_ar8b_status") != "ACCEPTED_AR8B_CREDENTIAL_METADATA_AUTHORITY"
        or provenance.get("accepted_ar8b_must_not_be_rewritten") is not True
    ):
        raise ValueError("accepted AR-8B registry provenance contract drifted")
    invariants = value.get("invariants")
    expected_invariants = {
        "canonical_composition_roots": 1,
        "plaintext_secret_values": "FORBIDDEN",
        "competing_mutable_authority": "FORBIDDEN",
        "routine_application_release_rotates_credentials": False,
        "application_deployment_and_credential_rotation_separated": True,
        "accepted_registry_is_immutable_base": True,
        "production_mutation_from_architecture_tooling": False,
        "operator_secret_readback": False,
    }
    if not isinstance(invariants, dict) or any(
        invariants.get(key) != expected for key, expected in expected_invariants.items()
    ):
        raise ValueError("current credential authority fail-closed invariants drifted")
    profile = read_json(root, EXPECTED_PROFILE_SECURITY)
    if profile.get("kind") != "PROFILE_SECURITY_AUTHORITY" or profile.get("status") != "current":
        raise ValueError("current profile-security authority identity/status drifted")
    return EXPECTED_REGISTRY, overlay_path, EXPECTED_LIFECYCLE


def validate_lifecycle(value: dict[str, Any]) -> None:
    if (
        value.get("schema_version") != 1
        or value.get("kind") != "CREDENTIAL_LIFECYCLE_AUTHORITY"
        or value.get("status") != "current"
    ):
        raise ValueError("credential lifecycle identity/version/status drifted")
    if value.get("credential_authority") != CURRENT_AUTHORITY:
        raise ValueError("credential lifecycle must point to current credential authority")
    for field, expected in {
        "production_mutation": False,
        "secret_plaintext_in_git": False,
        "routine_release_rotates_runtime_secrets": False,
        "routine_release_secret_transport": False,
    }.items():
        if value.get(field) != expected:
            raise ValueError(f"credential lifecycle fail-closed invariant drifted: {field}")
    invariants = value.get("global_invariants")
    required = {
        "verify_replacement_before_retire_previous": True,
        "environment_binding_explicit": True,
        "one_legitimate_mutable_authority_per_concern": True,
        "secret_readback": False,
        "routine_deployment_is_not_rotation_authority": True,
        "legacy_bundle_transport_is_steady_state_authority": False,
    }
    if not isinstance(invariants, dict) or any(
        invariants.get(key) != expected for key, expected in required.items()
    ):
        raise ValueError("credential lifecycle global invariants drifted")
    if set(invariants.get("legacy_bundle_bindings", [])) != set(LEGACY_BUNDLE_OWNERS):
        raise ValueError("credential lifecycle legacy bundle set drifted")


def validate_secret_transport_successor(value: dict[str, Any]) -> None:
    if (
        value.get("schema_version") != 1
        or value.get("kind") != "POLICY_TRANSITION"
        or value.get("status") != "candidate"
        or value.get("tracking_issue") != 361
        or value.get("parent_issue") != 308
        or value.get("canonical_inventory") != "architecture/inventory.json"
    ):
        raise ValueError("AR-8D secret-transport successor provenance drifted")
    successor = value.get("successor")
    if not isinstance(successor, dict):
        raise ValueError("AR-8D secret-transport successor metadata is missing")
    if successor.get("superseded_routine_deploy_bindings") != sorted(LEGACY_BUNDLE_OWNERS):
        raise ValueError("AR-8D successor legacy bundle set drifted")
    if (
        successor.get("routine_deploy_secret_value_transport") is not False
        or successor.get("routine_deploy_secret_mutation") is not False
        or successor.get("rotation_lifecycle") != "separate_explicit_rotation_authority"
    ):
        raise ValueError("AR-8D successor no longer separates deployment from credential rotation")


def walk(value: Any, path: str = "$") -> Iterable[tuple[str, str, Any]]:
    if isinstance(value, dict):
        for key, nested in value.items():
            yield path, str(key), nested
            yield from walk(nested, f"{path}.{key}")
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            yield from walk(nested, f"{path}[{index}]")


def validate_overlay(value: dict[str, Any]) -> None:
    if (
        value.get("schema_version") != 1
        or value.get("kind") != "CURRENT_CREDENTIAL_REGISTRY_OVERLAY"
        or value.get("status") != "current"
        or value.get("parent_issue") != 399
        or value.get("base_registry") != EXPECTED_REGISTRY
        or value.get("metadata_only") is not True
        or value.get("production_mutation") is not False
    ):
        raise ValueError("current credential registry overlay provenance/invariants drifted")
    for section in ("credentials", "dynamic_credential_domains", "future_trust_domains"):
        raw = value.get(section)
        if not isinstance(raw, list) or any(not isinstance(item, dict) for item in raw):
            raise ValueError(f"overlay {section} must be a list of objects")
    for location, key, nested in walk(value):
        if key.lower() in FORBIDDEN_VALUE_FIELDS:
            raise ValueError(f"forbidden value-bearing field {location}.{key}")
        if isinstance(nested, str) and any(pattern.search(nested) for pattern in MATERIAL_PATTERNS):
            raise ValueError(f"high-confidence credential material found at {location}.{key}")
    credentials = value["credentials"]
    if len(credentials) != 1:
        raise ValueError("AR-11 credential overlay must own exactly the read-only observation credential")
    observation = credentials[0]
    if (
        observation.get("id") != "cloudflare.release-observation-api"
        or observation.get("owner") != "release-set-promotion-observation"
        or observation.get("provider_capability") != "READ_ONLY_METADATA_OBSERVATION"
        or observation.get("automation_class") != "READ_ONLY_PROVIDER_OBSERVATION"
    ):
        raise ValueError("AR-11 observation credential least-privilege identity drifted")
    bindings = observation.get("bindings")
    if not isinstance(bindings, list) or len(bindings) != 1 or bindings[0].get("name") != "CLOUDFLARE_OBSERVE_API_TOKEN":
        raise ValueError("AR-11 observation credential binding drifted")
    if observation.get("environment_scope") != {"kind": "environment", "environments": ["staging"]}:
        raise ValueError("AR-11 observation credential must remain staging-only")


def compose_registry(base: dict[str, Any], overlay: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(base)
    for section in ("credentials", "dynamic_credential_domains", "future_trust_domains"):
        base_items = result.get(section)
        overlay_items = overlay.get(section)
        if not isinstance(base_items, list) or not isinstance(overlay_items, list):
            raise ValueError(f"credential registry section must be a list: {section}")
        base_items.extend(copy.deepcopy(overlay_items))
    return result


def tracked_files(root: Path) -> list[Path]:
    result = subprocess.run(["git", "ls-files", "-z"], cwd=root, capture_output=True, check=False)
    if result.returncode != 0:
        raise ValueError("git ls-files failed while discovering credential surfaces")
    return [root / raw.decode("utf-8") for raw in result.stdout.split(b"\0") if raw]


def text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return ""


def scan_material(root: Path, files: list[Path]) -> None:
    for path in files:
        relative = path.relative_to(root).as_posix()
        source = text(path)
        if relative not in SCAN_EXCLUSIONS and source and any(
            pattern.search(source) for pattern in MATERIAL_PATTERNS
        ):
            raise ValueError(f"high-confidence credential material found in tracked file: {relative}")


def discover(root: Path, files: list[Path]) -> dict[str, set[str]]:
    found: dict[str, set[str]] = {}

    def add(name: str, path: Path) -> None:
        found.setdefault(name, set()).add(path.relative_to(root).as_posix())

    for path in files:
        relative, source = path.relative_to(root).as_posix(), text(path)
        if not source:
            continue
        if relative.startswith(".github/workflows/") and path.suffix in {".yml", ".yaml"}:
            for name in WORKFLOW_SECRET.findall(source):
                add(name, path)
        if relative.startswith("deploy/cloudflare/") and path.suffix in {".json", ".jsonc"}:
            for block in WRANGLER_REQUIRED.findall(source):
                for name in QUOTED_IDENTIFIER.findall(block):
                    add(name, path)
        if relative.startswith(("apps/", "crates/")) and path.suffix == ".rs":
            for name in RUST_WORKER_SECRET.findall(source):
                add(name, path)
        if relative.startswith(("scripts/", "tools/")) and path.suffix == ".py":
            for pattern in PY_ENV_PATTERNS:
                for name in pattern.findall(source):
                    add(name, path)
        if relative.startswith(("scripts/", "tools/", ".github/")) and path.suffix in {".js", ".mjs", ".cjs", ".ts"}:
            for name in JS_ENV_LOOKUP.findall(source):
                add(name, path)
    return found


def entries(value: dict[str, Any]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for section in ("credentials", "dynamic_credential_domains", "future_trust_domains"):
        raw = value.get(section)
        if not isinstance(raw, list) or any(not isinstance(item, dict) for item in raw):
            raise ValueError(f"{section} must be a list of objects")
        result.extend(raw)
    return result


def validate_registry(
    value: dict[str, Any],
    detected: dict[str, set[str]],
    lifecycle: dict[str, Any],
    successor: dict[str, Any] | None = None,
) -> None:
    if value.get("schema_version") != 1 or value.get("status") != "ACCEPTED_AR8B_CREDENTIAL_METADATA_AUTHORITY":
        raise ValueError("accepted credential registry identity/version drifted")
    if (
        value.get("parent_issue") != 308
        or value.get("implementation_issue") != 309
        or value.get("canonical_inventory") != "architecture/inventory.json"
        or value.get("metadata_only") is not True
    ):
        raise ValueError("accepted credential registry provenance drifted")
    if set(value.get("canonical_environments", [])) != CANONICAL_ENVIRONMENTS:
        raise ValueError("credential registry environment vocabulary drifted")
    invariants = value.get("invariants")
    if (
        not isinstance(invariants, dict)
        or invariants.get("plaintext_in_git") != "FORBIDDEN"
        or invariants.get("competing_registry") != "FORBIDDEN"
        or invariants.get("mutable_authorities_per_concern") != 1
        or invariants.get("production_mutation") is not False
        or invariants.get("ar9_blocked") is not True
    ):
        raise ValueError("accepted AR-8B credential invariants drifted")
    for location, key, nested in walk(value):
        if key.lower() in FORBIDDEN_VALUE_FIELDS:
            raise ValueError(f"forbidden value-bearing field {location}.{key}")
        if isinstance(nested, str) and any(pattern.search(nested) for pattern in MATERIAL_PATTERNS):
            raise ValueError(f"high-confidence credential material found at {location}.{key}")
    ids: set[str] = set()
    owners: dict[str, str] = {}
    declaration_only: set[str] = set()
    for item in entries(value):
        logical_id = item.get("id", "<missing-id>")
        missing = REQUIRED_FIELDS - set(item)
        if missing:
            raise ValueError(f"{logical_id}: missing required fields {sorted(missing)!r}")
        if not isinstance(logical_id, str) or not logical_id or logical_id in ids:
            raise ValueError(f"invalid/duplicate credential authority id: {logical_id!r}")
        ids.add(logical_id)
        if not isinstance(item.get("externally_issued"), bool) or not isinstance(item.get("consumers"), list):
            raise ValueError(f"{logical_id}: malformed externally_issued/consumers")
        scope = item.get("environment_scope")
        if not isinstance(scope, dict):
            raise ValueError(f"{logical_id}: environment_scope must be an object")
        kind, environments = scope.get("kind"), scope.get("environments", [])
        if kind not in {"repository", "environment", "tenant_dynamic", "release"}:
            raise ValueError(f"{logical_id}: unknown environment scope")
        if kind == "environment":
            if (
                not isinstance(environments, list)
                or not environments
                or set(environments) - CANONICAL_ENVIRONMENTS
            ):
                raise ValueError(f"{logical_id}: invalid canonical environment scope")
        elif environments:
            raise ValueError(f"{logical_id}: environments allowed only for kind=environment")
        for field in (
            "class", "provider_system", "owner", "protected_value_authority",
            "legitimate_mutable_authority", "version_state_source", "automation_class",
            "rotation_recovery_policy", "future_cutover",
        ):
            if not isinstance(item.get(field), str) or not item[field].strip():
                raise ValueError(f"{logical_id}: {field} must be non-empty")
        bindings = item.get("bindings")
        if not isinstance(bindings, list):
            raise ValueError(f"{logical_id}: bindings must be a list")
        seen: set[tuple[str, str, str]] = set()
        for binding in bindings:
            if not isinstance(binding, dict):
                raise ValueError(f"{logical_id}: binding must be an object")
            name, surface = binding.get("name"), binding.get("surface")
            consumer = str(binding.get("consumer", ""))
            if not isinstance(name, str) or not name or not isinstance(surface, str) or not surface:
                raise ValueError(f"{logical_id}: binding name/surface must be non-empty")
            identity = (surface, name, consumer)
            if identity in seen:
                raise ValueError(f"{logical_id}: duplicate binding tuple {identity!r}")
            seen.add(identity)
            if surface in ENVIRONMENT_BOUND_SURFACES:
                binding_envs = binding.get("environments")
                if (
                    kind != "environment"
                    or not isinstance(binding_envs, list)
                    or set(binding_envs) != set(environments)
                ):
                    raise ValueError(f"{logical_id}/{name}: binding/environment scope mismatch")
            elif binding.get("environments"):
                raise ValueError(f"{logical_id}/{name}: non-environment binding declares environments")
            previous = owners.get(name)
            if previous is not None and previous != logical_id:
                raise ValueError(f"binding {name} belongs to multiple authorities: {previous}, {logical_id}")
            owners[name] = logical_id
            if binding.get("declaration_only") is True:
                declaration_only.add(name)
    validate_lifecycle(lifecycle)
    if successor is not None:
        validate_secret_transport_successor(successor)
    for name, owner in LEGACY_BUNDLE_OWNERS.items():
        if owners.get(name) != owner:
            raise ValueError(f"legacy bundle binding {name} lost canonical owner {owner}")
    missing = sorted(set(detected) - set(owners))
    if missing:
        raise ValueError("tracked credential bindings missing canonical authority: " + ", ".join(missing))
    stale = sorted(set(owners) - set(detected) - declaration_only - set(LEGACY_BUNDLE_OWNERS))
    if stale:
        raise ValueError("authority has stale non-declaration bindings: " + ", ".join(stale))


def validate_repository(root: Path = ROOT) -> State:
    root = root.resolve()
    composition = read_json(root, CURRENT_AUTHORITY)
    registry_path, overlay_path, lifecycle_path = validate_composition(root, composition)
    lifecycle = read_json(root, lifecycle_path)
    validate_lifecycle(lifecycle)
    successor = read_json(root, EXPECTED_SECRET_TRANSPORT_SUCCESSOR)
    validate_secret_transport_successor(successor)
    accepted_registry = read_json(root, registry_path)
    overlay = read_json(root, overlay_path)
    validate_overlay(overlay)
    registry = compose_registry(accepted_registry, overlay)
    files = tracked_files(root)
    scan_material(root, files)
    detected = discover(root, files)
    validate_registry(registry, detected, lifecycle, successor)
    return State(composition, registry, overlay, lifecycle, successor, detected)


def negative_self_test(state: State, root: Path = ROOT) -> None:
    def reject(
        label: str,
        registry: dict[str, Any],
        detected: dict[str, set[str]] | None = None,
        lifecycle: dict[str, Any] | None = None,
        successor: dict[str, Any] | None = None,
    ) -> None:
        try:
            validate_registry(
                registry,
                state.detected if detected is None else detected,
                state.lifecycle if lifecycle is None else lifecycle,
                state.successor if successor is None else successor,
            )
        except ValueError:
            return
        raise AssertionError(f"negative fixture unexpectedly passed: {label}")

    bad_root = copy.deepcopy(state.composition)
    bad_root["status"] = "historical"
    try:
        validate_composition(root, bad_root)
    except ValueError:
        pass
    else:
        raise AssertionError("current-authority status fixture unexpectedly passed")

    bad_overlay = copy.deepcopy(state.overlay)
    bad_overlay["credentials"][0]["provider_capability"] = "DEPLOY"
    try:
        validate_overlay(bad_overlay)
    except ValueError:
        pass
    else:
        raise AssertionError("observation credential mutation-capability fixture unexpectedly passed")

    bad_lifecycle = copy.deepcopy(state.lifecycle)
    bad_lifecycle["routine_release_secret_transport"] = True
    try:
        validate_lifecycle(bad_lifecycle)
    except ValueError:
        pass
    else:
        raise AssertionError("routine-release secret transport fixture unexpectedly passed")

    bad_successor = copy.deepcopy(state.successor)
    bad_successor["successor"]["routine_deploy_secret_mutation"] = True
    try:
        validate_secret_transport_successor(bad_successor)
    except ValueError:
        pass
    else:
        raise AssertionError("routine-deploy secret mutation successor fixture unexpectedly passed")

    wrong_successor_issue = copy.deepcopy(state.successor)
    wrong_successor_issue["tracking_issue"] = 999
    try:
        validate_secret_transport_successor(wrong_successor_issue)
    except ValueError:
        pass
    else:
        raise AssertionError("AR-8D successor provenance fixture unexpectedly passed")

    plaintext = copy.deepcopy(state.registry)
    plaintext["credentials"][0]["value"] = "forbidden"
    reject("value field", plaintext)
    material = copy.deepcopy(state.registry)
    material["credentials"][0]["rotation_recovery_policy"] = "github_pat_" + "A" * 20
    reject("secret material", material)
    env_index = next(
        (
            i
            for i, item in enumerate(state.registry["credentials"])
            if item.get("environment_scope", {}).get("kind") == "environment"
        ),
        None,
    )
    if env_index is None:
        raise AssertionError("self-test requires environment credential")
    wrong_env = copy.deepcopy(state.registry)
    wrong_env["credentials"][env_index]["environment_scope"]["environments"] = ["prod"]
    reject("unknown environment", wrong_env)
    missing = copy.deepcopy(state.registry)
    missing["credentials"][0].pop("rotation_recovery_policy")
    reject("required field", missing)
    duplicate = copy.deepcopy(state.registry)
    duplicate["credentials"].append(copy.deepcopy(duplicate["credentials"][0]))
    reject("duplicate authority", duplicate)
    sources = [i for i, item in enumerate(state.registry["credentials"]) if item.get("bindings")]
    if len(sources) < 2:
        raise AssertionError("self-test requires two bound credentials")
    dual = copy.deepcopy(state.registry)
    dual["credentials"][sources[1]]["bindings"].append(
        copy.deepcopy(dual["credentials"][sources[0]]["bindings"][0])
    )
    reject("dual binding owner", dual)
    binding_index = next(
        (
            i
            for i, binding in enumerate(state.registry["credentials"][env_index]["bindings"])
            if binding.get("surface") in ENVIRONMENT_BOUND_SURFACES
        ),
        None,
    )
    if binding_index is None:
        raise AssertionError("self-test requires environment-bound binding")
    scope = copy.deepcopy(state.registry)
    scope["credentials"][env_index]["bindings"][binding_index]["environments"] = ["production"]
    reject("binding scope", scope)
    unknown = copy.deepcopy(state.detected)
    unknown["POST_AR11_UNKNOWN_TRACKED_SECRET"] = {"tests/synthetic.yml"}
    reject("unknown static binding", state.registry, unknown)
    live = next((name for name in sorted(state.detected) if name not in LEGACY_BUNDLE_OWNERS), None)
    if live is None:
        raise AssertionError("self-test requires non-legacy binding")
    stale = copy.deepcopy(state.detected)
    stale.pop(live)
    reject("stale registry binding", state.registry, stale)
    legacy = copy.deepcopy(state.lifecycle)
    legacy["global_invariants"]["legacy_bundle_bindings"] = []
    reject("legacy bundle lifecycle", state.registry, lifecycle=legacy)
    successor_bindings = copy.deepcopy(state.successor)
    successor_bindings["successor"]["superseded_routine_deploy_bindings"] = []
    reject("legacy bundle successor set", state.registry, successor=successor_bindings)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    state = validate_repository(root)
    if args.self_test:
        negative_self_test(state, root)
    suffix = " and fail-closed negative fixtures" if args.self_test else ""
    print(
        f"Current credential authority validates {len(state.detected)} tracked static bindings{suffix}; "
        "immutable AR-8B provenance plus current additive overlay composed; AR-8D successor semantics preserved."
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        AssertionError,
        KeyError,
        TypeError,
        ValueError,
        UnicodeDecodeError,
        json.JSONDecodeError,
    ) as error:
        print(f"credential authority verification failed: {error}")
        raise SystemExit(1) from error
