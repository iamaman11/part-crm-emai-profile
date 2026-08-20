#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
from pathlib import Path
from typing import Any

RELEASE_ID = re.compile(r"^release-set-v2-sha256-[0-9a-f]{64}$")
PROFILE = "rehearsal-core-v1"
ENVIRONMENT = "staging"


class EvidenceError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise EvidenceError(message)


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")
    if not isinstance(value, dict):
        fail(f"{path} must contain a JSON object")
    return value


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def release_identity(document: dict[str, Any]) -> tuple[str, str]:
    release_set_id = document.get("release_set_id")
    if not isinstance(release_set_id, str) or not RELEASE_ID.fullmatch(release_set_id):
        fail("release-set.json has invalid v2 release_set_id")
    payload = dict(document)
    payload.pop("release_set_id", None)
    payload.pop("display_version", None)
    expected = "release-set-v2-sha256-" + hashlib.sha256(canonical_bytes(payload)).hexdigest()
    if release_set_id != expected:
        fail(f"release-set.json content address mismatch: expected {expected}, observed {release_set_id}")
    return release_set_id, hashlib.sha256(canonical_bytes(document)).hexdigest()


def require_bool(value: dict[str, Any], key: str, expected: bool, label: str) -> None:
    if value.get(key) is not expected:
        fail(f"{label} requires {key}={expected!r}")


def require_equal(value: dict[str, Any], key: str, expected: Any, label: str) -> None:
    if value.get(key) != expected:
        fail(f"{label} requires {key}={expected!r}, observed={value.get(key)!r}")


def verify_release_result(value: dict[str, Any], release_set_id: str, label: str) -> None:
    require_equal(value, "command", "release.verify", label)
    require_equal(value, "decision", "VALID", label)
    require_equal(value, "release_set_id", release_set_id, label)
    require_bool(value, "source_accepted", True, label)
    require_bool(value, "mutation_executed", False, label)
    verified_files = value.get("verified_files")
    if not isinstance(verified_files, int) or verified_files <= 0:
        fail(f"{label} must prove at least one verified durable file")
    components = value.get("verified_components")
    if not isinstance(components, list) or not components:
        fail(f"{label} must prove verified components")


def verify_plan(
    value: dict[str, Any],
    *,
    current: str,
    target: str,
    decision: str,
    label: str,
) -> None:
    require_equal(value, "command", "promotion.plan", label)
    require_equal(value, "environment", ENVIRONMENT, label)
    require_equal(value, "target_capability_profile_id", PROFILE, label)
    require_equal(value, "observed_current_release_set_id", current, label)
    require_equal(value, "target_release_set_id", target, label)
    require_equal(value, "decision", decision, label)
    require_bool(value, "execution_authorized", False, label)
    require_bool(value, "mutation_executed", False, label)
    actions = value.get("actions")
    if not isinstance(actions, list):
        fail(f"{label} actions must be an array")
    if decision == "NO_CHANGE" and actions:
        fail(f"{label} NO_CHANGE must have zero actions")
    if decision == "PLAN":
        deploy = [
            action
            for action in actions
            if isinstance(action, dict)
            and action.get("operation") == "DEPLOY_EXACT_RELEASE_SET_ARTIFACTS"
            and action.get("release_set_id") == target
        ]
        if len(deploy) != 1:
            fail(f"{label} PLAN must contain exactly one exact Release Set deploy action")


def verify_preflight(value: dict[str, Any], *, target: str, label: str) -> None:
    require_equal(value, "command", "promotion.preflight", label)
    require_equal(value, "environment", ENVIRONMENT, label)
    require_equal(value, "target_capability_profile_id", PROFILE, label)
    require_equal(value, "target_release_set_id", target, label)
    require_equal(value, "decision", "READY", label)
    require_bool(value, "ready", True, label)
    require_bool(value, "credential_values_accessed", False, label)
    require_bool(value, "provider_mutation_executed", False, label)
    require_bool(value, "mutation_executed", False, label)
    require_equal(value, "rollback_compatibility", "COMPATIBLE", label)


def verify_post(value: dict[str, Any], *, target: str, label: str) -> None:
    require_equal(value, "command", "promotion.verify", label)
    require_equal(value, "environment", ENVIRONMENT, label)
    require_equal(value, "target_capability_profile_id", PROFILE, label)
    require_equal(value, "target_release_set_id", target, label)
    require_equal(value, "decision", "VERIFIED", label)
    require_bool(value, "verified", True, label)
    require_bool(value, "mutation_executed", False, label)


