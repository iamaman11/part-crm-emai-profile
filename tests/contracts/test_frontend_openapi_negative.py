#!/usr/bin/env python3
"""Deliberately breaking PAS-2 frontend OpenAPI/compiler fixtures."""

from __future__ import annotations

import json
import runpy
import subprocess
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
CHECKER = ROOT / "scripts" / "check-frontend-openapi.py"


def checker_symbols() -> dict[str, Any]:
    return runpy.run_path(str(CHECKER))


def expect_validation_failure(
    validate_document: Any,
    document: dict[str, Any],
    expected_fragment: str,
) -> None:
    try:
        validate_document(document)
    except ValueError as error:
        if expected_fragment not in str(error):
            raise AssertionError(
                f"expected validation failure containing {expected_fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError(f"breaking fixture unexpectedly passed: {expected_fragment}")


def run_rust(document: dict[str, Any]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
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
        cwd=ROOT,
        input=json.dumps(document, separators=(",", ":")),
        text=True,
        capture_output=True,
        check=False,
    )


def expect_rust_validation_failure(document: dict[str, Any], expected_fragment: str) -> None:
    completed = run_rust(document)
    if completed.returncode == 0:
        raise AssertionError(f"Rust validator accepted breaking fixture: {expected_fragment}")
    diagnostics = f"{completed.stdout}\n{completed.stderr}"
    if expected_fragment not in diagnostics:
        raise AssertionError(
            f"Rust validator failed for the wrong reason; expected {expected_fragment!r}, got {diagnostics!r}"
        )


def operation(operation_id: str) -> dict[str, Any]:
    return {
        "operationId": operation_id,
        "responses": {"204": {"description": "No content"}},
    }


def valid_root() -> dict[str, Any]:
    return {
        "openapi": "3.1.0",
        "info": {"title": "PAS-2 negative fixture", "version": "1.0.0"},
        "paths": {},
        "components": {
            "schemas": {
                "Problem": {
                    "type": "object",
                    "additionalProperties": False,
                    "required": ["type"],
                    "properties": {"type": {"type": "string"}},
                }
            },
            "securitySchemes": {
                "cloudflareAccessJwt": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "Cf-Access-Jwt-Assertion",
                }
            },
        },
    }


