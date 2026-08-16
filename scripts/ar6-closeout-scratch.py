#!/usr/bin/env python3
"""Scratch-only generator for the AR-6 post-merge authority closeout."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXACT_GREEN = "9b06d542873ffa3122e53e107105098e21f5933c"
IMPLEMENTATION_MERGE = "d0229fedd81ee870822b6d9394bc4ee313ea3a3c"
AR6_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR6.md"
PYTHON_ESTATE = "architecture/python-estate-ar6.json"


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, value: str) -> None:
    (ROOT / path).write_text(value, encoding="utf-8", newline="\n")


def replace_once(path: str, old: str, new: str) -> None:
    value = read(path)
    count = value.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one marker, found {count}: {old!r}")
    write(path, value.replace(old, new, 1))


def replace_many(path: str, pairs: list[tuple[str, str]]) -> None:
    for old, new in pairs:
        replace_once(path, old, new)


def load_json(path: str) -> dict:
    value = json.loads(read(path))
    if not isinstance(value, dict):
        raise SystemExit(f"{path} must contain a JSON object")
    return value


def write_json(path: str, value: dict) -> None:
    write(path, json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def write_evidence() -> None:
    write(
        AR6_EVIDENCE,
        f"""# Architecture Re-baseline v3 — AR-6 Full Python Estate + read-only Rust opsctl

**Document status:** EVIDENCE / AR-6 accepted  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Tracking:** #266 / implementation #294 / closeout #296  
**Implementation PR:** #295  
**Exact-green implementation head:** `{EXACT_GREEN}`  
**Accepted implementation merge:** `{IMPLEMENTATION_MERGE}`  
**Applicable permanent workflows:** **13/13 success** on the unchanged exact head  
**Production mutation:** forbidden

## 1. Purpose

AR-6 establishes one fail-closed disposition for the complete Git-tracked Python estate and a bounded Rust operator-tool foundation without authorizing a global Python-to-Rust rewrite. It also establishes `tools/opsctl` as project-specific operator tooling while keeping application/domain/runtime code independent of it.

AR-6 is an operational/tooling authority dimension. It does not pretend that application architecture advanced beyond the accepted AR-4C remediation, and it does not replace the accepted AR-5 Wrangler/runtime-authority cleanup.

## 2. Accepted Python estate

`architecture/python-estate-ar6.json` is the accepted machine-readable disposition of all **116** Git-tracked Python files. Every tracked file is AST-parsed and semantically capability-audited; unknown tracked Python fails closed.

Accepted classification summary:

- `KEEP_PYTHON`: **108**;
- `MIGRATE_TO_RUST`: **2**;
- `DELETE_AFTER_SEQUENCE`: **6**;
- `WRAP_WITH_RUST`: **0**.

The two AR-11 migrations are the legacy D3 promotion entrypoint/core and target the future `opsctl` release/promotion command family. The six AR-10 retirements are retired markers or direct legacy browser/profile/R2 executables whose removal requires their recorded compatibility and retirement proofs.

The semantic audit deliberately rejected filename-only trust. It initially surfaced `scripts/check-external-review-attestations.py` because it performs bounded GitHub API reads and can consume a workflow-scoped token. Manual review confirmed that it is a read-only attestation verifier with no mutation surface, so it is an explicit `KEEP_PYTHON` exception rather than an implicit `check-*` allowance.

## 3. Accepted opsctl boundary

`tools/opsctl` is a standalone, dependency-free Rust workspace. AR-6 accepts exactly these commands:

- `doctor` — execute the two canonical Python repository validators in read-only `--check` mode;
- `status` — return canonical `docs/status.json`;
- `inventory` — return canonical `architecture/inventory.json`.

Permanent checks reject mutation commands, additional process-spawn sites, filesystem/environment mutation capabilities, network/provider/database/secret clients, and third-party Rust dependencies. `opsctl` remains an operator interface, not application business logic and not a competing state registry.

GitHub Actions / Environments remain orchestration, approval, concurrency and credential boundaries. Wrangler/provider APIs remain the eventual low-level provider mutation executors when a later owning slice explicitly authorizes mutation.

