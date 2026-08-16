#!/usr/bin/env python3
"""Validate AR-8B credential metadata against repository credential-binding surfaces.

This checker is deliberately metadata-only.  It never reads hosted secret values and it
rejects value-bearing authority fields.  Static credential names are discovered from
tracked GitHub Actions, Wrangler required-secret declarations and Worker env.secret()
lookups, then reconciled against one logical authority entry.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY_PATH = Path("architecture/credential-authority-ar8b.json")
CANONICAL_INVENTORY = "architecture/inventory.json"
CANONICAL_ENVIRONMENTS = {"rehearsal", "staging", "production"}

REQUIRED_ENTRY_FIELDS = {
    "id",
    "class",
    "provider_system",
    "environment_scope",
    "owner",
    "consumers",
    "bindings",
    "protected_value_authority",
    "legitimate_mutable_authority",
    "version_state_source",
    "automation_class",
    "externally_issued",
    "rotation_recovery_policy",
    "future_cutover",
}
FORBIDDEN_VALUE_FIELDS = {
    "value",
    "secret_value",
    "plaintext",
    "plaintext_value",
    "private_key",
    "password",
    "token",
    "token_value",
    "credential_value",
    "key_material",
    "raw_secret",
    "raw_token",
}
HIGH_CONFIDENCE_VALUE_PATTERNS = (
    re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
)
WORKFLOW_SECRET = re.compile(r"\bsecrets\.([A-Z][A-Z0-9_]*)\b")
WRANGLER_REQUIRED = re.compile(r'"required"\s*:\s*\[(.*?)\]', re.DOTALL)
QUOTED_IDENTIFIER = re.compile(r'"([A-Z][A-Z0-9_]*)"')
RUST_WORKER_SECRET = re.compile(r"\.secret\(\s*\"([A-Z][A-Z0-9_]*)\"\s*\)")
PY_ENV_LOOKUP = re.compile(
    r"(?:os\.environ(?:\.get)?\[?\s*|os\.getenv\(\s*)[\"']([A-Z][A-Z0-9_]*(?:SECRET|TOKEN|PASSWORD|PRIVATE_KEY|API_KEY|AUTH_KEY|KEYRING)[A-Z0-9_]*)[\"']"
)
JS_ENV_LOOKUP = re.compile(
    r"(?:process\.env\.|env\.)(([A-Z][A-Z0-9_]*(?:SECRET|TOKEN|PASSWORD|PRIVATE_KEY|API_KEY|AUTH_KEY|KEYRING)[A-Z0-9_]*))"
)


def _tracked_files(root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        check=True,
        capture_output=True,
    )
    return [root / Path(value.decode("utf-8")) for value in result.stdout.split(b"\0") if value]


def _text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return ""


def discover_static_bindings(root: Path) -> dict[str, set[str]]:
    """Return detected binding name -> source paths.

    The scan intentionally targets credential exposure/binding APIs instead of matching
    arbitrary words such as `token` in documentation.  `github.token` is not a hosted
    secret binding and therefore is not part of this inventory.
    """

    detected: dict[str, set[str]] = {}

    def add(name: str, path: Path) -> None:
        detected.setdefault(name, set()).add(path.relative_to(root).as_posix())

    for path in _tracked_files(root):
        relative = path.relative_to(root).as_posix()
        source = _text(path)
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
            for name in PY_ENV_LOOKUP.findall(source):
                add(name, path)

        if relative.startswith(("scripts/", "tools/", ".github/")) and path.suffix in {".js", ".mjs", ".cjs", ".ts"}:
            for match in JS_ENV_LOOKUP.finditer(source):
                add(match.group(1), path)

    return detected


def _walk(value: Any, path: str = "$") -> Iterable[tuple[str, str, Any]]:
    if isinstance(value, dict):
        for key, nested in value.items():
            yield path, str(key), nested
            yield from _walk(nested, f"{path}.{key}")
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            yield from _walk(nested, f"{path}[{index}]")


def _entries(payload: dict[str, Any]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for section in ("credentials", "dynamic_credential_domains", "future_trust_domains"):
        raw = payload.get(section)
        if not isinstance(raw, list):
            raise ValueError(f"{section} must be a list")
        for index, entry in enumerate(raw):
            if not isinstance(entry, dict):
                raise ValueError(f"{section}[{index}] must be an object")
            entries.append(entry)
    return entries


def _binding_index(entries: list[dict[str, Any]]) -> tuple[dict[str, str], set[str]]:
    owners: dict[str, str] = {}
    declaration_only: set[str] = set()
    for entry in entries:
        logical_id = str(entry.get("id", ""))
        bindings = entry.get("bindings", [])
        if not isinstance(bindings, list):
            raise ValueError(f"{logical_id}: bindings must be a list")
        seen_within_entry: set[tuple[str, str, str]] = set()
        for binding in bindings:
            if not isinstance(binding, dict):
                raise ValueError(f"{logical_id}: binding must be an object")
            name = binding.get("name")
            surface = binding.get("surface")
            if not isinstance(name, str) or not name:
                raise ValueError(f"{logical_id}: binding.name must be non-empty")
            if not isinstance(surface, str) or not surface:
                raise ValueError(f"{logical_id}/{name}: binding.surface must be non-empty")
            consumer = str(binding.get("consumer", ""))
            identity = (surface, name, consumer)
            if identity in seen_within_entry:
                raise ValueError(f"{logical_id}: duplicate binding tuple {identity!r}")
            seen_within_entry.add(identity)
            previous = owners.get(name)
            if previous is not None and previous != logical_id:
                raise ValueError(
                    f"binding {name} is assigned to multiple logical authorities: {previous}, {logical_id}"
                )
            owners[name] = logical_id
            if binding.get("declaration_only") is True:
                declaration_only.add(name)
    return owners, declaration_only


def _validate_environment_scope(logical_id: str, scope: Any) -> None:
    if not isinstance(scope, dict):
        raise ValueError(f"{logical_id}: environment_scope must be an object")
    kind = scope.get("kind")
    if kind not in {"repository", "environment", "tenant_dynamic", "release"}:
        raise ValueError(f"{logical_id}: unknown environment_scope.kind {kind!r}")
    environments = scope.get("environments", [])
    if kind == "environment":
        if not isinstance(environments, list) or not environments:
            raise ValueError(f"{logical_id}: environment scope requires environments")
        unknown = set(environments) - CANONICAL_ENVIRONMENTS
        if unknown:
            raise ValueError(f"{logical_id}: non-canonical environments {sorted(unknown)!r}")
    elif environments:
        raise ValueError(f"{logical_id}: environments are valid only for kind=environment")


def validate_payload(
    payload: dict[str, Any],
    detected: dict[str, set[str]],
    *,
    allow_declared_only: bool = True,
) -> dict[str, Any]:
    if payload.get("schema_version") != 1:
        raise ValueError("credential authority schema_version must be 1")
    if payload.get("status") != "CANDIDATE_AR8B_CREDENTIAL_METADATA_AUTHORITY":
        raise ValueError("credential authority must remain candidate until AR-8B acceptance")
    if payload.get("canonical_inventory") != CANONICAL_INVENTORY:
        raise ValueError("AR-8B must extend architecture/inventory.json, not establish a competing registry")
    if payload.get("metadata_only") is not True:
        raise ValueError("credential authority must be metadata_only=true")
    if set(payload.get("canonical_environments", [])) != CANONICAL_ENVIRONMENTS:
        raise ValueError("canonical_environments must be rehearsal/staging/production exactly")

    invariants = payload.get("invariants")
    if not isinstance(invariants, dict):
        raise ValueError("invariants must be an object")
    if invariants.get("plaintext_in_git") != "FORBIDDEN":
        raise ValueError("plaintext_in_git must be FORBIDDEN")
    if invariants.get("competing_registry") != "FORBIDDEN":
        raise ValueError("competing_registry must be FORBIDDEN")
    if invariants.get("mutable_authorities_per_concern") != 1:
        raise ValueError("one concern must have exactly one legitimate mutable authority")
    if invariants.get("production_mutation") is not False or invariants.get("ar9_blocked") is not True:
        raise ValueError("AR-8B must keep production mutation disabled and AR-9 blocked")

    for location, key, nested in _walk(payload):
        if key.lower() in FORBIDDEN_VALUE_FIELDS:
            raise ValueError(f"forbidden value-bearing field {location}.{key}")
        if isinstance(nested, str):
            for pattern in HIGH_CONFIDENCE_VALUE_PATTERNS:
                if pattern.search(nested):
                    raise ValueError(f"high-confidence credential material found at {location}.{key}")

    entries = _entries(payload)
    ids: set[str] = set()
    for entry in entries:
        missing = REQUIRED_ENTRY_FIELDS - set(entry)
        logical_id = entry.get("id", "<missing-id>")
        if missing:
            raise ValueError(f"{logical_id}: missing required fields {sorted(missing)!r}")
        if not isinstance(logical_id, str) or not logical_id:
            raise ValueError("credential entry id must be non-empty")
        if logical_id in ids:
            raise ValueError(f"duplicate credential authority id: {logical_id}")
        ids.add(logical_id)
        for field in (
            "class",
            "provider_system",
            "owner",
            "protected_value_authority",
            "legitimate_mutable_authority",
            "version_state_source",
            "automation_class",
            "rotation_recovery_policy",
            "future_cutover",
        ):
            if not isinstance(entry.get(field), str) or not entry[field].strip():
                raise ValueError(f"{logical_id}: {field} must be one non-empty string")
        if not isinstance(entry.get("externally_issued"), bool):
            raise ValueError(f"{logical_id}: externally_issued must be boolean")
        if not isinstance(entry.get("consumers"), list):
            raise ValueError(f"{logical_id}: consumers must be a list")
        _validate_environment_scope(logical_id, entry.get("environment_scope"))

    owners, declaration_only = _binding_index(entries)
    detected_names = set(detected)
    missing = sorted(detected_names - set(owners))
    if missing:
        details = ", ".join(f"{name} ({sorted(detected[name])})" for name in missing)
        raise ValueError(f"tracked credential bindings missing canonical authority: {details}")

    stale = sorted(set(owners) - detected_names - declaration_only)
    if stale:
        raise ValueError(
            "authority contains non-detected static bindings without declaration_only=true: "
            + ", ".join(stale)
        )
    if not allow_declared_only and declaration_only:
        raise ValueError(f"declaration-only bindings not allowed in this validation mode: {sorted(declaration_only)!r}")

    return {
        "schema_version": 1,
        "status": payload["status"],
        "source_authority": AUTHORITY_PATH.as_posix(),
        "metadata_only": True,
        "logical_authority_count": len(entries),
        "detected_static_binding_count": len(detected_names),
        "detected_static_bindings": sorted(detected_names),
        "declaration_only_bindings": sorted(declaration_only),
        "canonical_environments": sorted(CANONICAL_ENVIRONMENTS),
        "production_mutation": False,
        "ar9_blocked": True,
    }


def load_and_validate(root: Path) -> dict[str, Any]:
    path = root / AUTHORITY_PATH
    if not path.is_file():
        raise ValueError(f"missing AR-8B authority: {AUTHORITY_PATH}")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid JSON in {AUTHORITY_PATH}: {exc}") from exc
    if not isinstance(payload, dict):
        raise ValueError("AR-8B authority must contain a JSON object")
    return validate_payload(payload, discover_static_bindings(root))


def self_test(root: Path) -> None:
    payload = json.loads((root / AUTHORITY_PATH).read_text(encoding="utf-8"))
    detected = discover_static_bindings(root)
    validate_payload(payload, detected)

    def rejected(mutator: Any, *, detected_override: dict[str, set[str]] | None = None) -> None:
        candidate = copy.deepcopy(payload)
        mutator(candidate)
        try:
            validate_payload(candidate, detected if detected_override is None else detected_override)
        except ValueError:
            return
        raise AssertionError("AR-8B negative fixture unexpectedly passed")

    rejected(lambda candidate: candidate["credentials"][0].update({"value": "forbidden"}))
    rejected(lambda candidate: candidate["credentials"][0].update({"environment_scope": {"kind": "environment", "environments": ["prod"]}}))
    rejected(lambda candidate: candidate["credentials"][0].update({"legitimate_mutable_authority": ["one", "two"]}))

    def duplicate_id(candidate: dict[str, Any]) -> None:
        clone = copy.deepcopy(candidate["credentials"][0])
        clone["id"] = candidate["credentials"][1]["id"]
        candidate["credentials"].append(clone)

    rejected(duplicate_id)

    synthetic = copy.deepcopy(detected)
    synthetic["AR8B_UNKNOWN_TRACKED_SECRET"] = {"tests/synthetic-workflow.yml"}
    rejected(lambda candidate: None, detected_override=synthetic)

    def duplicate_binding(candidate: dict[str, Any]) -> None:
        first = candidate["credentials"][0]["bindings"][0]["name"]
        candidate["credentials"][1]["bindings"].append(
            {"surface": "github_actions_secret", "name": first}
        )

    rejected(duplicate_binding)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        projection = load_and_validate(root)
        if args.self_test:
            self_test(root)
    except (ValueError, AssertionError, subprocess.CalledProcessError) as exc:
        print(f"AR-8B credential authority check failed: {exc}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(projection, indent=2, sort_keys=True))
    else:
        print(
            "AR-8B credential metadata authority is fail-closed and covers "
            f"{projection['detected_static_binding_count']} tracked static bindings."
        )
        if args.self_test:
            print("AR-8B negative authority fixtures are rejected.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