def main() -> int:
    symbols = checker_symbols()
    validate_document = symbols["validate_document"]
    compile_operation_ir = symbols["compile_operation_ir"]
    render_compiler_ir = symbols["render_compiler_ir"]

    duplicate = valid_root()
    duplicate["paths"] = {
        "/api/v1/alpha": {"get": operation("duplicateOperation")},
        "/api/v1/beta": {"get": operation("duplicateOperation")},
    }
    expect_validation_failure(validate_document, duplicate, "duplicate operationId")

    incomplete_path = valid_root()
    incomplete_path["paths"] = {
        "/api/v1/things/{thingId}": {"get": operation("getThing")}
    }
    expect_validation_failure(validate_document, incomplete_path, "path parameter coverage mismatch")

    unsupported_media = valid_root()
    unsupported_media["paths"] = {
        "/api/v1/media": {
            "get": {
                "operationId": "getUnsupportedMedia",
                "responses": {
                    "200": {
                        "description": "Unsupported media",
                        "content": {"text/plain": {"schema": {"type": "string"}}},
                    }
                },
            }
        }
    }
    expect_validation_failure(validate_document, unsupported_media, "unsupported media type")

    unsupported_security = valid_root()
    unsupported_security["paths"] = {
        "/api/v1/security": {
            "get": {
                **operation("getUnsupportedSecurity"),
                "security": [{"otherAuth": []}],
            }
        }
    }
    expect_validation_failure(validate_document, unsupported_security, "unsupported security schemes")

    unsupported_schema = valid_root()
    unsupported_schema["components"]["schemas"]["Conditional"] = {
        "type": "object",
        "if": {"properties": {"kind": {"const": "alpha"}}},
    }
    expect_validation_failure(validate_document, unsupported_schema, "unsupported schema keywords")

    unsupported_explode = valid_root()
    unsupported_explode["paths"] = {
        "/api/v1/search": {
            "get": {
                "operationId": "getUnsupportedExplode",
                "parameters": [
                    {
                        "name": "q",
                        "in": "query",
                        "explode": False,
                        "schema": {"type": "string"},
                    }
                ],
                "responses": {"204": {"description": "No content"}},
            }
        }
    }
    expect_validation_failure(validate_document, unsupported_explode, "unsupported query explode")

    unsupported_response_header = valid_root()
    unsupported_response_header["paths"] = {
        "/api/v1/header": {
            "get": {
                "operationId": "getUnsupportedResponseHeader",
                "responses": {
                    "204": {
                        "description": "No content",
                        "headers": {
                            "X-Unsupported": {
                                "content": {
                                    "application/json": {"schema": {"type": "string"}}
                                }
                            }
                        },
                    }
                },
            }
        }
    }
    expect_validation_failure(
        validate_document,
        unsupported_response_header,
        "header content serialization is unsupported",
    )

    deterministic = valid_root()
    deterministic["paths"] = {
        "/api/v1/things/{thingId}": {
            "get": {
                "operationId": "getThing",
                "parameters": [
                    {
                        "name": "thingId",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string"},
                    },
                    {
                        "name": "expand",
                        "in": "query",
                        "schema": {"type": "boolean"},
                    },
                ],
                "responses": {
                    "200": {
                        "description": "Thing",
                        "headers": {
                            "ETag": {"schema": {"type": "string"}}
                        },
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "additionalProperties": False,
                                    "required": ["thingId"],
                                    "properties": {"thingId": {"type": "string"}},
                                }
                            }
                        },
                    }
                },
            }
        }
    }
    deterministic_index = validate_document(deterministic)
    first_ir = render_compiler_ir(compile_operation_ir(deterministic, deterministic_index))
    second_ir = render_compiler_ir(compile_operation_ir(deterministic, deterministic_index))
    if first_ir != second_ir:
        raise AssertionError("compiler IR is not deterministic for identical validated input")
    decoded_ir = json.loads(first_ir)
    if decoded_ir.get("schemaVersion") != 1 or len(decoded_ir.get("operations", [])) != 1:
        raise AssertionError("compiler IR lost operation structure")
    compiled_operation = decoded_ir["operations"][0]
    if compiled_operation["operationId"] != "getThing":
        raise AssertionError("compiler IR lost operation identity")
    if compiled_operation["responses"][0]["headers"][0]["name"] != "ETag":
        raise AssertionError("compiler IR lost declared response-header validation semantics")

    expect_rust_validation_failure(
        {"openapi": "3.0.3", "info": {"title": "mixed", "version": "1"}, "paths": {}},
        "must be exactly 3.1.0",
    )
    expect_rust_validation_failure(
        {
            "openapi": "3.1.0",
            "info": {"title": "network-ref", "version": "1"},
            "paths": {},
            "components": {
                "schemas": {"Remote": {"$ref": "https://example.invalid/schema.json"}}
            },
        },
        "network OpenAPI reference is forbidden",
    )
    expect_rust_validation_failure(
        {
            "openapi": "3.1.0",
            "info": {"title": "nullable", "version": "1"},
            "paths": {},
            "components": {
                "schemas": {"Legacy": {"type": "string", "nullable": True}}
            },
        },
        "legacy OpenAPI nullable is forbidden",
    )
    expect_rust_validation_failure(
        {
            "openapi": "3.1.0",
            "info": {"title": "problem", "version": "1"},
            "paths": {
                "/api/v1/problem": {
                    "get": {
                        "operationId": "getProblem",
                        "responses": {
                            "400": {
                                "description": "problem",
                                "content": {
                                    "application/problem+json": {
                                        "schema": {"type": "object"}
                                    }
                                },
                            }
                        },
                    }
                }
            },
        },
        "permissive application/problem+json schema is forbidden",
    )
    expect_rust_validation_failure(
        {
            "openapi": "3.1.0",
            "info": {"title": "legacy-repair", "version": "1"},
            "paths": {},
            "x-part-crm-request-digest": {"canonicalization": "part-crm-json-v1"},
        },
        "compiler repair extension is forbidden",
    )

    pass_through = valid_root()
    completed = run_rust(pass_through)
    if completed.returncode != 0:
        raise AssertionError(completed.stderr)
    if json.loads(completed.stdout) != pass_through:
        raise AssertionError("Rust validator mutated valid producer output")

    print("PAS-2 deliberately breaking frontend OpenAPI/compiler fixtures rejected as expected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
