#!/usr/bin/env python3
"""Generate PAS-2 leaf browser operations from the validated canonical OpenAPI input.

This file is a projection-only renderer. It deliberately imports the existing PAS-2
compiler/checker and cannot repair or supplement operation semantics. Method, path,
parameters, request bodies, response statuses, headers and schemas all come from the
validated OpenAPI document.
"""

from __future__ import annotations

import argparse
import json
import re
import runpy
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts" / "check-frontend-openapi.py"
OUTPUT = ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "operations.ts"
GENERATED_BY = "scripts/generate-frontend-openapi-runtime.py"
IDENTIFIER = re.compile(r"^[A-Za-z_$][A-Za-z0-9_$]*$")


def checker_symbols() -> dict[str, Any]:
    return runpy.run_path(str(CHECKER))


def property_name(name: str) -> str:
    return name if IDENTIFIER.fullmatch(name) else json.dumps(name, ensure_ascii=False)


def camel_identifier(value: str) -> str:
    words = [word for word in re.split(r"[^A-Za-z0-9_$]+", value) if word]
    if not words:
        raise ValueError(f"cannot derive TypeScript identifier from {value!r}")
    first = words[0]
    rendered = first[:1].lower() + first[1:]
    rendered += "".join(word[:1].upper() + word[1:] for word in words[1:])
    if not IDENTIFIER.fullmatch(rendered):
        raise ValueError(f"invalid generated TypeScript identifier {rendered!r}")
    return rendered


def pascal_identifier(value: str) -> str:
    identifier = camel_identifier(value)
    return identifier[:1].upper() + identifier[1:]


def schema_type(schema: dict[str, Any]) -> str:
    reference = schema.get("$ref")
    if isinstance(reference, str):
        prefix = "#/components/schemas/"
        if not reference.startswith(prefix):
            raise ValueError(f"unsupported runtime type reference {reference!r}")
        target = reference[len(prefix) :]
        if not IDENTIFIER.fullmatch(target):
            raise ValueError(f"component schema name is not a TypeScript identifier: {target!r}")
        siblings = {key: value for key, value in schema.items() if key != "$ref"}
        return target if not siblings else f"({target} & {schema_type(siblings)})"

    if "const" in schema:
        return json.dumps(schema["const"], ensure_ascii=False, separators=(",", ":"))

    enum = schema.get("enum")
    if isinstance(enum, list) and enum:
        return " | ".join(
            json.dumps(value, ensure_ascii=False, separators=(",", ":")) for value in enum
        )

    all_of = schema.get("allOf")
    if isinstance(all_of, list) and all_of:
        return " & ".join(f"({schema_type(child)})" for child in all_of if isinstance(child, dict))
    one_of = schema.get("oneOf")
    if isinstance(one_of, list) and one_of:
        return " | ".join(f"({schema_type(child)})" for child in one_of if isinstance(child, dict))
    any_of = schema.get("anyOf")
    if isinstance(any_of, list) and any_of:
        return " | ".join(f"({schema_type(child)})" for child in any_of if isinstance(child, dict))

    raw_type = schema.get("type")
    if isinstance(raw_type, list):
        return " | ".join(schema_type({**schema, "type": item}) for item in raw_type)
    if raw_type == "null":
        return "null"
    if raw_type == "string":
        return "string"
    if raw_type in {"integer", "number"}:
        return "number"
    if raw_type == "boolean":
        return "boolean"
    if raw_type == "array":
        items = schema.get("items")
        if not isinstance(items, dict):
            raise ValueError("array schema is missing items")
        return f"ReadonlyArray<{schema_type(items)}>"
    if raw_type == "object" or "properties" in schema or "additionalProperties" in schema:
        properties = schema.get("properties", {})
        if not isinstance(properties, dict):
            raise ValueError("object properties must be an object")
        required_raw = schema.get("required", [])
        if not isinstance(required_raw, list) or not all(isinstance(name, str) for name in required_raw):
            raise ValueError("object required must be a string array")
        required = set(required_raw)
        fields: list[str] = []
        for name in sorted(properties):
            child = properties[name]
            if not isinstance(child, dict):
                raise ValueError(f"property {name!r} schema must be an object")
            optional = "" if name in required else "?"
            fields.append(f"readonly {property_name(name)}{optional}: {schema_type(child)}")
        object_type = "{ " + "; ".join(fields) + " }"
        additional = schema.get("additionalProperties")
        if isinstance(additional, dict):
            return f"({object_type} & Readonly<Record<string, {schema_type(additional)}>>)"
        if not fields and additional is not False:
            return "Readonly<Record<string, unknown>>"
        return object_type

    return "unknown"


