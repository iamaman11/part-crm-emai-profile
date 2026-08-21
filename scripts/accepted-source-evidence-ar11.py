#!/usr/bin/env python3
"""Build deterministic metadata-only AcceptedSourceEvidence v1 from saved GitHub API responses.

This adapter has no network, credential, repository mutation, provider mutation, or policy
decision authority. GitHub Actions collects branch/compare responses; native Rust `opsctl`
owns the fail-closed acceptance decision.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
KIND = "AR11_ACCEPTED_SOURCE_EVIDENCE"
COLLECTION_AUTHORITY = "github-actions/github-api"
PROOF_METHOD = "GITHUB_COMPARE_API"


class EvidenceError(ValueError):
    pass


def fail(message: str) -> None:
    raise EvidenceError(message)


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def document(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def load_object(path: Path, label: str) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise EvidenceError(f"{label} must be valid JSON: {error}") from error
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    return value


def nested_string(value: dict[str, Any], path: tuple[str, ...], label: str) -> str:
    current: Any = value
    for key in path:
        if not isinstance(current, dict) or key not in current:
            fail(f"{label} missing {'.'.join(path)}")
        current = current[key]
    if not isinstance(current, str) or not current:
        fail(f"{label} field {'.'.join(path)} must be a non-empty string")
    return current


def integer(value: dict[str, Any], key: str, label: str) -> int:
    observed = value.get(key)
    if not isinstance(observed, int) or isinstance(observed, bool) or observed < 0:
        fail(f"{label} field {key} must be a non-negative integer")
    return observed


def git_sha(value: str, label: str) -> str:
    if len(value) != 40 or any(character not in "0123456789abcdef" for character in value):
        fail(f"{label} must be an exact 40-character lowercase hexadecimal SHA")
    return value


def compare_head_sha(
    *,
    repository: str,
    source_sha: str,
    observed_protected_main_sha: str,
    comparison: dict[str, Any],
) -> str:
    expected_url = (
        f"https://api.github.com/repos/{repository}/compare/"
        f"{source_sha}...{observed_protected_main_sha}"
    )
    observed_url = comparison.get("url")
    if observed_url != expected_url:
        fail("compare response URL is not bound to the exact repository/source/protected-main request")

    head_commit = comparison.get("head_commit")
    if isinstance(head_commit, dict):
        explicit_head_sha = nested_string(comparison, ("head_commit", "sha"), "compare response")
        if explicit_head_sha != observed_protected_main_sha:
            fail("compare response head_commit.sha differs from observed protected-main SHA")
    elif head_commit is not None:
        fail("compare response head_commit must be an object or null")

    return observed_protected_main_sha


def build_evidence(
    *,
    repository: str,
    release_set_id: str,
    source_sha: str,
    branch: dict[str, Any],
    comparison: dict[str, Any],
) -> dict[str, Any]:
    source_sha = git_sha(source_sha, "source SHA")
    branch_name = branch.get("name")
    if not isinstance(branch_name, str) or not branch_name:
        fail("branch response missing name")
    protected = branch.get("protected")
    if not isinstance(protected, bool):
        fail("branch response protected must be boolean")

    observed_protected_main_sha = git_sha(
        nested_string(branch, ("commit", "sha"), "branch response"),
        "observed protected-main SHA",
    )
    base_sha = git_sha(
        nested_string(comparison, ("base_commit", "sha"), "compare response"),
        "compare base SHA",
    )
    merge_base_sha = git_sha(
        nested_string(comparison, ("merge_base_commit", "sha"), "compare response"),
        "compare merge-base SHA",
    )
    status = comparison.get("status")
    if not isinstance(status, str):
        fail("compare response status must be a string")
    ahead_by = integer(comparison, "ahead_by", "compare response")
    behind_by = integer(comparison, "behind_by", "compare response")
    head_sha = compare_head_sha(
        repository=repository,
        source_sha=source_sha,
        observed_protected_main_sha=observed_protected_main_sha,
        comparison=comparison,
    )

    value: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "kind": KIND,
        "repository": repository,
        "release_set_id": release_set_id,
        "source_commit_sha": source_sha,
        "protected_ref": f"refs/heads/{branch_name}",
        "protected_ref_verified": protected,
        "observed_protected_main_sha": observed_protected_main_sha,
        "collection_authority": COLLECTION_AUTHORITY,
        "proof": {
            "method": PROOF_METHOD,
            "base_sha": base_sha,
            "head_sha": head_sha,
            "merge_base_sha": merge_base_sha,
            "status": status,
            "ahead_by": ahead_by,
            "behind_by": behind_by,
        },
    }
    value["evidence_sha256"] = sha256(canonical(value))
    return value


def write_evidence(args: argparse.Namespace) -> None:
    if not args.repository or not args.release_set_id or not args.source_sha:
        fail("repository, release-set-id, and source-sha are required")
    branch = load_object(args.branch_json, "branch response")
    comparison = load_object(args.compare_json, "compare response")
    value = build_evidence(
        repository=args.repository,
        release_set_id=args.release_set_id,
        source_sha=args.source_sha,
        branch=branch,
        comparison=comparison,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.is_symlink():
        fail(f"output may not be a symlink: {args.output}")
    args.output.write_bytes(document(value))


def expect_failure(callable_value: Any, expected_message: str) -> None:
    try:
        callable_value()
    except EvidenceError as error:
        if expected_message not in str(error):
            fail(f"unexpected self-test failure: {error}")
        return
    fail("self-test expected fail-closed evidence rejection")


def compare_url(repository: str, source: str, head: str) -> str:
    return f"https://api.github.com/repos/{repository}/compare/{source}...{head}"


def self_test() -> None:
    repository = "iamaman11/part-crm-emai-profile"
    source = "1" * 40
    head = "2" * 40
    branch = {"name": "main", "protected": True, "commit": {"sha": head}}
    ahead_without_head = {
        "url": compare_url(repository, source, head),
        "base_commit": {"sha": source},
        "merge_base_commit": {"sha": source},
        "status": "ahead",
        "ahead_by": 2,
        "behind_by": 0,
    }
    first = build_evidence(
        repository=repository,
        release_set_id="release-set-v1-sha256-" + "a" * 64,
        source_sha=source,
        branch=branch,
        comparison=ahead_without_head,
    )
    second = build_evidence(
        repository=repository,
        release_set_id="release-set-v1-sha256-" + "a" * 64,
        source_sha=source,
        branch=branch,
        comparison=ahead_without_head,
    )
    if first != second or len(first["evidence_sha256"]) != 64:
        fail("AcceptedSourceEvidence generation is not deterministic")
    if first["proof"]["head_sha"] != head or first["proof"]["status"] != "ahead":
        fail("raw ahead comparison normalization failed")

    identical_branch = {"name": "main", "protected": True, "commit": {"sha": source}}
    identical_without_head = {
        "url": compare_url(repository, source, source),
        "base_commit": {"sha": source},
        "merge_base_commit": {"sha": source},
        "status": "identical",
        "ahead_by": 0,
        "behind_by": 0,
    }
    identical = build_evidence(
        repository=repository,
        release_set_id="release-set-v2-sha256-" + "b" * 64,
        source_sha=source,
        branch=identical_branch,
        comparison=identical_without_head,
    )
    if identical["proof"]["head_sha"] != source or identical["proof"]["status"] != "identical":
        fail("raw identical comparison normalization failed")

    explicit_head = dict(ahead_without_head)
    explicit_head["head_commit"] = {"sha": head}
    explicit = build_evidence(
        repository=repository,
        release_set_id="release-set-v2-sha256-" + "c" * 64,
        source_sha=source,
        branch=branch,
        comparison=explicit_head,
    )
    if explicit["proof"]["head_sha"] != head:
        fail("explicit compare head consistency proof failed")

    missing_url = dict(ahead_without_head)
    missing_url.pop("url")
    expect_failure(
        lambda: build_evidence(
            repository=repository,
            release_set_id="release-set-v2-sha256-" + "d" * 64,
            source_sha=source,
            branch=branch,
            comparison=missing_url,
        ),
        "URL is not bound",
    )

    wrong_head_url = dict(ahead_without_head)
    wrong_head_url["url"] = compare_url(repository, source, "3" * 40)
    expect_failure(
        lambda: build_evidence(
            repository=repository,
            release_set_id="release-set-v2-sha256-" + "e" * 64,
            source_sha=source,
            branch=branch,
            comparison=wrong_head_url,
        ),
        "URL is not bound",
    )

    wrong_repository_url = dict(ahead_without_head)
    wrong_repository_url["url"] = compare_url("other/repository", source, head)
    expect_failure(
        lambda: build_evidence(
            repository=repository,
            release_set_id="release-set-v2-sha256-" + "f" * 64,
            source_sha=source,
            branch=branch,
            comparison=wrong_repository_url,
        ),
        "URL is not bound",
    )

    mismatched_explicit_head = dict(ahead_without_head)
    mismatched_explicit_head["head_commit"] = {"sha": "3" * 40}
    expect_failure(
        lambda: build_evidence(
            repository=repository,
            release_set_id="release-set-v2-sha256-" + "0" * 64,
            source_sha=source,
            branch=branch,
            comparison=mismatched_explicit_head,
        ),
        "differs from observed protected-main SHA",
    )

    malformed_source = "1" * 39
    expect_failure(
        lambda: build_evidence(
            repository=repository,
            release_set_id="release-set-v2-sha256-" + "1" * 64,
            source_sha=malformed_source,
            branch=branch,
            comparison=ahead_without_head,
        ),
        "source SHA must be an exact",
    )

    print("AR-11 accepted-source evidence collection adapter self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    result.add_argument("--self-test", action="store_true")
    result.add_argument("--repository")
    result.add_argument("--release-set-id")
    result.add_argument("--source-sha")
    result.add_argument("--branch-json", type=Path)
    result.add_argument("--compare-json", type=Path)
    result.add_argument("--output", type=Path)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        if args.branch_json is None or args.compare_json is None or args.output is None:
            fail("branch-json, compare-json, and output are required")
        write_evidence(args)
        return 0
    except EvidenceError as error:
        print(f"accepted-source evidence error: {error}", flush=True)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
