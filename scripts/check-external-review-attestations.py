#!/usr/bin/env python3
"""Observe external-evidence GitHub review objects for typed Rust verification."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlsplit
from urllib.request import Request, urlopen

CLAIM_DOMAIN = "external-evidence-review-v1"
OBSERVATION_KIND = "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION"
MAX_API_RESPONSE_BYTES = 256 * 1024
ISSUE_COMMENT_RE = re.compile(r"issuecomment-([0-9]+)\Z")
PULL_REVIEW_RE = re.compile(r"pullrequestreview-([0-9]+)\Z")
REVIEW_COMMENT_RE = re.compile(r"discussion_r([0-9]+)\Z")
LOGIN_RE = re.compile(r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?\Z")
REPOSITORY_RE = re.compile(r"([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)\Z")


class AttestationError(ValueError):
    pass


@dataclass(frozen=True)
class TerminalRecord:
    path: Path
    data: dict[str, Any]
    evidence_id: str
    gate: str
    status: str
    github_login: str
    review_reference: str
    reviewed_at: str


@dataclass(frozen=True)
class ReviewTarget:
    owner: str
    repository: str
    kind: str
    object_id: int
    pull_number: int | None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY"))
    parser.add_argument("--api-base", default="https://api.github.com")
    parser.add_argument(
        "--token",
        default=os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN"),
    )
    parser.add_argument("--output-observation-json", type=Path)
    parser.add_argument("--print-claims", action="store_true")
    return parser.parse_args()


def require_string(value: Any, where: str) -> str:
    if not isinstance(value, str) or not value:
        raise AttestationError(f"{where}: expected non-empty string")
    return value


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise AttestationError(f"{path}: unreadable JSON") from exc
    if not isinstance(value, dict):
        raise AttestationError(f"{path}: top-level value must be an object")
    return value


def parse_record(path: Path, data: dict[str, Any] | None = None) -> TerminalRecord | None:
    """Parse one terminal record for deterministic operator claim rendering only."""
    if data is None:
        data = load_json(path)
    status = require_string(data.get("status"), f"{path}.status")
    if status == "pending":
        return None
    if status not in {"passed", "failed"}:
        raise AttestationError(f"{path}.status: unsupported terminal status")

    review = data.get("review")
    if not isinstance(review, dict):
        raise AttestationError(f"{path}: terminal evidence requires review")
    login = require_string(review.get("github_login"), f"{path}.review.github_login")
    if not LOGIN_RE.fullmatch(login):
        raise AttestationError(f"{path}.review.github_login: invalid GitHub login")
    return TerminalRecord(
        path=path,
        data=data,
        evidence_id=require_string(data.get("evidence_id"), f"{path}.evidence_id"),
        gate=require_string(data.get("gate"), f"{path}.gate"),
        status=status,
        github_login=login,
        review_reference=require_string(
            review.get("review_reference"), f"{path}.review.review_reference"
        ),
        reviewed_at=require_string(review.get("reviewed_at"), f"{path}.review.reviewed_at"),
    )


def canonical_claim_payload(record: TerminalRecord) -> bytes:
    """Legacy-compatible bounded renderer; Rust owns acceptance semantics."""
    bound_record = {key: value for key, value in record.data.items() if key != "review"}
    payload = {"domain": CLAIM_DOMAIN, "record": bound_record}
    return json.dumps(
        payload,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def claim_sha256(record: TerminalRecord) -> str:
    return hashlib.sha256(canonical_claim_payload(record)).hexdigest()


def claim_body(record: TerminalRecord) -> str:
    return "\n".join(
        (
            CLAIM_DOMAIN,
            f"evidence_id={record.evidence_id}",
            f"gate={record.gate}",
            f"status={record.status}",
            f"claim_sha256={claim_sha256(record)}",
        )
    )


def parse_repository(value: str | None) -> tuple[str, str]:
    if value is None:
        raise AttestationError("repository is required to acquire GitHub review observations")
    match = REPOSITORY_RE.fullmatch(value)
    if match is None:
        raise AttestationError("repository must use owner/name form")
    return match.group(1), match.group(2)


def parse_review_target(reference: str) -> ReviewTarget:
    """Parse only enough provider addressing data to perform the GET observation."""
    parsed = urlsplit(reference)
    if parsed.scheme != "https" or parsed.netloc != "github.com" or parsed.query:
        raise AttestationError(f"invalid GitHub review reference: {reference}")
    parts = [part for part in parsed.path.split("/") if part]
    if len(parts) != 4 or parts[2] not in {"issues", "pull"} or not parts[3].isdigit():
        raise AttestationError(
            f"review reference must identify one issue or pull request: {reference}"
        )
    owner, repository = parts[0], parts[1]

    issue_match = ISSUE_COMMENT_RE.fullmatch(parsed.fragment)
    if issue_match is not None:
        return ReviewTarget(owner, repository, "issue_comment", int(issue_match.group(1)), None)

    if parts[2] != "pull":
        raise AttestationError("pull request review references must use a /pull/<number> URL")
    pull_number = int(parts[3])
    review_match = PULL_REVIEW_RE.fullmatch(parsed.fragment)
    if review_match is not None:
        return ReviewTarget(
            owner,
            repository,
            "pull_review",
            int(review_match.group(1)),
            pull_number,
        )
    comment_match = REVIEW_COMMENT_RE.fullmatch(parsed.fragment)
    if comment_match is not None:
        return ReviewTarget(
            owner,
            repository,
            "review_comment",
            int(comment_match.group(1)),
            pull_number,
        )
    raise AttestationError(f"unsupported GitHub review fragment: {reference}")


def api_url(api_base: str, target: ReviewTarget) -> str:
    base = api_base.rstrip("/")
    owner = quote(target.owner, safe="")
    repository = quote(target.repository, safe="")
    if target.kind == "issue_comment":
        return f"{base}/repos/{owner}/{repository}/issues/comments/{target.object_id}"
    if target.kind == "pull_review":
        assert target.pull_number is not None
        return (
            f"{base}/repos/{owner}/{repository}/pulls/{target.pull_number}"
            f"/reviews/{target.object_id}"
        )
    if target.kind == "review_comment":
        return f"{base}/repos/{owner}/{repository}/pulls/comments/{target.object_id}"
    raise AttestationError(f"unsupported review target kind: {target.kind}")


def fetch_json(url: str, token: str | None) -> tuple[bool, dict[str, Any] | None]:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "part-crm-email-profile-external-evidence",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = Request(url, headers=headers, method="GET")
    try:
        with urlopen(request, timeout=20) as response:
            raw = response.read(MAX_API_RESPONSE_BYTES + 1)
    except HTTPError:
        # Availability is an observed provider fact. Rust decides whether absence is valid.
        return False, None
    except URLError as exc:
        raise AttestationError(f"GitHub API request failed for {url}: {exc.reason}") from exc
    if len(raw) > MAX_API_RESPONSE_BYTES:
        raise AttestationError(f"GitHub API response exceeded {MAX_API_RESPONSE_BYTES} bytes")
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise AttestationError(f"GitHub API returned invalid JSON for {url}") from exc
    if not isinstance(value, dict):
        raise AttestationError(f"GitHub API returned a non-object for {url}")
    return True, value


def optional_string(value: Any) -> str | None:
    return value if isinstance(value, str) else None


def provider_observation(
    target: ReviewTarget,
    available: bool,
    payload: dict[str, Any] | None,
) -> dict[str, Any]:
    if not available or payload is None:
        return {
            "available": False,
            "login": None,
            "body": None,
            "effective_timestamp": None,
        }
    user = payload.get("user")
    login = optional_string(user.get("login")) if isinstance(user, dict) else None
    if target.kind in {"issue_comment", "review_comment"}:
        timestamp = optional_string(payload.get("updated_at"))
    else:
        timestamp = optional_string(payload.get("submitted_at"))
    return {
        "available": True,
        "login": login,
        "body": optional_string(payload.get("body")),
        "effective_timestamp": timestamp,
    }


def load_all_records(root: Path) -> list[tuple[Path, dict[str, Any]]]:
    records_dir = root / "evidence" / "external" / "records"
    if not records_dir.is_dir():
        raise AttestationError(f"missing records directory: {records_dir}")
    return [(path, load_json(path)) for path in sorted(records_dir.glob("*.json"))]


def review_reference_from_record(data: dict[str, Any]) -> str | None:
    review = data.get("review")
    if not isinstance(review, dict):
        return None
    reference = review.get("review_reference")
    return reference if isinstance(reference, str) and reference else None


def observe_tree(
    root: Path,
    repository_value: str | None,
    api_base: str,
    token: str | None,
) -> dict[str, Any]:
    repository = parse_repository(repository_value)
    observed_records: list[dict[str, Any]] = []
    for _path, data in load_all_records(root):
        reference = review_reference_from_record(data)
        if reference is None:
            observed_records.append(
                {
                    "record": data,
                    "review_repository": None,
                    "review_reference": None,
                    "provider_object": None,
                }
            )
            continue
        target = parse_review_target(reference)
        available, payload = fetch_json(api_url(api_base, target), token)
        observed_records.append(
            {
                "record": data,
                "review_repository": f"{target.owner}/{target.repository}",
                "review_reference": reference,
                "provider_object": provider_observation(target, available, payload),
            }
        )
    return {
        "schema_version": 1,
        "kind": OBSERVATION_KIND,
        "repository": f"{repository[0]}/{repository[1]}",
        "records": observed_records,
    }


def write_observation(path: Path, observation: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(observation, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def load_active_terminal_records(root: Path) -> list[TerminalRecord]:
    """Retained only for non-authoritative operator claim rendering."""
    entries: list[tuple[Path, dict[str, Any], str]] = []
    superseded: set[str] = set()
    for path, data in load_all_records(root):
        evidence_id = require_string(data.get("evidence_id"), f"{path}.evidence_id")
        entries.append((path, data, evidence_id))
        supersedes = data.get("supersedes")
        if supersedes is not None:
            superseded.add(require_string(supersedes, f"{path}.supersedes"))

    terminal: list[TerminalRecord] = []
    for path, data, evidence_id in entries:
        if evidence_id in superseded:
            continue
        record = parse_record(path, data)
        if record is not None:
            terminal.append(record)
    return terminal


def print_claims(root: Path) -> int:
    records = load_active_terminal_records(root)
    for record in records:
        print(f"# {record.path}")
        print(claim_body(record))
        print()
    print(f"printed {len(records)} active terminal review claim(s)")
    return 0


def main() -> int:
    args = parse_args()
    try:
        root = args.root.resolve()
        if args.print_claims:
            if args.output_observation_json is not None:
                raise AttestationError(
                    "--print-claims and --output-observation-json are mutually exclusive"
                )
            return print_claims(root)
        if args.output_observation_json is None:
            raise AttestationError(
                "--output-observation-json is required for the observer path"
            )
        observation = observe_tree(
            root,
            args.repository,
            args.api_base,
            args.token,
        )
        write_observation(args.output_observation_json, observation)
        print(
            "external review observation captured: "
            f"{len(observation['records'])} repository record(s)"
        )
        return 0
    except (OSError, AttestationError) as exc:
        print(f"external review observation failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
