#!/usr/bin/env python3
"""Reuse exact PR test evidence after a verified, tree-identical PR merge.

This is a read-only GitHub API attestation, not a cache. A missing/ambiguous
receipt returns reuse=false and runs the ordinary heavy tests on main.
The accepted-main Release Set is still built and signed from exact main.
"""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import os
import re
import runpy
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
import zlib
from collections.abc import Callable, Iterable
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

REPOSITORY = "iamaman11/part-crm-emai-profile"
SHA = re.compile(r"^[0-9a-f]{40}$")
# One exact PR workflow run owns the protected result and all source evidence.
CI_WORKFLOW = "CI"
REQUIRED = frozenset({CI_WORKFLOW})
# Exact meaningful steps must have actually RUN, not merely belong to a
# successful workflow with all expensive jobs skipped by a PR scope filter.
RUST_LINUX_FULL_STEPS = (
    "Test AR-6 opsctl Rust and doctor evidence",
    "Test Phase 2D query application and synthetic Bridge adapter",
    "Format",
    "Clippy native crates",
    "Test native crates",
    "Prove C2 Gmail OAuth fragment is deterministically generated",
    "Test Cloudflare adapter logic",
    "Test Worker native helpers",
    "Test mailbox secret resolver runtime",
    "Check extracted identity application crate for Workers target",
    "Check pure crates for Workers target",
)
RUST_LINUX_STATIC_REQUIRED = (
    "Enforce frontend sibling-feature boundaries",
    "Prove sibling feature internals are rejected",
    "Verify generated frontend contracts are current",
    "Prove Phase 2D grant-safe projections and query plans",
    "Scan tracked files for high-confidence secrets",
)
RUST_LINUX_LIGHT_CERT = "Certify inherited exact FULL Rust Linux proof"

CRITICAL: dict[str, dict[str, tuple[str, ...]]] = {
    "camoufox": {
        # One Windows job materializes, proves, packages, clean-extracts and
        # physically exercises exactly the same runtime bytes without a second
        # runner or artifact transport. Every listed step must actually finish
        # on the exact PR head/attempt before accepted-main can reuse proof.
        "Rust Bridge contracts (browser-independent)": (
            "Run existing Rust Bridge tests",
        ),
        "Profile Bridge Windows regression": (
            "Checkout exact candidate",
            "Verify exact source checkout",
            "Validate existing runtime owners",
            "Materialize official browser and verified offline fpgen model",
            "Prove official Firefox native restored tabs and POST safety on exact runtime",
            "Package exact compressed Release Set runtime input",
            "Extract and verify exact official runtime",
            "Run physical shipping reader and native Bridge Open on exact packaged runtime",
            "Bind exact physically tested runtime archive and manifest SHA",
            "Verify physical PR runtime uploads match exact tested SHA",
            "Upload SHA-bound physical runtime proof receipt",
            "Verify uploaded physical runtime proof receipt digest",
        ),
        # A successful meta status cannot replace the underlying real tests.
        "Real Camoufox cold-launch proof": (
            "Require exact official Camoufox and Bridge Windows proof",
        ),
    },
    "quality": {
        "Rust Linux and WASM": RUST_LINUX_STATIC_REQUIRED + RUST_LINUX_FULL_STEPS,
        "Cloudflare Worker Release Build": (
            "Build release Worker artifact",
            "Verify Worker artifact",
            "Build release Static Assets artifact",
            "Build immutable release package",
            "Verify immutable release archive against exact source",
            "Upload exact immutable release tar",
            "Verify uploaded artifact digest",
            "Build immutable Resolver Worker artifact",
            "Verify immutable Resolver archive against exact source",
            "Upload exact immutable Resolver tar",
            "Verify uploaded Resolver artifact digest",
        ),
        "Rust Windows And Profile Bridge Artifact": (
            "Check and test native crates",
            "Build release Profile Bridge executable",
            "Compile actual per-user installer and prove native signature rejection",
            "Upload disposable exact-head Windows Bridge candidate",
            "Verify uploaded Bridge artifact digest",
            "Write FULL Quality Windows runner identity receipt",
            "Upload FULL Quality Windows runner identity receipt",
            "Verify uploaded FULL Quality runner identity digest",
        ),
    },
    "local-profile": {
        "Local Profile Linux": ("Test local profile lifecycle",),
        "Local Profile Windows Core": (
            "Test local profile lifecycle",
            "Prove Windows lost-Commit K1 exact sparse WAL successor",
            "Prove OS-owned Restore scratch cleanup after Windows process death",
            "Prove native Open diagnostic cause on Windows",
            "Prove READY v11-to-v12 lost-Commit recovery and dirty-byte refusal",
            "Prove Windows Playwright Ready health confirmation through native process stderr",
            "Prove Windows release CMS verifier",
            "Prove application delivery signing without OS certificate-store changes",
            "Build AR-6 opsctl on Windows",
            "Exercise AR-6 opsctl read-only commands on Windows",
            "Write FULL Local Windows runner identity receipt",
            "Upload FULL Local Windows runner identity receipt",
            "Verify uploaded FULL Local runner identity digest",
        ),
        "Local Profile Windows K2": (
            "Verify exact source checkout",
            "Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits",
            "Write FULL K2 Windows runner identity receipt",
            "Upload FULL K2 Windows runner identity receipt",
            "Verify uploaded FULL K2 runner identity digest",
        ),
        # This is the existing branch-protected status context. Its success
        # must be downstream of both independently executing Windows jobs.
        "Local Profile Windows": ("Require both exact-head Windows proof jobs",),
    },
    "d1": {
        "Native opsctl and pinned Wrangler D1 proof": (
            "Verify exact source checkout",
            "Enforce pinned Wrangler version",
            "Verify protected executor and release identity contracts",
            "Prove K2 608 MiB D1 successor-v2 migration and append-only recovery",
            "Prove K2 append-only historical WORKING evidence cannot authorize prune",
            "Execute all opsctl D1 commands as real CLI processes",
            "Execute reusable D1 fault and recovery harness",
            "Upload D1 fault and recovery evidence",
        ),
        "Native opsctl D1 CLI portability on Windows": (
            "Verify exact source checkout",
            "Execute all native opsctl D1 commands on Windows",
        ),
    },
}
WORKFLOW = {gate: CI_WORKFLOW for gate in CRITICAL}

# A native-unaffected PR is not allowed to present a green-only surrogate.
# Its own Windows jobs must have executed the exact inherited-EXE verification,
# installer and upload, or the source-safe prior-proof certification. The
# expensive native test steps remain mandatory on every other PR.
NATIVE_REUSED_CRITICAL: dict[str, dict[str, tuple[str, ...]]] = {
    "quality": {
        # Static/source checks still execute on every PR. Cargo can be FULL,
        # or exactly inherited with a separately certified FULL proof.
        "Rust Linux and WASM": RUST_LINUX_STATIC_REQUIRED,
        "Cloudflare Worker Release Build": CRITICAL["quality"]["Cloudflare Worker Release Build"],
        "Rust Windows And Profile Bridge Artifact": (
            "Verify exact source checkout",
            "Reuse exact prior FULL-proof Profile Bridge executable",
            "Compile actual per-user installer and prove native signature rejection",
            "Upload disposable exact-head Windows Bridge candidate",
            "Verify uploaded Bridge artifact digest",
        ),
    },
    "local-profile": {
        "Local Profile Linux": ("Test local profile lifecycle",),
        "Local Profile Windows Core": (
            "Verify exact source checkout",
            "Certify unchanged exact prior FULL native physical proof",
        ),
        "Local Profile Windows K2": (
            "Verify exact source checkout",
            "Certify unchanged exact prior FULL native physical proof",
        ),
        "Local Profile Windows": ("Require both exact-head Windows proof jobs",),
    },
}

RUNNER_RECEIPT = {
    "quality": ("quality-native-runner.json", "Rust Windows And Profile Bridge Artifact"),
    "local-profile": ("local-native-runner.json", "Local Profile Windows Core"),
    # Independent physical K2 runs on a different Windows runner than Core.
    # Its own witnessed image/compiler cannot be inferred from Core's receipt.
    "local-profile-k2": ("local-k2-native-runner.json", "Local Profile Windows K2"),
}


class Unconfirmed(ValueError):
    pass


class GitHubProofReadUnavailable(Unconfirmed):
    """No reliable GitHub observation was available; not a proof rejection."""


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
    require(paths == {CI_WORKFLOW: ".github/workflows/ci.yml"},
            "exact consolidated CI workflow path missing from source")
    return paths


def protected_required_jobs() -> frozenset[str]:
    """Read the exact 22 protected contexts from their single desired-state owner."""
    source = Path(__file__).resolve().parent.parent / "architecture/github-governance.json"
    contract = json.loads(source.read_text(encoding="utf-8"))
    require(contract.get("repository") == REPOSITORY, "foreign GitHub governance contract")
    jobs = contract.get("main", {}).get("required_checks")
    require(isinstance(jobs, list) and len(jobs) == 22
            and all(isinstance(job, str) and bool(job) for job in jobs)
            and len(set(jobs)) == len(jobs),
            "protected required jobs missing or ambiguous")
    return frozenset(jobs)


def expected_pr_jobs() -> dict[str, str]:
    """Bind every consolidated CI source job, not only protected contexts.

    The D1 accepted-main router is the one intentionally skipped PR job.
    Everything else must genuinely complete successfully, including unprotected
    security/scope/proof jobs. Scope may skip expensive *steps*, never a job.
    """
    source = Path(__file__).resolve().parent.parent / ".github/workflows/ci.yml"
    contents = source.read_text(encoding="utf-8")
    require("\njobs:\n" in contents, "CI source lacks job inventory")
    body = contents.split("\njobs:\n", 1)[1]
    headers = list(re.finditer(r"(?m)^  ([a-z][a-z0-9-]*):[ \t]*$", body))
    require(len(headers) == 35, "consolidated CI source job inventory drift")
    expected: dict[str, str] = {}
    for index, header in enumerate(headers):
        job_id = header.group(1)
        block = body[header.end():headers[index + 1].start()
                     if index + 1 < len(headers) else len(body)]
        names = re.findall(r"(?m)^    name: (.+)$", block)
        require(len(names) == 1, f"CI job name missing/ambiguous: {job_id}")
        name = names[0].strip().strip("'\"")
        require(bool(name) and name not in expected,
                f"duplicate/empty CI source job name: {name}")
        # Exactly one PR-only skip is a native GitHub event condition, not
        # a successful-but-unexecuted proof or a job-side shell bypass.
        if job_id == "d1-evolution-accepted-pr-proof":
            require(re.search(r"(?m)^    if: github.event_name == 'push'$", block)
                    is not None, "D1 router must be push-only")
            expected[name] = "skipped"
        else:
            expected[name] = "success"
    require("Prove exact accepted FULL D1 PR evidence" in expected
            and expected["Prove exact accepted FULL D1 PR evidence"] == "skipped"
            and protected_required_jobs().issubset(expected),
            "unmatched protected or PR-only CI source jobs")
    return expected


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
        # Exact 35-job GitHub evidence sometimes times out mid-read. Retry
        # transport failures only; an invalid or unauthorized proof must not
        # be treated as transient, cached or downgraded to a green result.
        for attempt in range(3):
            try:
                with urllib.request.urlopen(request, timeout=12) as response:
                    return json.load(response)
            except urllib.error.HTTPError as error:
                if error.code not in {408, 429, 500, 502, 503, 504}:
                    if error.code in {401, 403}:
                        raise GitHubProofReadUnavailable(
                            f"GitHub proof read unauthorized (HTTP {error.code}) at {path}"
                        ) from error
                    raise
                last_error: Exception = error
            except (urllib.error.URLError, TimeoutError) as error:
                last_error = error
            if attempt == 2:
                raise GitHubProofReadUnavailable(
                    f"GitHub proof read exhausted after 3 attempts at {path}: "
                    f"{type(last_error).__name__}: {last_error}"
                ) from last_error
            time.sleep(0.25 * (attempt + 1))
        raise AssertionError("unreachable GitHub proof read")
    return read


