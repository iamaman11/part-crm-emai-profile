#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AR5_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR5.md"
HEAD = "afed435bb714794d6c4f252be6b44c592ee31b2b"
MERGE = "82d251a1d6666199c6eace393eedc1766157fcee"


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one marker {old!r}, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")


def insert_after(path: str, marker: str, addition: str) -> None:
    replace_once(path, marker, marker + addition)


def write_evidence() -> None:
    lines = [
        "# Architecture Re-baseline v3 — AR-5 Wrangler / Runtime Authority Cleanup",
        "",
        "**Document status:** EVIDENCE / AR-5 accepted  ",
        "**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  ",
        "**Tracking:** #266 / implementation #290 / closeout #292  ",
        "**Implementation PR:** #291  ",
        f"**Exact-green implementation head:** `{HEAD}`  ",
        f"**Accepted implementation merge:** `{MERGE}`  ",
        "**Applicable permanent workflows:** **13/13 success** on the unchanged exact head  ",
        "**Production mutation:** forbidden",
        "",
        "## 1. Purpose",
        "",
        "AR-5 applies the accepted AR-2 `GENERATION_VERIFICATION = DELETE` topology decision to the canonical runtime and deployment authority. The implementation removes a legacy Queue identity that had no accepted Queue envelope workload or independent consumer while preserving synchronous profile-generation verification through `ProfileGenerationVerifyApi -> execute_verify_generation`.",
        "",
        "AR-5 is runtime/deployment authority remediation. It does not replace the accepted AR-4C application-architecture remediation: `application_architecture` remains accepted through AR-4C and AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it.",
        "",
        "## 2. Accepted runtime authority",
        "",
        "The accepted control-plane Queue producer authority is exactly:",
        "",
        "- `INTEGRATION_EVENTS`;",
        "- `MAILBOX_JOBS`.",
        "",
        "`MAILBOX_JOBS` retains its accepted DLQ/consumer semantics. `GENERATION_VERIFICATION` is absent from canonical staging/production Wrangler producer bindings, the control-plane contract constant set, the `/bindings` runtime probe, deployment-manifest authority, and the control-plane Queue workload model.",
        "",
        "Generation verification remains synchronous application authority; AR-5 does not change its public HTTP route, authentication, state machine, D1/R2 behavior, or application use case.",
        "",
        "## 3. Accepted evidence",
        "",
        "- implementation issue: #290;",
        "- implementation PR: #291;",
        f"- exact-green implementation head: `{HEAD}`;",
        f"- accepted implementation merge: `{MERGE}`;",
        "- permanent PR workflows: **13/13 success**;",
        "- implementation branch at acceptance: `behind_by=0`;",
        "- unresolved review threads: **0**;",
        "- blocking reviews: **0**.",
        "",
        "The initial implementation candidate correctly failed the permanent Quality Gate because `scripts/cloudflare-deploy-config.py` still encoded the obsolete three-producer deployment model. The candidate was corrected rather than weakening the gate; the final exact head passed canonical Cloudflare deploy configuration, fail-closed environment fixtures, runtime binding topology, immutable release provenance, D3/bootstrap authority, Rust/WASM/native/Windows builds and the rest of the permanent workflow set.",
        "",
        "## 4. Preserved invariants",
        "",
        "- accepted AR-2 runtime-topology decision remains the provenance input;",
        "- AR-4C remains the latest accepted application-architecture remediation;",
        "- AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it;",
        "- resolver Worker/D1/service isolation remains intact;",
        "- `INTEGRATION_EVENTS`, `MAILBOX_JOBS` and mailbox DLQ remain real transport boundaries;",
        "- required secret names and staging/production isolation are unchanged;",
        "- no public HTTP/OpenAPI semantics changed;",
        "- no D1 migration/schema changed;",
        "- no Cloudflare/provider production resource was created, updated or deleted;",
        "- `architecture_complete=false`;",
        "- Production Core remains `BLOCKED`;",
        "- `production_ready=false`.",
        "",
        "## 5. Accepted machine state and handoff",
        "",
        "After this mandatory post-merge authority closeout:",
        "",
        "```text",
        "accepted architecture checkpoint = AR-5",
        "runtime authority cleanup = ACCEPTED",
        "application architecture = ACCEPTED_THROUGH_AR4C",
        "AR-4D = NOT_REQUIRED",
        "next slice = AR-6 — Full Python Estate + read-only Rust opsctl",
        "architecture_complete = false",
        "production_core_gate = BLOCKED",
        "production_ready = false",
        "production_mutation = false",
        "```",
        "",
        "AR-6 must start from the accepted AR-5 main state and must not reintroduce a second mutable authority for any lifecycle concern.",
        "",
    ]
    (ROOT / AR5_EVIDENCE).write_text("\n".join(lines), encoding="utf-8", newline="\n")