## 4. Cross-platform evidence and finding

The exact AR-6 candidate built and tested `opsctl` on Linux and Windows and executed `doctor`, `status`, and `inventory` on Windows. The first Windows execution correctly exposed a pre-existing portability defect in canonical documentation authority: repository-relative machine identities were compared using platform-native `str(Path(...))`.

AR-6 fixed the canonical validator rather than adding an `opsctl` workaround: repository identities now use POSIX representation through `Path.as_posix()`. The final unchanged exact head then passed real `opsctl.exe doctor/status/inventory` execution in the permanent Windows Local Profile Gate.

## 5. Accepted evidence

- implementation issue: #294;
- implementation PR: #295;
- exact-green implementation head: `{EXACT_GREEN}`;
- accepted implementation merge: `{IMPLEMENTATION_MERGE}`;
- permanent PR workflows: **13/13 success**;
- implementation branch at acceptance: `behind_by=0`;
- unresolved review threads: **0**;
- blocking reviews: **0**;
- PR Conversation comments: **0**;
- no production/provider/customer-state mutation.

The semantic remediation strengthened the initial candidate before acceptance: the final inventory is **108 KEEP / 2 MIGRATE / 6 DELETE / 0 WRAP**, not the earlier mechanical 111/2/3/0 classification.

## 6. Preserved invariants and handoff

After this mandatory post-merge authority closeout:

```text
accepted architecture checkpoint = AR-6
python / operator-tool authority = ACCEPTED
application architecture = ACCEPTED_THROUGH_AR4C
runtime authority cleanup = ACCEPTED_AR5
AR-4D = NOT_REQUIRED
next slice = AR-7 — Environments + GitHub Governance + Operational Boundaries
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

AR-7 must start from the accepted AR-6 main state. It may harden environments/governance/operational boundaries, but it must not silently widen `opsctl` into a second provider mutation authority or reclassify Python outside the accepted estate without updating the canonical AR-6 disposition and its gates.
""",
    )


def patch_python_estate_generator() -> None:
    replace_many(
        "scripts/python-estate-ar6.py",
        [
            ('"status": "AR6_CANDIDATE_PYTHON_ESTATE",', '"status": "AR6_ACCEPTED_PYTHON_ESTATE",'),
            ('"accepted_program_checkpoint_remains": "AR-5",', '"accepted_program_checkpoint": "AR-6",'),
            ('document.get("status") != "AR6_CANDIDATE_PYTHON_ESTATE"', 'document.get("status") != "AR6_ACCEPTED_PYTHON_ESTATE"'),
        ],
    )


def patch_status() -> None:
    status = load_json("docs/status.json")
    program = status["current"]["architecture_program"]
    if program["current_slice"] != "AR-5" or program["next_slice_after_acceptance"] != "AR-6":
        raise SystemExit("docs/status.json is not at the expected AR-5 -> AR-6 checkpoint")
    program["accepted_slices"].append("AR-6")
    program["current_slice"] = "AR-6"
    program["next_slice_after_acceptance"] = "AR-7"
    program["python_estate"] = PYTHON_ESTATE
    program["python_operational_evidence"] = AR6_EVIDENCE
    program["ar6_acceptance"] = {
        "issue": 294,
        "implementation_pr": 295,
        "exact_green_head": EXACT_GREEN,
        "implementation_merge": IMPLEMENTATION_MERGE,
        "applicable_permanent_workflows": "13/13",
        "closeout_issue": 296,
    }
    write_json("docs/status.json", status)


def patch_transition() -> None:
    value = load_json("architecture/architecture-rebaseline-v3-transition.json")
    if value["schema_version"] != 8 or value["current_slice"] != "AR-5":
        raise SystemExit("transition is not at accepted AR-5")
    value["schema_version"] = 9
    value["status"] = "ACTIVE_AFTER_ACCEPTED_AR6_MERGE"
    value["accepted_slices"].append("AR-6")
    value["current_slice"] = "AR-6"
    value["next_slice_after_acceptance"] = "AR-7"
    value["architecture_inventory_policy"]["ar6_remediation"] = (
        "ACCEPTED_FULL_PYTHON_ESTATE_AND_READ_ONLY_OPSCTL_IN_CANONICAL_INVENTORY"
    )
    value["application_architecture"]["program_handoff_status"] = "AR-6_ACCEPTED"
    value["application_architecture"]["program_next_required_slice"] = "AR-7"
    value["python_operational_authority"] = {
        "status": "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION",
        "evidence": AR6_EVIDENCE,
        "python_estate": PYTHON_ESTATE,
        "implementation_issue": 294,
        "implementation_pr": 295,
        "exact_green_head": EXACT_GREEN,
        "implementation_merge": IMPLEMENTATION_MERGE,
        "applicable_permanent_workflows": "13/13",
        "python_summary": {
            "tracked_python_files": 116,
            "KEEP_PYTHON": 108,
            "MIGRATE_TO_RUST": 2,
            "DELETE_AFTER_SEQUENCE": 6,
            "WRAP_WITH_RUST": 0,
        },
        "opsctl": {
            "path": "tools/opsctl",
            "mode": "READ_ONLY_FOUNDATION",
            "commands": ["doctor", "status", "inventory"],
            "third_party_dependencies": False,
            "provider_mutation": False,
        },
        "future_cutovers": {"AR-10": "DELETE_AFTER_SEQUENCE", "AR-11": "MIGRATE_TO_RUST"},
        "application_architecture_accepted_through": "AR-4C",
        "runtime_authority_cleanup_accepted_through": "AR-5",
        "production_mutation": False,
        "next_required_slice": "AR-7",
    }
    write_json("architecture/architecture-rebaseline-v3-transition.json", value)


def patch_human_docs() -> None:
    replace_many(
        "README.md",
        [
            ("AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C and AR-5.", "AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C, AR-5 and AR-6."),
            ("**Current accepted checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup.", "**Current accepted checkpoint:** AR-6 — Full Python Estate + read-only Rust opsctl."),
            ("**Next slice:** AR-6 — Full Python Estate + read-only Rust opsctl.", "**Next slice:** AR-7 — Environments + GitHub Governance + Operational Boundaries."),
            ("#266. AR-5 Wrangler / Runtime Authority Cleanup accepts the AR-2 generation-verification deletion in canonical runtime/deployment authority while the AR-4C-remediated accepted AR-3 application/runtime ownership contract remains in", "#266. AR-6 accepts the full Python estate and read-only Rust `opsctl` foundation while AR-5 remains the accepted runtime-authority cleanup and the AR-4C-remediated application/runtime ownership contract remains in"),
            ("- [`docs/ARCHITECTURE_REBASELINE_V3_AR5.md`](docs/ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;", "- [`docs/ARCHITECTURE_REBASELINE_V3_AR6.md`](docs/ARCHITECTURE_REBASELINE_V3_AR6.md) — accepted AR-6 Python-estate/read-only-opsctl evidence;\n- [`architecture/python-estate-ar6.json`](architecture/python-estate-ar6.json) — accepted full tracked Python disposition;\n- [`docs/ARCHITECTURE_REBASELINE_V3_AR5.md`](docs/ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;"),
        ],
    )
    replace_many(
        "docs/README.md",
        [
            ("AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C and AR-5.", "AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C, AR-5 and AR-6."),
            ("**Current accepted checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup.", "**Current accepted checkpoint:** AR-6 — Full Python Estate + read-only Rust opsctl."),
            ("**Next slice:** AR-6 — Full Python Estate + read-only Rust opsctl.", "**Next slice:** AR-7 — Environments + GitHub Governance + Operational Boundaries."),
            ("- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;", "- [`ARCHITECTURE_REBASELINE_V3_AR6.md`](ARCHITECTURE_REBASELINE_V3_AR6.md) — accepted AR-6 Python-estate/read-only-opsctl evidence;\n- [`../architecture/python-estate-ar6.json`](../architecture/python-estate-ar6.json) — accepted full tracked Python disposition;\n- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md) — accepted AR-5 Wrangler/runtime-authority cleanup evidence;"),
            ("preserving its repository-side D3 foundation; AR-5 is accepted, AR-4C remains the latest application-architecture remediation, AR-4D remains NOT_REQUIRED, and AR-6 is the only next architecture slice.", "preserving its repository-side D3 foundation; AR-6 is accepted, AR-5 remains the runtime-authority cleanup, AR-4C remains the latest application-architecture remediation, AR-4D remains NOT_REQUIRED, and AR-7 is the only next architecture slice."),
        ],
    )
    replace_many(
        "docs/INDEX.md",
        [
            ("AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C and AR-5 are accepted checkpoints.", "AR-0, AR-1, AR-2, AR-3, AR-4A, AR-4B, AR-4C, AR-5 and AR-6 are accepted checkpoints."),
            ("AR-5 — Wrangler / Runtime Authority Cleanup is the current accepted checkpoint.", "AR-6 — Full Python Estate + read-only Rust opsctl is the current accepted checkpoint."),
            ("AR-6 — Full Python Estate + read-only Rust opsctl is the only next slice; AR-4D remains NOT_REQUIRED unless later accepted evidence reopens it.", "AR-7 — Environments + GitHub Governance + Operational Boundaries is the only next slice; AR-4D remains NOT_REQUIRED unless later accepted evidence reopens it."),
            ("accepted AR-2 topology/D3 decision input; AR-5 has now accepted its generation-verification runtime/deployment cleanup while the application/runtime ownership projection remains accepted through AR-4C.", "accepted AR-2 topology/D3 decision input; AR-5 accepted its generation-verification runtime/deployment cleanup, AR-6 accepted the Python/opsctl operational-tooling dimension, and the application/runtime ownership projection remains accepted through AR-4C."),
            ("- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md)", "- [`ARCHITECTURE_REBASELINE_V3_AR6.md`](ARCHITECTURE_REBASELINE_V3_AR6.md)\n- [`../architecture/python-estate-ar6.json`](../architecture/python-estate-ar6.json)\n- [`ARCHITECTURE_REBASELINE_V3_AR5.md`](ARCHITECTURE_REBASELINE_V3_AR5.md)"),
        ],
    )
    replace_many(
        "docs/DEVELOPMENT_PLAN.md",
        [
            ("**Current accepted architecture checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup", "**Current accepted architecture checkpoint:** AR-6 — Full Python Estate + read-only Rust opsctl"),
            ("**Next architecture slice:** AR-6 — Full Python Estate + read-only Rust opsctl", "**Next architecture slice:** AR-7 — Environments + GitHub Governance + Operational Boundaries"),
            ("- AR-5 — Wrangler / Runtime Authority Cleanup: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-6 — Full Python Estate + read-only Rust opsctl: **NEXT**.\n- AR-7…AR-17: ordered future architecture slices.", "- AR-5 — Wrangler / Runtime Authority Cleanup: **DONE / ACCEPTED**.\n- AR-6 — Full Python Estate + read-only Rust opsctl: **CURRENT ACCEPTED CHECKPOINT**.\n- AR-7 — Environments + GitHub Governance + Operational Boundaries: **NEXT**.\n- AR-8…AR-17: ordered future architecture slices."),
            ("- Draft PR #269: feasibility evidence only; canonical read-only `opsctl` integration remains AR-6.", "- AR-6 accepted `architecture/python-estate-ar6.json` and the capability-bounded read-only `tools/opsctl` foundation; Draft PR #269 remains feasibility history only."),
            ("AR-5   Wrangler / Runtime Authority Cleanup                     CURRENT / ACCEPTED CHECKPOINT\nAR-6   Full Python Estate + read-only Rust opsctl                NEXT\nAR-7   Environments + GitHub Governance + Operational Boundaries", "AR-5   Wrangler / Runtime Authority Cleanup                      DONE / ACCEPTED\nAR-6   Full Python Estate + read-only Rust opsctl                CURRENT / ACCEPTED CHECKPOINT\nAR-7   Environments + GitHub Governance + Operational Boundaries NEXT"),
        ],
    )
    replace_many(
        "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md",
        [
            ("**Current accepted architecture checkpoint:** AR-5 — Wrangler / Runtime Authority Cleanup", "**Current accepted architecture checkpoint:** AR-6 — Full Python Estate + read-only Rust opsctl"),
            ("**Next slice:** AR-6 — Full Python Estate + read-only Rust opsctl", "**Next slice:** AR-7 — Environments + GitHub Governance + Operational Boundaries"),
            ("This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-5 is the latest accepted checkpoint. Its Wrangler / Runtime Authority Cleanup applies the accepted AR-2 `GENERATION_VERIFICATION = DELETE` decision to canonical runtime/deployment authority, with acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR5.md`. The latest application-architecture remediation remains AR-4C in `architecture/inventory.json`, with AR-4C/AR-4B/AR-4A evidence preserved and the AR-3 base contract unchanged; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`. AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it.", "This file is the single current architecture/program execution authority after the AR-1 authority cutover. AR-6 is the latest accepted checkpoint. Its accepted full Python estate is `architecture/python-estate-ar6.json` and its read-only Rust operator-tool foundation is `tools/opsctl`, with acceptance evidence in `docs/ARCHITECTURE_REBASELINE_V3_AR6.md`. AR-5 remains the accepted Wrangler/runtime-authority cleanup. The latest application-architecture remediation remains AR-4C in `architecture/inventory.json`, with AR-4C/AR-4B/AR-4A evidence preserved and the AR-3 base contract unchanged; the accepted AR-2 runtime-topology decision remains `architecture/runtime-topology-ar2.json`. AR-4D remains `NOT_REQUIRED` unless later accepted evidence reopens it."),
            ("AR-5   Wrangler / Runtime Authority Cleanup                     CURRENT / ACCEPTED CHECKPOINT\nAR-6   Full Python Estate + read-only Rust opsctl                NEXT\nAR-7   Environments + GitHub Governance + Operational Boundaries", "AR-5   Wrangler / Runtime Authority Cleanup                      DONE / ACCEPTED\nAR-6   Full Python Estate + read-only Rust opsctl                CURRENT / ACCEPTED CHECKPOINT\nAR-7   Environments + GitHub Governance + Operational Boundaries NEXT"),
        ],
    )


def patch_checker() -> None:
    path = "scripts/check-documentation-authority.py"
    replace_many(
        path,
        [
            ('CURRENT_SLICE = "AR-5"\nNEXT_SLICE = "AR-6"', 'CURRENT_SLICE = "AR-6"\nNEXT_SLICE = "AR-7"'),
            ('ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5"]', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5", "AR-6"]'),
            ('AR5_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR5.md")', 'AR5_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR5.md")\nAR6_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR6.md")\nPYTHON_ESTATE = Path("architecture/python-estate-ar6.json")'),
            ('    AR5_EVIDENCE,\n    Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"),', '    AR5_EVIDENCE,\n    AR6_EVIDENCE,\n    PYTHON_ESTATE,\n    Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"),'),
            ('        ar5_evidence = read(root, AR5_EVIDENCE)', '        ar5_evidence = read(root, AR5_EVIDENCE)\n        ar6_evidence = read(root, AR6_EVIDENCE)\n        python_estate = load_json(root, PYTHON_ESTATE)'),
            ('docs/status.json must be the current AR-5 schema/date projection', 'docs/status.json must be the current AR-6 schema/date projection'),
            ('production_ready must remain false throughout accepted AR-5', 'production_ready must remain false throughout accepted AR-6'),
            ('AR-5 architecture_complete/Production Core gate state must remain fail closed', 'AR-6 architecture_complete/Production Core gate state must remain fail closed'),
            ('if transition.get("schema_version") != 8 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR5_MERGE":\n        errors.append("architecture transition must encode accepted AR-5 state")', 'if transition.get("schema_version") != 9 or transition.get("status") != "ACTIVE_AFTER_ACCEPTED_AR6_MERGE":\n        errors.append("architecture transition must encode accepted AR-6 state")'),
            ('errors.append("architecture transition must encode AR-5 -> AR-6 sequencing")', 'errors.append("architecture transition must encode AR-6 -> AR-7 sequencing")'),
            ('errors.append("transition state must remain fail closed through AR-5")', 'errors.append("transition state must remain fail closed through AR-6")'),
            ('errors.append("architecture inventory AR-5 program state is stale")', 'errors.append("architecture inventory AR-6 program state is stale")'),
            ('common = ("Architecture Re-baseline v3", "issue #266", "AR-5", "AR-6", "production_ready=false")', 'common = ("Architecture Re-baseline v3", "issue #266", "AR-6", "AR-7", "production_ready=false")'),
            ('require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-5", "AR-6"),', 'require(index, ("CURRENT_AUTHORITY", "ARCHITECTURE_REBASELINE_V3_PLAN.md", "issue #266", "AR-6", "AR-7"),'),
            ('require(development, ("Document status:** GENERATED_PROJECTION", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "production_ready=false", "Immutable Accepted Phase Provenance"),', 'require(development, ("Document status:** GENERATED_PROJECTION", "AR-4C  Outbound Mail composition extraction", "AR-5   Wrangler / Runtime Authority Cleanup", "AR-6   Full Python Estate + read-only Rust opsctl", "AR-7   Environments + GitHub Governance + Operational Boundaries", "production_ready=false", "Immutable Accepted Phase Provenance"),'),
            ('require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-5", "Next slice:** AR-6",', 'require(plan, ("Document status:** CURRENT_AUTHORITY", "Tracking issue:** #266", "Current accepted architecture checkpoint:** AR-6", "Next slice:** AR-7",'),
            ('        ("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-5"\', \'"current_slice": "AR-4C"\', "current_slice"),', '        ("slice rollback", Path("docs/status.json"), \'"current_slice": "AR-6"\', \'"current_slice": "AR-5"\', "current_slice"),'),
            ('Architecture Re-baseline v3 AR-5 documentation authority negative fixtures passed.', 'Architecture Re-baseline v3 AR-6 documentation authority negative fixtures passed.'),
            ('Architecture Re-baseline v3 AR-5 documentation/program authority is consistent.', 'Architecture Re-baseline v3 AR-6 documentation/program authority is consistent.'),
        ],
    )
    value = read(path)
    marker = '        errors.append("docs/status.json AR-5 acceptance provenance drifted")\n'
    addition = marker + '''    ar6 = program.get("ar6_acceptance") if isinstance(program.get("ar6_acceptance"), dict) else {}\n    expected_python_summary = {\n        "tracked_python_files": 116,\n        "DELETE_AFTER_SEQUENCE": 6,\n        "KEEP_PYTHON": 108,\n        "MIGRATE_TO_RUST": 2,\n        "WRAP_WITH_RUST": 0,\n    }\n    if (\n        program.get("python_estate") != PYTHON_ESTATE.as_posix()\n        or program.get("python_operational_evidence") != AR6_EVIDENCE.as_posix()\n        or ar6.get("issue") != 294\n        or ar6.get("implementation_pr") != 295\n        or ar6.get("exact_green_head") != "9b06d542873ffa3122e53e107105098e21f5933c"\n        or ar6.get("implementation_merge") != "d0229fedd81ee870822b6d9394bc4ee313ea3a3c"\n        or ar6.get("applicable_permanent_workflows") != "13/13"\n        or ar6.get("closeout_issue") != 296\n    ):\n        errors.append("docs/status.json AR-6 acceptance provenance drifted")\n    if (\n        python_estate.get("status") != "AR6_ACCEPTED_PYTHON_ESTATE"\n        or python_estate.get("accepted_program_checkpoint") != "AR-6"\n        or python_estate.get("summary") != expected_python_summary\n    ):\n        errors.append("accepted AR-6 Python estate authority drifted")\n'''
    if value.count(marker) != 1:
        raise SystemExit("cannot insert AR-6 status/estate checker")
    write(path, value.replace(marker, addition, 1))

    value = read(path)
    marker = '        errors.append("transition AR-5 runtime-authority cleanup acceptance drifted")\n'
    addition = marker + '''    python_ops = transition.get("python_operational_authority") if isinstance(transition.get("python_operational_authority"), dict) else {}\n    python_opsctl = python_ops.get("opsctl") if isinstance(python_ops.get("opsctl"), dict) else {}\n    if (\n        python_ops.get("status") != "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION"\n        or python_ops.get("evidence") != AR6_EVIDENCE.as_posix()\n        or python_ops.get("python_estate") != PYTHON_ESTATE.as_posix()\n        or python_ops.get("implementation_pr") != 295\n        or python_ops.get("exact_green_head") != "9b06d542873ffa3122e53e107105098e21f5933c"\n        or python_ops.get("implementation_merge") != "d0229fedd81ee870822b6d9394bc4ee313ea3a3c"\n        or python_ops.get("next_required_slice") != "AR-7"\n        or python_ops.get("production_mutation") is not False\n        or python_opsctl.get("mode") != "READ_ONLY_FOUNDATION"\n        or python_opsctl.get("commands") != ["doctor", "status", "inventory"]\n        or python_opsctl.get("provider_mutation") is not False\n    ):\n        errors.append("transition AR-6 Python/opsctl acceptance drifted")\n'''
    if value.count(marker) != 1:
        raise SystemExit("cannot insert AR-6 transition checker")
    write(path, value.replace(marker, addition, 1))

    value = read(path)
    marker = '        errors.append("architecture inventory AR-5 runtime-authority cleanup projection drifted")\n'
    addition = marker + '''    inventory_python_ops = inventory.get("python_operational_authority") if isinstance(inventory.get("python_operational_authority"), dict) else {}\n    if (\n        inventory_python_ops.get("status") != "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION"\n        or inventory_python_ops.get("evidence") != AR6_EVIDENCE.as_posix()\n        or inventory_python_ops.get("python_estate") != PYTHON_ESTATE.as_posix()\n        or inventory_python_ops.get("implementation_pr") != 295\n        or inventory_python_ops.get("exact_green_head") != "9b06d542873ffa3122e53e107105098e21f5933c"\n        or inventory_python_ops.get("implementation_merge") != "d0229fedd81ee870822b6d9394bc4ee313ea3a3c"\n        or inventory_python_ops.get("next_required_slice") != "AR-7"\n        or inventory_python_ops.get("production_mutation") is not False\n    ):\n        errors.append("architecture inventory AR-6 Python/opsctl projection drifted")\n'''
    if value.count(marker) != 1:
        raise SystemExit("cannot insert AR-6 inventory checker")
    write(path, value.replace(marker, addition, 1))

    value = read(path)
    marker = '    require(ar5_evidence, ("AR-5 Wrangler / Runtime Authority Cleanup", "EVIDENCE / AR-5 accepted", "afed435bb714794d6c4f252be6b44c592ee31b2b", "82d251a1d6666199c6eace393eedc1766157fcee", "13/13 success", "AR-6", "Production Core remains `BLOCKED`"), "AR-5 evidence", errors)\n'
    addition = marker + '    require(ar6_evidence, ("AR-6 Full Python Estate + read-only Rust opsctl", "EVIDENCE / AR-6 accepted", "9b06d542873ffa3122e53e107105098e21f5933c", "d0229fedd81ee870822b6d9394bc4ee313ea3a3c", "13/13 success", "108", "AR-7", "production_core_gate = BLOCKED"), "AR-6 evidence", errors)\n'
    if value.count(marker) != 1:
        raise SystemExit("cannot insert AR-6 evidence checker")
    write(path, value.replace(marker, addition, 1))


def patch_inventory_generator() -> None:
    path = "scripts/generate-architecture-inventory.py"
    replace_many(
        path,
        [
            ('AR5_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR5.md"', 'AR5_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR5.md"\nAR6_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR6.md"\nPYTHON_ESTATE = "architecture/python-estate-ar6.json"'),
            ('ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5"]\nCURRENT_SLICE = "AR-5"\nNEXT_SLICE = "AR-6"', 'ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5", "AR-6"]\nCURRENT_SLICE = "AR-6"\nNEXT_SLICE = "AR-7"'),
            ('    {"path": AR5_EVIDENCE, "status": "EVIDENCE", "scope": "ar5_runtime_authority_cleanup_accepted"},', '    {"path": AR5_EVIDENCE, "status": "EVIDENCE", "scope": "ar5_runtime_authority_cleanup_accepted"},\n    {"path": AR6_EVIDENCE, "status": "EVIDENCE", "scope": "ar6_python_estate_and_read_only_opsctl_accepted"},\n    {"path": PYTHON_ESTATE, "status": "STABLE_AUTHORITY", "scope": "accepted_ar6_full_python_disposition"},'),
            ('docs/status.json must remain production_ready=false during accepted AR-5', 'docs/status.json must remain production_ready=false during accepted AR-6'),
            ('docs/status.json must keep accepted AR-5 architecture/gate state fail closed', 'docs/status.json must keep accepted AR-6 architecture/gate state fail closed'),
            ('docs/status.json must project accepted AR-5 -> next AR-6 sequencing', 'docs/status.json must project accepted AR-6 -> next AR-7 sequencing'),
            ('        "documentation_authority": {', '        "python_operational_authority": {\n            "schema_version": 1,\n            "status": "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION",\n            "evidence": AR6_EVIDENCE,\n            "python_estate": PYTHON_ESTATE,\n            "implementation_issue": 294,\n            "implementation_pr": 295,\n            "exact_green_head": "9b06d542873ffa3122e53e107105098e21f5933c",\n            "implementation_merge": "d0229fedd81ee870822b6d9394bc4ee313ea3a3c",\n            "applicable_permanent_workflows": "13/13",\n            "python_summary": {\n                "tracked_python_files": 116,\n                "KEEP_PYTHON": 108,\n                "MIGRATE_TO_RUST": 2,\n                "DELETE_AFTER_SEQUENCE": 6,\n                "WRAP_WITH_RUST": 0,\n            },\n            "opsctl": {\n                "path": "tools/opsctl",\n                "mode": "READ_ONLY_FOUNDATION",\n                "commands": ["doctor", "status", "inventory"],\n                "third_party_dependencies": False,\n                "provider_mutation": False,\n            },\n            "future_cutovers": {"AR-10": "DELETE_AFTER_SEQUENCE", "AR-11": "MIGRATE_TO_RUST"},\n            "application_architecture_accepted_through": "AR-4C",\n            "runtime_authority_cleanup_accepted_through": "AR-5",\n            "production_mutation": False,\n            "next_required_slice": "AR-7",\n        },\n        "documentation_authority": {'),
            ('            "runtime_authority_cleanup_evidence": AR5_EVIDENCE,', '            "runtime_authority_cleanup_evidence": AR5_EVIDENCE,\n            "python_operational_evidence": AR6_EVIDENCE,\n            "python_estate": PYTHON_ESTATE,'),
            ('    print("Architecture inventory accepted AR-5 runtime-authority negative self-test passed.")', '    python_ops = copy.deepcopy(expected)\n    python_ops["python_operational_authority"]["status"] = "AR6_CANDIDATE"\n    if serialized(python_ops) == serialized(expected):\n        raise SystemExit("inventory self-test failed to detect AR-6 Python/opsctl acceptance rollback")\n    print("Architecture inventory accepted AR-6 Python/opsctl negative self-test passed.")'),
            ('        print("Architecture inventory and accepted AR-5 runtime-authority projection are current.")', '        print("Architecture inventory and accepted AR-6 Python/opsctl projection are current.")'),
        ],
    )
    value = read(path)
    marker = '    if program.get("runtime_authority_cleanup_evidence") != AR5_EVIDENCE:\n        raise SystemExit("docs/status.json must project accepted AR-5 runtime-authority cleanup evidence")\n'
    addition = marker + '    if program.get("python_operational_evidence") != AR6_EVIDENCE or program.get("python_estate") != PYTHON_ESTATE:\n        raise SystemExit("docs/status.json must project accepted AR-6 Python/opsctl authority")\n'
    if value.count(marker) != 1:
        raise SystemExit("cannot bind AR-6 docs projection in inventory generator")
    write(path, value.replace(marker, addition, 1))


def main() -> None:
    write_evidence()
    patch_python_estate_generator()
    patch_status()
    patch_transition()
    patch_human_docs()
    patch_checker()
    patch_inventory_generator()
    print("AR-6 closeout source transaction prepared.")


if __name__ == "__main__":
    main()