def prove(main_sha: str, gate: str, api: Callable[[str], Any],
          *, require_steps: bool = True, native_reused: bool = False,
          proven_run: dict[str, Any] | None = None) -> str:
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
    source_run = latest[WORKFLOW[gate]]
    jobs = api(f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100")
    require(
        isinstance(jobs.get("jobs"), list)
        and isinstance(jobs.get("total_count"), int)
        and jobs["total_count"] <= 100,
        "incomplete PR job inventory",
    )
    by_name: dict[str, dict[str, Any]] = {}
    protected = protected_required_jobs()
    expected_jobs = expected_pr_jobs()
    expected_critical = NATIVE_REUSED_CRITICAL[gate] if native_reused else CRITICAL[gate]
    for job in jobs["jobs"]:
        name = job.get("name")
        require(name in expected_jobs and name not in by_name,
                f"unknown/duplicate source CI PR job: {name}")
        by_name[name] = job
    require(set(by_name) == set(expected_jobs) and jobs["total_count"] == len(by_name),
            "complete 35-job source CI PR inventory missing")
    for name, expected_conclusion in expected_jobs.items():
        job = by_name[name]
        require(job.get("status") == "completed"
                and job.get("conclusion") == expected_conclusion,
                f"source CI PR job not in required state: {name}")
        require(job.get("run_id") == source_run["id"]
                and job.get("run_attempt") == source_run["run_attempt"]
                and job.get("head_sha") == candidate
                and job.get("workflow_name") == CI_WORKFLOW,
                f"source CI PR job run/attempt/head/workflow mismatch: {name}")
    for name in protected:
        require(by_name[name]["conclusion"] == "success",
                f"protected PR status is not success: {name}")
    # A complete exact-tree CI run with skipped heavy Camoufox steps can
    # certify only its source identity, never physical execution by itself.
    if not require_steps:
        require(not native_reused, "ambiguous combined proof-mode bypass")
        require(gate == "camoufox", "only Camoufox has unchanged-input proof reuse")
        return candidate
    if native_reused:
        require(gate in NATIVE_REUSED_CRITICAL, "native reuse not permitted for gate")
    for name, steps in expected_critical.items():
        job = by_name.get(name, {})
        require(
            job.get("status") == "completed" and job.get("conclusion") == "success",
            f"required PR job did not complete: {name}",
        )
        require(
            job.get("run_id") == source_run["id"]
            and job.get("run_attempt") == source_run["run_attempt"]
            and job.get("head_sha") == candidate
            and job.get("workflow_name") == WORKFLOW[gate],
            f"required PR job run/attempt/head mismatch: {name}",
        )
        actual: dict[str, tuple[str | None, str | None]] = {}
        for step in job.get("steps", []):
            label = step.get("name")
            require(label not in actual, f"duplicate PR step: {name}/{label}")
            actual[label] = (step.get("status"), step.get("conclusion"))
        require(all(actual.get(step) == ("completed", "success") for step in steps),
                f"required PR test skipped/missing: {name}")
        if native_reused and gate == "quality" and name == "Rust Linux and WASM":
            full = all(actual.get(step) == ("completed", "success")
                       for step in RUST_LINUX_FULL_STEPS)
            light = (actual.get(RUST_LINUX_LIGHT_CERT) == ("completed", "success")
                     and all(actual.get(step) == ("completed", "skipped")
                             for step in RUST_LINUX_FULL_STEPS))
            require(full != light, "Quality Rust Linux is neither exact FULL nor proven LIGHT")
            if full:
                require(actual.get(RUST_LINUX_LIGHT_CERT) in
                        (None, ("completed", "skipped")),
                        "Quality Rust Linux falsely certified inheritance during FULL")
        if native_reused and (
            name == RUNNER_RECEIPT[gate][1]
            or (gate == "local-profile" and name == "Local Profile Windows K2")
        ):
            # A continue-on-error step can report conclusion=success despite
            # outcome=failure. The physical fallback must then run. Require
            # every FULL-only step truly skipped before accepting inheritance;
            # a mixed or green-only path is never a valid reused proof.
            for full_step in CRITICAL[gate][name]:
                if full_step in steps:
                    continue  # Installer/upload are still physically executed.
                require(actual.get(full_step) == ("completed", "skipped"),
                        f"native inherited path executed/missed FULL step: {full_step}")
    if proven_run is not None:
        proven_run.update(source_run)
    return candidate


RELEASE_ARTIFACTS = {
    "cloud": re.compile(r"cloudflare-v1-sha256-[0-9a-f]{64}\.tar\Z"),
    "resolver": re.compile(r"mailbox-secret-resolver-v1-sha256-[0-9a-f]{64}\.tar\Z"),
    "bridge": re.compile(r"profile-bridge\.exe\Z"),
}
RELEASE_PRODUCERS = {
    "cloud": "Cloudflare Worker Release Build",
    "resolver": "Cloudflare Worker Release Build",
    "bridge": "Rust Windows And Profile Bridge Artifact",
}
DIGEST = re.compile(r"sha256:([0-9a-f]{64})\Z")


def github_time(value: str) -> datetime:
    require(isinstance(value, str) and value.endswith("Z"), "missing UTC receipt time")
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def prove_release_artifacts(main_sha: str, api: Callable[[str], Any]) -> dict[str, Any]:
    """Bind the actual PR binaries to exact tree, Quality attempt and producers.

    A Quality run's artifact API does not expose run_attempt. Bound each artifact
    to the unique successful producer *job in that attempt* by creation time.
    """
    candidate, native_prior = prove_accepted_native_reuse(main_sha, "quality", api)
    root = f"repos/{REPOSITORY}"
    runs = api(f"{root}/actions/runs?head_sha={candidate}&event=pull_request&per_page=100")
    quality = max((run for run in runs["workflow_runs"] if run.get("name") == WORKFLOW["quality"]),
                  key=lambda run: run["id"])
    # GitHub may return an empty pull_requests list even on a real PR run.
    # Bind to the unique merged PR through SHA, branch, tree and completed time
    # instead; never accept a post-merge rerun as pre-merge quality evidence.
    merged_pr = api(f"{root}/commits/{main_sha}/pulls")[0]
    require(github_time(quality.get("created_at"))
            <= github_time(quality.get("updated_at"))
            <= github_time(merged_pr.get("merged_at")),
            "PR Quality attempt was not completed before merge")
    jobs = api(f"{root}/actions/runs/{quality['id']}/jobs?per_page=100")["jobs"]
    producers: dict[str, dict[str, Any]] = {}
    for job in jobs:
        if job.get("name") in RELEASE_PRODUCERS.values():
            require(job["name"] not in producers, "duplicate release artifact producer")
            require(job.get("run_id") == quality["id"]
                    and job.get("run_attempt") == quality["run_attempt"]
                    and job.get("conclusion") == "success", "producer attempt drift")
            producers[job["name"]] = job
    require(set(producers) == set(RELEASE_PRODUCERS.values()), "release producer missing")
    inventory = api(f"{root}/actions/runs/{quality['id']}/artifacts?per_page=100")
    require(isinstance(inventory.get("artifacts"), list)
            and type(inventory.get("total_count")) is int
            and inventory["total_count"] <= 100
            and inventory["total_count"] == len(inventory["artifacts"]),
            "incomplete release artifact inventory")
    selected: dict[str, dict[str, Any]] = {}
    for component, pattern in RELEASE_ARTIFACTS.items():
        name_matches = [a for a in inventory["artifacts"]
                        if isinstance(a.get("name"), str) and pattern.fullmatch(a["name"])]
        require(len(name_matches) == 1, f"ambiguous/missing PR {component} artifact")
        artifact = name_matches[0]
        producer = producers[RELEASE_PRODUCERS[component]]
        workflow = artifact.get("workflow_run") or {}
        digest = artifact.get("digest")
        started = github_time(producer.get("started_at"))
        completed = github_time(producer.get("completed_at"))
        created = github_time(artifact.get("created_at"))
        expires = github_time(artifact.get("expires_at"))
        require(
            type(artifact.get("id")) is int and artifact["id"] > 0
            and type(artifact.get("size_in_bytes")) is int and artifact["size_in_bytes"] > 0
            and isinstance(digest, str) and DIGEST.fullmatch(digest) is not None
            and artifact.get("expired") is False and expires > datetime.now(timezone.utc)
            and workflow.get("id") == quality["id"]
            and workflow.get("head_sha") == candidate
            and workflow.get("head_branch") == quality["head_branch"]
            and started <= created <= completed,
            f"unbound PR {component} artifact identity/attempt/digest",
        )
        selected[component] = {
            "id": artifact["id"], "name": artifact["name"],
            "sha256": DIGEST.fullmatch(digest).group(1),
            "size": artifact["size_in_bytes"],
        }
    require(len({receipt["id"] for receipt in selected.values()}) == len(selected),
            "artifact IDs overlap")
    if native_prior is not None:
        # A current PR may upload an inherited Bridge only after its original
        # FULL physical-proved producer and all transitive native inputs were
        # revalidated. The current raw artifact must contain EXACTLY those
        # previous bytes; a green installer/new upload is not proof alone.
        prior_receipt = prove_release_artifacts(native_prior, api)
        for field in ("sha256", "size"):
            require(selected["bridge"][field] == prior_receipt["artifacts"]["bridge"][field],
                    f"current inherited Bridge {field} differs from FULL proof")
    return {
        "candidate_sha": candidate,
        "quality_run_id": quality["id"],
        "quality_run_attempt": quality["run_attempt"],
        "native_prior_main_sha": native_prior,
        "artifacts": selected,
    }


def verify_downloaded_raw(destination: Path, kind: str, artifact: dict[str, Any]) -> Path:
    allowed_names = {artifact["name"]}
    files = list(destination.iterdir())
    require(len(files) == 1 and files[0].is_file() and not files[0].is_symlink()
            and files[0].name in allowed_names, "downloaded artifact shape mismatch")
    actual = files[0]
    require(actual.stat().st_size == artifact["size"], "artifact byte count mismatch")
    digest = hashlib.sha256()
    with actual.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    require(digest.hexdigest() == artifact["sha256"], "artifact byte digest mismatch")
    return actual


def signed_artifact_url(value: str) -> str:
    """Accept only an HTTPS object-store redirect; never forward the API token."""
    require(isinstance(value, str), "missing signed artifact URL")
    parts = urllib.parse.urlsplit(value)
    host = parts.hostname or ""
    require(parts.scheme == "https" and parts.username is None
            and parts.password is None and parts.port in (None, 443)
            and (host.endswith(".blob.core.windows.net")
                 or host.endswith(".githubusercontent.com")
                 or host.endswith(".amazonaws.com")),
            "unexpected artifact redirect origin")
    return value


def download_raw_artifact(artifact: dict[str, Any], destination: Path, token: str) -> None:
    """GitHub raw artifacts are NOT ZIP: `gh run download` cannot read them.

    The artifact-ID endpoint returns a short-lived, signed object-store URL.
    The first API request carries the GitHub token; the subsequent raw byte
    request carries no credentials. Size and digest are checked afterwards.
    """
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None

    endpoint = f"https://api.github.com/repos/{REPOSITORY}/actions/artifacts/{artifact['id']}/zip"
    request = urllib.request.Request(endpoint, headers={
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28",
    })
    try:
        with urllib.request.build_opener(NoRedirect).open(request, timeout=20):
            raise Unconfirmed("artifact API did not return a signed raw redirect")
    except urllib.error.HTTPError as response:
        require(response.code in (302, 303, 307),
                f"artifact API rejected raw download: HTTP {response.code}")
        url = signed_artifact_url(response.headers.get("Location"))
    with urllib.request.urlopen(url, timeout=35) as payload, destination.open("wb") as output:
        remaining = artifact["size"]
        while True:
            chunk = payload.read(min(1024 * 1024, remaining + 1))
            if not chunk:
                break
            require(len(chunk) <= remaining, "raw artifact exceeds expected byte count")
            output.write(chunk)
            remaining -= len(chunk)
        require(remaining == 0, "raw artifact download truncated")


RUNTIME_RECEIPT_FIELDS = frozenset({
    "schema_version", "kind", "source_commit_sha", "run_id", "run_attempt",
    "release_id", "origin", "archive_sha256", "archive_size_bytes",
    "manifest_sha256", "manifest_size_bytes",
})


def prove_runtime_producer(main_sha: str, api: Callable[[str], Any]) -> tuple[str, str]:
    """Select the current FULL Camoufox PR, or an unchanged FULL ancestor.

    A LIGHT PR is never a physical receipt. Historical FULL jobs without the
    new published-byte receipt are rejected by the artifact lookup below.
    """
    try:
        return main_sha, prove(main_sha, "camoufox", api)
    except Unconfirmed:
        prove(main_sha, "camoufox", api, require_steps=False)
        accepted = api(f"repos/{REPOSITORY}/commits/{main_sha}")
        parents = accepted.get("parents", [])
        require(isinstance(parents, list) and len(parents) in (1, 2),
                "runtime receipt has no accepted-main ancestor")
        base = parents[0]["sha"]
        prior = prove_unchanged_prior_camoufox(
            main_sha, base, api,
            lambda sha: list(github_first_parent(sha, api)),
            lambda old, new: github_changed_paths(old, new, api),
            camoufox_unrelated,
        )
        return prior, prove(prior, "camoufox", api)


def runtime_proof_artifacts(
    main_sha: str, api: Callable[[str], Any], expected_release_id: str,
) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    """Bind physical runtime artifacts to the exact tested PR job/attempt.

    Unlike a release identity derived only from source inputs, this contract
    requires a byte digest emitted *after* real Firefox and Bridge tests.
    """
    require(bool(SHA.fullmatch(main_sha)), "invalid main SHA for runtime custody")
    require(re.fullmatch(r"runtime-bundle-v3-sha256-[0-9a-f]{64}",
                         expected_release_id) is not None,
            "invalid exact runtime component identity")
    physical_main, candidate = prove_runtime_producer(main_sha, api)
    root = f"repos/{REPOSITORY}"
    accepted = api(f"{root}/commits/{physical_main}/pulls")
    require(isinstance(accepted, list) and len(accepted) == 1,
            "runtime physical PR merge is ambiguous")
    merged_at = github_time(accepted[0].get("merged_at"))
    runs = api(f"{root}/actions/runs?head_sha={candidate}&event=pull_request&per_page=100")
    require(isinstance(runs.get("workflow_runs"), list)
            and type(runs.get("total_count")) is int
            and runs["total_count"] <= 100,
            "incomplete runtime PR run inventory")
    matches = [run for run in runs["workflow_runs"]
               if run.get("name") == WORKFLOW["camoufox"]
               and run.get("head_sha") == candidate]
    require(bool(matches), "runtime physical workflow run missing")
    source = max(matches, key=lambda run: run["id"])
    require(source.get("status") == "completed"
            and source.get("conclusion") == "success"
            and github_time(source.get("created_at")) <=
            github_time(source.get("updated_at")) <= merged_at,
            "runtime physical workflow not completed before merge")
    jobs = api(f"{root}/actions/runs/{source['id']}/jobs?per_page=100")
    require(isinstance(jobs.get("jobs"), list)
            and type(jobs.get("total_count")) is int
            and jobs["total_count"] <= 100,
            "incomplete physical runtime producer jobs")
    producers = [job for job in jobs["jobs"]
                 if job.get("name") == "Profile Bridge Windows regression"]
    require(len(producers) == 1, "physical runtime producer job ambiguous")
    job = producers[0]
    require(job.get("conclusion") == "success"
            and job.get("run_id") == source["id"]
            and job.get("run_attempt") == source["run_attempt"]
            and job.get("head_sha") == candidate,
            "runtime producer run/attempt/source mismatch")
    required = {
        "Bind exact physically tested runtime archive and manifest SHA",
        "Verify physical PR runtime uploads match exact tested SHA",
        "Upload SHA-bound physical runtime proof receipt",
        "Verify uploaded physical runtime proof receipt digest",
    }
    steps = [(step.get("name"), step.get("status"), step.get("conclusion"))
             for step in job.get("steps", [])]
    require(len({name for name, _, _ in steps}) == len(steps),
            "duplicate physical runtime proof steps")
    require(all((name, "completed", "success") in steps for name in required),
            "physical runtime byte receipt was not completed")

    inventory = api(f"{root}/actions/runs/{source['id']}/artifacts?per_page=100")
    require(isinstance(inventory.get("artifacts"), list)
            and type(inventory.get("total_count")) is int
            and inventory["total_count"] <= 100
            and inventory["total_count"] == len(inventory["artifacts"]),
            "incomplete physical runtime artifact inventory")
    # actions/upload-artifact archive:false publishes the raw *file basename*
    # as the REST artifact name, regardless of a customized YAML name.
    # Exact producer/run/HEAD/attempt/expiry/SHA binding below supplies
    # authority; never rely on a logical artifact name ignored by the API.
    names = {
        "proof": "camoufox-runtime-proof.json",
        "archive": "runtime-bundle.tar.gz",
        "manifest": "runtime-manifest.json",
    }
    selected: dict[str, dict[str, Any]] = {}
    for component, name in names.items():
        found = [a for a in inventory["artifacts"] if a.get("name") == name]
        require(len(found) <= 1, f"ambiguous runtime {component} artifact")
        if not found:
            continue
        a = found[0]
        workflow = a.get("workflow_run") or {}
        digest = a.get("digest")
        require(type(a.get("id")) is int and a["id"] > 0
                and type(a.get("size_in_bytes")) is int and a["size_in_bytes"] > 0
                and isinstance(digest, str) and DIGEST.fullmatch(digest) is not None
                and a.get("expired") is False
                and github_time(a.get("expires_at")) > datetime.now(timezone.utc)
                and workflow.get("id") == source["id"]
                and workflow.get("head_sha") == candidate
                and workflow.get("head_branch") == source["head_branch"]
                and github_time(job.get("started_at")) <=
                github_time(a.get("created_at")) <=
                github_time(job.get("completed_at")) <= merged_at,
                f"runtime {component} artifact unbound from physical producer")
        selected[component] = {
            "id": a["id"], "name": name, "size": a["size_in_bytes"],
            "sha256": DIGEST.fullmatch(digest).group(1),
        }
    require("proof" in selected, "runtime physical byte receipt absent")
    require(len({a["id"] for a in selected.values()}) == len(selected),
            "runtime physical artifact IDs overlap")
    return {
        "candidate": candidate, "source_run_id": source["id"],
        "source_attempt": source["run_attempt"],
        "expected_release_id": expected_release_id,
    }, selected


def validate_physical_runtime_receipt(
    receipt: Any, reference: dict[str, Any], expected_release_id: str,
) -> None:
    require(isinstance(receipt, dict) and set(receipt) == RUNTIME_RECEIPT_FIELDS
            and receipt.get("schema_version") == 1
            and receipt.get("kind") == "CAMOUFOX_WINDOWS_PHYSICAL_RUNTIME_RECEIPT"
            and receipt.get("source_commit_sha") == reference["candidate"]
            and receipt.get("run_id") == reference["source_run_id"]
            and receipt.get("run_attempt") == reference["source_attempt"]
            and receipt.get("release_id") == expected_release_id
            and receipt.get("origin") in ("published", "built")
            and all(isinstance(receipt.get(key), str)
                    and re.fullmatch(r"[0-9a-f]{64}", receipt[key])
                    for key in ("archive_sha256", "manifest_sha256"))
            and all(type(receipt.get(key)) is int and receipt[key] > 0
                    for key in ("archive_size_bytes", "manifest_size_bytes"))
            and receipt["archive_size_bytes"] < 2 * 1024 * 1024 * 1024,
            "unbound or malformed runtime physical receipt")


def validate_physical_published_assets(
    release: Any, receipt: dict[str, Any], expected_release_id: str,
) -> None:
    require(isinstance(release, dict)
            and release.get("tag_name") == expected_release_id
            and release.get("draft") is False
            and release.get("prerelease") is False,
            "runtime publication identity/flags mismatch")
    assets = release.get("assets")
    require(isinstance(assets, list) and len(assets) == 2,
            "runtime publication asset inventory mismatch")
    for name, sha_field, size_field in (
        ("runtime-bundle.tar.gz", "archive_sha256", "archive_size_bytes"),
        ("runtime-manifest.json", "manifest_sha256", "manifest_size_bytes"),
    ):
        matches = [a for a in assets if isinstance(a, dict) and a.get("name") == name]
        require(len(matches) == 1 and matches[0].get("state") == "uploaded"
                and matches[0].get("digest") == f"sha256:{receipt[sha_field]}"
                and matches[0].get("size") == receipt[size_field],
                f"published runtime bytes differ from physically tested {name}")


def validate_physical_runtime_miss_artifacts(
    receipt: dict[str, Any], selected: dict[str, dict[str, Any]],
) -> None:
    require(receipt["origin"] == "built"
            and set(selected) == {"proof", "archive", "manifest"},
            "runtime publication miss lacks exact PR-tested bytes")
    for kind, sha_field, size_field in (
        ("archive", "archive_sha256", "archive_size_bytes"),
        ("manifest", "manifest_sha256", "manifest_size_bytes"),
    ):
        a = selected[kind]
        require(a["size"] == receipt[size_field]
                and a["sha256"] == receipt[sha_field],
                f"physical runtime {kind} artifact receipt mismatch")


def prepare_physical_runtime_component(
    main_sha: str, expected_release_id: str, publication_state: str,
    destination: Path, api: Callable[[str], Any], token: str,
) -> None:
    """Reuse the published physical bytes, or transfer the precise tested miss.

    Never rebuild the runtime on accepted main: source identity is insufficient
    to prove binary equality for a non-reproducible external browser package.
    """
    require(publication_state in {"reuse", "absent", "resume"},
            "unknown runtime publication state")
    require(not destination.is_symlink() and destination.is_dir(),
            "runtime evidence destination must be a clean directory")
    require(all(path.is_file() and not path.is_symlink()
                and path.name in {"runtime-input-identity.json", "runtime-release-probe.json"}
                for path in destination.iterdir()),
            "runtime proof destination contains unexpected files")
    current = api(f"repos/{REPOSITORY}/git/ref/heads/main")
    require(current.get("ref") == "refs/heads/main"
            and current.get("object", {}).get("sha") == main_sha,
            "runtime publication target is no longer protected main")
    reference, selected = runtime_proof_artifacts(main_sha, api, expected_release_id)
    with tempfile.TemporaryDirectory(prefix="camoufox-physical-custody-") as temporary:
        root = Path(temporary)
        raw = root / selected["proof"]["name"]
        download_raw_artifact(selected["proof"], raw, token)
        require(raw.stat().st_size == selected["proof"]["size"]
                and hashlib.sha256(raw.read_bytes()).hexdigest() == selected["proof"]["sha256"],
                "physical runtime receipt bytes invalid")
        try:
            receipt = json.loads(raw.read_text(encoding="utf-8-sig"))
        except (ValueError, UnicodeError) as error:
            raise Unconfirmed(f"physical runtime receipt JSON invalid: {error}") from error
        validate_physical_runtime_receipt(receipt, reference, expected_release_id)
        if publication_state == "reuse":
            release = api(f"repos/{REPOSITORY}/releases/tags/{expected_release_id}")
            validate_physical_published_assets(release, receipt, expected_release_id)
        else:
            validate_physical_runtime_miss_artifacts(receipt, selected)
            for kind, output_name, sha_field, size_field in (
                ("archive", "runtime-bundle.tar.gz", "archive_sha256", "archive_size_bytes"),
                ("manifest", "runtime-manifest.json", "manifest_sha256", "manifest_size_bytes"),
            ):
                a = selected[kind]
                target = destination / output_name
                require(not target.exists(), "runtime miss artifact destination exists")
                download_raw_artifact(a, target, token)
                require(target.stat().st_size == receipt[size_field]
                        and sha256_file_streamed(target) == receipt[sha_field],
                        f"PR-tested runtime {kind} bytes changed in main")
        (destination / "runtime-physical-proof.json").write_text(
            json.dumps(receipt, sort_keys=True, indent=2) + "\n",
            encoding="utf-8", newline="\n",
        )
    print(f"Exact physically tested runtime: main={main_sha} "
          f"PR_HEAD={reference['candidate']} run={reference['source_run_id']} "
          f"attempt={reference['source_attempt']} sha256={receipt['archive_sha256']} "
          f"publication={publication_state} origin={receipt['origin']}")


def sha256_file_streamed(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def current_windows_runner_identity() -> dict[str, str]:
    """Bind a physical test to a specific hosted Windows image and compiler."""
    image_os = os.environ.get("ImageOS", "")
    image_version = os.environ.get("ImageVersion", "")
    require(os.environ.get("RUNNER_OS") == "Windows"
            and bool(image_os) and bool(image_version)
            and len(image_os) <= 128 and len(image_version) <= 128,
            "Windows runner image identity unavailable")
    rustc = subprocess.check_output(["rustc", "-Vv"], text=True).strip()
    require("host: x86_64-pc-windows-msvc" in rustc and len(rustc) <= 2048,
            "native Windows Rust toolchain identity is unavailable")
    return {"image_os": image_os, "image_version": image_version,
            "rustc_vv": rustc}


def write_full_runner_receipt(gate: str, output: Path) -> None:
    require(gate in RUNNER_RECEIPT, "unknown physical Windows receipt owner")
    head = os.environ.get("PR_HEAD_SHA", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    attempt = os.environ.get("GITHUB_RUN_ATTEMPT", "")
    require(os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
            and os.environ.get("GITHUB_REPOSITORY") == REPOSITORY
            and bool(SHA.fullmatch(head)) and run_id.isdecimal()
            and attempt.isdecimal() and int(run_id) > 0 and int(attempt) > 0,
            "FULL native runner receipt is not bound to an exact PR attempt")
    require(subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() == head,
            "FULL native runner source SHA drift")
    require(not output.exists() and not output.is_symlink(),
            "runner receipt output must be new")
    receipt = {
        "schema_version": 1,
        "kind": "FULL_NATIVE_WINDOWS_RUNNER_IDENTITY",
        "gate": gate,
        "source_sha": head,
        "run_id": int(run_id),
        "run_attempt": int(attempt),
        "job_name": RUNNER_RECEIPT[gate][1],
        **current_windows_runner_identity(),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, sort_keys=True) + "\n", encoding="utf-8")


def select_full_runner_receipt(
    prior_main: str, gate: str, api: Callable[[str], Any],
    *, as_of: datetime | None = None,
) -> dict[str, Any]:
    """Select a unique raw receipt bound to the *executed* FULL Windows job."""
    require(gate in RUNNER_RECEIPT, "unknown native runner receipt gate")
    proof_gate = "local-profile" if gate == "local-profile-k2" else gate
    candidate = prove(prior_main, proof_gate, api)
    root = f"repos/{REPOSITORY}"
    runs = api(f"{root}/actions/runs?head_sha={candidate}&event=pull_request&per_page=100")
    require(isinstance(runs.get("workflow_runs"), list), "native run inventory missing")
    actual = [run for run in runs["workflow_runs"]
              if run.get("name") == WORKFLOW[proof_gate]]
    require(bool(actual), "native FULL source run missing")
    source_run = max(actual, key=lambda run: run["id"])
    jobs = api(f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100")
    require(isinstance(jobs.get("jobs"), list)
            and type(jobs.get("total_count")) is int
            and jobs["total_count"] <= 100,
            "native Windows job inventory incomplete")
    target_name = RUNNER_RECEIPT[gate][1]
    producers = [job for job in jobs["jobs"] if job.get("name") == target_name]
    require(len(producers) == 1, "native FULL Windows producer missing/ambiguous")
    producer = producers[0]
    require(producer.get("run_id") == source_run["id"]
            and producer.get("run_attempt") == source_run["run_attempt"]
            and producer.get("head_sha") == candidate
            and producer.get("conclusion") == "success",
            "native FULL producer attempt or source drift")
    inventory = api(f"{root}/actions/runs/{source_run['id']}/artifacts?per_page=100")
    require(isinstance(inventory.get("artifacts"), list)
            and type(inventory.get("total_count")) is int
            and inventory["total_count"] == len(inventory["artifacts"])
            and inventory["total_count"] <= 100,
            "native runner artifact inventory incomplete")
    filename = RUNNER_RECEIPT[gate][0]
    matches = [item for item in inventory["artifacts"] if item.get("name") == filename]
    require(len(matches) == 1, "native runner exact receipt absent/ambiguous")
    artifact = matches[0]
    digest = artifact.get("digest")
    created = github_time(artifact.get("created_at"))
    expires = github_time(artifact.get("expires_at"))
    now = as_of or datetime.now(timezone.utc)
    workflow = artifact.get("workflow_run") or {}
    require(type(artifact.get("id")) is int and artifact["id"] > 0
            and type(artifact.get("size_in_bytes")) is int
            and 1 <= artifact["size_in_bytes"] <= 4096
            and isinstance(digest, str) and DIGEST.fullmatch(digest) is not None
            and artifact.get("expired") is False and expires > now
            and created >= now - timedelta(days=30)
            and workflow.get("id") == source_run["id"]
            and workflow.get("head_sha") == candidate
            and workflow.get("head_branch") == source_run["head_branch"]
            and github_time(producer.get("started_at")) <= created
            <= github_time(producer.get("completed_at")),
            "native runner receipt age/attempt/origin/digest mismatch")
    return {
        "id": artifact["id"], "name": filename,
        "sha256": DIGEST.fullmatch(digest).group(1),
        "size": artifact["size_in_bytes"],
        "source_sha": candidate,
        "run_id": source_run["id"],
        "run_attempt": source_run["run_attempt"],
        "job_name": target_name,
        "gate": gate,
    }


def require_full_runner_identity(receipt: Any, artifact: dict[str, Any],
                                 identity: dict[str, str]) -> None:
    require(isinstance(receipt, dict)
            and receipt.get("schema_version") == 1
            and receipt.get("kind") == "FULL_NATIVE_WINDOWS_RUNNER_IDENTITY"
            and receipt.get("source_sha") == artifact["source_sha"]
            and receipt.get("run_id") == artifact["run_id"]
            and receipt.get("run_attempt") == artifact["run_attempt"]
            and receipt.get("gate") == artifact["gate"]
            and receipt.get("job_name") == artifact["job_name"],
            "native runner receipt not bound to FULL producer")
    require(all(receipt.get(field) == identity[field] for field in identity),
            "Windows image or compiler changed since FULL native physical proof")


def verify_full_runner_receipt(artifact: dict[str, Any], token: str) -> None:
    """No stale-image shortcut: compare today's physical runner and compiler."""
    with tempfile.TemporaryDirectory(prefix="native-runner-proof-") as temporary:
        directory = Path(temporary)
        download_raw_artifact(artifact, directory / artifact["name"], token)
        source = verify_downloaded_raw(directory, "native-runner", artifact)
        receipt = json.loads(source.read_text(encoding="utf-8"))
        require_full_runner_identity(receipt, artifact, current_windows_runner_identity())


def native_runner_receipt_scopes(pr_native_proof: bool,
                                 pr_native_bridge: bool) -> tuple[str, ...]:
    """A Linux PR probe selects raw receipts; Windows consumers verify images."""
    if pr_native_proof:
        return ()
    if pr_native_bridge:
        return ("quality",)
    return ("local-profile", "local-profile-k2")


def prepare_release_component(main_sha: str, component: str, api: Callable[[str], Any]) -> None:
    """Prove all PR artifacts; download exact native-equivalent compiler bytes.

    For native-unaffected frontend PRs, Bridge bytes originate from an earlier
    FULL-tested PR whose complete native input closure matches this PR; they
    are never represented as newly compiled from this PR's unrelated UI tree.
    No PR review-candidate manifest or package is copied into main's release.
    The existing main-SHA packagers must recreate and verify final provenance.
    """
    receipt = prove_release_artifacts(main_sha, api)
    checkout = Path(__file__).resolve().parents[1]
    require(subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout,
                                    text=True).strip() == main_sha, "main checkout identity drift")
    with tempfile.TemporaryDirectory(prefix="accepted-main-pr-binaries-") as temporary:
        scratch = Path(temporary)
        downloaded = {}
        requested = ("cloud", "resolver") if component == "cloud" else ("bridge",)
        require(component in ("cloud", "bridge"), "unknown release component")
        for kind in requested:
            artifact = receipt["artifacts"][kind]
            destination = scratch / kind
            destination.mkdir()
            download_raw_artifact(artifact, destination / artifact["name"],
                                  os.environ["GITHUB_TOKEN"])
            downloaded[kind] = verify_downloaded_raw(destination, kind, artifact)
            print(f"Verified PR binary origin: main={main_sha} "
                  f"PR_HEAD={receipt['candidate_sha']} run={receipt['quality_run_id']} "
                  f"attempt={receipt['quality_run_attempt']} component={kind} "
                  f"artifact_id={artifact['id']} sha256={artifact['sha256']}")

        if component == "cloud":
            sys.path.insert(0, str(checkout / "scripts"))
            cloud = runpy.run_path(str(checkout / "scripts/cloudflare-release.py"))
            resolver = runpy.run_path(str(checkout / "scripts/mailbox-secret-resolver-release.py"))
            candidate = receipt["candidate_sha"]
            # The exact checked-out main tree equals the PR tree, so using the
            # original strict verifier with check_git=False still validates
            # all source, migration, source-authority and immutable byte hashes.
            cloud_manifest = cloud["verify_archive"](
                checkout, downloaded["cloud"], expected_source_sha=candidate,
                expected_repository=REPOSITORY, expected_authority="review-candidate",
                check_git=False,
            )
            require(cloud_manifest["source"]["event"] == "pull_request",
                    "PR cloud producer was not a review candidate")
            resolver_manifest = resolver["verify_archive"](checkout, downloaded["resolver"], candidate)
            require(resolver_manifest["source_commit_sha"] == candidate,
                    "PR resolver candidate source drift")
            extracted = scratch / "extracted"
            extracted.mkdir()
            # The original archive readers reject links, traversal and
            # duplicate paths before any binary moves into the main checkout.
            with tarfile.open(downloaded["cloud"], "r:") as tar:
                for member in cloud["safe_archive_members"](tar):
                    target = extracted / member.name
                    if member.isdir():
                        target.mkdir(parents=True, exist_ok=True)
                    else:
                        target.parent.mkdir(parents=True, exist_ok=True)
                        with tar.extractfile(member) as source, target.open("wb") as output:
                            shutil.copyfileobj(source, output)
            resolver_extracted = resolver["safe_extract"](downloaded["resolver"], extracted)
            cloud_root = extracted / "cloudflare-release" / cloud_manifest["release_id"]
            require(cloud_root.is_dir() and not cloud_root.is_symlink(), "PR cloud tree missing")
            copies = (
                (cloud_root / "frontend", "frontend/dist"),
                (cloud_root / "worker", "apps/control-plane-worker/build"),
                (resolver_extracted / "worker", "apps/mailbox-secret-resolver-worker/build"),
            )
            for source, relative in copies:
                target = checkout / relative
                require(not target.exists(), f"unexpected prior compiled output: {relative}")
            created: list[Path] = []
            try:
                for source, relative in copies:
                    target = checkout / relative
                    created.append(target)
                    shutil.copytree(source, target, symlinks=False)
            except Exception:
                for target in created:
                    shutil.rmtree(target, ignore_errors=True)
                raise
        else:
            bridge = downloaded["bridge"]
            with bridge.open("rb") as binary:
                require(bridge.stat().st_size > 100_000 and binary.read(2) == b"MZ",
                        "PR bridge EXE shape drift")
            destination = checkout / "target/release/profile-bridge.exe"
            require(not destination.exists(), "unexpected prior Bridge executable")
            destination.parent.mkdir(parents=True, exist_ok=True)
            try:
                shutil.copyfile(bridge, destination)
            except Exception:
                destination.unlink(missing_ok=True)
                raise
    print(f"Reused verified PR-built {component} bytes from Quality run "
          f"{receipt['quality_run_id']}/attempt {receipt['quality_run_attempt']}; "
          "accepted-main packages will be recreated under main SHA")


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

    The current accepted main PR has already passed exact-tree/single-CI-run
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


def native_unrelated(paths: list[str]) -> bool:
    """Only isolated frontend source/public bytes can reuse native proofs.

    This intentionally excludes all docs, generated contracts, package/tooling,
    workflows, and source paths dynamically inspected by Rust tests. The
    existing advisory classifier supplies path-grammar validation only;
    this owner separately verifies live main, exact SHA and FULL proof history.
    """
    if not paths:
        return True
    valid_repo_path = runpy.run_path(
        str(Path(__file__).with_name("classify-ci-impact.py"))
    )["valid_repo_path"]
    return all(valid_repo_path(path) and path.startswith("frontend/src/")
               and path.endswith((".ts", ".tsx", ".css"))
               for path in paths)


def github_first_parent(base_sha: str, api: Callable[[str], Any]) -> Iterable[str]:
    """Lazy bounded ancestry: a recent FULL proof costs only one GitHub read."""
    require(bool(SHA.fullmatch(base_sha)), "invalid main ancestor")
    root = f"repos/{REPOSITORY}"
    current = base_sha
    for _ in range(16):
        commit = api(f"{root}/commits/{current}")
        require(commit.get("sha") == current, "GitHub ancestor commit identity drift")
        parents = commit.get("parents")
        require(isinstance(parents, list) and len(parents) <= 2,
                "invalid first-parent GitHub history")
        parent = None
        if parents:
            parent = parents[0].get("sha")
            require(isinstance(parent, str) and SHA.fullmatch(parent) is not None,
                    "invalid GitHub first-parent SHA")
        yield current
        if not parents:
            break
        current = parent


def github_changed_paths(old_sha: str, new_sha: str,
                         api: Callable[[str], Any]) -> list[str]:
    """Accept only an untruncated GitHub compare with proven ancestry.

    The API exposes at most 300 files; >=300, renames, or unknown statuses
    force a full physical replay. GitHub is also independently consulted on
    the exact source and prior full job attempt, not merely a diff label.
    """
    require(bool(SHA.fullmatch(old_sha) and SHA.fullmatch(new_sha)),
            "invalid GitHub compare SHA")
    comparison = api(f"repos/{REPOSITORY}/compare/{old_sha}...{new_sha}")
    require(comparison.get("base_commit", {}).get("sha") == old_sha
            and comparison.get("merge_base_commit", {}).get("sha") == old_sha,
            "GitHub compare ancestry mismatch")
    require(comparison.get("status") in ("ahead", "identical")
            and type(comparison.get("ahead_by")) is int
            and 0 <= comparison["ahead_by"] <= 250,
            "GitHub compare is not a bounded exact descendant")
    commits = comparison.get("commits")
    require(isinstance(commits, list)
            and len(commits) == comparison["ahead_by"]
            and (commits[-1].get("sha") == new_sha if commits else new_sha == old_sha),
            "GitHub compare is truncated or lacks exact head commit")
    files = comparison.get("files")
    require(isinstance(files, list) and len(files) < 300,
            "incomplete GitHub changed-file inventory")
    paths = []
    for item in files:
        require(isinstance(item, dict)
                and item.get("status") in ("added", "modified", "removed", "changed", "copied"),
                "ambiguous GitHub changed-file status")
        filename = item.get("filename")
        require(isinstance(filename, str), "missing GitHub changed path")
        paths.append(filename)
    require(comparison["status"] != "identical" or not paths,
            "identical GitHub compare contains changes")
    return paths


def prove_open_camoufox_light_pr(
    api: Callable[[str], Any], *, graph_api_only: bool = False,
) -> tuple[str, str]:
    """Require current protected base and only source-closure-unrelated PR edits.

    The runtime byte proof is then resolved against this base, not against an
    arbitrary archived commit or a PR's own metadata. This entrypoint must
    fail *before* a required green LIGHT Camoufox context can be emitted.
    """
    require(os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
            and os.environ.get("GITHUB_REPOSITORY") == REPOSITORY,
            "Camoufox LIGHT custody only permitted on canonical pull_request")
    head = os.environ.get("PR_HEAD_SHA", "")
    base = os.environ.get("PR_BASE_SHA", "")
    number = os.environ.get("PR_NUMBER", "")
    require(bool(SHA.fullmatch(head) and SHA.fullmatch(base))
            and number.isdecimal() and int(number) > 0,
            "invalid Camoufox PR exact source/base/number")
    if not graph_api_only:
        require(subprocess.check_output(["git", "rev-parse", "HEAD"],
                                        text=True).strip() == head,
                "Camoufox LIGHT PR checkout changed after event")
    root = f"repos/{REPOSITORY}"
    current_main = api(f"{root}/git/ref/heads/main")
    require(current_main.get("ref") == "refs/heads/main"
            and current_main.get("object", {}).get("sha") == base,
            "Camoufox LIGHT PR base is not current protected main")
    pr = api(f"{root}/pulls/{int(number)}")
    require(pr.get("state") == "open" and pr.get("merged_at") is None
            and pr.get("head", {}).get("sha") == head
            and pr.get("base", {}).get("sha") == base
            and pr.get("base", {}).get("ref") == "main"
            and pr.get("head", {}).get("repo", {}).get("full_name") == REPOSITORY
            and pr.get("base", {}).get("repo", {}).get("full_name") == REPOSITORY,
            "Camoufox LIGHT PR source/base/fork authority mismatch")
    changes = github_changed_paths(base, head, api)
    require(bool(changes) and camoufox_unrelated(changes),
            "Camoufox LIGHT PR changed physical source inputs or diff is incomplete")
    package = runpy.run_path(str(Path(__file__).with_name("camouhost-runtime-package.py")))
    release_id = package["runtime_input_identity"]()["release_id"]
    require(re.fullmatch(r"runtime-bundle-v3-sha256-[0-9a-f]{64}", release_id)
            is not None, "current runtime source input identity unavailable")
    return base, release_id


def prove_unchanged_prior_native(
    source_sha: str,
    base_sha: str,
    api: Callable[[str], Any],
    ancestors: Callable[[str], Iterable[str]],
    changed: Callable[[str, str], list[str]],
) -> str:
    """Find one previously completed FULL Quality *and* Local proof source.

    No transitive proof chaining: every accepted ancestor is verified against
    both genuinely executed physical Windows jobs, one successful 35-job CI run,
    exact original PR tree/run/attempt and native-independent tree diff.
    """
    require(bool(SHA.fullmatch(source_sha) and SHA.fullmatch(base_sha)),
            "invalid native source/base")
    count = 0
    first_rejection: str | None = None
    # Quality and Local are two views of the same *completed* exact-head CI
    # run. Re-read every immutable GitHub object at most once in this proof;
    # neither view may silently observe a different 35-job response.
    observations: dict[str, Any] = {}

    def one_snapshot(path: str) -> Any:
        if path not in observations:
            observations[path] = api(path)
        return observations[path]

    for prior in ancestors(base_sha):
        count += 1
        require(count <= 16 and (count != 1 or prior == base_sha),
                "invalid bounded native first-parent history")
        require(bool(SHA.fullmatch(prior)), "invalid prior native full-proof SHA")
        if not native_unrelated(changed(prior, source_sha)):
            break
        try:
            prove(prior, "quality", one_snapshot)
            prove(prior, "local-profile", one_snapshot)
            return prior
        except GitHubProofReadUnavailable:
            # Trying older source identities cannot repair missing GitHub
            # authority or transport; preserve the infrastructure outcome.
            raise
        except (Unconfirmed, KeyError, ValueError, TypeError,
                urllib.error.URLError, OSError) as error:
            if first_rejection is None:
                first_rejection = (
                    f"prior={prior} {type(error).__name__}: {str(error)[:300]}"
                )
            continue
    raise Unconfirmed(
        "no unchanged FULL Quality and Local Windows physical proof"
        + (f"; first rejected source: {first_rejection}" if first_rejection
           else "; no eligible unchanged ancestor")
    )


def prove_open_native_pr(api: Callable[[str], Any],
                         *, graph_api_only: bool,
                         as_of: datetime | None = None) -> tuple[str, dict[str, Any]]:
    """Only same-repository current-main PRs with an inherited raw EXE qualify.

    This is deliberately re-run by the Windows consumers before allowing a
    physical-test skip. A transient API, ancestry or artifact failure cannot
    accidentally result in success; the caller executes full proof instead.
    """
    require(os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
            and os.environ.get("GITHUB_REPOSITORY") == REPOSITORY,
            "native proof reuse only permitted on canonical pull_request")
    head = os.environ.get("PR_HEAD_SHA", "")
    base = os.environ.get("PR_BASE_SHA", "")
    number = os.environ.get("PR_NUMBER", "")
    require(bool(SHA.fullmatch(head) and SHA.fullmatch(base))
            and number.isdecimal() and int(number) > 0,
            "invalid exact PR source/base/number")
    require(subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() == head,
            "native PR checkout changed after event")
    root = f"repos/{REPOSITORY}"
    current_main = api(f"{root}/git/ref/heads/main")
    require(current_main.get("ref") == "refs/heads/main"
            and current_main.get("object", {}).get("sha") == base,
            "PR base is not current protected main")
    pr = api(f"{root}/pulls/{int(number)}")
    require(pr.get("state") == "open" and pr.get("merged_at") is None
            and pr.get("head", {}).get("sha") == head
            and pr.get("base", {}).get("sha") == base
            and pr.get("base", {}).get("ref") == "main"
            and pr.get("head", {}).get("repo", {}).get("full_name") == REPOSITORY
            and pr.get("base", {}).get("repo", {}).get("full_name") == REPOSITORY,
            "PR head/base/repository changed or foreign fork")
    if graph_api_only:
        own_changes = github_changed_paths(base, head, api)
        ancestry = lambda sha: github_first_parent(sha, api)
        changed = lambda old, new: github_changed_paths(old, new, api)
    else:
        require(subprocess.run(["git", "cat-file", "-e", f"{base}^{{commit}}"],
                               capture_output=True, check=False).returncode == 0,
                "PR checkout lacks exact protected-base history")
        own_changes = git_changed_paths(base, head)
        ancestry = git_first_parent
        changed = git_changed_paths
    require(bool(own_changes) and native_unrelated(own_changes),
            "PR changes native inputs or cannot prove tracked diff")
    prior = prove_unchanged_prior_native(
        head, base, api, ancestry, changed,
    )
    artifacts = prove_release_artifacts(prior, api)
    # First fast PR after this change must find prior FULL Windows image
    # receipts; older proof logs without a machine-readable image identity
    # cannot claim safe reuse on a potentially updated hosted runner.
    select_full_runner_receipt(prior, "quality", api, as_of=as_of)
    select_full_runner_receipt(prior, "local-profile", api, as_of=as_of)
    select_full_runner_receipt(prior, "local-profile-k2", api, as_of=as_of)
    require(artifacts.get("candidate_sha") != head
            and artifacts.get("artifacts", {}).get("bridge", {}).get("sha256"),
            "no independent inherited Bridge executable receipt")
    return prior, artifacts


D1_REQUIRED_SCENARIOS = frozenset({
    "ledger_noncanonical", "supported_ahead_recovery_window",
    "expand_old_new_runtime_rollout", "contract_rejection_admission",
    "contract_exact_successor", "transaction_observation_plan_target_drift",
    "authorization_freshness_and_scope_drift", "executor_checkout_identity_drift",
    "stale_or_unknown_target_fence", "runner_interruption_requires_recovery",
    "canonical_prefix_matrix_catalog", "canonical_prefix_matrix_resolver",
    "clean_bootstrap_and_replay_catalog", "clean_bootstrap_and_replay_resolver",
    "existing_catalog_like_reparse", "current_empty_baseline",
    "future_migration_append_catalog", "future_migration_append_resolver",
    "partial_prefix_failure", "repair_and_safe_continuation",
})


def validate_d1_fault_report(report: Any) -> None:
    """Independently check every historical physical D1 scenario receipt."""
    require(isinstance(report, dict) and report.get("schema_version") == 1
            and report.get("status") == "PASS"
            and report.get("mode") == "LOCAL_TEST_ONLY"
            and report.get("provider_mutation") is False
            and report.get("production_mutation") is False
            and report.get("remote_provider_credentials_present") is False,
            "D1 report metadata or local-only safety violated")
    scenarios = report.get("scenarios")
    require(isinstance(scenarios, list)
            and report.get("scenario_count") == len(D1_REQUIRED_SCENARIOS)
            and len(scenarios) == len(D1_REQUIRED_SCENARIOS),
            "D1 report lost one or more full scenarios")
    observed: list[str] = []
    for item in scenarios:
        require(isinstance(item, dict) and item.get("status") == "PASS"
                and item.get("provider_mutation") is False
                and item.get("production_mutation") is False
                and isinstance(item.get("owner"), str) and bool(item["owner"])
                and isinstance(item.get("proof_kind"), str) and bool(item["proof_kind"]),
                "D1 scenario has missing/unsafe typed proof")
        observed.append(item.get("scenario_id"))
    require(set(observed) == D1_REQUIRED_SCENARIOS and len(set(observed)) == len(observed),
            "D1 scenario IDs are missing, duplicated or substituted")


def select_d1_full_artifact(source_run: dict[str, Any], candidate: str,
                            api: Callable[[str], Any],
                            *, as_of: datetime | None = None) -> dict[str, Any]:
    """Bind one unexpired report ZIP to the exact successful FULL PR job attempt."""
    run_id, attempt = source_run.get("id"), source_run.get("run_attempt")
    require(type(run_id) is int and type(attempt) is int and attempt >= 1,
            "invalid FULL D1 run identity")
    root = f"repos/{REPOSITORY}"
    inventory = api(f"{root}/actions/runs/{run_id}/artifacts?per_page=100")
    require(isinstance(inventory.get("artifacts"), list)
            and isinstance(inventory.get("total_count"), int)
            and inventory["total_count"] == len(inventory["artifacts"])
            and inventory["total_count"] <= 100,
            "incomplete D1 full-report artifact inventory")
    name = f"d1-fault-recovery-{run_id}-{attempt}"
    matches = [a for a in inventory["artifacts"] if a.get("name") == name]
    require(len(matches) == 1, "missing/ambiguous exact FULL D1 report artifact")
    item = matches[0]
    provenance = item.get("workflow_run", {})
    require(type(item.get("id")) is int and item["id"] > 0
            and type(item.get("size_in_bytes")) is int
            and 100 <= item["size_in_bytes"] <= 128 * 1024
            and bool(DIGEST.fullmatch(item.get("digest") or ""))
            and item.get("expired") is False
            and isinstance(provenance, dict)
            and provenance.get("id") == run_id
            and provenance.get("head_sha") == candidate
            and provenance.get("head_branch") == source_run.get("head_branch"),
            "D1 report digest/source/run/attempt identity unconfirmed")
    now = as_of or datetime.now(timezone.utc)
    created, expires = github_time(item.get("created_at")), github_time(item.get("expires_at"))
    require(now - timedelta(days=30) <= created <= now + timedelta(minutes=5)
            and expires > now and expires > created,
            "D1 report artifact stale, future-dated or expired")
    return item


def verify_d1_full_artifact(item: dict[str, Any], token: str) -> None:
    """Read actual compressed ZIP bytes; green workflow alone is insufficient."""
    with tempfile.TemporaryDirectory(prefix="prior-full-d1-report-") as temporary:
        archive_path = Path(temporary) / "d1-report.zip"
        download_raw_artifact({"id": item["id"], "size": item["size_in_bytes"]},
                              archive_path, token)
        require(hashlib.sha256(archive_path.read_bytes()).hexdigest()
                == item["digest"].removeprefix("sha256:"),
                "D1 downloaded ZIP SHA-256 differs from GitHub digest")
        with zipfile.ZipFile(archive_path) as archive:
            members = archive.infolist()
            require(len(members) == 1 and members[0].filename == "fault-recovery-report.json"
                    and not members[0].is_dir()
                    and 100 <= members[0].file_size <= 512 * 1024,
                    "D1 physical report ZIP has unexpected or oversized content")
            report = json.loads(archive.read(members[0]).decode("utf-8"))
    validate_d1_fault_report(report)


def prove_accepted_d1_full(main_sha: str, api: Callable[[str], Any], token: str,
                           *, as_of: datetime | None = None) -> tuple[str, dict[str, Any]]:
    """Reuse one pre-merge FULL PR D1 witness, including the original ZIP bytes.

    The exact 35-job CI run and both real D1 jobs/critical steps must have run.
    A rerun or uploaded artifact after merge is not evidence for the accepted
    transaction, even if its eventual GitHub conclusion is green.
    """
    require(bool(token), "D1 FULL receipt requires read-only GitHub authority")
    accepted = api(f"repos/{REPOSITORY}/commits/{main_sha}")
    require(isinstance(accepted.get("parents"), list) and len(accepted["parents"]) == 1,
            "D1 main reuse requires the protected single-parent squash merge")
    source_run: dict[str, Any] = {}
    candidate = prove(main_sha, "d1", api, proven_run=source_run)
    merged = api(f"repos/{REPOSITORY}/commits/{main_sha}/pulls")
    require(isinstance(merged, list) and len(merged) == 1,
            "D1 accepted PR identity ambiguous")
    merged_at = github_time(merged[0].get("merged_at"))
    started = github_time(source_run.get("created_at"))
    completed = github_time(source_run.get("updated_at"))
    require(started <= completed <= merged_at,
            "D1 FULL workflow did not complete before protected merge")
    job_inventory = api(f"repos/{REPOSITORY}/actions/runs/{source_run['id']}/jobs?per_page=100")
    require(isinstance(job_inventory.get("jobs"), list)
            and type(job_inventory.get("total_count")) is int
            and job_inventory["total_count"] == len(job_inventory["jobs"])
            and job_inventory["total_count"] <= 100,
            "D1 FULL producer job inventory incomplete")
    producer_start: datetime | None = None
    producer_end: datetime | None = None
    for name in CRITICAL["d1"]:
        matches = [job for job in job_inventory["jobs"] if job.get("name") == name]
        require(len(matches) == 1, f"D1 FULL producer missing or ambiguous: {name}")
        job = matches[0]
        job_started = github_time(job.get("started_at"))
        job_completed = github_time(job.get("completed_at"))
        require(job.get("run_id") == source_run["id"]
                and job.get("run_attempt") == source_run["run_attempt"]
                and job.get("head_sha") == candidate
                and started <= job_started <= job_completed <= completed,
                f"D1 FULL {name} did not execute in accepted pre-merge attempt")
        if name == "Native opsctl and pinned Wrangler D1 proof":
            producer_start, producer_end = job_started, job_completed
    receipt = select_d1_full_artifact(source_run, candidate, api, as_of=as_of)
    created = github_time(receipt.get("created_at"))
    require(producer_start is not None and producer_end is not None
            and producer_start <= created <= producer_end <= completed <= merged_at,
            "D1 FULL ZIP appeared outside its accepted pre-merge Linux producer")
    verify_d1_full_artifact(receipt, token)
    return candidate, receipt


def prove_open_d1_light_pr(api: Callable[[str], Any], token: str) -> str:
    """Frontend-only PR may inherit only a truly executed prior FULL D1 report.

    The existing native source/ancestor owner proves strict frontend-only,
    exact current base, no fork, complete diff and past FULL Windows custody.
    The D1 owner additionally checks its own independent FULL job and ZIP.
    """
    prior, _ = prove_open_native_pr(api, graph_api_only=True)
    source_run: dict[str, Any] = {}
    candidate = prove(prior, "d1", api, proven_run=source_run)
    receipt = select_d1_full_artifact(source_run, candidate, api)
    verify_d1_full_artifact(receipt, token)
    return prior


def prove_accepted_native_reuse(main_sha: str, gate: str,
                                api: Callable[[str], Any]) -> tuple[str, str | None]:
    """Accept current FULL physical proof or independently validate inheritance.

    The main checkout may be shallow; the GitHub commit/compare API provides
    exact bounded first-parent and changed-byte provenance without a second
    local compilation or a separate acceptance registry.
    """
    try:
        return prove(main_sha, gate, api), None
    except (Unconfirmed, KeyError, ValueError, TypeError,
            urllib.error.URLError, OSError):
        require(gate in NATIVE_REUSED_CRITICAL, "native reuse gate unsupported")
    candidate = prove(main_sha, gate, api, native_reused=True)
    root = f"repos/{REPOSITORY}"
    accepted = api(f"{root}/commits/{main_sha}")
    require(accepted.get("sha") == main_sha, "native accepted main SHA changed")
    parents = accepted.get("parents")
    require(isinstance(parents, list) and bool(parents)
            and isinstance(parents[0].get("sha"), str)
            and SHA.fullmatch(parents[0]["sha"]) is not None,
            "invalid accepted-main base for native inheritance")
    prior = prove_unchanged_prior_native(
        main_sha, parents[0]["sha"], api,
        lambda sha: github_first_parent(sha, api),
        lambda old, new: github_changed_paths(old, new, api),
    )
    return candidate, prior


def fixture(gate: str) -> tuple[dict[str, Any], Callable[[str], Any]]:
    main, base, head, tree = ("a" * 40, "b" * 40, "c" * 40, "d" * 40)
    root = f"repos/{REPOSITORY}"
    paths = required_workflow_paths()
    pr = {
        "state": "closed", "merged_at": "2026-10-31T01:00:00Z",
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
         "head_repository": {"full_name": REPOSITORY}, "run_attempt": 1,
         "created_at": "2026-10-31T00:00:00Z", "updated_at": "2026-10-31T00:10:00Z"}
        for i, name in enumerate(sorted(REQUIRED))
    ]
    gate_run = next(run for run in runs if run["name"] == WORKFLOW[gate])
    critical_jobs = {name: steps for group in CRITICAL.values()
                     for name, steps in group.items()}
    all_jobs = {name: critical_jobs.get(name, ()) for name in expected_pr_jobs()}
    all_jobs.update(critical_jobs)
    jobs = [
        {"name": name, "status": "completed",
         "conclusion": expected_pr_jobs()[name],
         "run_id": gate_run["id"], "run_attempt": gate_run["run_attempt"],
         "head_sha": head, "workflow_name": WORKFLOW[gate],
         "started_at": "2026-10-31T00:00:00Z", "completed_at": "2026-10-31T00:10:00Z",
         "steps": [{"name": step, "status": "completed", "conclusion": "success"}
                   for step in steps]}
        for name, steps in sorted(all_jobs.items())
    ]
    artifact_inventory = []
    if gate == "quality":
        names = {
            "cloud": f"cloudflare-v1-sha256-{'1'*64}.tar",
            "resolver": f"mailbox-secret-resolver-v1-sha256-{'2'*64}.tar",
            "bridge": "profile-bridge.exe",
        }
        artifact_inventory = [
            {"id": index + 100, "name": name, "size_in_bytes": 2048,
             "digest": f"sha256:{str(index + 1)*64}", "expired": False,
             "created_at": "2026-10-31T00:05:00Z",
             "expires_at": "2030-01-01T00:00:00Z",
             "workflow_run": {
                 "id": gate_run["id"], "head_sha": head,
                 "head_branch": "codex/exact-pr-proof",
             }}
            for index, name in enumerate(names.values())
        ]
    for index, receipt_gate in enumerate(
        ("local-profile", "local-profile-k2") if gate == "local-profile"
        else ((gate,) if gate in RUNNER_RECEIPT else ())
    ):
        artifact_inventory.append({
            "id": 999 + index, "name": RUNNER_RECEIPT[receipt_gate][0],
            "size_in_bytes": 256, "digest": "sha256:" + "a" * 64,
            "expired": False,
            "created_at": "2026-10-31T00:05:00Z",
            "expires_at": "2030-01-01T00:00:00Z",
            "workflow_run": {
                "id": gate_run["id"], "head_sha": head,
                "head_branch": "codex/exact-pr-proof",
            },
        })
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
        f"{root}/actions/runs/{gate_run['id']}/artifacts?per_page=100":
            {"total_count": len(artifact_inventory), "artifacts": artifact_inventory},
    }
    return pages, lambda path: pages[path]


