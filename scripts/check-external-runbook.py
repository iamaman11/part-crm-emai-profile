#!/usr/bin/env python3
"""Fail closed when external-gate operator runbook coverage drifts from validators."""

from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from pathlib import Path
from types import ModuleType

SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_RUNBOOK = Path("docs/EXTERNAL_GATE_EXECUTION_RUNBOOK.md")
MARKER_RE = re.compile(
    r"^<!-- external-gate: ([a-z][a-z0-9_]*) -->$", re.MULTILINE
)


class RunbookValidationError(ValueError):
    pass


def load_prepare_module() -> ModuleType:
    path = SCRIPT_DIR / "prepare-external-evidence.py"
    spec = importlib.util.spec_from_file_location("external_evidence_prepare_contract", path)
    if spec is None or spec.loader is None:
        raise RunbookValidationError("unable to load external evidence preparation contract")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def accepted_gates() -> tuple[str, ...]:
    module = load_prepare_module()
    contract = getattr(module, "contract", None)
    if not callable(contract):
        raise RunbookValidationError("external evidence preparation contract is unavailable")
    gates = contract()
    if not isinstance(gates, dict) or not gates:
        raise RunbookValidationError("external evidence gate catalog is empty or invalid")
    return tuple(sorted(gates))


def validate_runbook(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    gates = accepted_gates()
    accepted = set(gates)
    matches = list(MARKER_RE.finditer(text))
    errors: list[str] = []

    seen: dict[str, int] = {}
    for match in matches:
        gate = match.group(1)
        line = text.count("\n", 0, match.start()) + 1
        if gate not in accepted:
            errors.append(f"{path}:{line}: unknown external gate marker {gate!r}")
        if gate in seen:
            errors.append(
                f"{path}:{line}: duplicate external gate marker {gate!r}; first at line {seen[gate]}"
            )
        else:
            seen[gate] = line

    missing = sorted(accepted - seen.keys())
    if missing:
        errors.append(f"{path}: missing external gate sections: {missing}")

    for index, match in enumerate(matches):
        gate = match.group(1)
        if gate not in accepted:
            continue
        section_end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        section = text[match.end():section_end]
        expected = f"python scripts/prepare-external-evidence.py describe --gate {gate}"
        if expected not in section:
            errors.append(
                f"{path}:{seen[gate]}: gate section {gate!r} must invoke its exact validator-derived describe command"
            )

    if len(matches) != len(gates):
        errors.append(
            f"{path}: external gate marker count {len(matches)} does not equal accepted gate count {len(gates)}"
        )
    return errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--runbook", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    path = args.runbook if args.runbook is not None else root / DEFAULT_RUNBOOK
    if not path.is_absolute():
        path = (root / path).resolve()
    try:
        errors = validate_runbook(path)
    except (OSError, RunbookValidationError, ValueError) as error:
        print(f"external runbook coverage gate failed: {error}", file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(
        f"external runbook coverage gate passed: {len(accepted_gates())} validator-derived gate section(s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
