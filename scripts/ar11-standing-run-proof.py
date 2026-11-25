#!/usr/bin/env python3
"""Verify one canonical AR11 predecessor, with narrow pre-effect recovery.

This is an offline GitHub-evidence adapter. It cannot deploy, recover provider
state, acquire credentials or decide whether a Release Set is compatible.
Unknown, post-boundary and unproven failures always stop staging.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any


SHA = re.compile(r"[0-9a-f]{40}\Z")
ROUTE = "Classify accepted-main READY vs internal rollback evidence"
PREFLIGHT = "Observe provider and publish lossless read-only terminal outcome"
ROLLBACK = "Prove incompatible or unknown rollback blocks before mutation"
PROMOTE = "Execute exact staging candidate after same-run READY evidence"
TERMINAL = "Publish one lossless staging AR11 terminal outcome"
EXPECTED_JOBS = {ROUTE, PREFLIGHT, ROLLBACK, PROMOTE, TERMINAL}
RESOLVE = "Resolve unique immutable Release Set for exact protected main"
BIND = "Bind standing staging authority to exact same-run READY evidence"
BEFORE_CREDENTIALS = "Pin exact standing staging account and Worker before deploy credentials"
MUTATION_BOUNDARY = "Mark provider mutation invocation boundary"
DEPLOY = "Deploy exact Release Set v3 bits after all fences"
SAFE_POST_FAILURE = {
    "Capture staging resolve phase state",
    "Capture staging mutation phase state",
    "Capture staging post-verify phase state",
    "Complete job",
}
SAFE_PRE_BIND = {"Set up job", RESOLVE}


class EvidenceRejected(ValueError):
    """A prior effect cannot be proven harmless."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise EvidenceRejected(message)


def exactly_one(items: list[dict[str, Any]], name: str) -> dict[str, Any]:
    matches = [item for item in items if item.get("name") == name]
    require(len(matches) == 1, f"missing or duplicate {name}")
    return matches[0]


def verify_prior_run(
    history: dict[str, Any], jobs_response: dict[str, Any], current_run_id: int
) -> dict[str, Any]:
    runs = history.get("workflow_runs")
    total = history.get("total_count")
    require(
        type(total) is int and total >= 2 and isinstance(runs, list)
        and len(runs) == min(total, 100),
        "canonical promotion history is incomplete",
    )
    require(
        all(
            isinstance(run, dict) and run.get("event") == "workflow_run"
            and run.get("head_branch") == "main"
            and type(run.get("id")) is int
            and type(run.get("run_number")) is int
            and run["run_number"] > 0
            for run in runs
        ),
        "untrusted canonical promotion history",
    )
    require(
        len({run["id"] for run in runs}) == len(runs)
        and len({run["run_number"] for run in runs}) == len(runs),
        "duplicate promotion run identities",
    )
    current = [run for run in runs if run["id"] == current_run_id]
    require(
        len(current) == 1 and current[0].get("run_attempt") == 1
        and current[0].get("status") == "in_progress"
        and isinstance(current[0].get("head_sha"), str)
        and SHA.fullmatch(current[0]["head_sha"]) is not None,
        "current same-attempt promotion proof unavailable",
    )
    earlier = [run for run in runs if run["run_number"] < current[0]["run_number"]]
    require(bool(earlier), "no predecessor to establish replay fence")
    previous = max(earlier, key=lambda run: run["run_number"])
    require(
        previous.get("status") == "completed"
        and previous.get("run_attempt") == 1
        and isinstance(previous.get("head_sha"), str)
        and SHA.fullmatch(previous["head_sha"]) is not None,
        "predecessor is unsettled, retried or malformed",
    )
    if previous.get("conclusion") == "success":
        return {"decision": "PRIOR_CANONICAL_SUCCESS", "predecessor_run_id": previous["id"]}

    require(previous.get("conclusion") == "failure", "unknown previous outcome")
    jobs = jobs_response.get("jobs")
    count = jobs_response.get("total_count")
    require(
        type(count) is int and count == len(EXPECTED_JOBS)
        and isinstance(jobs, list) and len(jobs) == count
        and all(isinstance(job, dict) for job in jobs)
        and {job.get("name") for job in jobs} == EXPECTED_JOBS
        and all(
            type(job.get("id")) is int
            and job.get("run_id") == previous["id"]
            and job.get("status") == "completed"
            for job in jobs
        )
        and len({job["id"] for job in jobs}) == count,
        "prior failing run has an unverified job topology",
    )
    require(
        exactly_one(jobs, ROUTE).get("conclusion") == "success"
        and exactly_one(jobs, PREFLIGHT).get("conclusion") == "success"
        and exactly_one(jobs, ROLLBACK).get("conclusion") == "skipped"
        and exactly_one(jobs, TERMINAL).get("conclusion") == "failure",
        "prior failing run is not the isolated AR11 pre-effect case",
    )
    promote = exactly_one(jobs, PROMOTE)
    require(promote.get("status") == "completed" and promote.get("conclusion") == "failure", "prior staging job did not fail as required")
    steps = promote.get("steps")
    require(isinstance(steps, list) and all(isinstance(step, dict) for step in steps), "prior job step evidence missing")
    names = [step.get("name") for step in steps]
    require(len(names) == len(set(names)), "duplicate prior job steps")
    require(
        exactly_one(steps, RESOLVE).get("conclusion") == "success"
        and exactly_one(steps, BIND).get("conclusion") == "failure",
        "previous failure was not exactly candidate binding",
    )
    require(
        all(
            exactly_one(steps, name).get("conclusion") == "skipped"
            for name in (BEFORE_CREDENTIALS, MUTATION_BOUNDARY, DEPLOY)
        ),
        "provider credential or mutation boundary may have been reached",
    )
    bind_number = exactly_one(steps, BIND).get("number")
    require(type(bind_number) is int and bind_number > 0, "candidate binding step order missing")
    require(
        all(
            type(step.get("number")) is int
            and (
                (step["number"] < bind_number and (step.get("conclusion") != "success" or step["name"] in SAFE_PRE_BIND))
                or step["number"] == bind_number
                or step.get("conclusion") != "success"
                or step["name"] in SAFE_POST_FAILURE
            )
            for step in steps
        ),
        "unrecognized successful step after candidate binding failure",
    )
    return {"decision": "AR14_EXACT_PRE_EFFECT_RECOVERY", "predecessor_run_id": previous["id"]}


