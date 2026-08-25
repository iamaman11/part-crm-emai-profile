#!/usr/bin/env python3
"""Generate canonical OpenAPI fragments from capability-owned Rust contracts.

PAS-2 Transaction B deliberately removes the historical Rust -> TypeScript schema projection.
Canonical OpenAPI is the sole frontend compiler input; TypeScript runtime operations are generated
separately from the validated composed OpenAPI document.
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
CLIENT_REGISTRY_OPENAPI_PATH = ROOT / "openapi" / "v1" / "fragments" / "client-registry.json"
QUERY_MAIL_OPENAPI_PATH = ROOT / "openapi" / "v1" / "fragments" / "query-mail.json"
OPERATOR_QUERY_OPENAPI_PATH = ROOT / "openapi" / "v1" / "fragments" / "operator-query.json"
CLIENT_MAIL_SEND_OPENAPI_PATH = ROOT / "openapi" / "v1" / "fragments" / "client-mail-send.json"


def run_export(bin_name: str, *arguments: str) -> dict[str, Any]:
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
    if not isinstance(document, dict):
        raise ValueError(f"{bin_name}: exported OpenAPI fragment must be an object")
    return document


def compact_json(document: dict[str, Any]) -> str:
    return json.dumps(
        document,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ) + "\n"


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
        if actual != expected:
            print(
                f"generated OpenAPI fragment drift: {path.relative_to(ROOT)}; "
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if canonical generated fragments differ")
    args = parser.parse_args()

    compatibility_registry = run_export("export_client_registry", "compatibility")
    query_mail = run_export("export_query_mail")
    operator_query = run_export("export_operator_query")
    client_mail_send = run_export("export_client_mail_send")

    results = [
        check_or_write(
            CLIENT_REGISTRY_OPENAPI_PATH,
            compact_json(compatibility_registry),
            args.check,
        ),
        check_or_write(QUERY_MAIL_OPENAPI_PATH, compact_json(query_mail), args.check),
        check_or_write(OPERATOR_QUERY_OPENAPI_PATH, compact_json(operator_query), args.check),
        check_or_write(CLIENT_MAIL_SEND_OPENAPI_PATH, compact_json(client_mail_send), args.check),
    ]
    if not all(results):
        return 1
    print(
        "canonical frontend OpenAPI fragments are deterministic and current"
        if args.check
        else "canonical frontend OpenAPI fragments generated"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(f"frontend OpenAPI generation failed closed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
