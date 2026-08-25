#!/usr/bin/env python3
"""PAS-2 Transaction A: prove one deterministic fail-closed frontend OpenAPI input."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
RENDER = ROOT / "scripts" / "render-openapi.py"
GOLDEN = ROOT / "tests" / "contracts" / "frontend-wire-golden-v1.json"
HTTP_METHODS = {"get", "post", "put", "patch", "delete"}
ALLOWED_SECURITY = {"cloudflareAccessJwt"}
ALLOWED_MEDIA_TYPES = {"application/json", "application/problem+json"}
BROWSER_EXTENSION = "x-part-crm-browser-transport"
DIGEST_EXTENSION = "x-part-crm-request-digest"
REQUIRED_HEADERS_EXTENSION = "x-part-crm-required-response-headers"
EXPECTED_BROWSER_POLICY = {
    "allowedPathPrefix": "/api/v1/",
    "absoluteUrls": "forbidden",
    "credentials": "same-origin",
    "openapiServers": "documentation-only",
    "redirect": "error",
}
EXPECTED_DIGEST_POLICY = {
    "algorithm": "sha-256",
    "canonicalization": "part-crm-json-v1",
    "encoding": "lowercase-hex",
    "source": "request-body-without-requestDigest",
    "rules": {
        "arrays": "preserve-order",
        "numbers": "integers-only",
        "objectKeys": "unicode-codepoint-ascending",
        "textEncoding": "utf-8",
        "whitespace": "none",
    },
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
    DIGEST_EXTENSION,
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


def rendered_compatibility_openapi() -> str:
    return run([sys.executable, str(RENDER)])


def closed_compiler_input() -> str:
    merged = rendered_compatibility_openapi()
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
        input_text=merged,
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


def validate_schema(document: dict[str, Any], schema: Any, location: str) -> None:
    if not isinstance(schema, dict):
        raise ValueError(f"{location}: schema must be an object")
    unknown = sorted(
        key
        for key in schema
        if key not in SCHEMA_KEYS and not key.startswith("x-part-crm-")
    )
    if unknown:
        raise ValueError(f"{location}: unsupported schema keywords {unknown}")
    if "nullable" in schema:
        raise ValueError(f"{location}: OpenAPI 3.0 nullable is forbidden")
    if "$ref" in schema:
        resolve(document, schema)
    if schema.get(DIGEST_EXTENSION) is not None and schema[DIGEST_EXTENSION] != EXPECTED_DIGEST_POLICY:
        raise ValueError(f"{location}: request digest extension differs from Rust authority")
    properties = schema.get("properties", {})
    if properties is not None:
        if not isinstance(properties, dict):
            raise ValueError(f"{location}: properties must be an object")
        for name, child in properties.items():
            validate_schema(document, child, f"{location}.properties.{name}")
            if name == "requestDigest":
                resolved = child
                if not isinstance(resolved, dict) or resolved.get(DIGEST_EXTENSION) != EXPECTED_DIGEST_POLICY:
                    raise ValueError(
                        f"{location}.properties.requestDigest: missing Rust-authored {DIGEST_EXTENSION}"
                    )
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


def validate_content(document: dict[str, Any], content: Any, location: str, *, problem_only: bool = False) -> None:
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
        validate_schema(document, media["schema"], f"{location}.{media_type}.schema")


def operation_index(document: dict[str, Any]) -> dict[str, tuple[str, str, dict[str, Any], dict[str, Any]]]:
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
                location = parameter.get("in")
                if location not in {"path", "query", "header"}:
                    raise ValueError(f"{operation_id}: unsupported parameter location {location!r}")
                if location == "path":
                    if parameter.get("required") is not True:
                        raise ValueError(f"{operation_id}: path parameter must be required")
                    name = parameter.get("name")
                    if not isinstance(name, str):
                        raise ValueError(f"{operation_id}: path parameter name is required")
                    path_parameters.add(name)
                style = parameter.get("style")
                if style is not None:
                    expected = {"path": "simple", "header": "simple", "query": "form"}[location]
                    if style != expected:
                        raise ValueError(f"{operation_id}: unsupported {location} style {style!r}")
                if "schema" in parameter:
                    validate_schema(document, parameter["schema"], f"{operation_id}.parameter.{parameter.get('name')}")
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
                        raise ValueError(f"{operation_id}: unsupported security schemes {sorted(unsupported)}")

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
                if headers:
                    if not isinstance(headers, dict):
                        raise ValueError(f"{operation_id} {status}: response headers must be an object")
                    required = response.get(REQUIRED_HEADERS_EXTENSION)
                    if required != sorted(headers):
                        raise ValueError(
                            f"{operation_id} {status}: required response-header extension must exactly match declared headers"
                        )
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


def required_request_headers(document: dict[str, Any], operation: dict[str, Any], path_item: dict[str, Any]) -> list[str]:
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
    content = response.get("content", {})
    return sorted(content)


def canonical_json_v1(value: Any, *, root: bool = True) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        raise ValueError("part-crm-json-v1 forbids floating-point numbers")
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    if isinstance(value, list):
        return "[" + ",".join(canonical_json_v1(child, root=False) for child in value) + "]"
    if isinstance(value, dict):
        keys = sorted(key for key in value if not (root and key == "requestDigest"))
        return "{" + ",".join(
            json.dumps(key, ensure_ascii=False, separators=(",", ":"))
            + ":"
            + canonical_json_v1(value[key], root=False)
            for key in keys
        ) + "}"
    raise ValueError(f"unsupported JSON value: {type(value).__name__}")


def assert_golden_vectors(document: dict[str, Any], index: dict[str, tuple[str, str, dict[str, Any], dict[str, Any]]]) -> None:
    golden = json.loads(GOLDEN.read_text(encoding="utf-8"))
    if golden.get("schemaVersion") != 1:
        raise ValueError("frontend wire golden schemaVersion must be 1")
    digest = golden["requestDigest"]
    canonical = canonical_json_v1(digest["input"])
    if canonical != digest["canonicalUtf8"]:
        raise ValueError("Python cross-language digest material differs from golden vector")
    actual_digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    if actual_digest != digest["sha256"]:
        raise ValueError("Python cross-language SHA-256 differs from golden vector")
    rust = json.loads(
        run(
            [
                "cargo",
                "run",
                "--locked",
                "--quiet",
                "-p",
                "control-plane-contract",
                "--bin",
                "export_frontend_openapi",
                "--",
                "--digest",
            ],
            input_text=json.dumps(digest["input"], ensure_ascii=False, separators=(",", ":")),
        )
    )
    if rust != {"canonicalUtf8": digest["canonicalUtf8"], "sha256": digest["sha256"]}:
        raise ValueError(f"Rust digest output differs from cross-language golden vector: {rust!r}")

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
            if response.get(REQUIRED_HEADERS_EXTENSION) != vector["headers"]:
                raise ValueError(f"{operation_id}: golden required response headers mismatch")
        else:
            raise ValueError(f"unknown golden vector kind: {kind}")


def validate_document(document: dict[str, Any]) -> dict[str, tuple[str, str, dict[str, Any], dict[str, Any]]]:
    if document.get("openapi") != "3.1.0":
        raise ValueError("frontend compiler input must be exactly OpenAPI 3.1.0")
    if document.get(BROWSER_EXTENSION) != EXPECTED_BROWSER_POLICY:
        raise ValueError("browser transport extension differs from Rust authority")
    components = document.get("components")
    if not isinstance(components, dict):
        raise ValueError("components must be an object")
    security_schemes = components.get("securitySchemes", {})
    if not isinstance(security_schemes, dict):
        raise ValueError("components.securitySchemes must be an object")
    unsupported_security = set(security_schemes) - ALLOWED_SECURITY
    if unsupported_security:
        raise ValueError(f"unsupported security schemes: {sorted(unsupported_security)}")
    for name, schema in components.get("schemas", {}).items():
        validate_schema(document, schema, f"components.schemas.{name}")
    return operation_index(document)


def main() -> int:
    first = closed_compiler_input()
    second = closed_compiler_input()
    if first.encode("utf-8") != second.encode("utf-8"):
        raise SystemExit("frontend OpenAPI generation is not byte-identical across repeated runs")
    document = json.loads(first)
    if not isinstance(document, dict):
        raise SystemExit("frontend OpenAPI compiler input must be a JSON object")
    index = validate_document(document)
    assert_golden_vectors(document, index)
    digest = hashlib.sha256(first.encode("utf-8")).hexdigest()
    print(
        f"PAS-2 frontend OpenAPI compiler input passed: operations={len(index)} bytes={len(first.encode('utf-8'))} sha256={digest}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
