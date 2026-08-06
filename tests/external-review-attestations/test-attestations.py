#!/usr/bin/env python3
"""Offline HTTP fixtures for external GitHub review attestation verification."""

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

AttestationError = MODULE.AttestationError
claim_body = MODULE.claim_body
load_terminal_records = MODULE.load_terminal_records
verify_tree = MODULE.verify_tree

REPOSITORY = "acme/profile-platform"
REVIEWER = "reviewer-one"
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


def terminal_data(
    evidence_id: str,
    gate: str,
    status: str,
    reference: str,
    reviewed_at: str = TIMESTAMP,
    github_login: str = REVIEWER,
) -> dict[str, Any]:
    return {
        "artifact_digests_sha256": ["11" * 32],
        "checks": [{"code": "synthetic_check", "outcome": "pass"}],
        "evidence_id": evidence_id,
        "gate": gate,
        "limitations": ["synthetic_fixture_only"],
        "observed_at": "2026-08-06T14:39:00Z",
        "references": ["review-report:sha256:" + "22" * 32],
        "review": {
            "github_login": github_login,
            "review_reference": reference,
            "reviewed_at": reviewed_at,
        },
        "schema_version": 1,
        "scope": {
            "environment": "none",
            "subject_id": "synthetic-attestation-fixture",
        },
        "status": status,
    }


def pending_data() -> dict[str, Any]:
    return {
        "artifact_digests_sha256": [],
        "checks": [],
        "evidence_id": "ev-20260806-pending-attestation",
        "gate": "product_license",
        "limitations": [],
        "observed_at": "2026-08-06T14:39:00Z",
        "references": ["review-report:sha256:" + "33" * 32],
        "schema_version": 1,
        "scope": {"environment": "none", "subject_id": "pending-fixture"},
        "status": "pending",
    }


def write_records(root: Path, values: list[dict[str, Any]]) -> None:
    directory = root / "evidence" / "external" / "records"
    directory.mkdir(parents=True)
    for value in values:
        path = directory / f"{value['evidence_id']}.json"
        path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def response_for(record: object, kind: str) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "body": claim_body(record),
        "user": {"login": REVIEWER},
    }
    if kind == "pull_review":
        payload["submitted_at"] = TIMESTAMP
    else:
        payload["created_at"] = TIMESTAMP
        payload["updated_at"] = TIMESTAMP
    return payload


def expect_failure(callable_value: object, label: str) -> None:
    try:
        callable_value()  # type: ignore[operator]
    except AttestationError:
        return
    raise AssertionError(f"negative attestation scenario unexpectedly passed: {label}")


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
            records_data = [
                terminal_data(
                    "ev-20260806-issue-comment",
                    "product_license",
                    "passed",
                    "https://github.com/acme/profile-platform/issues/9#issuecomment-101",
                ),
                terminal_data(
                    "ev-20260806-pull-review",
                    "privacy_retention_approval",
                    "passed",
                    "https://github.com/acme/profile-platform/pull/7#pullrequestreview-202",
                ),
                terminal_data(
                    "ev-20260806-review-comment",
                    "independent_security_review",
                    "failed",
                    "https://github.com/acme/profile-platform/pull/7#discussion_r303",
                ),
            ]
            write_records(root, records_data)
            records = load_terminal_records(root)
            by_id = {record.evidence_id: record for record in records}
            Handler.responses = {
                "/repos/acme/profile-platform/issues/comments/101": (
                    200,
                    response_for(by_id["ev-20260806-issue-comment"], "issue_comment"),
                ),
                "/repos/acme/profile-platform/pulls/7/reviews/202": (
                    200,
                    response_for(by_id["ev-20260806-pull-review"], "pull_review"),
                ),
                "/repos/acme/profile-platform/pulls/comments/303": (
                    200,
                    response_for(by_id["ev-20260806-review-comment"], "review_comment"),
                ),
            }
            verify_tree(root, REPOSITORY, api_base, None, False)
            assert len(Handler.requests_seen) == 3

            issue_payload = Handler.responses[
                "/repos/acme/profile-platform/issues/comments/101"
            ][1]
            original_issue_payload = json.loads(json.dumps(issue_payload))

            issue_payload["body"] = "external-evidence-review-v1\nwrong=true"
            expect_failure(
                lambda: verify_tree(root, REPOSITORY, api_base, None, False),
                "wrong claim body",
            )
            Handler.responses[
                "/repos/acme/profile-platform/issues/comments/101"
            ] = (200, json.loads(json.dumps(original_issue_payload)))

            Handler.responses[
                "/repos/acme/profile-platform/issues/comments/101"
            ][1]["user"]["login"] = "another-reviewer"
            expect_failure(
                lambda: verify_tree(root, REPOSITORY, api_base, None, False),
                "wrong author",
            )
            Handler.responses[
                "/repos/acme/profile-platform/issues/comments/101"
            ] = (200, json.loads(json.dumps(original_issue_payload)))

            Handler.responses[
                "/repos/acme/profile-platform/issues/comments/101"
            ][1]["updated_at"] = "2026-08-06T14:41:00Z"
            expect_failure(
                lambda: verify_tree(root, REPOSITORY, api_base, None, False),
                "edited timestamp",
            )
            Handler.responses[
                "/repos/acme/profile-platform/issues/comments/101"
            ] = (404, {"message": "Not Found"})
            expect_failure(
                lambda: verify_tree(root, REPOSITORY, api_base, None, False),
                "deleted review",
            )

        with tempfile.TemporaryDirectory() as temp_dir:
            pending_root = Path(temp_dir)
            write_records(pending_root, [pending_data()])
            before = len(Handler.requests_seen)
            verify_tree(pending_root, None, api_base, None, False)
            assert len(Handler.requests_seen) == before

        with tempfile.TemporaryDirectory() as temp_dir:
            foreign_root = Path(temp_dir)
            write_records(
                foreign_root,
                [
                    terminal_data(
                        "ev-20260806-foreign-review",
                        "product_license",
                        "passed",
                        "https://github.com/other/repository/issues/1#issuecomment-1",
                    )
                ],
            )
            expect_failure(
                lambda: verify_tree(foreign_root, REPOSITORY, api_base, None, False),
                "foreign repository",
            )
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

    print("external review attestation offline fixtures passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
