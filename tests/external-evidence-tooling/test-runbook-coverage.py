#!/usr/bin/env python3
"""Regression tests for validator-derived external gate runbook coverage."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[2]
CHECKER_PATH = ROOT / "scripts" / "check-external-runbook.py"
RUNBOOK_PATH = ROOT / "docs" / "EXTERNAL_GATE_EXECUTION_RUNBOOK.md"


def load_checker() -> ModuleType:
    spec = importlib.util.spec_from_file_location("external_runbook_checker", CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError("unable to load external runbook checker")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def expect_failure(checker: ModuleType, text: str, expected_fragment: str) -> None:
    with tempfile.TemporaryDirectory(prefix="external-runbook-test-") as temporary:
        path = Path(temporary) / "runbook.md"
        path.write_text(text, encoding="utf-8")
        errors = checker.validate_runbook(path)
    assert errors, "negative runbook fixture unexpectedly passed"
    assert any(expected_fragment in error for error in errors), errors


def main() -> int:
    checker = load_checker()
    text = RUNBOOK_PATH.read_text(encoding="utf-8")

    errors = checker.validate_runbook(RUNBOOK_PATH)
    assert not errors, errors

    gates = checker.accepted_gates()
    assert gates, "accepted external gate catalog must not be empty"
    first = gates[0]
    marker = f"<!-- external-gate: {first} -->"
    describe = f"python scripts/prepare-external-evidence.py describe --gate {first}"

    expect_failure(
        checker,
        text.replace(marker, "<!-- external-gate: unknown_gate -->", 1),
        "unknown external gate marker",
    )
    expect_failure(
        checker,
        text + f"\n{marker}\n```bash\n{describe}\n```\n",
        "duplicate external gate marker",
    )
    expect_failure(
        checker,
        text.replace(describe, "python scripts/prepare-external-evidence.py describe", 1),
        "must invoke its exact validator-derived describe command",
    )

    print("external runbook coverage regressions passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