def parameter_argument(parameter: dict[str, Any]) -> tuple[str | None, str | None]:
    location = parameter["in"]
    name = parameter["name"]
    if location == "header":
        lowered = name.lower()
        if lowered == "x-correlation-id":
            return None, "correlation"
        if lowered == "idempotency-key":
            return None, "idempotency"
        if lowered == "x-tenant-id":
            return "tenantId", None
    return camel_identifier(name), None


def runtime_parameter(parameter: dict[str, Any]) -> dict[str, Any]:
    argument_name, source = parameter_argument(parameter)
    rendered = {
        "name": parameter["name"],
        "in": parameter["in"],
        "required": bool(parameter["required"]),
        "style": parameter["style"],
        "explode": bool(parameter["explode"]),
        "schema": parameter["schema"],
    }
    if argument_name is not None:
        rendered["argumentName"] = argument_name
    if source is not None:
        rendered["source"] = source
    return rendered


def runtime_operation(operation: dict[str, Any]) -> dict[str, Any]:
    responses: list[dict[str, Any]] = []
    for response in operation["responses"]:
        responses.append(
            {
                "status": int(response["status"]),
                "headers": response["headers"],
                "content": response["content"],
            }
        )
    return {
        "operationId": operation["operationId"],
        "method": operation["method"],
        "path": operation["path"],
        "parameters": [runtime_parameter(parameter) for parameter in operation["parameters"]],
        "requestBody": operation["requestBody"],
        "responses": responses,
    }


def input_fields(operation: dict[str, Any]) -> tuple[list[tuple[str, str, bool]], bool]:
    fields: dict[str, tuple[str, bool]] = {}
    idempotency_required: bool | None = None
    for parameter in operation["parameters"]:
        argument_name, source = parameter_argument(parameter)
        if source == "idempotency":
            required = bool(parameter["required"])
            idempotency_required = required or bool(idempotency_required)
            continue
        if source == "correlation":
            continue
        if argument_name is None:
            raise ValueError(f"{operation['operationId']}: parameter has no generated argument")
        field_type = schema_type(parameter["schema"])
        required = bool(parameter["required"])
        existing = fields.get(argument_name)
        if existing is not None and existing[0] != field_type:
            raise ValueError(
                f"{operation['operationId']}: argument {argument_name} has conflicting schemas"
            )
        fields[argument_name] = (field_type, required or (existing[1] if existing else False))

    request_body = operation.get("requestBody")
    if request_body is not None:
        content = request_body["content"]
        body_types = sorted({schema_type(media["schema"]) for media in content})
        if not body_types:
            raise ValueError(f"{operation['operationId']}: request body has no schema")
        fields["body"] = (" | ".join(f"({value})" for value in body_types), True)

    rendered = [(name, value[0], value[1]) for name, value in sorted(fields.items())]
    if idempotency_required is not None:
        rendered.append(("idempotencyKey", "string", idempotency_required))
    rendered.append(("signal", "AbortSignal", False))
    required_input = any(required for _, _, required in rendered)
    return rendered, required_input


def success_responses(operation: dict[str, Any]) -> list[dict[str, Any]]:
    return [response for response in operation["responses"] if 200 <= int(response["status"]) < 300]


def success_type(operation: dict[str, Any]) -> str:
    types: set[str] = set()
    for response in success_responses(operation):
        if not response["content"]:
            types.add("undefined")
        else:
            types.update(schema_type(media["schema"]) for media in response["content"])
    if not types:
        raise ValueError(f"{operation['operationId']}: operation has no declared success response")
    return " | ".join(sorted(types))


def guard_name(operation_id: str, status: str) -> str:
    return f"is{pascal_identifier(operation_id)}Status{status}Payload"


def guard_type(response: dict[str, Any]) -> str:
    if not response["content"]:
        return "undefined"
    values = sorted({schema_type(media["schema"]) for media in response["content"]})
    return " | ".join(values)


