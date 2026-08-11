#!/usr/bin/env python3
"""Generate committed OpenAPI and TypeScript from canonical Rust contracts.

The accepted base public contract remains byte-stable. Capability-owned additive surfaces are emitted
as generated OpenAPI artifacts plus separate generated TypeScript modules. `--check` fails closed and
prints a unified diff for drift so fixes never require temporary generation workflows.
"""

from __future__ import annotations

import argparse
import difflib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
OPENAPI_PATH = ROOT / "contracts" / "generated" / "control-plane.openapi.json"
TYPESCRIPT_PATH = ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "control-plane.ts"
CLIENT_REGISTRY_OPENAPI_PATH = ROOT / "openapi" / "v1" / "fragments" / "client-registry.json"
CLIENT_REGISTRY_TYPESCRIPT_PATH = (
    ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "client-registry.ts"
)
QUERY_MAIL_OPENAPI_PATH = ROOT / "openapi" / "v1" / "fragments" / "query-mail.json"
QUERY_MAIL_TYPESCRIPT_PATH = (
    ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "query-mail.ts"
)
OPERATOR_QUERY_OPENAPI_PATH = ROOT / "openapi" / "v1" / "fragments" / "operator-query.json"
OPERATOR_QUERY_TYPESCRIPT_PATH = (
    ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "operator-query.ts"
)
PROFILE_GENERATION_OPENAPI_PATH = ROOT / "contracts" / "generated" / "profile-generation.openapi.json"
PROFILE_GENERATION_TYPESCRIPT_PATH = (
    ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "profile-generation.ts"
)
MAILBOX_OPENAPI_PATH = ROOT / "contracts" / "generated" / "mailbox.openapi.json"
MAILBOX_TYPESCRIPT_PATH = (
    ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "mailbox.ts"
)
COORDINATOR_OPENAPI_PATH = ROOT / "contracts" / "generated" / "coordinator.openapi.json"
COORDINATOR_TYPESCRIPT_PATH = (
    ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "coordinator.ts"
)
SOURCE_PATH = "crates/control-plane-contract/src/public_api.rs"
CLIENT_REGISTRY_SOURCE_PATH = "crates/control-plane-contract/src/client_registry_api.rs"
QUERY_MAIL_SOURCE_PATH = "crates/control-plane-contract/src/bin/export_query_mail.rs"
OPERATOR_QUERY_SOURCE_PATH = "crates/control-plane-contract/src/bin/export_operator_query.rs"
PROFILE_GENERATION_SOURCE_PATH = "crates/control-plane-contract/src/profile_generation_api.rs"
MAILBOX_SOURCE_PATH = "crates/control-plane-contract/src/mailbox_api.rs"
COORDINATOR_SOURCE_PATH = "crates/control-plane-contract/src/coordinator_api.rs"
GENERATOR_PATH = "scripts/generate-frontend-contracts.py"


def run_export(bin_name: str, *arguments: str) -> tuple[dict[str, Any], str]:
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--locked",
            "-p",
            "control-plane-contract",
            "--bin",
            bin_name,
            *(["--", *arguments] if arguments else []),
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


def schema_type(schema: dict[str, Any], *, support_nullable: bool = False) -> str:
    reference = schema.get("$ref")
    if isinstance(reference, str):
        prefix = "#/components/schemas/"
        if not reference.startswith(prefix):
            raise ValueError(f"unsupported schema reference: {reference}")
        rendered = reference[len(prefix) :]
    else:
        schema_kind = schema.get("type")
        if schema_kind == "string":
            raw_values = schema.get("enum")
            if isinstance(raw_values, list) and len(raw_values) == 1 and isinstance(raw_values[0], str):
                rendered = json.dumps(raw_values[0], ensure_ascii=False)
            else:
                rendered = "string"
        elif schema_kind in {"integer", "number"}:
            rendered = "number"
        elif schema_kind == "boolean":
            rendered = "boolean"
        elif schema_kind == "array":
            items = schema.get("items")
            if not isinstance(items, dict):
                raise ValueError("array schema is missing items")
            rendered = f"ReadonlyArray<{schema_type(items, support_nullable=support_nullable)}>"
        elif schema_kind == "object":
            additional = schema.get("additionalProperties")
            if isinstance(additional, dict):
                rendered = (
                    "Record<string, "
                    f"{schema_type(additional, support_nullable=support_nullable)}>"
                )
            else:
                rendered = "Record<string, unknown>"
        else:
            raise ValueError(f"unsupported schema: {schema!r}")

    if support_nullable and schema.get("nullable") is True:
        return f"{rendered} | null"
    return rendered


