#!/usr/bin/env python3
"""Negative proof for AR-11 generated release-input drift detection.

This test exercises the existing deterministic frontend-contract generator's
check-only comparison without executing a build. It proves that a stale or
missing committed generated projection is rejected before native release-policy
tests run.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "generate-frontend-contracts.py"
AUTHORITY = ROOT / "architecture" / "release-architecture-ar11.json"


def fail(message: str) -> None:
    raise AssertionError(message)


def load_generator():
    spec = importlib.util.spec_from_file_location("ar11_frontend_contract_generator", GENERATOR)
    if spec is None or spec.loader is None:
        fail("cannot load deterministic frontend contract generator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def prove_authority_bindings() -> None:
    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    rows = authority.get("release_inputs")
    if not isinstance(rows, list) or not rows:
        fail("AR-11 release_inputs are missing")
    generated = [row for row in rows if isinstance(row, dict) and row.get("generated_projection")]
    if not generated:
        fail("AR-11 release topology contains no generated projections")
    for row in generated:
        projection = row.get("generated_projection")
        identity = row.get("release_identity_source")
        generator = row.get("generator")
        verification = row.get("verification")
        if not isinstance(projection, str) or identity != projection:
            fail(f"generated release input identity drifted: {row.get('input_id')}")
        if not isinstance(generator, str) or not (ROOT / generator).is_file():
            fail(f"generated release input lacks deterministic generator: {row.get('input_id')}")
        if not isinstance(verification, list) or not verification:
            fail(f"generated release input lacks verification: {row.get('input_id')}")


def prove_stale_and_missing_are_rejected() -> None:
    generator = load_generator()
    with tempfile.TemporaryDirectory(prefix=".ar11-generated-drift-", dir=ROOT) as directory:
        probe = Path(directory) / "projection.json"
        probe.write_text("stale\n", encoding="utf-8")
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            accepted = generator.check_or_write(probe, "expected\n", True)
        if accepted or "generated contract drift" not in stderr.getvalue():
            fail("stale generated projection negative fixture unexpectedly passed")

        probe.unlink()
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            accepted = generator.check_or_write(probe, "expected\n", True)
        if accepted or "generated contract is missing" not in stderr.getvalue():
            fail("missing generated projection negative fixture unexpectedly passed")


def main() -> int:
    prove_authority_bindings()
    prove_stale_and_missing_are_rejected()
    print("AR-11 generated release-input stale/missing projections fail closed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
