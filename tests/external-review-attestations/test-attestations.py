#!/usr/bin/env python3
"""Offline fixtures proving the GitHub review shell only acquires observations."""

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

claim_body = MODULE.claim_body
load_active_terminal_records = MODULE.load_active_terminal_records
observe_tree = MODULE.observe_tree

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
    observed_at: str = "2026-08-06T14:39:00Z",
    supersedes: str | None = None,
) -> dict[str, Any]:
    value: dict[str, Any] = {
        "artifact_digests_sha256": ["11" * 32],
        "checks": [{"code": "synthetic_check", "outcome": "pass"}],
        "evidence_id": evidence_id,
        "gate": gate,
        "limitations": ["synthetic_fixture_only"],
        "observed_at": observed_at,
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
    if supersedes is not None:
        value["supersedes"] = supersedes
    return value


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
        path.write_text(
            json.dumps(value, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )


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


def record_observation(observation: dict[str, Any], evidence_id: str) -> dict[str, Any]:
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
            records = load_active_terminal_records(root)
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
            observation = observe_tree(root, REPOSITORY, api_base, None)
            assert observation["kind"] == "EXTERNAL_REVIEW_ATTESTATION_OBSERVATION"
            assert observation["repository"] == REPOSITORY
            assert len(observation["records"]) == 3
            assert len(Handler.requests_seen) == 3

            issue_path = "/repos/acme/profile-platform/issues/comments/101"
            original_issue_payload = json.loads(json.dumps(Handler.responses[issue_path][1]))

            # The shell must capture semantic drift without deciding whether it is valid.
            Handler.responses[issue_path][1]["body"] = "external-evidence-review-v1\nwrong=true"
            mutated = observe_tree(root, REPOSITORY, api_base, None)
            assert (
                record_observation(mutated, "ev-20260806-issue-comment")["provider_object"]["body"]
                == "external-evidence-review-v1\nwrong=true"
            )

            Handler.responses[issue_path] = (
                200,
                json.loads(json.dumps(original_issue_payload)),
            )
            Handler.responses[issue_path][1]["user"]["login"] = "another-reviewer"
            mutated = observe_tree(root, REPOSITORY, api_base, None)
            assert (
                record_observation(mutated, "ev-20260806-issue-comment")["provider_object"]["login"]
                == "another-reviewer"
            )

            Handler.responses[issue_path] = (
                200,
                json.loads(json.dumps(original_issue_payload)),
            )
            Handler.responses[issue_path][1]["updated_at"] = "2026-08-06T14:41:00Z"
            mutated = observe_tree(root, REPOSITORY, api_base, None)
            assert (
                record_observation(mutated, "ev-20260806-issue-comment")["provider_object"]["effective_timestamp"]
                == "2026-08-06T14:41:00Z"
            )

            Handler.responses[issue_path] = (404, {"message": "Not Found"})
            missing = observe_tree(root, REPOSITORY, api_base, None)
            missing_provider = record_observation(
                missing, "ev-20260806-issue-comment"
            )["provider_object"]
            assert missing_provider == {
                "available": False,
                "login": None,
                "body": None,
                "effective_timestamp": None,
            }

        with tempfile.TemporaryDirectory() as temp_dir:
            pending_root = Path(temp_dir)
            write_records(pending_root, [pending_data()])
            before = len(Handler.requests_seen)
            observation = observe_tree(pending_root, REPOSITORY, api_base, None)
            assert len(Handler.requests_seen) == before
            pending = observation["records"][0]
            assert pending["provider_object"] is None
            assert pending["review_repository"] is None
            assert pending["review_reference"] is None

        with tempfile.TemporaryDirectory() as temp_dir:
            recovery_root = Path(temp_dir)
            old_id = "ev-20260806-old-invalid-review"
            old_record = terminal_data(
                old_id,
                "product_license",
                "passed",
                "https://github.com/acme/profile-platform/issues/9#issuecomment-404",
                observed_at="2026-08-06T14:38:00Z",
            )
            active_record = terminal_data(
                "ev-20260806-active-replacement",
                "product_license",
                "passed",
                "https://github.com/acme/profile-platform/issues/9#issuecomment-505",
                supersedes=old_id,
            )
            write_records(recovery_root, [old_record, active_record])
            active = load_active_terminal_records(recovery_root)
            assert [record.evidence_id for record in active] == [
                "ev-20260806-active-replacement"
            ]
            Handler.responses = {
                "/repos/acme/profile-platform/issues/comments/404": (404, {"message": "Not Found"}),
                "/repos/acme/profile-platform/issues/comments/505": (
                    200,
                    response_for(active[0], "issue_comment"),
                ),
            }
            before = len(Handler.requests_seen)
            observation = observe_tree(recovery_root, REPOSITORY, api_base, None)
            requested = Handler.requests_seen[before:]
            assert requested == [
                "/repos/acme/profile-platform/issues/comments/505",
                "/repos/acme/profile-platform/issues/comments/404",
            ] or requested == [
                "/repos/acme/profile-platform/issues/comments/404",
                "/repos/acme/profile-platform/issues/comments/505",
            ]
            assert len(observation["records"]) == 2
            assert record_observation(observation, old_id)["provider_object"]["available"] is False
            assert (
                record_observation(observation, "ev-20260806-active-replacement")["provider_object"]["available"]
                is True
            )

        with tempfile.TemporaryDirectory() as temp_dir:
            foreign_root = Path(temp_dir)
            foreign = terminal_data(
                "ev-20260806-foreign-review",
                "product_license",
                "passed",
                "https://github.com/other/repository/issues/1#issuecomment-1",
            )
            write_records(foreign_root, [foreign])
            foreign_record = load_active_terminal_records(foreign_root)[0]
            Handler.responses = {
                "/repos/other/repository/issues/comments/1": (
                    200,
                    response_for(foreign_record, "issue_comment"),
                )
            }
            observation = observe_tree(foreign_root, REPOSITORY, api_base, None)
            observed = record_observation(observation, "ev-20260806-foreign-review")
            assert observed["review_repository"] == "other/repository"
            assert observed["provider_object"]["available"] is True
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

    print("external review observation offline fixtures passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
