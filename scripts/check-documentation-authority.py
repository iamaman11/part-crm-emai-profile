#!/usr/bin/env python3
"""Validate current documentation/program projection boundaries.

Architecture acceptance/current-slice state is owned exclusively by the generic Git acceptance
protocol. This checker intentionally does not re-derive Git history: it validates static program
policy, projection-only semantics, fail-closed compatibility projections and the identities of
current specialized AR-9/10/11 authorities. Historical lifecycle engines are not imported.
"""

from __future__ import annotations

import argparse
import copy
import json
import sys
from pathlib import Path
from typing import Any

import d1_repository_projection as d1_repository

ROOT = Path(__file__).resolve().parents[1]
ACCEPTANCE_POLICY = Path("architecture/architecture-acceptance-policy.json")
LIFECYCLE_POLICY = Path("architecture/lifecycle-projection-policy.json")
PROGRAM_SEQUENCE = Path("architecture/architecture-program-sequence.json")
STATUS = Path("docs/status.json")
TRANSITION = Path("architecture/architecture-rebaseline-v3-transition.json")
INVENTORY = Path("architecture/inventory.json")
PLAN = Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md")
PROTOCOL = Path("docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md")
AR10_AUTHORITY = Path("architecture/runtime-cutover-ar10.json")
AR11_AUTHORITY = Path("architecture/release-architecture-ar11.json")
IMPLEMENTATION_STUB = Path("IMPLEMENTATION_PLAN.md")
PROFILE_LIFECYCLE_STUB = Path("PROFILE_LIFECYCLE_PLAN.md")
PRE2J_STUB = Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md")
PROJECTION_PATHS = {
    "docs/status.json",
    "architecture/inventory.json",
    "architecture/architecture-rebaseline-v3-transition.json",
    "README.md",
    "docs/README.md",
    "docs/INDEX.md",
    "docs/DEVELOPMENT_PLAN.md",
    "docs/DEVELOPER_CAPABILITY_MATRIX.md",
}
TRACKED_LIFECYCLE_SNAPSHOTS = {
    "architecture/inventory.json",
    "docs/status.json",
    "architecture/architecture-rebaseline-v3-transition.json",
}


class DocumentationAuthorityError(ValueError):
    pass


def fail(message: str) -> None:
    raise DocumentationAuthorityError(message)


def read_text(root: Path, relative: Path) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        fail(f"required documentation/program file is missing or not regular: {relative}")
    return path.read_text(encoding="utf-8")


def load_json(root: Path, relative: Path) -> dict[str, Any]:
    try:
        value = json.loads(read_text(root, relative))
    except json.JSONDecodeError as error:
        fail(f"invalid JSON in {relative}: {error}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain one JSON object")
    return value


def require_markers(text: str, markers: tuple[str, ...], label: str) -> None:
    missing = [marker for marker in markers if marker not in text]
    if missing:
        fail(f"{label} is missing required markers: {missing}")


def validate_acceptance_policy(policy: dict[str, Any], sequence: dict[str, Any]) -> None:
    if (
        policy.get("schema_version") != 1
        or policy.get("kind") != "ARCHITECTURE_ACCEPTANCE_POLICY"
        or policy.get("status") != "current"
        or policy.get("program_sequence") != PROGRAM_SEQUENCE.as_posix()
        or policy.get("source_branch") != "main"
        or policy.get("source_history_count") != 1
    ):
        fail("architecture acceptance policy identity/single-history boundary drifted")
    if (
        sequence.get("schema_version") != 1
        or sequence.get("kind") != "ARCHITECTURE_PROGRAM_SEQUENCE"
        or sequence.get("state_model") != "STATIC_ORDER_ONLY"
        or sequence.get("mutable_lifecycle_state_forbidden") is not True
    ):
        fail("static architecture program sequence identity/state drifted")
    slices = sequence.get("slices")
    if not isinstance(slices, list) or not slices:
        fail("static architecture program sequence is empty")
    for item in slices:
        if not isinstance(item, dict):
            fail("architecture program sequence contains a non-object slice")
        for forbidden in ("accepted", "current", "accepted_checkpoint", "current_slice"):
            if forbidden in item:
                fail(f"static architecture sequence stores mutable lifecycle field {forbidden}")
    bootstrap = policy.get("state_derivation", {}).get("migration_bootstrap_acceptance")
    if not isinstance(bootstrap, dict):
        fail("architecture acceptance policy lost AR-11 migration bootstrap")
    if (
        bootstrap.get("slice") != "AR-11"
        or bootstrap.get("pr") != 374
        or bootstrap.get("candidate_tree") != bootstrap.get("merge_tree")
        or bootstrap.get("required_status_contexts_total") != bootstrap.get("required_status_contexts_success")
        or bootstrap.get("applicable_permanent_workflows_total") != bootstrap.get("applicable_permanent_workflows_success")
        or bootstrap.get("behind_by") != 0
        or bootstrap.get("blocking_reviews") != 0
        or bootstrap.get("unresolved_review_threads") != 0
        or bootstrap.get("production_mutation") is not False
    ):
        fail("AR-11 migration bootstrap acceptance summary drifted")


