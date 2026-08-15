#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    p = ROOT / path
    text = p.read_text(encoding="utf-8")
    observed = text.count(old)
    if observed < count:
        raise SystemExit(f"{path}: expected at least {count} occurrence(s) of {old!r}, found {observed}")
    p.write_text(text.replace(old, new, count), encoding="utf-8", newline="\n")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def already_closed() -> bool:
    plan = (ROOT / "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md").read_text(encoding="utf-8")
    return (
        "**Current accepted architecture checkpoint:** AR-3 — Application Architecture Contract" in plan
        and "**Next slice:** AR-4A — Composition-root consolidation" in plan
    )


def transform() -> None:
    # Single current authority.
    p = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
    replace(p, "**Current accepted architecture checkpoint:** AR-2 — Runtime Topology + D3 Compatibility", "**Current accepted architecture checkpoint:** AR-3 — Application Architecture Contract")
    replace(p, "**Next slice:** AR-3 — Application Architecture Contract", "**Next slice:** AR-4A — Composition-root consolidation")
    replace(
        p,
        "AR-2 is the latest accepted checkpoint; its normalized runtime-topology decision is `architecture/runtime-topology-ar2.json` with acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR2.md`.",
        "AR-3 is the latest accepted checkpoint. Its application/runtime ownership contract is projected in `architecture/inventory.json` with acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`.",
    )
    replace(
        p,
        "AR-2   Runtime Topology + D3 Compatibility                       CURRENT / ACCEPTED CHECKPOINT\nAR-3   Application Architecture Contract                         NEXT\nAR-4A  Composition-root consolidation",
        "AR-2   Runtime Topology + D3 Compatibility                       DONE / ACCEPTED\nAR-3   Application Architecture Contract                         CURRENT / ACCEPTED CHECKPOINT\nAR-4A  Composition-root consolidation                            NEXT",
    )
    replace(p, "AR-4D  Profile extraction only if AR-3 proves benefit", "AR-4D  Profile extraction — NOT REQUIRED by AR-3; reopen only by later accepted evidence")

    # Accepted AR-3 evidence must stop calling itself a candidate.
    p = "docs/ARCHITECTURE_REBASELINE_V3_AR3.md"
    replace(p, "**Document status:** EVIDENCE / AR-3 candidate", "**Document status:** EVIDENCE / AR-3 accepted")
    replace(
        p,
        "**Exact baseline:** `3c592e98a0435388119f5b224a864b8f0d649379`  \n**Production mutation:** forbidden",
        "**Exact baseline:** `3c592e98a0435388119f5b224a864b8f0d649379`  \n**Exact-green implementation candidate:** `f26726a5892e660940dffab7bce5615c3f13eb87`  \n**Accepted implementation merge:** `2b7e7ec828b7d29209b97adb5100b1c2559c73f0`  \n**Implementation PR:** #276 — 13/13 applicable permanent PR workflows passed on the unchanged exact head  \n**Production mutation:** forbidden",
    )

    # Human program projection.
    p = "docs/DEVELOPMENT_PLAN.md"
    replace(p, "**Current accepted architecture checkpoint:** AR-2 — Runtime Topology + D3 Compatibility", "**Current accepted architecture checkpoint:** AR-3 — Application Architecture Contract")
    replace(p, "**Next architecture slice:** AR-3 — Application Architecture Contract", "**Next architecture slice:** AR-4A — Composition-root consolidation")
    replace(
        p,
        "- AR-2 — Runtime Topology + D3 Compatibility: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-3 — Application Architecture Contract: **NEXT**.\n- AR-4A…AR-17: ordered future architecture slices.",
        "- AR-2 — Runtime Topology + D3 Compatibility: **DONE / ACCEPTED**.\n- AR-3 — Application Architecture Contract: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-4A — Composition-root consolidation: **NEXT**.\n- AR-4B…AR-17: ordered future architecture slices; AR-4D is skipped unless later accepted evidence reopens it.",
    )
    replace(
        p,
        "AR-2   Runtime Topology + D3 Compatibility                       CURRENT / ACCEPTED CHECKPOINT\nAR-3   Application Architecture Contract                         NEXT\nAR-4A  Composition-root consolidation",
        "AR-2   Runtime Topology + D3 Compatibility                       DONE / ACCEPTED\nAR-3   Application Architecture Contract                         CURRENT / ACCEPTED CHECKPOINT\nAR-4A  Composition-root consolidation                            NEXT",
    )
    replace(p, "AR-4D  Profile extraction only if AR-3 proves benefit", "AR-4D  Profile extraction — NOT REQUIRED by AR-3; reopen only by later accepted evidence")

    # Root/docs navigation projections.
    p = "README.md"
    replace(p, "- **Accepted architecture slices:** AR-0, AR-1 and AR-2.\n- **Current accepted checkpoint:** AR-2 — Runtime Topology + D3 Compatibility.\n- **Next slice:** AR-3 — Application Architecture Contract.", "- **Accepted architecture slices:** AR-0, AR-1, AR-2 and AR-3.\n- **Current accepted checkpoint:** AR-3 — Application Architecture Contract.\n- **Next slice:** AR-4A — Composition-root consolidation.")
    replace(p, "#266. AR-2 runtime-topology decisions are recorded in\n[`architecture/runtime-topology-ar2.json`](architecture/runtime-topology-ar2.json), with evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR2.md`](docs/ARCHITECTURE_REBASELINE_V3_AR2.md). Machine transition", "#266. AR-3 application/runtime ownership is projected in\n[`architecture/inventory.json`](architecture/inventory.json), with evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md); the accepted AR-2 topology input remains\n[`architecture/runtime-topology-ar2.json`](architecture/runtime-topology-ar2.json). Machine transition")
    replace(p, "- [`architecture/runtime-topology-ar2.json`](architecture/runtime-topology-ar2.json) — accepted AR-2\n  topology/D3 compatibility decision input for AR-3;", "- [`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 application architecture evidence;\n- [`architecture/runtime-topology-ar2.json`](architecture/runtime-topology-ar2.json) — accepted AR-2 topology/D3 input retained by the AR-3 projection;")

    p = "docs/README.md"
    replace(p, "- **Accepted architecture slices:** AR-0, AR-1 and AR-2.\n- **Current accepted checkpoint:** AR-2 — Runtime Topology + D3 Compatibility.\n- **Next slice:** AR-3 — Application Architecture Contract.", "- **Accepted architecture slices:** AR-0, AR-1, AR-2 and AR-3.\n- **Current accepted checkpoint:** AR-3 — Application Architecture Contract.\n- **Next slice:** AR-4A — Composition-root consolidation.")
    replace(p, "- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md) — AR-2 acceptance evidence;\n- [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json) — accepted AR-2 topology/D3 decision input for AR-3;", "- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 application architecture evidence;\n- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md) — accepted AR-2 topology/D3 evidence;\n- [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json) — accepted AR-2 topology/D3 decision input retained by AR-3;")
    replace(p, "preserving its repository-side D3 foundation; AR-3 is the only next architecture slice.", "preserving its repository-side D3 foundation; AR-3 is accepted and AR-4A is the only next architecture slice.")

    p = "docs/INDEX.md"
    replace(p, "- AR-0, AR-1 and AR-2 are accepted checkpoints.\n- AR-2 — Runtime Topology + D3 Compatibility is the current accepted checkpoint.\n- AR-3 — Application Architecture Contract is the only next slice.", "- AR-0, AR-1, AR-2 and AR-3 are accepted checkpoints.\n- AR-3 — Application Architecture Contract is the current accepted checkpoint.\n- AR-4A — Composition-root consolidation is the only next slice.")
    replace(p, "accepted AR-2 topology/D3 decision input; AR-3 owns its canonical runtime-resource projection.", "accepted AR-2 topology/D3 decision input retained by the accepted AR-3 application/runtime ownership projection.")
    replace(p, "- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md)\n- [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json)", "- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md)\n- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md)\n- [`../architecture/runtime-topology-ar2.json`](../architecture/runtime-topology-ar2.json)")

    # Machine status projection.
    status_path = ROOT / "docs/status.json"
    status = json.loads(status_path.read_text(encoding="utf-8"))
    program = status["current"]["architecture_program"]
    program["accepted_slices"] = ["AR-0", "AR-1", "AR-2", "AR-3"]
    program["current_slice"] = "AR-3"
    program["next_slice_after_acceptance"] = "AR-4A"
    program["application_architecture_evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR3.md"
    program["application_architecture_projection"] = "architecture/inventory.json::application_architecture"
    status["current"]["predecessor_external_d3"]["current_state"] = "closed_not_planned_after_ar2_acceptance"
    status["next_repository_step"] = {
        "number": None,
        "name": "Architecture Re-baseline v3 AR-4A",
        "status": "next_after_accepted_ar3_merge",
        "tracking_issue": 266,
        "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
    }
    status["implementation"]["architecture_rebaseline_v3"] = "active_issue_266_ar3_accepted_next_ar4a"
    status_path.write_text(json.dumps(status, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

    # Machine transition projection.
    transition_path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
    transition = json.loads(transition_path.read_text(encoding="utf-8"))
    transition["status"] = "ACTIVE_AFTER_ACCEPTED_AR3_MERGE"
    transition["accepted_slices"] = ["AR-0", "AR-1", "AR-2", "AR-3"]
    transition["current_slice"] = "AR-3"
    transition["next_slice_after_acceptance"] = "AR-4A"
    transition["issue_relationships"]["pre2j_d3_external_issue_251"]["current_state_after_ar2_closeout"] = "closed_not_planned"
    transition["application_architecture"] = {
        "canonical_projection": "architecture/inventory.json::application_architecture",
        "evidence": "docs/ARCHITECTURE_REBASELINE_V3_AR3.md",
        "status": "ACCEPTED",
        "ar4a": "NEXT_REQUIRED_SLICE",
        "ar4b": "ROUTE_OWNERSHIP_DEBT",
        "ar4c": "OUTBOUND_MAIL_COMPOSITION_DEBT",
        "ar4d": "NOT_REQUIRED_UNLESS_LATER_ACCEPTED_EVIDENCE_REOPENS",
        "production_mutation": False,
    }
    sequence = transition["binding_sequence_model"]["architecture_program"]
    sequence[sequence.index("AR-4D_IF_AR3_PROVES_BENEFIT")] = "AR-4D_NOT_REQUIRED_UNLESS_LATER_ACCEPTED_EVIDENCE_REOPENS"
    transition["architecture_inventory_policy"]["ar3_projection_ownership"] = "ACCEPTED_APPLICATION_ARCHITECTURE_CONTRACT_IN_CANONICAL_INVENTORY"
    transition_path.write_text(json.dumps(transition, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

    # Canonical inventory generator source moves from candidate sequencing to accepted AR-3 sequencing.
    p = "scripts/generate-architecture-inventory.py"
    replace(p, 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2"]\nCURRENT_SLICE = "AR-2"\nNEXT_SLICE = "AR-3"', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3"]\nCURRENT_SLICE = "AR-3"\nNEXT_SLICE = "AR-4A"')
    replace(p, '"scope": "ar3_application_architecture_contract_candidate"', '"scope": "ar3_application_architecture_contract_accepted"')
    replace(p, "must preserve accepted AR-2 -> active AR-3 sequencing until AR-3 acceptance", "must project accepted AR-3 -> active AR-4A sequencing after AR-3 closeout")

    # AR-3 projection itself becomes accepted and rejects regression to candidate status.
    p = "scripts/_ar3_application_architecture.py"
    replace(p, '"status": "AR3_APPLICATION_ARCHITECTURE_CONTRACT",', '"status": "ACCEPTED_AR3_APPLICATION_ARCHITECTURE_CONTRACT",')
    marker = '    expected = build_projection(root)\n\n'
    insertion = marker + '    candidate_status = copy.deepcopy(expected)\n    candidate_status["status"] = "AR3_APPLICATION_ARCHITECTURE_CONTRACT"\n    if candidate_status == expected:\n        raise SystemExit("AR-3 negative self-test failed to detect candidate-status regression")\n\n'
    replace(p, marker, insertion)

    # Documentation authority checker moves to AR-3 accepted / AR-4A next.
    p = "scripts/check-documentation-authority.py"
    replace(p, 'CURRENT_SLICE = "AR-2"\nNEXT_SLICE = "AR-3"', 'CURRENT_SLICE = "AR-3"\nNEXT_SLICE = "AR-4A"')
    replace(p, 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2"]', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3"]')
    replace(p, 'AR2_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR2.md")', 'AR2_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR2.md")\nAR3_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR3.md")')
    replace(p, "    AR2_EVIDENCE,\n    Path(\"docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md\"),", "    AR2_EVIDENCE,\n    AR3_EVIDENCE,\n    Path(\"docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md\"),")
    replace(p, "        ar2_evidence = read(root, AR2_EVIDENCE)\n        pre2j_stub", "        ar2_evidence = read(root, AR2_EVIDENCE)\n        ar3_evidence = read(root, AR3_EVIDENCE)\n        pre2j_stub")
    replace(p, "docs/status.json must be the AR-2 schema/date projection", "docs/status.json must be the current AR-3 schema/date projection")
    replace(p, "production_ready must remain false throughout AR-2", "production_ready must remain false throughout accepted AR-3")
    replace(p, "AR-2 architecture_complete/Production Core gate state must remain fail closed", "AR-3 architecture_complete/Production Core gate state must remain fail closed")
    replace(p, 'if predecessor.get("legacy_production_lane") != "DISABLED_BY_AR2":\n        errors.append("legacy D3 production lane must be disabled after AR-2")\n', 'if predecessor.get("legacy_production_lane") != "DISABLED_BY_AR2":\n        errors.append("legacy D3 production lane must be disabled after AR-2")\n    if predecessor.get("current_state") != "closed_not_planned_after_ar2_acceptance":\n        errors.append("issue #251 must remain closed not_planned after AR-2 acceptance")\n')
    replace(p, 'if transition.get("schema_version") != 7 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR2_MERGE":\n        errors.append("architecture transition must encode accepted AR-2 state")', 'if transition.get("schema_version") != 7 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR3_MERGE":\n        errors.append("architecture transition must encode accepted AR-3 state")')
    replace(p, "architecture transition must encode AR-2 -> AR-3 sequencing", "architecture transition must encode AR-3 -> AR-4A sequencing")
    replace(p, "transition state must remain fail closed through AR-2", "transition state must remain fail closed through AR-3")
    replace(p, "architecture inventory AR-2 program state is stale", "architecture inventory AR-3 program state is stale")
    replace(p, 'common = ("Architecture Re-baseline v3", "issue #266", "AR-2", "AR-3", "production_ready=false")', 'common = ("Architecture Re-baseline v3", "issue #266", "AR-3", "AR-4A", "production_ready=false")')
    replace(p, 'require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-2", "AR-3"), "docs/INDEX.md", errors)', 'require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-3", "AR-4A"), "docs/INDEX.md", errors)')
    replace(p, 'require(development, ("Document status:** GENERATED_PROJECTION", "AR-1   Architecture Authority Re-baseline", "AR-2   Runtime Topology + D3 Compatibility", "AR-3   Application Architecture Contract", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)', 'require(development, ("Document status:** GENERATED_PROJECTION", "AR-2   Runtime Topology + D3 Compatibility", "AR-3   Application Architecture Contract", "AR-4A  Composition-root consolidation", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)')
    replace(p, 'require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-2", "AR-2   Runtime Topology + D3 Compatibility", "AR-3   Application Architecture Contract", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)', 'require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-3", "Next slice:** AR-4A", "AR-3   Application Architecture Contract", "AR-4A  Composition-root consolidation", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)')
    replace(p, 'require(ar2_evidence, ("AR-2 Runtime Topology + D3 Compatibility", "GENERATION_VERIFICATION = DELETE", "legacy D3 production lane", "AR-5", "AR-11", "PC-1"), "AR-2 evidence", errors)', 'require(ar2_evidence, ("AR-2 Runtime Topology + D3 Compatibility", "GENERATION_VERIFICATION = DELETE", "legacy D3 production lane", "AR-5", "AR-11", "PC-1"), "AR-2 evidence", errors)\n    require(ar3_evidence, ("AR-3 Application Architecture Contract", "EVIDENCE / AR-3 accepted", "AR-4A", "AR-4B", "AR-4C", "NOT_REQUIRED", "architecture/inventory.json"), "AR-3 evidence", errors)')
    replace(p, '("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-2"\', \'"current_slice": "AR-1"\', "current_slice")', '("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-3"\', \'"current_slice": "AR-2"\', "current_slice")')
    replace(p, 'prefix="ar2-document-authority-"', 'prefix="ar3-document-authority-"')
    replace(p, 'print("Architecture Re-baseline v3 AR-2 documentation authority negative fixtures passed.")', 'print("Architecture Re-baseline v3 AR-3 documentation authority negative fixtures passed.")')
    replace(p, 'print("Architecture Re-baseline v3 AR-2 documentation/program authority is consistent.")', 'print("Architecture Re-baseline v3 AR-3 documentation/program authority is consistent.")')

    # Break the write/check bootstrap cycle only by pre-seeding fields that the checker reads.
    # The repository-owned generator immediately rewrites the complete canonical inventory afterwards.
    inventory_path = ROOT / "architecture/inventory.json"
    inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
    inventory["documentation_authority"]["current_slice"] = "AR-3"
    inventory["program_state"]["accepted_architecture_slices"] = ["AR-0", "AR-1", "AR-2", "AR-3"]
    inventory["program_state"]["current_architecture_slice"] = "AR-3"
    inventory["program_state"]["next_architecture_slice_after_acceptance"] = "AR-4A"
    inventory_path.write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def verify() -> None:
    run(sys.executable, "scripts/generate-architecture-inventory.py", "--write")
    run(sys.executable, "scripts/check-documentation-authority.py", "--root", ".")
    run(sys.executable, "scripts/check-documentation-authority.py", "--root", ".", "--self-test")
    run(sys.executable, "scripts/generate-architecture-inventory.py", "--check")
    run(sys.executable, "scripts/generate-architecture-inventory.py", "--self-test")
    run(sys.executable, "-m", "json.tool", "architecture/inventory.json")
    run(sys.executable, "-m", "json.tool", "architecture/architecture-rebaseline-v3-transition.json")
    run(sys.executable, "-m", "json.tool", "docs/status.json")
    run("git", "diff", "--check")


def main() -> int:
    if not already_closed():
        transform()
    verify()
    print("AR-3 authority closeout projection verified: accepted AR-3 -> AR-4A next.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
