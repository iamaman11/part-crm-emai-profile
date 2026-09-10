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
READ_ONLY_PROCEDURE = "AR11_RELEASE_SET_PROMOTION_READ_ONLY"
MANUAL_PROCEDURE = "AR11_RELEASE_SET_PROMOTION"


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
    digest: str | None = None
    try:
        digest = sha256_file(path)
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        return LoadedInput(label, path, "MALFORMED", None, digest, f"{label}: {error}")
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
        "procedure": READ_ONLY_PROCEDURE,
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



def valid_manual_state(value: dict[str, Any], phase: str, source_sha: str) -> bool:
    release_set_id = value.get("release_set_id")
    promotion_id = value.get("promotion_id")
    failed_boundary = value.get("failed_boundary")
    step_outcomes = value.get("step_outcomes")
    return (
        value.get("schema_version") == 1
        and value.get("kind") == "AR11_MANUAL_PHASE_STATE"
        and value.get("phase") == phase
        and value.get("source_sha") == source_sha
        and (release_set_id is None or (isinstance(release_set_id, str) and RELEASE_SET_ID.fullmatch(release_set_id)))
        and (promotion_id is None or (isinstance(promotion_id, str) and bool(promotion_id)))
        and isinstance(value.get("authorization_bound"), bool)
        and isinstance(value.get("provider_mutation_started"), bool)
        and isinstance(value.get("provider_mutation_executed"), bool)
        and value.get("production_mutation_executed") is False
        and (failed_boundary is None or (isinstance(failed_boundary, str) and bool(failed_boundary)))
        and isinstance(step_outcomes, dict)
        and all(isinstance(key, str) and isinstance(item, str) for key, item in step_outcomes.items())
    )


def valid_promotion_verify(
    value: dict[str, Any], environment: str, profile_id: str, release_set_id: str | None
) -> bool:
    blockers = string_list(value.get("blockers"))
    decision = value.get("decision")
    verified = value.get("verified")
    return (
        value.get("schema_version") == 1
        and value.get("command") == "promotion.verify"
        and decision in {"VERIFIED", "DRIFTED", "INCOMPLETE", "UNKNOWN"}
        and isinstance(verified, bool)
        and verified == (decision == "VERIFIED")
        and value.get("environment") == environment
        and (release_set_id is None or value.get("target_release_set_id") == release_set_id)
        and value.get("target_capability_profile_id") == profile_id
        and blockers is not None
        and (verified or bool(blockers))
        and value.get("mutation_executed") is False
    )


def manual_base(
    *,
    source_sha: str,
    tree_sha: str,
    release_set_id: str | None,
    promotion_id: str | None,
    environment: str,
    profile_id: str,
    authorization_bound: bool,
    provider_mutation_started: bool,
    provider_mutation_executed: bool,
    evidence_refs: dict[str, str],
    evidence_digests: dict[str, str],
) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "contract": CONTRACT,
        "procedure": MANUAL_PROCEDURE,
        "source_sha": source_sha,
        "tree_sha": tree_sha,
        "release_set_id": release_set_id,
        "promotion_id": promotion_id,
        "target_identity": {"environment": environment, "capability_profile_id": profile_id},
        "authorization_state": "BOUND_ONE_SHOT" if authorization_bound else "NOT_BOUND",
        "provider_mutation_started": provider_mutation_started,
        "provider_mutation_executed": provider_mutation_executed,
        "production_mutation_executed": False,
        "effect_state": "EXACT_NO_EFFECT" if not provider_mutation_started else "UNKNOWN",
        "evidence_refs": evidence_refs,
        "evidence_digests": evidence_digests,
    }


def manual_failed_no_effect(base: dict[str, Any], phase: str, diagnostic: dict[str, Any]) -> dict[str, Any]:
    return {
        **base,
        "status": "FAILED_NO_EFFECT",
        "phase": phase,
        "failed_gate": phase,
        "owner": None,
        "owner_contract": None,
        "owner_reason_code": None,
        "owner_diagnostic": {"verdict_available": False, **diagnostic},
        "summary": f"Manual AR11 stopped at {phase} before the provider mutation invocation boundary.",
        "remediation": "Resolve the exact failed boundary and obtain a fresh exact READY/authorization fence before any new mutation attempt.",
        "exact_next_action": "Return to the current stage owner; do not retry provider mutation until the existing READY, authorization, and exact-current fences are valid again.",
    }


