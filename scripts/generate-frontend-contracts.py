#!/usr/bin/env python3
"""Generate committed OpenAPI and TypeScript from the canonical Rust public contract.

The generator is repository-owned and dependency-free beyond the pinned Rust/Python toolchains.
It never edits generated output by hand: Rust emits OpenAPI, then this script deterministically
renders TypeScript from OpenAPI components. `--check` fails closed on any committed drift.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
OPENAPI_PATH = ROOT / "contracts" / "generated" / "control-plane.openapi.json"
TYPESCRIPT_PATH = ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "control-plane.ts"
SOURCE_PATH = "crates/control-plane-contract/src/public_api.rs"
GENERATOR_PATH = "scripts/generate-frontend-contracts.py"


def export_openapi() -> tuple[dict[str, Any], str]:
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--locked",
            "-p",
            "control-plane-contract",
            "--bin",
            "export_openapi",
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        if completed.stdout:
            print(completed.stdout, file=sys.stderr, end="")
        if completed.stderr:
            print(completed.stderr, file=sys.stderr, end="")
        raise SystemExit(completed.returncode)

    document = json.loads(completed.stdout)
    rendered = json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    return document, rendered


def schema_type(schema: dict[str, Any]) -> str:
    reference = schema.get("$ref")
    if isinstance(reference, str):
        prefix = "#/components/schemas/"
        if not reference.startswith(prefix):
            raise ValueError(f"unsupported schema reference: {reference}")
        return reference[len(prefix) :]

    schema_kind = schema.get("type")
    if schema_kind == "string":
        return "string"
    if schema_kind in {"integer", "number"}:
        return "number"
    if schema_kind == "boolean":
        return "boolean"
    if schema_kind == "array":
        items = schema.get("items")
        if not isinstance(items, dict):
            raise ValueError("array schema is missing items")
        return f"ReadonlyArray<{schema_type(items)}>"
    if schema_kind == "object":
        additional = schema.get("additionalProperties")
        if isinstance(additional, dict):
            return f"Record<string, {schema_type(additional)}>"
        return "Record<string, unknown>"
    raise ValueError(f"unsupported schema: {schema!r}")


def render_enum(name: str, schema: dict[str, Any]) -> list[str]:
    raw_values = schema.get("enum")
    if not isinstance(raw_values, list) or not raw_values or not all(isinstance(value, str) for value in raw_values):
        raise ValueError(f"string enum {name} has invalid values")
    values = ", ".join(json.dumps(value, ensure_ascii=False) for value in raw_values)
    return [
        f"export const {name}Values = [{values}] as const;",
        f"export type {name} = (typeof {name}Values)[number];",
        "",
    ]


def property_name(name: str) -> str:
    if name.isidentifier():
        return name
    return json.dumps(name, ensure_ascii=False)


def render_object(name: str, schema: dict[str, Any]) -> list[str]:
    properties = schema.get("properties")
    if not isinstance(properties, dict):
        raise ValueError(f"object schema {name} has no properties")
    raw_required = schema.get("required", [])
    if not isinstance(raw_required, list) or not all(isinstance(value, str) for value in raw_required):
        raise ValueError(f"object schema {name} has invalid required list")
    required = set(raw_required)

    lines = [f"export interface {name} {{"]
    for field_name in sorted(properties):
        field_schema = properties[field_name]
        if not isinstance(field_schema, dict):
            raise ValueError(f"property {name}.{field_name} has invalid schema")
        optional = "" if field_name in required else "?"
        lines.append(f"  {property_name(field_name)}{optional}: {schema_type(field_schema)};")
    lines.extend(["}", ""])
    return lines


def render_typescript(document: dict[str, Any]) -> str:
    schemas = document.get("components", {}).get("schemas", {})
    if not isinstance(schemas, dict) or not schemas:
        raise ValueError("OpenAPI components.schemas is empty")

    lines = [
        "// GENERATED FILE — DO NOT EDIT.",
        f"// Canonical Rust source: {SOURCE_PATH}",
        f"// Generated through: {GENERATOR_PATH}",
        "// Regenerate with: python scripts/generate-frontend-contracts.py",
        "",
    ]

    for name in sorted(schemas):
        schema = schemas[name]
        if not isinstance(schema, dict):
            raise ValueError(f"schema {name} is invalid")
        if schema.get("type") == "string" and "enum" in schema:
            lines.extend(render_enum(name, schema))
        elif schema.get("type") == "object":
            lines.extend(render_object(name, schema))
        else:
            raise ValueError(f"top-level schema {name} is unsupported")

    return "\n".join(lines).rstrip() + "\n"


def check_or_write(path: Path, expected: str, check: bool) -> bool:
    if check:
        try:
            actual = path.read_text(encoding="utf-8")
        except FileNotFoundError:
            print(f"generated contract is missing: {path.relative_to(ROOT)}", file=sys.stderr)
            return False
        if actual != expected:
            print(
                f"generated contract drift: {path.relative_to(ROOT)}; run python scripts/generate-frontend-contracts.py",
                file=sys.stderr,
            )
            return False
        return True

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(expected, encoding="utf-8", newline="\n")
    print(f"wrote {path.relative_to(ROOT)}")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if committed generated files differ")
    args = parser.parse_args()

    document, openapi = export_openapi()
    typescript = render_typescript(document)
    results = [
        check_or_write(OPENAPI_PATH, openapi, args.check),
        check_or_write(TYPESCRIPT_PATH, typescript, args.check),
    ]
    if not all(results):
        return 1
    print("frontend contracts are deterministic and current" if args.check else "frontend contracts generated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
