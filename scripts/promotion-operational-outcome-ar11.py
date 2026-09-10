#!/usr/bin/env python3
"""Project AR11 natural-owner verdicts into one lossless terminal operator outcome.

This module is deliberately transport-only. It validates the already-owned D1, Release and
Promotion JSON contracts, preserves the first authoritative blocker without translating its
reason taxonomy, and terminalizes infrastructure loss without inventing semantic state.
It has no provider client, credentials, mutation authority, or admission policy of its own.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

SOURCE_SHA = re.compile(r"[0-9a-f]{40}")
RELEASE_SET_ID = re.compile(r"release-set-v3-sha256-[0-9a-f]{64}")
CONTRACT = "PROMOTION_OPERATOR_OUTCOME_V1"
PROCEDURE = "AR11_RELEASE_SET_PROMOTION_READ_ONLY"


@dataclass(frozen=True)
class LoadedInput:
    label: str
    path: Path
    state: str
    value: dict[str, Any] | None
    digest: str | None
    error: str | None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_input(path: Path, label: str) -> LoadedInput:
    if path.is_symlink() or not path.is_file():
        return LoadedInput(label, path, "MISSING", None, None, f"{label} is unavailable")
    try:
        digest = sha256_file(path)
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        return LoadedInput(label, path, "MALFORMED", None, None, f"{label}: {error}")
    if not isinstance(value, dict):
        return LoadedInput(label, path, "MALFORMED", None, digest, f"{label} must be a JSON object")
    return LoadedInput(label, path, "PRESENT", value, digest, None)


def string_list(value: Any) -> list[str] | None:
    if not isinstance(value, list) or any(not isinstance(item, str) or not item for item in value):
        return None
    return list(value)


def valid_d1(value: dict[str, Any]) -> bool:
    reasons = string_list(value.get("reason_codes"))
    return (
        value.get("schema_version") == 1
        and value.get("command") == "d1 compatibility"
        and value.get("component") == "catalog"
        and value.get("mode") == "read-only"
        and value.get("mutation_executed") is False
        and isinstance(value.get("allowed"), bool)
        and reasons is not None
        and (value["allowed"] or bool(reasons))
    )


def valid_release(value: dict[str, Any]) -> bool:
    blockers = string_list(value.get("blockers"))
    steps = string_list(value.get("required_steps"))
    return (
        value.get("schema_version") == 2
        and value.get("command") == "release.compatibility"
        and value.get("mutation_executed") is False
        and isinstance(value.get("compatible"), bool)
        and blockers is not None
        and steps is not None
        and (value["compatible"] or bool(blockers))
    )


def valid_plan(value: dict[str, Any]) -> bool:
    blockers = string_list(value.get("blockers"))
    return (
        value.get("schema_version") == 1
        and value.get("command") == "promotion.plan"
        and value.get("mutation_executed") is False
        and value.get("execution_authorized") is False
        and value.get("decision") in {"PLAN", "NO_CHANGE", "BLOCKED"}
        and blockers is not None
        and (value["decision"] != "BLOCKED" or bool(blockers))
    )


def valid_preflight(value: dict[str, Any]) -> bool:
    blockers = string_list(value.get("blockers"))
    steps = string_list(value.get("required_steps"))
    ready = value.get("ready")
    decision = value.get("decision")
    return (
        value.get("schema_version") == 1
        and value.get("command") == "promotion.preflight"
        and isinstance(ready, bool)
        and decision in {"READY", "BLOCKED"}
        and ready == (decision == "READY")
        and value.get("credential_values_accessed") is False
        and value.get("provider_mutation_executed") is False
        and value.get("mutation_executed") is False
        and blockers is not None
        and steps is not None
        and isinstance(value.get("rollback_compatibility"), str)
        and (ready or bool(blockers))
    )


def valid_ready(value: dict[str, Any], source_sha: str, release_set_id: str) -> bool:
    return (
        value.get("schema_version") == 1
        and value.get("kind") == "AR11_READY_TO_MUTATE"
        and value.get("ready") is True
        and value.get("source_sha") == source_sha
        and value.get("release_set_id") == release_set_id
        and value.get("provider_mutation") is False
        and value.get("production_mutation") is False
        and value.get("credential_values_accessed") is False
        and isinstance(value.get("promotion_id"), str)
        and bool(value["promotion_id"])
    )


def first(values: list[str]) -> str | None:
    return values[0] if values else None


def owner_blocked(
    base: dict[str, Any],
    *,
    phase: str,
    owner: str,
    owner_contract: str,
    reason: str,
    diagnostic: dict[str, Any],
    remediation: str | None,
) -> dict[str, Any]:
    remediation_text = remediation or (
        f"Resolve only the condition reported by {owner} in owner_diagnostic, then rerun read-only AR11."
    )
    return {
        **base,
        "status": "BLOCKED",
        "phase": phase,
        "failed_gate": phase,
        "owner": owner,
        "owner_contract": owner_contract,
        "owner_reason_code": reason,
        "owner_diagnostic": diagnostic,
        "summary": f"{owner} blocked AR11 at {phase} with owner reason {reason}.",
        "remediation": remediation_text,
        "exact_next_action": remediation_text,
    }


def infrastructure_failure(
    base: dict[str, Any], phase: str, detail: dict[str, Any]
) -> dict[str, Any]:
    return {
        **base,
        "status": "INFRASTRUCTURE_FAILURE",
        "phase": phase,
        "failed_gate": phase,
        "owner": None,
        "owner_contract": None,
        "owner_reason_code": None,
        "owner_diagnostic": {"verdict_available": False, **detail},
        "summary": f"AR11 could not obtain a valid natural-owner verdict at {phase}.",
        "remediation": (
            "Repair the named orchestration/evidence boundary without inventing semantic owner state, "
            "then rerun the read-only procedure."
        ),
        "exact_next_action": (
            "Rerun automatic AR11 from exact accepted main after the failed boundary is repaired; "
            "do not authorize or execute provider mutation."
        ),
    }


def compose(
    *,
    source_sha: str,
    tree_sha: str,
    release_set_id: str,
    environment: str,
    profile_id: str,
    d1: dict[str, Any] | None,
    release: dict[str, Any] | None,
    plan: dict[str, Any] | None,
    preflight: dict[str, Any] | None,
    ready: dict[str, Any] | None,
    states: dict[str, dict[str, Any]],
    evidence_refs: dict[str, str],
    evidence_digests: dict[str, str],
    failed_boundary: str | None = None,
) -> dict[str, Any]:
    base: dict[str, Any] = {
        "schema_version": 1,
        "contract": CONTRACT,
        "procedure": PROCEDURE,
        "source_sha": source_sha,
        "tree_sha": tree_sha,
        "release_set_id": release_set_id,
        "promotion_id": None,
        "target_identity": {
            "environment": environment,
            "capability_profile_id": profile_id,
        },
        "authorization_state": "NOT_AUTHORIZED_READ_ONLY",
        "provider_mutation_started": False,
        "provider_mutation_executed": False,
        "production_mutation_executed": False,
        "effect_state": "EXACT_NO_EFFECT",
        "evidence_refs": evidence_refs,
        "evidence_digests": evidence_digests,
    }

    if failed_boundary:
        return infrastructure_failure(
            base, failed_boundary, {"failed_boundary": failed_boundary, "source": "workflow-boundary"}
        )

    if d1 is None or not valid_d1(d1):
        return infrastructure_failure(base, "D1_COMPATIBILITY", states["d1"])
    if not d1["allowed"]:
        reason = first(d1["reason_codes"])
        assert reason is not None
        return owner_blocked(
            base,
            phase="D1_COMPATIBILITY",
            owner="opsctl.d1.compatibility",
            owner_contract="d1 compatibility/v1",
            reason=reason,
            diagnostic=d1,
            remediation=None,
        )

    if release is None or not valid_release(release):
        return infrastructure_failure(base, "RELEASE_COMPATIBILITY", states["release"])
    if not release["compatible"]:
        reason = first(release["blockers"])
        assert reason is not None
        return owner_blocked(
            base,
            phase="RELEASE_COMPATIBILITY",
            owner="opsctl.release.compatibility",
            owner_contract="release.compatibility/v2",
            reason=reason,
            diagnostic=release,
            remediation=first(release["required_steps"]),
        )

    if plan is None or not valid_plan(plan):
        return infrastructure_failure(base, "PROMOTION_PLAN", states["plan"])
    if isinstance(plan.get("promotion_id"), str):
        base["promotion_id"] = plan["promotion_id"]
    if plan["decision"] == "BLOCKED":
        reason = first(plan["blockers"])
        assert reason is not None
        return owner_blocked(
            base,
            phase="PROMOTION_PLAN",
            owner="opsctl.promotion.plan",
            owner_contract="promotion.plan/v1",
            reason=reason,
            diagnostic=plan,
            remediation=None,
        )
    if plan["decision"] == "NO_CHANGE":
        return {
            **base,
            "status": "NOOP",
            "phase": "PROMOTION_PLAN",
            "failed_gate": None,
            "owner": "opsctl.promotion.plan",
            "owner_contract": "promotion.plan/v1",
            "owner_reason_code": "NO_CHANGE",
            "owner_diagnostic": plan,
            "summary": "Promotion owner reports NO_CHANGE; the target is already converged.",
            "remediation": "No provider remediation or mutation is required.",
            "exact_next_action": (
                "Record this no-op terminal outcome and return to the current stage owner; "
                "do not manufacture a provider write for evidence."
            ),
        }

    if preflight is None or not valid_preflight(preflight):
        return infrastructure_failure(base, "PROMOTION_PREFLIGHT", states["preflight"])
    if isinstance(preflight.get("promotion_id"), str):
        base["promotion_id"] = preflight["promotion_id"]
    if not preflight["ready"]:
        reason = first(preflight["blockers"])
        assert reason is not None
        return owner_blocked(
            base,
            phase="PROMOTION_PREFLIGHT",
            owner="opsctl.promotion.preflight",
            owner_contract="promotion.preflight/v1",
            reason=reason,
            diagnostic=preflight,
            remediation=first(preflight["required_steps"]),
        )

    if ready is None or not valid_ready(ready, source_sha, release_set_id):
        return infrastructure_failure(base, "READ_ONLY_READY_EVIDENCE", states["ready"])
    base["promotion_id"] = ready["promotion_id"]
    return {
        **base,
        "status": "READ_ONLY_READY",
        "phase": "AUTHORIZATION_BOUNDARY",
        "failed_gate": None,
        "owner": "opsctl.promotion.preflight",
        "owner_contract": "promotion.preflight/v1",
        "owner_reason_code": "READY",
        "owner_diagnostic": preflight,
        "summary": (
            "All read-only AR11 owners are READY and immutable READY_TO_MUTATE evidence exists; "
            "provider mutation remains unauthorized."
        ),
        "remediation": "No read-only remediation is required.",
        "exact_next_action": (
            "STOP at the existing explicit one-shot authorization boundary; do not execute provider "
            "mutation unless the owning stage records a fresh exact authorization."
        ),
    }


def state_for(record: LoadedInput) -> dict[str, Any]:
    return {
        "input": record.label,
        "input_state": record.state,
        "input_error": record.error,
    }


def build_from_files(args: argparse.Namespace) -> dict[str, Any]:
    records = {
        "d1": load_input(args.d1_compatibility_json, "catalog D1 compatibility"),
        "release": load_input(args.release_compatibility_json, "release compatibility"),
        "plan": load_input(args.promotion_plan_json, "promotion plan"),
        "preflight": load_input(args.promotion_preflight_json, "promotion preflight"),
        "ready": load_input(args.ready_to_mutate_json, "READY_TO_MUTATE evidence"),
    }
    evidence_refs: dict[str, str] = {}
    evidence_digests: dict[str, str] = {}
    for key, record in records.items():
        if record.state != "MISSING":
            evidence_refs[key] = f"{args.evidence_artifact}/{record.path.name}"
        if record.digest is not None:
            evidence_digests[key] = record.digest
    return compose(
        source_sha=args.source_sha,
        tree_sha=args.tree_sha,
        release_set_id=args.release_set_id,
        environment=args.environment,
        profile_id=args.profile_id,
        d1=records["d1"].value,
        release=records["release"].value,
        plan=records["plan"].value,
        preflight=records["preflight"].value,
        ready=records["ready"].value,
        states={key: state_for(record) for key, record in records.items()},
        evidence_refs=evidence_refs,
        evidence_digests=evidence_digests,
        failed_boundary=args.failed_boundary or None,
    )


def fixture_inputs() -> dict[str, dict[str, Any]]:
    promotion_id = "b" * 64
    return {
        "d1": {
            "schema_version": 1,
            "command": "d1 compatibility",
            "component": "catalog",
            "mode": "read-only",
            "mutation_executed": False,
            "allowed": True,
            "reason_codes": [],
        },
        "release": {
            "schema_version": 2,
            "command": "release.compatibility",
            "compatible": True,
            "blockers": [],
            "required_steps": [],
            "mutation_executed": False,
        },
        "plan": {
            "schema_version": 1,
            "command": "promotion.plan",
            "decision": "PLAN",
            "promotion_id": promotion_id,
            "blockers": [],
            "execution_authorized": False,
            "mutation_executed": False,
        },
        "preflight": {
            "schema_version": 1,
            "command": "promotion.preflight",
            "decision": "READY",
            "ready": True,
            "promotion_id": promotion_id,
            "rollback_compatibility": "COMPATIBLE",
            "blockers": [],
            "required_steps": [],
            "credential_values_accessed": False,
            "provider_mutation_executed": False,
            "mutation_executed": False,
        },
        "ready": {
            "schema_version": 1,
            "kind": "AR11_READY_TO_MUTATE",
            "ready": True,
            "source_sha": "a" * 40,
            "release_set_id": "release-set-v3-sha256-" + "c" * 64,
            "promotion_id": promotion_id,
            "credential_values_accessed": False,
            "provider_mutation": False,
            "production_mutation": False,
        },
    }


def fixture_compose(values: dict[str, dict[str, Any] | None], failed_boundary: str | None = None) -> dict[str, Any]:
    states = {
        key: {"input": key, "input_state": "PRESENT" if value is not None else "MISSING", "input_error": None}
        for key, value in values.items()
    }
    return compose(
        source_sha="a" * 40,
        tree_sha="d" * 40,
        release_set_id="release-set-v3-sha256-" + "c" * 64,
        environment="staging",
        profile_id="rehearsal-core-v2",
        d1=values["d1"],
        release=values["release"],
        plan=values["plan"],
        preflight=values["preflight"],
        ready=values["ready"],
        states=states,
        evidence_refs={},
        evidence_digests={},
        failed_boundary=failed_boundary,
    )


def assert_zero_effect(outcome: dict[str, Any]) -> None:
    assert outcome["provider_mutation_started"] is False
    assert outcome["provider_mutation_executed"] is False
    assert outcome["production_mutation_executed"] is False
    assert outcome["effect_state"] == "EXACT_NO_EFFECT"


def self_test() -> None:
    base = fixture_inputs()

    outcome = fixture_compose(base)
    assert outcome["status"] == "READ_ONLY_READY"
    assert outcome["phase"] == "AUTHORIZATION_BOUNDARY"
    assert outcome["authorization_state"] == "NOT_AUTHORIZED_READ_ONLY"
    assert_zero_effect(outcome)

    values = {key: dict(value) for key, value in base.items()}
    values["d1"]["allowed"] = False
    values["d1"]["reason_codes"] = ["REMOTE_SCHEMA_OUTSIDE_RELEASE_WINDOW"]
    outcome = fixture_compose(values)
    assert outcome["status"] == "BLOCKED" and outcome["phase"] == "D1_COMPATIBILITY"
    assert outcome["owner_reason_code"] == "REMOTE_SCHEMA_OUTSIDE_RELEASE_WINDOW"
    assert_zero_effect(outcome)

    values = {key: dict(value) for key, value in base.items()}
    values["release"]["compatible"] = False
    values["release"]["blockers"] = ["SCHEMA_INCOMPATIBLE"]
    values["release"]["required_steps"] = ["apply accepted compatibility steps"]
    outcome = fixture_compose(values)
    assert outcome["phase"] == "RELEASE_COMPATIBILITY"
    assert outcome["owner_reason_code"] == "SCHEMA_INCOMPATIBLE"
    assert outcome["exact_next_action"] == "apply accepted compatibility steps"
    assert_zero_effect(outcome)

    values = {key: dict(value) for key, value in base.items()}
    values["plan"]["decision"] = "BLOCKED"
    values["plan"]["blockers"] = ["PROVIDER_STATE_UNKNOWN"]
    outcome = fixture_compose(values)
    assert outcome["status"] == "BLOCKED" and outcome["phase"] == "PROMOTION_PLAN"
    assert outcome["owner_reason_code"] == "PROVIDER_STATE_UNKNOWN"
    assert_zero_effect(outcome)

    values = {key: dict(value) for key, value in base.items()}
    values["plan"]["decision"] = "NO_CHANGE"
    outcome = fixture_compose(values)
    assert outcome["status"] == "NOOP" and outcome["owner_reason_code"] == "NO_CHANGE"
    assert_zero_effect(outcome)

    for rollback, reason in (
        ("INCOMPATIBLE", "ROLLBACK_INCOMPATIBLE"),
        ("UNKNOWN", "ROLLBACK_COMPATIBILITY_UNKNOWN"),
    ):
        values = {key: dict(value) for key, value in base.items()}
        values["preflight"]["ready"] = False
        values["preflight"]["decision"] = "BLOCKED"
        values["preflight"]["rollback_compatibility"] = rollback
        values["preflight"]["blockers"] = [reason]
        values["preflight"]["required_steps"] = ["repair rollback compatibility evidence"]
        outcome = fixture_compose(values)
        assert outcome["phase"] == "PROMOTION_PREFLIGHT"
        assert outcome["owner_reason_code"] == reason
        assert outcome["owner_diagnostic"]["rollback_compatibility"] == rollback
        assert_zero_effect(outcome)

    values = {key: dict(value) for key, value in base.items()}
    values["preflight"]["ready"] = False
    values["preflight"]["decision"] = "BLOCKED"
    values["preflight"]["blockers"] = ["REQUIRED_BINDINGS_NOT_READY"]
    values["preflight"]["required_steps"] = ["prepare required bindings"]
    outcome = fixture_compose(values)
    assert outcome["status"] == "BLOCKED" and outcome["phase"] == "PROMOTION_PREFLIGHT"
    assert outcome["owner_reason_code"] == "REQUIRED_BINDINGS_NOT_READY"
    assert_zero_effect(outcome)

    values = {key: dict(value) for key, value in base.items()}
    values["d1"] = {"schema_version": 1, "command": "d1 compatibility"}
    outcome = fixture_compose(values)
    assert outcome["status"] == "INFRASTRUCTURE_FAILURE"
    assert outcome["phase"] == "D1_COMPATIBILITY"
    assert outcome["owner_reason_code"] is None
    assert outcome["owner_diagnostic"]["verdict_available"] is False
    assert_zero_effect(outcome)

    values = {key: dict(value) for key, value in base.items()}
    values["release"] = None
    outcome = fixture_compose(values)
    assert outcome["status"] == "INFRASTRUCTURE_FAILURE"
    assert outcome["phase"] == "RELEASE_COMPATIBILITY"
    assert outcome["owner_reason_code"] is None
    assert_zero_effect(outcome)

    outcome = fixture_compose(base, failed_boundary="PROVIDER_OBSERVATION")
    assert outcome["status"] == "INFRASTRUCTURE_FAILURE"
    assert outcome["phase"] == "PROVIDER_OBSERVATION"
    assert outcome["owner_reason_code"] is None
    assert_zero_effect(outcome)

    print("AR11 promotion OperationalOutcome fixture matrix passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-sha")
    parser.add_argument("--tree-sha")
    parser.add_argument("--release-set-id")
    parser.add_argument("--environment", default="staging")
    parser.add_argument("--profile-id", default="rehearsal-core-v2")
    parser.add_argument("--d1-compatibility-json", type=Path)
    parser.add_argument("--release-compatibility-json", type=Path)
    parser.add_argument("--promotion-plan-json", type=Path)
    parser.add_argument("--promotion-preflight-json", type=Path)
    parser.add_argument("--ready-to-mutate-json", type=Path)
    parser.add_argument("--failed-boundary", default="")
    parser.add_argument("--evidence-artifact")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0

    required = {
        "source-sha": args.source_sha,
        "tree-sha": args.tree_sha,
        "release-set-id": args.release_set_id,
        "d1-compatibility-json": args.d1_compatibility_json,
        "release-compatibility-json": args.release_compatibility_json,
        "promotion-plan-json": args.promotion_plan_json,
        "promotion-preflight-json": args.promotion_preflight_json,
        "ready-to-mutate-json": args.ready_to_mutate_json,
        "evidence-artifact": args.evidence_artifact,
        "output": args.output,
    }
    missing = [name for name, value in required.items() if value is None or value == ""]
    if missing:
        print(f"AR11 OperationalOutcome error: missing required arguments: {', '.join(missing)}", file=sys.stderr)
        return 2
    if SOURCE_SHA.fullmatch(args.source_sha) is None or SOURCE_SHA.fullmatch(args.tree_sha) is None:
        print("AR11 OperationalOutcome error: source/tree SHA must be exact 40-char lowercase hex", file=sys.stderr)
        return 2
    if RELEASE_SET_ID.fullmatch(args.release_set_id) is None:
        print("AR11 OperationalOutcome error: release-set-id must be an exact v3 Release Set ID", file=sys.stderr)
        return 2
    try:
        outcome = build_from_files(args)
        if args.output.exists():
            raise OSError(f"output already exists: {args.output}")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(outcome, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 0
    except OSError as error:
        print(f"AR11 OperationalOutcome error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
