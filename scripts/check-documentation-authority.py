#!/usr/bin/env python3
"""Fail closed when current repository documentation disagrees on phase, readiness or security authority."""

from __future__ import annotations

import argparse
import json
import shutil
import tempfile
from pathlib import Path

ACCEPTED_PHASE = "Phase 2I"
PRE2J_STATUS = "active_blocking_phase_2j"
PHASE2J_STATUS = "blocked_pre2j_closure"

REQUIRED_FILES = (
    Path("README.md"),
    Path("architecture/accepted-phases.json"),
    Path("docs/README.md"),
    Path("docs/INDEX.md"),
    Path("docs/status.json"),
    Path("docs/DEVELOPMENT_PLAN.md"),
    Path("docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"),
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
        remediation = read(root, Path("docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"))
        threat = read(root, Path("docs/THREAT_MODEL.md"))
        phase2i_threat = read(root, Path("docs/PHASE2I_THREAT_MODEL.md"))
        status = load_json(root, Path("docs/status.json"))
        ledger = load_json(root, Path("architecture/accepted-phases.json"))
    except ValueError as exc:
        return [str(exc)]

    phases = ledger.get("phases")
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
        errors.append("docs/status.json schema_version must be 2 for current/pre-2J projection")
    if status.get("as_of") != "2026-08-11":
        errors.append("docs/status.json as_of must match the accepted pre-2J documentation date")
    if status.get("production_ready") is not False:
        errors.append("docs/status.json production_ready must remain false before Phase 2J acceptance")
    if current.get("accepted_product_phase") != accepted_phase or accepted_phase != ACCEPTED_PHASE:
        errors.append("docs/status.json current accepted phase must match accepted-phases.json Phase 2I")
    if current.get("accepted_phase_ledger") != "architecture/accepted-phases.json":
        errors.append("docs/status.json must name architecture/accepted-phases.json as accepted-phase authority")

    pre2j = current.get("pre2j_remediation")
    if not isinstance(pre2j, dict) or pre2j.get("status") != PRE2J_STATUS:
        errors.append("docs/status.json must keep pre-2J remediation active and blocking Phase 2J")
    elif pre2j.get("plan") != "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md":
        errors.append("docs/status.json pre-2J remediation must point to the canonical remediation plan")

    phase2j = current.get("phase_2j")
    if not isinstance(phase2j, dict) or phase2j.get("status") != PHASE2J_STATUS:
        errors.append("docs/status.json must keep Phase 2J blocked by pre-2J closure")

    repository_step = status.get("repository_step")
    if not isinstance(repository_step, dict) or repository_step.get("historical") is not True:
        errors.append("docs/status.json Repository Step projection must be explicitly historical")
    next_step = status.get("next_repository_step")
    if not isinstance(next_step, dict) or next_step.get("status") != PHASE2J_STATUS:
        errors.append("docs/status.json compatibility next_repository_step must be blocked_pre2j_closure")

    common_markers = (
        "Accepted repository-local product phase: Phase 2I",
        "Pre-2J remediation: ACTIVE / BLOCKING PHASE 2J",
        "`production_ready=false`",
    )
    require(root_readme, common_markers, "README.md", errors)
    require(docs_readme, common_markers, "docs/README.md", errors)
    require(
        root_readme,
        (
            "Repository Steps 0–10 are historical",
            "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "docs/THREAT_MODEL.md",
            "docs/status.json",
        ),
        "README.md",
        errors,
    )
    require(
        docs_readme,
        (
            "docs/INDEX.md",
            "Repository Steps 0–10 are historical",
            "THREAT_MODEL.md",
            "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
        ),
        "docs/README.md",
        errors,
    )
    require(
        index,
        (
            "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "status.json",
            "THREAT_MODEL.md",
            "PHASE2I_THREAT_MODEL.md",
            "Phase 2J remains blocked while the pre-2J remediation plan is ACTIVE",
        ),
        "docs/INDEX.md",
        errors,
    )

    require(
        remediation,
        (
            "**Status:** ACTIVE / BLOCKING PHASE 2J",
            "**Production readiness:** remains `false`",
            "Phase 2J may resume only when all of the following are true on accepted `main`:",
        ),
        "pre-2J remediation plan",
        errors,
    )

    require(
        development,
        (
            "Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / NEXT AFTER PRE-2J",
            "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
            "No product phase is implementation-active while the pre-2J remediation plan is ACTIVE",
            "`production_ready=false`",
        ),
        "DEVELOPMENT_PLAN.md",
        errors,
    )
    for stale in (
        "Phase 2J — Production-readiness evidence and controlled rollout — NEXT",
        "Phase 2J is the unique NEXT",
        "Phase 2J real production evidence + controlled rollout                                                NEXT",
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
            "false Phase 2J activation",
            Path("docs/DEVELOPMENT_PLAN.md"),
            "Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / NEXT AFTER PRE-2J",
            "Phase 2J — Production-readiness evidence and controlled rollout — NEXT",
            "Phase 2J",
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
