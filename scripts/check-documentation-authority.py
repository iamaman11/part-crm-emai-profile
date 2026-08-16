#!/usr/bin/env python3
"""Fail closed when Architecture Re-baseline v3 documentation/program authority drifts."""

from __future__ import annotations

import argparse
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any

ACCEPTED_PHASE = "Phase 2I"
CURRENT_PROGRAM = "Architecture Re-baseline v3"
CURRENT_AUTHORITY = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
TRACKING_ISSUE = 266
SUBORDINATE_ISSUE = 268
CURRENT_SLICE = "AR-8"
NEXT_SLICE = "AR-9"
STATUS_DATE = "2026-08-16"
ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5", "AR-6", "AR-7"]
AR8_UMBRELLA_ISSUE = 308
AR8B_IMPLEMENTATION_ISSUE = 309
AR8_ACCEPTED_SUBSLICES = ["AR-8A"]
AR8_CURRENT_SUBSLICE = "AR-8B"
AR8_MANDATORY_REMAINING = ["AR-8B", "AR-8C", "AR-8D", "AR-8E", "AR-8F"]
TOPOLOGY = Path("architecture/runtime-topology-ar2.json")
AR2_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR2.md")
AR3_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR3.md")
AR4A_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4A.md")
AR4B_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4B.md")
AR4C_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md")
AR5_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR5.md")
AR6_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR6.md")
AR7_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR7.md")
GOVERNANCE_CONTRACT = Path("architecture/github-governance-ar7.json")
PYTHON_ESTATE = Path("architecture/python-estate-ar6.json")
CREDENTIAL_AUTHORITY = Path("architecture/credential-authority-ar8b.json")

REQUIRED_FILES = (
    Path("README.md"),
    Path("IMPLEMENTATION_PLAN.md"),
    Path("PROFILE_LIFECYCLE_PLAN.md"),
    Path("architecture/accepted-phases.json"),
    Path("architecture/architecture-rebaseline-v3-transition.json"),
    Path("architecture/inventory.json"),
    CREDENTIAL_AUTHORITY,
    TOPOLOGY,
    Path("docs/README.md"),
    Path("docs/INDEX.md"),
    Path("docs/status.json"),
    Path("docs/DEVELOPMENT_PLAN.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_AR0.md"),
    Path("docs/ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md"),
    AR2_EVIDENCE,
    AR3_EVIDENCE,
    AR4A_EVIDENCE,
    AR4B_EVIDENCE,
    AR4C_EVIDENCE,
    AR5_EVIDENCE,
    AR6_EVIDENCE,
    AR7_EVIDENCE,
    GOVERNANCE_CONTRACT,
    PYTHON_ESTATE,
    Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"),
    Path("docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"),
    Path("docs/THREAT_MODEL.md"),
    Path("history/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/DEVELOPMENT_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/status_pre_ar_v3_2026-08-15.json"),
    Path("history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md"),
    Path("history/ARCHITECTURE_REBASELINE_V3_PLAN_AR0_ACCEPTED_2026-08-15.md"),
    Path("history/architecture-rebaseline-v3-transition-ar0-accepted-2026-08-15.json"),
)


def read(root: Path, relative: Path) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"missing documentation-authority file: {relative}")
    return path.read_text(encoding="utf-8")


def load_json(root: Path, relative: Path) -> dict[str, Any]:
    try:
        payload = json.loads(read(root, relative))
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid JSON in {relative}: {exc}") from exc
    if not isinstance(payload, dict):
        raise ValueError(f"{relative} must contain a JSON object")
    return payload


def require(text: str, markers: tuple[str, ...], label: str, errors: list[str]) -> None:
    for marker in markers:
        if marker not in text:
            errors.append(f"{label} missing authority marker: {marker}")


