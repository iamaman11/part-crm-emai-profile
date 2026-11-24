#!/usr/bin/env python3
"""Reuse exact PR test evidence after a verified, tree-identical PR merge.

This is a read-only GitHub API attestation, not a cache. A missing/ambiguous
receipt returns reuse=false and runs the ordinary heavy tests on main.
The accepted-main Release Set is still built and signed from exact main.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import runpy
import subprocess
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
        # Keep these names bound to the *current* executable Windows steps.
        # The real browser, immutable package and shipping reader are separate
        # jobs; the final protected status depends on all three.
        "Rust Bridge contracts (browser-independent)": (
            "Run existing Rust Bridge tests",
        ),
        "Build and verify official Windows Camoufox runtime": (
            "Materialize official browser and verified offline fpgen model",
            "Prove official Firefox native restored tabs and POST safety on exact runtime",
            "Package exact compressed Release Set runtime input",
            "Upload exact-head official runtime for downstream Windows proofs",
        ),
        "Rust shipping reader verifies exact compressed Windows runtime": (
            "Run production Windows runtime archive reader",
        ),
        "Profile Bridge Windows regression": (
            "Run official Camoufox Playwright user Open and native file-lock proof through production Bridge crates",
        ),
        # A successful meta status cannot replace the underlying real tests.
        "Real Camoufox cold-launch proof": (
            "Require exact official Camoufox and Bridge Windows proof",
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


def required_workflow_paths() -> dict[str, str]:
    """Bind run names to their unique tracked workflow paths in this exact tree."""
    directory = Path(__file__).resolve().parent.parent / ".github/workflows"
    paths: dict[str, str] = {}
    for source in (*directory.glob("*.yml"), *directory.glob("*.yaml")):
        names = [line.partition(": ")[2].strip().strip("'\"")
                 for line in source.read_text(encoding="utf-8").splitlines()
                 if line.startswith("name: ")]
        for name in names:
            if name in REQUIRED:
                require(name not in paths, f"duplicate required workflow name: {name}")
                paths[name] = f".github/workflows/{source.name}"
    require(set(paths) == REQUIRED, "required workflow path missing from exact source")
    return paths


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


def prove(main_sha: str, gate: str, api: Callable[[str], Any],
          *, require_steps: bool = True) -> str:
    require(bool(SHA.fullmatch(main_sha)), "invalid main commit")
    require(gate in CRITICAL, "unknown gate")
    root = f"repos/{REPOSITORY}"
    accepted = api(f"{root}/commits/{main_sha}")
    require(accepted.get("sha") == main_sha, "main SHA drift")
    parents = accepted.get("parents", [])
    require(isinstance(parents, list) and len(parents) in (1, 2),
            "not an exact squash or two-parent PR merge")
    require(all(isinstance(parent, dict) and
                isinstance(parent.get("sha"), str) and
                SHA.fullmatch(parent["sha"]) for parent in parents),
            "invalid accepted-main parent SHA")
    base = parents[0]["sha"]
    tree = accepted["commit"]["tree"]["sha"]
    associated = api(f"{root}/commits/{main_sha}/pulls")
    require(isinstance(associated, list) and len(associated) == 1, "missing/ambiguous merged PR")
    pr = associated[0]
    candidate = pr.get("head", {}).get("sha")
    require(bool(SHA.fullmatch(candidate or "")), "invalid PR head")
    head_branch = pr.get("head", {}).get("ref")
    require(isinstance(head_branch, str) and bool(head_branch), "invalid PR head branch")
    if len(parents) == 2:
        require(parents[1]["sha"] == candidate,
                "merge second parent differs from tested PR head")
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
    paths = required_workflow_paths()
    for name, run in latest.items():
        require(
            run.get("status") == "completed" and run.get("conclusion") == "success",
            f"PR workflow not successful: {name}",
        )
        require(
            run.get("path") == paths[name]
            and run.get("head_branch") == head_branch
            and isinstance(run.get("repository"), dict)
            and run["repository"].get("full_name") == REPOSITORY
            and isinstance(run.get("head_repository"), dict)
            and run["head_repository"].get("full_name") == REPOSITORY
            and type(run.get("run_attempt")) is int
            and run["run_attempt"] >= 1,
            f"PR workflow path/repository/attempt mismatch: {name}",
        )
    # A successful, exact-tree PR with its heavy steps skipped is only a
    # candidate for a separately proven, unchanged prior-runtime receipt.
    # It must never be used as a standalone physical proof.
    if not require_steps:
        require(gate == "camoufox", "only Camoufox has unchanged-input proof reuse")
        return candidate
    witnesses = [(WORKFLOW[gate], CRITICAL[gate])]
    if gate == "camoufox":
        # The shipping reader no longer reruns all 541 native Bridge library
        # tests. Reuse is therefore allowed only when Quality Windows really
        # executed those tests at this exact PR head and attempt.
        witnesses.append((WORKFLOW["quality"], {
            "Rust Windows And Profile Bridge Artifact": ("Check and test native crates",),
        }))
    for workflow_name, required_jobs in witnesses:
        source_run = latest[workflow_name]
        jobs = api(f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100")
        require(
            isinstance(jobs.get("jobs"), list)
            and isinstance(jobs.get("total_count"), int)
            and jobs["total_count"] <= 100,
            f"incomplete PR job inventory: {workflow_name}",
        )
        by_name = {job.get("name"): job for job in jobs["jobs"]}
        for name, steps in required_jobs.items():
            job = by_name.get(name, {})
            require(
                job.get("status") == "completed" and job.get("conclusion") == "success",
                f"required PR job did not complete: {name}",
            )
            require(
                job.get("run_id") == source_run["id"]
                and job.get("run_attempt") == source_run["run_attempt"]
                and job.get("head_sha") == candidate
                and job.get("workflow_name") == workflow_name,
                f"required PR job run/attempt/head mismatch: {name}",
            )
            actual = {s.get("name"): s.get("conclusion") for s in job.get("steps", [])}
            require(all(actual.get(step) == "success" for step in steps),
                    f"required PR test skipped/missing: {name}")
    return candidate


def git_changed_paths(old_sha: str, new_sha: str) -> list[str]:
    """Read exact tracked tree differences; no globs or guessed components."""
    require(bool(SHA.fullmatch(old_sha) and SHA.fullmatch(new_sha)), "invalid diff SHA")
    raw = subprocess.check_output(
        ["git", "diff", "--no-renames", "--name-only", "-z", old_sha, new_sha]
    )
    return [name.decode("utf-8") for name in raw.split(b"\0") if name]


def git_first_parent(base_sha: str) -> list[str]:
    require(bool(SHA.fullmatch(base_sha)), "invalid base")
    lines = subprocess.check_output(
        ["git", "rev-list", "--first-parent", "--max-count=16", base_sha],
        text=True,
    ).splitlines()
    require(bool(lines) and lines[0] == base_sha, "invalid ancestor chain")
    require(all(SHA.fullmatch(candidate) for candidate in lines), "invalid ancestor SHA")
    return lines


def camoufox_unrelated(paths: list[str]) -> bool:
    # One fail-closed path classifier owns both PR gating and main proof reuse.
    if not paths:
        return True  # Byte-identical relevant source trees are unchanged.
    classifier = runpy.run_path(
        str(Path(__file__).with_name("classify-camoufox-runtime-scope.py"))
    )["classify"]
    heavy, _, _ = classifier(paths)
    return not heavy


def prove_unchanged_prior_camoufox(
    main_sha: str,
    base_sha: str,
    api: Callable[[str], Any],
    ancestors: Callable[[str], list[str]],
    changed: Callable[[str, str], list[str]],
    scope_unrelated: Callable[[list[str]], bool],
) -> str:
    """Trust a past *executed* official Windows proof only if every diff is safe.

    The current accepted main PR has already passed exact-tree/18-workflow
    validation. Past proof comes from the existing GitHub PR run and exact-tree
    PR-merge receipt; no new registry, signing, cache, or source-of-truth.
    """
    require(bool(SHA.fullmatch(base_sha)), "invalid current base")
    chain = ancestors(base_sha)
    require(bool(chain) and chain[0] == base_sha and len(chain) <= 16,
            "missing or unbounded first-parent history")
    for prior in chain:
        require(bool(SHA.fullmatch(prior)), "invalid historical commit")
        if not scope_unrelated(changed(prior, main_sha)):
            # Fail closed at first changed browser/Bridge/shared contract.
            break
        try:
            prove(prior, "camoufox", api)  # MUST have executed real Windows steps.
            return prior
        except (Unconfirmed, KeyError, ValueError, TypeError,
                urllib.error.URLError, OSError):
            continue
    raise Unconfirmed("no recent unchanged completed physical Camoufox proof")


def fixture(gate: str) -> tuple[dict[str, Any], Callable[[str], Any]]:
    main, base, head, tree = ("a" * 40, "b" * 40, "c" * 40, "d" * 40)
    root = f"repos/{REPOSITORY}"
    paths = required_workflow_paths()
    pr = {
        "state": "closed", "merged_at": "2026-10-31T00:00:00Z",
        "merge_commit_sha": main,
        "base": {"sha": base, "ref": "main", "repo": {"full_name": REPOSITORY}},
        "head": {"sha": head, "ref": "codex/exact-pr-proof",
                 "repo": {"full_name": REPOSITORY}},
    }
    runs = [
        {"id": i + 1, "name": name, "head_sha": head, "event": "pull_request",
         "status": "completed", "conclusion": "success",
         "path": paths[name],
         "head_branch": "codex/exact-pr-proof",
         "repository": {"full_name": REPOSITORY},
         "head_repository": {"full_name": REPOSITORY}, "run_attempt": 1}
        for i, name in enumerate(sorted(REQUIRED))
    ]
    gate_run = next(run for run in runs if run["name"] == WORKFLOW[gate])
    jobs = [
        {"name": name, "status": "completed", "conclusion": "success",
         "run_id": gate_run["id"], "run_attempt": gate_run["run_attempt"],
         "head_sha": head, "workflow_name": WORKFLOW[gate],
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
    if gate == "camoufox":
        quality_run = next(run for run in runs if run["name"] == WORKFLOW["quality"])
        pages[f"{root}/actions/runs/{quality_run['id']}/jobs?per_page=100"] = {
            "total_count": 1,
            "jobs": [{
                "name": "Rust Windows And Profile Bridge Artifact",
                "status": "completed", "conclusion": "success",
                "run_id": quality_run["id"], "run_attempt": quality_run["run_attempt"],
                "head_sha": head, "workflow_name": WORKFLOW["quality"],
                "steps": [{"name": "Check and test native crates", "conclusion": "success"}],
            }],
        }
    return pages, lambda path: pages[path]


def self_test() -> None:
    for gate in CRITICAL:
        pages, read = fixture(gate)
        require(prove("a" * 40, gate, read) == "c" * 40, "valid squash receipt rejected")
        prefix = f"repos/{REPOSITORY}"
        mainpath = f"{prefix}/commits/{'a'*40}"
        merge_pages = json.loads(json.dumps(pages))
        merge_pages[mainpath]["parents"].append({"sha": "c" * 40})
        require(prove("a" * 40, gate, lambda path: merge_pages[path]) == "c" * 40,
                "valid exact two-parent merge receipt rejected")
        for mutation in ("wrong-head-parent", "swapped-parents", "third-parent",
                         "merge-tree", "merge-failed-run", "merge-skipped-test"):
            clone = json.loads(json.dumps(merge_pages))
            if mutation == "wrong-head-parent":
                clone[mainpath]["parents"][1]["sha"] = "e" * 40
            elif mutation == "swapped-parents":
                clone[mainpath]["parents"].reverse()
            elif mutation == "third-parent":
                clone[mainpath]["parents"].append({"sha": "e" * 40})
            elif mutation == "merge-tree":
                clone[f"{prefix}/commits/{'c'*40}"]["commit"]["tree"]["sha"] = "e" * 40
            elif mutation == "merge-failed-run":
                runpath = f"{prefix}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
                target = next(x for x in clone[runpath]["workflow_runs"]
                              if x["name"] == WORKFLOW[gate])
                target["conclusion"] = "failure"
            else:
                jobpath = next(k for k in clone if k.endswith("/jobs?per_page=100"))
                clone[jobpath]["jobs"][0]["steps"][0]["conclusion"] = "skipped"
            try:
                prove("a" * 40, gate, lambda path: clone[path])
            except Unconfirmed:
                pass
            else:
                raise AssertionError(f"{gate}: accepted unsafe two-parent receipt: {mutation}")
        for mutation in ("tree", "parent", "failed-run", "pending-run", "missing-job",
                         "skipped-test", "foreign-repo", "wrong-merge", "incomplete-list",
                         "foreign-run-path", "foreign-run-branch", "foreign-run-repo", "foreign-head-repo",
                         "missing-run-attempt", "stale-job-attempt", "foreign-job-run",
                         "foreign-job-head", "foreign-job-workflow"):
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
            elif mutation in ("foreign-run-path", "foreign-run-branch", "foreign-run-repo", "foreign-head-repo",
                              "missing-run-attempt"):
                runpath = f"{prefix}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
                target = next(x for x in clone[runpath]["workflow_runs"]
                              if x["name"] == WORKFLOW[gate])
                if mutation == "foreign-run-path":
                    target["path"] = ".github/workflows/foreign-duplicate-name.yml"
                elif mutation == "foreign-run-branch":
                    target["head_branch"] = "foreign/branch"
                elif mutation == "foreign-run-repo":
                    target["repository"]["full_name"] = "other/repo"
                elif mutation == "foreign-head-repo":
                    target["head_repository"]["full_name"] = "other/repo"
                else:
                    target.pop("run_attempt")
            elif mutation in ("failed-run", "pending-run"):
                runpath = f"{prefix}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
                target = next(x for x in clone[runpath]["workflow_runs"] if x["name"] == WORKFLOW[gate])
                target["conclusion"] = "failure" if mutation == "failed-run" else None
            elif mutation in ("foreign-job-run", "foreign-job-head", "foreign-job-workflow",
                              "stale-job-attempt"):
                jobpath = next(k for k in clone if k.endswith("/jobs?per_page=100"))
                job = clone[jobpath]["jobs"][0]
                if mutation == "foreign-job-run":
                    job["run_id"] += 1
                elif mutation == "foreign-job-head":
                    job["head_sha"] = "e" * 40
                elif mutation == "foreign-job-workflow":
                    job["workflow_name"] = "Other Workflow"
                else:
                    job["run_attempt"] += 1
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
        # The source workflow splits the native proof across several jobs.
        # Missing or skipped evidence in *any* job must force a full replay.
        for required_job, required_steps in CRITICAL[gate].items():
            jobpath = next(k for k in pages if k.endswith("/jobs?per_page=100"))
            for mutation, skipped_step in [("missing", None)] + [
                ("skipped", name) for name in required_steps
            ]:
                clone = json.loads(json.dumps(pages))
                jobs = clone[jobpath]["jobs"]
                target = next(job for job in jobs if job["name"] == required_job)
                if mutation == "missing":
                    jobs.remove(target)
                    clone[jobpath]["total_count"] -= 1
                else:
                    next(step for step in target["steps"] if step["name"] == skipped_step)[
                        "conclusion"
                    ] = "skipped"
                try:
                    prove("a" * 40, gate, lambda path: clone[path])
                except Unconfirmed:
                    pass
                else:
                    raise AssertionError(
                        f"{gate}: accepted {mutation} evidence: {required_job}/{skipped_step}"
                    )
        if gate == "camoufox":
            quality_run = next(run for run in pages[
                f"{prefix}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
            ]["workflow_runs"] if run["name"] == WORKFLOW["quality"])
            quality_path = f"{prefix}/actions/runs/{quality_run['id']}/jobs?per_page=100"
            for mutation in ("missing", "failed-job", "skipped-step", "failed-step",
                             "stale-attempt", "wrong-run", "wrong-head", "wrong-workflow"):
                clone = json.loads(json.dumps(pages))
                evidence = clone[quality_path]
                job = evidence["jobs"][0]
                if mutation == "missing":
                    evidence["jobs"] = []
                    evidence["total_count"] = 0
                elif mutation == "failed-job":
                    job["conclusion"] = "failure"
                elif mutation == "skipped-step":
                    job["steps"][0]["conclusion"] = "skipped"
                elif mutation == "failed-step":
                    job["steps"][0]["conclusion"] = "failure"
                elif mutation == "stale-attempt":
                    job["run_attempt"] += 1
                elif mutation == "wrong-run":
                    job["run_id"] += 1
                elif mutation == "wrong-head":
                    job["head_sha"] = "e" * 40
                else:
                    job["workflow_name"] = WORKFLOW["camoufox"]
                try:
                    prove("a" * 40, gate, lambda path: clone[path])
                except Unconfirmed:
                    pass
                else:
                    raise AssertionError(f"Camoufox accepted unsafe Quality Windows proof: {mutation}")
    # Fail-closed historical reuse: current PR has all 18 successful checks
    # but SKIPPED heavy jobs, while older accepted main has exact real proof.
    now, _ = fixture("camoufox")
    root = f"repos/{REPOSITORY}"
    now_sha, base_sha, prior_head = ("a" * 40, "b" * 40, "f" * 40)
    current_run = f"{root}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
    current_camoufox = next(
        run for run in now[current_run]["workflow_runs"]
        if run["name"] == WORKFLOW["camoufox"]
    )
    current_jobs = f"{root}/actions/runs/{current_camoufox['id']}/jobs?per_page=100"
    old_jobs = json.loads(json.dumps(now[current_jobs]))
    current_quality = next(
        run for run in now[current_run]["workflow_runs"]
        if run["name"] == WORKFLOW["quality"]
    )
    current_quality_jobs = f"{root}/actions/runs/{current_quality['id']}/jobs?per_page=100"
    old_quality_jobs = json.loads(json.dumps(now[current_quality_jobs]))
    prior_run = f"{root}/actions/runs?head_sha={prior_head}&event=pull_request&per_page=100"
    prior_runs = json.loads(json.dumps(now[current_run]))
    for run in prior_runs["workflow_runs"]:
        run["head_sha"] = prior_head
        run["id"] += 100
    prior_camoufox = next(
        run for run in prior_runs["workflow_runs"]
        if run["name"] == WORKFLOW["camoufox"]
    )
    prior_quality = next(
        run for run in prior_runs["workflow_runs"]
        if run["name"] == WORKFLOW["quality"]
    )
    for job in old_jobs["jobs"]:
        job["run_id"] = prior_camoufox["id"]
        job["head_sha"] = prior_head
    for job in old_quality_jobs["jobs"]:
        job["run_id"] = prior_quality["id"]
        job["head_sha"] = prior_head
    prior_jobs = f"{root}/actions/runs/{prior_camoufox['id']}/jobs?per_page=100"
    prior_quality_jobs = f"{root}/actions/runs/{prior_quality['id']}/jobs?per_page=100"
    now[f"{root}/commits/{base_sha}"] = {
        "sha": base_sha,
        "parents": [{"sha": "e" * 40}],
        "commit": {"tree": {"sha": "8" * 40}},
    }
    now[f"{root}/commits/{base_sha}/pulls"] = [{
        "state": "closed", "merged_at": "2026-10-31T00:00:00Z",
        "merge_commit_sha": base_sha,
        "base": {
            "sha": "e" * 40, "ref": "main",
            "repo": {"full_name": REPOSITORY},
        },
        "head": {"sha": prior_head, "ref": "codex/prior-proof",
                 "repo": {"full_name": REPOSITORY}},
    }]
    now[f"{root}/commits/{prior_head}"] = {
        "sha": prior_head, "commit": {"tree": {"sha": "8" * 40}},
    }
    now[prior_run] = prior_runs
    for run in prior_runs["workflow_runs"]:
        run["head_branch"] = "codex/prior-proof"
    now[prior_jobs] = old_jobs
    now[prior_quality_jobs] = old_quality_jobs
    for job in now[current_jobs]["jobs"]:
        for step in job["steps"]:
            step["conclusion"] = "skipped"
    api = lambda path: now[path]
    try:
        prove(now_sha, "camoufox", api)
    except Unconfirmed:
        pass
    else:
        raise AssertionError("skipped PR steps were treated as physical proof")
    require(
        prove(now_sha, "camoufox", api, require_steps=False) == "c" * 40,
        "successful exact-tree PR receipt lost",
    )
    require(
        prove(base_sha, "camoufox", api) == prior_head,
        "existing real prior browser proof not accepted",
    )
    safe = ["scripts/release-core-overlay-ar11.py"]
    assert camoufox_unrelated(safe)
    assert not camoufox_unrelated(safe + ["apps/profile-bridge/src/main.rs"])
    assert not camoufox_unrelated(safe + ["runtime/camouhost/real.py"])
    def preserved(_old: str, _new: str) -> list[str]:
        return safe
    require(prove_unchanged_prior_camoufox(
        now_sha, base_sha, api, lambda _: [base_sha],
        preserved, camoufox_unrelated,
    ) == base_sha, "unchanged real prior browser proof lost")
    for unsafe in (
        ["runtime/camouhost/real.py"],
        ["apps/profile-bridge/src/main.rs"],
        [".github/workflows/camoufox-runtime-gate.yml"],
        ["scripts/classify-camoufox-runtime-scope.py"],
    ):
        try:
            prove_unchanged_prior_camoufox(
                now_sha, base_sha, api, lambda _: [base_sha],
                lambda _old, _new: safe + unsafe, camoufox_unrelated,
            )
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"unsafe prior browser proof reused for {unsafe}")
    prior_camoufox["conclusion"] = "failure"
    try:
        prove_unchanged_prior_camoufox(
            now_sha, base_sha, api, lambda _: [base_sha],
            preserved, camoufox_unrelated,
        )
    except Unconfirmed:
        pass
    else:
        raise AssertionError("failed prior PR workflow reused")
    prior_camoufox["conclusion"] = "success"
    now[prior_jobs]["jobs"][0]["steps"][0]["conclusion"] = "skipped"
    try:
        prove_unchanged_prior_camoufox(
            now_sha, base_sha, api, lambda _: [base_sha],
            preserved, camoufox_unrelated,
        )
    except Unconfirmed:
        pass
    else:
        raise AssertionError("skipped prior physical step reused")
    now[prior_jobs]["jobs"][0]["steps"][0]["conclusion"] = "success"
    now[prior_quality_jobs]["jobs"][0]["steps"][0]["conclusion"] = "skipped"
    try:
        prove_unchanged_prior_camoufox(
            now_sha, base_sha, api, lambda _: [base_sha],
            preserved, camoufox_unrelated,
        )
    except Unconfirmed:
        pass
    else:
        raise AssertionError("skipped prior Quality Windows native tests reused")
    now[prior_quality_jobs]["jobs"][0]["steps"][0]["conclusion"] = "success"
    try:
        prove_unchanged_prior_camoufox(
            now_sha, base_sha, api, lambda _: [],
            preserved, camoufox_unrelated,
        )
    except Unconfirmed:
        pass
    else:
        raise AssertionError("missing parent history passed")

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
        main_sha = os.environ.get("GITHUB_SHA", "")
        if token:
            try:
                api = api_reader(token)
                candidate = prove(main_sha, args.gate, api)
                print(f"Reuse verified PR test evidence from exact tree {candidate}")
                reuse = True
            except (Unconfirmed, KeyError, ValueError, TypeError, urllib.error.URLError, OSError) as error:
                if args.gate == "camoufox":
                    try:
                        # The current PR may legitimately skip heavy browser tests
                        # for an audited unrelated change. Require its exact-tree
                        # merge + all 18 successful workflow receipts first.
                        prove(main_sha, args.gate, api, require_steps=False)
                        checked_out = subprocess.check_output(
                            ["git", "rev-parse", "HEAD"], text=True
                        ).strip()
                        require(checked_out == main_sha, "local source/main identity drift")
                        base_sha = subprocess.check_output(
                            ["git", "rev-parse", f"{main_sha}^"], text=True
                        ).strip()
                        prior = prove_unchanged_prior_camoufox(
                            main_sha, base_sha, api, git_first_parent,
                            git_changed_paths, camoufox_unrelated,
                        )
                        print(f"Reuse previously executed exact Camoufox/Bridge proof at accepted main {prior};")
                        print("all later source changes were mechanically proven runtime-unrelated")
                        reuse = True
                    except (Unconfirmed, KeyError, ValueError, TypeError,
                            urllib.error.URLError, OSError, subprocess.SubprocessError,
                            UnicodeError) as prior_error:
                        print(f"No reusable Camoufox history ({type(prior_error).__name__}); full main test required")
                else:
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
