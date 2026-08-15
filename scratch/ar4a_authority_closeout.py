#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def exact(path: str, old: str, new: str, count: int = 1) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    observed = text.count(old)
    if observed != count:
        raise SystemExit(f"{path}: expected exactly {count} occurrence(s) of {old!r}, found {observed}")
    target.write_text(text.replace(old, new, count), encoding="utf-8", newline="\n")


def replace_all(path: str, old: str, new: str, expected: int) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    observed = text.count(old)
    if observed != expected:
        raise SystemExit(f"{path}: expected exactly {expected} occurrence(s) of {old!r}, found {observed}")
    target.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def project_human_docs() -> None:
    p = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
    exact(p, "**Current accepted architecture checkpoint:** AR-3 — Application Architecture Contract", "**Current accepted architecture checkpoint:** AR-4A — Composition-root consolidation")
    exact(p, "**Next slice:** AR-4A — Composition-root consolidation", "**Next slice:** AR-4B — Client Mail route ownership")
    exact(
        p,
        "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-3 is the latest accepted checkpoint. Its application/runtime ownership contract is projected in `architecture/inventory.json` with acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`.",
        "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-4A is the latest accepted checkpoint. Its composition-root remediation extends the accepted AR-3 application/runtime ownership contract in `architecture/inventory.json`, with AR-4A acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR4A.md` and the AR-3 base contract preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`.",
    )
    exact(
        p,
        "AR-3   Application Architecture Contract                         CURRENT / ACCEPTED CHECKPOINT\nAR-4A  Composition-root consolidation                            NEXT\nAR-4B  Client Mail route ownership",
        "AR-3   Application Architecture Contract                         DONE / ACCEPTED\nAR-4A  Composition-root consolidation                            CURRENT / ACCEPTED CHECKPOINT\nAR-4B  Client Mail route ownership                               NEXT",
    )

    p = "docs/DEVELOPMENT_PLAN.md"
    exact(p, "**Current accepted architecture checkpoint:** AR-3 — Application Architecture Contract", "**Current accepted architecture checkpoint:** AR-4A — Composition-root consolidation")
    exact(p, "**Next architecture slice:** AR-4A — Composition-root consolidation", "**Next architecture slice:** AR-4B — Client Mail route ownership")
    exact(
        p,
        "- AR-3 — Application Architecture Contract: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-4A — Composition-root consolidation: **NEXT**.\n- AR-4B…AR-17: ordered future architecture slices; AR-4D is skipped unless later accepted evidence reopens it.",
        "- AR-3 — Application Architecture Contract: **DONE / ACCEPTED**.\n- AR-4A — Composition-root consolidation: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-4B — Client Mail route ownership: **NEXT**.\n- AR-4C…AR-17: ordered future architecture slices; AR-4D is skipped unless later accepted evidence reopens it.",
    )
    exact(
        p,
        "AR-3   Application Architecture Contract                         CURRENT / ACCEPTED CHECKPOINT\nAR-4A  Composition-root consolidation                            NEXT\nAR-4B  Client Mail route ownership",
        "AR-3   Application Architecture Contract                         DONE / ACCEPTED\nAR-4A  Composition-root consolidation                            CURRENT / ACCEPTED CHECKPOINT\nAR-4B  Client Mail route ownership                               NEXT",
    )

    p = "README.md"
    exact(
        p,
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2 and AR-3.\n- **Current accepted checkpoint:** AR-3 — Application Architecture Contract.\n- **Next slice:** AR-4A — Composition-root consolidation.",
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3 and AR-4A.\n- **Current accepted checkpoint:** AR-4A — Composition-root consolidation.\n- **Next slice:** AR-4B — Client Mail route ownership.",
    )
    exact(
        p,
        "#266. AR-3 application/runtime ownership is projected in\n[`architecture/inventory.json`](architecture/inventory.json), with evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md); the accepted AR-2 topology input remains",
        "#266. AR-4A composition-root remediation extends the accepted AR-3 application/runtime ownership contract in\n[`architecture/inventory.json`](architecture/inventory.json), with acceptance evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md) and the AR-3 base contract preserved in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md); the accepted AR-2 topology input remains",
    )
    exact(
        p,
        "- [`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 application architecture evidence;",
        "- [`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;\n- [`docs/ARCHITECTURE_REBASELINE_V3_AR3.md`](docs/ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 base application architecture evidence;",
    )

    p = "docs/README.md"
    exact(
        p,
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2 and AR-3.\n- **Current accepted checkpoint:** AR-3 — Application Architecture Contract.\n- **Next slice:** AR-4A — Composition-root consolidation.",
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3 and AR-4A.\n- **Current accepted checkpoint:** AR-4A — Composition-root consolidation.\n- **Next slice:** AR-4B — Client Mail route ownership.",
    )
    exact(
        p,
        "- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 application architecture evidence;",
        "- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;\n- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md) — accepted AR-3 base application architecture evidence;",
    )
    exact(
        p,
        "preserving its repository-side D3 foundation; AR-3 is accepted and AR-4A is the only next architecture slice.",
        "preserving its repository-side D3 foundation; AR-4A is accepted and AR-4B is the only next architecture slice.",
    )

    p = "docs/INDEX.md"
    exact(
        p,
        "- AR-0, AR-1, AR-2 and AR-3 are accepted checkpoints.\n- AR-3 — Application Architecture Contract is the current accepted checkpoint.\n- AR-4A — Composition-root consolidation is the only next slice.",
        "- AR-0, AR-1, AR-2, AR-3 and AR-4A are accepted checkpoints.\n- AR-4A — Composition-root consolidation is the current accepted checkpoint.\n- AR-4B — Client Mail route ownership is the only next slice.",
    )
    exact(
        p,
        "accepted AR-2 topology/D3 decision input retained by the accepted AR-3 application/runtime ownership projection.",
        "accepted AR-2 topology/D3 decision input retained by the AR-4A-remediated accepted AR-3 application/runtime ownership projection.",
    )
    exact(
        p,
        "- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md)\n- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md)",
        "- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md)\n- [`ARCHITECTURE_REBASELINE_V3_AR3.md`](ARCHITECTURE_REBASELINE_V3_AR3.md)\n- [`ARCHITECTURE_REBASELINE_V3_AR2.md`](ARCHITECTURE_REBASELINE_V3_AR2.md)",
    )