def render_enum(name: str, schema: dict[str, Any]) -> list[str]:
    raw_values = schema.get("enum")
    if not isinstance(raw_values, list) or not raw_values or not all(
        isinstance(value, str) for value in raw_values
    ):
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


def render_object(
    name: str,
    schema: dict[str, Any],
    *,
    support_nullable: bool = False,
) -> list[str]:
    properties = schema.get("properties")
    if not isinstance(properties, dict):
        raise ValueError(f"object schema {name} has no properties")
    raw_required = schema.get("required", [])
    if not isinstance(raw_required, list) or not all(
        isinstance(value, str) for value in raw_required
    ):
        raise ValueError(f"object schema {name} has invalid required list")
    required = set(raw_required)

    lines = [f"export interface {name} {{"]
    for field_name in sorted(properties):
        field_schema = properties[field_name]
        if not isinstance(field_schema, dict):
            raise ValueError(f"property {name}.{field_name} has invalid schema")
        optional = "" if field_name in required else "?"
        lines.append(
            f"  {property_name(field_name)}{optional}: "
            f"{schema_type(field_schema, support_nullable=support_nullable)};"
        )
    lines.extend(["}", ""])
    return lines


def render_one_of(
    name: str,
    schema: dict[str, Any],
    *,
    support_nullable: bool = False,
) -> list[str]:
    variants = schema.get("oneOf")
    discriminator = schema.get("discriminator")
    if not isinstance(variants, list) or not variants or not all(
        isinstance(value, dict) for value in variants
    ):
        raise ValueError(f"oneOf schema {name} has invalid variants")
    if not isinstance(discriminator, dict):
        raise ValueError(f"oneOf schema {name} is missing discriminator")
    discriminator_name = discriminator.get("propertyName")
    if not isinstance(discriminator_name, str) or not discriminator_name:
        raise ValueError(f"oneOf schema {name} has invalid discriminator")

    lines = [f"export type {name} ="]
    observed_values: set[str] = set()
    for index, variant in enumerate(variants):
        if variant.get("type") != "object":
            raise ValueError(f"oneOf schema {name} variant {index} must be an object")
        properties = variant.get("properties")
        raw_required = variant.get("required", [])
        if not isinstance(properties, dict):
            raise ValueError(f"oneOf schema {name} variant {index} has no properties")
        if not isinstance(raw_required, list) or not all(
            isinstance(value, str) for value in raw_required
        ):
            raise ValueError(f"oneOf schema {name} variant {index} has invalid required list")
        required = set(raw_required)
        if discriminator_name not in required:
            raise ValueError(
                f"oneOf schema {name} variant {index} must require discriminator {discriminator_name}"
            )
        discriminator_schema = properties.get(discriminator_name)
        if not isinstance(discriminator_schema, dict):
            raise ValueError(
                f"oneOf schema {name} variant {index} is missing discriminator property"
            )
        discriminator_values = discriminator_schema.get("enum")
        if (
            discriminator_schema.get("type") != "string"
            or not isinstance(discriminator_values, list)
            or len(discriminator_values) != 1
            or not isinstance(discriminator_values[0], str)
        ):
            raise ValueError(
                f"oneOf schema {name} variant {index} discriminator must be one string literal"
            )
        discriminator_value = discriminator_values[0]
        if discriminator_value in observed_values:
            raise ValueError(
                f"oneOf schema {name} has duplicate discriminator value {discriminator_value}"
            )
        observed_values.add(discriminator_value)

        fields: list[str] = []
        for field_name in sorted(properties):
            field_schema = properties[field_name]
            if not isinstance(field_schema, dict):
                raise ValueError(
                    f"property {name} variant {index}.{field_name} has invalid schema"
                )
            optional = "" if field_name in required else "?"
            fields.append(
                f"{property_name(field_name)}{optional}: "
                f"{schema_type(field_schema, support_nullable=support_nullable)}"
            )
        lines.append(f"  | {{ {'; '.join(fields)} }}")
    lines[-1] += ";"
    lines.append("")
    return lines


