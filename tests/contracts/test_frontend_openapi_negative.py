#!/usr/bin/env python3
"""Deliberately breaking PAS-2 frontend OpenAPI fixtures."""

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

    print("PAS-2 deliberately breaking frontend OpenAPI fixtures rejected as expected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
