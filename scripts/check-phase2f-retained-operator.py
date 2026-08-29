#!/usr/bin/env python3
"""Permanent Phase 2F guard for graceful-close retained writer ownership."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

OPERATOR = Path("apps/profile-bridge/src/operator_flow.rs")
UNIT_TEST = Path("apps/profile-bridge/src/operator_flow.rs")
ACCEPTANCE_TEST = Path("apps/profile-bridge/tests/operator_flow_acceptance.rs")
SYNTHETIC = Path("apps/profile-bridge/src/bin/profile-bridge-synthetic.rs")

AUTHORITATIVE_OPEN = "pub fn open_authoritative"
SINGLE_OPEN_BODY = "fn open_with_materialization"
OPEN_PENDING_GUARD = "self.active.is_some() || self.retained_dirty.is_some()"
RETAINED_FIELD = "retained_dirty: Option<RetainedDirtyClose>"
RETAINED_HANDOFF = "RetainedDirtyClose::begin_after_browser_close("
RETAINED_ASSIGNMENT = "self.retained_dirty = Some(retained)"
SAVE_METHOD = "pub fn save_retained_successor"
SAVE_CONTROL = "ControlPlaneGenerationSuccessor::new(transport, &mut self.coordinator)"
SAVE_PREPARE = "prepare_retained_generation_successor("
SAVE_COMMIT = "publish_verify_and_commit_successor("
SAVE_COMPLETE = ".complete_committed_successor("
SAVE_CLEAR = "self.retained_dirty = None"
LEGACY_FINALIZE = "pub async fn finalize_dirty_close"
LEGACY_FINALIZE_GUARD = (
    '#[cfg(any(test, feature = "synthetic-test-bin"))]\n'
    "    #[allow(clippy::too_many_arguments)]\n"
    "    pub async fn finalize_dirty_close"
)
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


def regex_position(source: str, pattern: str) -> int:
    match = re.search(pattern, source)
    return -1 if match is None else match.start()


def failures_for_sources(
    operator: str,
    unit_test: str,
    acceptance_test: str,
    synthetic: str,
) -> list[str]:
    failures: list[str] = []
    production = operator.split("#[cfg(test)]", 1)[0]

    for fragment in (
        RETAINED_FIELD,
        AUTHORITATIVE_OPEN,
        SINGLE_OPEN_BODY,
        OPEN_PENDING_GUARD,
        SAVE_METHOD,
    ):
        if fragment not in production:
            failures.append(f"missing retained operator invariant: {fragment}")

    if LEGACY_FINALIZE not in operator or LEGACY_FINALIZE_GUARD not in operator:
        failures.append(
            "historical DeviceJob finalize must exist only behind test/synthetic cfg during predecessor migration"
        )

    open_flow = function_body(production, SINGLE_OPEN_BODY)
    pending_guard = open_flow.find(OPEN_PENDING_GUARD)
    device_identity = open_flow.find(".device_identity")
    enrollment = open_flow.find(".redeem_claim(")
    if min(pending_guard, device_identity, enrollment) < 0 or not (
        pending_guard < device_identity < enrollment
    ):
        failures.append(
            "pending dirty ownership must block in the single launch body before device/enrollment claim processing"
        )

    authoritative_flow = function_body(production, AUTHORITATIVE_OPEN)
    delegate = authoritative_flow.find("self.open_with_materialization(")
    rematerialize = authoritative_flow.find("ensure_authoritative_generation(")
    if min(delegate, rematerialize) < 0 or delegate >= rematerialize:
        failures.append(
            "production launch must delegate to the single launch body with authoritative generation rematerialization"
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

    save_flow = function_body(production, SAVE_METHOD)
    retained = regex_position(
        save_flow,
        r"self\s*\.\s*retained_dirty\s*\.\s*as_ref\s*\(\s*\)",
    )
    control = save_flow.find(SAVE_CONTROL)
    prepare = save_flow.find(SAVE_PREPARE)
    commit = save_flow.find(SAVE_COMMIT)
    complete = save_flow.find(SAVE_COMPLETE)
    clear = save_flow.rfind(SAVE_CLEAR)
    if min(retained, control, prepare, commit, complete, clear) < 0 or not (
        retained < control < prepare < commit < complete < clear
    ):
        failures.append(
            "canonical retained save must preserve prepare -> exact verify/commit -> post-commit completion -> clear ordering"
        )
    precommit_region = save_flow[:commit] if commit >= 0 else ""
    if SAVE_CLEAR in precommit_region or "coordinator.close_lease" in precommit_region:
        failures.append(
            "pre-commit failure region must retain dirty writer and coordinator ownership"
        )
    if "generation_id: committed.generation_id().clone()" not in save_flow:
        failures.append(
            "post-commit terminal bookkeeping must use committed N+1 generation identity"
        )
    if "DirtyCloseLocalOutcome::RematerializationBlocked" not in save_flow:
        failures.append(
            "post-commit local rematerialization failure must remain fail-closed"
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

    fixture = operator.replace("ensure_authoritative_generation(", "removed_rematerialization(", 1)
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("authoritative generation rematerialization" in failure for failure in rejected):
        return ["retained-operator missing authoritative rematerialization fixture unexpectedly passed"]

    fixture = operator.replace(RETAINED_HANDOFF, "RetainedDirtyClose::removed_handoff(", 1)
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("graceful close must hand writer ownership" in failure for failure in rejected):
        return ["retained-operator missing graceful handoff fixture unexpectedly passed"]

    fixture = operator.replace(SAVE_COMMIT, "removed_successor_commit(", 1)
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("canonical retained save" in failure for failure in rejected):
        return ["retained-operator missing canonical successor commit fixture unexpectedly passed"]

    fixture = operator.replace(LEGACY_FINALIZE_GUARD, LEGACY_FINALIZE, 1)
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("test/synthetic cfg" in failure for failure in rejected):
        return ["retained-operator production legacy-finalize fixture unexpectedly passed"]

    fixture = operator.replace(SAVE_CLEAR, "/* retained ownership clear removed */")
    rejected = failures_for_sources(fixture, unit_test, acceptance_test, synthetic)
    if not any("canonical retained save" in failure for failure in rejected):
        return ["retained-operator missing post-commit clear fixture unexpectedly passed"]

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