def project_ar4a_evidence() -> None:
    p = "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md"
    exact(p, "**Document status:** EVIDENCE / AR-4A candidate", "**Document status:** EVIDENCE / AR-4A accepted")
    exact(
        p,
        "**Exact accepted baseline:** `681831e23fc901d057553815a4de9f527f3c0d08`\n**Production mutation:** forbidden",
        "**Exact accepted baseline:** `681831e23fc901d057553815a4de9f527f3c0d08`\n**Exact-green implementation candidate:** `f257a30a1df437812edb5c9e4b33c3de7e0740bc`\n**Accepted implementation merge:** `74672285ef0146c2dc6da298024b378438e5a75d`\n**Implementation PR:** #280 — 13/13 applicable permanent PR workflows passed on the unchanged exact head\n**Production mutation:** forbidden",
    )
    exact(
        p,
        "The accepted AR-3 application architecture contract remains the authority while this document is a candidate. Acceptance is projected only after an exact-green guarded merge and mandatory post-merge authority closeout.",
        "The accepted AR-3 application architecture contract remains the base contract. AR-4A is accepted as its composition-root remediation after exact-green guarded merge and post-merge authority closeout; AR-4B is the next required slice.",
    )
    exact(p, "| Transport | Before AR-4A | Candidate composition seam |", "| Transport | Before AR-4A | Accepted composition seam |")
    exact(p, "## 5. Candidate exit criteria", "## 5. Acceptance record")
    exact(p, "- AR-4A-owned transport construction debt is projected as a candidate closure while AR-4B/AR-4C remain open;", "- AR-4A-owned transport construction debt is accepted as closed while AR-4B/AR-4C remain open;")
    exact(p, "- after guarded merge, post-merge authority closeout must mark AR-4A accepted and AR-4B next before AR-4B begins.", "- post-merge authority projects AR-4A accepted and AR-4B next before AR-4B begins.")