def render_typescript(
    document: dict[str, Any],
    *,
    source_path: str,
    imports: tuple[str, ...] = (),
    support_nullable: bool = False,
) -> str:
    schemas = document.get("components", {}).get("schemas", {})
    if not isinstance(schemas, dict) or not schemas:
        raise ValueError("OpenAPI components.schemas is empty")

    lines = [
        "// GENERATED FILE — DO NOT EDIT.",
        f"// Canonical Rust source: {source_path}",
        f"// Generated through: {GENERATOR_PATH}",
        "// Regenerate with: python scripts/generate-frontend-contracts.py",
        "",
    ]
    if imports:
        lines.extend(imports)
        lines.append("")

    for name in sorted(schemas):
        schema = schemas[name]
        if not isinstance(schema, dict):
            raise ValueError(f"schema {name} is invalid")
        if schema.get("type") == "string" and "enum" in schema:
            lines.extend(render_enum(name, schema))
        elif "oneOf" in schema:
            lines.extend(
                render_one_of(name, schema, support_nullable=support_nullable)
            )
        elif schema.get("type") == "object":
            lines.extend(
                render_object(name, schema, support_nullable=support_nullable)
            )
        else:
            raise ValueError(f"top-level schema {name} is unsupported")

    return "\n".join(lines).rstrip() + "\n"


def self_test_discriminated_unions() -> None:
    valid = {
        "oneOf": [
            {
                "type": "object",
                "required": ["type", "value"],
                "properties": {
                    "type": {"type": "string", "enum": ["alpha"]},
                    "value": {"type": "integer"},
                },
            },
            {
                "type": "object",
                "required": ["type"],
                "properties": {
                    "type": {"type": "string", "enum": ["beta"]},
                },
            },
        ],
        "discriminator": {"propertyName": "type"},
    }
    rendered = "\n".join(render_one_of("Tagged", valid))
    if 'type: "alpha"' not in rendered or 'type: "beta"' not in rendered:
        raise ValueError("discriminated union self-test lost literal discriminator values")

    invalid = {
        "oneOf": [
            {
                "type": "object",
                "required": ["type"],
                "properties": {
                    "type": {"type": "string", "enum": ["alpha", "beta"]},
                },
            }
        ],
        "discriminator": {"propertyName": "type"},
    }
    try:
        render_one_of("InvalidTagged", invalid)
    except ValueError:
        pass
    else:
        raise ValueError("discriminated union negative self-test accepted a non-literal discriminator")


def print_diff(path: Path, actual: str, expected: str) -> None:
    display = str(path.relative_to(ROOT))
    diff = difflib.unified_diff(
        actual.splitlines(keepends=True),
        expected.splitlines(keepends=True),
        fromfile=f"a/{display}",
        tofile=f"b/{display}",
    )
    print("".join(diff), file=sys.stderr, end="")


def check_or_write(path: Path, expected: str, check: bool) -> bool:
    if check:
        try:
            actual = path.read_text(encoding="utf-8")
        except FileNotFoundError:
            actual = ""
            print(f"generated contract is missing: {path.relative_to(ROOT)}", file=sys.stderr)
            print_diff(path, actual, expected)
            return False
        if actual != expected:
            print(
                f"generated contract drift: {path.relative_to(ROOT)}; "
                "run python scripts/generate-frontend-contracts.py",
                file=sys.stderr,
            )
            print_diff(path, actual, expected)
            return False
        return True

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(expected, encoding="utf-8", newline="\n")
    print(f"wrote {path.relative_to(ROOT)}")
    return True


