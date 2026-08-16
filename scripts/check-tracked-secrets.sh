#!/usr/bin/env bash
set -euo pipefail

pattern='BEGIN (RSA|EC|OPENSSH) PRIVATE KEY|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|-----BEGIN PRIVATE KEY-----'

if git grep -n -E "$pattern" -- . ':!scripts/check-tracked-secrets.sh'; then
  echo "Potential credential material found in tracked files." >&2
  exit 1
fi

python - <<'PY'
from __future__ import annotations

import copy
import json
import re
import subprocess
from pathlib import Path
from typing import Any, Iterable

ROOT = Path.cwd()
AUTHORITY = ROOT / "architecture/credential-authority-ar8b.json"
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


def tracked_files() -> list[Path]:
    result = subprocess.run(["git", "ls-files", "-z"], check=True, capture_output=True)
    return [ROOT / Path(value.decode("utf-8")) for value in result.stdout.split(b"\0") if value]


def text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return ""


def discover() -> dict[str, set[str]]:
    detected: dict[str, set[str]] = {}

    def add(name: str, path: Path) -> None:
        detected.setdefault(name, set()).add(path.relative_to(ROOT).as_posix())

    for path in tracked_files():
        relative = path.relative_to(ROOT).as_posix()
        source = text(path)
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


def walk(value: Any, path: str = "$") -> Iterable[tuple[str, str, Any]]:
    if isinstance(value, dict):
        for key, nested in value.items():
            yield path, str(key), nested
            yield from walk(nested, f"{path}.{key}")
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            yield from walk(nested, f"{path}[{index}]")


