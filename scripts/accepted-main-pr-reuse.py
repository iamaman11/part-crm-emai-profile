#!/usr/bin/env python3
"""Reuse exact PR test evidence after a verified, tree-identical PR merge.

This is a read-only GitHub API attestation, not a cache. A missing/ambiguous
receipt returns reuse=false and runs the ordinary heavy tests on main.
The accepted-main Release Set is still built and signed from exact main.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import runpy
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from datetime import datetime, timezone
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
        ),
        # A successful meta status cannot replace the underlying real tests.
        "Real Camoufox cold-launch proof": (
            "Require exact official Camoufox and Bridge Windows proof",
        ),
    },
    "quality": {
        "Rust Linux and WASM": ("Test native crates",),
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
    source_run = latest[WORKFLOW[gate]]
    jobs = api(f"{root}/actions/runs/{source_run['id']}/jobs?per_page=100")
    require(
        isinstance(jobs.get("jobs"), list)
        and isinstance(jobs.get("total_count"), int)
        and jobs["total_count"] <= 100,
        "incomplete PR job inventory",
    )
    by_name: dict[str, dict[str, Any]] = {}
    for job in jobs["jobs"]:
        name = job.get("name")
        if name in CRITICAL[gate]:
            require(name not in by_name, f"duplicate required PR job: {name}")
            by_name[name] = job
    for name, steps in CRITICAL[gate].items():
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
    candidate = prove(main_sha, "quality", api)
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
    return {
        "candidate_sha": candidate,
        "quality_run_id": quality["id"],
        "quality_run_attempt": quality["run_attempt"],
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


def prepare_release_component(main_sha: str, component: str, api: Callable[[str], Any]) -> None:
    """Prove all PR artifacts; download this component's exact compiler outputs.

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
    jobs = [
        {"name": name, "status": "completed", "conclusion": "success",
         "run_id": gate_run["id"], "run_attempt": gate_run["run_attempt"],
         "head_sha": head, "workflow_name": WORKFLOW[gate],
         "started_at": "2026-10-31T00:00:00Z", "completed_at": "2026-10-31T00:10:00Z",
         "steps": [{"name": step, "status": "completed", "conclusion": "success"}
                   for step in steps]}
        for name, steps in CRITICAL[gate].items()
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


def self_test() -> None:
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
            jobs[0]["steps"].append(dict(jobs[0]["steps"][0]))
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
            next(run for run in case[run_path]["workflow_runs"] if run["name"] == "Quality Gate")[
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
    parser.add_argument("--release-component", choices=("cloud", "bridge"))
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    if args.gate is None and args.release_component is None:
        return 0 if args.self_test else parser.error("--gate or --release-component is required")
    if args.gate is not None and args.release_component is not None:
        parser.error("choose only one reusable evidence route")
    reuse = False
    if os.environ.get("GITHUB_EVENT_NAME") == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main":
        token = os.environ.get("GITHUB_TOKEN")
        main_sha = os.environ.get("GITHUB_SHA", "")
        if token:
            try:
                api = api_reader(token)
                if args.release_component is None:
                    candidate = prove(main_sha, args.gate, api)
                    print(f"Reuse verified PR test evidence from exact tree {candidate}")
                else:
                    prepare_release_component(main_sha, args.release_component, api)
                reuse = True
            except (Unconfirmed, KeyError, ValueError, TypeError,
                    urllib.error.URLError, OSError, subprocess.SubprocessError,
                    tarfile.TarError, UnicodeError) as error:
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
                    print(f"No reusable evidence ({type(error).__name__}): {error}; full main execution required")
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