def compact_json(document: dict[str, Any]) -> str:
    return json.dumps(
        document,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if committed generated files differ")
    args = parser.parse_args()

    self_test_discriminated_unions()

    base_document, base_openapi = run_export("export_openapi")
    base_typescript = render_typescript(base_document, source_path=SOURCE_PATH)
    canonical_registry, _ = run_export("export_client_registry", "canonical")
    compatibility_registry, _ = run_export("export_client_registry", "compatibility")
    compatibility_registry_openapi = compact_json(compatibility_registry)
    registry_typescript = render_typescript(
        canonical_registry,
        source_path=CLIENT_REGISTRY_SOURCE_PATH,
        imports=("import type { ClientProjection } from './control-plane';",),
        support_nullable=True,
    )
    query_mail, _ = run_export("export_query_mail")
    query_mail_openapi = compact_json(query_mail)
    query_mail_typescript = render_typescript(
        query_mail,
        source_path=QUERY_MAIL_SOURCE_PATH,
        support_nullable=True,
    )
    operator_query, _ = run_export("export_operator_query")
    operator_query_openapi = compact_json(operator_query)
    operator_query_typescript = render_typescript(
        operator_query,
        source_path=OPERATOR_QUERY_SOURCE_PATH,
        support_nullable=True,
    )
    profile_generation, _ = run_export("export_profile_generation")
    profile_generation_openapi = compact_json(profile_generation)
    profile_generation_typescript = render_typescript(
        profile_generation,
        source_path=PROFILE_GENERATION_SOURCE_PATH,
        support_nullable=True,
    )
    mailbox, _ = run_export("export_mailbox")
    mailbox_openapi = compact_json(mailbox)
    mailbox_typescript = render_typescript(
        mailbox,
        source_path=MAILBOX_SOURCE_PATH,
        support_nullable=True,
    )
    coordinator, _ = run_export("export_coordinator")
    coordinator_openapi = compact_json(coordinator)
    coordinator_typescript = render_typescript(
        coordinator,
        source_path=COORDINATOR_SOURCE_PATH,
        support_nullable=True,
    )

    results = [
        check_or_write(OPENAPI_PATH, base_openapi, args.check),
        check_or_write(TYPESCRIPT_PATH, base_typescript, args.check),
        check_or_write(
            CLIENT_REGISTRY_OPENAPI_PATH,
            compatibility_registry_openapi,
            args.check,
        ),
        check_or_write(
            CLIENT_REGISTRY_TYPESCRIPT_PATH,
            registry_typescript,
            args.check,
        ),
        check_or_write(QUERY_MAIL_OPENAPI_PATH, query_mail_openapi, args.check),
        check_or_write(QUERY_MAIL_TYPESCRIPT_PATH, query_mail_typescript, args.check),
        check_or_write(OPERATOR_QUERY_OPENAPI_PATH, operator_query_openapi, args.check),
        check_or_write(
            OPERATOR_QUERY_TYPESCRIPT_PATH,
            operator_query_typescript,
            args.check,
        ),
        check_or_write(
            PROFILE_GENERATION_OPENAPI_PATH,
            profile_generation_openapi,
            args.check,
        ),
        check_or_write(
            PROFILE_GENERATION_TYPESCRIPT_PATH,
            profile_generation_typescript,
            args.check,
        ),
        check_or_write(MAILBOX_OPENAPI_PATH, mailbox_openapi, args.check),
        check_or_write(MAILBOX_TYPESCRIPT_PATH, mailbox_typescript, args.check),
        check_or_write(COORDINATOR_OPENAPI_PATH, coordinator_openapi, args.check),
        check_or_write(COORDINATOR_TYPESCRIPT_PATH, coordinator_typescript, args.check),
    ]
    if not all(results):
        return 1
    print("frontend contracts are deterministic and current" if args.check else "frontend contracts generated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
