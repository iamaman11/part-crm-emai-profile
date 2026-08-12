#!/usr/bin/env python3
"""Generate the one-shot B4 mailbox Client association fragment and TS DTOs."""

from __future__ import annotations

import argparse
import difflib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
OPENAPI_PATH = ROOT / "openapi/v1/fragments/mailbox-client-association.json"
TYPESCRIPT_PATH = ROOT / "frontend/src/shared/api/generated/mailbox-client-association.ts"
SOURCE_PATH = "crates/control-plane-contract/src/mailbox_client_association_api.rs"
GENERATOR_PATH = "scripts/generate-mailbox-client-association-contract.py"


def export_document() -> dict[str, Any]:
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--locked",
            "-p",
            "control-plane-contract",
            "--bin",
            "export_mailbox_client_association",
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
    value = json.loads(completed.stdout)
    if not isinstance(value, dict):
        raise ValueError("association exporter must return an OpenAPI fragment object")
    return value


def schema_type(schema: dict[str, Any]) -> str:
    reference = schema.get("$ref")
    if isinstance(reference, str):
        prefix = "#/components/schemas/"
        if not reference.startswith(prefix):
            raise ValueError(f"unsupported reference: {reference}")
        rendered = reference[len(prefix):]
    elif schema.get("type") == "string":
        enum = schema.get("enum")
        if isinstance(enum, list) and enum and all(isinstance(value, str) for value in enum):
            rendered = " | ".join(json.dumps(value) for value in enum)
        else:
            rendered = "string"
    elif schema.get("type") in {"integer", "number"}:
        rendered = "number"
    elif schema.get("type") == "boolean":
        rendered = "boolean"
    else:
        raise ValueError(f"unsupported association schema property: {schema!r}")
    if schema.get("nullable") is True:
        rendered += " | null"
    return rendered


def typescript(document: dict[str, Any]) -> str:
    schemas = document.get("components", {}).get("schemas", {})
    if not isinstance(schemas, dict) or not schemas:
        raise ValueError("association fragment is missing schemas")
    lines = [
        "// GENERATED FILE — DO NOT EDIT.",
        f"// Canonical Rust source: {SOURCE_PATH}",
        f"// Generated through: {GENERATOR_PATH}",
        "// Regenerate with: python scripts/generate-mailbox-client-association-contract.py",
        "",
    ]
    for name in sorted(schemas):
        schema = schemas[name]
        if not isinstance(schema, dict) or schema.get("type") != "object":
            raise ValueError(f"top-level association schema {name} must be an object")
        properties = schema.get("properties")
        required = schema.get("required", [])
        if not isinstance(properties, dict) or not isinstance(required, list):
            raise ValueError(f"association schema {name} is malformed")
        required_set = set(required)
        lines.append(f"export interface {name} {{")
        for field in sorted(properties):
            field_schema = properties[field]
            if not isinstance(field_schema, dict):
                raise ValueError(f"association property {name}.{field} is malformed")
            optional = "" if field in required_set else "?"
            lines.append(f"  {field}{optional}: {schema_type(field_schema)};")
        lines.extend(["}", ""])
    return "\n".join(lines).rstrip() + "\n"


def openapi(document: dict[str, Any]) -> str:
    return json.dumps(document, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n"


def diff(path: Path, actual: str, expected: str) -> None:
    print(
        "".join(
            difflib.unified_diff(
                actual.splitlines(keepends=True),
                expected.splitlines(keepends=True),
                fromfile=f"a/{path.relative_to(ROOT)}",
                tofile=f"b/{path.relative_to(ROOT)}",
            )
        ),
        file=sys.stderr,
        end="",
    )


def check_or_write(path: Path, expected: str, check: bool) -> bool:
    if check:
        actual = path.read_text(encoding="utf-8") if path.is_file() else ""
        if actual != expected:
            print(f"generated association contract drift: {path.relative_to(ROOT)}", file=sys.stderr)
            diff(path, actual, expected)
            return False
        return True
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(expected, encoding="utf-8", newline="\n")
    print(f"wrote {path.relative_to(ROOT)}")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    document = export_document()
    results = [
        check_or_write(OPENAPI_PATH, openapi(document), args.check),
        check_or_write(TYPESCRIPT_PATH, typescript(document), args.check),
    ]
    if not all(results):
        return 1
    print("mailbox Client association contract is deterministic and current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
