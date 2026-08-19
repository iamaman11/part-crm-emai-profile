#!/usr/bin/env python3
"""One-shot deterministic AR-10 acceptance transformer; removed before final merge."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AR10_HEAD = "c7f8ac9704433d3e52d3b79f985c9ac60aa068db"
AR10_MERGE = "7ab5edf583f541d08ff732624af25881d430d427"
AR10_ACCEPTANCE = "docs/evidence/2026-08-19-ar10-final-acceptance.json"
AR11_ISSUE = 372


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old in text:
        return text.replace(old, new, 1)
    if new in text:
        return text
    raise ValueError(f"{label}: expected text not found: {old!r}")


def replace_block(text: str, start: str, end: str, replacement: str, label: str) -> str:
    left = text.find(start)
    if left < 0:
        raise ValueError(f"{label}: start marker not found: {start}")
    right = text.find(end, left + len(start))
    if right < 0:
        raise ValueError(f"{label}: end marker not found: {end}")
    return text[:left] + replacement.rstrip() + "\n\n" + text[right:]


def update_runtime_authority() -> None:
    path = ROOT / "architecture/runtime-cutover-ar10.json"
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("status") not in {"AR10_IMPLEMENTED_PENDING_ACCEPTANCE", "accepted"}:
        raise ValueError("unexpected AR-10 runtime authority state")
    payload["status"] = "accepted"
    payload["acceptance"] = {
        "evidence": AR10_ACCEPTANCE,
        "implementation_pr": 371,
        "exact_green_head": AR10_HEAD,
        "implementation_merge": AR10_MERGE,
        "implementation_main_reread": AR10_MERGE,
        "applicable_permanent_workflows": "16/16",
        "behind_by": 0,
        "blocking_reviews": 0,
        "unresolved_review_threads": 0,
        "hosted_required_status_contexts": 23,
        "production_mutation": False,
    }
    acceptance = payload["repository_acceptance_evidence"]
    acceptance.update(
        {
            "real_camoufox_cold_launch": "ACCEPTED_EXACT_GREEN_HEAD",
            "cookie_and_localstorage_persistence": "ACCEPTED_EXACT_GREEN_HEAD",
            "cross_generation_isolation": "ACCEPTED_EXACT_GREEN_HEAD",
            "identity_mismatch_fail_closed": "ACCEPTED_EXACT_GREEN_HEAD",
            "bridge_to_real_camoufox_chain": "ACCEPTED_EXACT_GREEN_HEAD",
            "windows_profile_bridge_regression": "ACCEPTED_EXACT_GREEN_HEAD",
            "managed_runtime_failure_matrix": "ACCEPTED_EXACT_GREEN_HEAD",
            "firefox_writer_lock_failure_matrix": "ACCEPTED_EXACT_GREEN_HEAD",
            "hosted_camoufox_required_checks": "ACCEPTED_MAIN_23_REQUIRED_CONTEXTS",
        }
    )
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def update_inventory_generator() -> None:
    path = ROOT / "scripts/generate-architecture-inventory.py"
    text = path.read_text(encoding="utf-8")
    pairs = [
        (
            '"""Generate canonical inventory after accepted AR-9 and during AR-10."""',
            '"""Generate canonical inventory after accepted AR-10 and during AR-11."""',
        ),
        (
            'AR9_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar9-final-acceptance.json"',
            'AR9_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar9-final-acceptance.json"\nAR10_ACCEPTANCE_EVIDENCE = "docs/evidence/2026-08-19-ar10-final-acceptance.json"\nAR11_ISSUE = 372',
        ),
        ('engine.CURRENT_SLICE = "AR-10"', 'engine.CURRENT_SLICE = "AR-11"'),
        ('engine.NEXT_SLICE = "AR-11"', 'engine.NEXT_SLICE = "AR-12"'),
        ('engine.CURRENT_DELIVERY_CHECKPOINT = "AR-9"', 'engine.CURRENT_DELIVERY_CHECKPOINT = "AR-10"'),
        ('for accepted in ("AR-8", "AR-9"):', 'for accepted in ("AR-8", "AR-9", "AR-10"):'),
        ('"role": "CURRENT_AR10_RUNTIME_CUTOVER_PROJECTION",', '"role": "ACCEPTED_AR10_RUNTIME_CUTOVER_PROJECTION",\n        "acceptance_evidence": AR10_ACCEPTANCE_EVIDENCE,'),
        ('documentation["ar9_acceptance_evidence"] = AR9_ACCEPTANCE_EVIDENCE', 'documentation["ar9_acceptance_evidence"] = AR9_ACCEPTANCE_EVIDENCE\n    documentation["ar10_acceptance_evidence"] = AR10_ACCEPTANCE_EVIDENCE'),
        ('Wrote architecture/inventory.json with accepted AR-9 and current AR-10 runtime-cutover projection.', 'Wrote architecture/inventory.json with accepted AR-10 runtime cutover and current AR-11 projection.'),
        ('Architecture inventory projects accepted AR-9 and current AR-10 runtime cutover while production remains blocked.', 'Architecture inventory projects accepted AR-10 runtime cutover and current AR-11 while production remains blocked.'),
    ]
    for old, new in pairs:
        text = replace_once(text, old, new, "inventory generator")

    delivery = '''def completion_delivery_map() -> dict[str, object]:
    value = legacy_delivery_map()
    value["accepted_checkpoint"] = "AR-10"
    value["current_work"] = "AR-11"
    value["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-10",
        "current_subslice": "AR-11",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR10_CLOSEOUT",
    }
    value["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-10",
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "ar10_accepted": True,
        "acceptance_evidence": AR10_ACCEPTANCE_EVIDENCE,
    }
    value["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    value["next_gate"] = {"id": "AR-11_ACCEPTANCE", "issue": AR11_ISSUE, "on_success": "AR-12_BECOMES_CURRENT"}
    value["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "ar9_blocked": False,
        "ar10_accepted": True,
        "ar10_blocked": False,
        "ar11_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return value'''
    text = replace_block(text, "def completion_delivery_map(", "def completion_progress(", delivery, "inventory delivery")

    runtime_start = text.find("def runtime_cutover_projection()")
    runtime_end = text.find("def build_inventory()", runtime_start)
    if runtime_start < 0 or runtime_end < 0:
        raise ValueError("runtime projection function not found")
    runtime = text[runtime_start:runtime_end]
    runtime = replace_once(
        runtime,
        '    if authority.get("production_mutation") is not False:',
        '    if authority.get("status") != "accepted":\n        raise ValueError("AR-10 runtime cutover authority must be accepted after accepted main")\n    if authority.get("production_mutation") is not False:',
        "runtime accepted status",
    )
    text = text[:runtime_start] + runtime + text[runtime_end:]
    path.write_text(text, encoding="utf-8", newline="\n")


def update_ar10_gate() -> None:
    path = ROOT / "scripts/check-ar10-runtime-cutover.py"
    text = path.read_text(encoding="utf-8")
    text = replace_once(
        text,
        'AR10_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md")',
        'AR10_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md")\nAR10_ACCEPTANCE_EVIDENCE = Path("docs/evidence/2026-08-19-ar10-final-acceptance.json")',
        "AR10 evidence constant",
    )
    text = replace_once(
        text,
        'if authority.get("schema_version") != 1 or authority.get("status") != "AR10_IMPLEMENTED_PENDING_ACCEPTANCE":\n        fail("AR-10 runtime-cutover machine authority has invalid state")',
        'if authority.get("schema_version") != 1 or authority.get("status") != "accepted":\n        fail("AR-10 runtime-cutover machine authority must be accepted after guarded merge")',
        "AR10 accepted state",
    )
    text = replace_once(
        text,
        '    read_regular(root, AR10_EVIDENCE)',
        '    read_regular(root, AR10_EVIDENCE)\n    evidence = json.loads(read_regular(root, AR10_ACCEPTANCE_EVIDENCE))\n    if evidence.get("kind") != "AR10_FINAL_ACCEPTANCE" or evidence.get("implementation_merge") != "7ab5edf583f541d08ff732624af25881d430d427":\n        fail("AR-10 final acceptance evidence identity drifted")\n    if evidence.get("applicable_permanent_workflows") != "16/16" or evidence.get("production_mutation") is not False:\n        fail("AR-10 final acceptance evidence is incomplete or production-mutating")',
        "AR10 final evidence",
    )
    path.write_text(text, encoding="utf-8", newline="\n")


def update_documentation_checker() -> None:
    path = ROOT / "scripts/check-documentation-authority.py"
    text = path.read_text(encoding="utf-8")
    pairs = [
        ('"""Current documentation authority checker after accepted AR-9 and during AR-10."""', '"""Current documentation authority checker after accepted AR-10 and during AR-11."""'),
        ('AR10_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md")', 'AR10_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md")\nAR10_ACCEPTANCE_EVIDENCE = Path("docs/evidence/2026-08-19-ar10-final-acceptance.json")\nAR11_ISSUE = 372'),
        ('legacy.CURRENT_SLICE = "AR-10"', 'legacy.CURRENT_SLICE = "AR-11"'),
        ('legacy.NEXT_SLICE = "AR-11"', 'legacy.NEXT_SLICE = "AR-12"'),
        ('legacy.CURRENT_DELIVERY_CHECKPOINT = "AR-9"', 'legacy.CURRENT_DELIVERY_CHECKPOINT = "AR-10"'),
        ('for accepted in ("AR-8", "AR-9"):', 'for accepted in ("AR-8", "AR-9", "AR-10"):'),
        ('    AR10_EVIDENCE,\n):', '    AR10_EVIDENCE,\n    AR10_ACCEPTANCE_EVIDENCE,\n):'),
    ]
    for old, new in pairs:
        text = replace_once(text, old, new, "documentation checker")

    delivery = '''def expected_current_delivery_map() -> dict[str, object]:
    base = legacy_expected_current_delivery_map()
    base["accepted_checkpoint"] = "AR-10"
    base["current_work"] = "AR-11"
    base["source_implemented"] = {
        "status": "ACCEPTED",
        "through": "AR-10",
        "current_subslice": "AR-11",
        "current_subslice_source": "ACCEPTED_MAIN_AFTER_AR10_CLOSEOUT",
    }
    base["accepted_on_main"] = {
        "status": "COMPLETE",
        "through": "AR-10",
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "ar10_accepted": True,
        "acceptance_evidence": AR10_ACCEPTANCE_EVIDENCE.as_posix(),
    }
    base["current_blocker"] = {"issue": None, "status": "NONE", "blocks": "NONE"}
    base["next_gate"] = {"id": "AR-11_ACCEPTANCE", "issue": AR11_ISSUE, "on_success": "AR-12_BECOMES_CURRENT"}
    base["invariants"].update({
        "source_present_not_equal_production_enabled": True,
        "full_ar8_accepted": True,
        "ar9_accepted": True,
        "ar9_blocked": False,
        "ar10_accepted": True,
        "ar10_blocked": False,
        "ar11_blocked": False,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    })
    return base'''
    text = replace_block(text, "def expected_current_delivery_map(", "def validate_ar8_progress(", delivery, "documentation delivery")

    ar10 = '''def validate_ar10_runtime_authority(root: Path) -> None:
    source = load_json(root, AR10_AUTHORITY)
    if (
        source.get("kind") != "RUNTIME_CUTOVER_AUTHORITY"
        or source.get("schema_version") != 1
        or source.get("status") != "accepted"
        or source.get("owning_slice") != "AR-10"
        or source.get("owning_issue") != AR10_ISSUE
        or source.get("completion_pr") != 371
    ):
        raise ValueError("AR-10 runtime-cutover accepted source authority identity/state drifted")
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
    accepted = source.get("acceptance")
    if not isinstance(accepted, dict) or accepted.get("implementation_merge") != "7ab5edf583f541d08ff732624af25881d430d427":
        raise ValueError("AR-10 runtime authority lost guarded-merge acceptance provenance")
    evidence = load_json(root, AR10_ACCEPTANCE_EVIDENCE)
    exact = {
        "kind": "AR10_FINAL_ACCEPTANCE",
        "accepted_program_checkpoint": "AR-10",
        "tracking_issue": AR10_ISSUE,
        "implementation_pr": 371,
        "implementation_exact_green_head": "c7f8ac9704433d3e52d3b79f985c9ac60aa068db",
        "implementation_merge": "7ab5edf583f541d08ff732624af25881d430d427",
        "implementation_main_reread": "7ab5edf583f541d08ff732624af25881d430d427",
        "applicable_permanent_workflows": "16/16",
        "failed_workflows": 0,
        "pending_workflows": 0,
        "behind_by": 0,
        "blocking_reviews": 0,
        "unresolved_review_threads": 0,
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_authorized": False,
        "production_enabled": False,
        "production_mutation": False,
        "next_slice": "AR-11",
        "next_slice_issue": AR11_ISSUE,
    }
    for key, wanted in exact.items():
        if evidence.get(key) != wanted:
            raise ValueError(f"AR-10 final acceptance evidence {key} drifted")

    inventory = load_json(root, Path("architecture/inventory.json"))
    projection = inventory.get("runtime_cutover")
    if not isinstance(projection, dict):
        raise ValueError("canonical inventory lost AR-10 runtime_cutover projection")
    if (
        projection.get("source_authority") != AR10_AUTHORITY.as_posix()
        or projection.get("source_status") != "accepted"
        or projection.get("acceptance_evidence") != AR10_ACCEPTANCE_EVIDENCE.as_posix()
        or projection.get("tracking_issue") != AR10_ISSUE
        or projection.get("legacy_executables_remaining") != 0
    ):
        raise ValueError("canonical inventory accepted AR-10 runtime-cutover projection identity drifted")
    for key, wanted in {
        "architecture_complete": False,
        "production_core_gate": "BLOCKED",
        "production_ready": False,
        "production_mutation": False,
    }.items():
        if projection.get(key) != wanted:
            raise ValueError(f"canonical inventory AR-10 runtime projection {key} drifted")'''
    text = replace_block(text, "def validate_ar10_runtime_authority(", "def validate_current_human_projection(", ar10, "AR10 documentation authority")

    human = '''def validate_current_human_projection(root: Path) -> None:
    required = {
        "README.md": ("Current accepted checkpoint:** AR-10", "Current implementation:** AR-11", "COMPLETE THROUGH AR-10", "AR-11 acceptance"),
        "docs/README.md": ("Current accepted checkpoint:** AR-10", "Current implementation:** AR-11", "COMPLETE THROUGH AR-10", "AR-11 acceptance"),
        "docs/INDEX.md": ("AR-10 is accepted", "AR-11 is the current implementation slice", "COMPLETE THROUGH AR-10"),
        "docs/DEVELOPMENT_PLAN.md": ("Current accepted architecture checkpoint:** AR-10", "Current implementation:** AR-11", "AR-10  Runtime and Historical Executable Simplification", "AR-11  Release-set / Promotion Architecture", "COMPLETE THROUGH AR-10"),
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md": ("Current accepted architecture checkpoint:** AR-10", "Current implementation:** AR-11", "AR-10  Runtime and Historical Executable Simplification", "AR-11  Release-set / Promotion Architecture", "Binding `opsctl` evolution contract"),
        "docs/DEVELOPER_CAPABILITY_MATRIX.md": ("AR-10 source is accepted on `main`; AR-11 is the current architecture slice.", "COMPLETE THROUGH AR-10", "AR-11 acceptance", "production_ready=false"),
    }
    for relative, markers in required.items():
        body = (root / relative).read_text(encoding="utf-8")
        for marker in markers:
            if marker not in body:
                raise ValueError(f"{relative} missing accepted/current architecture marker: {marker}")'''
    text = replace_block(text, "def validate_current_human_projection(", "def validate(root:", human, "human documentation projection")
    path.write_text(text, encoding="utf-8", newline="\n")


def update_human_docs() -> None:
    replacements = {
        "README.md": [
            ("Current accepted checkpoint:** AR-9", "Current accepted checkpoint:** AR-10"),
            ("Current implementation:** AR-10", "Current implementation:** AR-11"),
            ("COMPLETE THROUGH AR-9", "COMPLETE THROUGH AR-10"),
            ("AR-10 acceptance", "AR-11 acceptance"),
        ],
        "docs/README.md": [
            ("Current accepted checkpoint:** AR-9", "Current accepted checkpoint:** AR-10"),
            ("Current implementation:** AR-10", "Current implementation:** AR-11"),
            ("COMPLETE THROUGH AR-9", "COMPLETE THROUGH AR-10"),
            ("AR-10 acceptance", "AR-11 acceptance"),
        ],
        "docs/INDEX.md": [
            ("AR-9 is accepted", "AR-10 is accepted"),
            ("AR-10 is the current implementation slice", "AR-11 is the current implementation slice"),
            ("COMPLETE THROUGH AR-9", "COMPLETE THROUGH AR-10"),
        ],
        "docs/DEVELOPMENT_PLAN.md": [
            ("Current accepted architecture checkpoint:** AR-9", "Current accepted architecture checkpoint:** AR-10"),
            ("Current implementation:** AR-10", "Current implementation:** AR-11"),
            ("COMPLETE THROUGH AR-9", "COMPLETE THROUGH AR-10"),
            ("AR-10 acceptance", "AR-11 acceptance"),
            ("AR-10  Runtime and Historical Executable Simplification              CURRENT", "AR-10  Runtime and Historical Executable Simplification              DONE / ACCEPTED"),
            ("AR-11  Release-set / Promotion Architecture", "AR-11  Release-set / Promotion Architecture                           CURRENT"),
        ],
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md": [
            ("Current accepted architecture checkpoint:** AR-9", "Current accepted architecture checkpoint:** AR-10"),
            ("Current implementation:** AR-10", "Current implementation:** AR-11"),
            ("COMPLETE THROUGH AR-9", "COMPLETE THROUGH AR-10"),
            ("AR-10 acceptance", "AR-11 acceptance"),
            ("AR-10  Runtime and Historical Executable Simplification              CURRENT", "AR-10  Runtime and Historical Executable Simplification              DONE / ACCEPTED"),
            ("AR-11  Release-set / Promotion Architecture", "AR-11  Release-set / Promotion Architecture                           CURRENT"),
            ("AR-9 source is accepted on `main`; AR-10 is the current architecture slice.", "AR-10 source is accepted on `main`; AR-11 is the current architecture slice."),
        ],
        "docs/DEVELOPER_CAPABILITY_MATRIX.md": [
            ("AR-9 source is accepted on `main`; AR-10 is the current architecture slice.", "AR-10 source is accepted on `main`; AR-11 is the current architecture slice."),
            ("COMPLETE THROUGH AR-9", "COMPLETE THROUGH AR-10"),
            ("AR-10 acceptance", "AR-11 acceptance"),
        ],
    }
    for relative, pairs in replacements.items():
        path = ROOT / relative
        body = path.read_text(encoding="utf-8")
        for old, new in pairs:
            body = replace_once(body, old, new, relative)
        path.write_text(body, encoding="utf-8", newline="\n")

    plan_path = ROOT / "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
    plan = plan_path.read_text(encoding="utf-8")
    old = "AR-9 is the latest accepted checkpoint, accepted through PR #367 / accepted main `5933a5e30a534209138485556b4a895706af765a` with repository evidence in `docs/evidence/2026-08-19-ar9-final-acceptance.json`."
    new = f"AR-10 is the latest accepted checkpoint, accepted through PR #371 / accepted main `{AR10_MERGE}` with repository evidence in `{AR10_ACCEPTANCE}`; AR-11 is current under issue #372."
    plan = replace_once(plan, old, new, "plan authority paragraph")
    plan_path.write_text(plan, encoding="utf-8", newline="\n")

    evidence_path = ROOT / "docs/ARCHITECTURE_REBASELINE_V3_AR10.md"
    body = evidence_path.read_text(encoding="utf-8")
    marker = "## Final acceptance — 2026-08-19"
    if marker not in body:
        body += f"""\n\n{marker}\n\nAR-10 is accepted. Implementation PR #371 was exact-head green at `{AR10_HEAD}` across 16/16 permanent workflows, merged by guarded squash to accepted `main` `{AR10_MERGE}`, and reread from that accepted main. Live `main` protection preserves the accepted AR-7 required-check baseline and additionally requires `Real Camoufox cold-launch proof` plus `Profile Bridge Windows regression` (23 required contexts total).\n\nThe accepted runtime path is the native Profile Bridge -> bounded typed/versioned IPC -> real pinned Camouhost/Camoufox persistent-context chain. Persistent browser state, generation-stable fingerprint identity, OS-level Firefox writer ownership, rollback/ambiguous-close behavior, malformed/oversized/replayed IPC rejection, zero remaining AR-10 historical direct executables, and zero `opsctl` production child-process spawn authority are permanent regression evidence.\n\nAcceptance does **not** authorize production: `architecture_complete=false`, `production_core_gate=BLOCKED`, `production_ready=false`, `production_mutation=false`. AR-11 — Release-set / Promotion Architecture is current under issue #372; AR-12 follows only after AR-11 acceptance. Canonical machine evidence is `{AR10_ACCEPTANCE}`.\n"""
        evidence_path.write_text(body, encoding="utf-8", newline="\n")


def main() -> int:
    update_runtime_authority()
    update_inventory_generator()
    update_ar10_gate()
    update_documentation_checker()
    update_human_docs()
    print("Prepared deterministic AR-10 accepted / AR-11 current source authorities.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
