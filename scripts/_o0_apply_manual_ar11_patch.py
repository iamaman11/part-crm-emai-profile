#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/release-set-promotion.yml"
PROJECTOR = ROOT / "scripts/promotion-operational-outcome-ar11.py"
RUST_TEST = ROOT / "tools/opsctl/tests/o0_ar11_operational_outcome.rs"
SELF = ROOT / "scripts/_o0_apply_manual_ar11_patch.py"
RUNNER = ROOT / ".github/workflows/_o0-branch-patch.yml"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, observed {count}")
    return text.replace(old, new, 1)


def patch_section(text: str, start: str, end: str, transform) -> str:
    start_i = text.index(start)
    end_i = text.index(end, start_i + len(start))
    section = text[start_i:end_i]
    section = transform(section)
    return text[:start_i] + section + text[end_i:]


def patch_projector() -> None:
    text = PROJECTOR.read_text(encoding="utf-8")
    text = replace_once(
        text,
        'PROCEDURE = "AR11_RELEASE_SET_PROMOTION_READ_ONLY"\n',
        'READ_ONLY_PROCEDURE = "AR11_RELEASE_SET_PROMOTION_READ_ONLY"\nMANUAL_PROCEDURE = "AR11_RELEASE_SET_PROMOTION"\n',
        "projector procedure constants",
    )
    text = replace_once(
        text,
        '''    try:\n        digest = sha256_file(path)\n        value = json.loads(path.read_text(encoding="utf-8"))\n    except (OSError, UnicodeError, json.JSONDecodeError) as error:\n        return LoadedInput(label, path, "MALFORMED", None, None, f"{label}: {error}")\n''',
        '''    digest: str | None = None\n    try:\n        digest = sha256_file(path)\n        value = json.loads(path.read_text(encoding="utf-8"))\n    except (OSError, UnicodeError, json.JSONDecodeError) as error:\n        return LoadedInput(label, path, "MALFORMED", None, digest, f"{label}: {error}")\n''',
        "preserve malformed input digest",
    )
    text = replace_once(text, '        "procedure": PROCEDURE,\n', '        "procedure": READ_ONLY_PROCEDURE,\n', "read-only procedure projection")

    manual_block = r'''

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
'''
    text = replace_once(text, '\ndef state_for(record: LoadedInput) -> dict[str, Any]:\n', manual_block + '\ndef state_for(record: LoadedInput) -> dict[str, Any]:\n', "manual projector insertion")

    manual_self_test = r'''

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
'''
    text = replace_once(text, '    print("AR11 promotion OperationalOutcome fixture matrix passed.")\n', manual_self_test + '\n    print("AR11 promotion OperationalOutcome fixture matrix passed.")\n', "manual self-test matrix")

    text = replace_once(
        text,
        '    parser = argparse.ArgumentParser()\n    parser.add_argument("--source-sha")\n',
        '    parser = argparse.ArgumentParser()\n    parser.add_argument("--mode", choices=("read-only", "manual"), default="read-only")\n    parser.add_argument("--source-sha")\n',
        "mode parser",
    )
    text = replace_once(
        text,
        '    parser.add_argument("--ready-to-mutate-json", type=Path)\n',
        '    parser.add_argument("--ready-to-mutate-json", type=Path)\n    parser.add_argument("--resolve-state-json", type=Path)\n    parser.add_argument("--mutation-state-json", type=Path)\n    parser.add_argument("--post-verify-state-json", type=Path)\n    parser.add_argument("--promotion-verify-json", type=Path)\n',
        "manual parser inputs",
    )
    old_required = '''    required = {\n        "source-sha": args.source_sha,\n        "tree-sha": args.tree_sha,\n        "release-set-id": args.release_set_id,\n        "d1-compatibility-json": args.d1_compatibility_json,\n        "release-compatibility-json": args.release_compatibility_json,\n        "promotion-plan-json": args.promotion_plan_json,\n        "promotion-preflight-json": args.promotion_preflight_json,\n        "ready-to-mutate-json": args.ready_to_mutate_json,\n        "evidence-artifact": args.evidence_artifact,\n        "output": args.output,\n    }\n    missing = [name for name, value in required.items() if value is None or value == ""]\n    if missing:\n        print(f"AR11 OperationalOutcome error: missing required arguments: {', '.join(missing)}", file=sys.stderr)\n        return 2\n    if SOURCE_SHA.fullmatch(args.source_sha) is None or SOURCE_SHA.fullmatch(args.tree_sha) is None:\n        print("AR11 OperationalOutcome error: source/tree SHA must be exact 40-char lowercase hex", file=sys.stderr)\n        return 2\n    if RELEASE_SET_ID.fullmatch(args.release_set_id) is None:\n        print("AR11 OperationalOutcome error: release-set-id must be an exact v3 Release Set ID", file=sys.stderr)\n        return 2\n    try:\n        outcome = build_from_files(args)\n'''
    new_required = '''    common_required = {\n        "source-sha": args.source_sha,\n        "tree-sha": args.tree_sha,\n        "evidence-artifact": args.evidence_artifact,\n        "output": args.output,\n    }\n    mode_required = (\n        {\n            "release-set-id": args.release_set_id,\n            "d1-compatibility-json": args.d1_compatibility_json,\n            "release-compatibility-json": args.release_compatibility_json,\n            "promotion-plan-json": args.promotion_plan_json,\n            "promotion-preflight-json": args.promotion_preflight_json,\n            "ready-to-mutate-json": args.ready_to_mutate_json,\n        }\n        if args.mode == "read-only"\n        else {\n            "resolve-state-json": args.resolve_state_json,\n            "mutation-state-json": args.mutation_state_json,\n            "post-verify-state-json": args.post_verify_state_json,\n            "promotion-verify-json": args.promotion_verify_json,\n        }\n    )\n    required = {**common_required, **mode_required}\n    missing = [name for name, value in required.items() if value is None or value == ""]\n    if missing:\n        print(f"AR11 OperationalOutcome error: missing required arguments: {', '.join(missing)}", file=sys.stderr)\n        return 2\n    if SOURCE_SHA.fullmatch(args.source_sha) is None or SOURCE_SHA.fullmatch(args.tree_sha) is None:\n        print("AR11 OperationalOutcome error: source/tree SHA must be exact 40-char lowercase hex", file=sys.stderr)\n        return 2\n    if args.mode == "read-only" and RELEASE_SET_ID.fullmatch(args.release_set_id) is None:\n        print("AR11 OperationalOutcome error: release-set-id must be an exact v3 Release Set ID", file=sys.stderr)\n        return 2\n    try:\n        outcome = build_from_files(args) if args.mode == "read-only" else build_manual_from_files(args)\n'''
    text = replace_once(text, old_required, new_required, "mode-specific required arguments")
    PROJECTOR.write_text(text, encoding="utf-8")


