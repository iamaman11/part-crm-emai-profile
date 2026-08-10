#!/usr/bin/env python3
"""Validate immutable accepted-phase provenance against the normative development plan."""

from __future__ import annotations

import copy
import json
import re
from pathlib import Path
from typing import Any

EXPECTED_PHASE_ORDER = (
    "Phase 1A",
    "Phase 1B",
    "Phase 2A",
    "Phase 2B",
    "Phase 2C",
    "Phase 2D",
    "Phase 2E",
    "Phase 2F",
)
SHA40 = re.compile(r"^[0-9a-f]{40}$")


def load_ledger(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("schema_version") != 1:
        raise ValueError("accepted phase ledger schema_version must be 1")
    phases = payload.get("accepted_phases")
    if not isinstance(phases, list):
        raise ValueError("accepted phase ledger accepted_phases must be a list")
    observed = tuple(item.get("phase") for item in phases if isinstance(item, dict))
    if observed != EXPECTED_PHASE_ORDER:
        raise ValueError(
            f"accepted phase ledger order mismatch: observed={observed}, expected={EXPECTED_PHASE_ORDER}"
        )
    for item in phases:
        if not isinstance(item, dict):
            raise ValueError("accepted phase ledger entries must be objects")
        required = {
            "phase",
            "issue",
            "implementation_pr",
            "source_head",
            "merge_sha",
            "permanent_workflows",
        }
        if set(item) != required:
            raise ValueError(
                f"accepted phase ledger entry keys mismatch for {item.get('phase')}: {sorted(item)}"
            )
        if not isinstance(item["issue"], int) or item["issue"] <= 0:
            raise ValueError(f"invalid issue number for {item['phase']}")
        if not isinstance(item["implementation_pr"], int) or item["implementation_pr"] <= 0:
            raise ValueError(f"invalid implementation PR for {item['phase']}")
        if not isinstance(item["permanent_workflows"], int) or item["permanent_workflows"] <= 0:
            raise ValueError(f"invalid workflow count for {item['phase']}")
        for key in ("source_head", "merge_sha"):
            value = item[key]
            if not isinstance(value, str) or SHA40.fullmatch(value) is None:
                raise ValueError(f"invalid {key} for {item['phase']}")
    return payload


def _phase_record_pattern(phase: str) -> re.Pattern[str]:
    escaped = re.escape(phase)
    return re.compile(
        rf"{escaped} was accepted through issue #(\d+) / (?:implementation )?PR #(\d+)"
        rf".*?exact proven source head\s*\n?`([0-9a-f]{{40}})`"
        rf".*?(?:guarded squash merge|squash merge)\s*\n?`([0-9a-f]{{40}})`",
        re.DOTALL,
    )


def validate_plan_provenance(plan: str, ledger: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    for expected in ledger["accepted_phases"]:
        phase = expected["phase"]
        matches = _phase_record_pattern(phase).findall(plan)
        if len(matches) != 1:
            errors.append(
                f"{phase} must have exactly one accepted provenance record; observed={len(matches)}"
            )
            continue
        issue, implementation_pr, source_head, merge_sha = matches[0]
        observed = {
            "issue": int(issue),
            "implementation_pr": int(implementation_pr),
            "source_head": source_head,
            "merge_sha": merge_sha,
        }
        for key, value in observed.items():
            if value != expected[key]:
                errors.append(
                    f"{phase} provenance mismatch for {key}: observed={value}, expected={expected[key]}"
                )
        workflow_marker = f"{expected['permanent_workflows']}/{expected['permanent_workflows']} permanent workflows"
        phase_start = plan.find(f"{phase} was accepted through issue #{expected['issue']}")
        if phase_start >= 0:
            next_start = len(plan)
            for later in ledger["accepted_phases"]:
                marker = f"{later['phase']} was accepted through issue #"
                candidate = plan.find(marker, phase_start + 1)
                if candidate >= 0:
                    next_start = min(next_start, candidate)
            block = plan[phase_start:next_start]
            if workflow_marker not in block and "12/12 permanent workflows" not in block:
                errors.append(f"{phase} accepted provenance block is missing workflow evidence")
    return errors


def provenance_self_test(plan: str, ledger: dict[str, Any]) -> None:
    if validate_plan_provenance(plan, ledger):
        raise ValueError("accepted phase provenance baseline is invalid before self-test")
    tampered = copy.deepcopy(ledger)
    original = tampered["accepted_phases"][-1]["merge_sha"]
    tampered["accepted_phases"][-1]["merge_sha"] = "0" * 40
    if tampered["accepted_phases"][-1]["merge_sha"] == original:
        raise ValueError("accepted phase provenance self-test could not tamper merge SHA")
    errors = validate_plan_provenance(plan, tampered)
    if not any("Phase 2F provenance mismatch for merge_sha" in error for error in errors):
        raise ValueError("tampered accepted phase merge SHA unexpectedly passed provenance validation")