def verify_bundle(
    root: Path,
    *,
    current: str,
    target: str,
    decision: str,
    label: str,
) -> str:
    release = load_json(root / "release-set.json")
    observed_id, document_digest = release_identity(release)
    if observed_id != target:
        fail(f"{label} release-set.json targets {observed_id}, expected {target}")

    release_verify = load_json(root / "release-verify.json")
    plan = load_json(root / "promotion-plan.json")
    preflight = load_json(root / "promotion-preflight.json")
    post = load_json(root / "promotion-verify.json")

    verify_release_result(release_verify, target, f"{label}:release.verify")
    verify_plan(plan, current=current, target=target, decision=decision, label=f"{label}:plan")
    verify_preflight(preflight, target=target, label=f"{label}:preflight")
    verify_post(post, target=target, label=f"{label}:post")

    promotion_id = plan.get("promotion_id")
    if not isinstance(promotion_id, str) or not promotion_id:
        fail(f"{label} plan promotion_id missing")
    if preflight.get("promotion_id") != promotion_id:
        fail(f"{label} plan/preflight promotion_id mismatch")
    return document_digest


def verify_blocked_preflight(path: Path, older: str) -> None:
    value = load_json(path)
    require_equal(value, "command", "promotion.preflight", "blocked rollback fixture")
    require_equal(value, "environment", ENVIRONMENT, "blocked rollback fixture")
    require_equal(value, "target_capability_profile_id", PROFILE, "blocked rollback fixture")
    require_equal(value, "target_release_set_id", older, "blocked rollback fixture")
    require_equal(value, "decision", "BLOCKED", "blocked rollback fixture")
    require_bool(value, "ready", False, "blocked rollback fixture")
    require_bool(value, "credential_values_accessed", False, "blocked rollback fixture")
    require_bool(value, "provider_mutation_executed", False, "blocked rollback fixture")
    require_bool(value, "mutation_executed", False, "blocked rollback fixture")
    blockers = value.get("blockers")
    if not isinstance(blockers, list) or not any(
        blocker in {"ROLLBACK_INCOMPATIBLE", "ROLLBACK_COMPATIBILITY_UNKNOWN"}
        for blocker in blockers
    ):
        fail("blocked rollback fixture must contain typed rollback compatibility blocker")


def verify_scenario(args: argparse.Namespace) -> dict[str, Any]:
    older = args.older
    newer = args.newer
    if not RELEASE_ID.fullmatch(older) or not RELEASE_ID.fullmatch(newer) or older == newer:
        fail("older/newer must be two distinct Release Set v2 IDs")

    b_first = verify_bundle(args.a_to_b, current=older, target=newer, decision="PLAN", label="A_TO_B")
    b_second = verify_bundle(
        args.b_no_change,
        current=newer,
        target=newer,
        decision="NO_CHANGE",
        label="B_NO_CHANGE",
    )
    a_first = verify_bundle(args.b_to_a, current=newer, target=older, decision="PLAN", label="B_TO_A")
    a_second = verify_bundle(
        args.a_no_change,
        current=older,
        target=older,
        decision="NO_CHANGE",
        label="A_NO_CHANGE",
    )
    if b_first != b_second:
        fail("B durable release-set document changed between deployment and NO_CHANGE proof")
    if a_first != a_second:
        fail("A durable release-set document changed between rollback and NO_CHANGE proof")
    verify_blocked_preflight(args.blocked_preflight, older)

    return {
        "schema_version": 1,
        "kind": "AR11_STAGING_REHEARSAL_EVIDENCE",
        "environment": ENVIRONMENT,
        "profile": PROFILE,
        "older_release_set_id": older,
        "newer_release_set_id": newer,
        "a_to_b": "VERIFIED",
        "b_no_change": "VERIFIED",
        "b_to_a": "VERIFIED",
        "a_no_change": "VERIFIED",
        "rollback_negative": "BLOCKED_BEFORE_MUTATION",
        "production_mutation": False,
    }