def manual_recovery_required(
    base: dict[str, Any],
    phase: str,
    diagnostic: dict[str, Any],
    *,
    owner: str | None = None,
    owner_contract: str | None = None,
    reason: str | None = None,
    effect_state: str = "UNKNOWN",
) -> dict[str, Any]:
    return {
        **base,
        "status": "RECOVERY_REQUIRED",
        "phase": phase,
        "failed_gate": phase,
        "owner": owner,
        "owner_contract": owner_contract,
        "owner_reason_code": reason,
        "owner_diagnostic": diagnostic,
        "recovery_owner": "AR-14",
        "effect_state": effect_state,
        "summary": f"Manual AR11 crossed the provider mutation boundary but did not prove terminal convergence at {phase}.",
        "remediation": "Use the existing AR-14 recovery inspect/plan/verify authority; preserve the captured provider/promotion evidence and do not perform automatic restore or blind deploy retry.",
        "exact_next_action": "Follow AR-14 read-only recovery inspection against the exact target/evidence before any separately authorized recovery or retry action.",
    }


def manual_infrastructure_after_verified(
    base: dict[str, Any], phase: str, verify: dict[str, Any], diagnostic: dict[str, Any]
) -> dict[str, Any]:
    return {
        **base,
        "status": "INFRASTRUCTURE_FAILURE",
        "phase": phase,
        "failed_gate": phase,
        "owner": None,
        "owner_contract": None,
        "owner_reason_code": None,
        "owner_diagnostic": {"verdict_available": False, "last_promotion_verify": verify, **diagnostic},
        "effect_state": "EFFECT_VERIFIED",
        "summary": f"Promotion owner verified the provider effect, but orchestration/evidence failed later at {phase}.",
        "remediation": "Repair only the named post-verification/evidence boundary; do not manufacture a second provider mutation for proof.",
        "exact_next_action": "Rerun the required read-only post-verification/evidence check from the exact accepted target without redeploying.",
    }


