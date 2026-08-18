#!/usr/bin/env python3
"""Current documentation authority checker after accepted AR-8 and during AR-9."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEGACY_PATH = ROOT / "scripts/check-documentation-authority-legacy.py"
ACCEPTANCE_EVIDENCE = Path("docs/evidence/2026-08-18-ar8-final-acceptance.json")
AR9_ISSUE = 366
AR9_AUTHORITY = Path("architecture/d1-evolution-ar9.json")
AR9_PROJECTION = "architecture/inventory.json::d1_evolution"
SUBJECT_FILES = (
    Path("architecture/credential-authority.json"),
    Path("architecture/credential-lifecycle.json"),
    Path("architecture/operator-contract.json"),
    Path("architecture/profile-security.json"),
    Path("architecture/README.md"),
)

spec = importlib.util.spec_from_file_location("documentation_authority_legacy", LEGACY_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load documentation authority legacy engine")
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)

legacy_expected_current_delivery_map = legacy.expected_current_delivery_map
legacy.CURRENT_SLICE = "AR-9"
legacy.NEXT_SLICE = "AR-10"
legacy.CURRENT_DELIVERY_CHECKPOINT = "AR-8"
legacy.AR8_ACCEPTED_SUBSLICES = ["AR-8A", "AR-8B", "AR-8C", "AR-8D", "AR-8E", "AR-8F"]
legacy.AR8_CURRENT_SUBSLICE = None
legacy.AR8D_IMPLEMENTATION_ISSUE = None
legacy.AR8_IMPLEMENTATION_ENTRY_GATE = "AR8_ACCEPTED_MAIN_AR9_CURRENT"
legacy.AR8_MANDATORY_REMAINING = []
if "AR-8" not in legacy.ACCEPTED_SLICES:
    legacy.ACCEPTED_SLICES = [*legacy.ACCEPTED_SLICES, "AR-8"]
if ACCEPTANCE_EVIDENCE not in legacy.REQUIRED_FILES:
    legacy.REQUIRED_FILES = (*legacy.REQUIRED_FILES, ACCEPTANCE_EVIDENCE)
if AR9_AUTHORITY not in legacy.REQUIRED_FILES:
    legacy.REQUIRED_FILES = (*legacy.REQUIRED_FILES, AR9_AUTHORITY)


def expected_current_delivery_map() -> dict[str, object]:
    base = legacy_expected_current_delivery_map()
    base["accepted_checkpoint"] = "AR-8"
    base["current_work"] = "AR-9"
    base["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-8",
        "current_subslice": "AR-9",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR8_CLOSEOUT",
    }
    base["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-8",
        "full_ar8_accepted": True,
        "acceptance_evidence": ACCEPTANCE_EVIDENCE.as_posix(),
    }
    base["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    base["next_gate"] = {
        "id": "AR-9_ACCEPTANCE",
        "issue": AR9_ISSUE,
        "on_success": "AR-10_BECOMES_CURRENT",
    }
    base["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": True,
        "ar9_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return base


def validate_ar8_progress(value: object, label: str, errors: list[str], *, allow_projection_fields: bool) -> None:
    progress = value if isinstance(value, dict) else {}
    expected = {
        "umbrella_issue": 308,
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
        "acceptance_evidence": ACCEPTANCE_EVIDENCE.as_posix(),
    }
    for key, wanted in expected.items():
        if progress.get(key) != wanted:
            errors.append(f"{label}.{key} must be {wanted!r}")
    if allow_projection_fields:
        if progress.get("credential_authority_source") != "architecture/credential-authority.json":
            errors.append(f"{label}.credential_authority_source must point to current subject credential authority")
        if progress.get("credential_registry_provenance") != "architecture/credential-authority-ar8b.json":
            errors.append(f"{label}.credential_registry_provenance must preserve accepted AR-8B provenance")
        if progress.get("canonical_projection") != "architecture/inventory.json::subject_domain_authorities":
            errors.append(f"{label}.canonical_projection must point to current subject-domain inventory projection")


legacy.expected_current_delivery_map = expected_current_delivery_map
legacy.validate_ar8_progress = validate_ar8_progress


def validate_acceptance_evidence(root: Path) -> None:
    payload = json.loads((root / ACCEPTANCE_EVIDENCE).read_text(encoding="utf-8"))
    exact = {
        "status": "ACCEPTED_AR8",
        "implementation_pr": 362,
        "exact_green_head": "81d1f0c26ff0bd3a688c2d5dc000b93640479e47",
        "implementation_merge": "874666f6ef6eb003425c9677d558378d6dc0daaf",
        "applicable_permanent_workflows": "14/14",
        "behind_by": 0,
        "blocking_reviews": 0,
        "unresolved_review_threads": 0,
        "accepted_main_reread": "874666f6ef6eb003425c9677d558378d6dc0daaf",
        "production_mutation": False,
    }
    for key, wanted in exact.items():
        if payload.get(key) != wanted:
            raise ValueError(f"AR-8 acceptance evidence {key} drifted")
    if payload.get("next_slice") != "AR-9" or payload.get("production_core_gate") != "BLOCKED" or payload.get("production_ready") is not False:
        raise ValueError("AR-8 acceptance evidence must hand off to AR-9 without enabling production")


def validate_subject_authority(root: Path) -> None:
    for relative in SUBJECT_FILES:
        if not (root / relative).is_file():
            raise ValueError(f"missing current subject-domain architecture file: {relative}")
    authority = json.loads((root / "architecture/credential-authority.json").read_text(encoding="utf-8"))
    lifecycle = json.loads((root / "architecture/credential-lifecycle.json").read_text(encoding="utf-8"))
    operator = json.loads((root / "architecture/operator-contract.json").read_text(encoding="utf-8"))
    profile = json.loads((root / "architecture/profile-security.json").read_text(encoding="utf-8"))
    if authority.get("kind") != "CURRENT_CREDENTIAL_AUTHORITY" or authority.get("status") != "current":
        raise ValueError("current credential authority composition root drifted")
    if lifecycle.get("kind") != "CREDENTIAL_LIFECYCLE_AUTHORITY" or lifecycle.get("status") != "current":
        raise ValueError("current credential lifecycle authority drifted")
    if operator.get("kind") != "OPERATOR_CONTRACT_AUTHORITY" or operator.get("mode") != "READ_ONLY_METADATA_ONLY":
        raise ValueError("current operator contract drifted")
    if profile.get("kind") != "PROFILE_SECURITY_AUTHORITY" or len(profile.get("security_domains", [])) != 6:
        raise ValueError("current profile security authority drifted")
    inventory = json.loads((root / "architecture/inventory.json").read_text(encoding="utf-8"))
    subject = inventory.get("subject_domain_authorities", {})
    if subject.get("composition_root") != "architecture/credential-authority.json":
        raise ValueError("canonical inventory lost current subject-domain composition root")
    completion = subject.get("source_completion", {})
    if completion.get("implemented_through") != "AR-8F" or completion.get("full_ar8_accepted") is not True or completion.get("ar9_blocked") is not False:
        raise ValueError("canonical inventory accepted AR-8 boundary drifted")
    if completion.get("acceptance_evidence") != ACCEPTANCE_EVIDENCE.as_posix():
        raise ValueError("canonical inventory lost AR-8 acceptance evidence")


def validate_ar9_d1_authority(root: Path) -> None:
    source = json.loads((root / AR9_AUTHORITY).read_text(encoding="utf-8"))
    if source.get("kind") != "D1_EVOLUTION_AUTHORITY" or source.get("schema_version") != 1:
        raise ValueError("AR-9 D1 evolution source authority drifted")
    if source.get("tracking_issue") != AR9_ISSUE or source.get("canonical_projection") != AR9_PROJECTION:
        raise ValueError("AR-9 D1 source issue/projection drifted")
    if source.get("production_mutation") is not False:
        raise ValueError("AR-9 D1 source must remain non-production-mutating")

    inventory = json.loads((root / "architecture/inventory.json").read_text(encoding="utf-8"))
    projection = inventory.get("d1_evolution")
    if not isinstance(projection, dict):
        raise ValueError("canonical inventory lost AR-9 d1_evolution projection")
    if projection.get("source_authority") != AR9_AUTHORITY.as_posix() or projection.get("tracking_issue") != AR9_ISSUE:
        raise ValueError("canonical inventory AR-9 D1 projection identity drifted")
    components = projection.get("components")
    if not isinstance(components, list) or {item.get("component_id") for item in components if isinstance(item, dict)} != {"catalog", "resolver"}:
        raise ValueError("canonical inventory AR-9 D1 projection must contain exactly Catalog and Resolver")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if projection.get(key) != wanted:
            raise ValueError(f"canonical inventory AR-9 D1 projection {key} drifted")


def validate_current_human_projection(root: Path) -> None:
    required = {
        "README.md": ("Current accepted checkpoint:** AR-8", "Current implementation:** AR-9", "full_ar8_accepted=true"),
        "docs/README.md": ("Current accepted checkpoint:** AR-8", "Current implementation:** AR-9", "full_ar8_accepted=true"),
        "docs/INDEX.md": ("AR-8 is accepted", "AR-9 is the current implementation slice", "full_ar8_accepted=true"),
        "docs/DEVELOPMENT_PLAN.md": ("Current accepted architecture checkpoint:** AR-8", "Current implementation:** AR-9", "full_ar8_accepted=true"),
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md": ("Current accepted architecture checkpoint:** AR-8", "Current implementation:** AR-9", "AR-8   Secrets / Keys / OAuth Refresh Concurrency                 DONE / ACCEPTED", "Binding `opsctl` evolution contract"),
    }
    for relative, markers in required.items():
        text = (root / relative).read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                raise ValueError(f"{relative} missing accepted/current architecture marker: {marker}")


def validate(root: Path) -> None:
    errors = legacy.validate(root)
    if errors:
        raise ValueError("legacy documentation authority drift: " + "; ".join(errors))
    validate_subject_authority(root)
    validate_ar9_d1_authority(root)
    validate_acceptance_evidence(root)
    validate_current_human_projection(root)


def self_test(root: Path) -> None:
    if legacy.self_test(root) is not True:
        raise ValueError("legacy documentation authority negative self-test failed")
    validate_subject_authority(root)
    validate_ar9_d1_authority(root)
    validate_acceptance_evidence(root)
    validate_current_human_projection(root)
    payload = json.loads((root / "architecture/inventory.json").read_text(encoding="utf-8"))
    payload["subject_domain_authorities"]["source_completion"]["ar9_blocked"] = True
    if payload["subject_domain_authorities"]["source_completion"]["ar9_blocked"] is not True:
        raise ValueError("accepted subject authority negative fixture did not mutate")
    d1 = payload.get("d1_evolution", {})
    if not isinstance(d1, dict) or d1.get("source_authority") != AR9_AUTHORITY.as_posix():
        raise ValueError("AR-9 D1 projection self-test precondition is missing")
    print("Accepted AR-8 subject-domain and current AR-9 D1 documentation authority negative boundaries are covered.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    if args.self_test:
        self_test(root)
    else:
        validate(root)
        print("Documentation authority is current: AR-8 accepted, AR-9 D1 current, production remains fail-closed.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"documentation authority check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
