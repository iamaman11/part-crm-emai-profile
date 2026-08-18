#!/usr/bin/env python3
"""Validate Cloudflare Worker secret binding names without accepting secret values."""

from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path
from typing import Any

MAX_DOCUMENT_BYTES = 256 * 1024
FORBIDDEN_VALUE_KEYS = {
    "value",
    "plaintext",
    "secret_value",
    "token_value",
    "credential_value",
    "private_key",
    "key_hex",
    "keyhex",
}
ALLOWED_SECRET_TYPES = {"secret_text"}


class ValidationError(ValueError):
    """Fail-closed metadata validation error."""


def fail(message: str) -> None:
    raise ValidationError(message)


def load_json(path: Path, label: str) -> Any:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be one regular file")
    size = path.stat().st_size
    if size <= 0 or size > MAX_DOCUMENT_BYTES:
        fail(f"{label} has an invalid bounded size")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"{label} is not strict UTF-8 JSON") from error


def require_object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be one JSON object")
    return value


def reject_value_shaped_fields(value: Any, path: str = "$SECRET_LIST") -> None:
    if isinstance(value, list):
        for index, item in enumerate(value):
            reject_value_shaped_fields(item, f"{path}[{index}]")
        return
    if not isinstance(value, dict):
        return
    for name, item in value.items():
        if name.lower() in FORBIDDEN_VALUE_KEYS:
            fail(f"secret-list metadata contains forbidden value-shaped field {path}.{name}")
        reject_value_shaped_fields(item, f"{path}.{name}")


def required_secret_names(config: Any, environment: str) -> list[str]:
    document = require_object(config, "rendered Wrangler config")
    environments = require_object(document.get("env"), "rendered Wrangler env")
    selected = require_object(environments.get(environment), f"rendered Wrangler env.{environment}")
    secrets = require_object(selected.get("secrets"), f"rendered Wrangler env.{environment}.secrets")
    required = secrets.get("required")
    if not isinstance(required, list) or not required:
        fail(f"rendered Wrangler env.{environment}.secrets.required must be non-empty")
    if any(not isinstance(name, str) or not valid_binding_name(name) for name in required):
        fail("required Worker secret binding names are malformed")
    if len(required) != len(set(required)):
        fail("required Worker secret binding names contain duplicates")
    return sorted(required)


def listed_secret_names(secret_list: Any) -> list[str]:
    reject_value_shaped_fields(secret_list)
    if not isinstance(secret_list, list):
        fail("wrangler secret list --format json must produce one JSON array")
    names: list[str] = []
    for index, item in enumerate(secret_list):
        entry = require_object(item, f"secret-list entry {index}")
        name = entry.get("name")
        secret_type = entry.get("type")
        if not isinstance(name, str) or not valid_binding_name(name):
            fail("secret-list entry has an invalid binding name")
        if secret_type not in ALLOWED_SECRET_TYPES:
            fail(f"secret-list entry {name} has unsupported type {secret_type!r}")
        names.append(name)
    if len(names) != len(set(names)):
        fail("secret-list metadata contains duplicate binding names")
    return sorted(names)


def validate(config: Any, environment: str, secret_list: Any) -> None:
    if environment not in {"staging", "production"}:
        fail("secret binding validation environment is not canonical")
    expected = required_secret_names(config, environment)
    actual = listed_secret_names(secret_list)
    if actual != expected:
        missing = sorted(set(expected) - set(actual))
        unexpected = sorted(set(actual) - set(expected))
        fail(
            "Worker secret binding inventory drifted: "
            f"missing={missing}, unexpected={unexpected}"
        )


def valid_binding_name(value: str) -> bool:
    return 1 <= len(value) <= 128 and all(
        character.isascii() and (character.isupper() or character.isdigit() or character == "_")
        for character in value
    )


def check(config_path: Path, environment: str, secret_list_path: Path) -> None:
    validate(
        load_json(config_path, "rendered Wrangler config"),
        environment,
        load_json(secret_list_path, "wrangler secret-list metadata"),
    )
    print(f"Worker secret binding metadata exactly matches secrets.required for {environment}.")


def expect_rejected(label: str, operation: Any) -> None:
    try:
        operation()
    except ValidationError:
        return
    fail(f"negative Worker secret binding fixture unexpectedly passed: {label}")


def self_test() -> None:
    config = {
        "env": {
            "staging": {
                "secrets": {
                    "required": [
                        "CALLER_AUTH_KEY",
                        "ENCRYPTION_KEYRING",
                    ]
                }
            },
            "production": {
                "secrets": {
                    "required": [
                        "CALLER_AUTH_KEY",
                        "ENCRYPTION_KEYRING",
                    ]
                }
            },
        }
    }
    valid = [
        {"name": "ENCRYPTION_KEYRING", "type": "secret_text"},
        {"name": "CALLER_AUTH_KEY", "type": "secret_text"},
    ]
    validate(config, "staging", valid)

    expect_rejected(
        "missing required secret",
        lambda: validate(config, "staging", valid[:-1]),
    )
    expect_rejected(
        "unexpected stale secret",
        lambda: validate(
            config,
            "staging",
            valid + [{"name": "STALE_SECRET", "type": "secret_text"}],
        ),
    )
    expect_rejected(
        "secret value field",
        lambda: validate(
            config,
            "staging",
            [{"name": "CALLER_AUTH_KEY", "type": "secret_text", "value": "forbidden"}],
        ),
    )
    expect_rejected(
        "missing secrets.required contract",
        lambda: validate({"env": {"staging": {}}}, "staging", valid),
    )
    expect_rejected(
        "noncanonical environment",
        lambda: validate(config, "prod", valid),
    )
    print("Worker secret binding metadata validator self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)
    check_parser = subcommands.add_parser("check")
    check_parser.add_argument("--config", type=Path, required=True)
    check_parser.add_argument("--environment", required=True)
    check_parser.add_argument("--secret-list", type=Path, required=True)
    subcommands.add_parser("self-test")
    return result


def main() -> None:
    args = parser().parse_args()
    try:
        if args.command == "check":
            check(args.config, args.environment, args.secret_list)
        elif args.command == "self-test":
            self_test()
        else:
            fail("unknown command")
    except ValidationError as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
