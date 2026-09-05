#!/usr/bin/env python3
"""Credential-free D1 prepare adapter.

This module composes the existing typed `opsctl d1` read-only surfaces without
reimplementing migration, compatibility, rollback, or precondition semantics.
It emits one secret-free PREPARE_READY/PREPARE_BLOCKED envelope before any
mutation authorization exists.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1


def _load_object_text(raw: str, label: str) -> dict[str, Any]:
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(f"{label} is not one JSON object") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{label} is not one JSON object")
    return value


def _fallback_gate(gate_id: str, reason_code: str, summary: str, observed: str) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "prepare_id": None,
        "transaction_id": None,
        "phase": "PREPARE",
        "gate_id": gate_id,
        "status": "ERROR",
        "reason_code": reason_code,
        "summary": summary,
        "expected": "typed credential-free opsctl result",
        "observed": observed[:160],
        "remediation": "Repair the credential-free input/tooling condition, then rerun prepare before requesting authorization.",
        "tool": {"name": "d1-prepare", "surface": "adapter", "version": "1"},
    }


def _typed_failure(stderr: str, gate_id: str, outer_reason: str) -> dict[str, Any]:
    try:
        value = _load_object_text(stderr.strip(), "opsctl stderr")
    except ValueError:
        return _fallback_gate(
            gate_id,
            outer_reason,
            "credential-free opsctl command failed without a typed error envelope",
            "stderr was not one typed JSON object",
        )
    gate = value.get("gate_result")
    if isinstance(gate, dict):
        return gate
    return _fallback_gate(
        gate_id,
        outer_reason,
        "credential-free opsctl command failed without gate_result",
        f"command={value.get('command')!r}; error={str(value.get('error'))[:96]}",
    )


def _denied_gate(plan: dict[str, Any], compatibility: dict[str, Any]) -> dict[str, Any]:
    source = plan if plan.get("allowed") is not True else compatibility
    reasons = source.get("reason_codes")
    reason_code = None
    if isinstance(reasons, list):
        reason_code = next((item for item in reasons if isinstance(item, str) and item), None)
    if reason_code is None:
        reason_code = "D1_NATIVE_PLAN_DENIED" if source is plan else "D1_COMPATIBILITY_DENIED"
    return {
        "schema_version": 1,
        "prepare_id": None,
        "transaction_id": None,
        "phase": "PREPARE",
        "gate_id": "d1.plan.admission" if source is plan else "d1.compatibility.admission",
        "status": "BLOCKED",
        "reason_code": reason_code,
        "summary": "typed D1 policy denied prepare admission",
        "expected": "allowed=true",
        "observed": f"allowed={source.get('allowed')!r}; decision={source.get('decision')!r}; ledger_state={source.get('ledger_state')!r}",
        "remediation": "Resolve the condition identified by the typed D1 reason_code at its natural owner, then rerun prepare before requesting authorization.",
        "tool": {"name": "opsctl", "surface": "d1", "version": "project-pinned"},
    }


def _envelope(component: str, plan: dict[str, Any] | None, compatibility: dict[str, Any] | None, gate_results: list[dict[str, Any]]) -> dict[str, Any]:
    ready = plan is not None and compatibility is not None and plan.get("allowed") is True and compatibility.get("allowed") is True and not gate_results
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "d1 prepare",
        "status": "PREPARE_READY" if ready else "PREPARE_BLOCKED",
        "mode": "read-only",
        "mutation_executed": False,
        "provider_mutation_executed": False,
        "authorization_consumed": False,
        "component": component,
        "plan": plan,
        "compatibility": compatibility,
        "gate_results": gate_results,
    }


def _run_opsctl(opsctl: Path, root: Path, action: str, args: list[str]) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    env.pop("CLOUDFLARE_API_TOKEN", None)
    env.pop("CLOUDFLARE_ACCOUNT_ID", None)
    return subprocess.run(
        [str(opsctl), "--root", str(root), "d1", action, *args],
        cwd=root,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def command_prepare(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    opsctl = Path(args.opsctl).resolve()
    common = [
        "--component", args.component,
        "--ledger-json", args.ledger_json,
        "--release-manifest", args.release_manifest,
    ]
    plan_args = [
        *common,
        "--current-manifest", args.current_manifest,
        "--known-good-manifest", args.known_good_manifest,
        "--preconditions-json", args.preconditions_json,
    ]

    plan_process = _run_opsctl(opsctl, root, "plan", plan_args)
    if plan_process.returncode != 0:
        result = _envelope(
            args.component,
            None,
            None,
            [_typed_failure(plan_process.stderr, "d1.plan.command", "D1_NATIVE_PLAN_COMMAND_FAILED")],
        )
        Path(args.output).write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 3

    try:
        plan = _load_object_text(plan_process.stdout, "opsctl d1 plan stdout")
    except ValueError as exc:
        result = _envelope(args.component, None, None, [_fallback_gate("d1.plan.output", "D1_NATIVE_PLAN_OUTPUT_INVALID", str(exc), "stdout was not one JSON object")])
        Path(args.output).write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 3

    compatibility_process = _run_opsctl(opsctl, root, "compatibility", common)
    if compatibility_process.returncode != 0:
        result = _envelope(
            args.component,
            plan,
            None,
            [_typed_failure(compatibility_process.stderr, "d1.compatibility.command", "D1_COMPATIBILITY_COMMAND_FAILED")],
        )
        Path(args.output).write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 3

    try:
        compatibility = _load_object_text(compatibility_process.stdout, "opsctl d1 compatibility stdout")
    except ValueError as exc:
        result = _envelope(args.component, plan, None, [_fallback_gate("d1.compatibility.output", "D1_COMPATIBILITY_OUTPUT_INVALID", str(exc), "stdout was not one JSON object")])
        Path(args.output).write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 3

    gates: list[dict[str, Any]] = []
    if plan.get("allowed") is not True or compatibility.get("allowed") is not True:
        gates.append(_denied_gate(plan, compatibility))
    result = _envelope(args.component, plan, compatibility, gates)
    Path(args.output).write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return 0 if result["status"] == "PREPARE_READY" else 3


def command_self_test(_: argparse.Namespace) -> int:
    ready_plan = {"allowed": True, "decision": "MIGRATION_REQUIRED", "ledger_state": "BEHIND_KNOWN_PREFIX", "reason_codes": [], "planned_migrations": ["0027.sql"]}
    ready_compat = {"allowed": True, "decision": "MIGRATION_REQUIRED", "ledger_state": "BEHIND_KNOWN_PREFIX", "reason_codes": []}
    ready = _envelope("catalog", ready_plan, ready_compat, [])
    assert ready["status"] == "PREPARE_READY"
    assert ready["authorization_consumed"] is False
    assert ready["provider_mutation_executed"] is False

    blocked_plan = {**ready_plan, "allowed": False, "decision": "CODE_ROLLBACK_BLOCKED", "reason_codes": ["CURRENT_RUNTIME_CONTEXT_MISSING"]}
    gate = _denied_gate(blocked_plan, ready_compat)
    blocked = _envelope("catalog", blocked_plan, ready_compat, [gate])
    assert blocked["status"] == "PREPARE_BLOCKED"
    assert blocked["gate_results"][0]["reason_code"] == "CURRENT_RUNTIME_CONTEXT_MISSING"

    historical_attempt_a = json.dumps({
        "schema_version": 1,
        "command": "d1",
        "status": "error",
        "mode": "read-only",
        "mutation_executed": False,
        "error": "D1 contract preconditions require a string component field",
        "gate_result": {
            "schema_version": 1,
            "prepare_id": None,
            "transaction_id": None,
            "phase": "INPUT_VALIDATION",
            "gate_id": "d1.preconditions.schema",
            "status": "BLOCKED",
            "reason_code": "D1_PRECONDITIONS_COMPONENT_INVALID",
            "summary": "D1 contract preconditions require a string component field",
            "expected": "component=\"catalog\"",
            "observed": "component field absent",
            "remediation": "Regenerate typed preconditions and rerun prepare before requesting authorization.",
            "tool": {"name": "opsctl", "surface": "d1", "version": "test"},
        },
    })
    typed = _typed_failure(historical_attempt_a, "d1.plan.command", "D1_NATIVE_PLAN_COMMAND_FAILED")
    assert typed["reason_code"] == "D1_PRECONDITIONS_COMPONENT_INVALID"
    assert typed["observed"] == "component field absent"
    assert "prepare" in typed["remediation"]

    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / "prepare.json"
        output.write_text(json.dumps(blocked, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        loaded = json.loads(output.read_text(encoding="utf-8"))
        assert loaded["mutation_executed"] is False
        assert loaded["authorization_consumed"] is False

    print("D1 credential-free prepare adapter self-test passed.")
    return 0


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    sub = root.add_subparsers(dest="command", required=True)

    prepare = sub.add_parser("prepare", help="compose one credential-free prepare result")
    prepare.add_argument("--opsctl", required=True)
    prepare.add_argument("--root", required=True)
    prepare.add_argument("--component", required=True, choices=("catalog", "resolver"))
    prepare.add_argument("--ledger-json", required=True)
    prepare.add_argument("--release-manifest", required=True)
    prepare.add_argument("--current-manifest", required=True)
    prepare.add_argument("--known-good-manifest", required=True)
    prepare.add_argument("--preconditions-json", required=True)
    prepare.add_argument("--output", required=True)
    prepare.set_defaults(func=command_prepare)

    self_test = sub.add_parser("self-test", help="run dependency-free prepare fixtures")
    self_test.set_defaults(func=command_self_test)
    return root


def main() -> int:
    args = parser().parse_args()
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