def entries(payload: dict[str, Any]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for section in ("credentials", "dynamic_credential_domains", "future_trust_domains"):
        raw = payload.get(section)
        if not isinstance(raw, list):
            raise ValueError(f"{section} must be a list")
        if any(not isinstance(entry, dict) for entry in raw):
            raise ValueError(f"{section} entries must be objects")
        result.extend(raw)
    return result


def validate(payload: dict[str, Any], detected: dict[str, set[str]]) -> None:
    if payload.get("schema_version") != 1:
        raise ValueError("credential authority schema_version must be 1")
    if payload.get("status") != "CANDIDATE_AR8B_CREDENTIAL_METADATA_AUTHORITY":
        raise ValueError("credential authority must remain candidate until AR-8B acceptance")
    if payload.get("canonical_inventory") != "architecture/inventory.json":
        raise ValueError("AR-8B must extend the canonical inventory, not create a competing registry")
    if payload.get("metadata_only") is not True:
        raise ValueError("credential authority must be metadata_only=true")
    if set(payload.get("canonical_environments", [])) != CANONICAL_ENVIRONMENTS:
        raise ValueError("canonical_environments must be rehearsal/staging/production exactly")

    invariants = payload.get("invariants")
    if not isinstance(invariants, dict):
        raise ValueError("invariants must be an object")
    if invariants.get("plaintext_in_git") != "FORBIDDEN" or invariants.get("competing_registry") != "FORBIDDEN":
        raise ValueError("plaintext and competing registries must remain forbidden")
    if invariants.get("mutable_authorities_per_concern") != 1:
        raise ValueError("one concern must have exactly one legitimate mutable authority")
    if invariants.get("production_mutation") is not False or invariants.get("ar9_blocked") is not True:
        raise ValueError("AR-8B must keep production mutation disabled and AR-9 blocked")

    for location, key, nested in walk(payload):
        if key.lower() in FORBIDDEN_VALUE_FIELDS:
            raise ValueError(f"forbidden value-bearing field {location}.{key}")
        if isinstance(nested, str) and any(pattern.search(nested) for pattern in HIGH_CONFIDENCE_VALUE_PATTERNS):
            raise ValueError(f"high-confidence credential material found at {location}.{key}")

    logical_entries = entries(payload)
    ids: set[str] = set()
    owners: dict[str, str] = {}
    declaration_only: set[str] = set()
    for entry in logical_entries:
        missing = REQUIRED_ENTRY_FIELDS - set(entry)
        logical_id = entry.get("id", "<missing-id>")
        if missing:
            raise ValueError(f"{logical_id}: missing required fields {sorted(missing)!r}")
        if not isinstance(logical_id, str) or not logical_id or logical_id in ids:
            raise ValueError(f"invalid or duplicate credential authority id: {logical_id!r}")
        ids.add(logical_id)
        if not isinstance(entry.get("externally_issued"), bool) or not isinstance(entry.get("consumers"), list):
            raise ValueError(f"{logical_id}: malformed externally_issued/consumers")
        scope = entry.get("environment_scope")
        if not isinstance(scope, dict):
            raise ValueError(f"{logical_id}: environment_scope must be an object")
        kind = scope.get("kind")
        environments = scope.get("environments", [])
        if kind not in {"repository", "environment", "tenant_dynamic", "release"}:
            raise ValueError(f"{logical_id}: unknown environment scope {kind!r}")
        if kind == "environment":
            if not isinstance(environments, list) or not environments or set(environments) - CANONICAL_ENVIRONMENTS:
                raise ValueError(f"{logical_id}: invalid canonical environment scope")
        elif environments:
            raise ValueError(f"{logical_id}: environments allowed only for kind=environment")
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
                raise ValueError(f"{logical_id}: {field} must be non-empty")
        bindings = entry.get("bindings")
        if not isinstance(bindings, list):
            raise ValueError(f"{logical_id}: bindings must be a list")
        seen: set[tuple[str, str, str]] = set()
        for binding in bindings:
            if not isinstance(binding, dict):
                raise ValueError(f"{logical_id}: binding must be an object")
            name = binding.get("name")
            surface = binding.get("surface")
            consumer = str(binding.get("consumer", ""))
            if not isinstance(name, str) or not name or not isinstance(surface, str) or not surface:
                raise ValueError(f"{logical_id}: binding name/surface must be non-empty")
            identity = (surface, name, consumer)
            if identity in seen:
                raise ValueError(f"{logical_id}: duplicate binding tuple {identity!r}")
            seen.add(identity)
            previous = owners.get(name)
            if previous is not None and previous != logical_id:
                raise ValueError(f"binding {name} belongs to multiple authorities: {previous}, {logical_id}")
            owners[name] = logical_id
            if binding.get("declaration_only") is True:
                declaration_only.add(name)

    missing = sorted(set(detected) - set(owners))
    if missing:
        details = ", ".join(f"{name} ({sorted(detected[name])})" for name in missing)
        raise ValueError(f"tracked credential bindings missing canonical authority: {details}")
    stale = sorted(set(owners) - set(detected) - declaration_only)
    if stale:
        raise ValueError("authority has non-detected bindings without declaration_only=true: " + ", ".join(stale))


if not AUTHORITY.is_file():
    raise SystemExit(f"missing AR-8B authority: {AUTHORITY.relative_to(ROOT)}")
payload = json.loads(AUTHORITY.read_text(encoding="utf-8"))
if not isinstance(payload, dict):
    raise SystemExit("AR-8B authority must contain one JSON object")
detected = discover()
validate(payload, detected)

negative = copy.deepcopy(payload)
negative["credentials"][0]["value"] = "forbidden"
try:
    validate(negative, detected)
except ValueError:
    pass
else:
    raise SystemExit("AR-8B negative value-bearing fixture unexpectedly passed")

synthetic = copy.deepcopy(detected)
synthetic["AR8B_UNKNOWN_TRACKED_SECRET"] = {"tests/synthetic-workflow.yml"}
try:
    validate(payload, synthetic)
except ValueError:
    pass
else:
    raise SystemExit("AR-8B unknown-binding fixture unexpectedly passed")

print(f"AR-8B credential metadata authority covers {len(detected)} tracked static bindings and rejects negative fixtures.")
PY

echo "No high-confidence credential patterns found in tracked files, and AR-8B metadata authority is consistent."
