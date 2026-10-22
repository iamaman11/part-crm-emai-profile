#!/usr/bin/env python3
"""Project external compatibility facts into Release Compatibility Evidence v2.

Static release identities are intentionally absent: native opsctl verifies them directly from
immutable Release Set/source bytes. This adapter carries only provider/external facts that
cannot be derived locally: Catalog D1, Resolver D1, and Windows delivery compatibility.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

ZERO_SHA = "0" * 64
CURRENT_TARGET_ID = re.compile(r"release-set-v3-sha256-[0-9a-f]{64}")


class EvidenceError(ValueError):
    pass


def fail(message: str) -> None:
    raise EvidenceError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path, label: str) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise EvidenceError(f"{label} is invalid JSON: {error}") from error
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def d1_dimension(path: Path | None, component: str) -> dict[str, str]:
    if path is None:
        return {
            "decision": "UNKNOWN",
            "evidence_sha256": ZERO_SHA,
            "policy_source": "opsctl.d1.compatibility",
        }
    value = load_json(path, f"{component} D1 compatibility output")
    if (
        value.get("schema_version") != 2
        or value.get("command") != "d1 compatibility"
        or value.get("component") != component
        or value.get("mode") != "read-only"
        or value.get("mutation_executed") is not False
        or not isinstance(value.get("allowed"), bool)
    ):
        fail(f"{component} D1 evidence is not native opsctl compatibility output")
    runtime = value.get("runtime_compatibility")
    if (
        not isinstance(runtime, dict)
        or not isinstance(runtime.get("compatible"), bool)
        or not isinstance(runtime.get("reason_codes"), list)
        or any(not isinstance(reason, str) or not reason for reason in runtime["reason_codes"])
        or not isinstance(runtime.get("required_steps"), list)
        or any(not isinstance(step, str) or not step for step in runtime["required_steps"])
        or (not runtime["compatible"] and not runtime["reason_codes"])
        or (runtime["compatible"] and not value["allowed"])
    ):
        fail(f"{component} D1 runtime compatibility verdict is missing or invalid")
    return {
        "decision": "COMPATIBLE" if runtime["compatible"] else "INCOMPATIBLE",
        "evidence_sha256": sha256_file(path),
        "policy_source": "opsctl.d1.compatibility",
    }


def windows_dimension(path: Path | None) -> dict[str, str]:
    if path is None:
        return {
            "decision": "UNKNOWN",
            "evidence_sha256": ZERO_SHA,
            "policy_source": "external.windows.delivery",
        }
    value = load_json(path, "Windows delivery evidence")
    if value.get("schema_version") != 1 or value.get("kind") != "WINDOWS_PROFILE_BRIDGE_DELIVERY_EVIDENCE":
        fail("Windows delivery evidence identity/version is invalid")
    decision = value.get("decision")
    if decision not in {"COMPATIBLE", "INCOMPATIBLE", "UNKNOWN"}:
        fail("Windows delivery evidence decision is invalid")
    return {
        "decision": decision,
        "evidence_sha256": sha256_file(path),
        "policy_source": "external.windows.delivery",
    }


def validate_target_id(release_set_id: str) -> None:
    if CURRENT_TARGET_ID.fullmatch(release_set_id) is None:
        fail("release_set_id must be an exact current Release Set v3 target ID")


def build(release_set_id: str, catalog: Path, resolver: Path | None, windows: Path | None) -> dict[str, Any]:
    validate_target_id(release_set_id)
    return {
        "schema_version": 2,
        "kind": "RELEASE_COMPATIBILITY_EVIDENCE",
        "release_set_id": release_set_id,
        "dimensions": {
            "catalog_d1": d1_dimension(catalog, "catalog"),
            "resolver_d1": d1_dimension(resolver, "resolver"),
            "windows_profile_bridge": windows_dimension(windows),
        },
    }


def self_test() -> None:
    current = "release-set-v3-sha256-" + "a" * 64
    for invalid in (
        "release-set-v2-sha256-" + "a" * 64,
        "release-set-v1-sha256-" + "a" * 64,
        "release-set-v4-sha256-" + "a" * 64,
        "release-set-v3-sha256-deadbeef",
    ):
        try:
            validate_target_id(invalid)
        except EvidenceError:
            continue
        fail(f"non-current target identity unexpectedly accepted: {invalid}")
    try:
        validate_target_id(current)
    except EvidenceError as error:
        fail(f"current Release Set v3 target identity unexpectedly rejected: {error}")
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "d1.json"
        value = {
            "schema_version": 2, "command": "d1 compatibility", "component": "catalog",
            "mode": "read-only", "mutation_executed": False, "allowed": True,
            "decision": "MIGRATION_REQUIRED",
            "runtime_compatibility": {"compatible": False, "reason_codes": ["RUNTIME_SCHEMA_MIGRATION_REQUIRED"], "required_steps": []},
        }
        path.write_text(json.dumps(value), encoding="utf-8")
        assert d1_dimension(path, "catalog")["decision"] == "INCOMPATIBLE"
        value["runtime_compatibility"] = {"compatible": True, "reason_codes": [], "required_steps": []}
        path.write_text(json.dumps(value), encoding="utf-8")
        assert d1_dimension(path, "catalog")["decision"] == "COMPATIBLE"
        for invalid in [
            {**value, "schema_version": 1},
            {**value, "runtime_compatibility": {}},
            {**value, "runtime_compatibility": {"compatible": False, "reason_codes": [], "required_steps": []}},
            {**value, "runtime_compatibility": {"compatible": True, "reason_codes": []}},
            {**value, "allowed": False},
        ]:
            path.write_text(json.dumps(invalid), encoding="utf-8")
            try:
                d1_dimension(path, "catalog")
            except EvidenceError:
                continue
            fail("old, missing or contradictory D1 runtime evidence unexpectedly accepted")
    print("AR-11 compatibility evidence v2 adapter self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release-set-id")
    parser.add_argument("--catalog-d1", type=Path)
    parser.add_argument("--resolver-d1", type=Path)
    parser.add_argument("--windows-delivery", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        if not args.release_set_id or args.catalog_d1 is None or args.output is None:
            fail("release-set-id, catalog-d1 and output are required")
        output = build(args.release_set_id, args.catalog_d1, args.resolver_d1, args.windows_delivery)
        if args.output.exists():
            fail(f"output already exists: {args.output}")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(output, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 0
    except (OSError, EvidenceError) as error:
        print(f"AR-11 compatibility evidence v2 error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