def self_test() -> None:
    sha = "a" * 40
    current = {"id": 200, "run_number": 11, "event": "workflow_run", "head_branch": "main", "run_attempt": 1, "status": "in_progress", "head_sha": sha}
    previous = {"id": 199, "run_number": 10, "event": "workflow_run", "head_branch": "main", "run_attempt": 1, "status": "completed", "conclusion": "failure", "head_sha": "b" * 40}
    history = {"total_count": 2, "workflow_runs": [current, previous]}
    steps = [
        {"name": "Set up job", "number": 1, "conclusion": "success"},
        {"name": RESOLVE, "number": 2, "conclusion": "success"},
        {"name": BIND, "number": 3, "conclusion": "failure"},
        {"name": BEFORE_CREDENTIALS, "number": 4, "conclusion": "skipped"},
        {"name": MUTATION_BOUNDARY, "number": 5, "conclusion": "skipped"},
        {"name": DEPLOY, "number": 6, "conclusion": "skipped"},
        {"name": "Capture staging mutation phase state", "number": 7, "conclusion": "success"},
    ]
    jobs = {"total_count": 5, "jobs": [
        {"id": 11, "name": ROUTE, "run_id": 199, "status": "completed", "conclusion": "success"},
        {"id": 12, "name": PREFLIGHT, "run_id": 199, "status": "completed", "conclusion": "success"},
        {"id": 13, "name": ROLLBACK, "run_id": 199, "status": "completed", "conclusion": "skipped"},
        {"id": 14, "name": PROMOTE, "run_id": 199, "status": "completed", "conclusion": "failure", "steps": steps},
        {"id": 15, "name": TERMINAL, "run_id": 199, "status": "completed", "conclusion": "failure"},
    ]}
    assert verify_prior_run(history, jobs, 200)["decision"] == "AR14_EXACT_PRE_EFFECT_RECOVERY"
    safe = json.loads(json.dumps(history))
    safe["workflow_runs"][1]["conclusion"] = "success"
    assert verify_prior_run(safe, {}, 200)["decision"] == "PRIOR_CANONICAL_SUCCESS"
    for edit in (
        lambda h, j: h["workflow_runs"][1].update(conclusion="cancelled"),
        lambda h, j: h["workflow_runs"][0].update(run_attempt=2),
        lambda h, j: j["jobs"][3]["steps"][4].update(conclusion="success"),
        lambda h, j: j["jobs"][3]["steps"][5].update(conclusion="failure"),
        lambda h, j: j["jobs"].append({"name": "Other mutation", "run_id": 199}),
        lambda h, j: j["jobs"][3]["steps"].append({"name": "Unknown effect", "number": 8, "conclusion": "success"}),
        lambda h, j: j["jobs"][3]["steps"][2].update(conclusion="skipped"),
        lambda h, j: j["jobs"][3]["steps"].insert(1, {"name": "Unexpected early effect", "number": 1, "conclusion": "success"}),
    ):
        hh, jj = json.loads(json.dumps(history)), json.loads(json.dumps(jobs))
        edit(hh, jj)
        try:
            verify_prior_run(hh, jj, 200)
        except EvidenceRejected:
            pass
        else:
            raise AssertionError("unsafe recovery was accepted")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--history", type=Path)
    parser.add_argument("--jobs", type=Path)
    parser.add_argument("--current-run-id", type=int)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("AR11 standing predecessor and AR14 pre-effect negative proofs PASS")
        return 0
    if args.history is None or args.jobs is None or args.current_run_id is None:
        parser.error("--history, --jobs and --current-run-id are required")
    try:
        result = verify_prior_run(
            json.loads(args.history.read_text(encoding="utf-8")),
            json.loads(args.jobs.read_text(encoding="utf-8")),
            args.current_run_id,
        )
    except (EvidenceRejected, OSError, ValueError, TypeError) as error:
        parser.exit(1, f"AR11_PREDECESSOR_PROOF_REJECTED: {error}\n")
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
