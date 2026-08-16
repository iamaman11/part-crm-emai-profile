#!/usr/bin/env python3
"""One-shot AR-4C authority closeout helper.

Temporary branch-only migration helper. It updates the already-merged AR-4C candidate to accepted
human/machine authority while preserving fail-closed production state. Remove before acceptance.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GREEN = "c62f3a7fb00acf16fa1a8a00d9d2f101949cf8a3"
MERGE = "d8382d1578c4911287fb76dd0b9966b23aa85c25"


def replace(relative: str, old: str, new: str, *, count: int | None = 1) -> None:
    path = ROOT / relative
    text = path.read_text(encoding="utf-8")
    observed = text.count(old)
    if count is not None and observed != count:
        raise SystemExit(
            f"{relative}: expected {count} occurrence(s) of {old!r}; observed {observed}"
        )
    if count is None and observed < 1:
        raise SystemExit(f"{relative}: expected at least one occurrence of {old!r}")
    text = text.replace(old, new) if count is None else text.replace(old, new, count)
    path.write_text(text, encoding="utf-8", newline="\n")


# Human authority/navigation projections.
replace(
    "README.md",
    "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A and AR-4B.",
    "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B and AR-4C.",
)
replace(
    "README.md",
    "- **Current accepted checkpoint:** AR-4B — Client Mail route ownership.",
    "- **Current accepted checkpoint:** AR-4C — Outbound Mail composition extraction.",
)
replace(
    "README.md",
    "- **Next slice:** AR-4C — Outbound Mail composition extraction.",
    "- **Next slice:** AR-5 — Wrangler / Runtime Authority Cleanup.",
)
replace(
    "README.md",
    "#266. AR-4B Client Mail route-ownership remediation extends the AR-4A-remediated accepted AR-3 application/runtime ownership contract in",
    "#266. AR-4C Outbound Mail composition extraction extends the AR-4B-remediated accepted AR-3 application/runtime ownership contract in",
)
replace(
    "README.md",
    "with AR-4B acceptance evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md), AR-4A evidence preserved in",
    "with AR-4C acceptance evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR4C.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4C.md), AR-4B evidence preserved in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md), AR-4A evidence preserved in",
)
replace(
    "README.md",
    "- [`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;",
    "- [`docs/ARCHITECTURE_REBASELINE_V3_AR4C.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4C.md) — accepted AR-4C Outbound Mail composition-extraction evidence;\n- [`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;",
)

for relative in ("docs/README.md",):
    replace(relative, "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A and AR-4B.", "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B and AR-4C.")
    replace(relative, "- **Current accepted checkpoint:** AR-4B — Client Mail route ownership.", "- **Current accepted checkpoint:** AR-4C — Outbound Mail composition extraction.")
    replace(relative, "- **Next slice:** AR-4C — Outbound Mail composition extraction.", "- **Next slice:** AR-5 — Wrangler / Runtime Authority Cleanup.")
replace(
    "docs/README.md",
    "- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;",
    "- [`ARCHITECTURE_REBASELINE_V3_AR4C.md`](ARCHITECTURE_REBASELINE_V3_AR4C.md) — accepted AR-4C Outbound Mail composition-extraction evidence;\n- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;",
)
replace(
    "docs/README.md",
    "preserving its repository-side D3 foundation; AR-4B is accepted and AR-4C is the only next architecture slice.",
    "preserving its repository-side D3 foundation; AR-4C is accepted, AR-4D remains NOT_REQUIRED, and AR-5 is the only next architecture slice.",
)

replace("docs/INDEX.md", "- AR-0, AR-1, AR-2, AR-3, AR-4A and AR-4B are accepted checkpoints.", "- AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B and AR-4C are accepted checkpoints.")
replace("docs/INDEX.md", "- AR-4B — Client Mail route ownership is the current accepted checkpoint.", "- AR-4C — Outbound Mail composition extraction is the current accepted checkpoint.")
replace("docs/INDEX.md", "- AR-4C — Outbound Mail composition extraction is the only next slice.", "- AR-5 — Wrangler / Runtime Authority Cleanup is the only next slice; AR-4D remains NOT_REQUIRED unless later accepted evidence reopens it.")
replace(
    "docs/INDEX.md",
    "accepted AR-2 topology/D3 decision input retained by the AR-4B-remediated, AR-4A-remediated accepted AR-3 application/runtime ownership projection.",
    "accepted AR-2 topology/D3 decision input retained by the AR-4C-remediated accepted AR-3 application/runtime ownership projection.",
)
replace(
    "docs/INDEX.md",
    "- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md)",
    "- [`ARCHITECTURE_REBASELINE_V3_AR4C.md`](ARCHITECTURE_REBASELINE_V3_AR4C.md)\n- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md)",
)

replace("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "**Current accepted architecture checkpoint:** AR-4B — Client Mail route ownership", "**Current accepted architecture checkpoint:** AR-4C — Outbound Mail composition extraction")
replace("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "**Next slice:** AR-4C — Outbound Mail composition extraction", "**Next slice:** AR-5 — Wrangler / Runtime Authority Cleanup")
replace(
    "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
    "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-4B is the latest accepted checkpoint. Its Client Mail route-ownership remediation extends the AR-4A-remediated accepted AR-3 application/runtime ownership contract in `architecture/inventory.json`, with AR-4B acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`, AR-4A evidence preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`, and the AR-3 base contract preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`.",
    "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-4C is the latest accepted checkpoint. Its Outbound Mail composition extraction extends the AR-4B-remediated accepted AR-3 application/runtime ownership contract in `architecture/inventory.json`, with AR-4C acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR4C.md`, AR-4B and AR-4A evidence preserved in their evidence documents, and the AR-3 base contract preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`. AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it.",
)
replace("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "AR-4B  Client Mail route ownership                               CURRENT / ACCEPTED CHECKPOINT", "AR-4B  Client Mail route ownership                               DONE / ACCEPTED")
replace("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "AR-4C  Outbound Mail composition extraction                      NEXT", "AR-4C  Outbound Mail composition extraction                      CURRENT / ACCEPTED CHECKPOINT")
replace("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-5   Wrangler / Runtime Authority Cleanup                     NEXT")

replace("docs/DEVELOPMENT_PLAN.md", "**Current accepted architecture checkpoint:** AR-4B — Client Mail route ownership", "**Current accepted architecture checkpoint:** AR-4C — Outbound Mail composition extraction")
replace("docs/DEVELOPMENT_PLAN.md", "**Next architecture slice:** AR-4C — Outbound Mail composition extraction", "**Next architecture slice:** AR-5 — Wrangler / Runtime Authority Cleanup")
replace("docs/DEVELOPMENT_PLAN.md", "- AR-4B — Client Mail route ownership: **CURRENT ACCEPTED CHECKPOINT**.", "- AR-4B — Client Mail route ownership: **DONE / ACCEPTED**.")
replace("docs/DEVELOPMENT_PLAN.md", "- AR-4C — Outbound Mail composition extraction: **NEXT**.", "- AR-4C — Outbound Mail composition extraction: **CURRENT ACCEPTED CHECKPOINT**.")
replace("docs/DEVELOPMENT_PLAN.md", "- AR-4D…AR-17: ordered future architecture slices; AR-4D is skipped unless later accepted evidence reopens it.", "- AR-4D remains **NOT REQUIRED** unless later accepted evidence reopens it.\n- AR-5 — Wrangler / Runtime Authority Cleanup: **NEXT**.\n- AR-6…AR-17: ordered future architecture slices.")
replace("docs/DEVELOPMENT_PLAN.md", "AR-4B  Client Mail route ownership                               CURRENT / ACCEPTED CHECKPOINT", "AR-4B  Client Mail route ownership                               DONE / ACCEPTED")
replace("docs/DEVELOPMENT_PLAN.md", "AR-4C  Outbound Mail composition extraction                      NEXT", "AR-4C  Outbound Mail composition extraction                      CURRENT / ACCEPTED CHECKPOINT")
replace("docs/DEVELOPMENT_PLAN.md", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-5   Wrangler / Runtime Authority Cleanup                     NEXT")

# AR-4C evidence becomes accepted and records exact implementation proof.
replace("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md", "Status: **CANDIDATE — NOT ACCEPTED UNTIL POST-MERGE CLOSEOUT**", "Status: **EVIDENCE / AR-4C accepted**")
replace(
    "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md",
    "Bounded implementation issue: #286  \nAccepted baseline: `bf323fbb8af160471299cdf30f0fcf406fe0457d` (`main`, AR-4B accepted)",
    "Accepted baseline: `bf323fbb8af160471299cdf30f0fcf406fe0457d` (`main`, AR-4B accepted)  \nImplementation issue: #286  \nImplementation PR: #287  \nExact-green implementation head: `c62f3a7fb00acf16fa1a8a00d9d2f101949cf8a3`  \nImplementation merge: `d8382d1578c4911287fb76dd0b9966b23aa85c25`  \nApplicable permanent workflows: **13/13 success**",
)
replace("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md", "## Candidate composition ownership", "## Accepted composition ownership")
replace("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md", "## Candidate machine state", "## Accepted machine state")
replace(
    "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md",
    "Until the separate closeout step accepts the merged implementation:\n\n- `accepted_through = AR-4B`;\n- candidate slice = `AR-4C`;\n- candidate status = `OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE`;\n- AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it;\n- next required slice after AR-4C acceptance = `AR-5`;",
    "After post-merge authority closeout:\n\n- `accepted_through = AR-4C`;\n- accepted status = `OUTBOUND_MAIL_COMPOSITION_EXTRACTION_ACCEPTED`;\n- AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it;\n- next required slice = `AR-5`;",
)
replace("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md", "The candidate must pass, at minimum:", "The implementation candidate was required to pass, at minimum:")

# Machine-readable status projection.
status_path = ROOT / "docs/status.json"
status = json.loads(status_path.read_text(encoding="utf-8"))
program = status["current"]["architecture_program"]
expected = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B"]
if program.get("accepted_slices") != expected:
    raise SystemExit("docs/status.json accepted_slices baseline drifted")
program["accepted_slices"].append("AR-4C")
program["current_slice"] = "AR-4C"
program["next_slice_after_acceptance"] = "AR-5"
program["application_architecture_evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"
program["ar4c_acceptance"] = {
    "issue": 286,
    "implementation_pr": 287,
    "exact_green_head": GREEN,
    "implementation_merge": MERGE,
    "applicable_permanent_workflows": "13/13",
}
status["next_repository_step"] = {
    "number": None,
    "name": "Architecture Re-baseline v3 AR-5",
    "status": "next_after_accepted_ar4c_merge",
    "tracking_issue": 266,
    "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
}
status["implementation"]["architecture_rebaseline_v3"] = "active_issue_266_ar4c_accepted_next_ar5"
status_path.write_text(json.dumps(status, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

# Machine transition projection.
transition_path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
transition = json.loads(transition_path.read_text(encoding="utf-8"))
if transition.get("accepted_slices") != expected:
    raise SystemExit("transition accepted_slices baseline drifted")
transition["status"] = "ACTIVE_AFTER_ACCEPTED_AR4C_MERGE"
transition["accepted_slices"].append("AR-4C")
transition["current_slice"] = "AR-4C"
transition["next_slice_after_acceptance"] = "AR-5"
transition["architecture_inventory_policy"]["ar4c_remediation"] = "ACCEPTED_OUTBOUND_MAIL_COMPOSITION_EXTRACTION_IN_CANONICAL_INVENTORY"
app = transition["application_architecture"]
app["evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"
app["status"] = "ACCEPTED_THROUGH_AR4C"
app["ar4c"] = "ACCEPTED_OUTBOUND_MAIL_COMPOSITION_EXTRACTION"
app["ar4d"] = "NOT_REQUIRED_UNLESS_LATER_ACCEPTED_EVIDENCE_REOPENS"
app["next_required_slice"] = "AR-5"
app["implementation_issue"] = 286
app["implementation_pr"] = 287
app["exact_green_head"] = GREEN
app["implementation_merge"] = MERGE
app["applicable_permanent_workflows"] = "13/13"
transition_path.write_text(json.dumps(transition, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

# Accepted application-architecture projection.
replace("scripts/_ar3_application_architecture.py", '"status": "AR4C_COMPOSITION_EXTRACTION_CANDIDATE"', '"status": "AR4C_COMPOSITION_EXTRACTION_ACCEPTED"')
replace("scripts/_ar3_application_architecture.py", '"status": "AR4C_OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE"', '"status": "AR4C_OUTBOUND_MAIL_COMPOSITION_EXTRACTION_ACCEPTED"')
replace("scripts/_ar3_application_architecture.py", '"status": "ACCEPTED_AR4B_APPLICATION_ARCHITECTURE_REMEDIATION"', '"status": "ACCEPTED_AR4C_APPLICATION_ARCHITECTURE_REMEDIATION"')
replace("scripts/_ar3_application_architecture.py", '"evidence": AR4B_EVIDENCE,\n        "projection_policy":', '"evidence": AR4C_EVIDENCE,\n        "projection_policy":')
replace("scripts/_ar3_application_architecture.py", '"AR4C_OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",', '"AR4C_OUTBOUND_MAIL_COMPOSITION_EXTRACTION_ACCEPTED",', count=None)
replace(
    "scripts/_ar3_application_architecture.py",
    '"remediation_state": {\n            "accepted_through": "AR-4B",\n            "candidate": "AR-4C",\n            "candidate_status": "OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",\n            "evidence": AR4C_EVIDENCE,\n            "next_after_acceptance": "AR-5",\n        },',
    '"remediation_state": {\n            "accepted_through": "AR-4C",\n            "status": "ACCEPTED",\n            "evidence": AR4C_EVIDENCE,\n            "next_required_slice": "AR-5",\n        },',
)
replace("scripts/_ar3_application_architecture.py", '"next_required_slice_after_ar4b": "AR-4C"', '"next_required_slice_after_ar4c": "AR-5"')
replace(
    "scripts/_ar3_application_architecture.py",
    'remediation["remediation_state"] = {\n        "accepted_through": "AR-4B",\n        "status": "ACCEPTED",\n        "evidence": AR4B_EVIDENCE,\n        "next_required_slice": "AR-4C",\n    }',
    'remediation["remediation_state"] = {\n        "accepted_through": "AR-4B",\n        "candidate": "AR-4C",\n        "candidate_status": "OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",\n        "evidence": AR4C_EVIDENCE,\n        "next_after_acceptance": "AR-5",\n    }',
)
replace("scripts/_ar3_application_architecture.py", "AR-4C negative self-test failed to distinguish candidate and accepted remediation state", "AR-4C negative self-test failed to detect accepted-state rollback to candidate remediation state")
replace("scripts/_ar3_application_architecture.py", "AR-4C Outbound Mail composition candidate negative self-tests passed.", "AR-4C accepted Outbound Mail composition negative self-tests passed.")

# Canonical inventory generator now treats AR-4C as accepted.
replace("scripts/generate-architecture-inventory.py", 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B"]', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C"]')
replace("scripts/generate-architecture-inventory.py", 'CURRENT_SLICE = "AR-4B"', 'CURRENT_SLICE = "AR-4C"')
replace("scripts/generate-architecture-inventory.py", 'NEXT_SLICE = "AR-4C"', 'NEXT_SLICE = "AR-5"')
replace("scripts/generate-architecture-inventory.py", '"scope": "ar4c_outbound_mail_composition_candidate"', '"scope": "ar4c_outbound_mail_composition_accepted"')
replace("scripts/generate-architecture-inventory.py", "production_ready=false during accepted AR-4B", "production_ready=false during accepted AR-4C")
replace("scripts/generate-architecture-inventory.py", "accepted AR-4B architecture/gate state fail closed", "accepted AR-4C architecture/gate state fail closed")
replace("scripts/generate-architecture-inventory.py", "accepted AR-4B -> next AR-4C sequencing", "accepted AR-4C -> next AR-5 sequencing")
replace(
    "scripts/generate-architecture-inventory.py",
    '"application_architecture_evidence": AR4B_EVIDENCE,\n            "application_architecture_candidate_evidence": AR4C_EVIDENCE,',
    '"application_architecture_evidence": AR4C_EVIDENCE,',
)
replace("scripts/generate-architecture-inventory.py", "Architecture inventory and AR-4C candidate composition projection are current.", "Architecture inventory and accepted AR-4C composition projection are current.")
replace("scripts/generate-architecture-inventory.py", "Architecture inventory AR-4C candidate negative self-test passed.", "Architecture inventory accepted AR-4C negative self-test passed.")

# Documentation authority checker advances in lockstep.
replace("scripts/check-documentation-authority.py", 'CURRENT_SLICE = "AR-4B"', 'CURRENT_SLICE = "AR-4C"')
replace("scripts/check-documentation-authority.py", 'NEXT_SLICE = "AR-4C"', 'NEXT_SLICE = "AR-5"')
replace("scripts/check-documentation-authority.py", 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B"]', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C"]')
replace("scripts/check-documentation-authority.py", 'AR4B_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4B.md")', 'AR4B_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4B.md")\nAR4C_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md")')
replace("scripts/check-documentation-authority.py", "    AR4B_EVIDENCE,\n", "    AR4B_EVIDENCE,\n    AR4C_EVIDENCE,\n")
replace("scripts/check-documentation-authority.py", "        ar4b_evidence = read(root, AR4B_EVIDENCE)\n", "        ar4b_evidence = read(root, AR4B_EVIDENCE)\n        ar4c_evidence = read(root, AR4C_EVIDENCE)\n")
replace("scripts/check-documentation-authority.py", "current AR-4B schema/date projection", "current AR-4C schema/date projection")
replace("scripts/check-documentation-authority.py", "production_ready must remain false throughout accepted AR-4B", "production_ready must remain false throughout accepted AR-4C")
replace("scripts/check-documentation-authority.py", "AR-4B architecture_complete/Production Core gate state must remain fail closed", "AR-4C architecture_complete/Production Core gate state must remain fail closed")
replace("scripts/check-documentation-authority.py", '"ACTIVE_AFTER_ACCEPTED_AR4B_MERGE"', '"ACTIVE_AFTER_ACCEPTED_AR4C_MERGE"')
replace("scripts/check-documentation-authority.py", "architecture transition must encode accepted AR-4B state", "architecture transition must encode accepted AR-4C state")
replace("scripts/check-documentation-authority.py", "architecture transition must encode AR-4B -> AR-4C sequencing", "architecture transition must encode AR-4C -> AR-5 sequencing")
replace("scripts/check-documentation-authority.py", "transition state must remain fail closed through AR-4B", "transition state must remain fail closed through AR-4C")
replace("scripts/check-documentation-authority.py", "architecture inventory AR-4B program state is stale", "architecture inventory AR-4C program state is stale")
replace("scripts/check-documentation-authority.py", 'common = ("Architecture Re-baseline v3", "issue #266", "AR-4B", "AR-4C", "production_ready=false")', 'common = ("Architecture Re-baseline v3", "issue #266", "AR-4C", "AR-5", "production_ready=false")')
replace("scripts/check-documentation-authority.py", 'require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-4B", "AR-4C"),', 'require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-4C", "AR-5"),')
replace("scripts/check-documentation-authority.py", 'require(development, ("Document status:** GENERATED_PROJECTION", "AR-4A  Composition-root consolidation", "AR-4B  Client Mail route ownership", "AR-4C  Outbound Mail composition extraction", "production_ready=false", "Immutable Accepted Phase Provenance"),', 'require(development, ("Document status:** GENERATED_PROJECTION", "AR-4B  Client Mail route ownership", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "production_ready=false", "Immutable Accepted Phase Provenance"),')
replace("scripts/check-documentation-authority.py", 'require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-4B", "Next slice:** AR-4C", "AR-4A  Composition-root consolidation", "AR-4B  Client Mail route ownership", "AR-4C  Outbound Mail composition extraction",', 'require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-4C", "Next slice:** AR-5", "AR-4B  Client Mail route ownership", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup",')
replace(
    "scripts/check-documentation-authority.py",
    '    require(ar4b_evidence, ("AR-4B Client Mail route ownership", "EVIDENCE / AR-4B accepted", "7ccdd1b0ed0c0eae974cd9bde15c87524315c023", "04b62c97813010ac283d8b70c81089f1c16f5672", "AR-4C", "Production Core remains `BLOCKED`"), "AR-4B evidence", errors)\n',
    '    require(ar4b_evidence, ("AR-4B Client Mail route ownership", "EVIDENCE / AR-4B accepted", "7ccdd1b0ed0c0eae974cd9bde15c87524315c023", "04b62c97813010ac283d8b70c81089f1c16f5672", "AR-4C", "Production Core remains `BLOCKED`"), "AR-4B evidence", errors)\n    require(ar4c_evidence, ("AR-4C Outbound Mail composition extraction", "EVIDENCE / AR-4C accepted", "c62f3a7fb00acf16fa1a8a00d9d2f101949cf8a3", "d8382d1578c4911287fb76dd0b9966b23aa85c25", "AR-5", "Production Core remains `BLOCKED`"), "AR-4C evidence", errors)\n',
)
replace("scripts/check-documentation-authority.py", '("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-4B"\', \'"current_slice": "AR-4A"\', "current_slice")', '("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-4C"\', \'"current_slice": "AR-4B"\', "current_slice")')
replace("scripts/check-documentation-authority.py", "Architecture Re-baseline v3 AR-4B documentation authority negative fixtures passed.", "Architecture Re-baseline v3 AR-4C documentation authority negative fixtures passed.")
replace("scripts/check-documentation-authority.py", "Architecture Re-baseline v3 AR-4B documentation/program authority is consistent.", "Architecture Re-baseline v3 AR-4C documentation/program authority is consistent.")

print("AR-4C closeout authority sources updated; regenerate canonical inventory next.")
