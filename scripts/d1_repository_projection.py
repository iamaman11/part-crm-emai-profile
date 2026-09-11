#!/usr/bin/env python3
"""Read the typed, SQL-derived D1 repository projection from opsctl.

This module is an outer build/tooling adapter only. It owns no D1 policy, migration
catalog algorithm, historical digest, provider access, or mutation capability.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
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


def validate(value: Any) -> dict[str, Any]:
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
    return validate(value)


def component(repository_root: Path, component_id: str) -> dict[str, Any]:
    return component_from_projection(load(repository_root), component_id)


def component_from_projection(value: dict[str, Any], component_id: str) -> dict[str, Any]:
    value = validate(value)
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


def executable_schema_authority_from_projection(value: dict[str, Any]) -> tuple[str, ...]:
    value = validate(value)
    authority = value.get("executable_schema_authority")
    if not isinstance(authority, list) or not authority:
        raise D1ProjectionError("typed D1 executable_schema_authority is missing")
    roots: list[str] = []
    for root in authority:
        if not isinstance(root, str) or not root:
            raise D1ProjectionError("typed D1 executable_schema_authority is malformed")
        path = Path(root)
        if path.is_absolute() or ".." in path.parts or "." in path.parts:
            raise D1ProjectionError("typed D1 executable_schema_authority contains unsafe root")
        roots.append(root)
    if len(set(roots)) != len(roots):
        raise D1ProjectionError("typed D1 executable_schema_authority contains duplicates")
    return tuple(roots)


def executable_migration_sources_from_projection(
    value: dict[str, Any], component_id: str
) -> tuple[tuple[str, str], ...]:
    governed_roots = set(executable_schema_authority_from_projection(value))
    selected = component_from_projection(value, component_id)
    sources = selected.get("executable_migration_sources")
    if not isinstance(sources, list) or not sources:
        raise D1ProjectionError(
            f"typed D1 {component_id} executable_migration_sources is missing"
        )
    result: list[tuple[str, str]] = []
    for source in sources:
        if not isinstance(source, dict):
            raise D1ProjectionError(
                f"typed D1 {component_id} executable migration source is malformed"
            )
        root = source.get("source_root")
        name = source.get("migration_file")
        if not isinstance(root, str) or root not in governed_roots:
            raise D1ProjectionError(
                f"typed D1 {component_id} migration source escaped executable schema authority"
            )
        if not isinstance(name, str) or not name or Path(name).name != name:
            raise D1ProjectionError(
                f"typed D1 {component_id} migration filename is unsafe"
            )
        result.append((root, name))
    if len(set(result)) != len(result):
        raise D1ProjectionError(
            f"typed D1 {component_id} executable migration sources contain duplicates"
        )
    return tuple(result)


def materialize_executable_migrations_from_projection(
    repository_root: Path,
    value: dict[str, Any],
    component_id: str,
    output_directory: Path,
) -> None:
    repository_root = repository_root.resolve()
    output_directory.mkdir(parents=True, exist_ok=True)
    for root, name in executable_migration_sources_from_projection(value, component_id):
        source = repository_root / root / name
        if source.is_symlink() or not source.is_file():
            raise D1ProjectionError(
                f"typed D1 {component_id} migration source is not a regular file: {source}"
            )
        resolved_source = source.resolve()
        try:
            resolved_source.relative_to(repository_root)
        except ValueError as error:
            raise D1ProjectionError(
                f"typed D1 {component_id} migration source escaped repository root"
            ) from error
        target = output_directory / name
        if target.exists():
            raise D1ProjectionError(
                f"typed D1 {component_id} migration materialization collision: {name}"
            )
        shutil.copyfile(resolved_source, target)


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


def _main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    materialize = subparsers.add_parser("materialize")
    materialize.add_argument("--projection", type=Path, required=True)
    materialize.add_argument("--repository-root", type=Path, required=True)
    materialize.add_argument("--component", required=True)
    materialize.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "materialize":
        try:
            value = validate(json.loads(args.projection.read_text(encoding="utf-8")))
            materialize_executable_migrations_from_projection(
                args.repository_root, value, args.component, args.output
            )
        except (D1ProjectionError, json.JSONDecodeError, OSError) as error:
            parser.error(str(error))
        return 0
    raise AssertionError("unreachable")


if __name__ == "__main__":
    raise SystemExit(_main())
