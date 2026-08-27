#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
from pathlib import Path
from typing import Any

RELEASE_ID = re.compile(r"^release-set-v3-sha256-[0-9a-f]{64}$")
PROFILE = "rehearsal-core-v2"
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


def require_equal(value: dict[str, Any], key: str, expected: Any, label: str) -> None:
    if value.get(key) != expected:
        fail(f"{label} requires {key}={expected!r}, observed={value.get(key)!r}")


def require_bool(value: dict[str, Any], key: str, expected: bool, label: str) -> None:
    if value.get(key) is not expected:
        fail(f"{label} requires {key}={expected!r}")


def release_bytes_digest(path: Path, target: str, label: str) -> str:
    release = load_json(path)
    require_equal(release, "schema_version", 3, label)
    require_equal(release, "release_set_id", target, label)
    try:
        payload = path.read_bytes()
    except OSError as error:
        fail(f"cannot read {path}: {error}")
    if not payload:
        fail(f"{label} release-set.json must not be empty")
    return hashlib.sha256(payload).hexdigest()


def verify_release_result(value: dict[str, Any], target: str, label: str) -> None:
    require_equal(value, "command", "release.verify", label)
    require_equal(value, "decision", "VALID", label)
    require_equal(value, "release_set_schema_version", 3, label)
    require_equal(value, "release_set_id", target, label)
    require_equal(value, "verification_scope", "CURRENT_V3_FULL_RELEASE_VERIFICATION", label)
    require_bool(value, "historical_compatibility_only", False, label)
    require_bool(value, "source_accepted", True, label)
    require_bool(value, "mutation_executed", False, label)
    if not isinstance(value.get("verified_files"), int) or value["verified_files"] <= 0:
        fail(f"{label} must prove durable verified files")
    if not isinstance(value.get("verified_components"), list) or not value["verified_components"]:
        fail(f"{label} must prove verified components")
    if not isinstance(value.get("verified_provenance_dimensions"), list) or not value[
        "verified_provenance_dimensions"
    ]:
        fail(f"{label} must prove current v3 provenance dimensions")