def validate_projection_policy(policy: dict[str, Any]) -> None:
    projection = policy.get("projection_policy")
    if not isinstance(projection, dict):
        fail("architecture acceptance policy lost projection_policy")
    if projection.get("authoritative") is not False:
        fail("tracked lifecycle projections may not become acceptance authority")
    if projection.get("generated_or_human_projection_only") is not True:
        fail("tracked lifecycle documents must remain projection-only")
    if projection.get("stale_projection_must_not_create_acceptance_authority") is not True:
        fail("stale projection fail-closed rule is missing")
    if projection.get("lifecycle_policy") != LIFECYCLE_POLICY.as_posix():
        fail("architecture acceptance policy lost lifecycle projection policy binding")
    if projection.get("tracked_mutable_lifecycle_state_forbidden_as_authority") is not True:
        fail("tracked mutable lifecycle state must remain forbidden as acceptance authority")
    if projection.get("program_document_lifecycle_status_is_projection") is not True:
        fail("program document lifecycle status must remain projection-only")
    if projection.get("future_acceptance_requires_source_projection_commit") is not False:
        fail("future acceptance must not require a second projection-only source commit")
    paths = projection.get("paths")
    if not isinstance(paths, list) or set(paths) != PROJECTION_PATHS or len(paths) != len(PROJECTION_PATHS):
        fail("architecture acceptance projection path set drifted")


def validate_lifecycle_projection_policy(lifecycle: dict[str, Any], acceptance: dict[str, Any]) -> None:
    if (
        lifecycle.get("schema_version") != 1
        or lifecycle.get("kind") != "LIFECYCLE_PROJECTION_POLICY"
        or lifecycle.get("status") != "current"
        or lifecycle.get("tracking_issue") != 375
        or lifecycle.get("production_mutation") is not False
    ):
        fail("lifecycle projection policy identity/ownership boundary drifted")

    live = lifecycle.get("live_state_authority")
    if not isinstance(live, dict):
        fail("lifecycle projection policy lost live_state_authority")
    if (
        live.get("acceptance_policy") != ACCEPTANCE_POLICY.as_posix()
        or live.get("program_sequence") != PROGRAM_SEQUENCE.as_posix()
        or live.get("deriver") != ".github/scripts/architecture-acceptance.mjs derive"
        or live.get("source_branch") != "main"
        or live.get("tracked_mutable_lifecycle_state") is not False
        or live.get("manual_current_slice_authority") is not False
        or live.get("manual_accepted_checkpoint_authority") is not False
    ):
        fail("lifecycle live-state authority boundary drifted")

    program = lifecycle.get("program_intent_boundary")
    if not isinstance(program, dict):
        fail("lifecycle projection policy lost program_intent_boundary")
    if (
        program.get("program_document") != PLAN.as_posix()
        or program.get("may_define_scope_sequence_and_invariants") is not True
        or program.get("may_independently_decide_accepted_checkpoint") is not False
        or program.get("may_independently_decide_current_slice") is not False
        or program.get("lifecycle_status_text_is_projection") is not True
    ):
        fail("program document boundary drifted from intent/projection-only role")

    snapshots = lifecycle.get("tracked_compatibility_snapshots")
    if not isinstance(snapshots, list) or len(snapshots) != len(TRACKED_LIFECYCLE_SNAPSHOTS):
        fail("tracked lifecycle compatibility snapshot registry drifted")
    observed_paths: set[str] = set()
    for item in snapshots:
        if not isinstance(item, dict):
            fail("tracked lifecycle snapshot entry must be an object")
        path = item.get("path")
        fields = item.get("lifecycle_projection_fields")
        if not isinstance(path, str) or path in observed_paths:
            fail("tracked lifecycle snapshot path is missing or duplicated")
        observed_paths.add(path)
        if item.get("classification") != "TRANSITION_PROVENANCE_ONLY_FOR_LIFECYCLE_STATE":
            fail(f"tracked lifecycle snapshot is not explicitly provenance-only: {path}")
        if not isinstance(fields, list) or not fields or any(not isinstance(field, str) or not field for field in fields):
            fail(f"tracked lifecycle snapshot projection field registry is invalid: {path}")
    if observed_paths != TRACKED_LIFECYCLE_SNAPSHOTS:
        fail("tracked lifecycle snapshot path set drifted")

    consumers = lifecycle.get("consumer_policy")
    if not isinstance(consumers, dict):
        fail("lifecycle projection policy lost consumer_policy")
    required_consumer_booleans = {
        "tracked_snapshot_may_decide_accepted_or_current_slice": False,
        "tracked_snapshot_may_authorize_production": False,
        "tracked_snapshot_may_drive_ar12_through_ar17_closeout": False,
        "human_projection_may_decide_accepted_or_current_slice": False,
        "operator_surface_must_label_snapshot_non_authoritative": True,
        "stable_inventory_generation_must_not_advance_lifecycle_state": True,
        "stable_inventory_generation_must_preserve_snapshot_without_advancing_it": True,
        "future_acceptance_requires_source_projection_commit": False,
        "duplicate_lifecycle_derivation_algorithm_forbidden": True,
    }
    for key, wanted in required_consumer_booleans.items():
        if consumers.get(key) is not wanted:
            fail(f"lifecycle projection consumer boundary drifted: {key}")

    fail_closed = lifecycle.get("fail_closed_invariants")
    if not isinstance(fail_closed, dict):
        fail("lifecycle projection policy lost fail_closed_invariants")
    expected_fail_closed = {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
        "source_present_not_equal_production_enabled": True,
    }
    for key, wanted in expected_fail_closed.items():
        if fail_closed.get(key) != wanted:
            fail(f"lifecycle projection fail-closed invariant drifted: {key}")

    forbidden = lifecycle.get("forbidden_patterns")
    if not isinstance(forbidden, dict):
        fail("lifecycle projection policy lost forbidden_patterns")
    for key in (
        "tracked_current_slice_as_authority",
        "tracked_accepted_checkpoint_as_authority",
        "per_ar_closeout_writer",
        "self_writing_ci",
        "second_source_merge_for_acceptance_projection",
        "retired_executable_materialization_for_execution",
    ):
        if forbidden.get(key) is not True:
            fail(f"lifecycle forbidden-pattern guard drifted: {key}")

    projection = acceptance.get("projection_policy")
    if not isinstance(projection, dict) or projection.get("lifecycle_policy") != LIFECYCLE_POLICY.as_posix():
        fail("acceptance policy and lifecycle projection policy are no longer mutually bound")