def validate_ar8_progress(value: object, label: str, errors: list[str], *, allow_projection_fields: bool) -> None:
    progress = value if isinstance(value, dict) else {}
    expected = {
        "umbrella_issue": AR8_UMBRELLA_ISSUE,
        "accepted_subslices": AR8_ACCEPTED_SUBSLICES,
        "current_subslice": AR8_CURRENT_SUBSLICE,
        "current_implementation_issue": AR8B_IMPLEMENTATION_ISSUE,
        "mandatory_remaining": AR8_MANDATORY_REMAINING,
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "production_mutation": False,
    }
    for key, wanted in expected.items():
        if progress.get(key) != wanted:
            errors.append(f"{label}.{key} must be {wanted!r}")
    if allow_projection_fields:
        if progress.get("credential_authority_source") != CREDENTIAL_AUTHORITY.as_posix():
            errors.append(f"{label}.credential_authority_source must point to AR-8B source authority")
        if progress.get("canonical_projection") != "architecture/inventory.json::credential_authority":
            errors.append(f"{label}.canonical_projection must remain inside canonical inventory")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    try:
        for relative in REQUIRED_FILES:
            read(root, relative)
        ledger = load_json(root, Path("architecture/accepted-phases.json"))
        status = load_json(root, Path("docs/status.json"))
        transition = load_json(root, Path("architecture/architecture-rebaseline-v3-transition.json"))
        topology = load_json(root, TOPOLOGY)
        inventory = load_json(root, Path("architecture/inventory.json"))
        credential_authority = load_json(root, CREDENTIAL_AUTHORITY)
        root_readme = read(root, Path("README.md"))
        docs_readme = read(root, Path("docs/README.md"))
        index = read(root, Path("docs/INDEX.md"))
        development = read(root, Path("docs/DEVELOPMENT_PLAN.md"))
        plan = read(root, Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"))
        ar2_evidence = read(root, AR2_EVIDENCE)
        ar3_evidence = read(root, AR3_EVIDENCE)
        ar4a_evidence = read(root, AR4A_EVIDENCE)
        ar4b_evidence = read(root, AR4B_EVIDENCE)
        ar4c_evidence = read(root, AR4C_EVIDENCE)
        ar5_evidence = read(root, AR5_EVIDENCE)
        ar6_evidence = read(root, AR6_EVIDENCE)
        ar7_evidence = read(root, AR7_EVIDENCE)
        governance = load_json(root, GOVERNANCE_CONTRACT)
        python_estate = load_json(root, PYTHON_ESTATE)
        pre2j_stub = read(root, Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"))
        implementation_stub = read(root, Path("IMPLEMENTATION_PLAN.md"))
        lifecycle_stub = read(root, Path("PROFILE_LIFECYCLE_PLAN.md"))
    except ValueError as exc:
        return [str(exc)]

    phases = ledger.get("accepted_phases")
    accepted_phase = None
    if isinstance(phases, list) and phases and isinstance(phases[-1], dict):
        accepted_phase = phases[-1].get("phase")
    if accepted_phase != ACCEPTED_PHASE:
        errors.append(f"accepted phase ledger must end at {ACCEPTED_PHASE}; observed {accepted_phase!r}")

    if status.get("schema_version") != 5 or status.get("as_of") != STATUS_DATE:
        errors.append("docs/status.json must be the current AR-8 schema/date projection")
    if status.get("production_ready") is not False:
        errors.append("production_ready must remain false throughout AR-8")
    current = status.get("current") if isinstance(status.get("current"), dict) else {}
    if current.get("accepted_product_phase") != ACCEPTED_PHASE:
        errors.append("docs/status.json accepted product phase must remain Phase 2I")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        errors.append("AR-8 architecture_complete/Production Core gate state must remain fail closed")

    program = current.get("architecture_program") if isinstance(current.get("architecture_program"), dict) else {}
    expected_program = {
        "name": CURRENT_PROGRAM,
        "status": "active",
        "authority": CURRENT_AUTHORITY,
        "tracking_issue": TRACKING_ISSUE,
        "subordinate_preproduction_issue": SUBORDINATE_ISSUE,
        "current_slice": CURRENT_SLICE,
        "next_slice_after_acceptance": NEXT_SLICE,
        "runtime_topology_decision": TOPOLOGY.as_posix(),
        "runtime_topology_evidence": AR2_EVIDENCE.as_posix(),
    }
    for key, expected in expected_program.items():
        if program.get(key) != expected:
            errors.append(f"docs/status.json architecture_program.{key} must be {expected!r}")
    if program.get("accepted_slices") != ACCEPTED_SLICES:
        errors.append(f"docs/status.json accepted_slices must remain fully accepted top-level slices only: {ACCEPTED_SLICES!r}")
    validate_ar8_progress(program.get("ar8_progress"), "docs/status.json architecture_program.ar8_progress", errors, allow_projection_fields=False)

    ar5 = program.get("ar5_acceptance") if isinstance(program.get("ar5_acceptance"), dict) else {}
    if (
        program.get("runtime_authority_cleanup_evidence") != AR5_EVIDENCE.as_posix()
        or ar5.get("issue") != 290
        or ar5.get("implementation_pr") != 291
        or ar5.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"
        or ar5.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"
        or ar5.get("applicable_permanent_workflows") != "13/13"
    ):
        errors.append("docs/status.json AR-5 acceptance provenance drifted")
    ar6 = program.get("ar6_acceptance") if isinstance(program.get("ar6_acceptance"), dict) else {}
    expected_python_summary = {
        "tracked_python_files": 116,
        "DELETE_AFTER_SEQUENCE": 6,
        "KEEP_PYTHON": 108,
        "MIGRATE_TO_RUST": 2,
        "WRAP_WITH_RUST": 0,
    }
    if (
        program.get("python_estate") != PYTHON_ESTATE.as_posix()
        or program.get("python_operational_evidence") != AR6_EVIDENCE.as_posix()
        or ar6.get("issue") != 294
        or ar6.get("implementation_pr") != 295
        or ar6.get("exact_green_head") != "9b06d542873ffa3122e53e107105098e21f5933c"
        or ar6.get("implementation_merge") != "d0229fedd81ee870822b6d9394bc4ee313ea3a3c"
        or ar6.get("applicable_permanent_workflows") != "13/13"
        or ar6.get("closeout_issue") != 296
    ):
        errors.append("docs/status.json AR-6 acceptance provenance drifted")
    ar7 = program.get("ar7_acceptance") if isinstance(program.get("ar7_acceptance"), dict) else {}
    if (
        program.get("github_governance_contract") != GOVERNANCE_CONTRACT.as_posix()
        or program.get("github_governance_evidence") != AR7_EVIDENCE.as_posix()
        or ar7.get("issue") != 298
        or ar7.get("implementation_pr") != 299
        or ar7.get("exact_green_head") != "1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7"
        or ar7.get("implementation_merge") != "3492273cb9237850e3fa27343cc5edbdb0f66aa1"
        or ar7.get("applicable_permanent_workflows") != "14/14"
        or ar7.get("hosted_audit_run_id") != 31953316327
        or ar7.get("hosted_contract_job") != "success"
        or ar7.get("hosted_state_job") != "success"
        or ar7.get("direct_main_negative_probe") != "HTTP_409_REJECTED_NO_SENTINEL"
        or ar7.get("required_status_checks") != 21
        or ar7.get("closeout_issue") != 300
    ):
        errors.append("docs/status.json AR-7 acceptance provenance drifted")

    acceptance = governance.get("acceptance") if isinstance(governance.get("acceptance"), dict) else {}
    if (
        governance.get("status") != "ACCEPTED_AR7_GITHUB_GOVERNANCE"
        or governance.get("repository") != "iamaman11/part-crm-emai-profile"
        or acceptance.get("implementation_pr") != 299
        or acceptance.get("hosted_audit_run_id") != 31953316327
        or acceptance.get("hosted_state_job") != "success"
        or acceptance.get("direct_main_negative_probe") != "HTTP_409_REJECTED_NO_SENTINEL"
        or acceptance.get("production_ready") is not False
    ):
        errors.append("accepted AR-7 GitHub governance contract drifted")
    if (
        python_estate.get("status") != "AR6_ACCEPTED_PYTHON_ESTATE"
        or python_estate.get("accepted_program_checkpoint") != "AR-6"
        or python_estate.get("summary") != expected_python_summary
    ):
        errors.append("accepted AR-6 Python estate authority drifted")

    if (
        credential_authority.get("status") != "CANDIDATE_AR8B_CREDENTIAL_METADATA_AUTHORITY"
        or credential_authority.get("parent_issue") != AR8_UMBRELLA_ISSUE
        or credential_authority.get("implementation_issue") != AR8B_IMPLEMENTATION_ISSUE
        or credential_authority.get("canonical_inventory") != "architecture/inventory.json"
        or credential_authority.get("metadata_only") is not True
    ):
        errors.append("AR-8B credential source authority provenance/policy drifted")

    predecessor = current.get("predecessor_external_d3") if isinstance(current.get("predecessor_external_d3"), dict) else {}
    if predecessor.get("issue") != 251:
        errors.append("AR-2 must preserve predecessor issue #251 provenance")
    if predecessor.get("role") != "AR2_CLASSIFIED_SUPERSEDED_FORWARD_PRODUCTION_SEQUENCE":
        errors.append("issue #251 must be classified as superseded forward production sequencing")
    if predecessor.get("legacy_production_lane") != "DISABLED_BY_AR2":
        errors.append("legacy D3 production lane must be disabled after AR-2")
    if predecessor.get("current_state") != "closed_not_planned_after_ar2_acceptance":
        errors.append("issue #251 must remain closed not_planned after AR-2 acceptance")

    pre2j = current.get("pre2j_product_readiness_remediation") if isinstance(current.get("pre2j_product_readiness_remediation"), dict) else {}
    if not (
        pre2j.get("status") == "active_blocking"
        and pre2j.get("tracking_issue") == 203
        and pre2j.get("authority_role") == "HISTORICAL_PREDECESSOR_BLOCKER_ONLY"
        and pre2j.get("forward_execution_authority") is False
    ):
        errors.append("#203 predecessor blocker lifecycle must remain compatible but non-forward")
    phase2j = current.get("phase_2j") if isinstance(current.get("phase_2j"), dict) else {}
    if phase2j.get("status") != "blocked_pending_repository_remediation" or phase2j.get("forward_execution_authority") is not False:
        errors.append("historical Phase 2J state must remain blocked/non-forward")

    if transition.get("schema_version") != 10 or transition.get("status") != "ACTIVE_DURING_AR8_AFTER_ACCEPTED_AR8A":
        errors.append("architecture transition must encode active AR-8 after accepted AR-8A")
    if transition.get("tracking_issue") != TRACKING_ISSUE or transition.get("current_authority") != CURRENT_AUTHORITY:
        errors.append("architecture transition authority drifted")
    if transition.get("accepted_slices") != ACCEPTED_SLICES:
        errors.append("architecture transition accepted_slices drifted")
    if transition.get("current_slice") != CURRENT_SLICE or transition.get("next_slice_after_acceptance") != NEXT_SLICE:
        errors.append("architecture transition must encode active AR-8 with AR-9 only after full acceptance")
    validate_ar8_progress(transition.get("ar8_progress"), "architecture transition ar8_progress", errors, allow_projection_fields=True)
    transition_state = transition.get("state_model") if isinstance(transition.get("state_model"), dict) else {}
    if transition_state.get("architecture_complete") is not False or transition_state.get("production_core_gate") != "BLOCKED" or transition_state.get("production_ready") is not False:
        errors.append("transition state must remain fail closed through AR-8")
    runtime = transition.get("runtime_topology") if isinstance(transition.get("runtime_topology"), dict) else {}
    if runtime.get("decision_authority") != TOPOLOGY.as_posix() or runtime.get("generation_verification_decision") != "DELETE" or runtime.get("legacy_d3_production_forward_execution") != "DISABLED":
        errors.append("transition lost accepted AR-2 runtime-topology decisions")
    cleanup = transition.get("runtime_authority_cleanup") if isinstance(transition.get("runtime_authority_cleanup"), dict) else {}
    if (
        runtime.get("generation_verification_source_binding_removal") != "ACCEPTED_AR5"
        or runtime.get("runtime_authority_cleanup_evidence") != AR5_EVIDENCE.as_posix()
        or cleanup.get("status") != "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP"
        or cleanup.get("evidence") != AR5_EVIDENCE.as_posix()
        or cleanup.get("implementation_pr") != 291
        or cleanup.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"
        or cleanup.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"
        or cleanup.get("next_required_slice") != "AR-6"
        or cleanup.get("production_mutation") is not False
    ):
        errors.append("transition AR-5 runtime-authority cleanup acceptance drifted")
    python_ops = transition.get("python_operational_authority") if isinstance(transition.get("python_operational_authority"), dict) else {}
    python_opsctl = python_ops.get("opsctl") if isinstance(python_ops.get("opsctl"), dict) else {}
    if (
        python_ops.get("status") != "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION"
        or python_ops.get("evidence") != AR6_EVIDENCE.as_posix()
        or python_ops.get("python_estate") != PYTHON_ESTATE.as_posix()
        or python_ops.get("implementation_pr") != 295
        or python_ops.get("exact_green_head") != "9b06d542873ffa3122e53e107105098e21f5933c"
        or python_ops.get("implementation_merge") != "d0229fedd81ee870822b6d9394bc4ee313ea3a3c"
        or python_ops.get("next_required_slice") != "AR-7"
        or python_ops.get("production_mutation") is not False
        or python_opsctl.get("mode") != "READ_ONLY_FOUNDATION"
        or python_opsctl.get("commands") != ["doctor", "status", "inventory"]
        or python_opsctl.get("provider_mutation") is not False
    ):
        errors.append("transition AR-6 Python/opsctl acceptance drifted")

    github_governance = transition.get("github_governance_authority") if isinstance(transition.get("github_governance_authority"), dict) else {}
    hosted = github_governance.get("hosted_audit") if isinstance(github_governance.get("hosted_audit"), dict) else {}
    negative_probe = github_governance.get("direct_main_negative_probe") if isinstance(github_governance.get("direct_main_negative_probe"), dict) else {}
    if (
        github_governance.get("status") != "ACCEPTED_AR7_GITHUB_GOVERNANCE"
        or github_governance.get("evidence") != AR7_EVIDENCE.as_posix()
        or github_governance.get("contract") != GOVERNANCE_CONTRACT.as_posix()
        or github_governance.get("implementation_pr") != 299
        or github_governance.get("exact_green_head") != "1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7"
        or github_governance.get("implementation_merge") != "3492273cb9237850e3fa27343cc5edbdb0f66aa1"
        or github_governance.get("applicable_permanent_workflows") != "14/14"
        or hosted.get("run_id") != 31953316327
        or hosted.get("hosted_state_job") != "success"
        or negative_probe.get("result") != "HTTP_409_REJECTED"
        or negative_probe.get("sentinel_present_after_probe") is not False
        or github_governance.get("next_required_slice") != "AR-8"
        or github_governance.get("production_mutation") is not False
    ):
        errors.append("transition AR-7 GitHub governance acceptance drifted")

    if topology.get("slice") != "AR-2" or topology.get("production_mutation") is not False:
        errors.append("AR-2 topology authority must remain non-mutating")
    generation = topology.get("generation_verification") if isinstance(topology.get("generation_verification"), dict) else {}
    if generation.get("decision") != "DELETE" or generation.get("source_binding_removal_slice") != "AR-5":
        errors.append("GENERATION_VERIFICATION delete decision drifted")
    d3 = topology.get("d3_compatibility") if isinstance(topology.get("d3_compatibility"), dict) else {}
    if d3.get("legacy_d3_production_lane") != "DISABLE_FORWARD_EXECUTION" or d3.get("generalize_release_semantics_in") != "AR-11":
        errors.append("AR-2 D3 compatibility decision drifted")

    doc_authority = inventory.get("documentation_authority") if isinstance(inventory.get("documentation_authority"), dict) else {}
    program_state = inventory.get("program_state") if isinstance(inventory.get("program_state"), dict) else {}
    if doc_authority.get("current_program") != CURRENT_AUTHORITY or doc_authority.get("current_slice") != CURRENT_SLICE:
        errors.append("architecture inventory documentation authority is stale")
    if doc_authority.get("runtime_topology_decision") != TOPOLOGY.as_posix():
        errors.append("architecture inventory must point to the accepted AR-2 topology decision")
    if doc_authority.get("credential_authority_source") != CREDENTIAL_AUTHORITY.as_posix() or doc_authority.get("credential_authority_projection") != "architecture/inventory.json::credential_authority":
        errors.append("architecture inventory must identify AR-8B source/projection authority")
    if program_state.get("accepted_architecture_slices") != ACCEPTED_SLICES or program_state.get("current_architecture_slice") != CURRENT_SLICE or program_state.get("next_architecture_slice_after_acceptance") != NEXT_SLICE:
        errors.append("architecture inventory active AR-8 program state is stale")
    validate_ar8_progress(program_state.get("ar8_progress"), "architecture inventory program_state.ar8_progress", errors, allow_projection_fields=False)
    if program_state.get("production_ready") is not False or program_state.get("production_core_gate") != "BLOCKED":
        errors.append("architecture inventory must remain fail closed")
    projected_credential = inventory.get("credential_authority") if isinstance(inventory.get("credential_authority"), dict) else {}
    if projected_credential != credential_authority:
        errors.append("architecture inventory credential_authority must be the exact generated projection of AR-8B source authority")

    inventory_cleanup = inventory.get("runtime_authority_cleanup") if isinstance(inventory.get("runtime_authority_cleanup"), dict) else {}
    if (
        inventory_cleanup.get("status") != "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP"
        or inventory_cleanup.get("evidence") != AR5_EVIDENCE.as_posix()
        or inventory_cleanup.get("implementation_pr") != 291
        or inventory_cleanup.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"
        or inventory_cleanup.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"
        or inventory_cleanup.get("next_required_slice") != "AR-6"
        or inventory_cleanup.get("production_mutation") is not False
    ):
        errors.append("architecture inventory AR-5 runtime-authority cleanup projection drifted")
    inventory_python_ops = inventory.get("python_operational_authority") if isinstance(inventory.get("python_operational_authority"), dict) else {}
    if (
        inventory_python_ops.get("status") != "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION"
        or inventory_python_ops.get("evidence") != AR6_EVIDENCE.as_posix()
        or inventory_python_ops.get("python_estate") != PYTHON_ESTATE.as_posix()
        or inventory_python_ops.get("implementation_pr") != 295
        or inventory_python_ops.get("exact_green_head") != "9b06d542873ffa3122e53e107105098e21f5933c"
        or inventory_python_ops.get("implementation_merge") != "d0229fedd81ee870822b6d9394bc4ee313ea3a3c"
        or inventory_python_ops.get("next_required_slice") != "AR-7"
        or inventory_python_ops.get("production_mutation") is not False
    ):
        errors.append("architecture inventory AR-6 Python/opsctl projection drifted")

    inventory_governance = inventory.get("github_governance_authority") if isinstance(inventory.get("github_governance_authority"), dict) else {}
    inventory_hosted = inventory_governance.get("hosted_audit") if isinstance(inventory_governance.get("hosted_audit"), dict) else {}
    if (
        inventory_governance.get("status") != "ACCEPTED_AR7_GITHUB_GOVERNANCE"
        or inventory_governance.get("evidence") != AR7_EVIDENCE.as_posix()
        or inventory_governance.get("contract") != GOVERNANCE_CONTRACT.as_posix()
        or inventory_governance.get("implementation_pr") != 299
        or inventory_governance.get("exact_green_head") != "1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7"
        or inventory_governance.get("implementation_merge") != "3492273cb9237850e3fa27343cc5edbdb0f66aa1"
        or inventory_governance.get("applicable_permanent_workflows") != "14/14"
        or inventory_hosted.get("run_id") != 31953316327
        or inventory_hosted.get("hosted_state_job") != "success"
        or inventory_governance.get("next_required_slice") != "AR-8"
        or inventory_governance.get("production_mutation") is not False
    ):
        errors.append("architecture inventory AR-7 GitHub governance projection drifted")

    common = ("Architecture Re-baseline v3", "issue #266", "AR-7", "AR-8", "production_ready=false")
    require(root_readme, common, "README.md", errors)
    require(docs_readme, common, "docs/README.md", errors)
    require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-7", "AR-8"), "docs/INDEX.md", errors)
    require(development, ("Document status:** GENERATED_PROJECTION", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "AR-7   Environments + GitHub Governance + Operational Boundaries", "AR-8   Secrets / Keys / OAuth Refresh Concurrency", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)
    require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-7", "Next slice:** AR-8", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)
    require(ar2_evidence, ("AR-2 Runtime Topology + D3 Compatibility", "GENERATION_VERIFICATION = DELETE", "legacy D3 production lane", "AR-5", "AR-11", "PC-1"), "AR-2 evidence", errors)
    require(ar3_evidence, ("AR-3 Application Architecture Contract", "EVIDENCE / AR-3 accepted", "AR-4A", "AR-4B", "AR-4C", "NOT_REQUIRED", "architecture/inventory.json"), "AR-3 evidence", errors)
    require(ar4a_evidence, ("AR-4A Composition-root consolidation", "EVIDENCE / AR-4A accepted", "f257a30a1df437812edb5c9e4b33c3de7e0740bc", "74672285ef0146c2dc6da298024b378438e5a75d", "AR-4B", "AR-4C", "Production Core remains `BLOCKED`"), "AR-4A evidence", errors)
    require(ar4b_evidence, ("AR-4B Client Mail route ownership", "EVIDENCE / AR-4B accepted", "7ccdd1b0ed0c0eae974cd9bde15c87524315c023", "04b62c97813010ac283d8b70c81089f1c16f5672", "AR-4C", "Production Core remains `BLOCKED`"), "AR-4B evidence", errors)
    require(ar4c_evidence, ("AR-4C Outbound Mail composition extraction", "EVIDENCE / AR-4C accepted", "c62f3a7fb00acf16fa1a8a00d9d2f101949cf8a3", "d8382d1578c4911287fb76dd0b9966b23aa85c25", "AR-5", "Production Core remains `BLOCKED`"), "AR-4C evidence", errors)
    require(ar5_evidence, ("AR-5 Wrangler / Runtime Authority Cleanup", "EVIDENCE / AR-5 accepted", "afed435bb714794d6c4f252be6b44c592ee31b2b", "82d251a1d6666199c6eace393eedc1766157fcee", "13/13 success", "AR-6", "Production Core remains `BLOCKED`"), "AR-5 evidence", errors)
    require(ar6_evidence, ("AR-6 Full Python Estate + read-only Rust opsctl", "EVIDENCE / AR-6 accepted", "9b06d542873ffa3122e53e107105098e21f5933c", "d0229fedd81ee870822b6d9394bc4ee313ea3a3c", "13/13 success", "108", "AR-7", "production_core_gate = BLOCKED"), "AR-6 evidence", errors)
    require(ar7_evidence, ("AR-7 — Environments + GitHub Governance + Operational Boundaries", "EVIDENCE / AR-7 accepted", "1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7", "3492273cb9237850e3fa27343cc5edbdb0f66aa1", "14/14 success", "31953316327", "HTTP 409", "AR-8", "production_core_gate = BLOCKED"), "AR-7 evidence", errors)
    require(pre2j_stub, ("ACCEPTED_HISTORICAL", "SUPERSEDED_FOR_FORWARD_EXECUTION", "Former tracking issue:** #203", "Current program authority"), "pre2J compatibility stub", errors)
    require(implementation_stub, ("Document status:** SUPERSEDED", "history/IMPLEMENTATION_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"), "IMPLEMENTATION_PLAN.md", errors)
    require(lifecycle_stub, ("Document status:** SUPERSEDED", "history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md", "ARCHITECTURE_REBASELINE_V3_PLAN.md"), "PROFILE_LIFECYCLE_PLAN.md", errors)
    return errors


def copy_fixture(source_root: Path, target_root: Path) -> None:
    for relative in REQUIRED_FILES:
        source = source_root / relative
        target = target_root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def mutate(path: Path, old: str, new: str) -> bool:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        return False
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
    return True


def self_test(root: Path) -> bool:
    baseline = validate(root)
    if baseline:
        print("documentation-authority self-test requires a valid baseline")
        for error in baseline:
            print(error)
        return False
    fixtures = [
        ("tracking rollback", Path("docs/status.json"), '"tracking_issue": 266', '"tracking_issue": 203', "tracking_issue"),
        ("active slice rollback", Path("docs/status.json"), '"current_slice": "AR-8"', '"current_slice": "AR-7"', "current_slice"),
        ("AR-8 subslice skip", Path("docs/status.json"), '"current_subslice": "AR-8B"', '"current_subslice": "AR-8C"', "current_subslice"),
        ("premature AR-8 acceptance", Path("docs/status.json"), '"full_ar8_accepted": false', '"full_ar8_accepted": true', "full_ar8_accepted"),
        ("premature AR-9 unblock", Path("docs/status.json"), '"ar9_blocked": true', '"ar9_blocked": false', "ar9_blocked"),
        ("premature architecture closeout", Path("docs/status.json"), '"architecture_complete": false', '"architecture_complete": true', "architecture_complete"),
        ("premature gate authorization", Path("docs/status.json"), '"production_core_gate": "BLOCKED"', '"production_core_gate": "AUTHORIZED"', "Production"),
        ("premature production readiness", Path("docs/status.json"), '"production_ready": false', '"production_ready": true', "production_ready"),
        ("generation queue resurrection", TOPOLOGY, '"decision": "DELETE"', '"decision": "KEEP"', "GENERATION_VERIFICATION"),
        ("legacy D3 production resurrection", TOPOLOGY, '"legacy_d3_production_lane": "DISABLE_FORWARD_EXECUTION"', '"legacy_d3_production_lane": "KEEP"', "D3"),
        ("historical #203 resurrected", Path("docs/status.json"), '"forward_execution_authority": false', '"forward_execution_authority": true', "#203"),
    ]
    for label, relative, old, new, expected in fixtures:
        with tempfile.TemporaryDirectory(prefix="ar3-document-authority-") as directory:
            fixture = Path(directory)
            copy_fixture(root, fixture)
            path = fixture / relative
            if not mutate(path, old, new):
                print(f"negative fixture source marker missing for {label}: {old}")
                return False
            errors = validate(fixture)
            if not errors or not any(expected.lower() in error.lower() for error in errors):
                print(f"negative fixture {label} was not rejected by the expected invariant: {errors}")
                return False
    print("Architecture Re-baseline v3 active AR-8 / current AR-8B documentation authority negative fixtures passed.")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    if args.self_test:
        return 0 if self_test(root) else 1
    errors = validate(root)
    if errors:
        for error in errors:
            print(error)
        return 1
    print("Architecture Re-baseline v3 active AR-8 / current AR-8B documentation/program authority is consistent.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