def render_guard(operation: dict[str, Any], response: dict[str, Any]) -> list[str]:
    if not response["content"]:
        return []
    name = guard_name(operation["operationId"], str(response["status"]))
    expected = guard_type(response)
    tests = [
        "matchesSchema("
        + json.dumps(media["schema"], sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        + ", value, COMPONENT_SCHEMAS)"
        for media in response["content"]
    ]
    return [
        f"function {name}(value: unknown): value is {expected} {{",
        f"  return {' || '.join(tests)};",
        "}",
        "",
    ]


def render_operation(operation: dict[str, Any]) -> list[str]:
    operation_id = operation["operationId"]
    if not IDENTIFIER.fullmatch(operation_id):
        raise ValueError(f"operationId is not a TypeScript identifier: {operation_id!r}")
    interface_name = f"{pascal_identifier(operation_id)}OperationInput"
    spec_name = f"{operation_id}Spec"
    fields, required_input = input_fields(operation)
    return_type = success_type(operation)

    lines = [f"export interface {interface_name} {{"]
    for name, field_type, required in fields:
        optional = "" if required else "?"
        lines.append(f"  readonly {name}{optional}: {field_type};")
    lines.extend(["}", ""])

    spec = runtime_operation(operation)
    lines.extend(
        [
            f"const {spec_name}: RuntimeOperationSpec = "
            + json.dumps(spec, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
            + ";",
            "",
        ]
    )
    for response in success_responses(operation):
        lines.extend(render_guard(operation, response))

    default = " = {}" if not required_input else ""
    lines.append(
        f"export async function {operation_id}(input: {interface_name}{default}): Promise<{return_type}> {{"
    )
    lines.append(f"  const result = await invokeOperation({spec_name}, input, COMPONENT_SCHEMAS);")
    lines.append("  switch (result.status) {")
    for response in success_responses(operation):
        status = str(response["status"])
        lines.append(f"    case {status}:")
        if not response["content"]:
            lines.append("      if (result.payload !== undefined) {")
            lines.append(
                f"        throw new ApiProtocolError({json.dumps(operation_id)}, 'declared no-body success returned a payload');"
            )
            lines.append("      }")
            lines.append("      return undefined;")
        else:
            name = guard_name(operation_id, status)
            lines.append(f"      if (!{name}(result.payload)) {{")
            lines.append(
                f"        throw new ApiProtocolError({json.dumps(operation_id)}, 'validated success payload did not satisfy generated type guard');"
            )
            lines.append("      }")
            lines.append("      return result.payload;")
    lines.append("    default:")
    lines.append(
        f"      throw new ApiProtocolError({json.dumps(operation_id)}, `runtime returned unexpected success status ${{result.status}}`);"
    )
    lines.extend(["  }", "}", ""])
    return lines


def render_typescript(document: dict[str, Any], ir: dict[str, Any]) -> str:
    components = document.get("components", {})
    schemas = components.get("schemas", {}) if isinstance(components, dict) else {}
    if not isinstance(schemas, dict):
        raise ValueError("validated OpenAPI components.schemas must be an object")
    operations = ir.get("operations")
    if not isinstance(operations, list):
        raise ValueError("compiled PAS-2 operation IR is missing operations")

    lines = [
        "// GENERATED FILE — DO NOT EDIT.",
        "// Semantic source: validated canonical OpenAPI v1 compiler input.",
        f"// Generated through: {GENERATED_BY}",
        "// Regenerate with: npm --prefix frontend run generate:api",
        "",
        "import { ApiProtocolError, invokeOperation, matchesSchema } from '../openapi-runtime';",
        "import type { JsonSchema, RuntimeOperationSpec } from '../openapi-runtime';",
        "",
    ]

    for name in sorted(schemas):
        schema = schemas[name]
        if not isinstance(name, str) or not IDENTIFIER.fullmatch(name):
            raise ValueError(f"component schema name is not a TypeScript identifier: {name!r}")
        if not isinstance(schema, dict):
            raise ValueError(f"component schema {name} must be an object")
        lines.append(f"export type {name} = {schema_type(schema)};")
    lines.append("")
    lines.append(
        "const COMPONENT_SCHEMAS: Readonly<Record<string, JsonSchema>> = "
        + json.dumps(schemas, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        + ";"
    )
    lines.append("")

    for operation in operations:
        if not isinstance(operation, dict):
            raise ValueError("compiled PAS-2 operation must be an object")
        lines.extend(render_operation(operation))
    return "\n".join(lines).rstrip() + "\n"


def compile_runtime() -> str:
    symbols = checker_symbols()
    source = symbols["rendered_openapi"]()
    document = json.loads(source)
    if not isinstance(document, dict):
        raise ValueError("PAS-2 compiler input must be an object")
    index = symbols["validate_document"](document)
    ir = symbols["compile_operation_ir"](document, index)
    return render_typescript(document, ir)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check-render", action="store_true", help="prove byte-identical rendering without writing")
    args = parser.parse_args()

    first = compile_runtime()
    second = compile_runtime()
    if first.encode("utf-8") != second.encode("utf-8"):
        raise SystemExit("PAS-2 generated runtime operations are not byte-identical")
    if args.check_render:
        print(f"PAS-2 generated runtime operation render is deterministic: bytes={len(first.encode('utf-8'))}")
        return 0

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(first, encoding="utf-8", newline="\n")
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(f"PAS-2 runtime codegen failed closed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