def validate_projection_fail_closed(status: dict[str, Any], transition: dict[str, Any], inventory: dict[str, Any]) -> None:
    if status.get("production_ready") is not False:
        fail("docs/status.json compatibility projection may not enable production")
    current = status.get("current")
    if not isinstance(current, dict):
        fail("docs/status.json current compatibility projection is missing")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        fail("docs/status.json compatibility projection must remain fail-closed")

    state_model = transition.get("state_model")
    if not isinstance(state_model, dict):
        fail("architecture transition compatibility projection lost state_model")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
    }.items():
        if state_model.get(key) != wanted:
            fail(f"architecture transition compatibility projection must remain fail-closed: {key}")

    delivery = inventory.get("current_delivery_map")
    invariants = delivery.get("invariants") if isinstance(delivery, dict) else None
    if not isinstance(invariants, dict):
        fail("architecture inventory lost current_delivery_map.invariants compatibility projection")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if invariants.get(key) != wanted:
            fail(f"architecture inventory compatibility projection must remain fail-closed: {key}")


def validate_owned_authorities(root: Path) -> None:
    d1 = d1_repository.load(root)
    if (
        d1.get("kind") != "D1_REPOSITORY_PROJECTION"
        or d1.get("semantic_authority") != "tools/opsctl/src/d1"
        or d1.get("production_mutation") is not False
    ):
        fail("typed SQL-derived D1 repository identity/state drifted")

    ar10 = load_json(root, AR10_AUTHORITY)
    if (
        ar10.get("kind") != "RUNTIME_CUTOVER_AUTHORITY"
        or ar10.get("status") != "accepted"
        or ar10.get("legacy_executables_remaining") != 0
        or ar10.get("production_mutation") is not False
    ):
        fail("accepted AR-10 runtime authority identity/state drifted")

    ar11 = load_json(root, AR11_AUTHORITY)
    if (
        ar11.get("kind") != "AR11_RELEASE_ARCHITECTURE_SOURCE"
        or ar11.get("owning_slice") != "AR-11"
        or ar11.get("owning_issue") != 372
        or ar11.get("production_mutation") is not False
        or ar11.get("architecture_complete") is not False
        or ar11.get("production_core_gate") != "BLOCKED"
        or ar11.get("production_ready") is not False
    ):
        fail("AR-11 release/capability/promotion source authority drifted")


