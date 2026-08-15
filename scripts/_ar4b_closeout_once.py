#!/usr/bin/env python3
"""One-shot AR-4B authority closeout helper.

Temporary branch-only migration helper. It performs exact, fail-closed authority replacements,
updates JSON projections structurally, and is removed before the closeout candidate is accepted.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GREEN = "7ccdd1b0ed0c0eae974cd9bde15c87524315c023"
MERGE = "04b62c97813010ac283d8b70c81089f1c16f5672"


def replace(path: str, pairs: list[tuple[str, str]]) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    for old, new in pairs:
        count = text.count(old)
        if count != 1:
            raise SystemExit(f"{path}: expected exactly one occurrence of {old!r}; observed {count}")
        text = text.replace(old, new, 1)
    target.write_text(text, encoding="utf-8", newline="\n")


replace(
    "README.md",
    [
        ("- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3 and AR-4A.", "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A and AR-4B."),
        ("- **Current accepted checkpoint:** AR-4A — Composition-root consolidation.", "- **Current accepted checkpoint:** AR-4B — Client Mail route ownership."),
        ("- **Next slice:** AR-4B — Client Mail route ownership.", "- **Next slice:** AR-4C — Outbound Mail composition extraction."),
        ("#266. AR-4A composition-root remediation extends the accepted AR-3 application/runtime ownership contract in", "#266. AR-4B Client Mail route-ownership remediation extends the AR-4A-remediated accepted AR-3 application/runtime ownership contract in"),
        ("with acceptance evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md) and the AR-3 base contract preserved in", "with AR-4B acceptance evidence in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md), AR-4A evidence preserved in\n[`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md), and the AR-3 base contract preserved in"),
        ("- [`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;", "- [`docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;\n- [`docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`](docs/ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;"),
    ],
)

replace(
    "docs/README.md",
    [
        ("- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3 and AR-4A.", "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A and AR-4B."),
        ("- **Current accepted checkpoint:** AR-4A — Composition-root consolidation.", "- **Current accepted checkpoint:** AR-4B — Client Mail route ownership."),
        ("- **Next slice:** AR-4B — Client Mail route ownership.", "- **Next slice:** AR-4C — Outbound Mail composition extraction."),
        ("- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;", "- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md) — accepted AR-4B Client Mail route-ownership evidence;\n- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md) — accepted AR-4A composition-root remediation evidence;"),
        ("preserving its repository-side D3 foundation; AR-4A is accepted and AR-4B is the only next architecture slice.", "preserving its repository-side D3 foundation; AR-4B is accepted and AR-4C is the only next architecture slice."),
    ],
)

replace(
    "docs/INDEX.md",
    [
        ("- AR-0, AR-1, AR-2, AR-3 and AR-4A are accepted checkpoints.", "- AR-0, AR-1, AR-2, AR-3, AR-4A and AR-4B are accepted checkpoints."),
        ("- AR-4A — Composition-root consolidation is the current accepted checkpoint.", "- AR-4B — Client Mail route ownership is the current accepted checkpoint."),
        ("- AR-4B — Client Mail route ownership is the only next slice.", "- AR-4C — Outbound Mail composition extraction is the only next slice."),
        ("accepted AR-2 topology/D3 decision input retained by the AR-4A-remediated accepted AR-3 application/runtime ownership projection.", "accepted AR-2 topology/D3 decision input retained by the AR-4B-remediated, AR-4A-remediated accepted AR-3 application/runtime ownership projection."),
        ("- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md)", "- [`ARCHITECTURE_REBASELINE_V3_AR4B.md`](ARCHITECTURE_REBASELINE_V3_AR4B.md)\n- [`ARCHITECTURE_REBASELINE_V3_AR4A.md`](ARCHITECTURE_REBASELINE_V3_AR4A.md)"),
    ],
)

replace(
    "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
    [
        ("**Current accepted architecture checkpoint:** AR-4A — Composition-root consolidation", "**Current accepted architecture checkpoint:** AR-4B — Client Mail route ownership"),
        ("**Next slice:** AR-4B — Client Mail route ownership", "**Next slice:** AR-4C — Outbound Mail composition extraction"),
        ("This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-4A is the latest accepted checkpoint. Its composition-root remediation extends the accepted AR-3 application/runtime ownership contract in `architecture/inventory.json`, with AR-4A acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR4A.md` and the AR-3 base contract preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`.", "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-4B is the latest accepted checkpoint. Its Client Mail route-ownership remediation extends the AR-4A-remediated accepted AR-3 application/runtime ownership contract in `architecture/inventory.json`, with AR-4B acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR4B.md`, AR-4A evidence preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR4A.md`, and the AR-3 base contract preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`."),
        ("AR-4A  Composition-root consolidation                            CURRENT / ACCEPTED CHECKPOINT", "AR-4A  Composition-root consolidation                            DONE / ACCEPTED"),
        ("AR-4B  Client Mail route ownership                               NEXT", "AR-4B  Client Mail route ownership                               CURRENT / ACCEPTED CHECKPOINT"),
        ("AR-4C  Outbound Mail composition extraction", "AR-4C  Outbound Mail composition extraction                      NEXT"),
    ],
)

replace(
    "docs/DEVELOPMENT_PLAN.md",
    [
        ("**Current accepted architecture checkpoint:** AR-4A — Composition-root consolidation", "**Current accepted architecture checkpoint:** AR-4B — Client Mail route ownership"),
        ("**Next architecture slice:** AR-4B — Client Mail route ownership", "**Next architecture slice:** AR-4C — Outbound Mail composition extraction"),
        ("- AR-4A — Composition-root consolidation: **CURRENT ACCEPTED CHECKPOINT**.", "- AR-4A — Composition-root consolidation: **DONE / ACCEPTED**."),
        ("- AR-4B — Client Mail route ownership: **NEXT**.", "- AR-4B — Client Mail route ownership: **CURRENT ACCEPTED CHECKPOINT**."),
        ("- AR-4C…AR-17: ordered future architecture slices; AR-4D is skipped unless later accepted evidence reopens it.", "- AR-4C — Outbound Mail composition extraction: **NEXT**.\n- AR-4D…AR-17: ordered future architecture slices; AR-4D is skipped unless later accepted evidence reopens it."),
        ("AR-4A  Composition-root consolidation                            CURRENT / ACCEPTED CHECKPOINT", "AR-4A  Composition-root consolidation                            DONE / ACCEPTED"),
        ("AR-4B  Client Mail route ownership                               NEXT", "AR-4B  Client Mail route ownership                               CURRENT / ACCEPTED CHECKPOINT"),
        ("AR-4C  Outbound Mail composition extraction", "AR-4C  Outbound Mail composition extraction                      NEXT"),
    ],
)

replace(
    "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md",
    [
        ("Status: **CANDIDATE — NOT ACCEPTED UNTIL POST-MERGE CLOSEOUT**", "Status: **EVIDENCE / AR-4B accepted**"),
        ("Accepted baseline: `c705dc45c9e923582daf531242bd2c6af2239597` (`main`, AR-4A accepted)", "Accepted baseline: `c705dc45c9e923582daf531242bd2c6af2239597` (`main`, AR-4A accepted)  \nImplementation issue: #282  \nImplementation PR: #283  \nExact-green implementation head: `7ccdd1b0ed0c0eae974cd9bde15c87524315c023`  \nImplementation merge: `04b62c97813010ac283d8b70c81089f1c16f5672`  \nApplicable permanent workflows: **13/13 success**"),
        ("## Candidate ownership", "## Accepted ownership"),
        ("## Candidate machine state", "## Accepted machine state"),
        ("Until the separate closeout step accepts the merged implementation:\n\n- `accepted_through = AR-4A`;\n- candidate slice = `AR-4B`;\n- candidate status = `ROUTE_OWNERSHIP_CONSOLIDATION_CANDIDATE`;\n- next slice after acceptance = `AR-4C`;", "After post-merge authority closeout:\n\n- `accepted_through = AR-4B`;\n- accepted status = `ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED`;\n- next required slice = `AR-4C`;"),
    ],
)

# Machine-readable status projection.
status_path = ROOT / "docs/status.json"
status = json.loads(status_path.read_text(encoding="utf-8"))
program = status["current"]["architecture_program"]
if program["accepted_slices"] != ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A"]:
    raise SystemExit("docs/status.json accepted_slices baseline drifted")
program["accepted_slices"].append("AR-4B")
program["current_slice"] = "AR-4B"
program["next_slice_after_acceptance"] = "AR-4C"
program["application_architecture_evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"
program["ar4b_acceptance"] = {
    "issue": 282,
    "implementation_pr": 283,
    "exact_green_head": GREEN,
    "implementation_merge": MERGE,
    "applicable_permanent_workflows": "13/13",
}
status["next_repository_step"] = {
    "number": None,
    "name": "Architecture Re-baseline v3 AR-4C",
    "status": "next_after_accepted_ar4b_merge",
    "tracking_issue": 266,
    "authority": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
}
status["implementation"]["architecture_rebaseline_v3"] = "active_issue_266_ar4b_accepted_next_ar4c"
status_path.write_text(json.dumps(status, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

# Machine transition projection.
transition_path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
transition = json.loads(transition_path.read_text(encoding="utf-8"))
if transition["accepted_slices"] != ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A"]:
    raise SystemExit("transition accepted_slices baseline drifted")
transition["status"] = "ACTIVE_AFTER_ACCEPTED_AR4B_MERGE"
transition["accepted_slices"].append("AR-4B")
transition["current_slice"] = "AR-4B"
transition["next_slice_after_acceptance"] = "AR-4C"
transition["architecture_inventory_policy"]["ar4b_remediation"] = "ACCEPTED_CLIENT_MAIL_ROUTE_OWNERSHIP_IN_CANONICAL_INVENTORY"
app = transition["application_architecture"]
app["evidence"] = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"
app["status"] = "ACCEPTED_THROUGH_AR4B"
app["ar4a"] = "ACCEPTED_COMPOSITION_ROOT_CONSOLIDATION"
app["ar4b"] = "ACCEPTED_CLIENT_MAIL_ROUTE_OWNERSHIP"
app["ar4c"] = "NEXT_REQUIRED_SLICE_OUTBOUND_MAIL_COMPOSITION_DEBT"
app["implementation_issue"] = 282
app["implementation_pr"] = 283
app["exact_green_head"] = GREEN
app["implementation_merge"] = MERGE
app["applicable_permanent_workflows"] = "13/13"
transition_path.write_text(json.dumps(transition, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

# Accepted application-architecture projection.
replace(
    "scripts/_ar3_application_architecture.py",
    [
        ("AR4B_ROUTE_OWNERSHIP_CONSOLIDATION_CANDIDATE", "AR4B_ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED"),
        ("\"status\": \"ACCEPTED_AR4A_APPLICATION_ARCHITECTURE_REMEDIATION\"", "\"status\": \"ACCEPTED_AR4B_APPLICATION_ARCHITECTURE_REMEDIATION\""),
        ("\"accepted_through\": \"AR-4A\",\n            \"candidate\": \"AR-4B\",\n            \"candidate_status\": \"ROUTE_OWNERSHIP_CONSOLIDATION_CANDIDATE\",\n            \"evidence\": AR4B_EVIDENCE,\n            \"next_after_acceptance\": \"AR-4C\",", "\"accepted_through\": \"AR-4B\",\n            \"status\": \"ACCEPTED\",\n            \"evidence\": AR4B_EVIDENCE,\n            \"next_required_slice\": \"AR-4C\","),
        ("\"next_required_slice_after_ar4a\": \"AR-4B\"", "\"next_required_slice_after_ar4b\": \"AR-4C\""),
        ("remediation[\"remediation_state\"] = {\n        \"accepted_through\": \"AR-4A\",\n        \"status\": \"ACCEPTED\",\n        \"evidence\": AR4A_EVIDENCE,\n        \"next_required_slice\": \"AR-4B\",\n    }", "remediation[\"remediation_state\"] = {\n        \"accepted_through\": \"AR-4A\",\n        \"candidate\": \"AR-4B\",\n        \"candidate_status\": \"ROUTE_OWNERSHIP_CONSOLIDATION_CANDIDATE\",\n        \"evidence\": AR4B_EVIDENCE,\n        \"next_after_acceptance\": \"AR-4C\",\n    }"),
        ("raise SystemExit(\"AR-4B negative self-test failed to distinguish candidate and accepted remediation state\")", "raise SystemExit(\"AR-4B negative self-test failed to detect accepted-state rollback to candidate remediation state\")"),
        ("print(\"AR-4B Client Mail route ownership candidate negative self-tests passed.\")", "print(\"AR-4B accepted Client Mail route ownership negative self-tests passed.\")"),
    ],
)

# Canonical inventory generator now treats AR-4B as accepted.
replace(
    "scripts/generate-architecture-inventory.py",
    [
        ("ACCEPTED_SLICES = [\"AR-0\", \"AR-1\", \"AR-2\", \"AR-3\", \"AR-4A\"]", "ACCEPTED_SLICES = [\"AR-0\", \"AR-1\", \"AR-2\", \"AR-3\", \"AR-4A\", \"AR-4B\"]"),
        ("CURRENT_SLICE = \"AR-4A\"", "CURRENT_SLICE = \"AR-4B\""),
        ("NEXT_SLICE = \"AR-4B\"", "NEXT_SLICE = \"AR-4C\""),
        ("{\"path\": AR4B_EVIDENCE, \"status\": \"EVIDENCE\", \"scope\": \"ar4b_client_mail_route_ownership_candidate\"}", "{\"path\": AR4B_EVIDENCE, \"status\": \"EVIDENCE\", \"scope\": \"ar4b_client_mail_route_ownership_accepted\"}"),
        ("docs/status.json must remain production_ready=false during accepted AR-4A / candidate AR-4B", "docs/status.json must remain production_ready=false during accepted AR-4B"),
        ("docs/status.json must keep AR-4A architecture/gate state fail closed during candidate AR-4B", "docs/status.json must keep accepted AR-4B architecture/gate state fail closed"),
        ("docs/status.json must project accepted AR-4A -> active AR-4B sequencing until AR-4B closeout", "docs/status.json must project accepted AR-4B -> next AR-4C sequencing"),
        ("\"application_architecture_evidence\": AR4A_EVIDENCE,\n            \"application_architecture_candidate_evidence\": AR4B_EVIDENCE,", "\"application_architecture_evidence\": AR4B_EVIDENCE,"),
        ("print(\"Architecture inventory and AR-4B candidate ownership projection are current.\")", "print(\"Architecture inventory and accepted AR-4B ownership projection are current.\")"),
        ("print(\"Architecture inventory AR-4B candidate negative self-test passed.\")", "print(\"Architecture inventory accepted AR-4B negative self-test passed.\")"),
    ],
)

# Documentation authority checker advances in lockstep.
replace(
    "scripts/check-documentation-authority.py",
    [
        ("CURRENT_SLICE = \"AR-4A\"", "CURRENT_SLICE = \"AR-4B\""),
        ("NEXT_SLICE = \"AR-4B\"", "NEXT_SLICE = \"AR-4C\""),
        ("ACCEPTED_SLICES = [\"AR-0\", \"AR-1\", \"AR-2\", \"AR-3\", \"AR-4A\"]", "ACCEPTED_SLICES = [\"AR-0\", \"AR-1\", \"AR-2\", \"AR-3\", \"AR-4A\", \"AR-4B\"]"),
        ("AR4A_EVIDENCE = Path(\"docs/ARCHITECTURE_REBASELINE_V3_AR4A.md\")", "AR4A_EVIDENCE = Path(\"docs/ARCHITECTURE_REBASELINE_V3_AR4A.md\")\nAR4B_EVIDENCE = Path(\"docs/ARCHITECTURE_REBASELINE_V3_AR4B.md\")"),
        ("    AR4A_EVIDENCE,\n", "    AR4A_EVIDENCE,\n    AR4B_EVIDENCE,\n"),
        ("        ar4a_evidence = read(root, AR4A_EVIDENCE)\n", "        ar4a_evidence = read(root, AR4A_EVIDENCE)\n        ar4b_evidence = read(root, AR4B_EVIDENCE)\n"),
        ("current AR-4A schema/date projection", "current AR-4B schema/date projection"),
        ("production_ready must remain false throughout accepted AR-4A", "production_ready must remain false throughout accepted AR-4B"),
        ("AR-4A architecture_complete/Production Core gate state must remain fail closed", "AR-4B architecture_complete/Production Core gate state must remain fail closed"),
        ("ACTIVE_AFTER_ACCEPTED_AR4A_MERGE", "ACTIVE_AFTER_ACCEPTED_AR4B_MERGE"),
        ("architecture transition must encode accepted AR-4A state", "architecture transition must encode accepted AR-4B state"),
        ("architecture transition must encode AR-4A -> AR-4B sequencing", "architecture transition must encode AR-4B -> AR-4C sequencing"),
        ("transition state must remain fail closed through AR-4A", "transition state must remain fail closed through AR-4B"),
        ("architecture inventory AR-4A program state is stale", "architecture inventory AR-4B program state is stale"),
        ("common = (\"Architecture Re-baseline v3\", \"issue #266\", \"AR-4A\", \"AR-4B\", \"production_ready=false\")", "common = (\"Architecture Re-baseline v3\", \"issue #266\", \"AR-4B\", \"AR-4C\", \"production_ready=false\")"),
        ("require(index, (\"CURRENT_AUTHORITY\", \"ARCHITECTURE_REBASELINE_V3_PLAN.md\", \"issue #266\", \"AR-4A\", \"AR-4B\"),", "require(index, (\"CURRENT_AUTHORITY\", \"ARCHITECTURE_REBASELINE_V3_PLAN.md\", \"issue #266\", \"AR-4B\", \"AR-4C\"),"),
        ("require(development, (\"Document status:** GENERATED_PROJECTION\", \"AR-3   Application Architecture Contract\", \"AR-4A  Composition-root consolidation\", \"AR-4B  Client Mail route ownership\", \"production_ready=false\", \"Immutable Accepted Phase Provenance\"),", "require(development, (\"Document status:** GENERATED_PROJECTION\", \"AR-4A  Composition-root consolidation\", \"AR-4B  Client Mail route ownership\", \"AR-4C  Outbound Mail composition extraction\", \"production_ready=false\", \"Immutable Accepted Phase Provenance\"),"),
        ("require(plan, (\"Document status:** CURRENT_AUTHORITY\", \"Tracking issue:** #266\", \"Current accepted architecture checkpoint:** AR-4A\", \"Next slice:** AR-4B\", \"AR-3   Application Architecture Contract\", \"AR-4A  Composition-root consolidation\", \"AR-4B  Client Mail route ownership\",", "require(plan, (\"Document status:** CURRENT_AUTHORITY\", \"Tracking issue:** #266\", \"Current accepted architecture checkpoint:** AR-4B\", \"Next slice:** AR-4C\", \"AR-4A  Composition-root consolidation\", \"AR-4B  Client Mail route ownership\", \"AR-4C  Outbound Mail composition extraction\","),
        ("    require(ar4a_evidence, (\"AR-4A Composition-root consolidation\", \"EVIDENCE / AR-4A accepted\", \"f257a30a1df437812edb5c9e4b33c3de7e0740bc\", \"74672285ef0146c2dc6da298024b378438e5a75d\", \"AR-4B\", \"AR-4C\", \"Production Core remains `BLOCKED`\"), \"AR-4A evidence\", errors)\n", "    require(ar4a_evidence, (\"AR-4A Composition-root consolidation\", \"EVIDENCE / AR-4A accepted\", \"f257a30a1df437812edb5c9e4b33c3de7e0740bc\", \"74672285ef0146c2dc6da298024b378438e5a75d\", \"AR-4B\", \"AR-4C\", \"Production Core remains `BLOCKED`\"), \"AR-4A evidence\", errors)\n    require(ar4b_evidence, (\"AR-4B Client Mail route ownership\", \"EVIDENCE / AR-4B accepted\", GREEN_HEAD if False else \"7ccdd1b0ed0c0eae974cd9bde15c87524315c023\", \"04b62c97813010ac283d8b70c81089f1c16f5672\", \"AR-4C\", \"Production Core remains `BLOCKED`\"), \"AR-4B evidence\", errors)\n"),
        ("(\"slice rollback\", Path(\"docs/status.json\"), '\"current_slice\": \"AR-4A\"', '\"current_slice\": \"AR-3\"', \"current_slice\")", "(\"slice rollback\", Path(\"docs/status.json\"), '\"current_slice\": \"AR-4B\"', '\"current_slice\": \"AR-4A\"', \"current_slice\")"),
        ("Architecture Re-baseline v3 AR-4A documentation authority negative fixtures passed.", "Architecture Re-baseline v3 AR-4B documentation authority negative fixtures passed."),
        ("Architecture Re-baseline v3 AR-4A documentation/program authority is consistent.", "Architecture Re-baseline v3 AR-4B documentation/program authority is consistent."),
    ],
)

print("AR-4B closeout authority sources updated; run canonical checks and inventory regeneration next.")