def patch_workflow() -> None:
    text = WORKFLOW.read_text(encoding="utf-8")
    text = replace_once(
        text,
        '      - name: Set up pinned Node runtime for read-only provider observation\n        uses: actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e\n',
        '      - name: Set up pinned Node runtime for read-only provider observation\n        id: node_runtime\n        uses: actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e\n',
        "automatic node id",
    )
    text = replace_once(
        text,
        '''          elif [ "${{ steps.source_checkout.outcome }}" = "failure" ]; then\n            failed_boundary="SOURCE_CHECKOUT"\n          elif [ "${{ steps.materialize.outcome }}" = "failure" ]; then\n''',
        '''          elif [ "${{ steps.source_checkout.outcome }}" = "failure" ]; then\n            failed_boundary="SOURCE_CHECKOUT"\n          elif [ "${{ steps.node_runtime.outcome }}" = "failure" ]; then\n            failed_boundary="NODE_RUNTIME_SETUP"\n          elif [ "${{ steps.materialize.outcome }}" = "failure" ]; then\n''',
        "automatic node boundary",
    )

    def resolve_transform(section: str) -> str:
        section = replace_once(
            section,
            '      promotion_id: ${{ steps.ready.outputs.promotion_id }}\n',
            '      promotion_id: ${{ steps.ready.outputs.promotion_id }}\n      phase_state_b64: ${{ steps.resolve_state.outputs.phase_state_b64 }}\n',
            "resolve phase output",
        )
        section = replace_once(
            section,
            '      - name: Checkout exact current protected-main source\n        uses:',
            '      - name: Checkout exact current protected-main source\n        id: source_checkout\n        uses:',
            "resolve checkout id",
        )
        section = replace_once(
            section,
            '      - name: Re-verify immutable target without provider credentials\n        env:',
            '      - name: Re-verify immutable target without provider credentials\n        id: target_verify\n        env:',
            "resolve target verify id",
        )
        section = replace_once(
            section,
            '''            --root . release verify --release-set "$RUNNER_TEMP/ready/release-set.json" \\\n            --source-root . --artifact-root "$release_root" \\\n            | jq -e '.decision == "VALID" and .release_set_schema_version == 3 and .source_accepted == true and .mutation_executed == false' >/dev/null\n''',
            '''            --root . release verify --release-set "$RUNNER_TEMP/ready/release-set.json" \\\n            --source-root . --artifact-root "$release_root" \\\n            > "$RUNNER_TEMP/manual-release-verify.json"\n          jq -e '.decision == "VALID" and .release_set_schema_version == 3 and .source_accepted == true and .mutation_executed == false' "$RUNNER_TEMP/manual-release-verify.json" >/dev/null\n''',
            "resolve capture-before-assert",
        )
        state = r'''

      - name: Capture manual resolve phase state
        id: resolve_state
        if: always()
        run: |
          set -euo pipefail
          source_sha="${{ steps.intent.outputs.main_sha }}"
          test -n "$source_sha" || source_sha="${{ needs.route.outputs.source_sha }}"
          release_set_id="${{ steps.intent.outputs.release_set_id }}"
          promotion_id="${{ steps.ready.outputs.promotion_id }}"
          failed_boundary=""
          if [ "${{ steps.intent.outcome }}" = "failure" ]; then failed_boundary="AUTHORIZATION_BINDING"
          elif [ "${{ steps.ready.outcome }}" = "failure" ]; then failed_boundary="READY_BINDING"
          elif [ "${{ steps.source_checkout.outcome }}" = "failure" ]; then failed_boundary="SOURCE_CHECKOUT"
          elif [ "${{ steps.target_verify.outcome }}" = "failure" ]; then failed_boundary="TARGET_RELEASE_VERIFY"
          fi
          authorization_bound=false
          if [ "${{ steps.intent.outcome }}" = "success" ] && [ "${{ steps.ready.outcome }}" = "success" ]; then authorization_bound=true; fi
          jq -n \
            --arg source_sha "$source_sha" --arg release_set_id "$release_set_id" --arg promotion_id "$promotion_id" \
            --arg failed_boundary "$failed_boundary" --argjson authorization_bound "$authorization_bound" \
            --arg intent "${{ steps.intent.outcome }}" --arg ready "${{ steps.ready.outcome }}" \
            --arg source_checkout "${{ steps.source_checkout.outcome }}" --arg target_verify "${{ steps.target_verify.outcome }}" \
            '{schema_version:1,kind:"AR11_MANUAL_PHASE_STATE",phase:"RESOLVE_VERIFY",source_sha:$source_sha,release_set_id:(if $release_set_id=="" then null else $release_set_id end),promotion_id:(if $promotion_id=="" then null else $promotion_id end),authorization_bound:$authorization_bound,provider_mutation_started:false,provider_mutation_executed:false,production_mutation_executed:false,failed_boundary:(if $failed_boundary=="" then null else $failed_boundary end),step_outcomes:{intent:$intent,ready:$ready,source_checkout:$source_checkout,target_verify:$target_verify}}' \
            > "$RUNNER_TEMP/manual-resolve-state.json"
          if [ -f "$RUNNER_TEMP/authorized-intent.json" ]; then
            jq -S . "$RUNNER_TEMP/authorized-intent.json" | sha256sum | cut -d' ' -f1 > "$RUNNER_TEMP/authorization-intent.sha256"
          fi
          phase_state_b64="$(base64 -w0 "$RUNNER_TEMP/manual-resolve-state.json")"
          echo "phase_state_b64=$phase_state_b64" >> "$GITHUB_OUTPUT"
'''
        return section.rstrip() + state + "\n"

    text = patch_section(text, "\n  resolve-verify:\n", "\n  mutate:\n", resolve_transform)

    def mutate_transform(section: str) -> str:
        section = replace_once(
            section,
            '''    permissions:\n      actions: read\n      contents: read\n      deployments: write\n    env:\n''',
            '''    permissions:\n      actions: read\n      contents: read\n      deployments: write\n    outputs:\n      phase_state_b64: ${{ steps.mutation_state.outputs.phase_state_b64 }}\n    env:\n''',
            "mutation phase output",
        )
        ids = [
            ("Checkout exact authorized source", "authorized_checkout"),
            ("Download and bind prior READY evidence before provider credentials", "ready_bind"),
            ("Re-verify exact immutable Release Set before credentials", "target_verify"),
            ("Set up pinned Node before deploy credential", "node_runtime"),
            ("Render mutation overlay and prove exact bits dry-run without deploy credential", "dry_run"),
            ("Activate deploy credential only after bound READY and authorization", "deploy_credential"),
            ("Re-fence Worker identity and D1 head immediately before mutation", "exact_fence"),
            ("Deploy exact Release Set v3 bits after all fences", "deploy"),
        ]
        for name, ident in ids:
            section = replace_once(section, f"      - name: {name}\n", f"      - name: {name}\n        id: {ident}\n", f"mutation id {ident}")
        section = replace_once(
            section,
            '''            --root . release verify --release-set "$RUNNER_TEMP/ready/release-set.json" \\\n            --source-root . --artifact-root "$release_root" \\\n            | jq -e '.decision == "VALID" and .release_set_schema_version == 3 and .source_accepted == true and .mutation_executed == false' >/dev/null\n''',
            '''            --root . release verify --release-set "$RUNNER_TEMP/ready/release-set.json" \\\n            --source-root . --artifact-root "$release_root" \\\n            > "$RUNNER_TEMP/manual-mutation-release-verify.json"\n          jq -e '.decision == "VALID" and .release_set_schema_version == 3 and .source_accepted == true and .mutation_executed == false' "$RUNNER_TEMP/manual-mutation-release-verify.json" >/dev/null\n''',
            "mutation capture-before-assert",
        )
        section = replace_once(
            section,
            '''      - name: Deploy exact Release Set v3 bits after all fences\n        id: deploy\n''',
            '''      - name: Mark provider mutation invocation boundary\n        id: mutation_start\n        run: |\n          set -euo pipefail\n          jq -n --arg source_sha "${{ needs.resolve-verify.outputs.main_sha }}" --arg release_set_id "$RELEASE_SET_ID" --arg promotion_id "${{ needs.resolve-verify.outputs.promotion_id }}" --arg authorization_digest "$AUTHORIZATION_DIGEST" --arg ready_evidence_sha256 "$READY_EVIDENCE_SHA256" '{schema_version:1,kind:"AR11_PROVIDER_MUTATION_INVOCATION",source_sha:$source_sha,release_set_id:$release_set_id,promotion_id:$promotion_id,authorization_digest:$authorization_digest,ready_evidence_sha256:$ready_evidence_sha256}' > "$RUNNER_TEMP/provider-mutation-invocation.json"\n\n      - name: Deploy exact Release Set v3 bits after all fences\n        id: deploy\n''',
            "mutation invocation marker",
        )
        state = r'''

      - name: Capture manual mutation phase state
        id: mutation_state
        if: always()
        run: |
          set -euo pipefail
          failed_boundary=""
          if [ "${{ steps.authorized_checkout.outcome }}" = "failure" ]; then failed_boundary="AUTHORIZED_SOURCE_CHECKOUT"
          elif [ "${{ steps.ready_bind.outcome }}" = "failure" ]; then failed_boundary="READY_FENCE_BINDING"
          elif [ "${{ steps.target_verify.outcome }}" = "failure" ]; then failed_boundary="TARGET_RELEASE_VERIFY"
          elif [ "${{ steps.node_runtime.outcome }}" = "failure" ]; then failed_boundary="NODE_RUNTIME_SETUP"
          elif [ "${{ steps.dry_run.outcome }}" = "failure" ]; then failed_boundary="MUTATION_DRY_RUN"
          elif [ "${{ steps.deploy_credential.outcome }}" = "failure" ]; then failed_boundary="DEPLOY_CREDENTIAL_ACTIVATION"
          elif [ "${{ steps.exact_fence.outcome }}" = "failure" ]; then failed_boundary="EXACT_CURRENT_FENCE"
          elif [ "${{ steps.mutation_start.outcome }}" = "failure" ]; then failed_boundary="MUTATION_INVOCATION_BOUNDARY"
          elif [ "${{ steps.deploy.outcome }}" = "failure" ]; then failed_boundary="PROVIDER_DEPLOY"
          fi
          mutation_started=false
          mutation_executed=false
          if [ "${{ steps.mutation_start.outcome }}" = "success" ]; then mutation_started=true; fi
          if [ "${{ steps.deploy.outcome }}" = "success" ]; then mutation_executed=true; fi
          jq -n \
            --arg source_sha "${{ needs.resolve-verify.outputs.main_sha }}" --arg release_set_id "$RELEASE_SET_ID" --arg promotion_id "${{ needs.resolve-verify.outputs.promotion_id }}" \
            --arg failed_boundary "$failed_boundary" --argjson mutation_started "$mutation_started" --argjson mutation_executed "$mutation_executed" \
            --arg authorized_checkout "${{ steps.authorized_checkout.outcome }}" --arg ready_bind "${{ steps.ready_bind.outcome }}" \
            --arg target_verify "${{ steps.target_verify.outcome }}" --arg node_runtime "${{ steps.node_runtime.outcome }}" \
            --arg dry_run "${{ steps.dry_run.outcome }}" --arg deploy_credential "${{ steps.deploy_credential.outcome }}" \
            --arg exact_fence "${{ steps.exact_fence.outcome }}" --arg mutation_start "${{ steps.mutation_start.outcome }}" --arg deploy "${{ steps.deploy.outcome }}" \
            '{schema_version:1,kind:"AR11_MANUAL_PHASE_STATE",phase:"MUTATE",source_sha:$source_sha,release_set_id:$release_set_id,promotion_id:(if $promotion_id=="" then null else $promotion_id end),authorization_bound:true,provider_mutation_started:$mutation_started,provider_mutation_executed:$mutation_executed,production_mutation_executed:false,failed_boundary:(if $failed_boundary=="" then null else $failed_boundary end),step_outcomes:{authorized_checkout:$authorized_checkout,ready_bind:$ready_bind,target_verify:$target_verify,node_runtime:$node_runtime,dry_run:$dry_run,deploy_credential:$deploy_credential,exact_fence:$exact_fence,mutation_start:$mutation_start,deploy:$deploy}}' \
            > "$RUNNER_TEMP/manual-mutation-state.json"
          phase_state_b64="$(base64 -w0 "$RUNNER_TEMP/manual-mutation-state.json")"
          echo "phase_state_b64=$phase_state_b64" >> "$GITHUB_OUTPUT"
'''
        return section.rstrip() + state + "\n"

    text = patch_section(text, "\n  mutate:\n", "\n  post-verify:\n", mutate_transform)

    def post_transform(section: str) -> str:
        section = replace_once(
            section,
            '''    permissions:\n      actions: read\n      contents: read\n    env:\n''',
            '''    permissions:\n      actions: read\n      contents: read\n    outputs:\n      phase_state_b64: ${{ steps.post_state.outputs.phase_state_b64 }}\n      promotion_verify_b64: ${{ steps.post_state.outputs.promotion_verify_b64 }}\n    env:\n''',
            "post phase outputs",
        )
        section = replace_once(section, '      - name: Checkout exact promoted source\n', '      - name: Checkout exact promoted source\n        id: source_checkout\n', "post checkout id")
        section = replace_once(section, '      - name: Download prior READY evidence and prepare read-only target\n', '      - name: Download prior READY evidence and prepare read-only target\n        id: target_prepare\n', "post target id")
        section = replace_once(section, '      - name: Set up pinned Node for post-deploy observation\n', '      - name: Set up pinned Node for post-deploy observation\n        id: node_runtime\n', "post node id")
        section = replace_once(
            section,
            '''            --release-set "$ready_root/release-set.json" --source-root . --artifact-root "$release_root" \\\n            | tee "$RUNNER_TEMP/post-release-verify.json" \\\n            | jq -e '.decision == "VALID" and .release_set_schema_version == 3 and .source_accepted == true and .mutation_executed == false' >/dev/null\n''',
            '''            --release-set "$ready_root/release-set.json" --source-root . --artifact-root "$release_root" \\\n            > "$RUNNER_TEMP/post-release-verify.json"\n          jq -e '.decision == "VALID" and .release_set_schema_version == 3 and .source_accepted == true and .mutation_executed == false' "$RUNNER_TEMP/post-release-verify.json" >/dev/null\n''',
            "post release capture-before-assert",
        )
        old_observe = r'''      - name: Re-observe provider and verify exact convergence
        run: |
          set -euo pipefail
          account_id="$(jq -er '.control_plane.account_id // .account_id' "$DEPLOY_MANIFEST")"
          worker_name="$(jq -er '.control_plane.worker_name // .worker_name' "$DEPLOY_MANIFEST")"
          r2_bucket="$(jq -er '.control_plane.r2_bucket_name // .r2_bucket_name' "$DEPLOY_MANIFEST")"
          queue_name="$(jq -er '.control_plane.integration_events_queue // .integration_events_queue' "$DEPLOY_MANIFEST")"
          custom_domain="$(jq -er '.control_plane.custom_domain // .custom_domain' "$DEPLOY_MANIFEST")"
          curl --fail-with-body --silent --show-error -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" "https://api.cloudflare.com/client/v4/accounts/$account_id/workers/scripts/$worker_name/deployments" | jq '.result.deployments[0]' > "$RUNNER_TEMP/post-deployment.json"
          npx --yes wrangler@4.94.0 d1 execute CATALOG_DB --remote --command 'SELECT name FROM d1_migrations ORDER BY name' --json --config "$WRANGLER_CONFIG" --env staging > "$RUNNER_TEMP/post-catalog-ledger.json"
          npx --yes wrangler@4.94.0 r2 bucket info "$r2_bucket" --json --config "$WRANGLER_CONFIG" --env staging > "$RUNNER_TEMP/post-r2-info.json"
          npx --yes wrangler@4.94.0 queues info "$queue_name" --config "$WRANGLER_CONFIG" --env staging >/dev/null
          jq -n --arg name "$queue_name" '{name:$name}' > "$RUNNER_TEMP/post-queue-info.json"
          npx --yes wrangler@4.94.0 secret list --format json --config "$WRANGLER_CONFIG" --env staging > "$RUNNER_TEMP/post-secret-list.json"
          python scripts/deployment-snapshot-ar11.py --environment staging --collected-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --deployment-status "$RUNNER_TEMP/post-deployment.json" --catalog-ledger "$RUNNER_TEMP/post-catalog-ledger.json" --r2-list "$RUNNER_TEMP/post-r2-info.json" --queue-list "$RUNNER_TEMP/post-queue-info.json" --secret-list "$RUNNER_TEMP/post-secret-list.json" --rendered-config "$WRANGLER_CONFIG" --deploy-manifest "$DEPLOY_MANIFEST" --current-release-set "$RUNNER_TEMP/ready/release-set.json" --output "$RUNNER_TEMP/post-snapshot.json"
          cargo run --locked --quiet --manifest-path tools/opsctl/Cargo.toml -- --root . promotion verify --release-set "$RUNNER_TEMP/ready/release-set.json" --source-root . --profile rehearsal-core-v2 --environment staging --snapshot "$RUNNER_TEMP/post-snapshot.json" --evidence-json "$RUNNER_TEMP/ready/compatibility-evidence.json" | tee "$RUNNER_TEMP/promotion-verify.json" | jq -e '.verified == true and .decision == "VERIFIED" and .mutation_executed == false' >/dev/null
          response="$RUNNER_TEMP/health-response"
          curl --silent --show-error --fail --connect-timeout 10 --max-time 30 -H "CF-Access-Client-Id: $CLOUDFLARE_ACCESS_CLIENT_ID" -H "CF-Access-Client-Secret: $CLOUDFLARE_ACCESS_CLIENT_SECRET" -o "$response" "https://$custom_domain/health"
          test "$(cat "$response")" = ok
'''
        new_observe = r'''      - name: Re-observe provider and capture promotion.verify natural-owner verdict
        id: observe_verify
        run: |
          set -euo pipefail
          account_id="$(jq -er '.control_plane.account_id // .account_id' "$DEPLOY_MANIFEST")"
          worker_name="$(jq -er '.control_plane.worker_name // .worker_name' "$DEPLOY_MANIFEST")"
          r2_bucket="$(jq -er '.control_plane.r2_bucket_name // .r2_bucket_name' "$DEPLOY_MANIFEST")"
          queue_name="$(jq -er '.control_plane.integration_events_queue // .integration_events_queue' "$DEPLOY_MANIFEST")"
          curl --fail-with-body --silent --show-error -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" "https://api.cloudflare.com/client/v4/accounts/$account_id/workers/scripts/$worker_name/deployments" | jq '.result.deployments[0]' > "$RUNNER_TEMP/post-deployment.json"
          npx --yes wrangler@4.94.0 d1 execute CATALOG_DB --remote --command 'SELECT name FROM d1_migrations ORDER BY name' --json --config "$WRANGLER_CONFIG" --env staging > "$RUNNER_TEMP/post-catalog-ledger.json"
          npx --yes wrangler@4.94.0 r2 bucket info "$r2_bucket" --json --config "$WRANGLER_CONFIG" --env staging > "$RUNNER_TEMP/post-r2-info.json"
          npx --yes wrangler@4.94.0 queues info "$queue_name" --config "$WRANGLER_CONFIG" --env staging >/dev/null
          jq -n --arg name "$queue_name" '{name:$name}' > "$RUNNER_TEMP/post-queue-info.json"
          npx --yes wrangler@4.94.0 secret list --format json --config "$WRANGLER_CONFIG" --env staging > "$RUNNER_TEMP/post-secret-list.json"
          python scripts/deployment-snapshot-ar11.py --environment staging --collected-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --deployment-status "$RUNNER_TEMP/post-deployment.json" --catalog-ledger "$RUNNER_TEMP/post-catalog-ledger.json" --r2-list "$RUNNER_TEMP/post-r2-info.json" --queue-list "$RUNNER_TEMP/post-queue-info.json" --secret-list "$RUNNER_TEMP/post-secret-list.json" --rendered-config "$WRANGLER_CONFIG" --deploy-manifest "$DEPLOY_MANIFEST" --current-release-set "$RUNNER_TEMP/ready/release-set.json" --output "$RUNNER_TEMP/post-snapshot.json"
          cargo run --locked --quiet --manifest-path tools/opsctl/Cargo.toml -- --root . promotion verify --release-set "$RUNNER_TEMP/ready/release-set.json" --source-root . --profile rehearsal-core-v2 --environment staging --snapshot "$RUNNER_TEMP/post-snapshot.json" --evidence-json "$RUNNER_TEMP/ready/compatibility-evidence.json" > "$RUNNER_TEMP/promotion-verify.json"
          jq -e '.schema_version == 1 and .command == "promotion.verify" and (.decision == "VERIFIED" or .decision == "DRIFTED" or .decision == "INCOMPLETE" or .decision == "UNKNOWN") and (.verified | type == "boolean") and (.blockers | type == "array") and .mutation_executed == false' "$RUNNER_TEMP/promotion-verify.json" >/dev/null
          decision="$(jq -er '.decision' "$RUNNER_TEMP/promotion-verify.json")"
          echo "decision=$decision" >> "$GITHUB_OUTPUT"

      - name: Verify health only after promotion owner reports VERIFIED
        id: health
        if: steps.observe_verify.outputs.decision == 'VERIFIED'
        run: |
          set -euo pipefail
          custom_domain="$(jq -er '.control_plane.custom_domain // .custom_domain' "$DEPLOY_MANIFEST")"
          response="$RUNNER_TEMP/health-response"
          curl --silent --show-error --fail --connect-timeout 10 --max-time 30 -H "CF-Access-Client-Id: $CLOUDFLARE_ACCESS_CLIENT_ID" -H "CF-Access-Client-Secret: $CLOUDFLARE_ACCESS_CLIENT_SECRET" -o "$response" "https://$custom_domain/health"
          test "$(cat "$response")" = ok
'''
        section = replace_once(section, old_observe, new_observe, "post promotion.verify capture")
        section = replace_once(
            section,
            '      - name: Upload metadata-only post-deploy evidence\n        uses:',
            '      - name: Upload metadata-only post-deploy evidence\n        id: post_evidence\n        if: always() && steps.observe_verify.outcome == \'success\'\n        uses:',
            "post evidence id",
        )
        state = r'''

      - name: Capture manual post-verify phase state
        id: post_state
        if: always()
        run: |
          set -euo pipefail
          failed_boundary=""
          if [ "${{ steps.source_checkout.outcome }}" = "failure" ]; then failed_boundary="POST_SOURCE_CHECKOUT"
          elif [ "${{ steps.target_prepare.outcome }}" = "failure" ]; then failed_boundary="POST_TARGET_MATERIALIZATION"
          elif [ "${{ steps.node_runtime.outcome }}" = "failure" ]; then failed_boundary="NODE_RUNTIME_SETUP"
          elif [ "${{ steps.observe_verify.outcome }}" = "failure" ]; then failed_boundary="POST_PROVIDER_OBSERVATION"
          elif [ "${{ steps.observe_verify.outputs.decision }}" = "VERIFIED" ] && [ "${{ steps.health.outcome }}" = "failure" ]; then failed_boundary="HEALTH_VERIFICATION"
          elif [ "${{ steps.post_evidence.outcome }}" = "failure" ]; then failed_boundary="POST_EVIDENCE_PUBLICATION"
          fi
          jq -n \
            --arg source_sha "${{ needs.resolve-verify.outputs.main_sha }}" --arg release_set_id "$RELEASE_SET_ID" --arg promotion_id "${{ needs.resolve-verify.outputs.promotion_id }}" \
            --arg failed_boundary "$failed_boundary" --arg source_checkout "${{ steps.source_checkout.outcome }}" \
            --arg target_prepare "${{ steps.target_prepare.outcome }}" --arg node_runtime "${{ steps.node_runtime.outcome }}" \
            --arg observe_verify "${{ steps.observe_verify.outcome }}" --arg verify_decision "${{ steps.observe_verify.outputs.decision }}" \
            --arg health "${{ steps.health.outcome }}" --arg post_evidence "${{ steps.post_evidence.outcome }}" \
            '{schema_version:1,kind:"AR11_MANUAL_PHASE_STATE",phase:"POST_VERIFY",source_sha:$source_sha,release_set_id:$release_set_id,promotion_id:(if $promotion_id=="" then null else $promotion_id end),authorization_bound:true,provider_mutation_started:true,provider_mutation_executed:true,production_mutation_executed:false,failed_boundary:(if $failed_boundary=="" then null else $failed_boundary end),step_outcomes:{source_checkout:$source_checkout,target_prepare:$target_prepare,node_runtime:$node_runtime,observe_verify:$observe_verify,verify_decision:$verify_decision,health:$health,post_evidence:$post_evidence}}' \
            > "$RUNNER_TEMP/manual-post-verify-state.json"
          phase_state_b64="$(base64 -w0 "$RUNNER_TEMP/manual-post-verify-state.json")"
          echo "phase_state_b64=$phase_state_b64" >> "$GITHUB_OUTPUT"
          promotion_verify_b64=""
          if [ -f "$RUNNER_TEMP/promotion-verify.json" ]; then promotion_verify_b64="$(base64 -w0 "$RUNNER_TEMP/promotion-verify.json")"; fi
          echo "promotion_verify_b64=$promotion_verify_b64" >> "$GITHUB_OUTPUT"
'''
        return section.rstrip() + state + "\n"

    text = patch_section(text, "\n  post-verify:\n", "\n  rollback-negative-evidence:\n", post_transform)

    manual_job = r'''

  manual-outcome:
    name: Publish one lossless manual AR11 terminal outcome
    needs: [route, resolve-verify, mutate, post-verify]
    if: always() && needs.route.outputs.mode == 'promote'
    runs-on: ubuntu-latest
    permissions:
      contents: read
    env:
      SOURCE_SHA: ${{ needs.route.outputs.source_sha }}
      RESOLVE_STATE_B64: ${{ needs.resolve-verify.outputs.phase_state_b64 }}
      MUTATION_STATE_B64: ${{ needs.mutate.outputs.phase_state_b64 }}
      POST_STATE_B64: ${{ needs.post-verify.outputs.phase_state_b64 }}
      PROMOTION_VERIFY_B64: ${{ needs.post-verify.outputs.promotion_verify_b64 }}
      RESOLVE_RESULT: ${{ needs.resolve-verify.result }}
      MUTATE_RESULT: ${{ needs.mutate.result }}
      POST_RESULT: ${{ needs.post-verify.result }}
    steps:
      - name: Checkout exact manual AR11 source
        id: source_checkout
        uses: actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a
        with:
          ref: ${{ needs.route.outputs.source_sha }}
          fetch-depth: 1
          persist-credentials: false

      - name: Materialize captured manual phase transport
        id: captured
        run: |
          set -euo pipefail
          python - "$RUNNER_TEMP" <<'PY'
          import base64, json, os, pathlib, sys
          root = pathlib.Path(sys.argv[1])
          source = os.environ['SOURCE_SHA']
          def write_state(name, phase, encoded, result, conservative_started=False, conservative_executed=False):
              path = root / name
              if encoded:
                  path.write_bytes(base64.b64decode(encoded, validate=True))
                  return
              state = {
                  'schema_version': 1,
                  'kind': 'AR11_MANUAL_PHASE_STATE',
                  'phase': phase,
                  'source_sha': source,
                  'release_set_id': None,
                  'promotion_id': None,
                  'authorization_bound': False,
                  'provider_mutation_started': conservative_started,
                  'provider_mutation_executed': conservative_executed,
                  'production_mutation_executed': False,
                  'failed_boundary': f'{phase}_JOB_UNTERMINALIZED' if result == 'failure' else f'{phase}_JOB_{result.upper()}',
                  'step_outcomes': {'job_result': result},
              }
              path.write_text(json.dumps(state, sort_keys=True, indent=2) + '\n', encoding='utf-8')
          write_state('manual-resolve-state.json', 'RESOLVE_VERIFY', os.environ.get('RESOLVE_STATE_B64',''), os.environ['RESOLVE_RESULT'])
          mutation_missing_started = os.environ['MUTATE_RESULT'] == 'failure'
          write_state('manual-mutation-state.json', 'MUTATE', os.environ.get('MUTATION_STATE_B64',''), os.environ['MUTATE_RESULT'], mutation_missing_started, False)
          post_missing_executed = os.environ['MUTATE_RESULT'] == 'success'
          write_state('manual-post-verify-state.json', 'POST_VERIFY', os.environ.get('POST_STATE_B64',''), os.environ['POST_RESULT'], post_missing_executed, post_missing_executed)
          encoded_verify = os.environ.get('PROMOTION_VERIFY_B64','')
          verify_path = root / 'promotion-verify.json'
          if encoded_verify:
              verify_path.write_bytes(base64.b64decode(encoded_verify, validate=True))
          else:
              verify_path.write_text('{"transport":"unavailable"}\n', encoding='utf-8')
          PY

      - name: Terminalize one lossless manual AR11 OperationalOutcome
        id: outcome
        run: |
          set -euo pipefail
          tree_sha="$(git rev-parse "$SOURCE_SHA^{tree}")"
          evidence_artifact="ar11-operational-outcome-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"
          python scripts/promotion-operational-outcome-ar11.py \
            --mode manual \
            --source-sha "$SOURCE_SHA" --tree-sha "$tree_sha" \
            --environment staging --profile-id rehearsal-core-v2 \
            --resolve-state-json "$RUNNER_TEMP/manual-resolve-state.json" \
            --mutation-state-json "$RUNNER_TEMP/manual-mutation-state.json" \
            --post-verify-state-json "$RUNNER_TEMP/manual-post-verify-state.json" \
            --promotion-verify-json "$RUNNER_TEMP/promotion-verify.json" \
            --evidence-artifact "$evidence_artifact" \
            --output "$RUNNER_TEMP/promotion-operational-outcome.json"
          terminal_dir="$RUNNER_TEMP/ar11-manual-terminal"
          mkdir -p "$terminal_dir"
          cp "$RUNNER_TEMP/promotion-operational-outcome.json" "$RUNNER_TEMP/manual-resolve-state.json" "$RUNNER_TEMP/manual-mutation-state.json" "$RUNNER_TEMP/manual-post-verify-state.json" "$RUNNER_TEMP/promotion-verify.json" "$terminal_dir/"
          status="$(jq -er '.status' "$RUNNER_TEMP/promotion-operational-outcome.json")"
          echo "status=$status" >> "$GITHUB_OUTPUT"
          {
            echo '## Manual AR11 OperationalOutcome'
            echo
            echo '```json'
            jq '{contract,procedure,status,phase,failed_gate,owner,owner_contract,owner_reason_code,recovery_owner,summary,remediation,exact_next_action,source_sha,tree_sha,release_set_id,promotion_id,target_identity,authorization_state,provider_mutation_started,provider_mutation_executed,production_mutation_executed,effect_state,evidence_refs,evidence_digests}' "$RUNNER_TEMP/promotion-operational-outcome.json"
            echo '```'
          } >> "$GITHUB_STEP_SUMMARY"

      - name: Upload terminal manual AR11 OperationalOutcome evidence
        id: outcome_upload
        if: always() && steps.outcome.outcome == 'success'
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a
        with:
          name: ar11-operational-outcome-${{ github.run_id }}-${{ github.run_attempt }}
          path: ${{ runner.temp }}/ar11-manual-terminal
          if-no-files-found: error
          retention-days: 30

      - name: Enforce terminal manual AR11 disposition after evidence publication
        if: always()
        run: |
          set -euo pipefail
          test "${{ steps.outcome.outcome }}" = "success"
          test "${{ steps.outcome_upload.outcome }}" = "success"
          jq -e '.contract == "PROMOTION_OPERATOR_OUTCOME_V1" and .procedure == "AR11_RELEASE_SET_PROMOTION" and .status == "COMPLETED" and .authorization_state == "BOUND_ONE_SHOT" and .provider_mutation_started == true and .provider_mutation_executed == true and .production_mutation_executed == false and .effect_state == "EFFECT_VERIFIED"' "$RUNNER_TEMP/promotion-operational-outcome.json" >/dev/null
'''
    text = replace_once(text, "\n  rollback-negative-evidence:\n", manual_job + "\n  rollback-negative-evidence:\n", "manual terminal job")
    WORKFLOW.write_text(text, encoding="utf-8")