def validate_compatibility_entrypoints(root: Path) -> None:
    require_markers(
        read_text(root, IMPLEMENTATION_STUB),
        ("Document status:** SUPERSEDED", "history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"),
        "IMPLEMENTATION_PLAN.md",
    )
    require_markers(
        read_text(root, PROFILE_LIFECYCLE_STUB),
        ("Document status:** SUPERSEDED", "history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"),
        "PROFILE_LIFECYCLE_PLAN.md",
    )
    require_markers(
        read_text(root, PRE2J_STUB),
        ("ACCEPTED_HISTORICAL", "SUPERSEDED_FOR_FORWARD_EXECUTION", "Former tracking issue:** #203", "Current program authority"),
        "pre-2J compatibility stub",
    )


def validate_human_authority(root: Path) -> None:
    require_markers(
        read_text(root, PLAN),
        (
            "Document status:** CURRENT_AUTHORITY",
            "Tracking issue:** #266",
            "source_present != production_enabled",
            "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity",
        ),
        "Architecture Re-baseline v3 plan",
    )
    require_markers(
        read_text(root, PROTOCOL),
        (
            "Architecture Acceptance Protocol",
            "architecture/architecture-acceptance-policy.json",
            "architecture/architecture-program-sequence.json",
            "append-only annotated Git tag",
            "PC-1",
        ),
        "architecture acceptance protocol",
    )


def validate(root: Path) -> None:
    policy = load_json(root, ACCEPTANCE_POLICY)
    lifecycle = load_json(root, LIFECYCLE_POLICY)
    sequence = load_json(root, PROGRAM_SEQUENCE)
    validate_acceptance_policy(policy, sequence)
    validate_projection_policy(policy)
    validate_lifecycle_projection_policy(lifecycle, policy)
    validate_projection_fail_closed(load_json(root, STATUS), load_json(root, TRANSITION), load_json(root, INVENTORY))
    validate_owned_authorities(root)
    validate_compatibility_entrypoints(root)
    validate_human_authority(root)


def self_test(root: Path) -> None:
    validate(root)
    policy = load_json(root, ACCEPTANCE_POLICY)
    lifecycle = load_json(root, LIFECYCLE_POLICY)
    negative = copy.deepcopy(policy)
    negative["projection_policy"]["authoritative"] = True
    try:
        validate_projection_policy(negative)
    except DocumentationAuthorityError:
        pass
    else:
        fail("authoritative tracked-projection negative fixture unexpectedly passed")

    duplicate_deriver = copy.deepcopy(lifecycle)
    duplicate_deriver["consumer_policy"]["duplicate_lifecycle_derivation_algorithm_forbidden"] = False
    try:
        validate_lifecycle_projection_policy(duplicate_deriver, policy)
    except DocumentationAuthorityError:
        pass
    else:
        fail("duplicate lifecycle derivation negative fixture unexpectedly passed")

    mutable_snapshot = copy.deepcopy(lifecycle)
    mutable_snapshot["live_state_authority"]["tracked_mutable_lifecycle_state"] = True
    try:
        validate_lifecycle_projection_policy(mutable_snapshot, policy)
    except DocumentationAuthorityError:
        pass
    else:
        fail("tracked mutable lifecycle authority negative fixture unexpectedly passed")

    status = load_json(root, STATUS)
    transition = load_json(root, TRANSITION)
    inventory = load_json(root, INVENTORY)
    bad_status = copy.deepcopy(status)
    bad_status["production_ready"] = True
    try:
        validate_projection_fail_closed(bad_status, transition, inventory)
    except DocumentationAuthorityError:
        pass
    else:
        fail("premature production-ready projection negative fixture unexpectedly passed")

    bad_transition = copy.deepcopy(transition)
    bad_transition["state_model"]["production_core_gate"] = "AUTHORIZED"
    try:
        validate_projection_fail_closed(status, bad_transition, inventory)
    except DocumentationAuthorityError:
        pass
    else:
        fail("premature production authorization projection negative fixture unexpectedly passed")

    print("Documentation authority negative matrix passed: lifecycle projections cannot become acceptance authority or enable production.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test(args.root.resolve())
        else:
            validate(args.root.resolve())
            print("Documentation/program projection boundaries are current; Git lifecycle acceptance remains exclusively owned by architecture-acceptance.")
        return 0
    except (DocumentationAuthorityError, OSError, UnicodeError, json.JSONDecodeError) as error:
        print(f"documentation authority check failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
