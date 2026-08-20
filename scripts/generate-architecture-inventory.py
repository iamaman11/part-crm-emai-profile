#!/usr/bin/env python3
"""Generate stable architecture inventory data without owning live AR lifecycle state.

Live accepted/current architecture state is derived exclusively from Git by
`.github/scripts/architecture-acceptance.mjs derive` under
`architecture/architecture-acceptance-policy.json`.

`architecture/inventory.json` remains the single architecture inventory. Its old lifecycle snapshot
fields are retained only as transition provenance and are deliberately preserved verbatim by
`--write`; this generator derives repository structure and subject/release projections but never
advances accepted/current/next AR state.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import _architecture_inventory_core as core  # noqa: E402
import _ar3_application_architecture as ar3  # noqa: E402

INVENTORY_PATH = ROOT / "architecture/inventory.json"
LIFECYCLE_POLICY = "architecture/lifecycle-projection-policy.json"
ACCEPTANCE_POLICY = "architecture/architecture-acceptance-policy.json"
D1_EVOLUTION_SOURCE = "architecture/d1-evolution-ar9.json"
RUNTIME_CUTOVER_SOURCE = "architecture/runtime-cutover-ar10.json"
RELEASE_ARCHITECTURE_SOURCE = "architecture/release-architecture-ar11.json"
AR9_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar9-final-acceptance.json"
AR10_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar10-final-acceptance.json"
AR11_ISSUE = 372
SUBJECTS = {
    "credential_authority": "architecture/credential-authority.json",
    "credential_lifecycle": "architecture/credential-lifecycle.json",
    "operator_contract": "architecture/operator-contract.json",
    "profile_security": "architecture/profile-security.json",
}
FORBIDDEN_VALUE_KEYS = {
    "value",
    "secret_value",
    "plaintext",
    "plaintext_value",
    "private_key",
    "password",
    "token_value",
    "credential_value",
    "key_material",
    "raw_secret",
    "raw_token",
}


class InventoryError(ValueError):
    pass


def fail(message: str) -> None:
    raise InventoryError(message)


def load_json(relative: str) -> dict[str, Any]:
    try:
        payload = json.loads((ROOT / relative).read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {relative}: {error}")
    if not isinstance(payload, dict):
        fail(f"{relative} must contain one JSON object")
    return payload


def load_inventory() -> dict[str, Any]:
    try:
        payload = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"architecture inventory is missing or malformed: {INVENTORY_PATH}: {error}") from error
    if not isinstance(payload, dict):
        raise SystemExit("architecture/inventory.json must contain one JSON object")
    return payload


def file_sha256(relative: str) -> str:
    canonical_text = (
        (ROOT / relative)
        .read_text(encoding="utf-8")
        .replace("\r\n", "\n")
        .replace("\r", "\n")
    )
    return hashlib.sha256(canonical_text.encode("utf-8")).hexdigest()


def stable_core_projection() -> dict[str, object]:
    core.validate_route_ownership()
    core.validate_docs()
    routes = [
        {
            "route_class": route_class,
            "capability": capability,
            "methods": methods,
            "path_template": template,
            "example_path": example_path,
            "authenticated": authenticated,
        }
        for route_class, capability, methods, template, example_path, authenticated in core.ROUTE_SPECS
    ]
    return {
        "workspace_members": core.workspace_members(),
        "d1_migrations": {"directory": "migrations/d1", "files": core.migration_files()},
        "routing": {
            "composed_entrypoint": "crates/control-plane-contract/src/lib.rs::classify_route",
            "dynamic_namespaces": ["/api", "/auth", "/bridge"],
            "classifiers": [
                {"capability": capability, "module": path}
                for capability, path in core.CLASSIFIERS
            ],
            "public_routes": routes,
        },
        "generated_contracts": core.GENERATED_CONTRACTS,
        "application_architecture": ar3.build_projection(ROOT),
    }


def find_forbidden_value_keys(value: Any, path: str = "$") -> list[str]:
    errors: list[str] = []
    if isinstance(value, list):
        for index, item in enumerate(value):
            errors.extend(find_forbidden_value_keys(item, f"{path}[{index}]"))
    elif isinstance(value, dict):
        for key, child in value.items():
            if key in FORBIDDEN_VALUE_KEYS:
                errors.append(f"{path}.{key}")
            errors.extend(find_forbidden_value_keys(child, f"{path}.{key}"))
    return errors


def subject_projection() -> dict[str, object]:
    authority = load_json(SUBJECTS["credential_authority"])
    lifecycle = load_json(SUBJECTS["credential_lifecycle"])
    operator = load_json(SUBJECTS["operator_contract"])
    profile = load_json(SUBJECTS["profile_security"])
    if authority.get("kind") != "CURRENT_CREDENTIAL_AUTHORITY" or authority.get("status") != "current":
        fail("current credential authority composition root is invalid")
    if lifecycle.get("kind") != "CREDENTIAL_LIFECYCLE_AUTHORITY" or lifecycle.get("status") != "current":
        fail("current credential lifecycle authority is invalid")
    if operator.get("kind") != "OPERATOR_CONTRACT_AUTHORITY" or operator.get("mode") != "READ_ONLY_METADATA_ONLY":
        fail("current operator authority is invalid")
    if profile.get("kind") != "PROFILE_SECURITY_AUTHORITY" or profile.get("status") != "current":
        fail("current profile security authority is invalid")
    domains = [entry.get("id") for entry in profile.get("security_domains", [])]
    if len(domains) != 6 or any(not isinstance(value, str) for value in domains):
        fail("profile security projection requires exactly six domains")
    forbidden = find_forbidden_value_keys([authority, lifecycle, operator, profile])
    if forbidden:
        fail(f"subject-domain authority contains secret-value fields: {forbidden}")
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
        fail("AR-9 D1 evolution authority identity/version is invalid")
    if authority.get("status") != "accepted":
        fail("AR-9 D1 evolution authority must remain accepted")
    if authority.get("canonical_projection") != "architecture/inventory.json::d1_evolution":
        fail("AR-9 D1 evolution canonical projection drifted")
    if authority.get("production_mutation") is not False:
        fail("AR-9 D1 evolution authority must remain non-production-mutating")
    policy = authority.get("global_policy")
    components = authority.get("components")
    if not isinstance(policy, dict) or not isinstance(components, list) or len(components) != 2:
        fail("AR-9 D1 evolution authority policy/components are malformed")
    projected_components: list[dict[str, object]] = []
    observed_ids: set[str] = set()
    for component in components:
        if not isinstance(component, dict):
            fail("AR-9 D1 component must be an object")
        component_id = component.get("component_id")
        historical = component.get("historical_epoch")
        freeze = historical.get("per_file_sha256_freeze") if isinstance(historical, dict) else None
        if component_id not in {"catalog", "resolver"} or component_id in observed_ids:
            fail("AR-9 D1 component identity set is invalid")
        if not isinstance(historical, dict) or not isinstance(freeze, dict) or freeze.get("status") != "FROZEN":
            fail(f"AR-9 D1 historical epoch is not frozen: {component_id}")
        if historical.get("retroactive_runtime_compatibility_claims") is not False:
            fail(f"AR-9 D1 historical epoch invented compatibility claims: {component_id}")
        observed_ids.add(str(component_id))
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
        fail("AR-9 D1 projection requires exactly Catalog and Resolver")
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
        fail("AR-10 runtime cutover authority identity/version is invalid")
    if authority.get("owning_slice") != "AR-10" or authority.get("owning_issue") != 368:
        fail("AR-10 runtime cutover ownership drifted")
    if authority.get("status") != "accepted":
        fail("AR-10 runtime cutover authority must remain accepted")
    if authority.get("production_mutation") is not False:
        fail("AR-10 runtime cutover must remain non-production-mutating")
    if authority.get("architecture_complete") is not False or authority.get("production_ready") is not False:
        fail("AR-10 runtime cutover may not advance production state")
    if authority.get("production_core_gate") != "BLOCKED":
        fail("AR-10 runtime cutover must keep Production Core blocked")
    real_runtime = authority.get("real_runtime")
    identity = authority.get("generation_identity")
    opsctl = authority.get("opsctl")
    if not isinstance(real_runtime, dict) or not isinstance(identity, dict) or not isinstance(opsctl, dict):
        fail("AR-10 runtime cutover authority is incomplete")
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
            "normal_launch_may_regenerate_browserforge_identity": identity.get("normal_launch_may_regenerate_browserforge_identity"),
            "incompatible_change_requires_candidate_generation": identity.get("incompatible_change_requires_candidate_generation"),
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
        fail("AR-11 release architecture source identity/version is invalid")
    if authority.get("owning_slice") != "AR-11" or authority.get("owning_issue") != AR11_ISSUE:
        fail("AR-11 release architecture ownership drifted")
    if authority.get("canonical_projection") != "architecture/inventory.json::release_architecture":
        fail("AR-11 release architecture canonical projection drifted")
    if authority.get("production_mutation") is not False:
        fail("AR-11 release architecture may not mutate production")
    if authority.get("architecture_complete") is not False or authority.get("production_ready") is not False:
        fail("AR-11 release architecture may not advance production state")
    if authority.get("production_core_gate") != "BLOCKED":
        fail("AR-11 release architecture must keep Production Core blocked")
    units = authority.get("activation_units")
    profiles = authority.get("release_profiles")
    surfaces = authority.get("execution_surfaces")
    closures = authority.get("deployment_closures")
    if not all(isinstance(value, list) for value in (units, profiles, surfaces, closures)):
        fail("AR-11 release architecture collections are malformed")
    if len(units) != 13 or len(profiles) != 5 or len(surfaces) < 20:
        fail("AR-11 release architecture coverage is incomplete")
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


def validate_lifecycle_boundary(inventory: dict[str, Any]) -> None:
    lifecycle = load_json(LIFECYCLE_POLICY)
    acceptance = load_json(ACCEPTANCE_POLICY)
    if lifecycle.get("kind") != "LIFECYCLE_PROJECTION_POLICY" or lifecycle.get("status") != "current":
        fail("lifecycle projection policy identity/status drifted")
    live = lifecycle.get("live_state_authority")
    consumer = lifecycle.get("consumer_policy")
    if not isinstance(live, dict) or not isinstance(consumer, dict):
        fail("lifecycle projection policy is incomplete")
    if (
        live.get("acceptance_policy") != ACCEPTANCE_POLICY
        or live.get("program_sequence") != "architecture/architecture-program-sequence.json"
        or live.get("deriver") != ".github/scripts/architecture-acceptance.mjs derive"
        or live.get("tracked_mutable_lifecycle_state") is not False
    ):
        fail("live lifecycle state is no longer exclusively Git-derived")
    if (
        consumer.get("tracked_snapshot_may_decide_accepted_or_current_slice") is not False
        or consumer.get("tracked_snapshot_may_authorize_production") is not False
        or consumer.get("tracked_snapshot_may_drive_ar12_through_ar17_closeout") is not False
        or consumer.get("stable_inventory_generation_must_preserve_snapshot_without_advancing_it") is not True
        or consumer.get("future_acceptance_requires_source_projection_commit") is not False
    ):
        fail("tracked lifecycle compatibility consumer policy drifted")
    projection_policy = acceptance.get("projection_policy")
    if not isinstance(projection_policy, dict):
        fail("architecture acceptance policy lost projection_policy")
    if (
        projection_policy.get("lifecycle_policy") != LIFECYCLE_POLICY
        or projection_policy.get("tracked_mutable_lifecycle_state_forbidden_as_authority") is not True
        or projection_policy.get("future_acceptance_requires_source_projection_commit") is not False
    ):
        fail("architecture acceptance policy does not bind lifecycle projections to generic authority")
    delivery = inventory.get("current_delivery_map")
    invariants = delivery.get("invariants") if isinstance(delivery, dict) else None
    if not isinstance(invariants, dict):
        fail("inventory compatibility snapshot lost fail-closed current_delivery_map.invariants")
    expected = {
        "source_present_not_equal_production_enabled": True,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }
    for key, wanted in expected.items():
        if invariants.get(key) != wanted:
            fail(f"inventory compatibility invariant drifted: {key}")
    program_state = inventory.get("program_state")
    if not isinstance(program_state, dict):
        fail("inventory lost program_state compatibility section")
    if program_state.get("architecture_complete") is not False:
        fail("inventory program_state may not mark architecture complete")
    if program_state.get("production_core_gate") != "BLOCKED":
        fail("inventory program_state may not authorize Production Core")
    if program_state.get("production_ready") is not False:
        fail("inventory program_state may not mark production ready")
    if program_state.get("production_mutation_allowed_during_ar0_ar17") is not False:
        fail("inventory program_state may not authorize production mutation during AR-0..AR-17")


def build_inventory() -> dict[str, object]:
    # Preserve historical/compatibility sections verbatim. They are not recomputed from accepted/current
    # AR state and therefore cannot become a second lifecycle authority.
    expected: dict[str, object] = copy.deepcopy(load_inventory())
    for key, value in stable_core_projection().items():
        expected[key] = value
    expected["subject_domain_authorities"] = subject_projection()
    expected["d1_evolution"] = d1_evolution_projection()
    expected["runtime_cutover"] = runtime_cutover_projection()
    expected["release_architecture"] = release_architecture_projection()
    validate_lifecycle_boundary(expected)
    return expected


def serialized(payload: object) -> str:
    return json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def check_current(expected: dict[str, object]) -> None:
    try:
        actual = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
    except (FileNotFoundError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"architecture inventory is missing or malformed: {INVENTORY_PATH}: {error}") from error
    if actual != expected:
        raise SystemExit(
            "architecture/inventory.json stable projection is stale; run scripts/generate-architecture-inventory.py --write. "
            "Live accepted/current AR state is Git-derived and is never advanced by this write."
        )


def run(command: list[str]) -> None:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
    if result.returncode != 0:
        details = "\n".join(value.strip() for value in (result.stdout, result.stderr) if value.strip())
        raise SystemExit(details or f"validator failed: {' '.join(command)}")
    if result.stdout.strip():
        print(result.stdout.strip())


def self_test(expected: dict[str, object]) -> None:
    workspace = copy.deepcopy(expected)
    workspace["workspace_members"] = [*workspace["workspace_members"], "crates/does-not-exist"]
    if workspace == expected:
        fail("workspace drift fixture did not mutate inventory")
    d1 = copy.deepcopy(expected)
    d1["d1_evolution"]["components"][0]["history_digest"] = "0" * 64
    if d1 == expected:
        fail("D1 drift fixture did not mutate inventory")
    release = copy.deepcopy(expected)
    release["release_architecture"]["source_sha256"] = "0" * 64
    if release == expected:
        fail("release architecture drift fixture did not mutate inventory")
    blocked = copy.deepcopy(expected)
    blocked["current_delivery_map"]["invariants"]["production_core_gate"] = "AUTHORIZED"
    try:
        validate_lifecycle_boundary(blocked)
    except InventoryError:
        pass
    else:
        fail("premature production authorization fixture unexpectedly passed")
    run([sys.executable, "scripts/generate-ar8-completion-status.py", "--self-test"])
    run([sys.executable, "scripts/check-documentation-authority.py", "--self-test"])
    run(["node", ".github/scripts/architecture-authority-check.mjs", "--self-test"])
    run(["node", ".github/scripts/profile-security-authority-check.mjs", "--self-test"])
    print("Stable inventory and lifecycle non-authority negative matrix passed.")


def credential_self_test() -> None:
    projection = subject_projection()
    sources = projection.get("sources")
    if not isinstance(sources, dict) or set(sources) != set(SUBJECTS):
        fail("subject-domain source registry self-test failed")
    mutated = copy.deepcopy(projection)
    mutated["sources"]["credential_authority"]["sha256"] = "0" * 64
    if mutated == projection:
        fail("subject-domain fingerprint negative fixture did not mutate projection")
    print("Subject-domain credential metadata projection self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    mode.add_argument("--credential-self-test", action="store_true")
    args = parser.parse_args()
    if args.credential_self_test:
        credential_self_test()
        return 0
    expected = build_inventory()
    if args.write:
        INVENTORY_PATH.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print(
            "Wrote stable architecture inventory data while preserving transition-only lifecycle snapshots; "
            "live accepted/current state remains Git-derived."
        )
    elif args.check:
        check_current(expected)
        run([sys.executable, "scripts/generate-ar8-completion-status.py", "--check"])
        run([sys.executable, "scripts/check-documentation-authority.py", "--check"])
        print(
            "Architecture inventory stable data is current; tracked lifecycle snapshots are non-authoritative "
            "transition provenance and production remains fail-closed."
        )
    else:
        self_test(expected)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (InventoryError, KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
