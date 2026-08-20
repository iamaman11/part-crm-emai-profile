#!/usr/bin/env python3
"""Generate canonical inventory after accepted AR-10 and during AR-11."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import credential_authority as credentials

ROOT = Path(__file__).resolve().parents[1]
ENGINE_PATH = ROOT / "scripts/generate-architecture-inventory-engine.py"
INVENTORY_PATH = ROOT / "architecture/inventory.json"
D1_EVOLUTION_SOURCE = "architecture/d1-evolution-ar9.json"
RUNTIME_CUTOVER_SOURCE = "architecture/runtime-cutover-ar10.json"
RELEASE_ARCHITECTURE_SOURCE = "architecture/release-architecture-ar11.json"
AR9_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar9-final-acceptance.json"
AR10_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar10-final-acceptance.json"
AR11_ISSUE = 372
ACCEPTANCE_DERIVER = ".github/scripts/architecture-acceptance.mjs"
PROGRAM_SEQUENCE = "architecture/architecture-program-sequence.json"
LIFECYCLE_PROJECTION_POLICY = "architecture/lifecycle-projection-policy.json"
SUBJECTS = {
    "credential_authority": "architecture/credential-authority.json",
    "credential_lifecycle": "architecture/credential-lifecycle.json",
    "operator_contract": "architecture/operator-contract.json",
    "profile_security": "architecture/profile-security.json",
}

spec = importlib.util.spec_from_file_location("architecture_inventory_engine", ENGINE_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load architecture inventory engine")
engine = importlib.util.module_from_spec(spec)
spec.loader.exec_module(engine)
legacy_delivery_map = engine.current_delivery_map
legacy_progress = engine.expected_ar8_progress
engine.CURRENT_SLICE = "AR-11"
engine.NEXT_SLICE = "AR-12"
engine.CURRENT_DELIVERY_CHECKPOINT = "AR-10"
engine.AR8_ACCEPTED_SUBSLICES = ["AR-8A", "AR-8B", "AR-8C", "AR-8D", "AR-8E", "AR-8F"]
engine.AR8_CURRENT_SUBSLICE = None
engine.AR8D_IMPLEMENTATION_ISSUE = None
engine.AR8_MANDATORY_REMAINING = []
engine.AR8_IMPLEMENTATION_ENTRY_GATE = "AR8_ACCEPTED_MAIN_AR9_CURRENT"
for accepted in ("AR-8", "AR-9", "AR-10"):
    if accepted not in engine.ACCEPTED_SLICES:
        engine.ACCEPTED_SLICES = [*engine.ACCEPTED_SLICES, accepted]


def current_credential_authority_source() -> tuple[dict[str, Any], dict[str, set[str]]]:
    state = credentials.validate_repository(ROOT)
    return state.registry, state.detected


def validate_current_credential_registry(
    payload: dict[str, Any], detected: dict[str, set[str]]
) -> None:
    lifecycle = credentials.read_json(ROOT, credentials.EXPECTED_LIFECYCLE)
    credentials.validate_registry(payload, detected, lifecycle)


def normalized_detected(value: dict[str, set[str]]) -> dict[str, tuple[str, ...]]:
    return {name: tuple(sorted(paths)) for name, paths in sorted(value.items())}


def current_credential_negative_self_test(
    payload: dict[str, Any], detected: dict[str, set[str]]
) -> None:
    state = credentials.validate_repository(ROOT)
    if payload != state.registry:
        raise ValueError("credential negative-self-test payload diverged from current authority")
    if normalized_detected(detected) != normalized_detected(state.detected):
        raise ValueError("credential negative-self-test bindings diverged from current authority")
    credentials.negative_self_test(state, ROOT)


def print_current_credential_check_summary(
    detected: dict[str, set[str]], *, self_tested: bool
) -> None:
    state = credentials.validate_repository(ROOT)
    if normalized_detected(detected) != normalized_detected(state.detected):
        raise ValueError("credential summary bindings diverged from current authority")
    suffix = " and fail-closed negative fixtures" if self_tested else ""
    print(
        f"Current credential authority covers {len(detected)} tracked static bindings{suffix}; "
        "historical credential validator execution is not required."
    )


engine.validate_credential_authority_source = current_credential_authority_source
engine.validate_credential_authority = validate_current_credential_registry
engine.credential_negative_self_test = current_credential_negative_self_test
engine.print_credential_check_summary = print_current_credential_check_summary


def completion_delivery_map() -> dict[str, object]:
    value = legacy_delivery_map()
    value["accepted_checkpoint"] = "AR-10"
    value["current_work"] = "AR-11"
    value["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-10",
        "current_subslice": "AR-11",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR10_CLOSEOUT",
    }
    value["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-10",
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "ar10_accepted": True,
        "acceptance_evidence": AR10_ACCEPTANCE_EVIDENCE,
    }
    value["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    value["next_gate"] = {
        "id": "AR-11_ACCEPTANCE",
        "issue": AR11_ISSUE,
        "on_success": "AR-12_BECOMES_CURRENT",
    }
    value["invariants"].update(
        {
            "source_present_not_equal_production_enabled": True,
            "full_ar8_accepted": True,
            "ar9_accepted": True,
            "ar9_blocked": False,
            "ar10_accepted": True,
            "ar10_blocked": False,
            "ar11_blocked": False,
            "architecture_complete": False,
            "production_core_gate": "BLOCKED",
            "production_ready": False,
            "production_mutation": False,
        }
    )
    return value


def completion_progress() -> dict[str, object]:
    value = legacy_progress()
    value.update(
        {
            "accepted_subslices": ["AR-8A", "AR-8B", "AR-8C", "AR-8D", "AR-8E", "AR-8F"],
            "current_subslice": None,
            "current_implementation_issue": None,
            "mandatory_remaining": [],
            "implementation_entry_gate": "AR8_ACCEPTED_MAIN_AR9_CURRENT",
            "full_ar8_accepted": True,
            "ar9_blocked": False,
            "production_mutation": False,
            "source_complete_candidate": False,
            "completion_pr": 362,
            "implemented_through": "AR-8F",
            "accepted_top_level_slice": "AR-8",
            "exact_green_head": "81d1f0c26ff0bd3a688c2d5dc000b93640479e47",
            "implementation_merge": "874666f6ef6eb003425c9677d558378d6dc0daaf",
            "applicable_permanent_workflows": "14/14",
            "accepted_main_reread": "874666f6ef6eb003425c9677d558378d6dc0daaf",
            "acceptance_evidence": "docs/evidence/2026-08-18-ar8-final-acceptance.json",
        }
    )
    return value


engine.current_delivery_map = completion_delivery_map
engine.expected_ar8_progress = completion_progress


def load_json(relative: str) -> dict[str, Any]:
    payload = json.loads((ROOT / relative).read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{relative} must contain one JSON object")
    return payload


def validate_derived_lifecycle_state(value: dict[str, Any]) -> dict[str, Any]:
    if value.get("schema_version") != 1:
        raise ValueError("Git-derived lifecycle state schema_version must be 1")
    accepted = value.get("accepted_checkpoint")
    current = value.get("current_slice")
    if not isinstance(accepted, str) or not accepted:
        raise ValueError("Git-derived lifecycle state is missing accepted_checkpoint")
    for field in ("architecture_complete", "production_ready", "production_mutation"):
        if not isinstance(value.get(field), bool):
            raise ValueError(f"Git-derived lifecycle state field {field} must be boolean")
    if value.get("production_core_gate") not in {"BLOCKED", "AUTHORIZED"}:
        raise ValueError("Git-derived lifecycle state has invalid production_core_gate")

    sequence = load_json(PROGRAM_SEQUENCE)
    if (
        sequence.get("schema_version") != 1
        or sequence.get("kind") != "ARCHITECTURE_PROGRAM_SEQUENCE"
        or sequence.get("state_model") != "STATIC_ORDER_ONLY"
        or sequence.get("mutable_lifecycle_state_forbidden") is not True
    ):
        raise ValueError("static architecture program sequence boundary drifted")
    slices = sequence.get("slices")
    if not isinstance(slices, list) or any(not isinstance(item, dict) for item in slices):
        raise ValueError("static architecture program sequence is malformed")
    accepted_entry = next((item for item in slices if item.get("id") == accepted), None)
    if accepted_entry is None:
        raise ValueError(f"Git-derived accepted checkpoint is absent from static sequence: {accepted}")
    expected_current = accepted_entry.get("successor")
    if current != expected_current:
        raise ValueError(
            f"Git-derived current slice does not match static successor: {current!r} != {expected_current!r}"
        )

    policy = load_json(LIFECYCLE_PROJECTION_POLICY)
    authority = policy.get("live_state_authority")
    consumers = policy.get("consumer_policy")
    if (
        policy.get("schema_version") != 1
        or policy.get("kind") != "LIFECYCLE_PROJECTION_POLICY"
        or policy.get("status") != "current"
        or not isinstance(authority, dict)
        or authority.get("deriver") != f"{ACCEPTANCE_DERIVER} derive"
        or authority.get("tracked_mutable_lifecycle_state") is not False
        or authority.get("manual_current_slice_authority") is not False
        or authority.get("manual_accepted_checkpoint_authority") is not False
        or not isinstance(consumers, dict)
        or consumers.get("tracked_snapshot_may_decide_accepted_or_current_slice") is not False
        or consumers.get("duplicate_lifecycle_derivation_algorithm_forbidden") is not True
    ):
        raise ValueError("lifecycle projection read-side authority policy drifted")
    return value


def derive_lifecycle_state() -> dict[str, Any]:
    result = subprocess.run(
        ["node", ACCEPTANCE_DERIVER, "derive"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        details = "\n".join(
            part.strip() for part in (result.stdout, result.stderr) if part.strip()
        )
        raise ValueError(details or "canonical Git-derived lifecycle command failed")
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ValueError("canonical Git-derived lifecycle command returned malformed JSON") from error
    if not isinstance(payload, dict):
        raise ValueError("canonical Git-derived lifecycle command must return one JSON object")
    return validate_derived_lifecycle_state(payload)


def validate_current_source_documents() -> None:
    for item in engine.DOCUMENT_STATUS:
        path = item.get("path") if isinstance(item, dict) else None
        if not isinstance(path, str) or not (ROOT / path).is_file():
            raise ValueError(f"document-status inventory path missing: {path!r}")

    # Lifecycle state is read only from the generic Git-derived acceptance authority.
    # Tracked status/inventory/transition fields remain compatibility projections and
    # are deliberately not consumed here to decide accepted/current architecture state.
    derive_lifecycle_state()

    runtime_gate = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-cloudflare-runtime-bindings.py")],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if runtime_gate.returncode != 0:
        details = "\n".join(
            part.strip() for part in (runtime_gate.stdout, runtime_gate.stderr) if part.strip()
        )
        raise ValueError(f"runtime authority gate failed:\n{details}")


engine.validate_source_documents = validate_current_source_documents


def file_sha256(relative: str) -> str:
    canonical_text = (
        (ROOT / relative)
        .read_text(encoding="utf-8")
        .replace("\r\n", "\n")
        .replace("\r", "\n")
    )
    return hashlib.sha256(canonical_text.encode("utf-8")).hexdigest()


def subject_projection() -> dict[str, object]:
    authority = load_json(SUBJECTS["credential_authority"])
    lifecycle = load_json(SUBJECTS["credential_lifecycle"])
    operator = load_json(SUBJECTS["operator_contract"])
    profile = load_json(SUBJECTS["profile_security"])
    if authority.get("kind") != "CURRENT_CREDENTIAL_AUTHORITY" or authority.get("status") != "current":
        raise ValueError("current credential authority composition root is invalid")
    if lifecycle.get("kind") != "CREDENTIAL_LIFECYCLE_AUTHORITY" or lifecycle.get("status") != "current":
        raise ValueError("current credential lifecycle authority is invalid")
    if operator.get("kind") != "OPERATOR_CONTRACT_AUTHORITY" or operator.get("mode") != "READ_ONLY_METADATA_ONLY":
        raise ValueError("current operator authority is invalid")
    if profile.get("kind") != "PROFILE_SECURITY_AUTHORITY" or profile.get("status") != "current":
        raise ValueError("current profile security authority is invalid")
    domains = [entry.get("id") for entry in profile.get("security_domains", [])]
    if len(domains) != 6 or any(not isinstance(value, str) for value in domains):
        raise ValueError("profile security projection requires exactly six domains")
    return {
        "schema_version": 1,
        "role": "CURRENT_SUBJECT_DOMAIN_PROJECTION",
        "composition_root": SUBJECTS["credential_authority"],
        "registry_provenance": "architecture/credential-authority-ar8b.json",
        "sources": {
            name: {"path": path, "sha256": file_sha256(path)}
            for name, path in SUBJECTS.items()
        },
        "credential_lifecycle_concern_ids": [entry.get("id") for entry in lifecycle.get("concerns", [])],
        "profile_security_domain_ids": domains,
        "operator_mode": operator.get("mode"),
        "completion_provenance": {
            "lifecycle_candidate": "docs/evidence/ar8-completion-lifecycle-candidate.json",
            "operator_rehearsal_candidate": "docs/evidence/ar8-operator-rehearsal-candidate.json",
            "secret_transport_successor_candidate": "docs/evidence/ar8-d-secret-transport-successor-candidate.json",
        },
        "source_completion": {
            "tracking_issue": 361,
            "umbrella_issue": 308,
            "completion_pr": 362,
            "implemented_through": "AR-8F",
            "state": "ACCEPTED_MAIN",
            "accepted_main_through": "AR-8",
            "full_ar8_accepted": True,
            "ar9_blocked": False,
            "exact_green_head": "81d1f0c26ff0bd3a688c2d5dc000b93640479e47",
            "implementation_merge": "874666f6ef6eb003425c9677d558378d6dc0daaf",
            "acceptance_evidence": "docs/evidence/2026-08-18-ar8-final-acceptance.json",
            "production_mutation": False,
        },
    }


def d1_evolution_projection() -> dict[str, object]:
    authority = load_json(D1_EVOLUTION_SOURCE)
    if authority.get("kind") != "D1_EVOLUTION_AUTHORITY" or authority.get("schema_version") != 1:
        raise ValueError("AR-9 D1 evolution authority identity/version is invalid")
    if authority.get("status") != "accepted":
        raise ValueError("AR-9 D1 evolution authority must project accepted status after accepted main")
    if authority.get("canonical_projection") != "architecture/inventory.json::d1_evolution":
        raise ValueError("AR-9 D1 evolution authority canonical projection drifted")
    if authority.get("production_mutation") is not False:
        raise ValueError("AR-9 D1 evolution authority must remain non-production-mutating")
    policy = authority.get("global_policy")
    components = authority.get("components")
    if not isinstance(policy, dict) or not isinstance(components, list) or len(components) != 2:
        raise ValueError("AR-9 D1 evolution authority policy/components are malformed")

    projected_components: list[dict[str, object]] = []
    observed_ids: set[str] = set()
    for component in components:
        if not isinstance(component, dict):
            raise ValueError("AR-9 D1 component must be an object")
        component_id = component.get("component_id")
        historical = component.get("historical_epoch")
        freeze = historical.get("per_file_sha256_freeze") if isinstance(historical, dict) else None
        if component_id not in {"catalog", "resolver"} or component_id in observed_ids:
            raise ValueError("AR-9 D1 component identity set is invalid")
        if not isinstance(historical, dict) or not isinstance(freeze, dict) or freeze.get("status") != "FROZEN":
            raise ValueError(f"AR-9 D1 historical epoch is not frozen: {component_id}")
        if historical.get("retroactive_runtime_compatibility_claims") is not False:
            raise ValueError(f"AR-9 D1 historical epoch invented compatibility claims: {component_id}")
        observed_ids.add(component_id)
        projected_components.append(
            {
                "component_id": component_id,
                "binding_identity": component.get("binding_identity"),
                "migration_root": component.get("migration_root"),
                "migration_ledger": component.get("migration_ledger"),
                "current_repository_revision": component.get("current_repository_revision"),
                "history_digest": component.get("history_digest"),
                "historical_epoch": {
                    "status": historical.get("status"),
                    "ordered_set_identity": historical.get("ordered_set_identity"),
                    "per_file_sha256_freeze": freeze,
                    "retroactive_runtime_compatibility_claims": False,
                },
                "post_epoch_migration_count": len(component.get("post_epoch_migrations", [])),
                "fresh_bootstrap_authority": component.get("fresh_bootstrap_authority"),
                "upgrade_authority": component.get("upgrade_authority"),
                "mutation_authority": component.get("mutation_authority"),
                "concurrency_authority": component.get("concurrency_authority"),
                "release_manifest_owner": component.get("release_manifest_owner"),
            }
        )
    if observed_ids != {"catalog", "resolver"}:
        raise ValueError("AR-9 D1 projection requires exactly Catalog and Resolver")

    return {
        "schema_version": 1,
        "role": "ACCEPTED_AR9_D1_EVOLUTION_PROJECTION",
        "source_authority": D1_EVOLUTION_SOURCE,
        "source_status": authority.get("status"),
        "acceptance_evidence": AR9_ACCEPTANCE_EVIDENCE,
        "tracking_issue": authority.get("tracking_issue"),
        "start_base": authority.get("start_base"),
        "migration_classes": policy.get("migration_classes"),
        "ledger_states": policy.get("ledger_states"),
        "rollout_decisions": policy.get("rollout_decisions"),
        "mutation_authority": policy.get("mutation_authority"),
        "policy_authority": policy.get("policy_authority"),
        "new_opsctl_process_spawn_sites": policy.get("new_opsctl_process_spawn_sites"),
        "opsctl_provider_credentials": policy.get("opsctl_provider_credentials"),
        "resource_auto_provisioning_allowed": policy.get("resource_auto_provisioning_allowed"),
        "database_lock_required_by_default": policy.get("database_lock_required_by_default"),
        "components": projected_components,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }


def runtime_cutover_projection() -> dict[str, object]:
    authority = load_json(RUNTIME_CUTOVER_SOURCE)
    if authority.get("kind") != "RUNTIME_CUTOVER_AUTHORITY" or authority.get("schema_version") != 1:
        raise ValueError("AR-10 runtime cutover authority identity/version is invalid")
    if authority.get("owning_slice") != "AR-10" or authority.get("owning_issue") != 368:
        raise ValueError("AR-10 runtime cutover ownership drifted")
    if authority.get("status") != "accepted":
        raise ValueError("AR-10 runtime cutover authority must be accepted after accepted main")
    if authority.get("production_mutation") is not False:
        raise ValueError("AR-10 runtime cutover must remain non-production-mutating")
    if authority.get("architecture_complete") is not False or authority.get("production_ready") is not False:
        raise ValueError("AR-10 runtime cutover may not advance production architecture state")
    if authority.get("production_core_gate") != "BLOCKED":
        raise ValueError("AR-10 runtime cutover must keep Production Core blocked")
    real_runtime = authority.get("real_runtime")
    identity = authority.get("generation_identity")
    opsctl = authority.get("opsctl")
    if not isinstance(real_runtime, dict) or not isinstance(identity, dict) or not isinstance(opsctl, dict):
        raise ValueError("AR-10 runtime cutover authority is incomplete")
    return {
        "schema_version": 1,
        "role": "ACCEPTED_AR10_RUNTIME_CUTOVER_PROJECTION",
        "acceptance_evidence": AR10_ACCEPTANCE_EVIDENCE,
        "source_authority": RUNTIME_CUTOVER_SOURCE,
        "source_status": authority.get("status"),
        "tracking_issue": authority.get("owning_issue"),
        "completion_pr": authority.get("completion_pr"),
        "start_base": authority.get("exact_start_base"),
        "real_runtime": {
            "repository_integrated": real_runtime.get("repository_integrated"),
            "production_certified": real_runtime.get("production_certified"),
            "entrypoint": real_runtime.get("entrypoint"),
            "runtime_lock": real_runtime.get("runtime_lock"),
            "python": real_runtime.get("python"),
            "camoufox_python": real_runtime.get("camoufox_python"),
            "camoufox_browser": real_runtime.get("camoufox_browser"),
            "browserforge": real_runtime.get("browserforge"),
            "playwright": real_runtime.get("playwright"),
            "persistent_context_required": real_runtime.get("persistent_context_required"),
            "stable_generation_user_data_dir": real_runtime.get("stable_generation_user_data_dir"),
        },
        "generation_identity": {
            "compatibility_version": identity.get("compatibility_version"),
            "fingerprint_policy_version": identity.get("fingerprint_policy_version"),
            "normal_launch_may_regenerate_browserforge_identity": identity.get(
                "normal_launch_may_regenerate_browserforge_identity"
            ),
            "incompatible_change_requires_candidate_generation": identity.get(
                "incompatible_change_requires_candidate_generation"
            ),
        },
        "opsctl": {
            "production_child_process_spawn_sites": opsctl.get("production_child_process_spawn_sites"),
            "provider_mutation_authority": opsctl.get("provider_mutation_authority"),
        },
        "legacy_executables_remaining": authority.get("legacy_executables_remaining"),
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }


def release_architecture_projection() -> dict[str, object]:
    authority = load_json(RELEASE_ARCHITECTURE_SOURCE)
    if authority.get("schema_version") != 1 or authority.get("kind") != "AR11_RELEASE_ARCHITECTURE_SOURCE":
        raise ValueError("AR-11 release architecture source identity/version is invalid")
    if authority.get("owning_slice") != "AR-11" or authority.get("owning_issue") != AR11_ISSUE:
        raise ValueError("AR-11 release architecture ownership drifted")
    if authority.get("canonical_projection") != "architecture/inventory.json::release_architecture":
        raise ValueError("AR-11 release architecture canonical projection drifted")
    if authority.get("production_mutation") is not False:
        raise ValueError("AR-11 release architecture may not mutate production")
    if authority.get("architecture_complete") is not False or authority.get("production_ready") is not False:
        raise ValueError("AR-11 release architecture may not advance production state")
    if authority.get("production_core_gate") != "BLOCKED":
        raise ValueError("AR-11 release architecture must keep Production Core blocked")

    units = authority.get("activation_units")
    profiles = authority.get("release_profiles")
    surfaces = authority.get("execution_surfaces")
    closures = authority.get("deployment_closures")
    if not all(isinstance(value, list) for value in (units, profiles, surfaces, closures)):
        raise ValueError("AR-11 release architecture collections are malformed")
    if len(units) != 13 or len(profiles) != 5 or len(surfaces) < 20:
        raise ValueError("AR-11 release architecture coverage is incomplete")

    return {
        "schema_version": 1,
        "role": "CURRENT_AR11_RELEASE_ARCHITECTURE_PROJECTION",
        "source_authority": RELEASE_ARCHITECTURE_SOURCE,
        "source_sha256": file_sha256(RELEASE_ARCHITECTURE_SOURCE),
        "source_status": authority.get("status"),
        "tracking_issue": authority.get("owning_issue"),
        "principles": authority.get("principles"),
        "activation_units": units,
        "release_profiles": profiles,
        "execution_surfaces": surfaces,
        "deployment_closures": closures,
        "component_release_owners": authority.get("component_release_owners"),
        "promotion_policy": authority.get("promotion_policy"),
        "compatibility_dimensions": authority.get("compatibility_dimensions"),
        "artifact_authority": authority.get("artifact_authority"),
        "effective_state_model": authority.get("effective_state_model"),
        "release_set": authority.get("release_set"),
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }


def build_inventory() -> dict[str, object]:
    expected = engine.build_inventory()
    expected["subject_domain_authorities"] = subject_projection()
    expected["d1_evolution"] = d1_evolution_projection()
    expected["runtime_cutover"] = runtime_cutover_projection()
    expected["release_architecture"] = release_architecture_projection()
    documentation = expected.setdefault("documentation_authority", {})
    documentation["current_credential_authority"] = SUBJECTS["credential_authority"]
    documentation["credential_registry_provenance"] = "architecture/credential-authority-ar8b.json"
    documentation["credential_lifecycle"] = SUBJECTS["credential_lifecycle"]
    documentation["operator_contract"] = SUBJECTS["operator_contract"]
    documentation["profile_security"] = SUBJECTS["profile_security"]
    documentation["ar8_completion_tracking_issue"] = 361
    documentation["ar8_completion_pr"] = 362
    documentation["ar8_acceptance_evidence"] = "docs/evidence/2026-08-18-ar8-final-acceptance.json"
    documentation["ar9_acceptance_evidence"] = AR9_ACCEPTANCE_EVIDENCE
    documentation["ar10_acceptance_evidence"] = AR10_ACCEPTANCE_EVIDENCE
    documentation["d1_evolution"] = "architecture/inventory.json::d1_evolution"
    documentation["d1_evolution_source"] = D1_EVOLUTION_SOURCE
    documentation["runtime_cutover"] = "architecture/inventory.json::runtime_cutover"
    documentation["runtime_cutover_source"] = RUNTIME_CUTOVER_SOURCE
    documentation["release_architecture"] = "architecture/inventory.json::release_architecture"
    documentation["release_architecture_source"] = RELEASE_ARCHITECTURE_SOURCE
    return expected


def serialized(payload: object) -> str:
    return json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def check_current(expected: dict[str, object]) -> None:
    try:
        actual = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
    except (FileNotFoundError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SystemExit(
            f"architecture inventory is missing or malformed: {INVENTORY_PATH}: {error}"
        ) from error
    if actual != expected:
        raise SystemExit(
            "architecture/inventory.json is stale; run scripts/generate-architecture-inventory.py --write"
        )


def run(command: list[str]) -> None:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
    if result.returncode != 0:
        details = "\n".join(
            value.strip() for value in (result.stdout, result.stderr) if value.strip()
        )
        raise SystemExit(details or f"validator failed: {' '.join(command)}")
    if result.stdout.strip():
        print(result.stdout.strip())


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    mode.add_argument("--credential-self-test", action="store_true")
    args = parser.parse_args()
    if args.credential_self_test:
        state = credentials.validate_repository(ROOT)
        current_credential_negative_self_test(state.registry, state.detected)
        print_current_credential_check_summary(state.detected, self_tested=True)
        return 0
    expected = build_inventory()
    if args.write:
        INVENTORY_PATH.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print(
            "Wrote architecture/inventory.json with accepted AR-10 runtime cutover and current AR-11 release architecture projection."
        )
    elif args.check:
        check_current(expected)
        engine.validate_full_documentation_authority()
        run([sys.executable, "scripts/generate-ar8-completion-status.py", "--check"])
        print(
            "Architecture inventory projects accepted AR-10 and current AR-11 release architecture while production remains blocked."
        )
    else:
        if engine.validate_credential_authority_source is not current_credential_authority_source:
            raise ValueError("current inventory credential source did not cut over to neutral authority")
        if engine.validate_credential_authority is not validate_current_credential_registry:
            raise ValueError("current inventory credential validation did not cut over to neutral authority")
        if engine.credential_negative_self_test is not current_credential_negative_self_test:
            raise ValueError("current inventory credential negative fixtures did not cut over to neutral authority")
        if engine.print_credential_check_summary is not print_current_credential_check_summary:
            raise ValueError("current inventory credential summary did not cut over to neutral authority")
        if engine.validate_source_documents is not validate_current_source_documents:
            raise ValueError("current inventory lifecycle reads did not cut over to Git-derived authority")
        derived = derive_lifecycle_state()
        malformed = json.loads(serialized(derived))
        malformed["current_slice"] = malformed["accepted_checkpoint"]
        try:
            validate_derived_lifecycle_state(malformed)
        except ValueError:
            pass
        else:
            raise ValueError("Git-derived lifecycle successor mismatch negative fixture unexpectedly passed")
        engine.self_test(expected)
        run([sys.executable, "scripts/credential_authority.py", "--self-test"])
        mutated = json.loads(serialized(expected))
        mutated.pop("runtime_cutover", None)
        if mutated == expected:
            raise ValueError("AR-10 runtime cutover projection negative fixture did not mutate inventory")
        if mutated == build_inventory():
            raise ValueError(
                "missing AR-10 runtime cutover projection negative fixture unexpectedly matched"
            )
        release_mutated = json.loads(serialized(expected))
        release_mutated.pop("release_architecture", None)
        if release_mutated == expected or release_mutated == build_inventory():
            raise ValueError(
                "missing AR-11 release architecture projection negative fixture unexpectedly matched"
            )
        run([sys.executable, "scripts/generate-ar8-completion-status.py", "--self-test"])
        run(["node", ACCEPTANCE_DERIVER, "self-test"])
        run(["node", ".github/scripts/architecture-authority-check.mjs", "--self-test"])
        run(["node", ".github/scripts/profile-security-authority-check.mjs", "--self-test"])
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
