#!/usr/bin/env python3
"""Fail closed when current repository documentation disagrees on phase/readiness authority."""

from __future__ import annotations

import argparse
import json
import shutil
import tempfile
from pathlib import Path

ACCEPTED_PHASE = "Phase 2I"
ARCHITECTURE_REMEDIATION_STATUS = "closed"
PRODUCT_READINESS_STATUS = "active_blocking"
PHASE2J_STATUS = "blocked_pending_repository_remediation"
TRACKING_ISSUE = 203
STATUS_DATE = "2026-08-12"
PRODUCT_READINESS_PLAN = "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"
ARCHITECTURE_CLOSEOUT_PLAN = "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"
DEVICE_AUTHORITY = [
    "device-domain",
    "application/use-case layer",
    "D1/persistence composition",
]
CLOSED_FINDINGS = [f"R{index}" for index in range(1, 10)]
INITIAL_SEVERITY = {"repository_owned_p0": 0, "repository_owned_p1": 5, "repository_owned_p2": 1}
HISTORICAL_DEVICE_LABELS = (
    "synthetic_device_authorization",
    "synthetic_device_grant_journal",
    "synthetic_two_device_contract",
)

REQUIRED_FILES = (
    Path("README.md"),
    Path("architecture/accepted-phases.json"),
    Path("docs/README.md"),
    Path("docs/INDEX.md"),
    Path("docs/status.json"),
    Path("docs/DEVELOPMENT_PLAN.md"),
    Path("docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"),
    Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"),
    Path("docs/THREAT_MODEL.md"),
    Path("docs/PHASE2I_THREAT_MODEL.md"),
)


def read(root: Path, relative: Path) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"missing documentation-authority file: {relative}")
    return path.read_text(encoding="utf-8")


def load_json(root: Path, relative: Path) -> dict:
    try:
        value = json.loads(read(root, relative))
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid JSON in {relative}: {exc}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{relative} must contain a JSON object")
    return value