def compose_manual_values(
    *,
    source_sha: str,
    tree_sha: str,
    environment: str,
    profile_id: str,
    resolve: dict[str, Any] | None,
    mutation: dict[str, Any] | None,
    post: dict[str, Any] | None,
    verify: dict[str, Any] | None,
    states: dict[str, dict[str, Any]],
    evidence_refs: dict[str, str],
    evidence_digests: dict[str, str],
) -> dict[str, Any]:
    for key, phase in (("resolve", "RESOLVE_VERIFY"), ("mutation", "MUTATE"), ("post", "POST_VERIFY")):
        value = {"resolve": resolve, "mutation": mutation, "post": post}[key]
        if value is None or not valid_manual_state(value, phase, source_sha):
            started = bool(mutation and mutation.get("provider_mutation_started") is True)
            executed = bool(mutation and mutation.get("provider_mutation_executed") is True)
            base = manual_base(
                source_sha=source_sha,
                tree_sha=tree_sha,
                release_set_id=None,
                promotion_id=None,
                environment=environment,
                profile_id=profile_id,
                authorization_bound=False,
                provider_mutation_started=started,
                provider_mutation_executed=executed,
                evidence_refs=evidence_refs,
                evidence_digests=evidence_digests,
            )
            if started:
                return manual_recovery_required(base, f"{phase}_STATE", states[key])
            return {
                **manual_failed_no_effect(base, f"{phase}_STATE", states[key]),
                "status": "INFRASTRUCTURE_FAILURE",
            }

    assert resolve is not None and mutation is not None and post is not None
    identities = [value.get("release_set_id") for value in (resolve, mutation, post) if value.get("release_set_id")]
    if len(set(identities)) > 1:
        started = mutation["provider_mutation_started"]
        base = manual_base(
            source_sha=source_sha,
            tree_sha=tree_sha,
            release_set_id=None,
            promotion_id=None,
            environment=environment,
            profile_id=profile_id,
            authorization_bound=resolve["authorization_bound"],
            provider_mutation_started=started,
            provider_mutation_executed=mutation["provider_mutation_executed"],
            evidence_refs=evidence_refs,
            evidence_digests=evidence_digests,
        )
        detail = {"release_set_identities": identities, "verdict_available": False}
        if started:
            return manual_recovery_required(base, "TARGET_IDENTITY_TRANSPORT", detail)
        return manual_failed_no_effect(base, "TARGET_IDENTITY_TRANSPORT", detail)

    release_set_id = identities[0] if identities else None
    promotion_ids = [value.get("promotion_id") for value in (resolve, mutation, post) if value.get("promotion_id")]
    promotion_id = promotion_ids[0] if promotion_ids and len(set(promotion_ids)) == 1 else None
    base = manual_base(
        source_sha=source_sha,
        tree_sha=tree_sha,
        release_set_id=release_set_id,
        promotion_id=promotion_id,
        environment=environment,
        profile_id=profile_id,
        authorization_bound=resolve["authorization_bound"],
        provider_mutation_started=mutation["provider_mutation_started"],
        provider_mutation_executed=mutation["provider_mutation_executed"],
        evidence_refs=evidence_refs,
        evidence_digests=evidence_digests,
    )

    if resolve["failed_boundary"]:
        return manual_failed_no_effect(base, resolve["failed_boundary"], resolve)
    if mutation["failed_boundary"]:
        if not mutation["provider_mutation_started"]:
            return manual_failed_no_effect(base, mutation["failed_boundary"], mutation)
        return manual_recovery_required(base, mutation["failed_boundary"], mutation)
    if not mutation["provider_mutation_started"]:
        return manual_failed_no_effect(base, "MUTATION_INVOCATION_BOUNDARY", mutation)
    if not mutation["provider_mutation_executed"]:
        return manual_recovery_required(base, "PROVIDER_DEPLOY", mutation)
    if not resolve["authorization_bound"]:
        return manual_recovery_required(base, "AUTHORIZATION_INVARIANT", resolve)

    verify_valid = verify is not None and valid_promotion_verify(verify, environment, profile_id, release_set_id)
    if post["failed_boundary"] and verify_valid and verify is not None and verify["decision"] == "VERIFIED":
        return manual_infrastructure_after_verified(base, post["failed_boundary"], verify, post)
    if post["failed_boundary"]:
        return manual_recovery_required(base, post["failed_boundary"], post)
    if not verify_valid or verify is None:
        return manual_recovery_required(base, "PROMOTION_VERIFY", states["verify"])
    if verify["decision"] != "VERIFIED":
        blockers = verify["blockers"]
        reason = first(blockers)
        assert reason is not None
        return manual_recovery_required(
            base,
            "PROMOTION_VERIFY",
            verify,
            owner="opsctl.promotion.verify",
            owner_contract="promotion.verify/v1",
            reason=reason,
            effect_state="RECOVERY_REQUIRED",
        )
    return {
        **base,
        "status": "COMPLETED",
        "phase": "POST_VERIFY",
        "failed_gate": None,
        "owner": "opsctl.promotion.verify",
        "owner_contract": "promotion.verify/v1",
        "owner_reason_code": "VERIFIED",
        "owner_diagnostic": verify,
        "effect_state": "EFFECT_VERIFIED",
        "summary": "Manual AR11 provider effect is verified by the existing promotion.verify natural owner.",
        "remediation": "No recovery action is required.",
        "exact_next_action": "Record the completed promotion receipt/evidence and return to the current stage owner.",
    }


