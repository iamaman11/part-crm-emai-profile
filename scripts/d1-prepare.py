#!/usr/bin/env python3
"""Credential-free D1 prepare adapter.

This module composes the existing typed `opsctl d1` read-only surfaces without
reimplementing migration, compatibility, rollback, or precondition semantics.
It emits one secret-free PREPARE_READY/PREPARE_BLOCKED envelope before any
mutation authorization exists.

TX-7 also uses this adapter to render the ephemeral canonical Prepare workflow
inputs from accepted local observation artifacts plus the typed D1 repository
projection. That rendering is transport adaptation only: the Rust transaction
owner still validates and seals the immutable TransactionId.
"""

from __future__ import annotations

import argparse
import hashlib
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


def _load_object_file(path: Path, label: str) -> dict[str, Any]:
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise ValueError(f"{label} is unavailable: {path}") from exc
    return _load_object_text(raw, label)


def _compact(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


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


def _build_operator_inputs(
    *,
    root: Path,
    prepare: dict[str, Any],
    observation_document: dict[str, Any],
    runtime_context: dict[str, Any],
    repository: dict[str, Any],
    source_sha: str,
    tree_sha: str,
    release_id: str,
    release_manifest_raw: bytes,
    evidence_ref: str,
) -> dict[str, Any]:
    if prepare.get("status") != "PREPARE_READY":
        raise ValueError("operator input requires PREPARE_READY observation evidence")
    if prepare.get("authorization_consumed") is not False:
        raise ValueError("operator input requires authorization_consumed=false")
    if prepare.get("mutation_executed") is not False or prepare.get("provider_mutation_executed") is not False:
        raise ValueError("operator input requires a mutation-free prepare observation")

    plan = prepare.get("plan")
    if not isinstance(plan, dict):
        raise ValueError("PREPARE_READY input is missing plan object")
    component = plan.get("component")
    target_revision = plan.get("target_revision")
    planned_names = plan.get("planned_migrations")
    transaction_policy = plan.get("transaction_policy")
    if not isinstance(component, str) or not component:
        raise ValueError("PREPARE_READY plan.component must be one non-empty string")
    if not isinstance(target_revision, str) or not target_revision:
        raise ValueError("PREPARE_READY plan.target_revision must be one non-empty string")
    if not isinstance(planned_names, list) or not planned_names or not all(isinstance(item, str) and item for item in planned_names):
        raise ValueError("PREPARE_READY plan.planned_migrations must be a non-empty string array")
    if len(set(planned_names)) != len(planned_names):
        raise ValueError("PREPARE_READY plan.planned_migrations must not contain duplicates")
    if not isinstance(transaction_policy, dict):
        raise ValueError("PREPARE_READY plan.transaction_policy must be one typed object")
    transaction_kind = transaction_policy.get("transaction_kind")
    phase = transaction_policy.get("phase")
    freshness_seconds = transaction_policy.get("observation_freshness_max_age_seconds")
    recovery_strategy = transaction_policy.get("recovery_strategy")
    if not isinstance(transaction_kind, str) or not transaction_kind:
        raise ValueError("typed transaction policy transaction_kind must be one non-empty string")
    if not isinstance(phase, str) or not phase:
        raise ValueError("typed transaction policy phase must be one non-empty string")
    if isinstance(freshness_seconds, bool) or not isinstance(freshness_seconds, int) or freshness_seconds <= 0:
        raise ValueError("typed transaction policy observation freshness must be one positive integer")
    if not isinstance(recovery_strategy, str) or not recovery_strategy:
        raise ValueError("typed transaction policy recovery_strategy must be one non-empty string")

    observation = observation_document.get("provider_observation_input")
    if not isinstance(observation, dict):
        raise ValueError("provider observation artifact is missing provider_observation_input")
    target = observation.get("target")
    remote_names = observation.get("remote_migrations")
    predecessor_digest = observation.get("remote_ledger_sha256")
    if not isinstance(target, dict):
        raise ValueError("provider observation target must be one object")
    if not isinstance(remote_names, list) or not all(isinstance(item, str) for item in remote_names):
        raise ValueError("provider observation remote_migrations must be a string array")
    if not isinstance(predecessor_digest, str) or not predecessor_digest:
        raise ValueError("provider observation remote_ledger_sha256 must be one non-empty string")

    components = repository.get("components")
    if not isinstance(components, list):
        raise ValueError("typed D1 repository projection is missing components")
    matching = [item for item in components if isinstance(item, dict) and item.get("component_id") == component]
    if len(matching) != 1:
        raise ValueError(f"typed D1 repository projection must contain exactly one {component} component")
    migration_sources = matching[0].get("executable_migration_sources")
    if not isinstance(migration_sources, list):
        raise ValueError(f"typed D1 repository projection for {component} is missing executable_migration_sources")
    source_by_name: dict[str, str] = {}
    for item in migration_sources:
        if not isinstance(item, dict):
            raise ValueError("executable_migration_sources entries must be objects")
        name = item.get("migration_file")
        source_root = item.get("source_root")
        if not isinstance(name, str) or not name or not isinstance(source_root, str) or not source_root:
            raise ValueError("executable migration source requires migration_file + source_root")
        if name in source_by_name:
            raise ValueError(f"duplicate executable migration source for {name}")
        source_by_name[name] = source_root

    root = root.resolve()
    planned: list[dict[str, str]] = []
    for name in planned_names:
        source_root = source_by_name.get(name)
        if source_root is None:
            raise ValueError(f"typed repository projection has no executable source for planned migration {name}")
        source_directory = (root / source_root).resolve()
        if not source_directory.is_relative_to(root):
            raise ValueError(f"planned migration source root escapes repository: {source_root}")
        path = source_directory / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"planned migration source missing or unsafe: {source_root}/{name}")
        resolved = path.resolve()
        if not resolved.is_relative_to(source_directory):
            raise ValueError(f"planned migration source escapes typed source root: {source_root}/{name}")
        planned.append({
            "migration_file": name,
            "content_sha256": hashlib.sha256(resolved.read_bytes()).hexdigest(),
        })

    try:
        release_manifest_text = release_manifest_raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ValueError("target release manifest must be UTF-8") from exc
    target_manifest = _load_object_text(release_manifest_text, "target release manifest")
    materialized_target = json.dumps(target_manifest, sort_keys=True, indent=2) + "\n"
    if materialized_target.encode("utf-8") != release_manifest_raw:
        raise ValueError("target release manifest bytes are not canonical Prepare materialization bytes")

    runtime = runtime_context.get("schema_contract")
    if not isinstance(runtime, dict):
        raise ValueError("runtime context is missing schema_contract")
    current_manifest = {"schema_contract": runtime}
    known_good_manifest = {"schema_contract": runtime}
    ledger = [{"results": [{"id": index + 1, "name": name} for index, name in enumerate(remote_names)], "success": True}]
    preconditions = {"component": component, "completed": []}
    transaction_input = {
        "schema_version": 1,
        "transaction_kind": transaction_kind,
        "phase": phase,
        "source_sha": source_sha,
        "tree_sha": tree_sha,
        "release_candidate_id": release_id,
        "release_manifest_digests": {component: hashlib.sha256(release_manifest_raw).hexdigest()},
        "target": target,
        "predecessor_ledger_sha256": predecessor_digest,
        "planned_migrations": planned,
        "freshness_max_age_seconds": freshness_seconds,
        "precondition_evidence_refs": [evidence_ref],
        "recovery_strategy": recovery_strategy,
        "expected_post_state": {"revision": target_revision},
    }
    inputs = {
        "source_sha": source_sha,
        "component": component,
        "ledger_json": _compact(ledger),
        "target_release_manifest_json": _compact(target_manifest),
        "current_release_manifest_json": _compact(current_manifest),
        "known_good_release_manifest_json": _compact(known_good_manifest),
        "preconditions_json": _compact(preconditions),
        "provider_observation_json": _compact(observation),
        "repository_projection_json": _compact(repository),
        "transaction_input_json": _compact(transaction_input),
    }
    return {
        "schema_version": 1,
        "command": "d1 operator-input",
        "mode": "read-only",
        "mutation_executed": False,
        "provider_mutation_executed": False,
        "authorization_consumed": False,
        "component": component,
        "inputs": inputs,
    }