def require(text: str, markers: tuple[str, ...], label: str, errors: list[str]) -> None:
    for marker in markers:
        if marker not in text:
            errors.append(f"{label} missing current-authority marker: {marker}")


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    try:
        root_readme = read(root, Path("README.md"))
        docs_readme = read(root, Path("docs/README.md"))
        index = read(root, Path("docs/INDEX.md"))
        development = read(root, Path("docs/DEVELOPMENT_PLAN.md"))
        architecture_closeout = read(root, Path("docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"))
        product_readiness = read(root, Path("docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"))
        threat = read(root, Path("docs/THREAT_MODEL.md"))
        phase2i_threat = read(root, Path("docs/PHASE2I_THREAT_MODEL.md"))
        status = load_json(root, Path("docs/status.json"))
        ledger = load_json(root, Path("architecture/accepted-phases.json"))
    except ValueError as exc:
        return [str(exc)]

    phases = ledger.get("accepted_phases")
    if not isinstance(phases, list) or not phases:
        errors.append("accepted phase ledger must contain a non-empty phases array")
        accepted_phase = None
    else:
        final = phases[-1]
        accepted_phase = final.get("phase") if isinstance(final, dict) else None
        if accepted_phase != ACCEPTED_PHASE:
            errors.append(
                f"accepted phase ledger must end at {ACCEPTED_PHASE}; observed {accepted_phase!r}"
            )

    current = status.get("current")
    if not isinstance(current, dict):
        errors.append("docs/status.json missing current projection")
        current = {}
    if status.get("schema_version") != 2:
        errors.append("docs/status.json schema_version must remain 2 for the current projection")
    if status.get("as_of") != STATUS_DATE:
        errors.append(f"docs/status.json as_of must be {STATUS_DATE}")
    if status.get("production_ready") is not False:
        errors.append("docs/status.json production_ready must remain false before Phase 2J acceptance")
    if current.get("accepted_product_phase") != accepted_phase or accepted_phase != ACCEPTED_PHASE:
        errors.append("docs/status.json current accepted phase must match accepted-phases.json Phase 2I")
    if current.get("accepted_phase_ledger") != "architecture/accepted-phases.json":
        errors.append("docs/status.json must name architecture/accepted-phases.json as accepted-phase authority")

    historical = current.get("pre2j_remediation")
    if not isinstance(historical, dict):
        errors.append("docs/status.json lost historical R1-R9 pre-2J remediation projection")
    else:
        if historical.get("status") != ARCHITECTURE_REMEDIATION_STATUS:
            errors.append("historical R1-R9 architecture remediation must remain closed")
        if historical.get("plan") != ARCHITECTURE_CLOSEOUT_PLAN:
            errors.append("historical R1-R9 architecture remediation must retain its accepted closeout plan")
        if historical.get("repository_owned_p0") != 0 or historical.get("repository_owned_p1") != 0:
            errors.append("historical R1-R9 closeout must retain repository-owned P0=0 and P1=0")
        if historical.get("closed_findings") != CLOSED_FINDINGS:
            errors.append("historical R1-R9 closeout must retain all R1-R9 as closed")

    followup = current.get("pre2j_product_readiness_remediation")
    if not isinstance(followup, dict):
        errors.append("docs/status.json missing active #203 product-readiness remediation blocker")
    else:
        if followup.get("status") != PRODUCT_READINESS_STATUS:
            errors.append("active #203 product-readiness remediation must remain active_blocking")
        if followup.get("plan") != PRODUCT_READINESS_PLAN:
            errors.append("active #203 product-readiness remediation must point to the canonical product-readiness plan")
        if followup.get("tracking_issue") != TRACKING_ISSUE:
            errors.append("active product-readiness remediation must remain tracked by issue #203")
        for key, expected in INITIAL_SEVERITY.items():
            if followup.get(key) != expected:
                errors.append(f"active #203 initial severity projection drifted: {key} must be {expected}")

    phase2j = current.get("phase_2j")
    if not isinstance(phase2j, dict) or phase2j.get("status") != PHASE2J_STATUS:
        errors.append("Phase 2J must remain blocked_pending_repository_remediation before accepted Batch F")
    else:
        if phase2j.get("blocked_by_issue") != TRACKING_ISSUE:
            errors.append("Phase 2J must remain blocked by issue #203")
        if phase2j.get("production_ready_may_change_only_after_acceptance") is not True:
            errors.append("docs/status.json must keep production readiness gated on Phase 2J acceptance")

    device_ownership = current.get("device_authorization_ownership")
    if not isinstance(device_ownership, dict):
        errors.append("docs/status.json missing current device authorization ownership")
    else:
        if device_ownership.get("production_authority") != DEVICE_AUTHORITY:
            errors.append("docs/status.json current production device authorization owners drifted")
        if device_ownership.get("certification_domain_authority") != "forbidden":
            errors.append("docs/status.json must forbid certification-domain device authorization authority")

    repository_step = status.get("repository_step")
    if not isinstance(repository_step, dict) or repository_step.get("historical") is not True:
        errors.append("docs/status.json Repository Step projection must remain explicitly historical")

    next_step = status.get("next_repository_step")
    if not isinstance(next_step, dict):
        errors.append("docs/status.json missing compatibility next_repository_step")
    else:
        if next_step.get("name") != "Pre-2J product-readiness remediation":
            errors.append("next_repository_step must name the #203 pre-2J product-readiness remediation")
        if next_step.get("status") != PRODUCT_READINESS_STATUS:
            errors.append("next_repository_step must remain active_blocking")
        if next_step.get("tracking_issue") != TRACKING_ISSUE:
            errors.append("next_repository_step must remain tracked by issue #203")
        if next_step.get("authority") != PRODUCT_READINESS_PLAN:
            errors.append("next_repository_step must point to the product-readiness remediation authority")

    implementation = status.get("implementation")
    if not isinstance(implementation, dict):
        errors.append("docs/status.json implementation projection is missing")
    else:
        if implementation.get("pre2j_architecture_remediation") != "closed_r1_r9":
            errors.append("implementation projection must preserve closed_r1_r9 history")
        if implementation.get("pre2j_product_readiness_remediation") != "active_issue_203":
            errors.append("implementation projection must record active issue #203 product-readiness remediation")

    evidence = status.get("evidence")
    historical_note = status.get("historical_device_authority_note")
    if not isinstance(evidence, dict):
        errors.append("docs/status.json historical evidence projection is missing")
    else:
        for label in HISTORICAL_DEVICE_LABELS:
            if label not in evidence:
                errors.append(f"docs/status.json lost historical Step 10 evidence label: {label}")
    if not isinstance(historical_note, str) or not all(label in historical_note for label in HISTORICAL_DEVICE_LABELS):
        errors.append("docs/status.json must explicitly classify historical device-authority labels as history")
    elif "not current production authority" not in historical_note:
        errors.append("docs/status.json historical device-authority note must reject current-authority interpretation")

    common_markers = (
        "Accepted repository-local product phase: Phase 2I",
        "R1–R9 pre-2J architecture remediation: CLOSED / ACCEPTED HISTORY",
        "Pre-2J product-readiness remediation: ACTIVE / BLOCKING Phase 2J",
        "`production_ready=false`",
    )
    require(root_readme, common_markers, "README.md", errors)
    require(docs_readme, common_markers, "docs/README.md", errors)
    require(
        root_readme,
        (
            "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
            "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "issue #203",
            "P0=0, P1=5, P2=1",
            "Phase 2J is blocked and has not started",
            "Repository Steps 0–10 are historical",
            "docs/THREAT_MODEL.md",
            "docs/status.json",
            "`certification-domain` is not a device-authorization authority",
        ),
        "README.md",
        errors,
    )
    require(
        docs_readme,
        (
            "INDEX.md",
            "PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
            "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "issue #203",
            "P0=0, P1=5 and P2=1",
            "Phase 2J is blocked and not started",
            "Repository Steps 0–10 are historical",
            "THREAT_MODEL.md",
        ),
        "docs/README.md",
        errors,
    )
    require(
        index,
        (
            "PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
            "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "issue #203",
            "P0=0, P1=5, P2=1",
            "Phase 2J is **blocked / pending repository remediation**",
            "status.json",
            "THREAT_MODEL.md",
            "PHASE2I_THREAT_MODEL.md",
        ),
        "docs/INDEX.md",
        errors,
    )

    require(
        architecture_closeout,
        (
            "**Status:** CLOSED / PHASE 2J UNBLOCKED",
            "**Production readiness:** remains `false`",
            "repository-owned P0 = 0",
            "repository-owned P1 = 0",
            "R1–R9 remain accepted and regression-protected",
        ),
        "historical pre-2J architecture remediation plan",
        errors,
    )

    require(
        product_readiness,
        (
            "**Status:** CANONICAL / ACTIVE BLOCKER FOR PHASE 2J",
            "**Tracking:** issue #203",
            "**Accepted product phase:** Phase 2I",
            "**Production readiness:** remains `false`",
            "R1-R9 and the closed pre-2J architecture remediation record remain accepted history",
            "Repository-owned severity at plan creation: **P0 = 0, P1 = 5, P2 = 1**",
            "## Batch 0 — Restore truthful current repository authority",
            "## Batch F",
        ),
        "pre-2J product-readiness remediation plan",
        errors,
    )

    require(
        development,
        (
            "Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / PENDING REPOSITORY REMEDIATION",
            "PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
            "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "issue #203",
            "Phase 2I remains the last accepted repository-local product phase",
            "`production_ready=false`",
            "## 13. Immediate Next Action",
        ),
        "DEVELOPMENT_PLAN.md",
        errors,
    )
    for stale in (
        "Phase 2J — Production-readiness evidence and controlled rollout — UNBLOCKED / NOT STARTED",
        "Phase 2J real production evidence + controlled rollout                                                UNBLOCKED / NOT STARTED",
        "Phase 2J is unblocked but not started",
        "Phase 2J is the unique next product phase",
        "Phase 2J is the next product phase but is unblocked/not started",
    ):
        if stale in development:
            errors.append(f"DEVELOPMENT_PLAN.md retains stale Phase 2J activation claim: {stale}")

    require(
        threat,
        (
            "Canonical current repository-local threat model",
            "Phase 2I accepted repository-local controls",
            "Phase 2J External residual risks",
            "production_ready=false",
            "Production device authorization is owned by `device-domain`, application/use-case orchestration and D1/persistence composition",
            "`certification-domain` is explicitly not a production device-authorization authority",
        ),
        "docs/THREAT_MODEL.md",
        errors,
    )
    if "Phase 0 baseline" in threat:
        errors.append("canonical docs/THREAT_MODEL.md must not remain labelled as a Phase 0 baseline")
    require(
        phase2i_threat,
        (
            "Historical accepted Phase 2I evidence",
            "Canonical current threat model: [THREAT_MODEL.md](THREAT_MODEL.md)",
        ),
        "docs/PHASE2I_THREAT_MODEL.md",
        errors,
    )

    return errors


