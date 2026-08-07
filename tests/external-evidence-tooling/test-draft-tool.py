#!/usr/bin/env python3
"""Regression tests for fail-safe external evidence draft preparation."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "scripts/prepare-external-evidence.py"
BASE_VALIDATOR = ROOT / "scripts/check-external-evidence.py"
SCOPE_VALIDATOR = ROOT / "scripts/check-external-evidence-scope.py"
REFERENCE = "https://github.com/iamaman11/part-crm-emai-profile/issues/3"
OBSERVED_AT = "2026-08-07T19:15:00Z"


def run(*arguments: str, expect_success: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [sys.executable, str(TOOL), *arguments],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if expect_success and result.returncode != 0:
        raise AssertionError(f"tool failed: {result.stderr}")
    if not expect_success and result.returncode == 0:
        raise AssertionError(f"negative command unexpectedly passed: {result.stdout}")
    return result


def draft_arguments(
    *,
    gate: str,
    environment: str,
    evidence_id: str,
    output: Path,
    status: str = "pending",
    reference: str = REFERENCE,
    observed_at: str = OBSERVED_AT,
) -> list[str]:
    return [
        "draft",
        "--gate",
        gate,
        "--status",
        status,
        "--evidence-id",
        evidence_id,
        "--observed-at",
        observed_at,
        "--environment",
        environment,
        "--subject-id",
        f"subject-{gate.replace('_', '-')}",
        "--reference",
        reference,
        "--limitation",
        "external-operation-pending",
        "--output",
        str(output),
    ]


def verify_describe_contract() -> dict[str, dict[str, list[str]]]:
    payload = json.loads(run("describe").stdout)
    assert payload["draft_status"] == "pending_only"
    assert payload["gate_count"] == 12
    gates = payload["gates"]
    assert len(gates) == 12
    for gate, contract in gates.items():
        assert contract["allowed_environments"]
        assert contract["required_checks"]
        single = json.loads(run("describe", "--gate", gate).stdout)
        assert single == {
            "allowed_environments": contract["allowed_environments"],
            "draft_status": "pending_only",
            "gate": gate,
            "required_checks": contract["required_checks"],
        }
    return gates


def verify_all_gate_drafts(gates: dict[str, dict[str, list[str]]]) -> None:
    with tempfile.TemporaryDirectory(prefix="external-evidence-tooling-") as temporary:
        root = Path(temporary)
        records = root / "evidence/external/records"
        records.mkdir(parents=True)

        for index, (gate, gate_contract) in enumerate(sorted(gates.items()), start=1):
            evidence_id = f"ev-20260807-draft-{index:02d}"
            output = records / f"{evidence_id}.json"
            environment = gate_contract["allowed_environments"][0]
            run(*draft_arguments(
                gate=gate,
                environment=environment,
                evidence_id=evidence_id,
                output=output,
            ))
            data = json.loads(output.read_text(encoding="utf-8"))
            assert data["status"] == "pending"
            assert data["checks"] == []
            assert data["artifact_digests_sha256"] == []
            assert "review" not in data
            assert data["gate"] == gate
            assert data["scope"]["environment"] == environment

        for validator in (BASE_VALIDATOR, SCOPE_VALIDATOR):
            result = subprocess.run(
                [sys.executable, str(validator), "--root", str(root)],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=False,
            )
            if result.returncode != 0:
                raise AssertionError(f"generated drafts failed {validator.name}: {result.stderr}")


def verify_fail_closed(gates: dict[str, dict[str, list[str]]]) -> None:
    with tempfile.TemporaryDirectory(prefix="external-evidence-tooling-negative-") as temporary:
        records = Path(temporary)
        terminal = records / "ev-20260807-terminal.json"
        run(
            *draft_arguments(
                gate="product_license",
                environment="none",
                evidence_id="ev-20260807-terminal",
                output=terminal,
                status="passed",
            ),
            expect_success=False,
        )
        assert not terminal.exists()

        wrong_environment = records / "ev-20260807-wrong-environment.json"
        run(
            *draft_arguments(
                gate="trusted_windows_signing",
                environment="staging",
                evidence_id="ev-20260807-wrong-environment",
                output=wrong_environment,
            ),
            expect_success=False,
        )
        assert not wrong_environment.exists()

        wrong_date = records / "ev-20260806-wrong-date.json"
        run(
            *draft_arguments(
                gate="product_license",
                environment="none",
                evidence_id="ev-20260806-wrong-date",
                output=wrong_date,
            ),
            expect_success=False,
        )
        assert not wrong_date.exists()

        unsafe_reference = records / "ev-20260807-unsafe-reference.json"
        run(
            *draft_arguments(
                gate="product_license",
                environment="none",
                evidence_id="ev-20260807-unsafe-reference",
                output=unsafe_reference,
                reference=f"{REFERENCE}?token=forbidden",
            ),
            expect_success=False,
        )
        assert not unsafe_reference.exists()

        first_gate = sorted(gates)[0]
        environment = gates[first_gate]["allowed_environments"][0]
        existing = records / "ev-20260807-existing.json"
        arguments = draft_arguments(
            gate=first_gate,
            environment=environment,
            evidence_id="ev-20260807-existing",
            output=existing,
        )
        run(*arguments)
        original = existing.read_bytes()
        run(*arguments, expect_success=False)
        assert existing.read_bytes() == original

        wrong_name = records / "not-the-evidence-id.json"
        run(
            *draft_arguments(
                gate="product_license",
                environment="none",
                evidence_id="ev-20260807-correct-name",
                output=wrong_name,
            ),
            expect_success=False,
        )
        assert not wrong_name.exists()

    run("describe", "--gate", "not-a-gate", expect_success=False)


def main() -> int:
    gates = verify_describe_contract()
    verify_all_gate_drafts(gates)
    verify_fail_closed(gates)
    print("external evidence draft tooling tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
