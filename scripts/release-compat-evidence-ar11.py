#!/usr/bin/env python3
"""Project native opsctl D1 results into the AR-11 compatibility evidence envelope.

This file is deliberately an adapter, not a policy engine. It never evaluates schema,
API, protocol, runtime, or rollout compatibility. D1 compatibility is copied from the
native `opsctl d1 compatibility` `allowed` result; static dimensions are transport-only
placeholders ignored by Rust; Windows delivery is copied from explicit external evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

STATIC_DIMENSIONS = (
    "public_api",
    "frontend_api",
    "resolver_protocol",
    "bridge_protocol",
    "camouhost_ipc",
    "runtime_bundle",
    "profile_format",
    "browser_identity_policy",
)
ZERO_SHA = "0" * 64


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
        value.get("schema_version") != 1
        or value.get("command") != "d1 compatibility"
        or value.get("component") != component
        or value.get("mode") != "read-only"
        or value.get("mutation_executed") is not False
        or not isinstance(value.get("allowed"), bool)
    ):
        fail(f"{component} D1 evidence is not native opsctl compatibility output")
    return {
        "decision": "COMPATIBLE" if value["allowed"] else "INCOMPATIBLE",
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


def build(
    release_set_id: str,
    catalog: Path,
    resolver: Path | None,
    windows: Path | None,
) -> dict[str, Any]:
    if not release_set_id.startswith("release-set-v1-sha256-"):
        fail("release_set_id must be a v1 content-addressed Release Set ID")
    dimensions: dict[str, Any] = {
        "catalog_d1": d1_dimension(catalog, "catalog"),
        "resolver_d1": d1_dimension(resolver, "resolver"),
        "windows_profile_bridge": windows_dimension(windows),
    }
    for name in STATIC_DIMENSIONS:
        dimensions[name] = {
            "decision": "UNKNOWN",
            "evidence_sha256": ZERO_SHA,
            "policy_source": "transport-only;opsctl.release.compatibility-is-authority",
        }
    return {
        "schema_version": 1,
        "kind": "RELEASE_COMPATIBILITY_EVIDENCE",
        "release_set_id": release_set_id,
        "dimensions": dimensions,
    }


def self_test() -> None:
    if len(STATIC_DIMENSIONS) != 8 or "catalog_d1" in STATIC_DIMENSIONS:
        fail("static/external compatibility boundary drifted")
    print("AR-11 compatibility evidence adapter self-test passed.")


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
        print(f"AR-11 compatibility evidence error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
