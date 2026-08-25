#!/usr/bin/env python3
"""PAS-2 Transaction A: prove one deterministic, non-repairing frontend OpenAPI input."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
RENDER = ROOT / "scripts" / "render-openapi.py"
GOLDEN = ROOT / "tests" / "contracts" / "frontend-wire-golden-v1.json"
HTTP_METHODS = {"get", "post", "put", "patch", "delete"}
ALLOWED_SECURITY = {"cloudflareAccessJwt"}
ALLOWED_MEDIA_TYPES = {"application/json", "application/problem+json"}
FORBIDDEN_REPAIR_EXTENSIONS = {
    "x-part-crm-request-digest",
    "x-part-crm-required-response-headers",
    "x-part-crm-browser-transport",
}
SCHEMA_KEYS = {
    "$ref",
    "$comment",
    "title",
    "description",
    "type",
    "format",
    "enum",
    "const",
    "default",
    "examples",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "oneOf",
    "anyOf",
    "allOf",
    "not",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
    "readOnly",
    "writeOnly",
}


def run(command: list[str], *, input_text: str | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        text=True,
        input=input_text,
        capture_output=True,
    )
    if completed.returncode != 0:
        sys.stderr.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise SystemExit(completed.returncode)
    return completed.stdout


def rendered_openapi() -> str:
    return run([sys.executable, str(RENDER)])


def compiler_input(source: str) -> str:
    return run(
        [
            "cargo",
            "run",
            "--locked",
            "--quiet",
            "-p",
            "control-plane-contract",
            "--bin",
            "export_frontend_openapi",
        ],
        input_text=source,
    )


def resolve(document: dict[str, Any], value: Any) -> Any:
    if not isinstance(value, dict) or "$ref" not in value:
        return value
    reference = value["$ref"]
    if not isinstance(reference, str) or not reference.startswith("#/"):
        raise ValueError(f"unsupported reference {reference!r}")
    current: Any = document
    for raw in reference[2:].split("/"):
        key = raw.replace("~1", "/").replace("~0", "~")
        if not isinstance(current, dict) or key not in current:
            raise ValueError(f"unresolved reference {reference}")
        current = current[key]
    if len(value) == 1:
        return current
    if not isinstance(current, dict):
        raise ValueError(f"reference siblings require an object target: {reference}")
    merged = dict(current)
    merged.update({key: child for key, child in value.items() if key != "$ref"})
    return merged


def reject_repair_extensions(value: Any, location: str = "root") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key in FORBIDDEN_REPAIR_EXTENSIONS:
                raise ValueError(f"{location}: obsolete compiler repair extension is forbidden: {key}")
            reject_repair_extensions(child, f"{location}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            reject_repair_extensions(child, f"{location}[{index}]")


def validate_schema(document: dict[str, Any], schema: Any, location: str) -> None:
    if not isinstance(schema, dict):
        raise ValueError(f"{location}: schema must be an object")
    unknown = sorted(key for key in schema if key not in SCHEMA_KEYS and not key.startswith("x-"))
    if unknown:
        raise ValueError(f"{location}: unsupported schema keywords {unknown}")
    if "nullable" in schema:
        raise ValueError(f"{location}: OpenAPI 3.0 nullable is forbidden; producer must emit JSON Schema 2020-12")
    if "$ref" in schema:
        resolve(document, schema)
    properties = schema.get("properties", {})
    if properties is not None:
        if not isinstance(properties, dict):
            raise ValueError(f"{location}: properties must be an object")
        for name, child in properties.items():
            validate_schema(document, child, f"{location}.properties.{name}")
    for keyword in ("items", "not", "additionalProperties"):
        child = schema.get(keyword)
        if isinstance(child, dict):
            validate_schema(document, child, f"{location}.{keyword}")
    for keyword in ("oneOf", "anyOf", "allOf"):
        children = schema.get(keyword, [])
        if children is not None:
            if not isinstance(children, list):
                raise ValueError(f"{location}.{keyword}: must be an array")
            for index, child in enumerate(children):
                validate_schema(document, child, f"{location}.{keyword}[{index}]")


def validate_content(
    document: dict[str, Any],
    content: Any,
    location: str,
    *,
    problem_only: bool = False,
) -> None:
    if not isinstance(content, dict) or not content:
        raise ValueError(f"{location}: content must be a non-empty object")
    for media_type, media in content.items():
        normalized = media_type.split(";", 1)[0].strip().lower()
        if normalized not in ALLOWED_MEDIA_TYPES:
            raise ValueError(f"{location}: unsupported media type {media_type!r}")
        if problem_only and normalized != "application/problem+json":
            raise ValueError(f"{location}: declared problem response must be application/problem+json")
        if not isinstance(media, dict) or "schema" not in media:
            raise ValueError(f"{location}.{media_type}: schema is required")
        schema = media["schema"]
        validate_schema(document, schema, f"{location}.{media_type}.schema")
        if normalized == "application/problem+json":
            resolved = resolve(document, schema)
            if isinstance(resolved, dict) and (
                not resolved or (len(resolved) == 1 and resolved.get("type") == "object")
            ):
                raise ValueError(
                    f"{location}.{media_type}: permissive problem schema is forbidden; fix producer output"
                )


def operation_index(
    document: dict[str, Any],
) -> dict[str, tuple[str, str, dict[str, Any], dict[str, Any]]]:
    paths = document.get("paths")
    if not isinstance(paths, dict):
        raise ValueError("paths must be an object")
    index: dict[str, tuple[str, str, dict[str, Any], dict[str, Any]]] = {}
    for path, path_item in paths.items():
        if not isinstance(path, str) or not path.startswith("/api/v1/") or "://" in path:
            raise ValueError(f"browser operation path must be same-origin relative /api/v1/*: {path!r}")
        if not isinstance(path_item, dict):
            raise ValueError(f"{path}: path item must be an object")
        placeholders = set(re.findall(r"\{([^{}]+)\}", path))
        for method, operation in path_item.items():
            if method == "parameters" or method.startswith("x-"):
                continue
            if method not in HTTP_METHODS:
                raise ValueError(f"{path}: unsupported HTTP/path-item key {method!r}")
            if not isinstance(operation, dict):
                raise ValueError(f"{method.upper()} {path}: operation must be an object")
            operation_id = operation.get("operationId")
            if not isinstance(operation_id, str) or not operation_id:
                raise ValueError(f"{method.upper()} {path}: operationId is required")
            if operation_id in index:
                raise ValueError(f"duplicate operationId: {operation_id}")
            index[operation_id] = (method.upper(), path, operation, path_item)

            parameters: list[Any] = []
            for owner in (path_item, operation):
                raw = owner.get("parameters", [])
                if not isinstance(raw, list):
                    raise ValueError(f"{operation_id}: parameters must be an array")
                parameters.extend(raw)
            path_parameters: set[str] = set()
            for parameter_value in parameters:
                parameter = resolve(document, parameter_value)
                if not isinstance(parameter, dict):
                    raise ValueError(f"{operation_id}: parameter must resolve to an object")
                parameter_location = parameter.get("in")
                if parameter_location not in {"path", "query", "header"}:
                    raise ValueError(
                        f"{operation_id}: unsupported parameter location {parameter_location!r}"
                    )
                if parameter_location == "path":
                    if parameter.get("required") is not True:
                        raise ValueError(f"{operation_id}: path parameter must be required")
                    name = parameter.get("name")
                    if not isinstance(name, str):
                        raise ValueError(f"{operation_id}: path parameter name is required")
                    path_parameters.add(name)
                style = parameter.get("style")
                if style is not None:
                    expected = {"path": "simple", "header": "simple", "query": "form"}[
                        parameter_location
                    ]
                    if style != expected:
                        raise ValueError(
                            f"{operation_id}: unsupported {parameter_location} style {style!r}"
                        )
                if "schema" in parameter:
                    validate_schema(
                        document,
                        parameter["schema"],
                        f"{operation_id}.parameter.{parameter.get('name')}",
                    )
            if path_parameters != placeholders:
                raise ValueError(
                    f"{operation_id}: path parameter coverage mismatch placeholders={sorted(placeholders)!r} declared={sorted(path_parameters)!r}"
                )

            security = operation.get("security", document.get("security", []))
            if security is not None:
                if not isinstance(security, list):
                    raise ValueError(f"{operation_id}: security must be an array")
                for requirement in security:
                    if not isinstance(requirement, dict):
                        raise ValueError(f"{operation_id}: security requirement must be an object")
                    unsupported = set(requirement) - ALLOWED_SECURITY
                    if unsupported:
                        raise ValueError(
                            f"{operation_id}: unsupported security schemes {sorted(unsupported)}"
                        )

            request_body = operation.get("requestBody")
            if request_body is not None:
                body = resolve(document, request_body)
                if not isinstance(body, dict) or body.get("required") is not True:
                    raise ValueError(f"{operation_id}: requestBody must be explicitly required")
                validate_content(document, body.get("content"), f"{operation_id}.requestBody")

            responses = operation.get("responses")
            if not isinstance(responses, dict) or not responses:
                raise ValueError(f"{operation_id}: responses must be non-empty")
            for status, response_value in responses.items():
                if not re.fullmatch(r"[1-5][0-9][0-9]", str(status)):
                    raise ValueError(f"{operation_id}: unsupported response status {status!r}")
                response = resolve(document, response_value)
                if not isinstance(response, dict):
                    raise ValueError(f"{operation_id} {status}: response must resolve to an object")
                if str(status) == "204" and "content" in response:
                    raise ValueError(f"{operation_id}: declared 204 must not contain a body")
                headers = response.get("headers", {})
                if headers and not isinstance(headers, dict):
                    raise ValueError(f"{operation_id} {status}: response headers must be an object")
                content = response.get("content")
                if content is not None:
                    validate_content(document, content, f"{operation_id}.responses.{status}")
                    if int(status) >= 400:
                        validate_content(
                            document,
                            content,
                            f"{operation_id}.responses.{status}",
                            problem_only=True,
                        )
    return index


def required_request_headers(
    document: dict[str, Any], operation: dict[str, Any], path_item: dict[str, Any]
) -> list[str]:
    result: list[str] = []
    for owner in (path_item, operation):
        for raw in owner.get("parameters", []):
            parameter = resolve(document, raw)
            if parameter.get("in") == "header" and parameter.get("required") is True:
                result.append(parameter["name"])
    return sorted(result)


def security_names(operation: dict[str, Any], document: dict[str, Any]) -> list[str]:
    result: set[str] = set()
    for requirement in operation.get("security", document.get("security", [])) or []:
        result.update(requirement)
    return sorted(result)


def media_types(document: dict[str, Any], response_value: Any) -> list[str]:
    response = resolve(document, response_value)
    return sorted(response.get("content", {}))


def assert_golden_vectors(
    document: dict[str, Any],
    index: dict[str, tuple[str, str, dict[str, Any], dict[str, Any]]],
) -> None:
    golden = json.loads(GOLDEN.read_text(encoding="utf-8"))
    if golden.get("schemaVersion") != 1:
        raise ValueError("frontend wire golden schemaVersion must be 1")
    if "requestDigest" in golden:
        raise ValueError("frontend wire golden must not own requestDigest semantics")

    for vector in golden["wireVectors"]:
        operation_id = vector["operationId"]
        if operation_id not in index:
            raise ValueError(f"golden operation is missing from canonical input: {operation_id}")
        method, path, operation, path_item = index[operation_id]
        kind = vector["kind"]
        if kind in {"get", "mutation"}:
            if method != vector["method"] or path != vector["path"]:
                raise ValueError(f"{operation_id}: golden method/path mismatch")
            if required_request_headers(document, operation, path_item) != vector["requiredRequestHeaders"]:
                raise ValueError(f"{operation_id}: golden required request headers mismatch")
            if security_names(operation, document) != vector["security"]:
                raise ValueError(f"{operation_id}: golden security mismatch")
            status = vector["successStatus"]
            if status not in operation["responses"]:
                raise ValueError(f"{operation_id}: golden success status {status} is missing")
            if media_types(document, operation["responses"][status]) != [vector["successMediaType"]]:
                raise ValueError(f"{operation_id}: golden success media type mismatch")
        elif kind == "declared-204":
            response = resolve(document, operation["responses"].get(vector["status"]))
            if not isinstance(response, dict) or "content" in response:
                raise ValueError(f"{operation_id}: golden 204 no-body semantic mismatch")
        elif kind == "declared-problem":
            status = vector["status"]
            if media_types(document, operation["responses"].get(status)) != [vector["mediaType"]]:
                raise ValueError(f"{operation_id}: golden problem media type mismatch")
        elif kind == "required-response-headers":
            response = resolve(document, operation["responses"].get(vector["status"]))
            headers = response.get("headers", {}) if isinstance(response, dict) else {}
            if not isinstance(headers, dict) or sorted(headers) != vector["headers"]:
                raise ValueError(f"{operation_id}: golden declared response headers mismatch")
        else:
            raise ValueError(f"unknown golden vector kind: {kind}")


def validate_document(
    document: dict[str, Any],
) -> dict[str, tuple[str, str, dict[str, Any], dict[str, Any]]]:
    if document.get("openapi") != "3.1.0":
        raise ValueError("frontend compiler input must be exactly OpenAPI 3.1.0")
    reject_repair_extensions(document)
    components = document.get("components")
    if not isinstance(components, dict):
        raise ValueError("components must be an object")
    security_schemes = components.get("securitySchemes", {})
    if not isinstance(security_schemes, dict):
        raise ValueError("components.securitySchemes must be an object")
    unsupported_security = set(security_schemes) - ALLOWED_SECURITY
    if unsupported_security:
        raise ValueError(f"unsupported security schemes: {sorted(unsupported_security)}")
    schemas = components.get("schemas", {})
    if not isinstance(schemas, dict):
        raise ValueError("components.schemas must be an object")
    for name, schema in schemas.items():
        validate_schema(document, schema, f"components.schemas.{name}")
    return operation_index(document)


def main() -> int:
    run([sys.executable, str(RENDER), "--check"])
    source = rendered_openapi()
    first = compiler_input(source)
    second = compiler_input(source)
    if first.encode("utf-8") != second.encode("utf-8"):
        raise SystemExit("frontend OpenAPI validation/export is not byte-identical across repeated runs")

    source_document = json.loads(source)
    document = json.loads(first)
    if not isinstance(source_document, dict) or not isinstance(document, dict):
        raise SystemExit("frontend OpenAPI compiler input must be a JSON object")
    if document != source_document:
        raise SystemExit("frontend compiler repaired or mutated producer output; producer must be fixed instead")

    index = validate_document(document)
    assert_golden_vectors(document, index)
    digest = hashlib.sha256(first.encode("utf-8")).hexdigest()
    print(
        f"PAS-2 frontend OpenAPI validation passed: operations={len(index)} bytes={len(first.encode('utf-8'))} sha256={digest}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