def update_markdown() -> None:
    replace_once(
        "README.md",
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B and AR-4C.\n- **Current accepted checkpoint:** AR-4C — Outbound Mail composition extraction.\n- **Next slice:** AR-5 — Wrangler / Runtime Authority Cleanup.",
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C and AR-5.\n- **Current accepted checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup.\n- **Next slice:** AR-6 — Full Python Estate + read-only Rust opsctl.",
    )
    replace_once(
        "README.md",
        "#266. AR-4C Outbound Mail composition extraction extends the AR-4B-remediated accepted AR-3 application/runtime ownership contract in",
        "#266. AR-5 Wrangler / Runtime Authority Cleanup accepts the AR-2 generation-verification deletion in canonical runtime/deployment authority while the AR-4C-remediated accepted AR-3 application/runtime ownership contract remains in",
    )
    insert_after(
        "README.md",
        "- [`docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`](docs/ARCHITECTURE_REBASELINE_V3_PLAN.md) — single current\n  architecture/program execution authority, issue #266;\n",
        "- [`docs/ARCHITECTURE_REBASELINE_V3_AR5.md`](docs/ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;\n",
    )

    replace_once(
        "docs/README.md",
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B and AR-4C.\n- **Current accepted checkpoint:** AR-4C — Outbound Mail composition extraction.\n- **Next slice:** AR-5 — Wrangler / Runtime Authority Cleanup.",
        "- **Accepted architecture slices:** AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C and AR-5.\n- **Current accepted checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup.\n- **Next slice:** AR-6 — Full Python Estate + read-only Rust opsctl.",
    )
    insert_after(
        "docs/README.md",
        "- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md) — CURRENT_AUTHORITY, issue #266;\n",
        "- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;\n",
    )
    replace_once(
        "docs/README.md",
        "preserving its repository-side D3 foundation; AR-4C is accepted, AR-4D remains NOT_REQUIRED, and AR-5 is the only next architecture slice.",
        "preserving its repository-side D3 foundation; AR-5 is accepted, AR-4C remains the latest application-architecture remediation, AR-4D remains NOT_REQUIRED, and AR-6 is the only next architecture slice.",
    )

    replace_once(
        "docs/INDEX.md",
        "- AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B and AR-4C are accepted checkpoints.\n- AR-4C — Outbound Mail composition extraction is the current accepted checkpoint.\n- AR-5 — Wrangler / Runtime Authority Cleanup is the only next slice; AR-4D remains NOT_REQUIRED unless later accepted evidence reopens it.",
        "- AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C and AR-5 are accepted checkpoints.\n- AR-5 — Wrangler / Runtime Authority Cleanup is the current accepted checkpoint.\n- AR-6 — Full Python Estate + read-only Rust opsctl is the only next slice; AR-4D remains NOT_REQUIRED unless later accepted evidence reopens it.",
    )
    replace_once(
        "docs/INDEX.md",
        "accepted AR-2 topology/D3 decision input retained by the AR-4C-remediated accepted AR-3 application/runtime ownership projection.",
        "accepted AR-2 topology/D3 decision input; AR-5 has now accepted its generation-verification runtime/deployment cleanup while the application/runtime ownership projection remains accepted through AR-4C.",
    )
    insert_after(
        "docs/INDEX.md",
        "- [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)\n",
        "- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md)\n",
    )

    replace_once(
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "**Current accepted architecture checkpoint:** AR-4C — Outbound Mail composition extraction\n**Next slice:** AR-5 — Wrangler / Runtime Authority Cleanup",
        "**Current accepted architecture checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup\n**Next slice:** AR-6 — Full Python Estate + read-only Rust opsctl",
    )
    replace_once(
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-4C is the latest accepted checkpoint. Its Outbound Mail composition extraction extends the AR-4B-remediated accepted AR-3 application/runtime ownership contract in `architecture/inventory.json`, with AR-4C acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR4C.md`, AR-4B and AR-4A evidence preserved in their evidence documents, and the AR-3 base contract preserved in `docs/ARCHITECTURE_REBASELINE_V3_AR3.md`; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`. AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it.",
        "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-5 is the latest accepted checkpoint. Its Wrangler / Runtime Authority Cleanup applies the accepted AR-2 `GENERATION_VERIFICATION = DELETE` decision to canonical runtime/deployment authority, with acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR5.md`. The latest application-architecture remediation remains AR-4C in `architecture/inventory.json`, with AR-4C/AR-4B/AR-4A evidence preserved and the AR-3 base contract unchanged; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`. AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it.",
    )
    replace_once(
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "AR-4C  Outbound Mail composition extraction                      CURRENT / ACCEPTED CHECKPOINT\nAR-4D  Profile extraction — NOT REQUIRED by AR-3; reopen only by later accepted evidence\nAR-5   Wrangler / Runtime Authority Cleanup                     NEXT\nAR-6   Full Python Estate + read-only Rust opsctl",
        "AR-4C  Outbound Mail composition extraction                      DONE / ACCEPTED\nAR-4D  Profile extraction — NOT REQUIRED by AR-3; reopen only by later accepted evidence\nAR-5   Wrangler / Runtime Authority Cleanup                     CURRENT / ACCEPTED CHECKPOINT\nAR-6   Full Python Estate + read-only Rust opsctl                NEXT",
    )
    replace_once(
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        "- `GENERATION_VERIFICATION = DELETE`; source/Wrangler binding removal belongs to AR-5 and the queue must not be provisioned by PC-1;",
        "- `GENERATION_VERIFICATION = DELETE`; AR-5 accepted removal from canonical source/Wrangler/deployment authority, and the queue must not be provisioned by PC-1;",
    )

    replace_once(
        "docs/DEVELOPMENT_PLAN.md",
        "**Current accepted architecture checkpoint:** AR-4C — Outbound Mail composition extraction\n**Next architecture slice:** AR-5 — Wrangler / Runtime Authority Cleanup",
        "**Current accepted architecture checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup\n**Next architecture slice:** AR-6 — Full Python Estate + read-only Rust opsctl",
    )
    replace_once(
        "docs/DEVELOPMENT_PLAN.md",
        "- AR-4C — Outbound Mail composition extraction: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-4D remains **NOT REQUIRED** unless later accepted evidence reopens it.\n- AR-5 — Wrangler / Runtime Authority Cleanup: **NEXT**.\n- AR-6…AR-17: ordered future architecture slices.",
        "- AR-4C — Outbound Mail composition extraction: **DONE / ACCEPTED**.\n- AR-4D remains **NOT REQUIRED** unless later accepted evidence reopens it.\n- AR-5 — Wrangler / Runtime Authority Cleanup: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-6 — Full Python Estate + read-only Rust opsctl: **NEXT**.\n- AR-7…AR-17: ordered future architecture slices.",
    )
    replace_once(
        "docs/DEVELOPMENT_PLAN.md",
        "- `GENERATION_VERIFICATION=DELETE`; source/Wrangler binding cleanup belongs to AR-5, not AR-2.",
        "- `GENERATION_VERIFICATION=DELETE`; AR-5 accepted source/Wrangler/deployment authority cleanup while preserving synchronous verification semantics.",
    )
    replace_once(
        "docs/DEVELOPMENT_PLAN.md",
        "AR-4C  Outbound Mail composition extraction                      CURRENT / ACCEPTED CHECKPOINT\nAR-4D  Profile extraction — NOT REQUIRED by AR-3; reopen only by later accepted evidence\nAR-5   Wrangler / Runtime Authority Cleanup                     NEXT\nAR-6   Full Python Estate + read-only Rust opsctl",
        "AR-4C  Outbound Mail composition extraction                      DONE / ACCEPTED\nAR-4D  Profile extraction — NOT REQUIRED by AR-3; reopen only by later accepted evidence\nAR-5   Wrangler / Runtime Authority Cleanup                     CURRENT / ACCEPTED CHECKPOINT\nAR-6   Full Python Estate + read-only Rust opsctl                NEXT",
    )


