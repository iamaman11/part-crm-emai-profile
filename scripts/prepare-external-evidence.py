#!/usr/bin/env python3
"""Prepare fail-safe pending external-evidence drafts from accepted validator contracts."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path
from types import ModuleType
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent


class PreparationError(ValueError):
    pass


def load_validator(module_name: str, filename: str) -> ModuleType:
    path = SCRIPT_DIR / filename
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise PreparationError(f"unable to load validator contract: {filename}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


BASE = load_validator("external_evidence_validator_contract", "check-external-evidence.py")
SCOPE = load_validator("external_evidence_scope_contract", "check-external-evidence-scope.py")


def contract() -> dict[str, dict[str, list[str]]]:
    gates = getattr(BASE, "GATE_CHECKS", None)
    environments = getattr(SCOPE, "GATE_ENVIRONMENTS", None)
    if not isinstance(gates, dict) or not isinstance(environments, dict):
        raise PreparationError("accepted validator contract is unavailable")
    if set(gates) != set(environments):
        raise PreparationError("external evidence gate and scope contracts have drifted")
    return {
        gate: {
            "allowed_environments": sorted(environments[gate]),
            "required_checks": list(gates[gate]),
        }
        for gate in sorted(gates)
    }


def canonical_json(data: dict[str, Any]) -> str:
    canonical = getattr(BASE, "canonical_json", None)
    if not callable(canonical):
        raise PreparationError("accepted canonical JSON function is unavailable")
    return canonical(data)


def validate_candidate(data: dict[str, Any]) -> None:
    evidence_id = data["evidence_id"]
    with tempfile.TemporaryDirectory(prefix="external-evidence-draft-") as temporary:
        path = Path(temporary) / f"{evidence_id}.json"
        path.write_text(canonical_json(data), encoding="utf-8")
        try:
            BASE.validate_record(path)
            SCOPE.validate_record(path)
        except (ValueError, OSError) as error:
            raise PreparationError(str(error)) from error


def build_pending(args: argparse.Namespace) -> dict[str, Any]:
    if args.status != "pending":
        raise PreparationError("operator tooling can create pending evidence only")

    gates = contract()
    if args.gate not in gates:
        raise PreparationError(f"unsupported gate: {args.gate}")
    if args.environment not in gates[args.gate]["allowed_environments"]:
        allowed = gates[args.gate]["allowed_environments"]
        raise PreparationError(
            f"environment {args.environment!r} is invalid for {args.gate}; allowed={allowed}"
        )

    record: dict[str, Any] = {
        "artifact_digests_sha256": [],
        "checks": [],
        "evidence_id": args.evidence_id,
        "gate": args.gate,
        "limitations": args.limitation,
        "observed_at": args.observed_at,
        "references": [args.reference],
        "schema_version": 1,
        "scope": {
            "environment": args.environment,
            "subject_id": args.subject_id,
        },
        "status": "pending",
    }
    validate_candidate(record)
    return record


def write_record(record: dict[str, Any], output: Path | None) -> None:
    rendered = canonical_json(record)
    if output is None:
        sys.stdout.write(rendered)
        return
    expected_name = f"{record['evidence_id']}.json"
    if output.name != expected_name:
        raise PreparationError(f"output filename must be {expected_name}")
    if output.exists():
        raise PreparationError(f"refusing to overwrite existing evidence record: {output}")
    if not output.parent.is_dir():
        raise PreparationError(f"output directory does not exist: {output.parent}")
    output.write_text(rendered, encoding="utf-8")


def add_draft_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--gate", required=True)
    parser.add_argument("--status", default="pending")
    parser.add_argument("--evidence-id", required=True)
    parser.add_argument("--observed-at", required=True)
    parser.add_argument("--environment", required=True)
    parser.add_argument("--subject-id", required=True)
    parser.add_argument("--reference", required=True)
    parser.add_argument("--limitation", action="append", default=[])
    parser.add_argument("--output", type=Path)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Describe external gates or create validator-approved pending evidence drafts."
    )
    commands = parser.add_subparsers(dest="command", required=True)

    describe = commands.add_parser("describe", help="print the accepted external gate contract")
    describe.add_argument("--gate")

    draft = commands.add_parser(
        "draft",
        help="create a canonical pending record; terminal evidence is intentionally unsupported",
    )
    add_draft_arguments(draft)
    return parser.parse_args()


def describe(gate: str | None) -> int:
    gates = contract()
    if gate is not None:
        if gate not in gates:
            raise PreparationError(f"unsupported gate: {gate}")
        payload: object = {
            "gate": gate,
            "draft_status": "pending_only",
            **gates[gate],
        }
    else:
        payload = {
            "draft_status": "pending_only",
            "gate_count": len(gates),
            "gates": gates,
        }
    print(json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False))
    return 0


def main() -> int:
    args = parse_args()
    try:
        if args.command == "describe":
            return describe(args.gate)
        if args.command == "draft":
            record = build_pending(args)
            write_record(record, args.output)
            return 0
        raise PreparationError("unsupported command")
    except (OSError, PreparationError) as error:
        print(f"external evidence preparation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
