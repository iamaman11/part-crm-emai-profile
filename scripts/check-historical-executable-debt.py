#!/usr/bin/env python3
"""Fail closed when post-AR-11 historical executable debt drifts.

The inventory classifies suspicious lifecycle/documentation/D3/recovery paths by current role.
Historical Git data may be inspected statically, but retired implementation must never be
materialized, imported or executed just to prove the past.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "architecture/historical-executable-debt.json"
ALLOWED = {
    "CURRENT_INVARIANT",
    "UPGRADE_ROLLBACK_REQUIRED",
    "TRANSITION_PROVENANCE_ONLY",
    "DEAD",
}
EXECUTABLE_PREFIXES = (".github/scripts/", ".github/workflows/", "scripts/", "tools/")
EXECUTABLE_SUFFIXES = {".py", ".sh", ".mjs", ".js", ".cjs", ".ts", ".yml", ".yaml", ".rs"}
FORWARD_CLOSEOUT = re.compile(
    r"(?:^|/)(?:ar|architecture-ar)[0-9]+[^/]*(?:acceptance-)?closeout[^/]*\.(?:py|mjs|js|yml|yaml)$",
    re.IGNORECASE,
)
STRING_ONLY_REFERENCE = re.compile(r"^[\"'][^\"']+[\"'],?$")
STRING_ASSIGNMENT_REFERENCE = re.compile(
    r"^(?:(?:const|let|var)\s+[A-Za-z_$][A-Za-z0-9_$]*\s*=\s*[\"'][^\"']+[\"'];?|[\"'][^\"']+[\"']\s*:\s*[\"'][^\"']+[\"'],?)$"
)


class DebtError(ValueError):
    pass


def fail(message: str) -> None:
    raise DebtError(message)


def load_inventory() -> dict[str, Any]:
    try:
        value = json.loads(INVENTORY.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {INVENTORY.relative_to(ROOT)}: {error}")
    if not isinstance(value, dict):
        fail("historical executable debt inventory must contain one JSON object")
    return value


def tracked_paths() -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=False
    )
    if result.returncode != 0:
        fail("git ls-files failed while discovering executable debt surfaces")
    return sorted(
        raw.decode("utf-8") for raw in result.stdout.split(b"\0") if raw
    )


def executable_paths(paths: list[str]) -> list[str]:
    return [
        path
        for path in paths
        if path.startswith(EXECUTABLE_PREFIXES) and Path(path).suffix in EXECUTABLE_SUFFIXES
    ]


def text(relative: str) -> str:
    try:
        return (ROOT / relative).read_text(encoding="utf-8")
    except (OSError, UnicodeError):
        return ""


def reference_kind(line: str) -> str:
    stripped = line.strip()
    if not stripped or stripped.startswith(("#", "//", "/*", "*")):
        return "static_reference"
    if re.search(r"\btest\s+!\s+-[ef]\b", stripped) or "must stay absent" in stripped.lower():
        return "absence_assertion"
    if STRING_ONLY_REFERENCE.fullmatch(stripped) or STRING_ASSIGNMENT_REFERENCE.fullmatch(stripped):
        return "static_reference"
    return "caller"


def discover_references(target: str, paths: list[str]) -> tuple[list[str], list[str], list[str]]:
    callers: set[str] = set()
    absence: set[str] = set()
    static: set[str] = set()
    for relative in paths:
        if relative == target:
            continue
        source = text(relative)
        if target not in source:
            continue
        for line in source.splitlines():
            if target not in line:
                continue
            kind = reference_kind(line)
            if kind == "caller":
                callers.add(relative)
            elif kind == "absence_assertion":
                absence.add(relative)
            else:
                static.add(relative)
    return sorted(callers), sorted(absence), sorted(static)


def validate_policy(payload: dict[str, Any]) -> None:
    if (
        payload.get("schema_version") != 1
        or payload.get("kind") != "HISTORICAL_EXECUTABLE_DEBT_INVENTORY"
        or payload.get("status") != "current"
        or payload.get("tracking_issue") != 375
    ):
        fail("historical executable debt inventory identity/version/ownership drifted")
    policy = payload.get("policy")
    if not isinstance(policy, dict):
        fail("historical executable debt policy is missing")
    if set(policy.get("allowed_classifications", [])) != ALLOWED:
        fail("historical executable debt classification taxonomy drifted")
    expected = {
        "historical_git_static_read_allowed": True,
        "retired_code_materialization_allowed": False,
        "retired_code_import_allowed": False,
        "retired_code_execution_allowed": False,
        "forward_per_ar_closeout_executable_allowed": False,
        "production_mutation": False,
        "caller_discovery": "DERIVED_FROM_CURRENT_EXECUTABLE_REFERENCES_BY_VALIDATOR",
    }
    for key, wanted in expected.items():
        if policy.get(key) != wanted:
            fail(f"historical executable debt policy drifted: {key}")


def validate_entries(payload: dict[str, Any], paths: list[str]) -> list[dict[str, object]]:
    entries = payload.get("entries")
    if not isinstance(entries, list) or not entries:
        fail("historical executable debt inventory has no entries")
    observed: set[str] = set()
    path_set = set(paths)
    executable = executable_paths(paths)
    report: list[dict[str, object]] = []

    required_fields = {
        "path",
        "classification",
        "expected_present",
        "standalone_entrypoint",
        "proves",
        "duplicate_or_successor",
        "reads_git_history",
        "executes_historical_code",
        "materializes_retired_code",
        "runtime_mutation_authority",
        "remote_mutation_authority",
        "disposition",
    }

    for raw in entries:
        if not isinstance(raw, dict):
            fail("historical executable debt entry must be an object")
        missing = required_fields - set(raw)
        if missing:
            fail(f"historical executable debt entry is missing fields: {sorted(missing)}")
        target = raw.get("path")
        classification = raw.get("classification")
        if not isinstance(target, str) or not target or target in observed:
            fail(f"historical executable debt path is invalid or duplicated: {target!r}")
        observed.add(target)
        if classification not in ALLOWED:
            fail(f"invalid debt classification for {target}: {classification}")
        if not all(isinstance(raw.get(key), bool) for key in (
            "expected_present",
            "standalone_entrypoint",
            "reads_git_history",
            "executes_historical_code",
            "materializes_retired_code",
            "runtime_mutation_authority",
            "remote_mutation_authority",
        )):
            fail(f"debt classification booleans are malformed for {target}")
        present = target in path_set
        if present is not raw["expected_present"]:
            fail(
                f"debt path presence drifted for {target}: expected_present={raw['expected_present']} actual={present}"
            )
        if raw["executes_historical_code"] or raw["materializes_retired_code"]:
            fail(f"retired historical execution/materialization is forbidden: {target}")
        if classification in {"TRANSITION_PROVENANCE_ONLY", "DEAD"} and (
            raw["runtime_mutation_authority"] or raw["remote_mutation_authority"]
        ):
            fail(f"provenance/dead path may not retain mutation authority: {target}")
        if classification == "DEAD" and present:
            fail(f"DEAD executable must be absent: {target}")
        if classification == "TRANSITION_PROVENANCE_ONLY" and not present:
            fail(f"provenance-only path unexpectedly disappeared without inventory update: {target}")
        if classification == "CURRENT_INVARIANT" and (
            raw["runtime_mutation_authority"] or raw["remote_mutation_authority"]
        ):
            fail(f"current validation invariant unexpectedly owns mutation authority: {target}")
        if classification == "UPGRADE_ROLLBACK_REQUIRED" and not present:
            fail(f"required recovery/rollback authority is missing: {target}")

        callers, absence, static = discover_references(target, executable)
        if classification in {"TRANSITION_PROVENANCE_ONLY", "DEAD"} and callers:
            fail(f"retired path has current executable callers: {target}: {callers}")
        if classification in {"CURRENT_INVARIANT", "UPGRADE_ROLLBACK_REQUIRED"}:
            if not callers and not raw["standalone_entrypoint"]:
                fail(f"current/recovery path has no executable caller and is not standalone: {target}")

        report.append(
            {
                "path": target,
                "classification": classification,
                "callers": callers,
                "absence_assertions": absence,
                "static_references": static,
            }
        )
    return report


def validate_no_forward_closeout(paths: list[str]) -> None:
    offenders = [path for path in executable_paths(paths) if FORWARD_CLOSEOUT.search(path)]
    if offenders:
        fail(f"per-AR closeout executable machinery is forbidden in forward procedure: {offenders}")


def validate_no_retired_materialization(payload: dict[str, Any], paths: list[str]) -> None:
    retired = [
        entry["path"]
        for entry in payload["entries"]
        if entry["classification"] in {"DEAD", "TRANSITION_PROVENANCE_ONLY"}
    ]
    for relative in executable_paths(paths):
        source = text(relative)
        if not source:
            continue
        compact = re.sub(r"\s+", " ", source)
        for target in retired:
            if target not in source:
                continue
            escaped = re.escape(target)
            materialize = re.search(
                rf"git\s+(?:show|checkout|restore)\b.{{0,800}}(?:>|--|\s){escaped}",
                compact,
                flags=re.IGNORECASE,
            )
            if materialize:
                fail(f"retired code materialization path detected in {relative}: {target}")


def validate(payload: dict[str, Any]) -> list[dict[str, object]]:
    validate_policy(payload)
    paths = tracked_paths()
    validate_no_forward_closeout(paths)
    report = validate_entries(payload, paths)
    validate_no_retired_materialization(payload, paths)
    return report


def self_test(payload: dict[str, Any]) -> None:
    validate(payload)

    bad_policy = copy.deepcopy(payload)
    bad_policy["policy"]["retired_code_execution_allowed"] = True
    try:
        validate_policy(bad_policy)
    except DebtError:
        pass
    else:
        fail("retired-code-execution policy negative fixture unexpectedly passed")

    bad_classification = copy.deepcopy(payload)
    bad_classification["entries"][0]["classification"] = "LEGACY_BUT_PROBABLY_FINE"
    try:
        validate_policy(bad_classification)
        validate_entries(bad_classification, tracked_paths())
    except DebtError:
        pass
    else:
        fail("unknown debt classification negative fixture unexpectedly passed")

    bad_mutation = copy.deepcopy(payload)
    mutated_retired = False
    for entry in bad_mutation["entries"]:
        if entry["classification"] in {"TRANSITION_PROVENANCE_ONLY", "DEAD"}:
            entry["remote_mutation_authority"] = True
            mutated_retired = True
            break
    if not mutated_retired:
        fail("retired mutation-authority negative fixture has no retired entry")
    try:
        validate_entries(bad_mutation, tracked_paths())
    except DebtError:
        pass
    else:
        fail("retired mutation-authority negative fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    payload = load_inventory()
    if args.self_test:
        self_test(payload)
        print("Historical executable debt negative matrix passed.")
        return 0
    report = validate(payload)
    print(json.dumps({"historical_executable_debt": report}, indent=2, sort_keys=True))
    print("Historical executable debt inventory is current; retired code replay remains fail closed.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except DebtError as error:
        print(f"historical executable debt check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
