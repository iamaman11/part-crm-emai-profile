#!/usr/bin/env python3
"""Validate current documentation/program boundaries without replaying legacy lifecycle state.

Architecture acceptance/current-slice state is owned by the generic Git acceptance protocol and is
resolved at read time. Tracked status/inventory/README material is projection-only and must never
become a second lifecycle authority. Historical AR-specific security/governance invariants are
validated by their owning permanent gates rather than duplicated here.
"""

from __future__ import annotations

import argparse
import copy
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
ACCEPTANCE_POLICY = Path("architecture/architecture-acceptance-policy.json")
PROGRAM_SEQUENCE = Path("architecture/architecture-program-sequence.json")
STATUS = Path("docs/status.json")
TRANSITION = Path("architecture/architecture-rebaseline-v3-transition.json")
INVENTORY = Path("architecture/inventory.json")
PLAN = Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md")
PROTOCOL = Path("docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md")
AR9_AUTHORITY = Path("architecture/d1-evolution-ar9.json")
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


def run_acceptance(root: Path, command: str) -> dict[str, Any] | None:
    completed = subprocess.run(
        ["node", ".github/scripts/architecture-acceptance.mjs", command],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        fail(f"architecture acceptance {command} failed: {detail}")
    if command != "derive":
        return None
    try:
        value = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        fail(f"architecture acceptance derive emitted invalid JSON: {error}")
    if not isinstance(value, dict):
        fail("architecture acceptance derive must emit one JSON object")
    return value


def require_markers(text: str, markers: tuple[str, ...], label: str) -> None:
    missing = [marker for marker in markers if marker not in text]
    if missing:
        fail(f"{label} is missing required markers: {missing}")


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
    paths = projection.get("paths")
    if not isinstance(paths, list) or set(paths) != PROJECTION_PATHS or len(paths) != len(PROJECTION_PATHS):
        fail("architecture acceptance projection path set drifted")


def validate_derived_state(state: dict[str, Any]) -> None:
    accepted = state.get("accepted_checkpoint")
    current = state.get("current_slice")
    if not isinstance(accepted, str) or not accepted.startswith("AR-"):
        fail("derived accepted checkpoint is malformed")
    if current is not None and (not isinstance(current, str) or not current.startswith("AR-")):
        fail("derived current slice is malformed")
    if accepted == "AR-17":
        expected = {
            "architecture_complete": True,
            "production_core_gate": "AUTHORIZED",
            "production_ready": False,
            "production_mutation": False,
        }
    else:
        expected = {
            "architecture_complete": False,
            "production_core_gate": "BLOCKED",
            "production_ready": False,
            "production_mutation": False,
        }
    for key, wanted in expected.items():
        if state.get(key) != wanted:
            fail(f"derived lifecycle state {key} drifted: expected {wanted!r}, observed {state.get(key)!r}")


def validate_projection_fail_closed(status: dict[str, Any], transition: dict[str, Any], inventory: dict[str, Any]) -> None:
    if status.get("production_ready") is not False:
        fail("docs/status.json may not project production_ready=true before PC-1")
    current = status.get("current")
    if not isinstance(current, dict):
        fail("docs/status.json current projection is missing")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        fail("docs/status.json must remain fail-closed while post-AR-11 cleanup is active")

    state_model = transition.get("state_model")
    if not isinstance(state_model, dict):
        fail("architecture transition projection lost state_model")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
    }.items():
        if state_model.get(key) != wanted:
            fail(f"architecture transition projection must remain fail-closed: {key}")

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
    ar9 = load_json(root, AR9_AUTHORITY)
    if (
        ar9.get("kind") != "D1_EVOLUTION_AUTHORITY"
        or ar9.get("status") != "accepted"
        or ar9.get("production_mutation") is not False
    ):
        fail("accepted AR-9 D1 authority identity/state drifted")

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


def validate(root: Path) -> dict[str, Any]:
    run_acceptance(root, "contract")
    state = run_acceptance(root, "derive")
    assert state is not None
    validate_derived_state(state)

    policy = load_json(root, ACCEPTANCE_POLICY)
    sequence = load_json(root, PROGRAM_SEQUENCE)
    if policy.get("kind") != "ARCHITECTURE_ACCEPTANCE_POLICY" or policy.get("status") != "current":
        fail("architecture acceptance policy identity/state drifted")
    if sequence.get("kind") != "ARCHITECTURE_PROGRAM_SEQUENCE" or sequence.get("state_model") != "STATIC_ORDER_ONLY":
        fail("static architecture program sequence identity/state drifted")
    validate_projection_policy(policy)

    status = load_json(root, STATUS)
    transition = load_json(root, TRANSITION)
    inventory = load_json(root, INVENTORY)
    validate_projection_fail_closed(status, transition, inventory)
    validate_owned_authorities(root)
    validate_compatibility_entrypoints(root)
    validate_human_authority(root)
    return state


def self_test(root: Path) -> None:
    state = validate(root)
    policy = load_json(root, ACCEPTANCE_POLICY)
    negative = copy.deepcopy(policy)
    negative["projection_policy"]["authoritative"] = True
    try:
        validate_projection_policy(negative)
    except DocumentationAuthorityError:
        pass
    else:
        fail("authoritative tracked-projection negative fixture unexpectedly passed")

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

    run_acceptance(root, "self-test")
    print(
        "Documentation authority negative matrix passed: tracked lifecycle projections are non-authoritative, "
        f"derived acceptance checkpoint={state['accepted_checkpoint']} current={state['current_slice']}."
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        if args.self_test:
            self_test(root)
        else:
            state = validate(root)
            print(
                "Documentation/program authority is consistent: lifecycle state is Git-derived, tracked projections are non-authoritative, "
                f"accepted={state['accepted_checkpoint']} current={state['current_slice']}."
            )
        return 0
    except (DocumentationAuthorityError, OSError, UnicodeError, json.JSONDecodeError) as error:
        print(f"documentation authority check failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
