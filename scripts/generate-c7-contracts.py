#!/usr/bin/env python3
"""Generate C7 TypeScript from the Rust-checked committed OpenAPI artifact."""

from __future__ import annotations

import argparse
import json
import runpy
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
OPENAPI_PATH = ROOT / "contracts" / "generated" / "client-mail-send.openapi.json"
TYPESCRIPT_PATH = (
    ROOT / "frontend" / "src" / "shared" / "api" / "generated" / "client-mail-send.ts"
)
SOURCE_PATH = "crates/control-plane-contract/src/client_mail_send_api.rs"
SHARED_GENERATOR = ROOT / "scripts" / "generate-frontend-contracts.py"


def load_openapi() -> dict[str, Any]:
    document = json.loads(OPENAPI_PATH.read_text(encoding="utf-8"))
    if not isinstance(document, dict):
        raise ValueError("C7 OpenAPI artifact must be a JSON object")
    return document


def expected_typescript() -> str:
    shared = runpy.run_path(str(SHARED_GENERATOR))
    render_typescript = shared["render_typescript"]
    rendered = render_typescript(
        load_openapi(),
        source_path=SOURCE_PATH,
        support_nullable=True,
    )
    return rendered.replace(
        "// Regenerate with: python scripts/generate-frontend-contracts.py",
        "// Regenerate with: python scripts/generate-c7-contracts.py",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = expected_typescript()
    if args.check:
        try:
            actual = TYPESCRIPT_PATH.read_text(encoding="utf-8")
        except FileNotFoundError:
            actual = ""
        if actual != expected:
            print("generated C7 TypeScript contract is stale")
            return 1
        print("C7 TypeScript contract is deterministic and current")
        return 0
    TYPESCRIPT_PATH.parent.mkdir(parents=True, exist_ok=True)
    TYPESCRIPT_PATH.write_text(expected, encoding="utf-8", newline="\n")
    print(f"wrote {TYPESCRIPT_PATH.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