def build_manual_from_files(args: argparse.Namespace) -> dict[str, Any]:
    records = {
        "resolve": load_input(args.resolve_state_json, "manual resolve phase state"),
        "mutation": load_input(args.mutation_state_json, "manual mutation phase state"),
        "post": load_input(args.post_verify_state_json, "manual post-verify phase state"),
        "verify": load_input(args.promotion_verify_json, "promotion verify owner verdict"),
    }
    evidence_refs: dict[str, str] = {}
    evidence_digests: dict[str, str] = {}
    for key, record in records.items():
        if record.state != "MISSING":
            evidence_refs[key] = f"{args.evidence_artifact}/{record.path.name}"
        if record.digest is not None:
            evidence_digests[key] = record.digest
    return compose_manual_values(
        source_sha=args.source_sha,
        tree_sha=args.tree_sha,
        environment=args.environment,
        profile_id=args.profile_id,
        resolve=records["resolve"].value,
        mutation=records["mutation"].value,
        post=records["post"].value,
        verify=records["verify"].value,
        states={key: state_for(record) for key, record in records.items()},
        evidence_refs=evidence_refs,
        evidence_digests=evidence_digests,
    )

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



    manual_source = "a" * 40
    manual_release = "release-set-v3-sha256-" + "c" * 64
    manual_promotion = "b" * 64
    resolve_state = {
        "schema_version": 1,
        "kind": "AR11_MANUAL_PHASE_STATE",
        "phase": "RESOLVE_VERIFY",
        "source_sha": manual_source,
        "release_set_id": manual_release,
        "promotion_id": manual_promotion,
        "authorization_bound": True,
        "provider_mutation_started": False,
        "provider_mutation_executed": False,
        "production_mutation_executed": False,
        "failed_boundary": None,
        "step_outcomes": {"intent": "success", "ready": "success", "target_verify": "success"},
    }
    mutation_state = {
        "schema_version": 1,
        "kind": "AR11_MANUAL_PHASE_STATE",
        "phase": "MUTATE",
        "source_sha": manual_source,
        "release_set_id": manual_release,
        "promotion_id": manual_promotion,
        "authorization_bound": True,
        "provider_mutation_started": True,
        "provider_mutation_executed": True,
        "production_mutation_executed": False,
        "failed_boundary": None,
        "step_outcomes": {"exact_fence": "success", "mutation_start": "success", "deploy": "success"},
    }
    post_state = {
        "schema_version": 1,
        "kind": "AR11_MANUAL_PHASE_STATE",
        "phase": "POST_VERIFY",
        "source_sha": manual_source,
        "release_set_id": manual_release,
        "promotion_id": manual_promotion,
        "authorization_bound": True,
        "provider_mutation_started": True,
        "provider_mutation_executed": True,
        "production_mutation_executed": False,
        "failed_boundary": None,
        "step_outcomes": {"observe_verify": "success", "health": "success", "post_evidence": "success"},
    }
    verified = {
        "schema_version": 1,
        "command": "promotion.verify",
        "decision": "VERIFIED",
        "verified": True,
        "environment": "staging",
        "target_release_set_id": manual_release,
        "target_capability_profile_id": "rehearsal-core-v2",
        "blockers": [],
        "mutation_executed": False,
    }
    manual_states = {key: {"input": key, "input_state": "PRESENT", "input_error": None} for key in ("resolve", "mutation", "post", "verify")}

    def manual_case(resolve, mutation, post, verify):
        return compose_manual_values(
            source_sha=manual_source,
            tree_sha="d" * 40,
            environment="staging",
            profile_id="rehearsal-core-v2",
            resolve=resolve,
            mutation=mutation,
            post=post,
            verify=verify,
            states=manual_states,
            evidence_refs={},
            evidence_digests={},
        )

    pre_effect = dict(resolve_state)
    pre_effect["failed_boundary"] = "AUTHORIZATION_BINDING"
    pre_effect["authorization_bound"] = False
    not_started = dict(mutation_state)
    not_started["provider_mutation_started"] = False
    not_started["provider_mutation_executed"] = False
    not_started["failed_boundary"] = "MUTATE_SKIPPED"
    outcome = manual_case(pre_effect, not_started, post_state, None)
    assert outcome["status"] == "FAILED_NO_EFFECT"
    assert outcome["phase"] == "AUTHORIZATION_BINDING"
    assert_zero_effect(outcome)

    fence_fail = dict(mutation_state)
    fence_fail["provider_mutation_started"] = False
    fence_fail["provider_mutation_executed"] = False
    fence_fail["failed_boundary"] = "EXACT_CURRENT_FENCE"
    outcome = manual_case(resolve_state, fence_fail, post_state, None)
    assert outcome["status"] == "FAILED_NO_EFFECT" and outcome["effect_state"] == "EXACT_NO_EFFECT"

    deploy_fail = dict(mutation_state)
    deploy_fail["provider_mutation_executed"] = False
    deploy_fail["failed_boundary"] = "PROVIDER_DEPLOY"
    outcome = manual_case(resolve_state, deploy_fail, post_state, None)
    assert outcome["status"] == "RECOVERY_REQUIRED" and outcome["effect_state"] == "UNKNOWN"
    assert outcome["recovery_owner"] == "AR-14"

    drifted = dict(verified)
    drifted["decision"] = "DRIFTED"
    drifted["verified"] = False
    drifted["blockers"] = ["DEPLOYED_RELEASE_SET_MISMATCH"]
    outcome = manual_case(resolve_state, mutation_state, post_state, drifted)
    assert outcome["status"] == "RECOVERY_REQUIRED"
    assert outcome["effect_state"] == "RECOVERY_REQUIRED"
    assert outcome["owner"] == "opsctl.promotion.verify"
    assert outcome["owner_reason_code"] == "DEPLOYED_RELEASE_SET_MISMATCH"
    assert outcome["owner_diagnostic"]["blockers"] == ["DEPLOYED_RELEASE_SET_MISMATCH"]

    outcome = manual_case(resolve_state, mutation_state, post_state, verified)
    assert outcome["status"] == "COMPLETED"
    assert outcome["effect_state"] == "EFFECT_VERIFIED"
    assert outcome["provider_mutation_started"] is True
    assert outcome["provider_mutation_executed"] is True

    health_fail = dict(post_state)
    health_fail["failed_boundary"] = "HEALTH_VERIFICATION"
    health_fail["step_outcomes"] = {**post_state["step_outcomes"], "health": "failure"}
    outcome = manual_case(resolve_state, mutation_state, health_fail, verified)
    assert outcome["status"] == "INFRASTRUCTURE_FAILURE"
    assert outcome["effect_state"] == "EFFECT_VERIFIED"
    assert outcome["owner_reason_code"] is None

    outcome = manual_case(resolve_state, mutation_state, post_state, None)
    assert outcome["status"] == "RECOVERY_REQUIRED" and outcome["effect_state"] == "UNKNOWN"

    print("AR11 promotion OperationalOutcome fixture matrix passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("read-only", "manual"), default="read-only")
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
    parser.add_argument("--resolve-state-json", type=Path)
    parser.add_argument("--mutation-state-json", type=Path)
    parser.add_argument("--post-verify-state-json", type=Path)
    parser.add_argument("--promotion-verify-json", type=Path)
    parser.add_argument("--failed-boundary", default="")
    parser.add_argument("--evidence-artifact")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0

    common_required = {
        "source-sha": args.source_sha,
        "tree-sha": args.tree_sha,
        "evidence-artifact": args.evidence_artifact,
        "output": args.output,
    }
    mode_required = (
        {
            "release-set-id": args.release_set_id,
            "d1-compatibility-json": args.d1_compatibility_json,
            "release-compatibility-json": args.release_compatibility_json,
            "promotion-plan-json": args.promotion_plan_json,
            "promotion-preflight-json": args.promotion_preflight_json,
            "ready-to-mutate-json": args.ready_to_mutate_json,
        }
        if args.mode == "read-only"
        else {
            "resolve-state-json": args.resolve_state_json,
            "mutation-state-json": args.mutation_state_json,
            "post-verify-state-json": args.post_verify_state_json,
            "promotion-verify-json": args.promotion_verify_json,
        }
    )
    required = {**common_required, **mode_required}
    missing = [name for name, value in required.items() if value is None or value == ""]
    if missing:
        print(f"AR11 OperationalOutcome error: missing required arguments: {', '.join(missing)}", file=sys.stderr)
        return 2
    if SOURCE_SHA.fullmatch(args.source_sha) is None or SOURCE_SHA.fullmatch(args.tree_sha) is None:
        print("AR11 OperationalOutcome error: source/tree SHA must be exact 40-char lowercase hex", file=sys.stderr)
        return 2
    if args.mode == "read-only" and RELEASE_SET_ID.fullmatch(args.release_set_id) is None:
        print("AR11 OperationalOutcome error: release-set-id must be an exact v3 Release Set ID", file=sys.stderr)
        return 2
    try:
        outcome = build_from_files(args) if args.mode == "read-only" else build_manual_from_files(args)
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