def normalized_local_windows_graph(source: str) -> str:
    """Prove namespaced CI dependencies before applying the exact graph audit.

    The original independent Core/K2 graph keeps its semantic contract; the
    only intentional change is prefixing each source-local ID under one CI.
    """
    for old in ("accepted-pr-proof", "local-profile-windows-core",
                "local-profile-windows-k2", "local-profile-windows"):
        new = f"local-profile-{old}"
        require(re.search(rf"(?m)^  {re.escape(new)}:[ \t]*$", source)
                is not None, f"missing namespaced Local Profile job: {new}")
        source = source.replace(new, old)
    return source


def assert_local_windows_graph(source: str) -> None:
    """Keep the existing protected status downstream of BOTH real Windows jobs.

    No new status context or branch-protection mutation is needed. Source
    regression must fail the already-required admission self-test before a
    misleading green Local Profile Windows aggregate can be accepted.
    """
    headers = list(re.finditer(r"(?m)^  ([a-z][a-z0-9-]*):[ \t]*$", source))
    blocks = {item.group(1): source[item.end():headers[index + 1].start()
                                     if index + 1 < len(headers) else len(source)].lstrip("\n")
              for index, item in enumerate(headers)}
    core = blocks.get("local-profile-windows-core", "")
    k2 = blocks.get("local-profile-windows-k2", "")
    aggregate = blocks.get("local-profile-windows", "")
    physical = "Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits"
    admission = ("always() && (github.event_name == 'pull_request' || "
                 "needs.accepted-pr-proof.result != 'success' || "
                 "needs.accepted-pr-proof.outputs.reuse != 'true')")

    def step_body(job: str, name: str) -> str:
        headers = list(re.finditer(r"(?m)^      - name: (.+)$", job))
        matches = [index for index, item in enumerate(headers) if item.group(1) == name]
        require(len(matches) == 1, f"required Windows step missing/duplicate: {name}")
        index = matches[0]
        body = job[headers[index].end():headers[index + 1].start()
                   if index + 1 < len(headers) else len(job)]
        return "\n".join(line for line in body.strip("\n").splitlines()
                         if not line.lstrip().startswith("#")).strip("\n")

    def actual_job_header(job: str) -> str:
        # Consolidated CI gives every source job an explicit permissions map.
        # That map precedes the source-original needs/if/name/runner fields.
        require(job.startswith("    permissions:\n"),
                "CI job omitted its independent permissions policy")
        return re.sub(r"\A    permissions:\n(?:      [a-z-]+: [a-z]+\n)+",
                      "", job, count=1)

    for job, name in ((core, "Local Profile Windows Core"),
                      (k2, "Local Profile Windows K2")):
        required_header = (
            "    needs: accepted-pr-proof\n"
            f"    if: {admission}\n"
            f"    name: {name}\n"
            "    runs-on: windows-latest\n"
            "    outputs:\n"
            "      proof_mode: ${{ steps.native-prior.outcome == 'success' && 'INHERITED' || 'FULL' }}\n"
            "    steps:\n"
        )
        require(actual_job_header(job).startswith(required_header)
                and not re.search(r"(?m)^    continue-on-error:", job),
                f"local Windows {name} is not its exact-source independent runner")
        checkout = step_body(job, "Checkout")
        require(checkout == (
            "        uses: actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a\n"
            "        with:\n"
            "          ref: ${{ github.event.pull_request.head.sha || github.sha }}\n"
            "          persist-credentials: false"
        ), f"local Windows {name} checkout not bound to exact source")
        source = step_body(job, "Verify exact source checkout")
        require(source == (
            "        env:\n"
            "          EXPECTED_SOURCE_SHA: ${{ github.event.pull_request.head.sha || github.sha }}\n"
            '        run: python -c "import os, subprocess; actual = '
            "subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(); "
            "expected = os.environ['EXPECTED_SOURCE_SHA']; "
            'assert actual == expected, f\'expected {expected}, got {actual}\'"'
        ), f"local Windows {name} source equality bypassed")
        certificate = step_body(job, "Certify unchanged exact prior FULL native physical proof")
        require(certificate == (
            "        if: github.event_name == 'pull_request' && needs.accepted-pr-proof.outputs.native_unaffected == 'true'\n"
            "        id: native-prior\n"
            "        continue-on-error: true\n"
            "        env:\n"
            "          GITHUB_TOKEN: ${{ github.token }}\n"
            "          PR_HEAD_SHA: ${{ github.event.pull_request.head.sha }}\n"
            "          PR_BASE_SHA: ${{ github.event.pull_request.base.sha }}\n"
            "          PR_NUMBER: ${{ github.event.pull_request.number }}\n"
            "        run: python scripts/accepted-main-pr-reuse.py --pr-native-certify"
        ) and len(re.findall(r"(?m)^\s+continue-on-error:\s*true\s*$", job)) == 1,
                f"local Windows {name} prior-certifier forged or physical failure ignored")
    require(f"      - name: {physical}\n" not in core
            and step_body(k2, physical) == (
                "        if: steps.native-prior.outcome != 'success'\n"
                "        run: python scripts/test-k2-large-native-streaming.py"
            ),
            "K2 must execute physically on its own Windows runner on FULL")
    require("      - name: Write FULL Local Windows runner identity receipt\n" in core
            and "      - name: Write FULL K2 Windows runner identity receipt\n" in k2,
            "Core and K2 must each witness their own physical Windows runner")
    for label, exact_test in (
        ("Prove Windows lost-Commit K1 exact sparse WAL successor",
         "shipping_generation_successor_control::tests::"
         "fresh_lone_intent_first_seals_then_commit_and_next_claim_finishes_active_exact"),
        ("Prove Windows release CMS verifier",
         "production_verifier_accepts_only_exact_pinned_application_signature"),
    ):
        guarded = step_body(core, label)
        require(all(fragment in guarded for fragment in (
            "        if: steps.native-prior.outcome != 'success'",
            "        shell: pwsh",
            "          $ErrorActionPreference = 'Stop'",
            "          $PSNativeCommandUseErrorActionPreference = $true",
            exact_test,
            "--exact",
            "--list 2>&1",
            "Count -ne 1",
            "if ($LASTEXITCODE -ne 0",
            "1 passed; 0 failed; 0 ignored;",
        )) and guarded.count("if ($LASTEXITCODE -ne 0") == 2,
                f"Windows {label} lacks exact one-match/one-pass proof")
    required_aggregate = (
        "    needs: [accepted-pr-proof, local-profile-windows-core, local-profile-windows-k2]\n"
        f"    if: {admission}\n"
        "    name: Local Profile Windows\n"
        "    runs-on: ubuntu-latest\n"
        "    steps:\n"
        "      - name: Require both exact-head Windows proof jobs\n"
        "        env:\n"
        "          ADMISSION: ${{ needs.accepted-pr-proof.result }}\n"
        "          CORE: ${{ needs.local-profile-windows-core.result }}\n"
        "          K2: ${{ needs.local-profile-windows-k2.result }}\n"
        "          CORE_MODE: ${{ needs.local-profile-windows-core.outputs.proof_mode }}\n"
        "          K2_MODE: ${{ needs.local-profile-windows-k2.outputs.proof_mode }}\n"
        "        run: |\n"
        '          test "$ADMISSION" = success\n'
        '          test "$CORE" = success\n'
        '          test "$K2" = success\n'
        '          test "$CORE_MODE" = "$K2_MODE"\n'
        '          test "$CORE_MODE" = FULL || test "$CORE_MODE" = INHERITED'
    )
    aggregate = actual_job_header(aggregate)
    aggregate = "\n".join(line for line in aggregate.strip("\n").splitlines()
                          if not line.lstrip().startswith("#")).rstrip("\n")
    require(aggregate == required_aggregate,
            "protected Local Profile Windows context can bypass a physical job")


