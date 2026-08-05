#!/usr/bin/env python3
"""Lint v1 contract roots and reject backwards-incompatible removals."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

HTTP_METHODS = {"get", "put", "post", "delete", "patch", "options", "head", "trace"}
PACKAGE_RE = re.compile(r"\bpackage\s+([A-Za-z0-9_.]+)\s*;")
MESSAGE_RE = re.compile(r"\bmessage\s+([A-Za-z0-9_]+)\s*\{(.*?)\}", re.DOTALL)
FIELD_RE = re.compile(
    r"^\s*(?:repeated\s+|optional\s+)?[A-Za-z0-9_.<>]+\s+([A-Za-z0-9_]+)\s*=\s*([1-9][0-9]*)\s*;",
    re.MULTILINE,
)


@dataclass(frozen=True)
class ProtoContract:
    package: str
    messages: dict[str, dict[int, str]]


def load_json(path: Path) -> dict[str, object]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def lint_openapi(document: dict[str, object], path: Path) -> list[str]:
    errors: list[str] = []
    if not str(document.get("openapi", "")).startswith("3.1."):
        errors.append(f"{path}: OpenAPI version must be 3.1.x")
    info = document.get("info")
    if not isinstance(info, dict) or not str(info.get("version", "")).startswith("1."):
        errors.append(f"{path}: info.version must remain in v1")
    paths = document.get("paths")
    if not isinstance(paths, dict):
        return errors + [f"{path}: paths must be an object"]

    operation_ids: set[str] = set()
    for route, item in paths.items():
        if not str(route).startswith("/api/v1/"):
            errors.append(f"{path}: unversioned route {route}")
        if not isinstance(item, dict):
            errors.append(f"{path}: route {route} must be an object")
            continue
        for method, operation in item.items():
            if method not in HTTP_METHODS or not isinstance(operation, dict):
                continue
            operation_id = operation.get("operationId")
            if not isinstance(operation_id, str) or not operation_id:
                errors.append(f"{path}: {method.upper()} {route} lacks operationId")
            elif operation_id in operation_ids:
                errors.append(f"{path}: duplicate operationId {operation_id}")
            else:
                operation_ids.add(operation_id)
    return errors


def compare_openapi(
    baseline: dict[str, object], current: dict[str, object], path: Path
) -> list[str]:
    errors: list[str] = []
    baseline_paths = baseline.get("paths", {})
    current_paths = current.get("paths", {})
    if not isinstance(baseline_paths, dict) or not isinstance(current_paths, dict):
        return [f"{path}: invalid paths shape"]

    for route, baseline_item in baseline_paths.items():
        current_item = current_paths.get(route)
        if not isinstance(baseline_item, dict) or not isinstance(current_item, dict):
            errors.append(f"{path}: removed path {route}")
            continue
        for method, baseline_operation in baseline_item.items():
            if method not in HTTP_METHODS or not isinstance(baseline_operation, dict):
                continue
            current_operation = current_item.get(method)
            if not isinstance(current_operation, dict):
                errors.append(f"{path}: removed operation {method.upper()} {route}")
                continue
            if current_operation.get("operationId") != baseline_operation.get("operationId"):
                errors.append(f"{path}: changed operationId for {method.upper()} {route}")

    baseline_schemas = (
        baseline.get("components", {}).get("schemas", {})
        if isinstance(baseline.get("components"), dict)
        else {}
    )
    current_schemas = (
        current.get("components", {}).get("schemas", {})
        if isinstance(current.get("components"), dict)
        else {}
    )
    if isinstance(baseline_schemas, dict) and isinstance(current_schemas, dict):
        for name, baseline_schema in baseline_schemas.items():
            current_schema = current_schemas.get(name)
            if not isinstance(baseline_schema, dict) or not isinstance(current_schema, dict):
                errors.append(f"{path}: removed schema {name}")
                continue
            baseline_properties = baseline_schema.get("properties", {})
            current_properties = current_schema.get("properties", {})
            if isinstance(baseline_properties, dict) and isinstance(current_properties, dict):
                removed = set(baseline_properties) - set(current_properties)
                if removed:
                    errors.append(f"{path}: schema {name} removed properties {sorted(removed)}")
            baseline_required = set(baseline_schema.get("required", []))
            current_required = set(current_schema.get("required", []))
            if not baseline_required.issubset(current_required):
                errors.append(f"{path}: schema {name} removed required properties")
    return errors


def parse_proto(path: Path) -> tuple[ProtoContract | None, list[str]]:
    text = path.read_text(encoding="utf-8")
    errors: list[str] = []
    if 'syntax = "proto3";' not in text:
        errors.append(f"{path}: syntax must be proto3")
    package_match = PACKAGE_RE.search(text)
    if package_match is None:
        return None, errors + [f"{path}: missing package"]
    package = package_match.group(1)
    if not package.endswith(".v1"):
        errors.append(f"{path}: package must end in .v1")

    messages: dict[str, dict[int, str]] = {}
    for message_match in MESSAGE_RE.finditer(text):
        message_name = message_match.group(1)
        fields: dict[int, str] = {}
        names: set[str] = set()
        for field_match in FIELD_RE.finditer(message_match.group(2)):
            field_name = field_match.group(1)
            field_number = int(field_match.group(2))
            if field_number in fields:
                errors.append(
                    f"{path}: message {message_name} duplicates field number {field_number}"
                )
            if field_name in names:
                errors.append(f"{path}: message {message_name} duplicates field {field_name}")
            fields[field_number] = field_name
            names.add(field_name)
        messages[message_name] = fields
    if not messages:
        errors.append(f"{path}: no messages found")
    return ProtoContract(package=package, messages=messages), errors


def load_proto_tree(root: Path) -> tuple[dict[Path, ProtoContract], list[str]]:
    contracts: dict[Path, ProtoContract] = {}
    errors: list[str] = []
    proto_root = root / "proto"
    for path in sorted(proto_root.rglob("*.proto")):
        contract, parse_errors = parse_proto(path)
        errors.extend(parse_errors)
        if contract is not None:
            contracts[path.relative_to(proto_root)] = contract
    if not contracts:
        errors.append(f"{proto_root}: no protobuf contracts found")
    return contracts, errors


def compare_proto(
    baseline: dict[Path, ProtoContract], current: dict[Path, ProtoContract]
) -> list[str]:
    errors: list[str] = []
    for relative_path, baseline_contract in baseline.items():
        current_contract = current.get(relative_path)
        if current_contract is None:
            errors.append(f"removed protobuf file {relative_path}")
            continue
        if current_contract.package != baseline_contract.package:
            errors.append(f"{relative_path}: package changed")
        for message_name, baseline_fields in baseline_contract.messages.items():
            current_fields = current_contract.messages.get(message_name)
            if current_fields is None:
                errors.append(f"{relative_path}: removed message {message_name}")
                continue
            for field_number, field_name in baseline_fields.items():
                if current_fields.get(field_number) != field_name:
                    errors.append(
                        f"{relative_path}: removed/renamed {message_name}.{field_name} = {field_number}"
                    )
    return errors


def check(current_root: Path, baseline_root: Path) -> list[str]:
    errors: list[str] = []
    current_openapi_path = current_root / "openapi/v1/openapi.json"
    baseline_openapi_path = baseline_root / "openapi/v1/openapi.json"
    try:
        current_openapi = load_json(current_openapi_path)
        baseline_openapi = load_json(baseline_openapi_path)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        return [str(error)]

    errors.extend(lint_openapi(current_openapi, current_openapi_path))
    errors.extend(compare_openapi(baseline_openapi, current_openapi, current_openapi_path))

    current_proto, current_errors = load_proto_tree(current_root)
    baseline_proto, baseline_errors = load_proto_tree(baseline_root)
    errors.extend(current_errors)
    errors.extend(baseline_errors)
    errors.extend(compare_proto(baseline_proto, current_proto))
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--current-root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--baseline-root", type=Path, default=Path.cwd() / "contracts/baseline"
    )
    args = parser.parse_args()

    errors = check(args.current_root, args.baseline_root)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("OpenAPI and protobuf v1 contracts are backwards compatible.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
