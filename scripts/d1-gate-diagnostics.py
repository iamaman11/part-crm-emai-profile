#!/usr/bin/env python3
"""Fail-closed, metadata-only diagnostics for the protected D1 policy gate.

This helper deliberately records only stable gate/reason metadata. It never copies
provider responses, release manifests, preconditions, credentials, or raw opsctl
payloads into diagnostics.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
BLOCKED_DECISIONS = {
    "CODE_ROLLBACK_BLOCKED",
    "FAIL_FORWARD_REQUIRED",
    "CONTRACT_BLOCKED",
    "RECOVERY_REQUIRED",
}

REASON_CODES = {
    "D1_NATIVE_PLAN_COMMAND_FAILED",
    "D1_COMPATIBILITY_COMMAND_FAILED",
    "D1_NATIVE_PLAN_OUTPUT_INVALID",
    "D1_NATIVE_PLAN_DENIED",
    "D1_COMPATIBILITY_OUTPUT_INVALID",
    "D1_COMPATIBILITY_DENIED",
    "D1_ROLLBACK_POLICY_BLOCKED",
    "D1_CONTRACT_PLAN_MIGRATION_MISMATCH",
    "D1_CONTRACT_RECOVERY_STRATEGY_MISMATCH",
}


@dataclass(frozen=True)
class DiagnosticFailure(Exception):
    gate: str
    reason_code: str
    detail: str
    exit_code: int = 2


def _load_json_object(path: Path, gate: str, reason_code: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise DiagnosticFailure(gate, reason_code, f"JSON object unavailable: {type(exc).__name__}") from exc
    if not isinstance(value, dict):
        raise DiagnosticFailure(gate, reason_code, "output is not one JSON object")
    return value


def _bounded_scalar(value: Any) -> str:
    if isinstance(value, (str, int, float, bool)) or value is None:
        text = str(value)
        return text[:160]
    return type(value).__name__


def evaluate_policy(plan_path: Path, compatibility_path: Path) -> bool:
    plan = _load_json_object(plan_path, "native-plan-output", "D1_NATIVE_PLAN_OUTPUT_INVALID")
    compatibility = _load_json_object(
        compatibility_path,
        "compatibility-output",
        "D1_COMPATIBILITY_OUTPUT_INVALID",
    )

    # Preserve the old fail-closed result while making the most specific policy
    # reason observable even when opsctl also reports allowed=false.
    decision = plan.get("decision")
    if decision in BLOCKED_DECISIONS:
        raise DiagnosticFailure(
            "rollback-policy",
            "D1_ROLLBACK_POLICY_BLOCKED",
            f"decision={_bounded_scalar(decision)}",
        )

    if plan.get("command") == "d1 contract-transition":
        if plan.get("planned_migrations") != ["0032_pas2_payload_fingerprint_contract.sql"]:
            raise DiagnosticFailure(
                "contract-policy",
                "D1_CONTRACT_PLAN_MIGRATION_MISMATCH",
                "contract transition is not the exact sole 0032 migration",
            )
        if plan.get("recovery_strategy") != "FAIL_FORWARD_ONLY":
            raise DiagnosticFailure(
                "contract-policy",
                "D1_CONTRACT_RECOVERY_STRATEGY_MISMATCH",
                f"recovery_strategy={_bounded_scalar(plan.get('recovery_strategy'))}",
            )

    if plan.get("allowed") is not True:
        raise DiagnosticFailure(
            "native-plan-policy",
            "D1_NATIVE_PLAN_DENIED",
            f"allowed={_bounded_scalar(plan.get('allowed'))}; decision={_bounded_scalar(plan.get('decision'))}",
        )
    if compatibility.get("allowed") is not True:
        raise DiagnosticFailure(
            "compatibility-policy",
            "D1_COMPATIBILITY_DENIED",
            f"allowed={_bounded_scalar(compatibility.get('allowed'))}; decision={_bounded_scalar(compatibility.get('decision'))}",
        )

    return bool(plan.get("planned_migrations"))


def _gha_escape(text: str) -> str:
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def record_diagnostic(
    diagnostics_path: Path,
    *,
    gate: str,
    reason_code: str,
    exit_code: int,
    detail: str,
    publish: bool = True,
) -> None:
    if reason_code not in REASON_CODES:
        raise SystemExit(f"unknown D1 diagnostic reason code: {reason_code}")
    diagnostics_path.parent.mkdir(parents=True, exist_ok=True)
    record = {
        "schema_version": SCHEMA_VERSION,
        "gate": gate,
        "reason_code": reason_code,
        "exit_code": int(exit_code),
        "allowed": False,
        "detail": detail,
    }
    with diagnostics_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")

    if not publish:
        return

    print(
        f"::error title={_gha_escape(reason_code)}::"
        f"D1 fail-closed gate={_gha_escape(gate)} reason_code={_gha_escape(reason_code)} "
        f"exit_code={int(exit_code)} detail={_gha_escape(detail)}"
    )

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        try:
            with Path(summary).open("a", encoding="utf-8") as handle:
                handle.write(
                    "### D1 fail-closed gate diagnostic\n"
                    f"- gate: `{gate}`\n"
                    f"- reason_code: `{reason_code}`\n"
                    f"- exit_code: `{int(exit_code)}`\n"
                    f"- detail: `{detail}`\n"
                )
        except OSError as exc:
            print(f"warning: unable to append GitHub step summary: {type(exc).__name__}", file=sys.stderr)


def _write_apply_required(path: Path, required: bool) -> None:
    with path.open("a", encoding="utf-8") as handle:
        handle.write(f"apply_required={'true' if required else 'false'}\n")


def command_record(args: argparse.Namespace) -> int:
    record_diagnostic(
        Path(args.diagnostics),
        gate=args.gate,
        reason_code=args.reason_code,
        exit_code=args.exit_code,
        detail=args.detail,
    )
    return 0


def command_evaluate(args: argparse.Namespace) -> int:
    diagnostics = Path(args.diagnostics)
    try:
        required = evaluate_policy(Path(args.plan), Path(args.compatibility))
    except DiagnosticFailure as exc:
        record_diagnostic(
            diagnostics,
            gate=exc.gate,
            reason_code=exc.reason_code,
            exit_code=exc.exit_code,
            detail=exc.detail,
        )
        return exc.exit_code
    _write_apply_required(Path(args.github_output), required)
    return 0


def _write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value) + "\n", encoding="utf-8")


def command_self_test(_: argparse.Namespace) -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        plan_path = root / "plan.json"
        compatibility_path = root / "compatibility.json"
        diagnostics_path = root / "diagnostics.jsonl"
        output_path = root / "github-output"

        good_plan = {
            "allowed": True,
            "decision": "SAFE",
            "command": "d1 plan",
            "planned_migrations": ["0027_pas2_payload_fingerprint_expand.sql"],
        }
        good_compatibility = {"allowed": True, "decision": "SAFE"}
        _write_json(plan_path, good_plan)
        _write_json(compatibility_path, good_compatibility)
        assert evaluate_policy(plan_path, compatibility_path) is True

        cases = [
            ("D1_NATIVE_PLAN_OUTPUT_INVALID", "{not-json\n", good_compatibility),
            ("D1_NATIVE_PLAN_DENIED", {**good_plan, "allowed": False}, good_compatibility),
            ("D1_COMPATIBILITY_OUTPUT_INVALID", good_plan, "[]\n"),
            ("D1_COMPATIBILITY_DENIED", good_plan, {"allowed": False, "decision": "UNSAFE"}),
            (
                "D1_ROLLBACK_POLICY_BLOCKED",
                {**good_plan, "allowed": False, "decision": "FAIL_FORWARD_REQUIRED"},
                good_compatibility,
            ),
            (
                "D1_CONTRACT_PLAN_MIGRATION_MISMATCH",
                {
                    **good_plan,
                    "allowed": False,
                    "command": "d1 contract-transition",
                    "planned_migrations": ["0031_device_binding_governance.sql"],
                    "recovery_strategy": "FAIL_FORWARD_ONLY",
                },
                good_compatibility,
            ),
            (
                "D1_CONTRACT_RECOVERY_STRATEGY_MISMATCH",
                {
                    **good_plan,
                    "allowed": False,
                    "command": "d1 contract-transition",
                    "planned_migrations": ["0032_pas2_payload_fingerprint_contract.sql"],
                    "recovery_strategy": "RESTORE",
                },
                good_compatibility,
            ),
        ]
        for expected, plan_value, compatibility_value in cases:
            if isinstance(plan_value, str):
                plan_path.write_text(plan_value, encoding="utf-8")
            else:
                _write_json(plan_path, plan_value)
            if isinstance(compatibility_value, str):
                compatibility_path.write_text(compatibility_value, encoding="utf-8")
            else:
                _write_json(compatibility_path, compatibility_value)
            try:
                evaluate_policy(plan_path, compatibility_path)
            except DiagnosticFailure as exc:
                assert exc.reason_code == expected, (expected, exc)
            else:
                raise AssertionError(f"negative diagnostic fixture unexpectedly passed: {expected}")

        for reason_code, gate in (
            ("D1_NATIVE_PLAN_COMMAND_FAILED", "native-plan-command"),
            ("D1_COMPATIBILITY_COMMAND_FAILED", "compatibility-command"),
        ):
            record_diagnostic(
                diagnostics_path,
                gate=gate,
                reason_code=reason_code,
                exit_code=17,
                detail="self-test command failure",
                publish=False,
            )
        records = [json.loads(line) for line in diagnostics_path.read_text(encoding="utf-8").splitlines()]
        assert [record["reason_code"] for record in records] == [
            "D1_NATIVE_PLAN_COMMAND_FAILED",
            "D1_COMPATIBILITY_COMMAND_FAILED",
        ]
        assert all(record["allowed"] is False for record in records)
        assert all(set(record) == {"schema_version", "gate", "reason_code", "exit_code", "allowed", "detail"} for record in records)

        _write_apply_required(output_path, True)
        assert output_path.read_text(encoding="utf-8") == "apply_required=true\n"

    print("D1 fail-closed diagnostic helper self-test passed.")
    return 0


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    sub = root.add_subparsers(dest="command", required=True)

    record = sub.add_parser("record", help="append one metadata-only fail-closed diagnostic")
    record.add_argument("--diagnostics", required=True)
    record.add_argument("--gate", required=True)
    record.add_argument("--reason-code", required=True, choices=sorted(REASON_CODES))
    record.add_argument("--exit-code", required=True, type=int)
    record.add_argument("--detail", required=True)
    record.set_defaults(func=command_record)

    evaluate = sub.add_parser("evaluate", help="evaluate plan/compatibility policy and emit stable diagnostics on denial")
    evaluate.add_argument("--plan", required=True)
    evaluate.add_argument("--compatibility", required=True)
    evaluate.add_argument("--diagnostics", required=True)
    evaluate.add_argument("--github-output", required=True)
    evaluate.set_defaults(func=command_evaluate)

    self_test = sub.add_parser("self-test", help="run dependency-free negative diagnostic fixtures")
    self_test.set_defaults(func=command_self_test)
    return root


def main() -> int:
    args = parser().parse_args()
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
