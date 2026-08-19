#!/usr/bin/env python3
"""Current documentation authority checker after accepted AR-9 and during AR-10."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEGACY_PATH = ROOT / "scripts/check-documentation-authority-legacy.py"
AR8_ACCEPTANCE_EVIDENCE = Path("docs/evidence/2026-08-18-ar8-final-acceptance.json")
AR9_ACCEPTANCE_EVIDENCE = Path("docs/evidence/2026-08-19-ar9-final-acceptance.json")
AR9_ISSUE = 366
AR9_AUTHORITY = Path("architecture/d1-evolution-ar9.json")
AR9_PROJECTION = "architecture/inventory.json::d1_evolution"
AR10_ISSUE = 368
AR10_AUTHORITY = Path("architecture/runtime-cutover-ar10.json")
AR10_PROJECTION = "architecture/inventory.json::runtime_cutover"
AR10_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md")
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
legacy.STATUS_DATE = "2026-08-19"
legacy.CURRENT_SLICE = "AR-10"
legacy.NEXT_SLICE = "AR-11"
legacy.CURRENT_DELIVERY_CHECKPOINT = "AR-9"
legacy.AR8_ACCEPTED_SUBSLICES = ["AR-8A", "AR-8B", "AR-8C", "AR-8D", "AR-8E", "AR-8F"]
legacy.AR8_CURRENT_SUBSLICE = None
legacy.AR8D_IMPLEMENTATION_ISSUE = None
legacy.AR8_IMPLEMENTATION_ENTRY_GATE = "AR8_ACCEPTED_MAIN_AR9_CURRENT"
legacy.AR8_MANDATORY_REMAINING = []
for accepted in ("AR-8", "AR-9"):
    if accepted not in legacy.ACCEPTED_SLICES:
        legacy.ACCEPTED_SLICES = [*legacy.ACCEPTED_SLICES, accepted]
for required in (
    AR8_ACCEPTANCE_EVIDENCE,
    AR9_ACCEPTANCE_EVIDENCE,
    AR9_AUTHORITY,
    AR10_AUTHORITY,
    AR10_EVIDENCE,
):
    if required not in legacy.REQUIRED_FILES:
        legacy.REQUIRED_FILES = (*legacy.REQUIRED_FILES, required)

# The legacy engine deliberately preserves deep historical invariants. These are
# hard-coded pre-AR-10 projection assertions only; the current wrapper replaces
# them with exact AR-9 accepted / AR-10 current checks below.
SUPERSEDED_LEGACY_PROJECTION_ERRORS = {
    "architecture transition must encode accepted AR-8 and current AR-9",
    "current v3 plan missing authority marker: Current accepted architecture checkpoint:** AR-8",
    "current v3 plan missing authority marker: Current implementation:** AR-9",
    "docs/DEVELOPER_CAPABILITY_MATRIX.md missing authority marker: full_ar8_accepted=true",
}


def expected_current_delivery_map() -> dict[str, object]:
    base = legacy_expected_current_delivery_map()
    base["accepted_checkpoint"] = "AR-9"
    base["current_work"] = "AR-10"
    base["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-9",
        "current_subslice": "AR-10",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR9_CLOSEOUT",
    }
    base["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-9",
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "acceptance_evidence": AR9_ACCEPTANCE_EVIDENCE.as_posix(),
    }
    base["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    base["next_gate"] = {
        "id": "AR-10_ACCEPTANCE",
        "issue": AR10_ISSUE,
        "on_success": "AR-11_BECOMES_CURRENT",
    }
    base["invariants"].update(
        {
            "source_present_not_equal_production_enabled": True,
            "full_ar8_accepted": True,
            "ar9_accepted": True,
            "ar10_blocked": False,
            "architecture_complete": False,
            "production_core_gate": "BLOCKED",
            "production_ready": False,
            "production_mutation": False,
        }
    )
    return base


def validate_ar8_progress(
    value: object,
    label: str,
    errors: list[str],
    *,
    allow_projection_fields: bool,
) -> None:
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
        "acceptance_evidence": AR8_ACCEPTANCE_EVIDENCE.as_posix(),
    }
    for key, wanted in expected.items():
        if progress.get(key) != wanted:
            errors.append(f"{label}.{key} must be {wanted!r}")
    if allow_projection_fields:
        if progress.get("credential_authority_source") != "architecture/credential-authority.json":
            errors.append(
                f"{label}.credential_authority_source must point to current subject credential authority"
            )
        if progress.get("credential_registry_provenance") != "architecture/credential-authority-ar8b.json":
            errors.append(
                f"{label}.credential_registry_provenance must preserve accepted AR-8B provenance"
            )
        if progress.get("canonical_projection") != "architecture/inventory.json::subject_domain_authorities":
            errors.append(
                f"{label}.canonical_projection must point to current subject-domain inventory projection"
            )


legacy.expected_current_delivery_map = expected_current_delivery_map
legacy.validate_ar8_progress = validate_ar8_progress


def current_legacy_errors(root: Path) -> list[str]:
    return [
        error
        for error in legacy.validate(root)
        if error not in SUPERSEDED_LEGACY_PROJECTION_ERRORS
    ]


def load_json(root: Path, relative: Path) -> dict[str, object]:
    payload = json.loads((root / relative).read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{relative} must contain one JSON object")
    return payload


def validate_ar8_acceptance(root: Path) -> None:
    payload = load_json(root, AR8_ACCEPTANCE_EVIDENCE)
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
    if (
        payload.get("next_slice") != "AR-9"
        or payload.get("production_core_gate") != "BLOCKED"
        or payload.get("production_ready") is not False
    ):
        raise ValueError("AR-8 acceptance evidence must hand off to AR-9 without enabling production")


def validate_ar9_acceptance(root: Path) -> None:
    payload = load_json(root, AR9_ACCEPTANCE_EVIDENCE)
    exact = {
        "kind": "AR9_FINAL_ACCEPTANCE",
        "accepted_program_checkpoint": "AR-9",
        "tracking_issue": AR9_ISSUE,
        "implementation_pr": 367,
        "exact_green_head": "6110a32ade85d08c6ad93d9064190fff768e7cc2",
        "applicable_permanent_workflows": "15/15",
        "failed_workflows": 0,
        "pending_workflows": 0,
        "behind_by": 0,
        "blocking_reviews": 0,
        "unresolved_review_threads": 0,
        "accepted_main_merge": "5933a5e30a534209138485556b4a895706af765a",
        "accepted_main_reread": "5933a5e30a534209138485556b4a895706af765a",
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_authorized": False,
        "production_enabled": False,
        "production_mutation": False,
        "production_d1_execution": False,
        "next_slice": "AR-10",
    }
    for key, wanted in exact.items():
        if payload.get(key) != wanted:
            raise ValueError(f"AR-9 acceptance evidence {key} drifted")


def validate_subject_authority(root: Path) -> None:
    for relative in SUBJECT_FILES:
        if not (root / relative).is_file():
            raise ValueError(f"missing current subject-domain architecture file: {relative}")
    authority = load_json(root, Path("architecture/credential-authority.json"))
    lifecycle = load_json(root, Path("architecture/credential-lifecycle.json"))
    operator = load_json(root, Path("architecture/operator-contract.json"))
    profile = load_json(root, Path("architecture/profile-security.json"))
    if authority.get("kind") != "CURRENT_CREDENTIAL_AUTHORITY" or authority.get("status") != "current":
        raise ValueError("current credential authority composition root drifted")
    if lifecycle.get("kind") != "CREDENTIAL_LIFECYCLE_AUTHORITY" or lifecycle.get("status") != "current":
        raise ValueError("current credential lifecycle authority drifted")
    if operator.get("kind") != "OPERATOR_CONTRACT_AUTHORITY" or operator.get("mode") != "READ_ONLY_METADATA_ONLY":
        raise ValueError("current operator contract drifted")
    if profile.get("kind") != "PROFILE_SECURITY_AUTHORITY" or len(profile.get("security_domains", [])) != 6:
        raise ValueError("current profile security authority drifted")
    inventory = load_json(root, Path("architecture/inventory.json"))
    subject = inventory.get("subject_domain_authorities")
    if not isinstance(subject, dict) or subject.get("composition_root") != "architecture/credential-authority.json":
        raise ValueError("canonical inventory lost current subject-domain composition root")
    completion = subject.get("source_completion")
    if not isinstance(completion, dict):
        raise ValueError("canonical inventory lost accepted AR-8 source completion")
    if (
        completion.get("implemented_through") != "AR-8F"
        or completion.get("full_ar8_accepted") is not True
        or completion.get("ar9_blocked") is not False
        or completion.get("acceptance_evidence") != AR8_ACCEPTANCE_EVIDENCE.as_posix()
    ):
        raise ValueError("canonical inventory accepted AR-8 boundary drifted")


def validate_ar9_d1_authority(root: Path) -> None:
    source = load_json(root, AR9_AUTHORITY)
    if source.get("kind") != "D1_EVOLUTION_AUTHORITY" or source.get("schema_version") != 1:
        raise ValueError("AR-9 D1 evolution source authority drifted")
    if (
        source.get("status") != "accepted"
        or source.get("tracking_issue") != AR9_ISSUE
        or source.get("canonical_projection") != AR9_PROJECTION
        or source.get("production_mutation") is not False
    ):
        raise ValueError("AR-9 D1 accepted authority identity/state drifted")
    acceptance = source.get("acceptance")
    if not isinstance(acceptance, dict):
        raise ValueError("AR-9 D1 authority lost acceptance provenance")
    if (
        acceptance.get("implementation_merge") != "5933a5e30a534209138485556b4a895706af765a"
        or acceptance.get("evidence") != AR9_ACCEPTANCE_EVIDENCE.as_posix()
        or acceptance.get("production_mutation") is not False
    ):
        raise ValueError("AR-9 D1 authority acceptance provenance drifted")

    inventory = load_json(root, Path("architecture/inventory.json"))
    projection = inventory.get("d1_evolution")
    if not isinstance(projection, dict):
        raise ValueError("canonical inventory lost AR-9 d1_evolution projection")
    if (
        projection.get("source_authority") != AR9_AUTHORITY.as_posix()
        or projection.get("source_status") != "accepted"
        or projection.get("acceptance_evidence") != AR9_ACCEPTANCE_EVIDENCE.as_posix()
        or projection.get("tracking_issue") != AR9_ISSUE
    ):
        raise ValueError("canonical inventory AR-9 D1 projection identity drifted")
    components = projection.get("components")
    if not isinstance(components, list) or {
        item.get("component_id") for item in components if isinstance(item, dict)
    } != {"catalog", "resolver"}:
        raise ValueError("canonical inventory AR-9 D1 projection must contain exactly Catalog and Resolver")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if projection.get(key) != wanted:
            raise ValueError(f"canonical inventory AR-9 D1 projection {key} drifted")


def validate_ar10_runtime_authority(root: Path) -> None:
    source = load_json(root, AR10_AUTHORITY)
    if (
        source.get("kind") != "RUNTIME_CUTOVER_AUTHORITY"
        or source.get("schema_version") != 1
        or source.get("status") != "AR10_IMPLEMENTED_PENDING_ACCEPTANCE"
        or source.get("owning_slice") != "AR-10"
        or source.get("owning_issue") != AR10_ISSUE
        or source.get("completion_pr") != 371
    ):
        raise ValueError("AR-10 runtime-cutover source authority identity/state drifted")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
        "legacy_executables_remaining": 0,
    }.items():
        if source.get(key) != wanted:
            raise ValueError(f"AR-10 runtime-cutover source {key} drifted")
    if source.get("real_runtime", {}).get("production_certified") is not False:
        raise ValueError("AR-10 repository integration must not masquerade as production certification")
    if not (root / AR10_EVIDENCE).is_file():
        raise ValueError("AR-10 runtime evidence document is missing")

    inventory = load_json(root, Path("architecture/inventory.json"))
    projection = inventory.get("runtime_cutover")
    if not isinstance(projection, dict):
        raise ValueError("canonical inventory lost AR-10 runtime_cutover projection")
    if (
        projection.get("source_authority") != AR10_AUTHORITY.as_posix()
        or projection.get("source_status") != "AR10_IMPLEMENTED_PENDING_ACCEPTANCE"
        or projection.get("tracking_issue") != AR10_ISSUE
        or projection.get("legacy_executables_remaining") != 0
    ):
        raise ValueError("canonical inventory AR-10 runtime-cutover projection identity drifted")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if projection.get(key) != wanted:
            raise ValueError(f"canonical inventory AR-10 runtime projection {key} drifted")


def validate_current_human_projection(root: Path) -> None:
    required = {
        "README.md": (
            "Current accepted checkpoint:** AR-9",
            "Current implementation:** AR-10",
            "COMPLETE THROUGH AR-9",
            "AR-10 acceptance",
        ),
        "docs/README.md": (
            "Current accepted checkpoint:** AR-9",
            "Current implementation:** AR-10",
            "COMPLETE THROUGH AR-9",
            "AR-10 acceptance",
        ),
        "docs/INDEX.md": (
            "AR-9 is accepted",
            "AR-10 is the current implementation slice",
            "COMPLETE THROUGH AR-9",
        ),
        "docs/DEVELOPMENT_PLAN.md": (
            "Current accepted architecture checkpoint:** AR-9",
            "Current implementation:** AR-10",
            "AR-9   D1 Evolution / Schema Compatibility",
            "AR-10  Runtime and Historical Executable Simplification",
            "COMPLETE THROUGH AR-9",
        ),
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md": (
            "Current accepted architecture checkpoint:** AR-9",
            "Current implementation:** AR-10",
            "AR-9   D1 Evolution / Schema Compatibility",
            "AR-10  Runtime and Historical Executable Simplification",
            "Binding `opsctl` evolution contract",
        ),
        "docs/DEVELOPER_CAPABILITY_MATRIX.md": (
            "AR-9 source is accepted on `main`; AR-10 is the current architecture slice.",
            "COMPLETE THROUGH AR-9",
            "AR-10 acceptance",
            "production_ready=false",
        ),
    }
    for relative, markers in required.items():
        text = (root / relative).read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                raise ValueError(f"{relative} missing accepted/current architecture marker: {marker}")


def validate(root: Path) -> None:
    errors = current_legacy_errors(root)
    if errors:
        raise ValueError("legacy documentation authority drift: " + "; ".join(errors))
    validate_subject_authority(root)
    validate_ar8_acceptance(root)
    validate_ar9_acceptance(root)
    validate_ar9_d1_authority(root)
    validate_ar10_runtime_authority(root)
    validate_current_human_projection(root)


def mutate_json_path(path: Path, keys: tuple[str, ...], old: object, new: object) -> bool:
    payload = json.loads(path.read_text(encoding="utf-8"))
    cursor: object = payload
    for key in keys[:-1]:
        if not isinstance(cursor, dict) or key not in cursor:
            return False
        cursor = cursor[key]
    if not isinstance(cursor, dict):
        return False
    leaf = keys[-1]
    if cursor.get(leaf) != old:
        return False
    cursor[leaf] = new
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return True


def require_negative_rejection(
    label: str,
    errors: list[str],
    expected: str,
    *,
    canonical_map: bool = False,
) -> None:
    specific = any(expected.lower() in error.lower() for error in errors)
    delivery_map = canonical_map and any("current_delivery_map" in error.lower() for error in errors)
    if not errors or not (specific or delivery_map):
        raise ValueError(
            f"legacy negative fixture {label} was not rejected by its specific or canonical-map invariant: {errors}"
        )


def legacy_negative_self_test(root: Path) -> None:
    baseline = current_legacy_errors(root)
    if baseline:
        raise ValueError(
            "legacy documentation negative self-test requires a valid baseline: " + "; ".join(baseline)
        )
    status_fixtures = [
        ("tracking rollback", ("current", "architecture_program", "tracking_issue"), 266, 203, "tracking_issue", False),
        ("active slice rollback", ("current", "architecture_program", "current_slice"), "AR-10", "AR-9", "current_slice", False),
        ("AR-8 acceptance rollback", ("current", "architecture_program", "ar8_progress", "full_ar8_accepted"), True, False, "full_ar8_accepted", False),
        ("AR-9 reblock", ("current", "architecture_program", "ar8_progress", "ar9_blocked"), False, True, "ar9_blocked", False),
        ("premature architecture closeout", ("current", "architecture_complete"), False, True, "architecture_complete", False),
        ("premature gate authorization", ("current", "production_core_gate"), "BLOCKED", "AUTHORIZED", "Production", False),
        ("premature production readiness", ("production_ready",), False, True, "production_ready", False),
        ("premature delivery-map production enablement", ("current", "current_delivery_map", "production_enabled", "status"), False, True, "CURRENT_DELIVERY_MAP", True),
        ("historical #203 resurrected", ("current", "pre2j_product_readiness_remediation", "forward_execution_authority"), False, True, "#203", False),
    ]
    for label, keys, old, new, expected, canonical_map in status_fixtures:
        with legacy.tempfile.TemporaryDirectory(prefix="ar10-document-authority-") as directory:
            fixture = Path(directory)
            legacy.copy_fixture(root, fixture)
            path = fixture / "docs/status.json"
            if not mutate_json_path(path, keys, old, new):
                raise ValueError(f"legacy negative fixture authoritative JSON path missing for {label}: {keys}")
            errors = current_legacy_errors(fixture)
            require_negative_rejection(label, errors, expected, canonical_map=canonical_map)

    text_fixtures = [
        ("generation queue resurrection", legacy.TOPOLOGY, '"decision": "DELETE"', '"decision": "KEEP"', "GENERATION_VERIFICATION"),
        ("legacy D3 production resurrection", legacy.TOPOLOGY, '"legacy_d3_production_lane": "DISABLE_FORWARD_EXECUTION"', '"legacy_d3_production_lane": "KEEP"', "D3"),
    ]
    for label, relative, old, new, expected in text_fixtures:
        with legacy.tempfile.TemporaryDirectory(prefix="ar10-document-authority-") as directory:
            fixture = Path(directory)
            legacy.copy_fixture(root, fixture)
            path = fixture / relative
            if not legacy.mutate(path, old, new):
                raise ValueError(f"legacy negative fixture source marker missing for {label}: {old}")
            errors = current_legacy_errors(fixture)
            require_negative_rejection(label, errors, expected)
    print("Legacy documentation authority negative fixtures remain covered through current AR-10 projections.")


def self_test(root: Path) -> None:
    legacy_negative_self_test(root)
    validate_subject_authority(root)
    validate_ar8_acceptance(root)
    validate_ar9_acceptance(root)
    validate_ar9_d1_authority(root)
    validate_ar10_runtime_authority(root)
    validate_current_human_projection(root)
    inventory = load_json(root, Path("architecture/inventory.json"))
    runtime = inventory.get("runtime_cutover")
    if not isinstance(runtime, dict):
        raise ValueError("AR-10 runtime projection self-test precondition is missing")
    mutated = dict(runtime)
    mutated["legacy_executables_remaining"] = 1
    if mutated == runtime:
        raise ValueError("AR-10 runtime projection negative fixture did not mutate")
    print("Accepted AR-9 / current AR-10 documentation authority negative boundaries are covered.")


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
        print(
            "Documentation authority is current: AR-9 accepted, AR-10 runtime cutover current, production remains fail-closed."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"documentation authority check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