def write_fixture_bundle(root: Path, current: str, target: str, decision: str) -> None:
    payload = {"schema_version": 2, "source": {"commit_sha": "a" * 40}}
    release_id = "release-set-v2-sha256-" + hashlib.sha256(canonical_bytes(payload)).hexdigest()
    if release_id != target:
        payload = {"schema_version": 2, "fixture_target": target}
        target = "release-set-v2-sha256-" + hashlib.sha256(canonical_bytes(payload)).hexdigest()
    release = dict(payload)
    release["release_set_id"] = target
    root.mkdir(parents=True, exist_ok=True)
    (root / "release-set.json").write_text(json.dumps(release), encoding="utf-8")
    (root / "release-verify.json").write_text(
        json.dumps(
            {
                "command": "release.verify",
                "decision": "VALID",
                "release_set_id": target,
                "source_accepted": True,
                "verified_files": 4,
                "verified_components": ["control_plane"],
                "mutation_executed": False,
            }
        ),
        encoding="utf-8",
    )
    promotion_id = hashlib.sha256(f"{current}:{target}".encode()).hexdigest()
    actions: list[dict[str, Any]] = []
    if decision == "PLAN":
        actions = [
            {
                "authority": "WRANGLER_DEPLOY",
                "operation": "DEPLOY_EXACT_RELEASE_SET_ARTIFACTS",
                "release_set_id": target,
            }
        ]
    (root / "promotion-plan.json").write_text(
        json.dumps(
            {
                "command": "promotion.plan",
                "decision": decision,
                "promotion_id": promotion_id,
                "environment": ENVIRONMENT,
                "observed_current_release_set_id": current,
                "target_release_set_id": target,
                "target_capability_profile_id": PROFILE,
                "actions": actions,
                "execution_authorized": False,
                "mutation_executed": False,
            }
        ),
        encoding="utf-8",
    )
    (root / "promotion-preflight.json").write_text(
        json.dumps(
            {
                "command": "promotion.preflight",
                "decision": "READY",
                "ready": True,
                "promotion_id": promotion_id,
                "environment": ENVIRONMENT,
                "target_release_set_id": target,
                "target_capability_profile_id": PROFILE,
                "rollback_compatibility": "COMPATIBLE",
                "credential_values_accessed": False,
                "provider_mutation_executed": False,
                "mutation_executed": False,
            }
        ),
        encoding="utf-8",
    )
    (root / "promotion-verify.json").write_text(
        json.dumps(
            {
                "command": "promotion.verify",
                "decision": "VERIFIED",
                "verified": True,
                "environment": ENVIRONMENT,
                "target_release_set_id": target,
                "target_capability_profile_id": PROFILE,
                "mutation_executed": False,
            }
        ),
        encoding="utf-8",
    )


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="ar11-fc6-") as temporary:
        root = Path(temporary)
        a_payload = {"schema_version": 2, "fixture": "A"}
        b_payload = {"schema_version": 2, "fixture": "B"}
        older = "release-set-v2-sha256-" + hashlib.sha256(canonical_bytes(a_payload)).hexdigest()
        newer = "release-set-v2-sha256-" + hashlib.sha256(canonical_bytes(b_payload)).hexdigest()

        def bundle(name: str, payload: dict[str, Any], current: str, target: str, decision: str) -> Path:
            directory = root / name
            write_fixture_bundle(directory, current, target, decision)
            release = dict(payload)
            release["release_set_id"] = target
            (directory / "release-set.json").write_text(json.dumps(release), encoding="utf-8")
            return directory

        a_to_b = bundle("a-to-b", b_payload, older, newer, "PLAN")
        b_no_change = bundle("b-no-change", b_payload, newer, newer, "NO_CHANGE")
        b_to_a = bundle("b-to-a", a_payload, newer, older, "PLAN")
        a_no_change = bundle("a-no-change", a_payload, older, older, "NO_CHANGE")
        blocked = root / "blocked.json"
        blocked.write_text(
            json.dumps(
                {
                    "command": "promotion.preflight",
                    "decision": "BLOCKED",
                    "ready": False,
                    "environment": ENVIRONMENT,
                    "target_release_set_id": older,
                    "target_capability_profile_id": PROFILE,
                    "blockers": ["ROLLBACK_INCOMPATIBLE"],
                    "credential_values_accessed": False,
                    "provider_mutation_executed": False,
                    "mutation_executed": False,
                }
            ),
            encoding="utf-8",
        )
        args = argparse.Namespace(
            older=older,
            newer=newer,
            a_to_b=a_to_b,
            b_no_change=b_no_change,
            b_to_a=b_to_a,
            a_no_change=a_no_change,
            blocked_preflight=blocked,
        )
        result = verify_scenario(args)
        if result["rollback_negative"] != "BLOCKED_BEFORE_MUTATION":
            fail("positive rehearsal self-test did not converge")
        tampered = load_json(a_to_b / "promotion-plan.json")
        tampered["mutation_executed"] = True
        (a_to_b / "promotion-plan.json").write_text(json.dumps(tampered), encoding="utf-8")
        try:
            verify_scenario(args)
        except EvidenceError:
            pass
        else:
            fail("mutation-executed rehearsal fixture unexpectedly passed")
    print("AR-11 FC-6 staging rehearsal evidence verifier negative self-test passed.")


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    root.add_argument("--self-test", action="store_true")
    root.add_argument("--older")
    root.add_argument("--newer")
    root.add_argument("--a-to-b", type=Path)
    root.add_argument("--b-no-change", type=Path)
    root.add_argument("--b-to-a", type=Path)
    root.add_argument("--a-no-change", type=Path)
    root.add_argument("--blocked-preflight", type=Path)
    root.add_argument("--output", type=Path)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        required = [
            args.older,
            args.newer,
            args.a_to_b,
            args.b_no_change,
            args.b_to_a,
            args.a_no_change,
            args.blocked_preflight,
        ]
        if any(value is None for value in required):
            fail("real rehearsal verification requires all A/B bundle arguments and blocked preflight evidence")
        result = verify_scenario(args)
        text = json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n"
        if args.output is None:
            print(text, end="")
        else:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(text, encoding="utf-8")
        return 0
    except EvidenceError as error:
        print(f"AR-11 FC-6 rehearsal evidence error: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