def project_json_state() -> None:
    path = ROOT / "docs/status.json"
    status = json.loads(path.read_text(encoding="utf-8"))
    status["as_of"] = "2026-08-16"
    program = status["current"]["architecture_program"]
    program["accepted_slices"] = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A"]
    program["current_slice"] = "AR-4A"
    program["next_slice_after_acceptance"] = "AR-4B"
    program["application_architecture_evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md"
    program["application_architecture_base_evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR3.md"
    program["ar4a_acceptance"] = {
        "issue": 279,
        "implementation_pr": 280,
        "exact_green_head": "f257a30a1df437812edb5c9e4b33c3de7e0740bc",
        "implementation_merge": "74672285ef0146c2dc6da298024b378438e5a75d",
        "applicable_permanent_workflows": "13/13",
    }
    status["next_repository_step"] = {
        "number": None,
        "name": "Architecture Re-baseline v3 AR-4B",
        "status": "next_after_accepted_ar4a_merge",
        "tracking_issue": 266,
        "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
    }
    status["implementation"]["architecture_rebaseline_v3"] = "active_issue_266_ar4a_accepted_next_ar4b"
    path.write_text(json.dumps(status, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

    path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
    transition = json.loads(path.read_text(encoding="utf-8"))
    transition["status"] = "ACTIVE_AFTER_ACCEPTED_AR4A_MERGE"
    transition["accepted_slices"] = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A"]
    transition["current_slice"] = "AR-4A"
    transition["next_slice_after_acceptance"] = "AR-4B"
    transition["application_architecture"] = {
        "canonical_projection": "architecture/inventory.json::application_architecture",
        "base_contract_evidence": "docs/ARCHITECTURE_REBASELINE_V3_AR3.md",
        "evidence": "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md",
        "status": "ACCEPTED_THROUGH_AR4A",
        "ar4a": "ACCEPTED_COMPOSITION_ROOT_CONSOLIDATION",
        "ar4b": "NEXT_REQUIRED_SLICE_ROUTE_OWNERSHIP_DEBT",
        "ar4c": "OUTBOUND_MAIL_COMPOSITION_DEBT",
        "ar4d": "NOT_REQUIRED_UNLESS_LATER_ACCEPTED_EVIDENCE_REOPENS",
        "implementation_pr": 280,
        "exact_green_head": "f257a30a1df437812edb5c9e4b33c3de7e0740bc",
        "implementation_merge": "74672285ef0146c2dc6da298024b378438e5a75d",
        "production_mutation": False,
    }
    transition["architecture_inventory_policy"]["ar4a_remediation"] = "ACCEPTED_COMPOSITION_ROOT_CONSOLIDATION_IN_CANONICAL_INVENTORY"
    path.write_text(json.dumps(transition, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def project_application_generator() -> None:
    p = "scripts/_ar3_application_architecture.py"
    exact(p, '"status": "CENTRAL_COMPOSITION_ROOT_AR4A_CANDIDATE",', '"status": "CENTRAL_COMPOSITION_ROOT_AR4A_ACCEPTED",')
    replace_all(p, '"status": "AR4A_CENTRALIZED_COMPOSITION_CANDIDATE",', '"status": "AR4A_CENTRALIZED_COMPOSITION_ACCEPTED",', 4)
    exact(p, '"status": "AR4A_COMPOSITION_ROOT_CONSOLIDATION_CANDIDATE",', '"status": "AR4A_COMPOSITION_ROOT_CONSOLIDATION_ACCEPTED",')
    exact(p, '            "AR4A_CENTRALIZED_COMPOSITION_CANDIDATE",', '            "AR4A_CENTRALIZED_COMPOSITION_ACCEPTED",')
    exact(
        p,
        '        "status": "ACCEPTED_AR3_APPLICATION_ARCHITECTURE_CONTRACT",\n        "topology_source": RUNTIME_TOPOLOGY,\n        "evidence": AR3_EVIDENCE,',
        '        "status": "ACCEPTED_AR4A_APPLICATION_ARCHITECTURE_REMEDIATION",\n        "topology_source": RUNTIME_TOPOLOGY,\n        "base_contract_evidence": AR3_EVIDENCE,\n        "evidence": AR4A_EVIDENCE,',
    )
    exact(
        p,
        '        "remediation_state": {\n            "accepted_through": "AR-3",\n            "candidate": "AR-4A",\n            "candidate_status": "COMPOSITION_ROOT_CONSOLIDATION_CANDIDATE",\n            "evidence": AR4A_EVIDENCE,\n            "next_after_acceptance": "AR-4B",\n        },',
        '        "remediation_state": {\n            "accepted_through": "AR-4A",\n            "status": "ACCEPTED",\n            "evidence": AR4A_EVIDENCE,\n            "next_required_slice": "AR-4B",\n        },',
    )
    exact(p, '        "next_required_slice_after_ar3": "AR-4A",', '        "next_required_slice_after_ar4a": "AR-4B",')
    exact(
        p,
        '    candidate_status = copy.deepcopy(expected)\n    candidate_status["status"] = "AR3_APPLICATION_ARCHITECTURE_CONTRACT"\n    if candidate_status == expected:\n        raise SystemExit("AR-3 negative self-test failed to detect candidate-status regression")',
        '    candidate_status = copy.deepcopy(expected)\n    candidate_status["status"] = "ACCEPTED_AR3_APPLICATION_ARCHITECTURE_CONTRACT"\n    if candidate_status == expected:\n        raise SystemExit("AR-4A negative self-test failed to detect accepted-state rollback to AR-3")',
    )
    exact(
        p,
        '    remediation = copy.deepcopy(expected)\n    remediation["remediation_state"]["candidate_status"] = "ACCEPTED"\n    if remediation == expected:\n        raise SystemExit("AR-4A negative self-test failed to distinguish candidate and accepted remediation state")',
        '    remediation = copy.deepcopy(expected)\n    remediation["remediation_state"] = {\n        "accepted_through": "AR-3",\n        "candidate": "AR-4A",\n        "candidate_status": "COMPOSITION_ROOT_CONSOLIDATION_CANDIDATE",\n        "evidence": AR4A_EVIDENCE,\n        "next_after_acceptance": "AR-4B",\n    }\n    if remediation == expected:\n        raise SystemExit("AR-4A negative self-test failed to detect candidate-state regression")',
    )
    exact(p, 'print("AR-3 application architecture + AR-4A composition negative self-tests passed.")', 'print("AR-4A accepted application architecture remediation negative self-tests passed.")')


def project_inventory_generator() -> None:
    p = "scripts/generate-architecture-inventory.py"
    exact(p, 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3"]\nCURRENT_SLICE = "AR-3"\nNEXT_SLICE = "AR-4A"', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A"]\nCURRENT_SLICE = "AR-4A"\nNEXT_SLICE = "AR-4B"')
    exact(p, '"scope": "ar4a_composition_root_consolidation_candidate"', '"scope": "ar4a_composition_root_consolidation_accepted"')
    exact(p, "docs/status.json must remain production_ready=false during AR-3", "docs/status.json must remain production_ready=false during accepted AR-4A")
    exact(p, "docs/status.json must keep AR-3 architecture/gate state fail closed", "docs/status.json must keep AR-4A architecture/gate state fail closed")
    exact(p, "docs/status.json must project accepted AR-3 -> active AR-4A sequencing after AR-3 closeout", "docs/status.json must project accepted AR-4A -> active AR-4B sequencing after AR-4A closeout")
    exact(
        p,
        '            "application_architecture_evidence": ar3.AR3_EVIDENCE,\n            "application_architecture_projection": "architecture/inventory.json::application_architecture",',
        '            "application_architecture_evidence": AR4A_EVIDENCE,\n            "application_architecture_base_evidence": ar3.AR3_EVIDENCE,\n            "application_architecture_projection": "architecture/inventory.json::application_architecture",',
    )


def project_documentation_checker() -> None:
    p = "scripts/check-documentation-authority.py"
    exact(p, 'CURRENT_SLICE = "AR-3"\nNEXT_SLICE = "AR-4A"', 'CURRENT_SLICE = "AR-4A"\nNEXT_SLICE = "AR-4B"')
    exact(p, 'STATUS_DATE = "2026-08-15"', 'STATUS_DATE = "2026-08-16"')
    exact(p, 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3"]', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A"]')
    exact(p, 'AR3_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR3.md")', 'AR3_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR3.md")\nAR4A_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4A.md")')
    exact(p, '    AR3_EVIDENCE,\n    Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"),', '    AR3_EVIDENCE,\n    AR4A_EVIDENCE,\n    Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"),')
    exact(p, '        ar3_evidence = read(root, AR3_EVIDENCE)\n        pre2j_stub', '        ar3_evidence = read(root, AR3_EVIDENCE)\n        ar4a_evidence = read(root, AR4A_EVIDENCE)\n        pre2j_stub')
    exact(p, "docs/status.json must be the current AR-3 schema/date projection", "docs/status.json must be the current AR-4A schema/date projection")
    exact(p, "production_ready must remain false throughout accepted AR-3", "production_ready must remain false throughout accepted AR-4A")
    exact(p, "AR-3 architecture_complete/Production Core gate state must remain fail closed", "AR-4A architecture_complete/Production Core gate state must remain fail closed")
    exact(p, 'if transition.get("schema_version") != 7 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR3_MERGE":\n        errors.append("architecture transition must encode accepted AR-3 state")', 'if transition.get("schema_version") != 7 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR4A_MERGE":\n        errors.append("architecture transition must encode accepted AR-4A state")')
    exact(p, "architecture transition must encode AR-3 -> AR-4A sequencing", "architecture transition must encode AR-4A -> AR-4B sequencing")
    exact(p, "transition state must remain fail closed through AR-3", "transition state must remain fail closed through AR-4A")
    exact(p, "architecture inventory AR-3 program state is stale", "architecture inventory AR-4A program state is stale")
    exact(p, 'common = ("Architecture Re-baseline v3", "issue #266", "AR-3", "AR-4A", "production_ready=false")', 'common = ("Architecture Re-baseline v3", "issue #266", "AR-4A", "AR-4B", "production_ready=false")')
    exact(p, 'require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-3", "AR-4A"), "docs/INDEX.md", errors)', 'require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-4A", "AR-4B"), "docs/INDEX.md", errors)')
    exact(
        p,
        'require(development, ("Document status:** GENERATED_PROJECTION", "AR-2   Runtime Topology + D3 Compatibility", "AR-3   Application Architecture Contract", "AR-4A  Composition-root consolidation", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)',
        'require(development, ("Document status:** GENERATED_PROJECTION", "AR-3   Application Architecture Contract", "AR-4A  Composition-root consolidation", "AR-4B  Client Mail route ownership", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)',
    )
    exact(
        p,
        'require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-3", "Next slice:** AR-4A", "AR-3   Application Architecture Contract", "AR-4A  Composition-root consolidation", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)',
        'require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-4A", "Next slice:** AR-4B", "AR-3   Application Architecture Contract", "AR-4A  Composition-root consolidation", "AR-4B  Client Mail route ownership", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)',
    )
    exact(p, 'require(ar3_evidence, ("AR-3 Application Architecture Contract", "EVIDENCE / AR-3 accepted", "AR-4A", "AR-4B", "AR-4C", "NOT_REQUIRED", "architecture/inventory.json"), "AR-3 evidence", errors)', 'require(ar3_evidence, ("AR-3 Application Architecture Contract", "EVIDENCE / AR-3 accepted", "AR-4A", "AR-4B", "AR-4C", "NOT_REQUIRED", "architecture/inventory.json"), "AR-3 evidence", errors)\n    require(ar4a_evidence, ("AR-4A Composition-root consolidation", "EVIDENCE / AR-4A accepted", "f257a30a1df437812edb5c9e4b33c3de7e0740bc", "74672285ef0146c2dc6da298024b378438e5a75d", "AR-4B", "AR-4C", "Production Core remains `BLOCKED`"), "AR-4A evidence", errors)')
    exact(p, '("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-3"\', \'"current_slice": "AR-2"\', "current_slice"),', '("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-4A"\', \'"current_slice": "AR-3"\', "current_slice"),')
    exact(p, 'print("Architecture Re-baseline v3 AR-3 documentation authority negative fixtures passed.")', 'print("Architecture Re-baseline v3 AR-4A documentation authority negative fixtures passed.")')
    exact(p, 'print("Architecture Re-baseline v3 AR-3 documentation/program authority is consistent.")', 'print("Architecture Re-baseline v3 AR-4A documentation/program authority is consistent.")')


project_human_docs()
project_ar4a_evidence()
project_json_state()
project_application_generator()
project_inventory_generator()
project_documentation_checker()
print("AR-4A post-merge authority closeout projection applied deterministically.")