def copy_fixture(source_root: Path, target_root: Path) -> None:
    for relative in REQUIRED_FILES:
        source = source_root / relative
        target = target_root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def self_test(root: Path) -> bool:
    baseline = validate(root)
    if baseline:
        print("documentation-authority self-test requires a valid baseline")
        for error in baseline:
            print(error)
        return False

    fixtures: list[tuple[str, Path, str, str, str]] = [
        (
            "stale current phase",
            Path("docs/status.json"),
            '"accepted_product_phase": "Phase 2I"',
            '"accepted_product_phase": "Phase 2H"',
            "current accepted phase",
        ),
        (
            "false Phase 2J unblocking",
            Path("docs/status.json"),
            '"status": "blocked_pending_repository_remediation"',
            '"status": "unblocked_not_started"',
            "Phase 2J",
        ),
        (
            "false Phase 2J acceptance",
            Path("docs/status.json"),
            '"status": "blocked_pending_repository_remediation"',
            '"status": "accepted"',
            "Phase 2J",
        ),
        (
            "active blocker disappearance",
            Path("docs/status.json"),
            '"status": "active_blocking"',
            '"status": "closed"',
            "active #203",
        ),
        (
            "active blocker issue drift",
            Path("docs/status.json"),
            '"tracking_issue": 203',
            '"tracking_issue": 204',
            "issue #203",
        ),
        (
            "historical R1-R9 remediation reactivated",
            Path("docs/status.json"),
            '"status": "closed"',
            '"status": "active"',
            "historical R1-R9",
        ),
        (
            "historical R1-R9 finding lost",
            Path("docs/status.json"),
            '"R8", "R9"',
            '"R8"',
            "all R1-R9",
        ),
        (
            "stale security authority",
            Path("docs/THREAT_MODEL.md"),
            "Canonical current repository-local threat model",
            "Phase 0 baseline",
            "THREAT_MODEL",
        ),
        (
            "premature production readiness",
            Path("docs/status.json"),
            '"production_ready": false',
            '"production_ready": true',
            "production_ready",
        ),
        (
            "certification device authority resurrection",
            Path("docs/status.json"),
            '"certification_domain_authority": "forbidden"',
            '"certification_domain_authority": "allowed"',
            "certification-domain device authorization authority",
        ),
        (
            "historical device evidence misclassified",
            Path("docs/status.json"),
            "not current production authority",
            "current production authority",
            "historical device-authority note",
        ),
    ]

    for label, relative, old, new, expected_error in fixtures:
        with tempfile.TemporaryDirectory(prefix="documentation-authority-") as directory:
            fixture = Path(directory)
            copy_fixture(root, fixture)
            path = fixture / relative
            text = path.read_text(encoding="utf-8")
            if old not in text:
                print(f"negative fixture source marker missing for {label}: {old}")
                return False
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            errors = validate(fixture)
            if not errors or not any(expected_error.lower() in error.lower() for error in errors):
                print(f"negative documentation fixture unexpectedly passed: {label}")
                for error in errors:
                    print(error)
                return False
            print(f"negative documentation fixture rejected as expected: {label}")
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
    print("current documentation, readiness and security authority: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())