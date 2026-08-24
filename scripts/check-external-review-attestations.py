#!/usr/bin/env python3
"""Acquire GitHub review objects as raw facts for typed Rust verification."""

from __future__ import annotations

import argparse
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

OBSERVATION_KIND = "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION"
MAX_API_RESPONSE_BYTES = 256 * 1024
ISSUE_COMMENT_RE = re.compile(r"issuecomment-([0-9]+)\Z")
PULL_REVIEW_RE = re.compile(r"pullrequestreview-([0-9]+)\Z")
REVIEW_COMMENT_RE = re.compile(r"discussion_r([0-9]+)\Z")
REPOSITORY_RE = re.compile(r"([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)\Z")


class ObservationError(ValueError):
    pass


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
    parser.add_argument("--output-observation-json", type=Path, required=True)
    return parser.parse_args()


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ObservationError(f"{path}: unreadable JSON") from exc
    if not isinstance(value, dict):
        raise ObservationError(f"{path}: top-level value must be an object")
    return value


def parse_repository(value: str | None) -> tuple[str, str]:
    if value is None:
        raise ObservationError("repository is required for GitHub review observation")
    match = REPOSITORY_RE.fullmatch(value)
    if match is None:
        raise ObservationError("repository must use owner/name form")
    return match.group(1), match.group(2)


def parse_review_target(reference: str) -> ReviewTarget:
    """Decode only provider addressing; Rust owns reference validity semantics."""
    parsed = urlsplit(reference)
    if parsed.scheme != "https" or parsed.netloc != "github.com" or parsed.query:
        raise ObservationError(f"cannot address GitHub review reference: {reference}")
    parts = [part for part in parsed.path.split("/") if part]
    if len(parts) != 4 or parts[2] not in {"issues", "pull"} or not parts[3].isdigit():
        raise ObservationError(f"cannot address GitHub review reference: {reference}")
    owner, repository = parts[0], parts[1]

    issue_match = ISSUE_COMMENT_RE.fullmatch(parsed.fragment)
    if issue_match is not None:
        return ReviewTarget(owner, repository, "issue_comment", int(issue_match.group(1)), None)
    if parts[2] != "pull":
        raise ObservationError(f"cannot address GitHub review reference: {reference}")
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
    raise ObservationError(f"cannot address GitHub review reference: {reference}")


def api_url(api_base: str, target: ReviewTarget) -> str:
    base = api_base.rstrip("/")
    owner = quote(target.owner, safe="")
    repository = quote(target.repository, safe="")
    if target.kind == "issue_comment":
        return f"{base}/repos/{owner}/{repository}/issues/comments/{target.object_id}"
    if target.kind == "pull_review":
        if target.pull_number is None:
            raise ObservationError("pull-review provider address lacks pull number")
        return (
            f"{base}/repos/{owner}/{repository}/pulls/{target.pull_number}"
            f"/reviews/{target.object_id}"
        )
    if target.kind == "review_comment":
        return f"{base}/repos/{owner}/{repository}/pulls/comments/{target.object_id}"
    raise ObservationError(f"unsupported provider address kind: {target.kind}")


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
        return False, None
    except URLError as exc:
        raise ObservationError(f"GitHub API request failed for {url}: {exc.reason}") from exc
    if len(raw) > MAX_API_RESPONSE_BYTES:
        raise ObservationError(f"GitHub API response exceeded {MAX_API_RESPONSE_BYTES} bytes")
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ObservationError(f"GitHub API returned invalid JSON for {url}") from exc
    if not isinstance(value, dict):
        raise ObservationError(f"GitHub API returned a non-object for {url}")
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
    timestamp = (
        optional_string(payload.get("submitted_at"))
        if target.kind == "pull_review"
        else optional_string(payload.get("updated_at"))
    )
    return {
        "available": True,
        "login": login,
        "body": optional_string(payload.get("body")),
        "effective_timestamp": timestamp,
    }


def load_all_records(root: Path) -> list[dict[str, Any]]:
    records_dir = root / "evidence" / "external" / "records"
    if not records_dir.is_dir():
        raise ObservationError(f"missing records directory: {records_dir}")
    return [load_json(path) for path in sorted(records_dir.glob("*.json"))]


def review_reference(data: dict[str, Any]) -> str | None:
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
    for data in load_all_records(root):
        reference = review_reference(data)
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


def main() -> int:
    args = parse_args()
    try:
        observation = observe_tree(
            args.root.resolve(),
            args.repository,
            args.api_base,
            args.token,
        )
        write_observation(args.output_observation_json, observation)
        print(
            "external review raw observation captured: "
            f"{len(observation['records'])} repository record(s)"
        )
        return 0
    except (OSError, ObservationError) as exc:
        print(f"external review observation failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
