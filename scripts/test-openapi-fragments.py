#!/usr/bin/env python3
"""Prove additive OpenAPI fragments merge deterministically and fail closed."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
from pathlib import Path
from types import ModuleType
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts" / "check-contract-compatibility.py"
GENERATION_FRAGMENT = ROOT / "openapi" / "v1" / "fragments" / "profile-generations.json"


def load_checker() -> ModuleType:
    spec = importlib.util.spec_from_file_location("contract_checker_test", CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {CHECKER_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def root_document() -> dict[str, object]:
    return {
        "openapi": "3.1.0",
        "info": {"title": "fixture", "version": "1.0.0"},
        "paths": {
            "/api/v1/base": {
                "get": {
                    "operationId": "getBase",
                    "responses": {"200": {"description": "ok"}},
                }
            }
        },
        "components": {"schemas": {"Base": {"type": "object"}}},
    }


def expect_value_error(operation: Callable[[], object], fragment: str) -> None:
    try:
        operation()
    except ValueError as error:
        if fragment not in str(error):
            raise AssertionError(
                f"expected ValueError containing {fragment!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError(f"operation unexpectedly passed; expected {fragment!r}")


def assert_generation_fragment_surface() -> None:
    fragment = json.loads(GENERATION_FRAGMENT.read_text(encoding="utf-8"))
    paths = fragment["paths"]
    base = "/api/v1/tenants/{tenantId}/profiles/{profileId}/generations"
    expected = {
        base: ("post", "registerProfileGeneration"),
        f"{base}/{{generationId}}": ("get", "getProfileGeneration"),
        f"{base}/{{generationId}}/verify": ("post", "verifyProfileGeneration"),
        f"{base}/{{generationId}}/activate": ("post", "activateProfileGeneration"),
        f"{base}/{{generationId}}/deactivate": (
            "post",
            "deactivateProfileGeneration",
        ),
        f"{base}/{{generationId}}/quarantine": (
            "post",
            "quarantineProfileGeneration",
        ),
    }
    assert set(paths) == set(expected), set(paths)
    for path, (method, operation_id) in expected.items():
        operation = paths[path][method]
        assert operation["operationId"] == operation_id
        if method == "post":
            assert operation["responses"]["503"] == {
                "$ref": "#/components/responses/DependencyUnavailable"
            }

    schemas = fragment["components"]["schemas"]
    deactivate = schemas["DeactivateProfileGenerationRequest"]
    assert deactivate["type"] == "object"
    assert deactivate["additionalProperties"] is False
    assert set(deactivate["required"]) == {"expectedProfileVersion", "requestDigest"}
    assert set(deactivate["properties"]) == {"expectedProfileVersion", "requestDigest"}

    dependency = fragment["components"]["responses"]["DependencyUnavailable"]
    assert "application/problem+json" in dependency["content"]


def main() -> int:
    checker = load_checker()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_json(root / "openapi/v1/openapi.json", root_document())
        write_json(
            root / "openapi/v1/fragments/10-feature.json",
            {
                "paths": {
                    "/api/v1/feature": {
                        "post": {
                            "operationId": "createFeature",
                            "responses": {"201": {"description": "created"}},
                        }
                    }
                },
                "components": {"schemas": {"Feature": {"type": "object"}}},
            },
        )
        merged, _ = checker.load_openapi_tree(root)
        assert list(merged["paths"]) == ["/api/v1/base", "/api/v1/feature"]
        assert "Feature" in merged["components"]["schemas"]

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_json(root / "openapi/v1/openapi.json", root_document())
        write_json(
            root / "openapi/v1/fragments/duplicate-path.json",
            {"paths": {"/api/v1/base": {}}},
        )
        expect_value_error(
            lambda: checker.load_openapi_tree(root), "duplicate path entry"
        )

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_json(root / "openapi/v1/openapi.json", root_document())
        write_json(
            root / "openapi/v1/fragments/duplicate-schema.json",
            {"components": {"schemas": {"Base": {"type": "string"}}}},
        )
        expect_value_error(
            lambda: checker.load_openapi_tree(root),
            "duplicate components.schemas entry",
        )

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_json(root / "openapi/v1/openapi.json", root_document())
        write_json(
            root / "openapi/v1/fragments/unsupported.json",
            {"components": {"callbacks": {"Unexpected": {}}}},
        )
        expect_value_error(
            lambda: checker.load_openapi_tree(root),
            "unsupported component sections",
        )

    assert_generation_fragment_surface()
    print("Additive OpenAPI fragment governance passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