def command_operator_input(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    opsctl = Path(args.opsctl).resolve()
    preflight_dir = Path(args.preflight_dir).resolve()
    prepare = _load_object_file(preflight_dir / "prepare.json", "preflight prepare")
    observation = _load_object_file(preflight_dir / "provider-observation.json", "provider observation")
    runtime_context = _load_object_file(preflight_dir / "runtime-context.json", "runtime context")
    repository_process = _run_opsctl(opsctl, root, "repository", [])
    if repository_process.returncode != 0:
        raise ValueError(f"opsctl d1 repository failed: {repository_process.stderr.strip()[:160]}")
    repository = _load_object_text(repository_process.stdout, "opsctl d1 repository stdout")
    release_manifest_raw = Path(args.release_manifest).read_bytes()
    result = _build_operator_inputs(
        root=root,
        prepare=prepare,
        observation_document=observation,
        runtime_context=runtime_context,
        repository=repository,
        source_sha=args.source_sha,
        tree_sha=args.tree_sha,
        release_id=args.release_id,
        release_manifest_raw=release_manifest_raw,
        evidence_ref=args.evidence_ref,
    )
    Path(args.output).write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return 0


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
        root = Path(directory)
        migration_dir = root / "migrations" / "d1"
        migration_dir.mkdir(parents=True)
        migration = migration_dir / "0042_dynamic_operator_fixture.sql"
        migration.write_text("CREATE TABLE tx7_fixture(id INTEGER PRIMARY KEY);\n", encoding="utf-8")
        release_manifest = {
            "schema_contract": {
                "database_component": "catalog",
                "target_schema_revision": "0042_dynamic_operator_fixture.sql",
                "supported_schema_min": "0041.sql",
                "supported_schema_max": "0042_dynamic_operator_fixture.sql",
            }
        }
        release_raw = (json.dumps(release_manifest, sort_keys=True, indent=2) + "\n").encode("utf-8")
        prepare = {
            "status": "PREPARE_READY",
            "authorization_consumed": False,
            "mutation_executed": False,
            "provider_mutation_executed": False,
            "plan": {
                "component": "catalog",
                "target_revision": "0042_dynamic_operator_fixture.sql",
                "planned_migrations": ["0042_dynamic_operator_fixture.sql"],
                "transaction_policy": {
                    "schema_version": 17,
                    "transaction_kind": "FIXTURE_TRANSACTION_KIND",
                    "phase": "FIXTURE_PHASE",
                    "observation_freshness_max_age_seconds": 321,
                    "recovery_strategy": "FIXTURE_RECOVERY",
                },
            },
        }
        observation = {
            "provider_observation_input": {
                "schema_version": 1,
                "target": {
                    "environment": "rehearsal",
                    "account_id": "account-1",
                    "database_name": "d1-rehearsal",
                    "database_id": "database-1",
                },
                "observed_at_unix_seconds": 1_788_640_000,
                "observation_source": "fixture",
                "remote_ledger_sha256": "22" * 32,
                "remote_migrations": ["0041.sql"],
                "wrangler_pending_migrations": ["0042_dynamic_operator_fixture.sql"],
                "deployment_identity": "deployment-1",
                "time_travel_bookmark_capable": True,
            }
        }
        runtime_context = {"schema_contract": {"target_schema_revision": "0041.sql", "supported_schema_max": "0042_dynamic_operator_fixture.sql"}}
        repository = {
            "schema_version": 1,
            "components": [{
                "component_id": "catalog",
                "executable_migration_sources": [{
                    "migration_file": "0042_dynamic_operator_fixture.sql",
                    "source_root": "migrations/d1",
                }],
            }],
        }
        projection = _build_operator_inputs(
            root=root,
            prepare=prepare,
            observation_document=observation,
            runtime_context=runtime_context,
            repository=repository,
            source_sha="33" * 20,
            tree_sha="44" * 20,
            release_id=f"release-set-v3-sha256-{'55' * 32}",
            release_manifest_raw=release_raw,
            evidence_ref="fixture:observation",
        )
        assert projection["command"] == "d1 operator-input"
        assert projection["mutation_executed"] is False
        assert projection["authorization_consumed"] is False
        tx_input = json.loads(projection["inputs"]["transaction_input_json"])
        assert tx_input["planned_migrations"][0]["migration_file"] == "0042_dynamic_operator_fixture.sql"
        assert tx_input["expected_post_state"] == {"revision": "0042_dynamic_operator_fixture.sql"}
        assert tx_input["planned_migrations"][0]["content_sha256"] == hashlib.sha256(migration.read_bytes()).hexdigest()
        assert tx_input["transaction_kind"] == "FIXTURE_TRANSACTION_KIND"
        assert tx_input["phase"] == "FIXTURE_PHASE"
        assert tx_input["freshness_max_age_seconds"] == 321
        assert tx_input["recovery_strategy"] == "FIXTURE_RECOVERY"
        assert "0031_device_binding_governance.sql" not in projection["inputs"]["transaction_input_json"]

        broken_prepare = json.loads(json.dumps(prepare))
        broken_prepare["plan"]["planned_migrations"] = ["missing.sql"]
        try:
            _build_operator_inputs(
                root=root,
                prepare=broken_prepare,
                observation_document=observation,
                runtime_context=runtime_context,
                repository=repository,
                source_sha="33" * 20,
                tree_sha="44" * 20,
                release_id=f"release-set-v3-sha256-{'55' * 32}",
                release_manifest_raw=release_raw,
                evidence_ref="fixture:observation",
            )
        except ValueError as exc:
            assert "no executable source" in str(exc)
        else:
            raise AssertionError("operator input must fail closed when typed migration source is absent")

        missing_policy = json.loads(json.dumps(prepare))
        del missing_policy["plan"]["transaction_policy"]
        try:
            _build_operator_inputs(
                root=root,
                prepare=missing_policy,
                observation_document=observation,
                runtime_context=runtime_context,
                repository=repository,
                source_sha="33" * 20,
                tree_sha="44" * 20,
                release_id=f"release-set-v3-sha256-{'55' * 32}",
                release_manifest_raw=release_raw,
                evidence_ref="fixture:observation",
            )
        except ValueError as exc:
            assert "transaction_policy" in str(exc)
        else:
            raise AssertionError("operator input must fail closed when typed transaction policy is absent")

        output = root / "prepare.json"
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

    operator_input = sub.add_parser(
        "operator-input",
        help="render ephemeral canonical Prepare workflow inputs from accepted local observations",
    )
    operator_input.add_argument("--opsctl", required=True)
    operator_input.add_argument("--root", required=True)
    operator_input.add_argument("--preflight-dir", required=True)
    operator_input.add_argument("--source-sha", required=True)
    operator_input.add_argument("--tree-sha", required=True)
    operator_input.add_argument("--release-id", required=True)
    operator_input.add_argument("--release-manifest", required=True)
    operator_input.add_argument("--evidence-ref", required=True)
    operator_input.add_argument("--output", required=True)
    operator_input.set_defaults(func=command_operator_input)

    self_test = sub.add_parser("self-test", help="run dependency-free prepare fixtures")
    self_test.set_defaults(func=command_self_test)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        return int(args.func(args))
    except ValueError as exc:
        print(f"d1-prepare input error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