def self_test() -> None:
    # A transient GitHub jobs endpoint timeout may not discard valid FULL
    # evidence. Credential/authorization and malformed replies still fail once.
    from unittest import mock

    evidence_path = f"repos/{REPOSITORY}/actions/runs/42/jobs?per_page=100"
    with (mock.patch.object(urllib.request, "urlopen", side_effect=[
          urllib.error.URLError("temporary timeout"), io.BytesIO(b'{"total_count":35}')
          ]) as transport, mock.patch.object(time, "sleep") as pause):
        require(api_reader("test-token")(evidence_path) == {"total_count": 35}
                and transport.call_count == 2 and pause.call_count == 1,
                "transient exact GitHub proof response was not recovered")
    with (mock.patch.object(urllib.request, "urlopen",
          side_effect=urllib.error.URLError("persistent timeout")) as transport,
          mock.patch.object(time, "sleep") as pause):
        try:
            api_reader("test-token")(evidence_path)
        except GitHubProofReadUnavailable as error:
            require("exhausted after 3 attempts" in str(error)
                    and evidence_path in str(error)
                    and transport.call_count == 3 and pause.call_count == 2,
                    "exhausted GitHub evidence read lost its exact typed cause")
        else:
            raise AssertionError("repeated GitHub timeouts accepted evidence")
    forbidden = urllib.error.HTTPError(
        f"https://api.github.com/{evidence_path}", 403,
        "Resource not accessible by integration", None, None,
    )
    with (mock.patch.object(urllib.request, "urlopen",
          side_effect=forbidden) as transport,
          mock.patch.object(time, "sleep") as pause):
        try:
            api_reader("test-token")(evidence_path)
        except GitHubProofReadUnavailable as error:
            require("unauthorized (HTTP 403)" in str(error)
                    and transport.call_count == 1
                    and pause.call_count == 0,
                    "GitHub authorization failure was incorrectly retried")
        else:
            raise AssertionError("GitHub authorization failure accepted evidence")
    with (mock.patch.object(urllib.request, "urlopen",
          return_value=io.BytesIO(b'{invalid')) as transport,
          mock.patch.object(time, "sleep") as pause):
        try:
            api_reader("test-token")(evidence_path)
        except ValueError:
            require(transport.call_count == 1 and pause.call_count == 0,
                    "malformed GitHub proof body was incorrectly retried")
        else:
            raise AssertionError("malformed GitHub evidence was accepted")
    try:
        prove_unchanged_prior_native(
            "a" * 40, "a" * 40, lambda _key: {},
            lambda base: [base], lambda _old, _new: [],
        )
    except Unconfirmed as error:
        require("first rejected source" in str(error)
                and "main SHA drift" in str(error),
                "failed ancestor proof lost its exact semantic reason")
    else:
        raise AssertionError("unproven FULL ancestor was accepted")
    unavailable_reads = 0

    def unavailable_api(_path: str) -> Any:
        nonlocal unavailable_reads
        unavailable_reads += 1
        raise GitHubProofReadUnavailable("GitHub request deadline")

    try:
        prove_unchanged_prior_native(
            "a" * 40, "a" * 40, unavailable_api,
            lambda base: [base, "b" * 40], lambda _old, _new: [],
        )
    except GitHubProofReadUnavailable as error:
        require(str(error) == "GitHub request deadline" and unavailable_reads == 1,
                "read outage was converted to a missing semantic FULL proof")
    else:
        raise AssertionError("GitHub outage initiated ancestor fallback")
    shared_pages, _ = fixture("quality")
    shared_reads: dict[str, int] = {}

    def shared_api(path: str) -> Any:
        shared_reads[path] = shared_reads.get(path, 0) + 1
        return shared_pages[path]

    require(prove_unchanged_prior_native(
        "a" * 40, "a" * 40, shared_api,
        lambda base: [base], lambda _old, _new: [],
    ) == "a" * 40, "valid unchanged FULL Quality/Local source not found")
    require(shared_reads.get(next(key for key in shared_pages
                                  if key.endswith("/jobs?per_page=100"))) == 1,
            "Quality and Local re-fetched the same immutable 35-job proof")

    assert native_runner_receipt_scopes(True, False) == ()
    assert native_runner_receipt_scopes(False, True) == ("quality",)
    assert native_runner_receipt_scopes(False, False) == (
        "local-profile", "local-profile-k2",
    )
    graph = normalized_local_windows_graph((Path(__file__).resolve().parent.parent
             / ".github/workflows/ci.yml").read_text(encoding="utf-8"))
    assert_local_windows_graph(graph)
    for unsafe, repaired in (
        ("local-profile-windows-core, local-profile-windows-k2", "local-profile-windows-core"),
        ('test "$K2" = success', 'test "$K2" = skipped'),
        ('test "$CORE_MODE" = "$K2_MODE"', 'true # ignore mixed modes'),
        ("    name: Local Profile Windows\n", "    name: Local Profile Windows Unprotected\n"),
        ("        run: python scripts/test-k2-large-native-streaming.py\n", "        run: echo K2 skipped\n"),
        ("          K2: ${{ needs.local-profile-windows-k2.result }}",
         "          K2: success"),
        ("          CORE: ${{ needs.local-profile-windows-core.result }}",
         "          CORE: success"),
        ("          ADMISSION: ${{ needs.accepted-pr-proof.result }}",
         "          ADMISSION: success"),
        ("          K2_MODE: ${{ needs.local-profile-windows-k2.outputs.proof_mode }}",
         "          K2_MODE: FULL"),
        ("      proof_mode: ${{ steps.native-prior.outcome == 'success' && 'INHERITED' || 'FULL' }}",
         "      proof_mode: FULL"),
        ("        run: |\n          test \"$ADMISSION\" = success",
         "        run: |\n          exit 0\n          test \"$ADMISSION\" = success"),
        ("    needs: [accepted-pr-proof, local-profile-windows-core, local-profile-windows-k2]\n"
         "    if: always() && ",
         "    needs: [accepted-pr-proof, local-profile-windows-core, local-profile-windows-k2]\n"
         "    if: always() && false # "),
        ("      - name: Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits\n"
         "        if: steps.native-prior.outcome != 'success'\n",
         "      - name: Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits\n"
         "        if: steps.native-prior.outcome != 'success'\n"
         "        continue-on-error: true\n"),
        ("Count -ne 1", "Count -lt 0"),
        ("1 passed; 0 failed; 0 ignored;", "0 passed; 0 failed; 0 ignored;"),
        ("--list 2>&1", "--list # skipped"),
        ("shipping_generation_successor_control::tests::fresh_lone_intent_first_seals_then_commit_and_next_claim_finishes_active_exact",
         "wrong_k1_no_test"),
        ("          ref: ${{ github.event.pull_request.head.sha || github.sha }}",
         "          ref: main"),
        ("    runs-on: windows-latest\n    outputs:",
         "    runs-on: windows-latest\n    continue-on-error: true\n    outputs:"),
        ("        run: python scripts/accepted-main-pr-reuse.py --pr-native-certify\n",
         "        run: exit 0\n"),
        ("      - name: Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits\n"
         "        if: steps.native-prior.outcome != 'success'\n",
         "      - name: Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits\n"
         "        if: steps.native-prior.outcome != 'success'\n"
         "        continue-on-error: true\n"),
    ):
        require(unsafe in graph, f"negative aggregate/K2 fixture not present: {unsafe}")
        mutated = graph.replace(unsafe, repaired, 1)
        if (unsafe.startswith("          ref: ")
                or unsafe in ("Count -ne 1", "1 passed; 0 failed; 0 ignored;",
                              "--list 2>&1",
                              "shipping_generation_successor_control::tests::"
                              "fresh_lone_intent_first_seals_then_commit_and_next_claim_finishes_active_exact")):
            prefix, local = graph.split("  local-profile-windows-core:\n", 1)
            mutated = (prefix + "  local-profile-windows-core:\n"
                       + local.replace(unsafe, repaired, 1))
        try:
            assert_local_windows_graph(mutated)
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"unsafe protected K2 graph accepted: {unsafe}")

    d1_pages, d1_api = fixture("d1")
    d1_candidate = "c" * 40
    d1_run: dict[str, Any] = {}
    require(prove("a" * 40, "d1", d1_api, proven_run=d1_run) == d1_candidate
            and d1_run["name"] == WORKFLOW["d1"],
            "real FULL D1 Windows/Linux proof was not recognized")
    d1_jobs_key = next(key for key in d1_pages if key.endswith("/jobs?per_page=100"))
    for fault in ("skipped-harness", "skipped-k2", "missing-windows", "duplicate-job"):
        case = json.loads(json.dumps(d1_pages))
        jobs = case[d1_jobs_key]["jobs"]
        if fault in ("skipped-harness", "skipped-k2"):
            target = ("Execute reusable D1 fault and recovery harness"
                      if fault == "skipped-harness"
                      else "Prove K2 608 MiB D1 successor-v2 migration and append-only recovery")
            next(step for job in jobs for step in job["steps"]
                 if step["name"] == target)["conclusion"] = "skipped"
        elif fault == "missing-windows":
            jobs.pop(next(i for i, job in enumerate(jobs)
                          if job["name"] == "Native opsctl D1 CLI portability on Windows"))
        else:
            jobs.append(dict(jobs[0]))
        try:
            prove("a" * 40, "d1", lambda key: case[key])
        except (Unconfirmed, KeyError):
            pass
        else:
            raise AssertionError(f"unsafe historical D1 full proof accepted: {fault}")

    valid_d1_report = {
        "schema_version": 1, "status": "PASS", "mode": "LOCAL_TEST_ONLY",
        "provider_mutation": False, "production_mutation": False,
        "remote_provider_credentials_present": False,
        "scenario_count": len(D1_REQUIRED_SCENARIOS),
        "scenarios": [{"scenario_id": name, "status": "PASS", "owner": "D1",
                       "proof_kind": "LOCAL_WAL", "provider_mutation": False,
                       "production_mutation": False}
                      for name in sorted(D1_REQUIRED_SCENARIOS)],
    }
    validate_d1_fault_report(valid_d1_report)
    for fault in ("missing", "duplicate", "unknown", "failed", "unsafe", "remote", "untyped"):
        case = json.loads(json.dumps(valid_d1_report))
        if fault == "missing":
            case["scenarios"].pop()
        elif fault == "duplicate":
            case["scenarios"][0] = dict(case["scenarios"][1])
        elif fault == "unknown":
            case["scenarios"][0]["scenario_id"] = "fake_green_D1"
        elif fault == "failed":
            case["scenarios"][0]["status"] = "SKIPPED"
        elif fault == "unsafe":
            case["scenarios"][0]["provider_mutation"] = True
        elif fault == "remote":
            case["remote_provider_credentials_present"] = True
        else:
            case["scenarios"][0]["proof_kind"] = ""
        try:
            validate_d1_fault_report(case)
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"unsafe compressed D1 receipt accepted: {fault}")

    fixture_now = datetime(2026, 11, 24, tzinfo=timezone.utc)
    full_artifact = {
        "id": 42, "name": f"d1-fault-recovery-{d1_run['id']}-1",
        "size_in_bytes": 512, "digest": "sha256:" + "a" * 64,
        "expired": False, "created_at": "2026-10-31T00:05:00Z",
        "expires_at": "2026-12-24T00:00:00Z",
        "workflow_run": {"id": d1_run["id"], "head_sha": d1_candidate,
                         "head_branch": d1_run["head_branch"]},
    }
    artifacts_key = f"repos/{REPOSITORY}/actions/runs/{d1_run['id']}/artifacts?per_page=100"
    d1_pages[artifacts_key] = {"total_count": 1, "artifacts": [full_artifact]}
    require(select_d1_full_artifact(d1_run, d1_candidate, d1_api,
                                    as_of=fixture_now)["id"] == 42,
            "exact previous D1 ZIP receipt not recognized")
    for fault in ("missing", "duplicate", "wrong-run", "wrong-head", "wrong-attempt",
                  "bad-digest", "expired", "stale", "wrong-branch"):
        case = json.loads(json.dumps(d1_pages))
        items = case[artifacts_key]["artifacts"]
        if fault == "missing":
            items.clear()
        elif fault == "duplicate":
            items.append(dict(items[0]))
        elif fault == "wrong-run":
            items[0]["workflow_run"]["id"] += 1
        elif fault == "wrong-head":
            items[0]["workflow_run"]["head_sha"] = "f" * 40
        elif fault == "wrong-attempt":
            items[0]["name"] += "-2"
        elif fault == "bad-digest":
            items[0]["digest"] = "missing"
        elif fault == "expired":
            items[0]["expired"] = True
        elif fault == "stale":
            items[0]["created_at"] = "2026-09-01T00:00:00Z"
        else:
            items[0]["workflow_run"]["head_branch"] = "foreign"
        case[artifacts_key]["total_count"] = len(items)
        try:
            select_d1_full_artifact(d1_run, d1_candidate, lambda key: case[key],
                                    as_of=fixture_now)
        except (Unconfirmed, KeyError):
            pass
        else:
            raise AssertionError(f"unsafe GitHub D1 artifact admitted: {fault}")

    with tempfile.TemporaryDirectory(prefix="d1-full-proof-self-test-") as temporary:
        path = Path(temporary) / "report.zip"
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as output:
            output.writestr("fault-recovery-report.json", json.dumps(valid_d1_report))
        original_zip_bytes = path.read_bytes()
        physical = dict(full_artifact)
        physical["size_in_bytes"] = path.stat().st_size
        physical["digest"] = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
        prior_download = globals()["download_raw_artifact"]
        try:
            globals()["download_raw_artifact"] = lambda _artifact, target, _token: shutil.copyfile(path, target)
            verify_d1_full_artifact(physical, "offline-self-test-token")
            # The accepted-main bypass is valid only for the COMPOSED exact
            # proof: squash merge, current FULL Linux+Windows, pre-merge Linux
            # producer ZIP and its original SHA-256/20 real local scenarios.
            d1_main_pages = json.loads(json.dumps(d1_pages))
            d1_main_pages[artifacts_key]["artifacts"] = [dict(physical)]
            candidate, receipt = prove_accepted_d1_full(
                "a" * 40, lambda key: d1_main_pages[key],
                "offline-self-test-token", as_of=fixture_now,
            )
            require(candidate == d1_candidate and receipt["id"] == physical["id"],
                    "accepted-main D1 FULL ZIP custody rejected valid pre-merge proof")
            runs_key = next(key for key in d1_main_pages
                            if "/actions/runs?head_sha=" in key)
            for fault in (
                "non-squash", "light-pr", "missing-windows", "post-merge-run",
                "post-merge-linux", "post-merge-windows", "zip-after-linux",
                "post-merge-zip", "missing-zip", "wrong-zip-digest",
            ):
                case = json.loads(json.dumps(d1_main_pages))
                jobs = case[d1_jobs_key]["jobs"]
                artifact = case[artifacts_key]["artifacts"][0]
                if fault == "non-squash":
                    case[f"repos/{REPOSITORY}/commits/{'a' * 40}"]["parents"].append(
                        {"sha": d1_candidate})
                elif fault == "light-pr":
                    next(step for job in jobs for step in job["steps"]
                         if step["name"] == "Execute reusable D1 fault and recovery harness")[
                             "conclusion"] = "skipped"
                elif fault == "missing-windows":
                    jobs[:] = [job for job in jobs
                               if job["name"] != "Native opsctl D1 CLI portability on Windows"]
                    case[d1_jobs_key]["total_count"] = len(jobs)
                elif fault == "post-merge-run":
                    next(run for run in case[runs_key]["workflow_runs"]
                         if run["name"] == WORKFLOW["d1"])["updated_at"] = "2026-10-31T02:00:00Z"
                elif fault in ("post-merge-linux", "post-merge-windows"):
                    producer = ("Native opsctl D1 CLI portability on Windows"
                                if fault == "post-merge-windows"
                                else "Native opsctl and pinned Wrangler D1 proof")
                    next(job for job in jobs if job["name"] == producer)[
                        "completed_at"] = "2026-10-31T02:00:00Z"
                elif fault == "zip-after-linux":
                    artifact["created_at"] = "2026-10-31T00:11:00Z"
                elif fault == "post-merge-zip":
                    artifact["created_at"] = "2026-11-01T00:00:00Z"
                elif fault == "missing-zip":
                    case[artifacts_key]["artifacts"].clear()
                    case[artifacts_key]["total_count"] = 0
                else:
                    artifact["digest"] = "sha256:" + "f" * 64
                try:
                    prove_accepted_d1_full(
                        "a" * 40, lambda key: case[key],
                        "offline-self-test-token", as_of=fixture_now,
                    )
                except (Unconfirmed, KeyError):
                    pass
                else:
                    raise AssertionError(f"unsafe accepted-main D1 bypass: {fault}")
            try:
                prove_accepted_d1_full("a" * 40, lambda key: d1_main_pages[key], "",
                                       as_of=fixture_now)
            except Unconfirmed:
                pass
            else:
                raise AssertionError("D1 main bypass accepted absent API authority")
            try:
                prove_accepted_d1_full(
                    "a" * 40,
                    lambda _key: (_ for _ in ()).throw(urllib.error.URLError("offline")),
                    "offline-self-test-token", as_of=fixture_now,
                )
            except urllib.error.URLError:
                pass  # The public --gate caller must emit reuse=false and FULL.
            else:
                raise AssertionError("D1 main accepted unavailable GitHub API")
            for fault in ("wrong-sha", "bad-zip", "unsafe-report"):
                case = dict(physical)
                if fault == "wrong-sha":
                    case["digest"] = "sha256:" + "f" * 64
                elif fault == "bad-zip":
                    path.write_bytes(b"not a ZIP")
                    case["size_in_bytes"] = path.stat().st_size
                    case["digest"] = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
                else:
                    with zipfile.ZipFile(path, "w") as output:
                        invalid = dict(valid_d1_report)
                        invalid["status"] = "FAIL"
                        output.writestr("fault-recovery-report.json", json.dumps(invalid))
                    case["size_in_bytes"] = path.stat().st_size
                    case["digest"] = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
                try:
                    verify_d1_full_artifact(case, "offline-self-test-token")
                except (Unconfirmed, zipfile.BadZipFile):
                    pass
                else:
                    raise AssertionError(f"unsafe downloaded D1 report accepted: {fault}")
            corrupt = json.loads(json.dumps(d1_main_pages))
            damaged = corrupt[artifacts_key]["artifacts"][0]
            damaged["size_in_bytes"] = path.stat().st_size
            damaged["digest"] = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
            try:
                prove_accepted_d1_full(
                    "a" * 40, lambda key: corrupt[key],
                    "offline-self-test-token", as_of=fixture_now,
                )
            except Unconfirmed:
                pass
            else:
                raise AssertionError("accepted-main D1 bypass admitted unsafe ZIP report")
            path.write_bytes(original_zip_bytes)
            # Exercise the actual --gate CLI decision, not just its attestor.
            original_args = sys.argv
            original_reader = globals()["api_reader"]
            env_keys = ("GITHUB_EVENT_NAME", "GITHUB_REF", "GITHUB_SHA", "GITHUB_TOKEN")
            original_environment = {key: os.environ.get(key) for key in env_keys}
            try:
                os.environ.update({"GITHUB_EVENT_NAME": "push",
                                   "GITHUB_REF": "refs/heads/main",
                                   "GITHUB_SHA": "a" * 40,
                                   "GITHUB_TOKEN": "offline-self-test-token"})
                output_path = Path(temporary) / "github-output.txt"
                sys.argv = [str(Path(__file__)), "--gate", "d1",
                            "--github-output", str(output_path)]
                for mode, expected in (("valid", "true"), ("missing-token", "false"),
                                       ("api-unavailable", "false"),
                                       ("corrupt-zip", "false")):
                    output_path.unlink(missing_ok=True)
                    active_pages = d1_main_pages
                    if mode == "missing-token":
                        os.environ.pop("GITHUB_TOKEN", None)
                    else:
                        os.environ["GITHUB_TOKEN"] = "offline-self-test-token"
                    if mode == "corrupt-zip":
                        path.write_bytes(b"not-a-real-ZIP" * 100)
                        active_pages = json.loads(json.dumps(d1_main_pages))
                        item = active_pages[artifacts_key]["artifacts"][0]
                        item["size_in_bytes"] = path.stat().st_size
                        item["digest"] = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
                    if mode == "api-unavailable":
                        globals()["api_reader"] = lambda _token: (
                            lambda _key: (_ for _ in ()).throw(urllib.error.URLError("offline")))
                    else:
                        globals()["api_reader"] = lambda _token: (
                            lambda key: active_pages[key])
                    with contextlib.redirect_stdout(io.StringIO()):
                        status = main()
                    require(status == 0 and output_path.read_text(encoding="utf-8")
                            == f"reuse={expected}\n",
                            f"D1 main gate did not fail FULL safely for {mode}")
            finally:
                sys.argv = original_args
                globals()["api_reader"] = original_reader
                for key, value in original_environment.items():
                    if value is None:
                        os.environ.pop(key, None)
                    else:
                        os.environ[key] = value
        finally:
            globals()["download_raw_artifact"] = prior_download

        report_path = Path(temporary) / "local-D1-report.json"
        report_path.write_text(json.dumps(valid_d1_report), encoding="utf-8")
        cli = [sys.executable, str(Path(__file__).resolve()),
               "--verify-d1-report", str(report_path)]
        require(subprocess.run(cli, capture_output=True, text=True, check=False).returncode == 0,
                "physical PR D1 report CLI rejected exact 20 local scenarios")
        unsafe_report = dict(valid_d1_report)
        unsafe_report["remote_provider_credentials_present"] = True
        report_path.write_text(json.dumps(unsafe_report), encoding="utf-8")
        require(subprocess.run(cli, capture_output=True, text=True, check=False).returncode != 0,
                "physical PR D1 report CLI accepted remote/provider escape")

    for unsafe in ("http://raw.blob.core.windows.net/example",
                   "https://blob.core.windows.net.evil.example/raw",
                   "https://evil.example/raw", "https://user:pass@host.githubusercontent.com/raw",
                   "file:///etc/passwd", "https://api.github.com/artifacts/raw"):
        try:
            signed_artifact_url(unsafe)
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"untrusted signed artifact origin accepted: {unsafe}")
    require(signed_artifact_url("https://results.blob.core.windows.net/x?signature=1")
            == "https://results.blob.core.windows.net/x?signature=1",
            "valid signed raw artifact origin rejected")
    with tempfile.TemporaryDirectory(prefix="accepted-main-artifact-fixture-") as temporary:
        base = Path(temporary)
        target = base / "profile-bridge.exe"
        target.write_bytes(b"MZ" + b"verified-pr-binary")
        identity = {"name": "profile-bridge.exe", "size": target.stat().st_size,
                    "sha256": hashlib.sha256(target.read_bytes()).hexdigest()}
        require(verify_downloaded_raw(base, "bridge", identity) == target,
                "verified PR Bridge binary rejected")
        for fault in ("tamper", "wrong-size", "wrong-digest", "extra-file", "symlink"):
            case = dict(identity)
            if fault == "tamper":
                target.write_bytes(b"MZ" + b"unverified-binary")
            elif fault == "wrong-size":
                case["size"] += 1
            elif fault == "wrong-digest":
                case["sha256"] = "f" * 64
            elif fault == "extra-file":
                (base / "other.bin").write_bytes(b"extra")
            else:
                target.unlink()
                target.symlink_to(base / "other.bin")
            try:
                verify_downloaded_raw(base, "bridge", case)
            except Unconfirmed:
                pass
            else:
                raise AssertionError(f"unverified PR binary accepted: {fault}")
            if target.is_symlink():
                target.unlink()
            (base / "other.bin").unlink(missing_ok=True)
            target.write_bytes(b"MZ" + b"verified-pr-binary")
    release_pages, release_read = fixture("quality")
    valid_release = prove_release_artifacts("a" * 40, release_read)
    require(set(valid_release["artifacts"]) == set(RELEASE_ARTIFACTS),
            "valid PR release bytes rejected")
    release_job_path = next(key for key in release_pages if key.endswith("/jobs?per_page=100"))
    release_artifact_path = next(key for key in release_pages if key.endswith("/artifacts?per_page=100"))
    for failure in (
        "duplicate-job", "duplicate-step", "missing-artifact", "duplicate-artifact",
        "wrong-archive-name", "wrong-artifact-head", "wrong-artifact-run", "post-merge-rerun",
        "bad-digest", "missing-size", "expired", "stale-created", "future-created",
    ):
        case = json.loads(json.dumps(release_pages))
        jobs = case[release_job_path]["jobs"]
        inventory = case[release_artifact_path]
        target = inventory["artifacts"][0]
        if failure == "duplicate-job":
            jobs.append(dict(jobs[0]))
            case[release_job_path]["total_count"] += 1
        elif failure == "duplicate-step":
            producer = next(job for job in jobs
                            if job["name"] == "Cloudflare Worker Release Build")
            producer["steps"].append(dict(producer["steps"][0]))
        elif failure == "missing-artifact":
            inventory["artifacts"].pop(0)
            inventory["total_count"] -= 1
        elif failure == "duplicate-artifact":
            inventory["artifacts"].append(dict(target))
            inventory["total_count"] += 1
        elif failure == "wrong-archive-name":
            target["name"] = "unsafe-cloud.tar"
        elif failure == "wrong-artifact-head":
            target["workflow_run"]["head_sha"] = "e" * 40
        elif failure == "wrong-artifact-run":
            target["workflow_run"]["id"] += 1
        elif failure == "post-merge-rerun":
            run_path = next(key for key in case if key.endswith("&event=pull_request&per_page=100"))
            next(run for run in case[run_path]["workflow_runs"] if run["name"] == CI_WORKFLOW)[
                "updated_at"] = "2026-11-01T00:00:00Z"
        elif failure == "bad-digest":
            target["digest"] = "sha256:missing"
        elif failure == "missing-size":
            target["size_in_bytes"] = 0
        elif failure == "expired":
            target["expired"] = True
        elif failure == "stale-created":
            target["created_at"] = "2026-10-30T00:00:00Z"
        elif failure == "future-created":
            target["created_at"] = "2026-11-01T00:00:00Z"
        try:
            prove_release_artifacts("a" * 40, lambda path: case[path])
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"unsafe PR compiled release reuse accepted: {failure}")
    # One successful aggregate status cannot hide a skipped, renamed, stale,
    # duplicated or absent unprotected source job. Every PR-side constituent
    # job is admitted by exact identity, not by only the 22 protected names.
    inventory_pages, _ = fixture("quality")
    inventory_key = next(key for key in inventory_pages if key.endswith("/jobs?per_page=100"))
    for fault in ("unprotected-skipped", "unprotected-missing", "unprotected-failed",
                  "unprotected-stale-attempt", "unprotected-foreign-workflow",
                  "unprotected-renamed", "unprotected-duplicate", "unknown-job",
                  "protected-skipped", "protected-missing", "protected-stale-run",
                  "d1-router-falsely-green", "d1-router-missing", "truncated-jobs"):
        case = json.loads(json.dumps(inventory_pages))
        inventory = case[inventory_key]
        rows = inventory["jobs"]
        unprotected = next(row for row in rows
                           if row["name"] == "Bridge Host Ops Linux Policy")
        protected = next(row for row in rows if row["name"] == "React Operator UI")
        skipped = next(row for row in rows
                       if row["name"] == "Prove exact accepted FULL D1 PR evidence")
        if fault == "unprotected-skipped":
            unprotected["conclusion"] = "skipped"
        elif fault == "unprotected-missing":
            rows.remove(unprotected)
        elif fault == "unprotected-failed":
            unprotected["conclusion"] = "failure"
        elif fault == "unprotected-stale-attempt":
            unprotected["run_attempt"] += 1
        elif fault == "unprotected-foreign-workflow":
            unprotected["workflow_name"] = "Other Workflow"
        elif fault == "unprotected-renamed":
            unprotected["name"] += " unprotected"
        elif fault == "unprotected-duplicate":
            rows.append(dict(unprotected))
        elif fault == "unknown-job":
            rows.append(dict(unprotected, name="Unreviewed CI admission"))
        elif fault == "protected-skipped":
            protected["conclusion"] = "skipped"
        elif fault == "protected-missing":
            rows.remove(protected)
        elif fault == "protected-stale-run":
            protected["run_id"] += 1
        elif fault == "d1-router-falsely-green":
            skipped["conclusion"] = "success"
        elif fault == "d1-router-missing":
            rows.remove(skipped)
        else:
            inventory["total_count"] = 101
        inventory["total_count"] = (101 if fault == "truncated-jobs" else len(rows))
        try:
            prove("a" * 40, "quality", lambda key: case[key])
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"consolidated CI accepted unsafe source job: {fault}")

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
                target = next(job for job in clone[jobpath]["jobs"]
                              if job["name"] in CRITICAL[gate])
                target["steps"][0]["conclusion"] = "skipped"
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
                job = next(job for job in clone[jobpath]["jobs"]
                           if job["name"] in CRITICAL[gate])
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
                    target = next(job for job in clone[suffix]["jobs"]
                                  if job["name"] in CRITICAL[gate])
                    target["steps"][0]["conclusion"] = "skipped"
            try:
                prove("a" * 40, gate, lambda path: clone[path])
            except Unconfirmed:
                pass
            else:
                raise AssertionError(f"{gate}: accepted unsafe proof: {mutation}")
        # Every required physical test must have run on the exact PR attempt.
        # Missing, skipped, failed, or unfinished evidence forces replay.
        for required_job, required_steps in CRITICAL[gate].items():
            jobpath = next(k for k in pages if k.endswith("/jobs?per_page=100"))
            for mutation, skipped_step in [("missing", None)] + [
                (outcome, name)
                for name in required_steps
                for outcome in ("skipped", "failure", "in_progress")
            ]:
                clone = json.loads(json.dumps(pages))
                jobs = clone[jobpath]["jobs"]
                target = next(job for job in jobs if job["name"] == required_job)
                if mutation == "missing":
                    jobs.remove(target)
                    clone[jobpath]["total_count"] -= 1
                else:
                    target_step = next(step for step in target["steps"]
                                       if step["name"] == skipped_step)
                    if mutation == "in_progress":
                        target_step["status"] = mutation
                    else:
                        target_step["conclusion"] = mutation
                try:
                    prove("a" * 40, gate, lambda path: clone[path])
                except Unconfirmed:
                    pass
                else:
                    raise AssertionError(
                        f"{gate}: accepted {mutation} evidence: {required_job}/{skipped_step}"
                    )
    # Fail-closed historical reuse: current PR has all 22 protected checks
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
    prior_run = f"{root}/actions/runs?head_sha={prior_head}&event=pull_request&per_page=100"
    prior_runs = json.loads(json.dumps(now[current_run]))
    for run in prior_runs["workflow_runs"]:
        run["head_sha"] = prior_head
        run["id"] += 100
    prior_camoufox = next(
        run for run in prior_runs["workflow_runs"]
        if run["name"] == WORKFLOW["camoufox"]
    )
    for job in old_jobs["jobs"]:
        job["run_id"] = prior_camoufox["id"]
        job["head_sha"] = prior_head
    prior_jobs = f"{root}/actions/runs/{prior_camoufox['id']}/jobs?per_page=100"
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
        [".github/workflows/ci.yml"],
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
    prior_real = next(job for job in now[prior_jobs]["jobs"]
                      if job["name"] == "Profile Bridge Windows regression")
    prior_real["steps"][0]["conclusion"] = "skipped"
    try:
        prove_unchanged_prior_camoufox(
            now_sha, base_sha, api, lambda _: [base_sha],
            preserved, camoufox_unrelated,
        )
    except Unconfirmed:
        pass
    else:
        raise AssertionError("skipped prior physical step reused")
    try:
        prove_unchanged_prior_camoufox(
            now_sha, base_sha, api, lambda _: [],
            preserved, camoufox_unrelated,
        )
    except Unconfirmed:
        pass
    else:
        raise AssertionError("missing parent history passed")

    # Native FAST is limited to isolated UI source. Documentation may be an
    # input to dynamically loaded tests; lockfiles/scripts/native Rust are
    # always a full physical run, even if the application already works.
    assert native_unrelated(["frontend/src/app/router.tsx", "frontend/src/styles/root.css"])
    for changed in (
        ["docs/INSTALL.md"], ["README.md"], ["frontend/public/logo.svg"],
        ["frontend/src/app/icon.svg"], ["Cargo.toml"], ["Cargo.lock"],
        ["apps/profile-bridge/src/main.rs"],
        [".github/workflows/ci.yml"],
        ["scripts/accepted-main-pr-reuse.py"],
        ["frontend/src/router.tsx", "crates/contracts/src/lib.rs"],
        ["frontend/src/../app/main.tsx"], ["frontend/src/foo\\bar.ts"],
    ):
        assert not native_unrelated(changed), f"unsafe native source scope accepted: {changed}"

    old_sha, new_sha = "1" * 40, "2" * 40
    valid_compare = {
        "base_commit": {"sha": old_sha},
        "merge_base_commit": {"sha": old_sha},
        "status": "ahead", "ahead_by": 1,
        "commits": [{"sha": new_sha}],
        "files": [{"filename": "frontend/src/main.tsx", "status": "modified"}],
    }
    compare_url = f"repos/{REPOSITORY}/compare/{old_sha}...{new_sha}"
    assert github_changed_paths(old_sha, new_sha,
                                lambda url: {compare_url: valid_compare}[url]) == ["frontend/src/main.tsx"]
    for fault in ("wrong-base", "wrong-merge-base", "wrong-head", "diverged",
                  "truncated-files", "missing-commits", "renamed", "unknown-status",
                  "unsafe-path", "unexpected-identical"):
        case = json.loads(json.dumps(valid_compare))
        if fault == "wrong-base":
            case["base_commit"]["sha"] = "3" * 40
        elif fault == "wrong-merge-base":
            case["merge_base_commit"]["sha"] = "3" * 40
        elif fault == "wrong-head":
            case["commits"][0]["sha"] = "3" * 40
        elif fault == "diverged":
            case["status"] = "diverged"
        elif fault == "truncated-files":
            case["files"] *= 300
        elif fault == "missing-commits":
            case["commits"] = []
        elif fault == "renamed":
            case["files"][0]["status"] = "renamed"
            case["files"][0]["previous_filename"] = "apps/profile-bridge/src/main.rs"
        elif fault == "unknown-status":
            case["files"][0]["status"] = "unsafe"
        elif fault == "unsafe-path":
            case["files"][0]["filename"] = "apps/profile-bridge/src/main.rs"
        else:
            case["status"] = "identical"
        try:
            paths = github_changed_paths(old_sha, new_sha,
                                         lambda url: {compare_url: case}[url])
            require(native_unrelated(paths), "native input changed in exact REST compare")
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"unsafe GitHub compare native proof accepted: {fault}")

    fixture_now = datetime(2026, 11, 24, tzinfo=timezone.utc)
    for gate in RUNNER_RECEIPT:
        pages, read = fixture("local-profile" if gate == "local-profile-k2" else gate)
        receipt = select_full_runner_receipt("a" * 40, gate, read, as_of=fixture_now)
        assert receipt["name"] == RUNNER_RECEIPT[gate][0]
        content = {
            "schema_version": 1, "kind": "FULL_NATIVE_WINDOWS_RUNNER_IDENTITY",
            "source_sha": receipt["source_sha"], "run_id": receipt["run_id"],
            "run_attempt": receipt["run_attempt"], "gate": gate,
            "job_name": receipt["job_name"], "image_os": "win25",
            "image_version": "20261117.1", "rustc_vv": "rustc version (stable)\nhost: x86_64-pc-windows-msvc",
        }
        identity = {field: content[field]
                    for field in ("image_os", "image_version", "rustc_vv")}
        require_full_runner_identity(content, receipt, identity)
        for fault in ("source_sha", "run_id", "run_attempt", "gate", "job_name",
                      "image_os", "image_version", "rustc_vv"):
            broken = dict(content)
            broken[fault] = "foreign" if isinstance(broken[fault], str) else 99
            try:
                require_full_runner_identity(broken, receipt, identity)
            except Unconfirmed:
                pass
            else:
                raise AssertionError(f"untrusted FULL native runner content accepted: {fault}")
        artifact_path = next(key for key in pages if key.endswith("/artifacts?per_page=100"))
        for fault in ("missing", "duplicate", "wrong-run", "wrong-head", "expired",
                      "bad-digest", "oversized", "stale-created", "bad-attempt"):
            case = json.loads(json.dumps(pages))
            inventory = case[artifact_path]
            target = next(item for item in inventory["artifacts"]
                          if item["name"] == RUNNER_RECEIPT[gate][0])
            if fault == "missing":
                inventory["artifacts"].remove(target)
                inventory["total_count"] -= 1
            elif fault == "duplicate":
                inventory["artifacts"].append(dict(target))
                inventory["total_count"] += 1
            elif fault == "wrong-run":
                target["workflow_run"]["id"] += 1
            elif fault == "wrong-head":
                target["workflow_run"]["head_sha"] = "f" * 40
            elif fault == "expired":
                target["expired"] = True
            elif fault == "bad-digest":
                target["digest"] = "sha256:missing"
            elif fault == "oversized":
                target["size_in_bytes"] = 4097
            elif fault == "stale-created":
                target["created_at"] = "2026-01-01T00:05:00Z"
            else:
                jobs_path = next(key for key in case if key.endswith("/jobs?per_page=100"))
                producer = next(job for job in case[jobs_path]["jobs"]
                                if job["name"] == RUNNER_RECEIPT[gate][1])
                producer["run_attempt"] += 1
            try:
                select_full_runner_receipt("a" * 40, gate,
                                           lambda path: case[path], as_of=fixture_now)
            except Unconfirmed:
                pass
            else:
                raise AssertionError(f"untrusted native runner artifact accepted: {gate}/{fault}")

    # Both previous required Windows proof groups must have executed on ONE
    # accepted CI run. A green Quality-only or Local-only proof is insufficient.
    quality_pages, _ = fixture("quality")
    local_pages, _ = fixture("local-profile")
    two_full = json.loads(json.dumps(quality_pages))
    for key, value in local_pages.items():
        if key.endswith("/artifacts?per_page=100"):
            local_receipts = [artifact for artifact in value["artifacts"]
                              if artifact["name"] in
                              {RUNNER_RECEIPT["local-profile"][0],
                               RUNNER_RECEIPT["local-profile-k2"][0]}]
            two_full[key]["artifacts"].extend(local_receipts)
            two_full[key]["total_count"] = len(two_full[key]["artifacts"])
    full_api = lambda key: two_full[key]
    assert prove_unchanged_prior_native(
        "a" * 40, "a" * 40, full_api,
        lambda base: [base],
        lambda old, head: ["frontend/src/app/router.tsx"],
    ) == "a" * 40
    for gate, missing_step in (("quality", "Check and test native crates"),
                               ("local-profile", "Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits")):
        case = json.loads(json.dumps(two_full))
        workflow_name = WORKFLOW[gate]
        original_runs = case[f"repos/{REPOSITORY}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"]["workflow_runs"]
        run_id = next(run["id"] for run in original_runs if run["name"] == workflow_name)
        jobs_path = f"repos/{REPOSITORY}/actions/runs/{run_id}/jobs?per_page=100"
        for job in case[jobs_path]["jobs"]:
            for step in job["steps"]:
                if step["name"] == missing_step:
                    step["conclusion"] = "skipped"
        try:
            prove_unchanged_prior_native(
                "a" * 40, "a" * 40, lambda key: case[key],
                lambda base: [base],
                lambda old, head: ["frontend/src/app/router.tsx"],
            )
        except Unconfirmed:
            pass
        else:
            raise AssertionError(f"prior FULL native proof missing physical {gate}/{missing_step}")

    # Simulate the *next* frontend PR merged after a prior FULL Quality+Local
    # run. Its current Windows jobs have no K1/K2, but must have run all the
    # listed SHA-bound inheritance/installer steps and upload the identical
    # original Bridge bytes. This tests main's shallow-checkout REST proof.
    new_main, new_head, new_tree = "6" * 40, "7" * 40, "8" * 40
    prior_main = "a" * 40
    shared_run_path = f"repos/{REPOSITORY}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"
    current_runs = json.loads(json.dumps(two_full[shared_run_path]))
    for run in current_runs["workflow_runs"]:
        run["id"] += 1000
        run["head_sha"] = new_head
        run["head_branch"] = "codex/native-unaffected-ui"
    new_pages = dict(two_full)
    new_pages[f"repos/{REPOSITORY}/commits/{new_main}"] = {
        "sha": new_main, "parents": [{"sha": prior_main}],
        "commit": {"tree": {"sha": new_tree}},
    }
    new_pages[f"repos/{REPOSITORY}/commits/{new_main}/pulls"] = [{
        "state": "closed", "merged_at": "2026-11-01T01:00:00Z",
        "merge_commit_sha": new_main,
        "base": {"sha": prior_main, "ref": "main", "repo": {"full_name": REPOSITORY}},
        "head": {"sha": new_head, "ref": "codex/native-unaffected-ui",
                 "repo": {"full_name": REPOSITORY}},
    }]
    new_pages[f"repos/{REPOSITORY}/commits/{new_head}"] = {
        "sha": new_head, "commit": {"tree": {"sha": new_tree}},
    }
    new_pages[f"repos/{REPOSITORY}/commits/{'b'*40}"] = {
        "sha": "b" * 40, "parents": [], "commit": {"tree": {"sha": "0" * 40}},
    }
    new_pages[f"repos/{REPOSITORY}/actions/runs?head_sha={new_head}&event=pull_request&per_page=100"] = current_runs
    # All 35 source jobs belong to the same CI run; update provenance once.
    old_id = current_runs["workflow_runs"][0]["id"] - 1000
    new_id = old_id + 1000
    current_job_path = f"repos/{REPOSITORY}/actions/runs/{new_id}/jobs?per_page=100"
    new_pages[current_job_path] = json.loads(json.dumps(
        two_full[f"repos/{REPOSITORY}/actions/runs/{old_id}/jobs?per_page=100"]))
    for job in new_pages[current_job_path]["jobs"]:
        job["run_id"] = new_id
        job["head_sha"] = new_head
    native_compare_url = f"repos/{REPOSITORY}/compare/{prior_main}...{new_main}"
    new_pages[native_compare_url] = {
        "base_commit": {"sha": prior_main},
        "merge_base_commit": {"sha": prior_main},
        "status": "ahead", "ahead_by": 1,
        "commits": [{"sha": new_main}],
        "files": [{"filename": "frontend/src/app/main.tsx", "status": "modified"}],
    }
    for gate in NATIVE_REUSED_CRITICAL:
        name = WORKFLOW[gate]
        run = next(row for row in current_runs["workflow_runs"] if row["name"] == name)
        current_jobs = []
        for job_name, required_steps in NATIVE_REUSED_CRITICAL[gate].items():
            steps = [{"name": label, "status": "completed", "conclusion": "success"}
                     for label in required_steps]
            if gate == "quality" and job_name == "Rust Linux and WASM":
                # Native-reused frontend PR: every Cargo step was truly
                # skipped; the independent prior-FULL certificate ran.
                steps.extend({"name": label, "status": "completed", "conclusion": "skipped"}
                             for label in RUST_LINUX_FULL_STEPS)
                steps.append({"name": RUST_LINUX_LIGHT_CERT,
                              "status": "completed", "conclusion": "success"})
            if job_name == RUNNER_RECEIPT[gate][1] or (
                gate == "local-profile" and job_name == "Local Profile Windows K2"
            ):
                steps.extend({"name": label, "status": "completed", "conclusion": "skipped"}
                             for label in CRITICAL[gate][job_name]
                             if label not in required_steps)
            current_jobs.append({
                "name": job_name, "status": "completed", "conclusion": "success",
                "run_id": run["id"], "run_attempt": 1,
                "head_sha": new_head, "workflow_name": name,
                "started_at": "2026-10-31T00:00:00Z",
                "completed_at": "2026-10-31T00:10:00Z", "steps": steps,
            })
        existing = new_pages[current_job_path]["jobs"]
        replacements = {job["name"]: job for job in current_jobs}
        existing[:] = [replacements.get(job["name"], job) for job in existing]
        if gate == "quality":
            old_inventory = two_full[f"repos/{REPOSITORY}/actions/runs/{run['id'] - 1000}/artifacts?per_page=100"]
            copied_inventory = json.loads(json.dumps(old_inventory))
            for artifact in copied_inventory["artifacts"]:
                artifact["id"] += 1000
                artifact["workflow_run"].update({
                    "id": run["id"], "head_sha": new_head,
                    "head_branch": "codex/native-unaffected-ui",
                })
            new_pages[f"repos/{REPOSITORY}/actions/runs/{run['id']}/artifacts?per_page=100"] = copied_inventory
    require(prove_accepted_native_reuse(new_main, "quality", lambda key: new_pages[key])
            == (new_head, prior_main), "exact new PR inherited Quality proof rejected")
    # Also preserve the already-valid shape where an unrelated frontend PR
    # reuses native Windows but still runs every Linux Cargo step physically.
    full_linux_pages = json.loads(json.dumps(new_pages))
    quality_run = next(row for row in current_runs["workflow_runs"]
                       if row["name"] == WORKFLOW["quality"])
    linux_jobs_url = f"repos/{REPOSITORY}/actions/runs/{quality_run['id']}/jobs?per_page=100"
    full_linux_job = next(job for job in full_linux_pages[linux_jobs_url]["jobs"]
                          if job["name"] == "Rust Linux and WASM")
    for step in full_linux_job["steps"]:
        step["conclusion"] = "skipped" if step["name"] == RUST_LINUX_LIGHT_CERT else "success"
    require(prove_accepted_native_reuse(new_main, "quality", lambda key: full_linux_pages[key])
            == (new_head, prior_main), "genuine Linux FULL on reused-native PR rejected")
    require(prove_accepted_native_reuse(new_main, "local-profile", lambda key: new_pages[key])
            == (new_head, prior_main), "exact new PR inherited Local proof rejected")
    verified_inherited = prove_release_artifacts(new_main, lambda key: new_pages[key])
    require(verified_inherited["native_prior_main_sha"] == prior_main,
            "new PR raw Bridge inherited source not preserved")
    for fault in ("current-bridge-sha", "current-bridge-size", "fake-continue-on-error",
                  "missing-prior-k1", "missing-prior-k2", "native-changed", "compare-truncated",
                  "linux-cert-missing", "linux-cert-failure", "linux-fake-native-step",
                  "linux-run-attempt", "linux-foreign-head", "linux-static-skipped",
                  "prior-linux-cargo-missing", "prior-linux-static-missing",
                  "k2-current-missing", "k2-current-skipped", "k2-current-failed",
                  "k2-current-attempt", "k2-current-head", "k2-cert-missing",
                  "k2-fake-full", "k2-aggregator-skipped"):
        case = json.loads(json.dumps(new_pages))
        quality_run = next(run for run in current_runs["workflow_runs"]
                           if run["name"] == WORKFLOW["quality"])
        local_run = next(run for run in current_runs["workflow_runs"]
                         if run["name"] == WORKFLOW["local-profile"])
        quality_artifacts = case[f"repos/{REPOSITORY}/actions/runs/{quality_run['id']}/artifacts?per_page=100"]["artifacts"]
        current_bridge = next(item for item in quality_artifacts if item["name"] == "profile-bridge.exe")
        if fault == "current-bridge-sha":
            current_bridge["digest"] = "sha256:" + "f" * 64
        elif fault == "current-bridge-size":
            current_bridge["size_in_bytes"] += 1
        elif fault == "fake-continue-on-error":
            jobs = case[f"repos/{REPOSITORY}/actions/runs/{quality_run['id']}/jobs?per_page=100"]["jobs"]
            windows = next(job for job in jobs if job["name"] == RUNNER_RECEIPT["quality"][1])
            next(step for step in windows["steps"]
                 if step["name"] == "Check and test native crates")["conclusion"] = "failure"
        elif fault in ("linux-cert-missing", "linux-cert-failure", "linux-fake-native-step",
                       "linux-run-attempt", "linux-foreign-head", "linux-static-skipped"):
            linux = next(job for job in case[linux_jobs_url]["jobs"]
                         if job["name"] == "Rust Linux and WASM")
            if fault == "linux-run-attempt":
                linux["run_attempt"] += 1
            elif fault == "linux-foreign-head":
                linux["head_sha"] = "d" * 40
            elif fault == "linux-fake-native-step":
                next(step for step in linux["steps"]
                     if step["name"] == "Test native crates")["conclusion"] = "success"
            elif fault == "linux-static-skipped":
                next(step for step in linux["steps"]
                     if step["name"] == RUST_LINUX_STATIC_REQUIRED[0])["conclusion"] = "skipped"
            else:
                for step in linux["steps"]:
                    if step["name"] == RUST_LINUX_LIGHT_CERT:
                        step["conclusion"] = ("skipped" if fault == "linux-cert-missing"
                                              else "failure")
        elif fault in ("missing-prior-k1", "missing-prior-k2"):
            original_id = quality_run["id"] - 1000 if fault == "missing-prior-k1" else local_run["id"] - 1000
            jobs = case[f"repos/{REPOSITORY}/actions/runs/{original_id}/jobs?per_page=100"]["jobs"]
            target_step = "Check and test native crates" if fault == "missing-prior-k1" else "Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits"
            for job in jobs:
                for step in job["steps"]:
                    if step["name"] == target_step:
                        step["conclusion"] = "skipped"
        elif fault.startswith("k2-"):
            job_key = f"repos/{REPOSITORY}/actions/runs/{local_run['id']}/jobs?per_page=100"
            inventory = case[job_key]
            jobs = inventory["jobs"]
            k2 = next(job for job in jobs if job["name"] == "Local Profile Windows K2")
            if fault == "k2-current-missing":
                jobs.remove(k2)
                inventory["total_count"] -= 1
            elif fault in ("k2-current-skipped", "k2-current-failed"):
                k2["conclusion"] = ("skipped" if fault.endswith("skipped") else "failure")
            elif fault == "k2-current-attempt":
                k2["run_attempt"] += 1
            elif fault == "k2-current-head":
                k2["head_sha"] = "d" * 40
            elif fault == "k2-aggregator-skipped":
                aggregator = next(job for job in jobs
                                  if job["name"] == "Local Profile Windows")
                aggregator["steps"][0]["conclusion"] = "skipped"
            else:
                step = next(step for step in k2["steps"]
                            if step["name"] == "Certify unchanged exact prior FULL native physical proof")
                if fault == "k2-cert-missing":
                    step["conclusion"] = "skipped"
                else:
                    # An inherited K2 cannot simultaneously claim its expensive
                    # physical Rust test was FULL on the current PR attempt.
                    k2["steps"].append({
                        "name": "Prove K2 256/512 MiB file streaming on Windows with peak RAM and disk limits",
                        "status": "completed", "conclusion": "success",
                    })
        elif fault in ("prior-linux-cargo-missing", "prior-linux-static-missing"):
            original_jobs = case[f"repos/{REPOSITORY}/actions/runs/{quality_run['id'] - 1000}/jobs?per_page=100"]["jobs"]
            linux = next(job for job in original_jobs if job["name"] == "Rust Linux and WASM")
            next(step for step in linux["steps"]
                 if step["name"] == (RUST_LINUX_FULL_STEPS[0] if fault == "prior-linux-cargo-missing"
                                      else RUST_LINUX_STATIC_REQUIRED[0]))["conclusion"] = "skipped"
        elif fault == "native-changed":
            case[native_compare_url]["files"][0]["filename"] = "Cargo.lock"
        else:
            case[native_compare_url]["files"] *= 300
        try:
            if fault.startswith("k2-"):
                prove_accepted_native_reuse(new_main, "local-profile", lambda key: case[key])
            else:
                prove_release_artifacts(new_main, lambda key: case[key])
        except (Unconfirmed, KeyError):
            pass
        else:
            raise AssertionError(f"unsafe inherited main Bridge proof accepted: {fault}")

    # The PR-side gate must reject stale base/head, foreign forks and bad
    # ancestry before a Windows consumer can skip any physical K1/K2.
    actual_checkout = subprocess.check_output(["git", "rev-parse", "HEAD"],
                                              text=True).strip()
    require(SHA.fullmatch(actual_checkout) is not None,
            "self-test checkout SHA unavailable")
    pr_api = dict(two_full)
    pr_api[f"repos/{REPOSITORY}/commits/{'b'*40}"] = {
        "sha": "b" * 40, "parents": [], "commit": {"tree": {"sha": "0" * 40}},
    }
    pr_api[f"repos/{REPOSITORY}/git/ref/heads/main"] = {
        "ref": "refs/heads/main", "object": {"sha": prior_main},
    }
    pr_api[f"repos/{REPOSITORY}/pulls/42"] = {
        "state": "open", "merged_at": None,
        "head": {"sha": actual_checkout, "repo": {"full_name": REPOSITORY}},
        "base": {"sha": prior_main, "ref": "main",
                 "repo": {"full_name": REPOSITORY}},
    }
    pr_compare_url = f"repos/{REPOSITORY}/compare/{prior_main}...{actual_checkout}"
    pr_api[pr_compare_url] = {
        "base_commit": {"sha": prior_main},
        "merge_base_commit": {"sha": prior_main},
        "status": "ahead", "ahead_by": 1,
        "commits": [{"sha": actual_checkout}],
        "files": [{"filename": "frontend/src/app/router.tsx", "status": "modified"}],
    }
    pr_environment = {
        "GITHUB_EVENT_NAME": "pull_request", "GITHUB_REPOSITORY": REPOSITORY,
        "PR_HEAD_SHA": actual_checkout, "PR_BASE_SHA": prior_main,
        "PR_NUMBER": "42",
    }
    original_environment = {key: os.environ.get(key) for key in pr_environment}
    try:
        os.environ.update(pr_environment)
        assert prove_open_native_pr(lambda path: pr_api[path], graph_api_only=True,
                                    as_of=fixture_now)[0] == prior_main
        for fault in ("stale-main", "stale-pr-head", "stale-pr-base",
                      "foreign-head-repo", "foreign-base-repo", "closed-pr",
                      "native-source", "incomplete-pr-compare", "invalid-event",
                      "prior-linux-test-skipped", "prior-linux-static-skipped"):
            case = json.loads(json.dumps(pr_api))
            if fault == "stale-main":
                case[f"repos/{REPOSITORY}/git/ref/heads/main"]["object"]["sha"] = "9" * 40
            elif fault == "stale-pr-head":
                case[f"repos/{REPOSITORY}/pulls/42"]["head"]["sha"] = "9" * 40
            elif fault == "stale-pr-base":
                case[f"repos/{REPOSITORY}/pulls/42"]["base"]["sha"] = "9" * 40
            elif fault == "foreign-head-repo":
                case[f"repos/{REPOSITORY}/pulls/42"]["head"]["repo"]["full_name"] = "other/repo"
            elif fault == "foreign-base-repo":
                case[f"repos/{REPOSITORY}/pulls/42"]["base"]["repo"]["full_name"] = "other/repo"
            elif fault == "closed-pr":
                case[f"repos/{REPOSITORY}/pulls/42"]["state"] = "closed"
            elif fault == "native-source":
                case[pr_compare_url]["files"][0]["filename"] = "apps/profile-bridge/src/main.rs"
            elif fault == "incomplete-pr-compare":
                case[pr_compare_url]["files"] *= 300
            elif fault in ("prior-linux-test-skipped", "prior-linux-static-skipped"):
                original_quality = next(run for run in
                                        case[f"repos/{REPOSITORY}/actions/runs?head_sha={'c'*40}&event=pull_request&per_page=100"]["workflow_runs"]
                                        if run["name"] == WORKFLOW["quality"])
                original_jobs = case[f"repos/{REPOSITORY}/actions/runs/{original_quality['id']}/jobs?per_page=100"]["jobs"]
                linux = next(job for job in original_jobs if job["name"] == "Rust Linux and WASM")
                required_step = (RUST_LINUX_FULL_STEPS[4] if fault == "prior-linux-test-skipped"
                                 else RUST_LINUX_STATIC_REQUIRED[2])
                next(step for step in linux["steps"]
                     if step["name"] == required_step)["conclusion"] = "skipped"
            else:
                os.environ["GITHUB_EVENT_NAME"] = "push"
            try:
                prove_open_native_pr(lambda path: case[path], graph_api_only=True,
                                     as_of=fixture_now)
            except (Unconfirmed, KeyError):
                pass
            else:
                raise AssertionError(f"unsafe current PR native proof accepted: {fault}")
            finally:
                os.environ["GITHUB_EVENT_NAME"] = "pull_request"
    finally:
        for key, value in original_environment.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value

    # A missing accepted-PR Bridge receipt may mean current K1/K2 were safely
    # inherited, not executed. Main must NEVER compile a replacement EXE that
    # has received neither the exact current nor the inherited physical proof.
    no_token = dict(os.environ)
    no_token.update({"GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/main"})
    no_token.pop("GITHUB_TOKEN", None)
    process = subprocess.run(
        [sys.executable, str(Path(__file__).resolve()), "--release-component", "bridge"],
        env=no_token, capture_output=True, text=True, check=False,
    )
    assert process.returncode != 0 and "BRIDGE_RELEASE_REUSE_NOT_PROVEN" in process.stderr

    # One post-physical raw receipt, bound to the real FULL Gate attempt,
    # authorizes exact bytes. A LIGHT PR inherits only a recent unchanged FULL
    # receipt, never a skipped browser job or a source-identity-only guess.
    release_id = "runtime-bundle-v3-sha256-" + "e" * 64
    full_pages, _ = fixture("camoufox")
    root = f"repos/{REPOSITORY}"
    main_sha, head_sha = "a" * 40, "c" * 40
    history_url = f"{root}/actions/runs?head_sha={head_sha}&event=pull_request&per_page=100"
    source_run = next(run for run in full_pages[history_url]["workflow_runs"]
                      if run["name"] == WORKFLOW["camoufox"])
    inventory_url = f"{root}/actions/runs/{source_run['id']}/artifacts?per_page=100"
    proof_artifact = {
        "id": 979, "name": "camoufox-runtime-proof.json",
        "size_in_bytes": 512, "digest": "sha256:" + "f" * 64,
        "expired": False, "created_at": "2026-10-31T00:05:00Z",
        "expires_at": "2030-01-01T00:00:00Z",
        "workflow_run": {
            "id": source_run["id"], "head_sha": head_sha,
            "head_branch": source_run["head_branch"],
        },
    }
    full_pages[inventory_url]["artifacts"].append(proof_artifact)
    full_pages[inventory_url]["total_count"] += 1
    reference, artifacts = runtime_proof_artifacts(
        main_sha, lambda key: full_pages[key], release_id,
    )
    require(reference["candidate"] == head_sha and set(artifacts) == {"proof"},
            "positive FULL runtime source/receipt artifact rejected")
    receipt = {
        "schema_version": 1, "kind": "CAMOUFOX_WINDOWS_PHYSICAL_RUNTIME_RECEIPT",
        "source_commit_sha": head_sha,
        "run_id": source_run["id"], "run_attempt": source_run["run_attempt"],
        "release_id": release_id, "origin": "published",
        "archive_sha256": "1" * 64, "archive_size_bytes": 1_525_000_000,
        "manifest_sha256": "2" * 64, "manifest_size_bytes": 1_063_374,
    }
    validate_physical_runtime_receipt(receipt, reference, release_id)
    published = {
        "tag_name": release_id, "draft": False, "prerelease": False,
        "assets": [
            {"name": "runtime-bundle.tar.gz", "state": "uploaded",
             "digest": "sha256:" + receipt["archive_sha256"],
             "size": receipt["archive_size_bytes"]},
            {"name": "runtime-manifest.json", "state": "uploaded",
             "digest": "sha256:" + receipt["manifest_sha256"],
             "size": receipt["manifest_size_bytes"]},
        ],
    }
    validate_physical_published_assets(published, receipt, release_id)
    def reject(proof: Callable[[], None], name: str) -> None:
        try:
            proof()
        except (Unconfirmed, KeyError, ValueError, TypeError):
            return
        raise AssertionError(f"unsafe runtime custody accepted: {name}")
    for fault in ("wrong-head", "wrong-attempt", "wrong-release-id",
                  "wrong-digest", "extra-field", "zero-size", "fake-origin"):
        changed = json.loads(json.dumps(receipt))
        if fault == "wrong-head":
            changed["source_commit_sha"] = "9" * 40
        elif fault == "wrong-attempt":
            changed["run_attempt"] += 1
        elif fault == "wrong-release-id":
            changed["release_id"] = "runtime-bundle-v3-sha256-" + "9" * 64
        elif fault == "wrong-digest":
            changed["archive_sha256"] = "not-a-sha"
        elif fault == "extra-field":
            changed["untrusted"] = True
        elif fault == "zero-size":
            changed["archive_size_bytes"] = 0
        else:
            changed["origin"] = "assumed"
        reject(lambda: validate_physical_runtime_receipt(changed, reference, release_id),
               fault)
    for fault in ("wrong-published-sha", "wrong-published-size",
                  "duplicate-asset", "draft-release"):
        changed = json.loads(json.dumps(published))
        if fault == "wrong-published-sha":
            changed["assets"][0]["digest"] = "sha256:" + "9" * 64
        elif fault == "wrong-published-size":
            changed["assets"][0]["size"] += 1
        elif fault == "duplicate-asset":
            changed["assets"][1] = dict(changed["assets"][0])
        else:
            changed["draft"] = True
        reject(lambda: validate_physical_published_assets(changed, receipt, release_id),
               fault)
    reject(lambda: validate_physical_runtime_miss_artifacts(receipt, artifacts),
           "published-only receipt accepted on runtime publication miss")
    built = dict(receipt, origin="built")
    miss_artifacts = {
        **artifacts,
        "archive": {"sha256": built["archive_sha256"],
                    "size": built["archive_size_bytes"]},
        "manifest": {"sha256": built["manifest_sha256"],
                     "size": built["manifest_size_bytes"]},
    }
    validate_physical_runtime_miss_artifacts(built, miss_artifacts)
    # GitHub raw artifact inventories use on-disk basenames, not logical
    # upload-artifact name= values. Validate the REST producer inventory for
    # a 1.5-GiB MISS, alongside the independent small receipt HIT.
    built_pages = json.loads(json.dumps(full_pages))
    for name, digest, size, identifier in (
        ("runtime-bundle.tar.gz", built["archive_sha256"],
         built["archive_size_bytes"], 980),
        ("runtime-manifest.json", built["manifest_sha256"],
         built["manifest_size_bytes"], 981),
    ):
        item = json.loads(json.dumps(proof_artifact))
        item.update(id=identifier, name=name, digest="sha256:" + digest,
                    size_in_bytes=size)
        built_pages[inventory_url]["artifacts"].append(item)
        built_pages[inventory_url]["total_count"] += 1
    built_reference, built_selected = runtime_proof_artifacts(
        main_sha, lambda key: built_pages[key], release_id,
    )
    require(built_reference["candidate"] == head_sha
            and set(built_selected) == {"proof", "archive", "manifest"},
            "raw GitHub artifact basename inventory was not resolved")
    validate_physical_runtime_miss_artifacts(built, built_selected)
    for fault in ("configured-logical-name", "missing-raw-archive",
                  "duplicate-raw-archive", "wrong-raw-archive-SHA"):
        changed = json.loads(json.dumps(built_pages))
        inventory = changed[inventory_url]
        archive = next(a for a in inventory["artifacts"]
                       if a["name"] == "runtime-bundle.tar.gz")
        if fault == "configured-logical-name":
            archive["name"] = f"camoufox-runtime-bundle-{head_sha}"
        elif fault == "missing-raw-archive":
            inventory["artifacts"].remove(archive)
            inventory["total_count"] -= 1
        elif fault == "duplicate-raw-archive":
            inventory["artifacts"].append(dict(archive))
            inventory["total_count"] += 1
        else:
            archive["digest"] = "sha256:" + "9" * 64
        def prove_miss() -> None:
            _, selected = runtime_proof_artifacts(
                main_sha, lambda key: changed[key], release_id,
            )
            validate_physical_runtime_miss_artifacts(built, selected)
        reject(prove_miss, "raw GitHub artifact " + fault)
    for fault in ("missing-archive", "missing-manifest", "tampered-archive"):
        changed = json.loads(json.dumps(miss_artifacts))
        if fault == "missing-archive":
            changed.pop("archive")
        elif fault == "missing-manifest":
            changed.pop("manifest")
        else:
            changed["archive"]["sha256"] = "9" * 64
        reject(lambda: validate_physical_runtime_miss_artifacts(built, changed),
               fault)
    for fault in ("missing-receipt", "legacy-configured-name", "expired-receipt", "wrong-artifact-head",
                  "wrong-artifact-run", "duplicate-receipt", "stale-attempt",
                  "missing-physical-step", "post-merge-rerun"):
        changed = json.loads(json.dumps(full_pages))
        inventory = changed[inventory_url]
        item = next(a for a in inventory["artifacts"]
                    if a["name"] == proof_artifact["name"])
        if fault == "missing-receipt":
            inventory["artifacts"].remove(item)
            inventory["total_count"] -= 1
        elif fault == "legacy-configured-name":
            item["name"] = f"camoufox-runtime-proof-{head_sha}"
        elif fault == "expired-receipt":
            item["expired"] = True
        elif fault == "wrong-artifact-head":
            item["workflow_run"]["head_sha"] = "9" * 40
        elif fault == "wrong-artifact-run":
            item["workflow_run"]["id"] += 1
        elif fault == "duplicate-receipt":
            inventory["artifacts"].append(dict(item))
            inventory["total_count"] += 1
        elif fault == "stale-attempt":
            jobs = changed[f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100"]["jobs"]
            producer = next(j for j in jobs
                            if j["name"] == "Profile Bridge Windows regression")
            producer["run_attempt"] += 1
        elif fault == "missing-physical-step":
            jobs = changed[f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100"]["jobs"]
            producer = next(j for j in jobs
                            if j["name"] == "Profile Bridge Windows regression")
            step = next(s for s in producer["steps"]
                        if s["name"] == "Upload SHA-bound physical runtime proof receipt")
            step["conclusion"] = "skipped"
        else:
            source = next(run for run in changed[history_url]["workflow_runs"]
                          if run["name"] == WORKFLOW["camoufox"])
            source["updated_at"] = "2026-11-01T00:00:00Z"
        reject(lambda: runtime_proof_artifacts(main_sha, lambda key: changed[key],
                                               release_id), fault)

    # Inherited LIGHT success comes from current exact-tree 35-job CI plus the
    # previous accepted main FULL physical receipt, with only documented
    # Camoufox-unrelated changes (including exact frontend/src TS/CSS).
    light_main, light_head = "6" * 40, "7" * 40
    light = json.loads(json.dumps(full_pages))
    light[f"{root}/commits/{light_main}"] = {
        "sha": light_main, "parents": [{"sha": main_sha}],
        "commit": {"tree": {"sha": "8" * 40}},
    }
    light[f"{root}/commits/{light_main}/pulls"] = [{
        "state": "closed", "merged_at": "2026-11-01T01:00:00Z",
        "merge_commit_sha": light_main,
        "base": {"sha": main_sha, "ref": "main",
                 "repo": {"full_name": REPOSITORY}},
        "head": {"sha": light_head, "ref": "codex/frontend-only",
                 "repo": {"full_name": REPOSITORY}},
    }]
    light[f"{root}/commits/{light_head}"] = {
        "sha": light_head, "commit": {"tree": {"sha": "8" * 40}},
    }
    light[f"{root}/commits/{'b'*40}"] = {
        "sha": "b" * 40, "parents": [],
        "commit": {"tree": {"sha": "0" * 40}},
    }
    light_runs = json.loads(json.dumps(full_pages[history_url]))
    for run in light_runs["workflow_runs"]:
        run["id"] += 1000
        run["head_sha"] = light_head
        run["head_branch"] = "codex/frontend-only"
        run["created_at"] = "2026-11-01T00:00:00Z"
        run["updated_at"] = "2026-11-01T00:10:00Z"
    light_history_url = f"{root}/actions/runs?head_sha={light_head}&event=pull_request&per_page=100"
    light[light_history_url] = light_runs
    light_source = next(run for run in light_runs["workflow_runs"]
                        if run["name"] == WORKFLOW["camoufox"])
    light_jobs = json.loads(json.dumps(full_pages[
        f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100"]))
    for job in light_jobs["jobs"]:
        job["run_id"] = light_source["id"]
        job["head_sha"] = light_head
        for step in job["steps"]:
            step["conclusion"] = "skipped"
    light[f"{root}/actions/runs/{light_source['id']}/jobs?per_page=100"] = light_jobs
    light[f"{root}/compare/{main_sha}...{light_main}"] = {
        "base_commit": {"sha": main_sha},
        "merge_base_commit": {"sha": main_sha},
        "status": "ahead", "ahead_by": 1,
        "commits": [{"sha": light_main}],
        "files": [{"filename": "docs/runtime-note.md", "status": "modified"}],
    }
    require(prove(main_sha, "camoufox", lambda key: light[key]) == head_sha,
            "fixture lost prior FULL physical Camoufox proof")
    require(camoufox_unrelated(github_changed_paths(
        main_sha, light_main, lambda key: light[key])),
        "runtime-unrelated fixture did not classify LIGHT")
    inherited_source, inherited_head = prove_runtime_producer(
        light_main, lambda key: light[key],
    )
    require(inherited_source == main_sha and inherited_head == head_sha,
            "LIGHT frontend PR did not inherit same-source FULL physical Gate")
    inherited_reference, inherited_artifacts = runtime_proof_artifacts(
        light_main, lambda key: light[key], release_id,
    )
    require(inherited_reference["candidate"] == head_sha
            and set(inherited_artifacts) == {"proof"},
            "LIGHT runtime physical SHA receipt was not inherited")
    # The PR-side gate must make its LIGHT decision against live protected
    # main and the exact source HEAD before it can suppress physical Windows.
    light[f"{root}/git/ref/heads/main"] = {
        "ref": "refs/heads/main", "object": {"sha": main_sha},
    }
    light[f"{root}/pulls/42"] = {
        "state": "open", "merged_at": None,
        "head": {"sha": light_head, "repo": {"full_name": REPOSITORY}},
        "base": {"sha": main_sha, "ref": "main",
                 "repo": {"full_name": REPOSITORY}},
    }
    light[f"{root}/compare/{main_sha}...{light_head}"] = {
        "base_commit": {"sha": main_sha},
        "merge_base_commit": {"sha": main_sha},
        "status": "ahead", "ahead_by": 1,
        "commits": [{"sha": light_head}],
        "files": [{"filename": "frontend/src/app/main.tsx", "status": "modified"}],
    }
    light_environment = {
        "GITHUB_EVENT_NAME": "pull_request",
        "GITHUB_REPOSITORY": REPOSITORY,
        "PR_HEAD_SHA": light_head,
        "PR_BASE_SHA": main_sha,
        "PR_NUMBER": "42",
    }
    old_environment = {key: os.environ.get(key) for key in light_environment}
    try:
        os.environ.update(light_environment)
        base, current_release_id = prove_open_camoufox_light_pr(
            lambda key: light[key], graph_api_only=True,
        )
        require(base == main_sha and current_release_id.startswith(
            "runtime-bundle-v3-sha256-"),
            "current exact frontend-only PR could not seek published-byte proof")
        for fault in ("stale-base", "wrong-head", "foreign-repo",
                      "runtime-change", "package-change", "renamed",
                      "incomplete-compare", "closed-pr", "invalid-event"):
            changed = json.loads(json.dumps(light))
            if fault == "stale-base":
                changed[f"{root}/git/ref/heads/main"]["object"]["sha"] = "9" * 40
            elif fault == "wrong-head":
                changed[f"{root}/pulls/42"]["head"]["sha"] = "9" * 40
            elif fault == "foreign-repo":
                changed[f"{root}/pulls/42"]["head"]["repo"]["full_name"] = "foreign/repo"
            elif fault == "runtime-change":
                changed[f"{root}/compare/{main_sha}...{light_head}"]["files"][0]["filename"] = "runtime/camouhost/real.py"
            elif fault == "package-change":
                changed[f"{root}/compare/{main_sha}...{light_head}"]["files"][0]["filename"] = "frontend/package.json"
            elif fault == "renamed":
                changed[f"{root}/compare/{main_sha}...{light_head}"]["files"][0]["status"] = "renamed"
            elif fault == "incomplete-compare":
                changed[f"{root}/compare/{main_sha}...{light_head}"]["files"] *= 300
            elif fault == "closed-pr":
                changed[f"{root}/pulls/42"]["state"] = "closed"
            else:
                os.environ["GITHUB_EVENT_NAME"] = "push"
            reject(lambda: prove_open_camoufox_light_pr(
                lambda key: changed[key], graph_api_only=True,
            ), fault)
            os.environ["GITHUB_EVENT_NAME"] = "pull_request"
    finally:
        for key, value in old_environment.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
    light[f"{root}/compare/{main_sha}...{light_main}"]["files"][0]["filename"] = "runtime/camouhost/real.py"
    reject(lambda: prove_runtime_producer(light_main, lambda key: light[key]),
           "LIGHT inherited changed runtime source")

    print("Accepted-main exact PR proof: positive and negative fixtures PASS")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--gate", choices=sorted(CRITICAL))
    parser.add_argument("--release-component", choices=("cloud", "bridge", "runtime"))
    parser.add_argument("--runtime-release-id")
    parser.add_argument("--runtime-publication-state", choices=("reuse", "absent", "resume"))
    parser.add_argument("--runtime-output-dir", type=Path)
    parser.add_argument("--pr-native-proof", action="store_true")
    parser.add_argument("--pr-quality-rust-linux-proof", action="store_true")
    parser.add_argument("--pr-d1-proof", action="store_true")
    parser.add_argument("--verify-d1-report", type=Path)
    parser.add_argument("--pr-camoufox-byte-proof", action="store_true")
    parser.add_argument("--pr-native-certify", action="store_true")
    parser.add_argument("--pr-native-bridge", type=Path)
    parser.add_argument("--native-runner-receipt", choices=sorted(RUNNER_RECEIPT))
    parser.add_argument("--receipt-output", type=Path)
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    routes = sum((args.gate is not None, args.release_component is not None,
                  args.pr_camoufox_byte_proof,
                  args.pr_native_proof, args.pr_d1_proof,
                  args.verify_d1_report is not None,
                  args.pr_quality_rust_linux_proof, args.pr_native_certify,
                  args.pr_native_bridge is not None,
                  args.native_runner_receipt is not None))
    if routes == 0:
        return 0 if args.self_test else parser.error("one evidence route is required")
    if routes != 1:
        parser.error("choose exactly one evidence route")
    if args.verify_d1_report is not None:
        path = args.verify_d1_report
        require(path.is_file() and not path.is_symlink()
                and 100 <= path.stat().st_size <= 512 * 1024,
                "local D1 report file absent, linked or oversized")
        validate_d1_fault_report(json.loads(path.read_text(encoding="utf-8")))
        print("D1_FULL_LOCAL_REPORT_VALIDATED=PASS;scenarios=20;provider_mutation=false")
        return 0
    if args.native_runner_receipt is not None:
        if args.receipt_output is None:
            parser.error("--native-runner-receipt requires --receipt-output")
        write_full_runner_receipt(args.native_runner_receipt, args.receipt_output)
        print(f"FULL {args.native_runner_receipt} Windows runner identity receipt ready")
        return 0
    if args.pr_camoufox_byte_proof:
        try:
            token = os.environ.get("GITHUB_TOKEN")
            require(bool(token), "Camoufox LIGHT PR needs read-only GitHub authority")
            api = api_reader(token)
            base, release_id = prove_open_camoufox_light_pr(api)
            with tempfile.TemporaryDirectory(prefix="camoufox-light-published-byte-") as temporary:
                destination = Path(temporary)
                prepare_physical_runtime_component(
                    base, release_id, "reuse", destination, api, token,
                )
            print(f"CAMOUFOX_LIGHT_PUBLISHED_BYTES_PROVEN=PASS;base={base};release_id={release_id}")
            return 0
        except (Unconfirmed, KeyError, ValueError, TypeError, AssertionError,
                urllib.error.URLError, OSError, subprocess.SubprocessError,
                UnicodeError) as error:
            print(f"CAMOUFOX_LIGHT_BYTE_PROOF_UNAVAILABLE ({type(error).__name__}): "
                  f"{error}; full real Windows/Camoufox proof mandatory")
            return 1
    if args.pr_d1_proof:
        verified = False
        try:
            token = os.environ.get("GITHUB_TOKEN")
            require(bool(token), "D1 LIGHT proof requires read-only GitHub authority")
            prior = prove_open_d1_light_pr(api_reader(token), token)
            print(f"D1_LIGHT_FULL_REPORT_PROVEN=PASS;prior_FULL_main={prior}")
            verified = True
        except (Unconfirmed, KeyError, ValueError, TypeError, AssertionError,
                urllib.error.URLError, OSError, subprocess.SubprocessError,
                UnicodeError, zipfile.BadZipFile) as error:
            print(f"D1_LIGHT_FULL_REPORT_UNAVAILABLE ({type(error).__name__}): {error}; "
                  "run real FULL D1 Linux proof")
        if args.github_output is not None:
            with args.github_output.open("a", encoding="utf-8") as output:
                output.write(f"d1_light={'true' if verified else 'false'}\n")
        return 0  # An absent/ambiguous proof always executes FULL on this PR.
    if args.pr_quality_rust_linux_proof:
        # Reuses the existing exact-tree native source/closure attestor. It
        # independently proves a prior genuinely executed FULL Quality Linux
        # (all nine Cargo steps) and FULL Local Windows source, not a LIGHT
        # predecessor, before any current-head Rust compilation is skipped.
        try:
            token = os.environ.get("GITHUB_TOKEN")
            require(bool(token), "Quality Rust Linux proof needs read-only GitHub authority")
            prior, receipt = prove_open_native_pr(api_reader(token), graph_api_only=False)
            print(f"QUALITY_RUST_LINUX_PRIOR_FULL_PROVEN=PASS;"
                  f"PR_HEAD={os.environ['PR_HEAD_SHA']};prior_FULL_main={prior};"
                  f"prior_quality_run={receipt['quality_run_id']};"
                  f"attempt={receipt['quality_run_attempt']}")
            return 0
        except (Unconfirmed, KeyError, ValueError, TypeError, AssertionError,
                urllib.error.URLError, OSError, subprocess.SubprocessError,
                UnicodeError) as error:
            print(f"QUALITY_RUST_LINUX_PRIOR_FULL_UNAVAILABLE ({type(error).__name__}): "
                  f"{error}; execute all FULL Rust Linux Cargo steps")
            return 1
    if args.pr_native_proof or args.pr_native_certify or args.pr_native_bridge is not None:
        verified = False
        try:
            token = os.environ.get("GITHUB_TOKEN")
            require(bool(token), "native PR proof needs read-only GitHub API authority")
            api = api_reader(token)
            prior, receipt = prove_open_native_pr(
                api, graph_api_only=not args.pr_native_proof,
            )
            bridge = receipt["artifacts"]["bridge"]
            for gate in native_runner_receipt_scopes(
                args.pr_native_proof, args.pr_native_bridge is not None,
            ):
                verify_full_runner_receipt(select_full_runner_receipt(prior, gate, api), token)
            if args.pr_native_bridge is not None:
                destination = args.pr_native_bridge.absolute()
                require(not destination.exists() and not destination.is_symlink(),
                        "inherited Bridge destination is not clean")
                destination.parent.mkdir(parents=True, exist_ok=True)
                with tempfile.TemporaryDirectory(prefix="prior-physical-bridge-") as temporary:
                    directory = Path(temporary)
                    raw = directory / bridge["name"]
                    download_raw_artifact(bridge, raw, token)
                    verified_source = verify_downloaded_raw(directory, "bridge", bridge)
                    with verified_source.open("rb") as executable:
                        require(bridge["size"] > 100_000 and executable.read(2) == b"MZ",
                                "previous FULL-proven Bridge EXE has invalid Windows shape")
                    try:
                        shutil.copyfile(verified_source, destination)
                        require(destination.stat().st_size == bridge["size"],
                                "inherited Bridge copy size mismatch")
                        require(hashlib.sha256(destination.read_bytes()).hexdigest() == bridge["sha256"],
                                "inherited Bridge copy SHA-256 mismatch")
                    except Exception:
                        destination.unlink(missing_ok=True)
                        raise
            print(f"Certified unchanged native input graph: PR_HEAD={os.environ['PR_HEAD_SHA']} "
                  f"prior_FULL_main={prior} previous_bridge_sha256={bridge['sha256']} "
                  f"prior_quality_run={receipt['quality_run_id']} "
                  f"attempt={receipt['quality_run_attempt']}")
            verified = True
        except (Unconfirmed, KeyError, ValueError, TypeError, AssertionError,
                urllib.error.URLError, OSError, subprocess.SubprocessError,
                UnicodeError) as error:
            print(f"No valid inherited native proof ({type(error).__name__}): {error}; "
                  "full native execution required")
        if args.pr_native_proof:
            if args.github_output is not None:
                with args.github_output.open("a", encoding="utf-8") as output:
                    output.write(f"native_unaffected={'true' if verified else 'false'}\n")
            return 0  # Any error explicitly selects FULL native proof.
        return 0 if verified else 1  # Windows falls back on failed inheritance.
    reuse = False
    if os.environ.get("GITHUB_EVENT_NAME") == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main":
        token = os.environ.get("GITHUB_TOKEN")
        main_sha = os.environ.get("GITHUB_SHA", "")
        if token:
            try:
                api = api_reader(token)
                if args.release_component is None:
                    if args.gate == "d1":
                        candidate, receipt = prove_accepted_d1_full(main_sha, api, token)
                        print(f"Reuse pre-merge FULL D1 Linux+Windows and SHA-verified "
                              f"20-scenario PR ZIP from exact tree {candidate};"
                              f"artifact_id={receipt['id']}")
                    elif args.gate in NATIVE_REUSED_CRITICAL:
                        candidate, prior = prove_accepted_native_reuse(main_sha, args.gate, api)
                        if prior is None:
                            print(f"Reuse verified current PR FULL native proof from exact tree {candidate}")
                        else:
                            print(f"Reuse current exact-tree PR {candidate} and prior FULL "
                                  f"native proof at accepted main {prior}; native inputs unchanged")
                    else:
                        candidate = prove(main_sha, args.gate, api)
                        print(f"Reuse verified PR test evidence from exact tree {candidate}")
                elif args.release_component == "runtime":
                    require(bool(args.runtime_release_id)
                            and args.runtime_publication_state is not None
                            and args.runtime_output_dir is not None,
                            "runtime component requires release ID, publication state and output dir")
                    prepare_physical_runtime_component(
                        main_sha, args.runtime_release_id,
                        args.runtime_publication_state, args.runtime_output_dir,
                        api, token,
                    )
                else:
                    prepare_release_component(main_sha, args.release_component, api)
                reuse = True
            except (Unconfirmed, KeyError, ValueError, TypeError,
                    urllib.error.URLError, OSError, subprocess.SubprocessError,
                    tarfile.TarError, UnicodeError, zipfile.BadZipFile,
                    zipfile.LargeZipFile, zlib.error, EOFError) as error:
                if args.gate == "camoufox":
                    try:
                        # The current PR may legitimately skip heavy browser tests
                        # for an audited unrelated change. Require its exact-tree
                        # merge + all 35 exact CI source job outcomes first.
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
                    print(f"No reusable evidence ({type(error).__name__}): {error}; full main execution required")
        else:
            print("No GitHub API authority; run full main gate")
    else:
        print("Not an accepted-main push; full PR test is mandatory")
    if args.release_component == "bridge" and not reuse:
        print("BRIDGE_RELEASE_REUSE_NOT_PROVEN: exact physically proven PR Bridge "
              "artifact missing; refusing untested main recompilation", file=sys.stderr)
        return 1
    if args.release_component == "runtime" and not reuse:
        print("RUNTIME_PHYSICAL_BYTES_NOT_PROVEN: no exact published-byte Gate "
              "receipt for this accepted main/runtime identity; refusing "
              "main-side rebuild or unproven Release publication", file=sys.stderr)
        return 1
    if args.github_output is not None:
        with args.github_output.open("a", encoding="utf-8") as handle:
            handle.write(f"reuse={'true' if reuse else 'false'}\n")
    print(f"reuse={'true' if reuse else 'false'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
