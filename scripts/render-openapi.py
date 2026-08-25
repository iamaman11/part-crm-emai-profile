#!/usr/bin/env python3
"""Render the canonical OpenAPI v1 document from root plus additive fragments."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts" / "check-contract-compatibility.py"


def load_checker() -> ModuleType:
    spec = importlib.util.spec_from_file_location("contract_checker", CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {CHECKER_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def render(checker: ModuleType, root: Path) -> str:
    document, _root_path = checker.load_openapi_tree(root)
    return json.dumps(document, indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="prove repeated canonical composition is byte-identical without writing output",
    )
    args = parser.parse_args()

    if args.check and args.output is not None:
        parser.error("--check and --output are mutually exclusive")

    checker = load_checker()
    rendered = render(checker, args.root)
    if args.check:
        repeated = render(checker, args.root)
        if rendered.encode("utf-8") != repeated.encode("utf-8"):
            raise SystemExit("canonical OpenAPI render is not byte-identical across repeated composition")
        print("Canonical OpenAPI render is deterministic and current")
        return 0

    if args.output is None:
        print(rendered, end="")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