def patch_rust_test() -> None:
    text = RUST_TEST.read_text(encoding="utf-8")
    old = '''    assert!(workflow.contains("Enforce terminal AR11 disposition after evidence publication"));\n\n    let terminalize = workflow\n'''
    new = '''    assert!(workflow.contains("Enforce terminal AR11 disposition after evidence publication"));\n    assert!(workflow.contains("id: node_runtime"));\n    assert!(workflow.contains("NODE_RUNTIME_SETUP"));\n    assert!(workflow.contains("manual-outcome:"));\n    assert!(workflow.contains("AR11_RELEASE_SET_PROMOTION"));\n    assert!(workflow.contains("Mark provider mutation invocation boundary"));\n    assert!(workflow.contains("RECOVERY_REQUIRED"));\n    assert!(!workflow.contains("promotion-verify.json\\\" | jq -e '.verified == true"));\n\n    let terminalize = workflow\n'''
    text = replace_once(text, old, new, "rust workflow assertions")
    old_tail = '''    assert!(\n        terminalize < enforce,\n        "outcome must survive before final assertion"\n    );\n    Ok(())\n}\n'''
    new_tail = '''    assert!(\n        terminalize < enforce,\n        "outcome must survive before final assertion"\n    );\n    let mutation_start = workflow\n        .find("Mark provider mutation invocation boundary")\n        .ok_or_else(|| "provider mutation invocation marker must exist".to_string())?;\n    let deploy = workflow\n        .find("Deploy exact Release Set v3 bits after all fences")\n        .ok_or_else(|| "provider deploy step must exist".to_string())?;\n    let manual_terminalize = workflow\n        .find("Terminalize one lossless manual AR11 OperationalOutcome")\n        .ok_or_else(|| "manual terminal outcome step must exist".to_string())?;\n    let manual_enforce = workflow\n        .find("Enforce terminal manual AR11 disposition after evidence publication")\n        .ok_or_else(|| "manual final enforcement step must exist".to_string())?;\n    assert!(mutation_start < deploy, "effect-start marker must precede deploy invocation");\n    assert!(deploy < manual_terminalize, "manual terminal outcome must observe deploy result");\n    assert!(manual_terminalize < manual_enforce, "manual outcome must be published before final assertion");\n    Ok(())\n}\n'''
    text = replace_once(text, old_tail, new_tail, "rust manual ordering proof")
    RUST_TEST.write_text(text, encoding="utf-8")


def main() -> None:
    patch_projector()
    patch_workflow()
    patch_rust_test()
    SELF.unlink()
    RUNNER.unlink()
    print("O0 manual AR11 patch applied; temporary branch-only patch surfaces removed.")


if __name__ == "__main__":
    main()
