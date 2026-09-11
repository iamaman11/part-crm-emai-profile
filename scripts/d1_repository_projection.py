#!/usr/bin/env python3
"""Read the typed, SQL-derived D1 repository projection from opsctl.

This module is an outer build/tooling adapter only. It owns no D1 policy, migration
catalog algorithm, historical digest, provider access, or mutation capability.
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any

SOURCE_ROOT = Path(__file__).resolve().parents[1]


class D1ProjectionError(ValueError):
    """The canonical typed D1 projection could not be obtained or validated."""


def _command(repository_root: Path) -> list[str]:
    binary = os.environ.get("OPSCTL_BIN")
    if binary:
        return [binary, "--root", str(repository_root), "d1", "repository"]
    return [
        "cargo",
        "run",
        "--locked",
        "--quiet",
        "--manifest-path",
        str(SOURCE_ROOT / "tools/opsctl/Cargo.toml"),
        "--",
        "--root",
        str(repository_root),
        "d1",
        "repository",
    ]


def load(repository_root: Path) -> dict[str, Any]:
    result = subprocess.run(
        _command(repository_root.resolve()),
        cwd=SOURCE_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        details = "\n".join(
            value.strip() for value in (result.stdout, result.stderr) if value.strip()
        )
        raise D1ProjectionError(details or "opsctl d1 repository failed")
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise D1ProjectionError("opsctl d1 repository returned malformed JSON") from error
    if not isinstance(value, dict):
        raise D1ProjectionError("opsctl d1 repository must return one JSON object")
    if value.get("schema_version") != 1 or value.get("kind") != "D1_REPOSITORY_PROJECTION":
        raise D1ProjectionError("opsctl D1 repository projection identity/version mismatch")
    digest = value.get("repository_identity_sha256")
    if not isinstance(digest, str) or len(digest) != 64:
        raise D1ProjectionError("opsctl D1 repository projection lacks its typed identity")
    components = value.get("components")
    if not isinstance(components, list) or len(components) != 2:
        raise D1ProjectionError("opsctl D1 repository projection must contain two components")
    return value


def component(repository_root: Path, component_id: str) -> dict[str, Any]:
    return component_from_projection(load(repository_root), component_id)


def component_from_projection(value: dict[str, Any], component_id: str) -> dict[str, Any]:
    matches = [
        entry
        for entry in value["components"]
        if isinstance(entry, dict) and entry.get("component_id") == component_id
    ]
    if len(matches) != 1:
        raise D1ProjectionError(
            f"opsctl D1 repository projection must contain exactly one {component_id} component"
        )
    return matches[0]


def release_contract(repository_root: Path, component_id: str) -> dict[str, str]:
    return release_contract_from_projection(load(repository_root), component_id)


def release_contract_from_projection(
    value: dict[str, Any], component_id: str
) -> dict[str, str]:
    selected = component_from_projection(value, component_id)
    value = selected.get("release_schema_contract")
    if not isinstance(value, dict):
        raise D1ProjectionError(f"typed D1 {component_id} release schema contract is missing")
    required = {
        "database_component",
        "target_schema_revision",
        "supported_schema_min",
        "supported_schema_max",
        "migration_history_digest",
        "compatibility_policy_digest",
    }
    if set(value) != required or any(
        not isinstance(value.get(field), str) or not value[field] for field in required
    ):
        raise D1ProjectionError(f"typed D1 {component_id} release schema contract is malformed")
    return value
