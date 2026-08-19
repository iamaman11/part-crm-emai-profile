#!/usr/bin/env python3
"""Generate canonical inventory after accepted AR-8 and during AR-9."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
ENGINE_PATH = ROOT / "scripts/generate-architecture-inventory-engine.py"
INVENTORY_PATH = ROOT / "architecture/inventory.json"
D1_EVOLUTION_SOURCE = "architecture/d1-evolution-ar9.json"
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
engine.CURRENT_SLICE = "AR-9"
engine.NEXT_SLICE = "AR-10"
engine.CURRENT_DELIVERY_CHECKPOINT = "AR-8"
engine.AR8_ACCEPTED_SUBSLICES = ["AR-8A", "AR-8B", "AR-8C", "AR-8D", "AR-8E", "AR-8F"]
engine.AR8_CURRENT_SUBSLICE = None
engine.AR8D_IMPLEMENTATION_ISSUE = None
engine.AR8_MANDATORY_REMAINING = []
engine.AR8_IMPLEMENTATION_ENTRY_GATE = "AR8_ACCEPTED_MAIN_AR9_CURRENT"
if "AR-8" not in engine.ACCEPTED_SLICES:
    engine.ACCEPTED_SLICES = [*engine.ACCEPTED_SLICES, "AR-8"]


def completion_delivery_map() -> dict[str, object]:
    value = legacy_delivery_map()
    value["accepted_checkpoint"] = "AR-8"
    value["current_work"] = "AR-9"
    value["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-8",
        "current_subslice": "AR-9",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR8_CLOSEOUT",
    }
    value["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-8",
        "full_ar8_accepted": True,
        "acceptance_evidence": "docs/evidence/2026-08-18-ar8-final-acceptance.json",
    }
    value["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    value["next_gate"] = {"id": "AR-9_ACCEPTANCE", "issue": 366, "on_success": "AR-10_BECOMES_CURRENT"}
    value["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": True,
        "ar9_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return value


def completion_progress() -> dict[str, object]:
    value = legacy_progress()
    value.update({
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
    })
    return value


engine.current_delivery_map = completion_delivery_map
engine.expected_ar8_progress = completion_progress


def load_json(relative: str) -> dict[str, Any]:
    payload = json.loads((ROOT / relative).read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{relative} must contain one JSON object")
    return payload


def file_sha256(relative: str) -> str:
    canonical_text = (ROOT / relative).read_text(encoding="utf-8").replace("\r\n", "\n").replace("\r", "\n")
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
        "sources": {name: {"path": path, "sha256": file_sha256(path)} for name, path in SUBJECTS.items()},
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
        projected_components.append({
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
        })
    if observed_ids != {"catalog", "resolver"}:
        raise ValueError("AR-9 D1 projection requires exactly Catalog and Resolver")

    return {
        "schema_version": 1,
        "role": "CURRENT_AR9_D1_EVOLUTION_PROJECTION",
        "source_authority": D1_EVOLUTION_SOURCE,
        "source_status": authority.get("status"),
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


def build_inventory() -> dict[str, object]:
    expected = engine.build_inventory()
    expected["subject_domain_authorities"] = subject_projection()
    expected["d1_evolution"] = d1_evolution_projection()
    documentation = expected.setdefault("documentation_authority", {})
    documentation["current_credential_authority"] = SUBJECTS["credential_authority"]
    documentation["credential_registry_provenance"] = "architecture/credential-authority-ar8b.json"
    documentation["credential_lifecycle"] = SUBJECTS["credential_lifecycle"]
    documentation["operator_contract"] = SUBJECTS["operator_contract"]
    documentation["profile_security"] = SUBJECTS["profile_security"]
    documentation["ar8_completion_tracking_issue"] = 361
    documentation["ar8_completion_pr"] = 362
    documentation["ar8_acceptance_evidence"] = "docs/evidence/2026-08-18-ar8-final-acceptance.json"
    documentation["d1_evolution"] = "architecture/inventory.json::d1_evolution"
    documentation["d1_evolution_source"] = D1_EVOLUTION_SOURCE
    return expected


def serialized(payload: object) -> str:
    return json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def check_current(expected: dict[str, object]) -> None:
    try:
        actual = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
    except (FileNotFoundError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"architecture inventory is missing or malformed: {INVENTORY_PATH}: {error}") from error
    if actual != expected:
        raise SystemExit("architecture/inventory.json is stale; run scripts/generate-architecture-inventory.py --write")


def run(command: list[str]) -> None:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
    if result.returncode != 0:
        details = "\n".join(value.strip() for value in (result.stdout, result.stderr) if value.strip())
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
        payload, detected = engine.validate_credential_authority_source()
        engine.credential_negative_self_test(payload, detected)
        engine.print_credential_check_summary(detected, self_tested=True)
        return 0
    expected = build_inventory()
    if args.write:
        INVENTORY_PATH.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print("Wrote architecture/inventory.json with accepted AR-8 and current AR-9 D1 evolution projection.")
    elif args.check:
        check_current(expected)
        engine.validate_full_documentation_authority()
        run([sys.executable, "scripts/generate-ar8-completion-status.py", "--check"])
        print("Architecture inventory projects accepted AR-8 subject-domain authorities and current AR-9 D1 evolution while production remains blocked.")
    else:
        engine.self_test(expected)
        mutated = json.loads(serialized(expected))
        mutated.pop("d1_evolution", None)
        if mutated == expected:
            raise ValueError("D1 evolution projection negative fixture did not mutate inventory")
        try:
            if mutated == build_inventory():
                raise ValueError("missing D1 evolution projection negative fixture unexpectedly matched")
        except ValueError:
            raise
        run([sys.executable, "scripts/generate-ar8-completion-status.py", "--self-test"])
        run(["node", ".github/scripts/architecture-authority-check.mjs", "--self-test"])
        run(["node", ".github/scripts/profile-security-authority-check.mjs", "--self-test"])
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
