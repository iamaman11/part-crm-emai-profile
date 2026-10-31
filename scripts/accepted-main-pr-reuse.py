#!/usr/bin/env python3
"""Reuse exact PR test evidence after a verified, tree-identical squash merge.

This is a read-only GitHub API attestation, not a cache. A missing/ambiguous
receipt returns reuse=false and runs the ordinary heavy tests on main.
The accepted-main Release Set is still built and signed from exact main.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import urllib.error
import urllib.request
from collections.abc import Callable
from pathlib import Path
from typing import Any

REPOSITORY = "iamaman11/part-crm-emai-profile"
SHA = re.compile(r"^[0-9a-f]{40}$")
# Permanent PR workflows required by the current repository acceptance contract.
REQUIRED = frozenset({
    "Bridge Host Operations Gate", "Camoufox Runtime Gate",
    "Certification Gate", "Cross-Component Acceptance Gate",
    "D1 Evolution Gate", "Encrypted Generation Gate",
    "External Evidence Gate", "External Review Attestation Gate",
    "Frontend Gate", "GitHub Governance Gate", "Local Profile Gate",
    "Profile Generation Gate", "Quality Gate", "Release Architecture Gate",
    "Repository Quality Audit Gate", "Resolver D1 First Bootstrap",
    "Runtime Bundle Gate", "V2 Staging Envelope Observation",
})
# Exact meaningful steps must have actually RUN, not merely belong to a
# successful workflow with all expensive jobs skipped by a PR scope filter.
CRITICAL: dict[str, dict[str, tuple[str, ...]]] = {
    "camoufox": {
        "Real Camoufox cold-launch proof": (
            "Prove Profile Bridge -> managed IPC -> real Camoufox chain",
            "Prove authoritative P3 browser-state portability through managed Camoufox",
        ),
        "Profile Bridge Windows regression": (
            "Prove Windows local owned Playwright agent protocol",
            "Prove packaged Windows post-launch failure cleanup and immediate relaunch",
            "Prove clean-host Windows authoritative P3 browser-state portability",
        ),
    },
    "quality": {
        "Rust Linux and WASM": ("Test native crates",),
        "Rust Windows And Profile Bridge Artifact": (
            "Check and test native crates",
            "Compile actual per-user installer and prove native signature rejection",
        ),
    },
    "local-profile": {
        "Local Profile Linux": ("Test local profile lifecycle",),
        "Local Profile Windows": (
            "Test local profile lifecycle",
            "Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits",
            "Prove Windows release CMS verifier",
        ),
    },
}
WORKFLOW = {
    "camoufox": "Camoufox Runtime Gate",
    "quality": "Quality Gate",
    "local-profile": "Local Profile Gate",
}


class Unconfirmed(ValueError):
    pass


def require(ok: bool, reason: str) -> None:
    if not ok:
        raise Unconfirmed(reason)


def api_reader(token: str) -> Callable[[str], Any]:
    def read(path: str) -> Any:
        request = urllib.request.Request(
            f"https://api.github.com/{path}",
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {token}",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.load(response)
    return read


def prove(main_sha: str, gate: str, api: Callable[[str], Any]) -> str:
    require(bool(SHA.fullmatch(main_sha)), "invalid main commit")
    require(gate in CRITICAL, "unknown gate")
    root = f"repos/{REPOSITORY}"
    accepted = api(f"{root}/commits/{main_sha}")
    require(accepted.get("sha") == main_sha, "main SHA drift")
    parents = accepted.get("parents", [])
    require(len(parents) == 1, "not an exact single-parent squash merge")
    base = parents[0]["sha"]
    tree = accepted["commit"]["tree"]["sha"]
    associated = api(f"{root}/commits/{main_sha}/pulls")
    require(isinstance(associated, list) and len(associated) == 1, "missing/ambiguous merged PR")
    pr = associated[0]
    candidate = pr.get("head", {}).get("sha")
    require(bool(SHA.fullmatch(candidate or "")), "invalid PR head")
    require(
        pr.get("state") == "closed"
        and bool(pr.get("merged_at"))
        and pr.get("merge_commit_sha") == main_sha
        and pr.get("base", {}).get("ref") == "main"
        and pr.get("base", {}).get("sha") == base
        and pr.get("base", {}).get("repo", {}).get("full_name") == REPOSITORY
        and pr.get("head", {}).get("repo", {}).get("full_name") == REPOSITORY,
        "merge/base/repository mismatch",
    )
    original = api(f"{root}/commits/{candidate}")
    require(original.get("sha") == candidate, "PR head drift")
    require(original["commit"]["tree"]["sha"] == tree, "accepted main tree differs from tested PR")
    history = api(f"{root}/actions/runs?head_sha={candidate}&event=pull_request&per_page=100")
    require(
        isinstance(history.get("workflow_runs"), list)
        and isinstance(history.get("total_count"), int)
        and history["total_count"] <= 100,
        "incomplete PR workflow inventory",
    )
    latest: dict[str, dict[str, Any]] = {}
    for run in history["workflow_runs"]:
        if run.get("head_sha") != candidate or run.get("event") != "pull_request":
            continue
        name = run.get("name")
        if name in REQUIRED and (name not in latest or run["id"] > latest[name]["id"]):
            latest[name] = run
    require(set(latest) == REQUIRED, "required PR workflow evidence missing")
    for name, run in latest.items():
        require(
            run.get("status") == "completed" and run.get("conclusion") == "success",
            f"PR workflow not successful: {name}",
        )
    source_run = latest[WORKFLOW[gate]]
    jobs = api(f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100")
    require(
        isinstance(jobs.get("jobs"), list)
        and isinstance(jobs.get("total_count"), int)
        and jobs["total_count"] <= 100,
        "incomplete PR job inventory",
    )
    by_name = {job.get("name"): job for job in jobs["jobs"]}
    for name, steps in CRITICAL[gate].items():
        job = by_name.get(name, {})
        require(
            job.get("status") == "completed" and job.get("conclusion") == "success",
            f"required PR job did not complete: {name}",
        )
        actual = {s.get("name"): s.get("conclusion") for s in job.get("steps", [])}
        require(all(actual.get(step) == "success" for step in steps),
                f"required PR test skipped/missing: {name}")
    return candidate


def fixture(gate: str) -> tuple[dict[str, Any], Callable[[str], Any]]:
    main, base, head, tree = ("a" * 40, "b" * 40, "c" * 40, "d" * 40)
    root = f"repos/{REPOSITORY}"
    pr = {
        "state": "closed", "merged_at": "2026-10-31T00:00:00Z",
        "merge_commit_sha": main,
        "base": {"sha": base, "ref": "main", "repo": {"full_name": REPOSITORY}},
        "head": {"sha": head, "repo": {"full_name": REPOSITORY}},
    }
    runs = [
        {"id": i + 1, "name": name, "head_sha": head, "event": "pull_request",
         "status": "completed", "conclusion": "success"}
        for i, name in enumerate(sorted(REQUIRED))
    ]
    gate_run = next(run for run in runs if run["name"] == WORKFLOW[gate])
    jobs = [
        {"name": name, "status": "completed", "conclusion": "success",
         "steps": [{"name": step, "conclusion": "success"} for step in steps]}
        for name, steps in CRITICAL[gate].items()
    ]
    pages: dict[str, Any] = {
        f"{root}/commits/{main}": {
            "sha": main, "parents": [{"sha": base}], "commit": {"tree": {"sha": tree}}
        },
        f"{root}/commits/{main}/pulls": [pr],
        f"{root}/commits/{head}": {
            "sha": head, "commit": {"tree": {"sha": tree}}
        },
        f"{root}/actions/runs?head_sha={head}&event=pull_request&per_page=100":
            {"total_count": len(runs), "workflow_runs": runs},
        f"{root}/actions/runs/{gate_run['id']}/jobs?per_page=100":
            {"total_count": len(jobs), "jobs": jobs},
    }
    return pages, lambda path: pages[path]


def self_test() -> None:
    for gate in CRITICAL:
        pages, read = fixture(gate)
        require(prove("a" * 40, gate, read) == "c" * 40, "valid squash receipt rejected")
        for mutation in ("tree", "parent", "failed-run", "pending-run", "missing-job",
                         "skipped-test", "foreign-repo", "wrong-merge", "incomplete-list"):
            clone = json.loads(json.dumps(pages))
            prefix = f"repos/{REPOSITORY}"
            if mutation == "tree":
                clone[f"{prefix}/commits/{'c'*40}"]["commit"]["tree"]["sha"] = "e" * 40
            elif mutation == "parent":
                clone[f"{prefix}/commits/{'a'*40}"]["parents"][0]["sha"] = "e" * 40
            elif mutation == "foreign-repo":
                clone[f"{prefix}/commits/{'a'*40}/pulls"][0]["head"]["repo"]["full_name"] = "other/repo"
            elif mutation == "wrong-merge":
                clone[f"{prefix}/commits/{'a'*40}/pulls"][0]["merge_commit_sha"] = "e" * 40
            elif mutation == "incomplete-list":
                runpath = f"{prefix}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
                clone[runpath]["total_count"] = 101
            elif mutation in ("failed-run", "pending-run"):
                runpath = f"{prefix}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
                target = next(x for x in clone[runpath]["workflow_runs"] if x["name"] == WORKFLOW[gate])
                target["conclusion"] = "failure" if mutation == "failed-run" else None
            else:
                suffix = next(k for k in clone if k.endswith("/jobs?per_page=100"))
                if mutation == "missing-job":
                    clone[suffix]["jobs"].pop()
                    clone[suffix]["total_count"] -= 1
                else:
                    clone[suffix]["jobs"][0]["steps"][0]["conclusion"] = "skipped"
            try:
                prove("a" * 40, gate, lambda path: clone[path])
            except Unconfirmed:
                pass
            else:
                raise AssertionError(f"{gate}: accepted unsafe proof: {mutation}")
    print("Accepted-main exact PR proof: positive and negative fixtures PASS")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--gate", choices=sorted(CRITICAL))
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    if args.gate is None:
        return 0 if args.self_test else parser.error("--gate is required")
    reuse = False
    if os.environ.get("GITHUB_EVENT_NAME") == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main":
        token = os.environ.get("GITHUB_TOKEN")
        if token:
            try:
                candidate = prove(os.environ.get("GITHUB_SHA", ""), args.gate, api_reader(token))
                print(f"Reuse verified PR test evidence from exact tree {candidate}")
                reuse = True
            except (Unconfirmed, KeyError, ValueError, TypeError, urllib.error.URLError, OSError) as error:
                print(f"No reusable evidence ({type(error).__name__}); run full main gate")
        else:
            print("No GitHub API authority; run full main gate")
    else:
        print("Not an accepted-main push; full PR test is mandatory")
    if args.github_output is not None:
        with args.github_output.open("a", encoding="utf-8") as handle:
            handle.write(f"reuse={'true' if reuse else 'false'}\n")
    print(f"reuse={'true' if reuse else 'false'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
