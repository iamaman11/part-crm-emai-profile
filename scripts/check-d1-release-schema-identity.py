#!/usr/bin/env python3
"""Prove Catalog and Resolver immutable release identities include their D1 schema policy."""

from __future__ import annotations

import argparse
import ast
import importlib.util
import sys
from pathlib import Path
from types import ModuleType
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CATALOG_RELEASE = ROOT / "scripts" / "cloudflare-release.py"
RESOLVER_RELEASE = ROOT / "scripts" / "mailbox-secret-resolver-release.py"


class SchemaIdentityError(ValueError):
    pass


def fail(message: str) -> None:
    raise SchemaIdentityError(message)


def load_module(path: Path, name: str) -> ModuleType:
    specification = importlib.util.spec_from_file_location(name, path)
    if specification is None or specification.loader is None:
        fail(f"cannot load release module: {path}")
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def function_node(path: Path, name: str) -> ast.FunctionDef:
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    matches = [
        node
        for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name == name
    ]
    if len(matches) != 1:
        fail(f"{path.name} must define exactly one {name}()")
    return matches[0]


def dict_contains_schema_contract(function: ast.FunctionDef) -> bool:
    for node in ast.walk(function):
        if not isinstance(node, ast.Dict):
            continue
        for key, value in zip(node.keys, node.values, strict=True):
            if not isinstance(key, ast.Constant) or key.value != "schema_contract":
                continue
            if (
                isinstance(value, ast.Call)
                and isinstance(value.func, ast.Name)
                and value.func.id == "load_schema_contract"
            ):
                return True
    return False


def call_hashes_payload(function: ast.FunctionDef, helper_name: str) -> bool:
    for node in ast.walk(function):
        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name):
            continue
        if node.func.id != helper_name or len(node.args) != 1:
            continue
        if isinstance(node.args[0], ast.Name) and node.args[0].id == "payload":
            return True
    return False


def resolver_hashes_canonical_payload(function: ast.FunctionDef) -> bool:
    for node in ast.walk(function):
        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name):
            continue
        if node.func.id != "sha256_bytes" or len(node.args) != 1:
            continue
        canonical = node.args[0]
        if (
            isinstance(canonical, ast.Call)
            and isinstance(canonical.func, ast.Name)
            and canonical.func.id == "canonical"
            and len(canonical.args) == 1
            and isinstance(canonical.args[0], ast.Name)
            and canonical.args[0].id == "payload"
        ):
            return True
    return False


def conservative_window(contract: Any, component: str) -> None:
    if not isinstance(contract, dict) or contract.get("database_component") != component:
        fail(f"{component} release schema contract is missing")
    target = contract.get("target_schema_revision")
    if not isinstance(target, str) or not target:
        fail(f"{component} target schema revision is missing")
    if contract.get("supported_schema_min") != target or contract.get("supported_schema_max") != target:
        fail(f"{component} frozen epoch must keep supported_min = target = supported_max")
    for field in ("migration_history_digest", "compatibility_policy_digest"):
        value = contract.get(field)
        if not isinstance(value, str) or not value:
            fail(f"{component} schema contract lacks {field}")


def synthetic_payload(component: str) -> dict[str, Any]:
    return {
        "binary_bits_sha256": "a" * 64,
        "configuration_sha256": "b" * 64,
        "schema_contract": {
            "database_component": component,
            "target_schema_revision": "0001_fixture.sql",
            "supported_schema_min": "0001_fixture.sql",
            "supported_schema_max": "0001_fixture.sql",
            "migration_history_digest": "c" * 64,
            "compatibility_policy_digest": "d" * 64,
        },
    }


def mutate_policy(payload: dict[str, Any]) -> dict[str, Any]:
    mutated = {
        **payload,
        "schema_contract": dict(payload["schema_contract"]),
    }
    mutated["schema_contract"]["compatibility_policy_digest"] = "e" * 64
    return mutated


def validate(root: Path = ROOT) -> None:
    catalog_path = root / CATALOG_RELEASE.relative_to(ROOT)
    resolver_path = root / RESOLVER_RELEASE.relative_to(ROOT)
    catalog_builder = function_node(catalog_path, "build_manifest_payload")
    catalog_finalize = function_node(catalog_path, "finalized_manifest")
    resolver_builder = function_node(resolver_path, "manifest_for")

    if not dict_contains_schema_contract(catalog_builder):
        fail("Catalog manifest builder must embed load_schema_contract(root) before release ID hashing")
    if not call_hashes_payload(catalog_finalize, "release_id_for"):
        fail("Catalog finalized manifest must derive release_id from the complete payload")
    if not dict_contains_schema_contract(resolver_builder):
        fail("Resolver manifest builder must embed load_schema_contract(root) before release ID hashing")
    if not resolver_hashes_canonical_payload(resolver_builder):
        fail("Resolver manifest builder must hash the complete canonical payload")

    catalog = load_module(catalog_path, "ar9_catalog_release")
    resolver = load_module(resolver_path, "ar9_resolver_release")
    conservative_window(catalog.load_schema_contract(root), "catalog")
    conservative_window(resolver.load_schema_contract(root), "resolver")

    catalog_payload = synthetic_payload("catalog")
    catalog_mutated = mutate_policy(catalog_payload)
    catalog_before = catalog.release_id_for(catalog_payload)
    catalog_after = catalog.release_id_for(catalog_mutated)
    if catalog_before == catalog_after:
        fail("Catalog schema policy mutation did not change immutable release ID")

    resolver_payload = synthetic_payload("resolver")
    resolver_mutated = mutate_policy(resolver_payload)
    resolver_before = resolver.RELEASE_PREFIX + resolver.sha256_bytes(resolver.canonical(resolver_payload))
    resolver_after = resolver.RELEASE_PREFIX + resolver.sha256_bytes(resolver.canonical(resolver_mutated))
    if resolver_before == resolver_after:
        fail("Resolver schema policy mutation did not change immutable release ID")


def self_test() -> None:
    validate(ROOT)
    catalog = load_module(CATALOG_RELEASE, "ar9_catalog_release_negative")
    payload = synthetic_payload("catalog")
    changed_binary = {**payload, "binary_bits_sha256": "f" * 64}
    if catalog.release_id_for(payload) == catalog.release_id_for(changed_binary):
        fail("release identity negative fixture did not bind binary bits")
    print("D1 release schema-identity negative fixture passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        validate(ROOT)
        print(
            "Catalog and Resolver immutable release identities bind target/min/max revision, history digest, "
            "compatibility-policy digest, and change when policy changes with binary bits held constant."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SchemaIdentityError as error:
        print(f"D1 release schema identity error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