def verify_plan(
    value: dict[str, Any], *, current: str, target: str, decision: str, label: str
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


def verify_preflight(value: dict[str, Any], target: str, label: str) -> None:
    require_equal(value, "command", "promotion.preflight", label)
    require_equal(value, "environment", ENVIRONMENT, label)
    require_equal(value, "target_capability_profile_id", PROFILE, label)
    require_equal(value, "target_release_set_id", target, label)
    require_equal(value, "decision", "READY", label)
    require_bool(value, "ready", True, label)
    require_equal(value, "rollback_compatibility", "COMPATIBLE", label)
    require_bool(value, "credential_values_accessed", False, label)
    require_bool(value, "provider_mutation_executed", False, label)
    require_bool(value, "mutation_executed", False, label)


def verify_post(value: dict[str, Any], target: str, label: str) -> None:
    require_equal(value, "command", "promotion.verify", label)
    require_equal(value, "environment", ENVIRONMENT, label)
    require_equal(value, "target_capability_profile_id", PROFILE, label)
    require_equal(value, "target_release_set_id", target, label)
    require_equal(value, "decision", "VERIFIED", label)
    require_bool(value, "verified", True, label)
    require_bool(value, "mutation_executed", False, label)


def verify_bundle(
    root: Path, *, current: str, target: str, decision: str, label: str
) -> str:
    digest = release_bytes_digest(root / "release-set.json", target, label)
    verify_release_result(load_json(root / "release-verify.json"), target, f"{label}:release.verify")
    plan = load_json(root / "promotion-plan.json")
    preflight = load_json(root / "promotion-preflight.json")
    verify_plan(plan, current=current, target=target, decision=decision, label=f"{label}:plan")
    verify_preflight(preflight, target, f"{label}:preflight")
    verify_post(load_json(root / "promotion-verify.json"), target, f"{label}:post")
    promotion_id = plan.get("promotion_id")
    if not isinstance(promotion_id, str) or not promotion_id:
        fail(f"{label} plan promotion_id missing")
    if preflight.get("promotion_id") != promotion_id:
        fail(f"{label} plan/preflight promotion_id mismatch")
    return digest


def verify_blocked_preflight(path: Path, older: str) -> None:
    value = load_json(path)
    label = "blocked rollback fixture"
    require_equal(value, "command", "promotion.preflight", label)
    require_equal(value, "environment", ENVIRONMENT, label)
    require_equal(value, "target_capability_profile_id", PROFILE, label)
    require_equal(value, "target_release_set_id", older, label)
    require_equal(value, "decision", "BLOCKED", label)
    require_bool(value, "ready", False, label)
    require_bool(value, "credential_values_accessed", False, label)
    require_bool(value, "provider_mutation_executed", False, label)
    require_bool(value, "mutation_executed", False, label)
    blockers = value.get("blockers")
    allowed = {"ROLLBACK_INCOMPATIBLE", "ROLLBACK_COMPATIBILITY_UNKNOWN"}
    if not isinstance(blockers, list) or not any(item in allowed for item in blockers):
        fail("blocked rollback fixture must contain a typed rollback compatibility blocker")


def verify_scenario(args: argparse.Namespace) -> dict[str, Any]:
    older, newer = args.older, args.newer
    if not isinstance(older, str) or not isinstance(newer, str):
        fail("older/newer Release Set IDs are required")
    if not RELEASE_ID.fullmatch(older) or not RELEASE_ID.fullmatch(newer) or older == newer:
        fail("older/newer must be two distinct current Release Set v3 IDs")

    b_first = verify_bundle(args.a_to_b, current=older, target=newer, decision="PLAN", label="A_TO_B")
    b_second = verify_bundle(
        args.b_no_change, current=newer, target=newer, decision="NO_CHANGE", label="B_NO_CHANGE"
    )
    a_first = verify_bundle(args.b_to_a, current=newer, target=older, decision="PLAN", label="B_TO_A")
    a_second = verify_bundle(
        args.a_no_change, current=older, target=older, decision="NO_CHANGE", label="A_NO_CHANGE"
    )
    if b_first != b_second:
        fail("B durable release-set bytes changed between deployment and NO_CHANGE proof")
    if a_first != a_second:
        fail("A durable release-set bytes changed between rollback and NO_CHANGE proof")
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


def fixture_release_id(label: str) -> str:
    # Shape-only fixture ID; semantic Release Set identity remains owned by typed Rust.
    return "release-set-v3-sha256-" + hashlib.sha256(f"fixture:{label}".encode()).hexdigest()


def write_fixture_bundle(
    root: Path, *, marker: str, current: str, target: str, decision: str
) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "release-set.json").write_text(
        json.dumps(
            {"schema_version": 3, "release_set_id": target, "fixture": marker},
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    (root / "release-verify.json").write_text(
        json.dumps(
            {
                "command": "release.verify",
                "decision": "VALID",
                "release_set_schema_version": 3,
                "release_set_id": target,
                "verification_scope": "CURRENT_V3_FULL_RELEASE_VERIFICATION",
                "historical_compatibility_only": False,
                "source_accepted": True,
                "verified_files": 5,
                "verified_components": ["control_plane"],
                "verified_provenance_dimensions": ["contracts"],
                "mutation_executed": False,
            }
        ),
        encoding="utf-8",
    )
    promotion_id = hashlib.sha256(f"{current}:{target}".encode()).hexdigest()
    actions = []
    if decision == "PLAN":
        actions = [
            {
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


def expect_rejected(args: argparse.Namespace, message: str) -> None:
    try:
        verify_scenario(args)
    except EvidenceError:
        return
    fail(message)


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="ar11-fc6-") as temporary:
        root = Path(temporary)
        older, newer = fixture_release_id("A"), fixture_release_id("B")
        paths = [root / name for name in ("a-to-b", "b-no-change", "b-to-a", "a-no-change")]
        write_fixture_bundle(paths[0], marker="B", current=older, target=newer, decision="PLAN")
        write_fixture_bundle(paths[1], marker="B", current=newer, target=newer, decision="NO_CHANGE")
        write_fixture_bundle(paths[2], marker="A", current=newer, target=older, decision="PLAN")
        write_fixture_bundle(paths[3], marker="A", current=older, target=older, decision="NO_CHANGE")
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
            a_to_b=paths[0],
            b_no_change=paths[1],
            b_to_a=paths[2],
            a_no_change=paths[3],
            blocked_preflight=blocked,
        )
        verify_scenario(args)

        plan = paths[0] / "promotion-plan.json"
        original = plan.read_bytes()
        value = load_json(plan)
        value["mutation_executed"] = True
        plan.write_text(json.dumps(value), encoding="utf-8")
        expect_rejected(args, "mutation-executed evidence unexpectedly passed")
        plan.write_bytes(original)

        release = paths[1] / "release-set.json"
        original = release.read_bytes()
        release.write_bytes(original + b" ")
        expect_rejected(args, "changed durable Release Set bytes unexpectedly passed")
        release.write_bytes(original)

        result = paths[0] / "release-verify.json"
        original = result.read_bytes()
        value = load_json(result)
        value.update(
            {
                "release_set_schema_version": 2,
                "verification_scope": "HISTORICAL_V2_SOURCE_AND_ARTIFACT_INTEGRITY",
                "historical_compatibility_only": True,
            }
        )
        result.write_text(json.dumps(value), encoding="utf-8")
        expect_rejected(args, "historical v2 verification unexpectedly passed current ceremony")
        result.write_bytes(original)

        v2_args = argparse.Namespace(**vars(args))
        v2_args.older = "release-set-v2-sha256-" + "d" * 64
        expect_rejected(v2_args, "historical v2 ID unexpectedly passed current ceremony")

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
            fail("real rehearsal verification requires all A/B bundles and blocked preflight evidence")
        text = json.dumps(verify_scenario(args), sort_keys=True, separators=(",", ":")) + "\n"
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
