#!/usr/bin/env python3
"""Permanent Phase 2F guard for graceful-close retained writer ownership."""

from __future__ import annotations

import argparse
from pathlib import Path

OPERATOR = Path("apps/profile-bridge/src/operator_flow.rs")
UNIT_TEST = Path("apps/profile-bridge/src/operator_flow.rs")
ACCEPTANCE_TEST = Path("apps/profile-bridge/tests/operator_flow_acceptance.rs")
SYNTHETIC = Path("apps/profile-bridge/src/bin/profile-bridge-synthetic.rs")

OPEN_PENDING_GUARD = "self.active.is_some() || self.retained_dirty.is_some()"
RETAINED_FIELD = "retained_dirty: Option<RetainedDirtyClose>"
RETAINED_HANDOFF = "RetainedDirtyClose::begin_after_browser_close("
RETAINED_ASSIGNMENT = "self.retained_dirty = Some(retained)"
FINALIZE_METHOD = "pub async fn finalize_dirty_close"
FINALIZE_RETAINED = ".retained_dirty\n            .as_mut()"
FINALIZE_DELEGATE = ".finalize("
FINALIZE_CLEAR = "self.retained_dirty = None"
SYNTHETIC_MARKER = "synthetic-operator-complete state=DIRTY_LOCAL_COMMITTED_GENERATION"

REQUIRED_UNIT_TESTS = (
    "composed_operator_close_retains_dirty_ownership_until_commit",
    "pending_dirty_close_blocks_second_ownership_before_claim_replay",
)
REQUIRED_ACCEPTANCE_TESTS = (
    "busy_invalid_and_replayed_claims_fail_before_second_ownership",
)
REQUIRED_SYNTHETIC_SURFACES = (
    "finalize_dirty_close",
    "DirtyCloseLocalOutcome::CandidateAccepted",
    "LocalGenerationState::SupersededEvictable",
    "workspace_lock_released()",
    "coordinator_lease_released()",
    "cleanup_failures().any()",
    SYNTHETIC_MARKER,
)


def read(root: Path, relative: Path) -> str:
    return (root / relative).read_text(encoding="utf-8")


def function_body(source: str, marker: str) -> str:
    start = source.find(marker)
    if start < 0:
        return ""
    opening = source.find("{", start)
    if opening < 0:
        return ""
    depth = 0
    for index in range(opening, len(source)):
        character = source[index]
        if character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return source[opening : index + 1]
    return ""


def failures_for_sources(
    operator: str,
    unit_test: str,
    acceptance_test: str,
    synthetic: str,
) -> list[str]:
    failures: list[str] = []
    production = operator.split("#[cfg(test)]", 1)[0]

    for fragment in (RETAINED_FIELD, OPEN_PENDING_GUARD, FINALIZE_METHOD):
        if fragment not in production:
            failures.append(f"missing retained operator invariant: {fragment}")

    open_flow = function_body(production, "pub fn open(")
    pending_guard = open_flow.find(OPEN_PENDING_GUARD)
    device_identity = open_flow.find(".device_identity")
    enrollment = open_flow.find(".redeem_claim(")
    if min(pending_guard, device_identity, enrollment) < 0 or not (
        pending_guard < device_identity < enrollment
    ):
        failures.append(
            "pending dirty ownership must block before device/enrollment claim processing"
        )

    close_flow = function_body(production, "pub fn close(")
    runtime_close = close_flow.find("RuntimeSessionOrchestrator::close(")
    handoff = close_flow.find(RETAINED_HANDOFF)
    assignment = close_flow.find(RETAINED_ASSIGNMENT)
    failure_cleanup = close_flow.find("cleanup_active_session")
    if min(runtime_close, handoff, assignment, failure_cleanup) < 0 or not (
        runtime_close < handoff < assignment < failure_cleanup
    ):
        failures.append(
            "graceful close must hand writer ownership to RetainedDirtyClose before failure-only cleanup"
        )
    success_region = close_flow[handoff:assignment] if handoff >= 0 and assignment >= 0 else ""
    for forbidden in ("workspace_lock.release()", "coordinator.close_lease"):
        if forbidden in success_region:
            failures.append(
                f"graceful retained-close handoff must not release ownership early: {forbidden}"
            )

    finalize_flow = function_body(production, FINALIZE_METHOD)
    retained = finalize_flow.find(FINALIZE_RETAINED)
    delegated = finalize_flow.find(FINALIZE_DELEGATE)
    cleanup = finalize_flow.find("workspace_lock: !completion.workspace_lock_released()")
    terminal = finalize_flow.find("let terminal = OperatorTerminalRecord")
    clear = finalize_flow.find(FINALIZE_CLEAR)
    if min(retained, delegated, cleanup, terminal, clear) < 0 or not (
        retained < delegated < cleanup < terminal < clear
    ):
        failures.append(
            "dirty close finalization must use retained ownership and clear it only after authoritative completion bookkeeping"
        )

    for test_name in REQUIRED_UNIT_TESTS:
        if test_name not in unit_test:
            failures.append(f"missing retained operator unit test: {test_name}")
    for test_name in REQUIRED_ACCEPTANCE_TESTS:
        if test_name not in acceptance_test:
            failures.append(f"missing retained operator acceptance test: {test_name}")
    for fragment in REQUIRED_SYNTHETIC_SURFACES:
        if fragment not in synthetic:
            failures.append(f"missing retained operator synthetic evidence: {fragment}")

    return failures


def check(root: Path) -> list[str]:
    try:
        operator = read(root, OPERATOR)
        unit_test = read(root, UNIT_TEST)
        acceptance_test = read(root, ACCEPTANCE_TEST)
        synthetic = read(root, SYNTHETIC)
    except OSError as error:
        return [f"could not read Phase 2F retained-operator source: {error}"]
    return failures_for_sources(operator, unit_test, acceptance_test, synthetic)


def self_test(root: Path) -> list[str]:
    try:
        operator = read(root, OPERATOR)
        unit_test = read(root, UNIT_TEST)
        acceptance_test = read(root, ACCEPTANCE_TEST)
        synthetic = read(root, SYNTHETIC)
    except OSError as error:
        return [f"could not read Phase 2F retained-operator source: {error}"]

    fixture = operator.replace(OPEN_PENDING_GUARD, "self.active.is_some()", 1)
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("pending dirty ownership" in failure or OPEN_PENDING_GUARD in failure for failure in rejected):
        return ["retained-operator premature second-ownership fixture unexpectedly passed"]

    fixture = operator.replace(RETAINED_HANDOFF, "RetainedDirtyClose::removed_handoff(", 1)
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("graceful close must hand writer ownership" in failure for failure in rejected):
        return ["retained-operator missing graceful handoff fixture unexpectedly passed"]

    fixture = operator.replace(FINALIZE_CLEAR, "/* retained ownership clear removed */", 1)
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("clear it only after authoritative completion" in failure for failure in rejected):
        return ["retained-operator missing post-finalize clear fixture unexpectedly passed"]

    return []


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()

    failures = self_test(root) if args.self_test else check(root)
    if failures:
        for failure in failures:
            print(f"ERROR: {failure}")
        return 1
    if args.self_test:
        print("Phase 2F retained-operator negative fixtures were rejected.")
    else:
        print("Phase 2F retained operator ownership policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