def update_status() -> None:
    path = ROOT / "docs/status.json"
    status = json.loads(path.read_text(encoding="utf-8"))
    program = status["current"]["architecture_program"]
    if program["current_slice"] != "AR-4C" or program["next_slice_after_acceptance"] != "AR-5":
        raise SystemExit("status baseline is not accepted AR-4C -> AR-5")
    if "AR-5" in program["accepted_slices"]:
        raise SystemExit("AR-5 unexpectedly already accepted in status baseline")
    program["accepted_slices"].append("AR-5")
    program["current_slice"] = "AR-5"
    program["next_slice_after_acceptance"] = "AR-6"
    program["runtime_authority_cleanup_evidence"] = AR5_EVIDENCE
    program["ar5_acceptance"] = {
        "issue": 290,
        "implementation_pr": 291,
        "exact_green_head": HEAD,
        "implementation_merge": MERGE,
        "applicable_permanent_workflows": "13/13",
        "closeout_issue": 292,
    }
    status["next_repository_step"]["name"] = "Architecture Re-baseline v3 AR-6"
    status["next_repository_step"]["status"] = "next_after_accepted_ar5_merge"
    status["implementation"]["architecture_rebaseline_v3"] = "active_issue_266_ar5_accepted_next_ar6"
    path.write_text(json.dumps(status, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def update_transition() -> None:
    path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
    transition = json.loads(path.read_text(encoding="utf-8"))
    if transition["current_slice"] != "AR-4C" or transition["next_slice_after_acceptance"] != "AR-5":
        raise SystemExit("transition baseline is not accepted AR-4C -> AR-5")
    transition["schema_version"] = 8
    transition["status"] = "ACTIVE_AFTER_ACCEPTED_AR5_MERGE"
    transition["accepted_slices"].append("AR-5")
    transition["current_slice"] = "AR-5"
    transition["next_slice_after_acceptance"] = "AR-6"
    transition["runtime_topology"]["generation_verification_source_binding_removal"] = "ACCEPTED_AR5"
    transition["runtime_topology"]["runtime_authority_cleanup_evidence"] = AR5_EVIDENCE
    transition["architecture_inventory_policy"]["ar5_remediation"] = "ACCEPTED_RUNTIME_AUTHORITY_CLEANUP_IN_CANONICAL_INVENTORY"
    transition["application_architecture"]["program_handoff_status"] = "AR-5_ACCEPTED"
    transition["application_architecture"]["program_next_required_slice"] = "AR-6"
    transition["runtime_authority_cleanup"] = {
        "status": "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP",
        "evidence": AR5_EVIDENCE,
        "implementation_issue": 290,
        "implementation_pr": 291,
        "exact_green_head": HEAD,
        "implementation_merge": MERGE,
        "applicable_permanent_workflows": "13/13",
        "generation_verification": {
            "topology_decision": "DELETE",
            "wrangler_producer_binding": "ABSENT",
            "runtime_contract_binding": "ABSENT",
            "deployment_manifest_identity": "ABSENT",
            "queue_workload": "ABSENT",
            "verification_authority": "SYNCHRONOUS_APPLICATION_ROUTE",
        },
        "preserved_queue_producers": ["INTEGRATION_EVENTS", "MAILBOX_JOBS"],
        "production_mutation": False,
        "next_required_slice": "AR-6",
    }
    path.write_text(json.dumps(transition, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def update_checker() -> None:
    path = "scripts/check-documentation-authority.py"
    replace_once(path, 'CURRENT_SLICE = "AR-4C"\nNEXT_SLICE = "AR-5"', 'CURRENT_SLICE = "AR-5"\nNEXT_SLICE = "AR-6"')
    replace_once(path, 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C"]', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5"]')
    insert_after(path, 'AR4C_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR4C.md")\n', 'AR5_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR5.md")\n')
    insert_after(path, '    AR4C_EVIDENCE,\n', '    AR5_EVIDENCE,\n')
    insert_after(path, '        ar4c_evidence = read(root, AR4C_EVIDENCE)\n', '        ar5_evidence = read(root, AR5_EVIDENCE)\n')
    replace_once(path, 'docs/status.json must be the current AR-4C schema/date projection', 'docs/status.json must be the current AR-5 schema/date projection')
    replace_once(path, 'production_ready must remain false throughout accepted AR-4C', 'production_ready must remain false throughout accepted AR-5')
    replace_once(path, 'AR-4C architecture_complete/Production Core gate state must remain fail closed', 'AR-5 architecture_complete/Production Core gate state must remain fail closed')
    replace_once(path, 'if transition.get("schema_version") != 7 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR4C_MERGE":\n        errors.append("architecture transition must encode accepted AR-4C state")', 'if transition.get("schema_version") != 8 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR5_MERGE":\n        errors.append("architecture transition must encode accepted AR-5 state")')
    replace_once(path, 'architecture transition must encode AR-4C -> AR-5 sequencing', 'architecture transition must encode AR-5 -> AR-6 sequencing')
    replace_once(path, 'transition state must remain fail closed through AR-4C', 'transition state must remain fail closed through AR-5')
    replace_once(path, 'architecture inventory AR-4C program state is stale', 'architecture inventory AR-5 program state is stale')
    replace_once(path, '    common = ("Architecture Re-baseline v3", "issue #266", "AR-4C", "AR-5", "production_ready=false")', '    common = ("Architecture Re-baseline v3", "issue #266", "AR-5", "AR-6", "production_ready=false")')
    replace_once(path, '    require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-4C", "AR-5"), "docs/INDEX.md", errors)', '    require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-5", "AR-6"), "docs/INDEX.md", errors)')
    replace_once(path, '    require(development, ("Document status:** GENERATED_PROJECTION", "AR-4B  Client Mail route ownership", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)', '    require(development, ("Document status:** GENERATED_PROJECTION", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "production_ready=false", "Immutable Accepted Phase Provenance"), "docs/DEVELOPMENT_PLAN.md", errors)')
    replace_once(path, '    require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-4C", "Next slice:** AR-5", "AR-4B  Client Mail route ownership", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)', '    require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-5", "Next slice:** AR-6", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "source_present != production_enabled", "No production provisioning, promotion or other real production mutation is an AR-0…AR-17 activity"), "current v3 plan", errors)')
    insert_after(path, '    require(ar4c_evidence, ("AR-4C Outbound Mail composition extraction", "EVIDENCE / AR-4C accepted", "c62f3a7fb00acf16fa1a8a00d9d2f101949cf8a3", "d8382d1578c4911287fb76dd0b9966b23aa85c25", "AR-5", "Production Core remains `BLOCKED`"), "AR-4C evidence", errors)\n', '    require(ar5_evidence, ("AR-5 Wrangler / Runtime Authority Cleanup", "EVIDENCE / AR-5 accepted", "afed435bb714794d6c4f252be6b44c592ee31b2b", "82d251a1d6666199c6eace393eedc1766157fcee", "13/13 success", "AR-6", "Production Core remains `BLOCKED`"), "AR-5 evidence", errors)\n')
    replace_once(path, '        ("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-4C"\', \'"current_slice": "AR-4B"\', "current_slice"),', '        ("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-5"\', \'"current_slice": "AR-4C"\', "current_slice"),')
    replace_once(path, 'Architecture Re-baseline v3 AR-4C documentation authority negative fixtures passed.', 'Architecture Re-baseline v3 AR-5 documentation authority negative fixtures passed.')
    replace_once(path, 'Architecture Re-baseline v3 AR-4C documentation/program authority is consistent.', 'Architecture Re-baseline v3 AR-5 documentation/program authority is consistent.')

    insert_after(
        path,
        '    if program.get("accepted_slices") != ACCEPTED_SLICES:\n        errors.append(f"docs/status.json accepted_slices must be {ACCEPTED_SLICES!r}")\n',
        '    ar5 = program.get("ar5_acceptance") if isinstance(program.get("ar5_acceptance"), dict) else {}\n'
        '    if (\n'
        '        program.get("runtime_authority_cleanup_evidence") != str(AR5_EVIDENCE)\n'
        '        or ar5.get("issue") != 290\n'
        '        or ar5.get("implementation_pr") != 291\n'
        '        or ar5.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"\n'
        '        or ar5.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"\n'
        '        or ar5.get("applicable_permanent_workflows") != "13/13"\n'
        '    ):\n'
        '        errors.append("docs/status.json AR-5 acceptance provenance drifted")\n',
    )
    insert_after(
        path,
        '    if runtime.get("decision_authority") != str(TOPOLOGY) or runtime.get("generation_verification_decision") != "DELETE" or runtime.get("legacy_d3_production_forward_execution") != "DISABLED":\n        errors.append("transition lost accepted AR-2 runtime-topology decisions")\n',
        '    cleanup = transition.get("runtime_authority_cleanup") if isinstance(transition.get("runtime_authority_cleanup"), dict) else {}\n'
        '    if (\n'
        '        runtime.get("generation_verification_source_binding_removal") != "ACCEPTED_AR5"\n'
        '        or runtime.get("runtime_authority_cleanup_evidence") != str(AR5_EVIDENCE)\n'
        '        or cleanup.get("status") != "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP"\n'
        '        or cleanup.get("evidence") != str(AR5_EVIDENCE)\n'
        '        or cleanup.get("implementation_pr") != 291\n'
        '        or cleanup.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"\n'
        '        or cleanup.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"\n'
        '        or cleanup.get("next_required_slice") != "AR-6"\n'
        '        or cleanup.get("production_mutation") is not False\n'
        '    ):\n'
        '        errors.append("transition AR-5 runtime-authority cleanup acceptance drifted")\n',
    )
    insert_after(
        path,
        '    if program_state.get("production_ready") is not False or program_state.get("production_core_gate") != "BLOCKED":\n        errors.append("architecture inventory must remain fail closed")\n',
        '    inventory_cleanup = inventory.get("runtime_authority_cleanup") if isinstance(inventory.get("runtime_authority_cleanup"), dict) else {}\n'
        '    if (\n'
        '        inventory_cleanup.get("status") != "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP"\n'
        '        or inventory_cleanup.get("evidence") != str(AR5_EVIDENCE)\n'
        '        or inventory_cleanup.get("implementation_pr") != 291\n'
        '        or inventory_cleanup.get("exact_green_head") != "afed435bb714794d6c4f252be6b44c592ee31b2b"\n'
        '        or inventory_cleanup.get("implementation_merge") != "82d251a1d6666199c6eace393eedc1766157fcee"\n'
        '        or inventory_cleanup.get("next_required_slice") != "AR-6"\n'
        '        or inventory_cleanup.get("production_mutation") is not False\n'
        '    ):\n'
        '        errors.append("architecture inventory AR-5 runtime-authority cleanup projection drifted")\n',
    )


def update_generator() -> None:
    path = "scripts/generate-architecture-inventory.py"
    insert_after(path, 'AR4C_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"\n', 'AR5_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR5.md"\n')
    replace_once(path, 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C"]\nCURRENT_SLICE = "AR-4C"\nNEXT_SLICE = "AR-5"', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5"]\nCURRENT_SLICE = "AR-5"\nNEXT_SLICE = "AR-6"')
    insert_after(path, '    {"path": AR4C_EVIDENCE, "status": "EVIDENCE", "scope": "ar4c_outbound_mail_composition_accepted"},\n', '    {"path": AR5_EVIDENCE, "status": "EVIDENCE", "scope": "ar5_runtime_authority_cleanup_accepted"},\n')
    replace_once(path, 'docs/status.json must remain production_ready=false during accepted AR-4C', 'docs/status.json must remain production_ready=false during accepted AR-5')
    replace_once(path, 'docs/status.json must keep accepted AR-4C architecture/gate state fail closed', 'docs/status.json must keep accepted AR-5 architecture/gate state fail closed')
    replace_once(path, 'docs/status.json must project accepted AR-4C -> next AR-5 sequencing', 'docs/status.json must project accepted AR-5 -> next AR-6 sequencing')
    insert_after(
        path,
        '    if program.get("runtime_topology_decision") != RUNTIME_TOPOLOGY:\n        raise SystemExit("docs/status.json must project the accepted AR-2 topology decision")\n',
        '    if program.get("runtime_authority_cleanup_evidence") != AR5_EVIDENCE:\n'
        '        raise SystemExit("docs/status.json must project accepted AR-5 runtime-authority cleanup evidence")\n'
        '    runtime_gate = subprocess.run(\n'
        '        [sys.executable, str(ROOT / "scripts/check-cloudflare-runtime-bindings.py")],\n'
        '        cwd=ROOT, text=True, capture_output=True, check=False,\n'
        '    )\n'
        '    if runtime_gate.returncode != 0:\n'
        '        details = "\\n".join(value.strip() for value in (runtime_gate.stdout, runtime_gate.stderr) if value.strip())\n'
        '        raise SystemExit(f"AR-5 runtime authority gate failed:\\n{details}")\n',
    )
    insert_after(
        path,
        '        "application_architecture": application_architecture,\n',
        '        "runtime_authority_cleanup": {\n'
        '            "schema_version": 1,\n'
        '            "status": "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP",\n'
        '            "topology_decision_source": RUNTIME_TOPOLOGY,\n'
        '            "evidence": AR5_EVIDENCE,\n'
        '            "implementation_issue": 290,\n'
        '            "implementation_pr": 291,\n'
        '            "exact_green_head": "afed435bb714794d6c4f252be6b44c592ee31b2b",\n'
        '            "implementation_merge": "82d251a1d6666199c6eace393eedc1766157fcee",\n'
        '            "applicable_permanent_workflows": "13/13",\n'
        '            "generation_verification": {\n'
        '                "topology_decision": "DELETE",\n'
        '                "wrangler_producer_binding": "ABSENT",\n'
        '                "runtime_contract_binding": "ABSENT",\n'
        '                "deployment_manifest_identity": "ABSENT",\n'
        '                "queue_workload": "ABSENT",\n'
        '                "verification_authority": "SYNCHRONOUS_APPLICATION_ROUTE",\n'
        '            },\n'
        '            "preserved_queue_producers": ["INTEGRATION_EVENTS", "MAILBOX_JOBS"],\n'
        '            "application_architecture_accepted_through": "AR-4C",\n'
        '            "ar4d": "NOT_REQUIRED_UNLESS_LATER_ACCEPTED_EVIDENCE_REOPENS",\n'
        '            "production_mutation": False,\n'
        '            "next_required_slice": "AR-6",\n'
        '        },\n',
    )
    insert_after(path, '            "runtime_topology_projection_owner": "AR-3",\n', '            "runtime_authority_cleanup_evidence": AR5_EVIDENCE,\n')
    insert_after(
        path,
        '    topology = copy.deepcopy(expected)\n    topology["documentation_authority"]["runtime_topology_decision"] = "architecture/other.json"\n    if serialized(topology) == serialized(expected):\n        raise SystemExit("inventory self-test failed to detect topology authority drift")\n',
        '    runtime_cleanup = copy.deepcopy(expected)\n'
        '    runtime_cleanup["runtime_authority_cleanup"]["status"] = "AR5_CANDIDATE"\n'
        '    if serialized(runtime_cleanup) == serialized(expected):\n'
        '        raise SystemExit("inventory self-test failed to detect AR-5 runtime-authority acceptance rollback")\n',
    )
    replace_once(path, 'Architecture inventory accepted AR-4C negative self-test passed.', 'Architecture inventory accepted AR-5 runtime-authority negative self-test passed.')
    replace_once(path, 'Architecture inventory and accepted AR-4C composition projection are current.', 'Architecture inventory and accepted AR-5 runtime-authority projection are current.')


def main() -> None:
    write_evidence()
    update_markdown()
    update_status()
    update_transition()
    update_checker()
    update_generator()


if __name__ == "__main__":
    main()
