#!/usr/bin/env python3
"""Offline fixtures proving the GitHub review shell only acquires raw observations."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "check-external-review-attestations.py"
SPEC = importlib.util.spec_from_file_location("external_review_attestations", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load external review attestation module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)

observe_tree = MODULE.observe_tree

REPOSITORY = "acme/profile-platform"
TIMESTAMP = "2026-08-06T14:40:00Z"


class Handler(BaseHTTPRequestHandler):
    responses: dict[str, tuple[int, dict[str, Any]]] = {}
    requests_seen: list[str] = []

    def do_GET(self) -> None:  # noqa: N802 - stdlib callback name
        self.__class__.requests_seen.append(self.path)
        status, payload = self.__class__.responses.get(
            self.path,
            (404, {"message": "Not Found"}),
        )
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:
        del format, args


def record_data(evidence_id: str, reference: str | None) -> dict[str, Any]:
    value: dict[str, Any] = {
        "artifact_digests_sha256": ["11" * 32],
        "checks": [{"code": "synthetic_check", "outcome": "pass"}],
        "evidence_id": evidence_id,
        "gate": "product_license",
        "limitations": ["synthetic_fixture_only"],
        "observed_at": "2026-08-06T14:39:00Z",
        "references": ["review-report:sha256:" + "22" * 32],
        "schema_version": 1,
        "scope": {
            "environment": "none",
            "subject_id": "synthetic-attestation-fixture",
        },
        "status": "passed" if reference is not None else "pending",
    }
    if reference is not None:
        value["review"] = {
            "github_login": "expected-reviewer",
            "review_reference": reference,
            "reviewed_at": TIMESTAMP,
        }
    return value


def write_records(root: Path, values: list[dict[str, Any]]) -> None:
    directory = root / "evidence" / "external" / "records"
    directory.mkdir(parents=True)
    for value in values:
        path = directory / f"{value['evidence_id']}.json"
        path.write_text(
            json.dumps(value, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )


def provider_payload(body: str, login: str, kind: str, timestamp: str) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "body": body,
        "user": {"login": login},
    }
    if kind == "pull_review":
        payload["submitted_at"] = timestamp
    else:
        payload["updated_at"] = timestamp
    return payload


def observed_record(observation: dict[str, Any], evidence_id: str) -> dict[str, Any]:
    for item in observation["records"]:
        if item["record"].get("evidence_id") == evidence_id:
            return item
    raise AssertionError(f"missing observed record {evidence_id}")


def main() -> int:
    Handler.responses = {}
    Handler.requests_seen = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    api_base = f"http://127.0.0.1:{server.server_port}"

    try:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            records = [
                record_data(
                    "ev-20260806-issue-comment",
                    "https://github.com/acme/profile-platform/issues/9#issuecomment-101",
                ),
                record_data(
                    "ev-20260806-pull-review",
                    "https://github.com/acme/profile-platform/pull/7#pullrequestreview-202",
                ),
                record_data(
                    "ev-20260806-review-comment",
                    "https://github.com/acme/profile-platform/pull/7#discussion_r303",
                ),
                record_data("ev-20260806-pending", None),
            ]
            write_records(root, records)
            Handler.responses = {
                "/repos/acme/profile-platform/issues/comments/101": (
                    200,
                    provider_payload("raw issue body", "issue-reviewer", "issue_comment", TIMESTAMP),
                ),
                "/repos/acme/profile-platform/pulls/7/reviews/202": (
                    200,
                    provider_payload("raw review body", "pull-reviewer", "pull_review", TIMESTAMP),
                ),
                "/repos/acme/profile-platform/pulls/comments/303": (
                    200,
                    provider_payload("raw comment body", "comment-reviewer", "review_comment", TIMESTAMP),
                ),
            }

            observation = observe_tree(root, REPOSITORY, api_base, None)
            assert observation["kind"] == "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION"
            assert observation["repository"] == REPOSITORY
            assert len(observation["records"]) == 4
            assert sorted(Handler.requests_seen) == sorted(Handler.responses)

            issue = observed_record(observation, "ev-20260806-issue-comment")
            assert issue["review_repository"] == REPOSITORY
            assert issue["provider_object"] == {
                "available": True,
                "login": "issue-reviewer",
                "body": "raw issue body",
                "effective_timestamp": TIMESTAMP,
            }
            pull_review = observed_record(observation, "ev-20260806-pull-review")
            assert pull_review["provider_object"]["login"] == "pull-reviewer"
            assert pull_review["provider_object"]["effective_timestamp"] == TIMESTAMP
            review_comment = observed_record(observation, "ev-20260806-review-comment")
            assert review_comment["provider_object"]["body"] == "raw comment body"

            pending = observed_record(observation, "ev-20260806-pending")
            assert pending["review_repository"] is None
            assert pending["review_reference"] is None
            assert pending["provider_object"] is None

            # Semantic drift is captured verbatim. Rust, not this shell, decides validity.
            issue_path = "/repos/acme/profile-platform/issues/comments/101"
            Handler.responses[issue_path] = (
                200,
                provider_payload(
                    "external-evidence-review-v1\nwrong=true",
                    "different-reviewer",
                    "issue_comment",
                    "2026-08-06T14:41:00Z",
                ),
            )
            drift = observe_tree(root, REPOSITORY, api_base, None)
            drifted = observed_record(drift, "ev-20260806-issue-comment")["provider_object"]
            assert drifted["body"] == "external-evidence-review-v1\nwrong=true"
            assert drifted["login"] == "different-reviewer"
            assert drifted["effective_timestamp"] == "2026-08-06T14:41:00Z"

            Handler.responses[issue_path] = (404, {"message": "Not Found"})
            missing = observe_tree(root, REPOSITORY, api_base, None)
            assert observed_record(missing, "ev-20260806-issue-comment")["provider_object"] == {
                "available": False,
                "login": None,
                "body": None,
                "effective_timestamp": None,
            }

        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            foreign = record_data(
                "ev-20260806-foreign-review",
                "https://github.com/other/repository/issues/1#issuecomment-1",
            )
            write_records(root, [foreign])
            Handler.responses = {
                "/repos/other/repository/issues/comments/1": (
                    200,
                    provider_payload("foreign body", "foreign-reviewer", "issue_comment", TIMESTAMP),
                )
            }
            observation = observe_tree(root, REPOSITORY, api_base, None)
            observed = observed_record(observation, "ev-20260806-foreign-review")
            assert observed["review_repository"] == "other/repository"
            assert observed["provider_object"]["available"] is True
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

    print("external review raw-observation fixtures passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
