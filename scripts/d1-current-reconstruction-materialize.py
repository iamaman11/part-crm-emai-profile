#!/usr/bin/env python3
"""Materialize exact CURRENT Catalog reconstruction bytes from the existing D1 authority.

This is a credential-free adapter, not a second schema or mutation owner. It accepts only an already
sealed CURRENT reconstruction projection, rebinds it to the production ``opsctl d1 repository``
projection, and delegates SQL construction to ``cloudflare-d1-bootstrap.py``. It never contacts the
provider and never authorizes a mutation.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import re
import sys
import tempfile
from pathlib import Path
from types import ModuleType
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
BOOTSTRAP_AUTHORITY = ROOT / "scripts" / "cloudflare-d1-bootstrap.py"
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_OBJECT_RE = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
RELEASE_SET_RE = re.compile(r"^release-set-v3-sha256-[0-9a-f]{64}$")
MIGRATION_RE = re.compile(r"^[0-9]{4}_[a-z0-9_]+\.sql$")
ALLOWED_EFFECT = "D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION"
FORBIDDEN_EFFECTS = [
    "D1_MIGRATIONS_APPLY_EXACT_PLAN",
    "D1_CREATE",
    "D1_DELETE",
    "D1_TIME_TRAVEL_RESTORE",
    "RESOURCE_AUTO_PROVISION",
    "PRODUCTION_MUTATION",
]
RECONSTRUCTION_TOP_KEYS = {
    "schema_version",
    "status",
    "mode",
    "authorization_required",
    "authorization_consumed",
    "mutation_executed",
    "provider_mutation_executed",
    "reconstruction_id",
    "plan",
}
RECONSTRUCTION_PLAN_KEYS = {
    "schema_version",
    "kind",
    "disposition",
    "component",
    "source_sha",
    "tree_sha",
    "release_set_id",
    "release_manifest_sha256",
    "repository_identity_sha256",
    "construction_sha256",
    "target_schema_revision",
    "supported_schema_min",
    "supported_schema_max",
    "provider_observation",
    "observation_digest",
    "freshness_max_age_seconds",
    "allowed_provider_effects",
    "forbidden_provider_effects",
    "expected_post_state",
}
PROVIDER_OBSERVATION_KEYS = {
    "target",
    "observed_at_unix_seconds",
    "fresh_until_unix_seconds",
    "observation_source",
    "predecessor_ledger_sha256",
    "remote_migrations",
}
EXPECTED_POST_KEYS = {
    "component",
    "target_schema_revision",
    "ledger_migrations",
    "construction_sha256",
    "repository_identity_sha256",
}
TARGET_KEYS = {"environment", "account_id", "database_name", "database_id"}


class MaterializeError(ValueError):
    """Raised when the reconstruction/materialization binding fails closed."""


def fail(message: str) -> None:
    raise MaterializeError(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise MaterializeError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def read_strict_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError, MaterializeError) as error:
        raise MaterializeError(f"cannot read strict {label}: {error}") from error


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def canonical_sha256(value: Any) -> str:
    return hashlib.sha256(canonical_json(value).encode("utf-8")).hexdigest()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def require_exact_object(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        fail(f"{label} must contain exact keys {sorted(keys)}")
    return value


def require_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        fail(f"{label} must be one non-empty string")
    return value


def require_sha256(value: Any, label: str) -> str:
    value = require_string(value, label)
    if SHA256_RE.fullmatch(value) is None:
        fail(f"{label} must be exactly 64 lowercase hexadecimal characters")
    return value


def load_bootstrap_authority() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "part_crm_cloudflare_d1_bootstrap_authority", BOOTSTRAP_AUTHORITY
    )
    if spec is None or spec.loader is None:
        fail("cannot load existing Cloudflare D1 bootstrap authority")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def validate_reconstruction_binding(
    reconstruction: Any,
    repository: Any,
    bootstrap: ModuleType,
) -> dict[str, Any]:
    root = require_exact_object(reconstruction, RECONSTRUCTION_TOP_KEYS, "CURRENT reconstruction")
    if (
        root.get("schema_version") != 1
        or root.get("status") != "RECONSTRUCTION_PREPARED"
        or root.get("mode") != "read-only"
        or root.get("authorization_required") is not True
        or root.get("authorization_consumed") is not False
        or root.get("mutation_executed") is not False
        or root.get("provider_mutation_executed") is not False
    ):
        fail("CURRENT reconstruction is not one unconsumed read-only prepared operation")
    reconstruction_id = require_sha256(root.get("reconstruction_id"), "reconstruction_id")
    plan = require_exact_object(root.get("plan"), RECONSTRUCTION_PLAN_KEYS, "CURRENT reconstruction plan")
    if (
        plan.get("schema_version") != 1
        or plan.get("kind") != "D1_CURRENT_FRESH_ZERO_RECONSTRUCTION"
        or plan.get("disposition") != "PREPROD_BASELINE_DRIFT"
        or plan.get("component") != "catalog"
    ):
        fail("CURRENT reconstruction kind/disposition/component drifted")
    if canonical_sha256(plan) != reconstruction_id:
        fail("reconstruction_id does not bind the exact canonical reconstruction plan")

    source_sha = require_string(plan.get("source_sha"), "plan.source_sha")
    tree_sha = require_string(plan.get("tree_sha"), "plan.tree_sha")
    if GIT_OBJECT_RE.fullmatch(source_sha) is None or GIT_OBJECT_RE.fullmatch(tree_sha) is None:
        fail("CURRENT reconstruction source/tree identity is malformed")
    release_set_id = require_string(plan.get("release_set_id"), "plan.release_set_id")
    if RELEASE_SET_RE.fullmatch(release_set_id) is None:
        fail("CURRENT reconstruction Release Set identity is malformed")
    require_sha256(plan.get("release_manifest_sha256"), "plan.release_manifest_sha256")

    construction, _paths = bootstrap.validate_current_fresh_zero_projection(repository, ROOT)
    repository_identity = require_sha256(
        repository.get("repository_identity_sha256") if isinstance(repository, dict) else None,
        "repository.repository_identity_sha256",
    )
    envelope = require_exact_object(
        repository.get("fresh_zero_construction") if isinstance(repository, dict) else None,
        {"construction", "construction_sha256"},
        "repository fresh_zero_construction",
    )
    construction_digest = require_sha256(
        envelope.get("construction_sha256"), "repository construction_sha256"
    )
    if plan.get("repository_identity_sha256") != repository_identity:
        fail("reconstruction repository identity differs from CURRENT typed D1 authority")
    if plan.get("construction_sha256") != construction_digest:
        fail("reconstruction construction digest differs from CURRENT typed D1 authority")
    target_revision = require_string(construction.get("target_schema_revision"), "CURRENT target")
    if plan.get("target_schema_revision") != target_revision:
        fail("reconstruction target schema differs from CURRENT fresh-zero construction")

    provider = require_exact_object(
        plan.get("provider_observation"), PROVIDER_OBSERVATION_KEYS, "provider observation"
    )
    target = require_exact_object(provider.get("target"), TARGET_KEYS, "reconstruction target")
    if target.get("environment") != "staging":
        fail("CURRENT reconstruction target must be exactly staging")
    for key in ("account_id", "database_name", "database_id"):
        require_string(target.get(key), f"reconstruction target.{key}")
    if provider.get("remote_migrations") != []:
        fail("CURRENT reconstruction materialization requires exactly empty predecessor migrations")
    observed = provider.get("observed_at_unix_seconds")
    fresh_until = provider.get("fresh_until_unix_seconds")
    freshness = plan.get("freshness_max_age_seconds")
    if (
        isinstance(observed, bool)
        or not isinstance(observed, int)
        or observed <= 0
        or isinstance(fresh_until, bool)
        or not isinstance(fresh_until, int)
        or isinstance(freshness, bool)
        or not isinstance(freshness, int)
        or freshness <= 0
        or fresh_until != observed + freshness
    ):
        fail("CURRENT reconstruction observation freshness binding is invalid")
    require_string(provider.get("observation_source"), "provider observation source")
    require_sha256(provider.get("predecessor_ledger_sha256"), "predecessor ledger digest")
    if plan.get("observation_digest") != canonical_sha256(provider):
        fail("reconstruction observation_digest does not bind the exact provider observation")

    if plan.get("allowed_provider_effects") != [ALLOWED_EFFECT]:
        fail("CURRENT reconstruction provider effect is not the sole exact bootstrap effect")
    if plan.get("forbidden_provider_effects") != FORBIDDEN_EFFECTS:
        fail("CURRENT reconstruction forbidden-effect set drifted")

    expected = require_exact_object(
        plan.get("expected_post_state"), EXPECTED_POST_KEYS, "expected post-state"
    )
    sources = construction.get("migration_sources")
    if not isinstance(sources, list) or not sources:
        fail("CURRENT construction has no migration sources")
    expected_ledger: list[str] = []
    for source in sources:
        if not isinstance(source, dict):
            fail("CURRENT construction source must be an object")
        migration_file = require_string(source.get("migration_file"), "construction migration_file")
        if MIGRATION_RE.fullmatch(migration_file) is None or migration_file in expected_ledger:
            fail("CURRENT construction migration ledger identity is malformed or duplicated")
        expected_ledger.append(migration_file)
    if expected_ledger[-1] != target_revision:
        fail("CURRENT construction ledger does not terminate at target revision")
    if (
        expected.get("component") != "catalog"
        or expected.get("target_schema_revision") != target_revision
        or expected.get("ledger_migrations") != expected_ledger
        or expected.get("construction_sha256") != construction_digest
        or expected.get("repository_identity_sha256") != repository_identity
    ):
        fail("reconstruction expected post-state differs from CURRENT construction")
    deferred = construction.get("deferred_revisions")
    if not isinstance(deferred, list) or any(item in expected_ledger for item in deferred):
        fail("CURRENT construction deferred revisions overlap materialized ledger")

    return {
        "reconstruction_id": reconstruction_id,
        "source_sha": source_sha,
        "tree_sha": tree_sha,
        "release_set_id": release_set_id,
        "target": target,
        "repository_identity_sha256": repository_identity,
        "construction_sha256": construction_digest,
        "target_schema_revision": target_revision,
        "expected_ledger_migrations": expected_ledger,
        "deferred_revisions": deferred,
    }


def write_exact(path: Path, payload: bytes) -> None:
    if path.exists() and path.is_symlink():
        fail(f"output path must not be a symlink: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.parent.is_symlink():
        fail(f"output parent must not be a symlink: {path.parent}")
    path.write_bytes(payload)


def materialize_wrangler_migration_inventory(bootstrap: ModuleType, output_root: Path) -> list[str]:
    source_dir = Path(bootstrap.MIGRATIONS_DIR)
    migrations = bootstrap.validated_migrations(source_dir)
    target_dir = output_root / "migrations"
    if target_dir.exists():
        if target_dir.is_symlink() or not target_dir.is_dir():
            fail(f"Wrangler migration output must be a real directory: {target_dir}")
        if any(target_dir.iterdir()):
            fail(f"Wrangler migration output must start empty: {target_dir}")
    else:
        target_dir.mkdir(parents=True)
    names: list[str] = []
    for source in migrations:
        payload = source.read_bytes()
        target = target_dir / source.name
        write_exact(target, payload)
        if target.read_bytes() != payload:
            fail(f"Wrangler migration materialization drifted: {source.name}")
        names.append(source.name)
    return names


def materialize(reconstruction_path: Path, output_sql: Path, output_metadata: Path) -> dict[str, Any]:
    bootstrap = load_bootstrap_authority()
    reconstruction = read_strict_json(reconstruction_path, "CURRENT reconstruction JSON")
    repository = bootstrap.current_repository_projection()
    binding = validate_reconstruction_binding(reconstruction, repository, bootstrap)
    convergence = bootstrap.prove_current_fresh_zero_convergence(repository, ROOT)
    payload = bootstrap.build_current_fresh_zero_bootstrap_bytes(repository, ROOT)
    payload_sha256 = sha256_bytes(payload)
    if convergence.get("bootstrap_sha256") != payload_sha256:
        fail("CURRENT bootstrap bytes differ from convergence proof identity")
    wrangler_inventory = materialize_wrangler_migration_inventory(bootstrap, output_sql.parent)
    metadata = {
        "schema_version": 1,
        "kind": "D1_CURRENT_RECONSTRUCTION_MATERIALIZATION",
        "mode": "read-only",
        **binding,
        "provider_effect": ALLOWED_EFFECT,
        "bootstrap_sha256": payload_sha256,
        "bootstrap_bytes": len(payload),
        "wrangler_migration_inventory": wrangler_inventory,
        "provider_mutation_authorized": False,
        "production_mutation_authorized": False,
    }
    write_exact(output_sql, payload)
    write_exact(output_metadata, (canonical_json(metadata) + "\n").encode("utf-8"))
    return metadata


def expect_rejected(label: str, operation: Callable[[], Any]) -> None:
    try:
        operation()
    except (MaterializeError, ValueError):
        return
    fail(f"negative reconstruction materialization fixture unexpectedly passed: {label}")


def synthetic_reconstruction(repository: dict[str, Any], bootstrap: ModuleType) -> dict[str, Any]:
    construction, _paths = bootstrap.validate_current_fresh_zero_projection(repository, ROOT)
    target_revision = construction["target_schema_revision"]
    expected_ledger = [source["migration_file"] for source in construction["migration_sources"]]
    repository_identity = repository["repository_identity_sha256"]
    construction_digest = repository["fresh_zero_construction"]["construction_sha256"]
    provider = {
        "target": {
            "environment": "staging",
            "account_id": "account-self-test",
            "database_name": "catalog-self-test",
            "database_id": "database-self-test",
        },
        "observed_at_unix_seconds": 1_789_410_000,
        "fresh_until_unix_seconds": 1_789_410_900,
        "observation_source": "credential-free-self-test",
        "predecessor_ledger_sha256": canonical_sha256({"remote_migrations": []}),
        "remote_migrations": [],
    }
    plan = {
        "schema_version": 1,
        "kind": "D1_CURRENT_FRESH_ZERO_RECONSTRUCTION",
        "disposition": "PREPROD_BASELINE_DRIFT",
        "component": "catalog",
        "source_sha": "11" * 20,
        "tree_sha": "22" * 20,
        "release_set_id": "release-set-v3-sha256-" + "33" * 32,
        "release_manifest_sha256": "44" * 32,
        "repository_identity_sha256": repository_identity,
        "construction_sha256": construction_digest,
        "target_schema_revision": target_revision,
        "supported_schema_min": target_revision,
        "supported_schema_max": target_revision,
        "provider_observation": provider,
        "observation_digest": canonical_sha256(provider),
        "freshness_max_age_seconds": 900,
        "allowed_provider_effects": [ALLOWED_EFFECT],
        "forbidden_provider_effects": FORBIDDEN_EFFECTS.copy(),
        "expected_post_state": {
            "component": "catalog",
            "target_schema_revision": target_revision,
            "ledger_migrations": expected_ledger,
            "construction_sha256": construction_digest,
            "repository_identity_sha256": repository_identity,
        },
    }
    return {
        "schema_version": 1,
        "status": "RECONSTRUCTION_PREPARED",
        "mode": "read-only",
        "authorization_required": True,
        "authorization_consumed": False,
        "mutation_executed": False,
        "provider_mutation_executed": False,
        "reconstruction_id": canonical_sha256(plan),
        "plan": plan,
    }


def self_test() -> None:
    bootstrap = load_bootstrap_authority()
    repository = bootstrap.current_repository_projection()
    valid = synthetic_reconstruction(repository, bootstrap)
    binding = validate_reconstruction_binding(valid, repository, bootstrap)
    first = bootstrap.build_current_fresh_zero_bootstrap_bytes(repository, ROOT)
    second = bootstrap.build_current_fresh_zero_bootstrap_bytes(repository, ROOT)
    if first != second or not first:
        fail("CURRENT reconstruction adapter did not receive deterministic bootstrap bytes")
    if binding["expected_ledger_migrations"][-1] != binding["target_schema_revision"]:
        fail("self-test CURRENT ledger does not terminate at target revision")

    expected_inventory = [path.name for path in bootstrap.validated_migrations(bootstrap.MIGRATIONS_DIR)]
    with tempfile.TemporaryDirectory(prefix="d1-reconstruction-wrangler-") as temp_dir:
        output_root = Path(temp_dir)
        actual_inventory = materialize_wrangler_migration_inventory(bootstrap, output_root)
        if actual_inventory != expected_inventory:
            fail("self-test Wrangler migration inventory differs from canonical Catalog migrations")
        for name in actual_inventory:
            source = Path(bootstrap.MIGRATIONS_DIR) / name
            target = output_root / "migrations" / name
            if sha256_bytes(source.read_bytes()) != sha256_bytes(target.read_bytes()):
                fail(f"self-test Wrangler migration bytes drifted: {name}")

    repository_drift = copy.deepcopy(valid)
    repository_drift["plan"]["repository_identity_sha256"] = "0" * 64
    repository_drift["reconstruction_id"] = canonical_sha256(repository_drift["plan"])
    expect_rejected(
        "repository identity drift",
        lambda: validate_reconstruction_binding(repository_drift, repository, bootstrap),
    )

    construction_drift = copy.deepcopy(valid)
    construction_drift["plan"]["construction_sha256"] = "0" * 64
    construction_drift["reconstruction_id"] = canonical_sha256(construction_drift["plan"])
    expect_rejected(
        "construction identity drift",
        lambda: validate_reconstruction_binding(construction_drift, repository, bootstrap),
    )

    production = copy.deepcopy(valid)
    production["plan"]["provider_observation"]["target"]["environment"] = "production"
    production["plan"]["observation_digest"] = canonical_sha256(
        production["plan"]["provider_observation"]
    )
    production["reconstruction_id"] = canonical_sha256(production["plan"])
    expect_rejected(
        "Production target",
        lambda: validate_reconstruction_binding(production, repository, bootstrap),
    )

    migration_fallback = copy.deepcopy(valid)
    migration_fallback["plan"]["allowed_provider_effects"] = [
        "D1_MIGRATIONS_APPLY_EXACT_PLAN"
    ]
    migration_fallback["reconstruction_id"] = canonical_sha256(migration_fallback["plan"])
    expect_rejected(
        "ordinary migration fallback",
        lambda: validate_reconstruction_binding(migration_fallback, repository, bootstrap),
    )

    ledger_drift = copy.deepcopy(valid)
    ledger_drift["plan"]["expected_post_state"]["ledger_migrations"].pop()
    ledger_drift["reconstruction_id"] = canonical_sha256(ledger_drift["plan"])
    expect_rejected(
        "shortened target ledger",
        lambda: validate_reconstruction_binding(ledger_drift, repository, bootstrap),
    )

    print(
        "CURRENT reconstruction materializer adapter passed: "
        f"reconstruction_id={binding['reconstruction_id']} "
        f"target={binding['target_schema_revision']} "
        f"bootstrap_sha256={sha256_bytes(first)} "
        f"wrangler_migrations={len(expected_inventory)} provider_mutation=NO production_mutation=NO"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("self-test")
    materialize_parser = subparsers.add_parser("materialize")
    materialize_parser.add_argument("--reconstruction-json", type=Path, required=True)
    materialize_parser.add_argument("--output-sql", type=Path, required=True)
    materialize_parser.add_argument("--output-metadata", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "self-test":
        self_test()
        return 0
    if args.command == "materialize":
        metadata = materialize(args.reconstruction_json, args.output_sql, args.output_metadata)
        print(canonical_json(metadata))
        return 0
    fail(f"unsupported command: {args.command}")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except MaterializeError as error:
        raise SystemExit(f"CURRENT D1 reconstruction materialization rejected: {error}") from error
