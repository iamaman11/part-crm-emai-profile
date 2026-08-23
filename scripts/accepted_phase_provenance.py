#!/usr/bin/env python3
"""Validate the canonical immutable accepted-product-phase ledger.

`architecture/accepted-phases.json` is the machine-readable provenance owner. Human
navigation/projection documents may reference it, but they must not duplicate the full
ledger merely to satisfy a checker.
"""

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
    "Phase 2G",
    "Phase 2H",
    "Phase 2I",
)
SHA40 = re.compile(r"^[0-9a-f]{40}$")
_REQUIRED_ENTRY_KEYS = {
    "phase",
    "issue",
    "implementation_pr",
    "source_head",
    "merge_sha",
    "permanent_workflows",
}


def validate_ledger(payload: dict[str, Any]) -> None:
    if set(payload) != {"schema_version", "accepted_phases"}:
        raise ValueError(f"accepted phase ledger top-level keys mismatch: {sorted(payload)}")
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
        if set(item) != _REQUIRED_ENTRY_KEYS:
            raise ValueError(
                f"accepted phase ledger entry keys mismatch for {item.get('phase')}: {sorted(item)}"
            )
        if not isinstance(item["issue"], int) or item["issue"] <= 0:
            raise ValueError(f"invalid issue number for {item['phase']}")
        if not isinstance(item["implementation_pr"], int) or item["implementation_pr"] <= 0:
            raise ValueError(f"invalid implementation PR for {item['phase']}")
        if item["permanent_workflows"] != 12:
            raise ValueError(f"accepted workflow count must remain 12 for {item['phase']}")
        for key in ("source_head", "merge_sha"):
            value = item[key]
            if not isinstance(value, str) or SHA40.fullmatch(value) is None:
                raise ValueError(f"invalid {key} for {item['phase']}")


def load_ledger(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("accepted phase ledger must be an object")
    validate_ledger(payload)
    return payload


def provenance_self_test(ledger: dict[str, Any]) -> None:
    """Prove the ledger validator fails closed without depending on human prose."""

    validate_ledger(ledger)

    fixtures: list[tuple[str, dict[str, Any]]] = []

    wrong_schema = copy.deepcopy(ledger)
    wrong_schema["schema_version"] = 2
    fixtures.append(("schema version", wrong_schema))

    wrong_order = copy.deepcopy(ledger)
    wrong_order["accepted_phases"] = list(reversed(wrong_order["accepted_phases"]))
    fixtures.append(("phase order", wrong_order))

    missing_key = copy.deepcopy(ledger)
    del missing_key["accepted_phases"][-1]["merge_sha"]
    fixtures.append(("missing entry key", missing_key))

    invalid_sha = copy.deepcopy(ledger)
    invalid_sha["accepted_phases"][-1]["merge_sha"] = "not-a-sha"
    fixtures.append(("invalid merge sha", invalid_sha))

    wrong_workflow_count = copy.deepcopy(ledger)
    wrong_workflow_count["accepted_phases"][-1]["permanent_workflows"] = 13
    fixtures.append(("workflow count", wrong_workflow_count))

    for label, fixture in fixtures:
        try:
            validate_ledger(fixture)
        except ValueError:
            continue
        raise ValueError(f"accepted phase provenance self-test unexpectedly accepted {label} fixture")
