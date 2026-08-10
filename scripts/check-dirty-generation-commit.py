#!/usr/bin/env python3
"""Enforce Phase 2F Bridge dirty-generation publish/commit ordering."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FLOW = Path("apps/profile-bridge/src/dirty_generation_commit.rs")
LIB = Path("apps/profile-bridge/src/lib.rs")


def read(root: Path, relative: Path) -> str:
    return (root / relative).read_text(encoding="utf-8")


def production(source: str) -> str:
    return source.split("#[cfg(test)]", 1)[0]


def flow_body(source: str) -> str:
    marker = "pub async fn publish_and_commit_dirty_generation"
    start = source.find(marker)
    if start < 0:
        return ""
    return source[start:]


def request_body(source: str) -> str:
    marker = "pub struct DirtyGenerationCommitRequest {"
    start = source.find(marker)
    if start < 0:
        return ""
    remainder = source[start + len(marker) :]
    end = remainder.find("\n}\n\nimpl DirtyGenerationCommitRequest")
    return remainder if end < 0 else remainder[:end]


def errors(root: Path) -> list[str]:
    result: list[str] = []
    source = production(read(root, FLOW))
    lib = read(root, LIB)
    body = flow_body(source)
    request = request_body(source)

    if "pub mod dirty_generation_commit;" not in lib:
        result.append("Bridge dirty-generation commit orchestration must be exported")
    for required in [
        "validate_execution(scope, proof, prepared)",
        "publish_prepared_dirty_generation(scope, prepared, upload, verifier)",
        "DirtyGenerationCommitRequest::from_published(proof, &published)",
        ".commit_dirty_generation(scope, &request)",
        "metadata.base_generation_id() != Some(proof.generation_id())",
        "lease.status() != LeaseStatus::Active",
    ]:
        if required not in source:
            result.append(f"Bridge dirty-generation commit boundary missing: {required}")

    publish_index = body.find("publish_prepared_dirty_generation")
    commit_index = body.find(".commit_dirty_generation")
    if publish_index < 0 or commit_index < 0 or publish_index >= commit_index:
        result.append("Bridge must publish and exact-verify immutable generation before metadata commit")

    for forbidden in [
        "mark_synced(",
        "workspace_lock.release",
        "coordinator.release",
        "std::fs::remove",
        "reqwest",
        "ureq",
        "R2GenerationObjects",
        "R2_PROFILES_BINDING",
    ]:
        if forbidden in body:
            result.append(f"Bridge publish/commit flow must not mutate ownership or bind provider transport: {forbidden}")

    for forbidden_field in [
        "tenant_id",
        "device_id",
        "observed_at",
        "executed_at",
        "expected_job_version",
        "expected_profile_version",
        "coordinator_version",
        "coordinator_sequence",
    ]:
        if forbidden_field in request:
            result.append(
                f"Bridge commit request must not carry server-derived authority field: {forbidden_field}"
            )

    return result


def self_test() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        for relative in [FLOW, LIB]:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(read(ROOT, relative), encoding="utf-8")
        flow = root / FLOW
        text = flow.read_text(encoding="utf-8")
        text = text.replace(
            "let published = publish_prepared_dirty_generation(scope, prepared, upload, verifier)",
            "let published = commit.commit_dirty_generation(scope, &DirtyGenerationCommitRequest::from_published(proof, unreachable!())).await.unwrap();\n    let published = publish_prepared_dirty_generation(scope, prepared, upload, verifier)",
            1,
        )
        flow.write_text(text, encoding="utf-8")
        detected = errors(root)
        if not any("publish and exact-verify" in item for item in detected):
            raise AssertionError("commit-before-publish negative fixture unexpectedly passed")
    print("Phase 2F Bridge dirty-generation ordering negative fixture rejected as expected.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    detected = errors(ROOT)
    if detected:
        for item in detected:
            print(item)
        return 1
    print("Phase 2F Bridge dirty-generation publish-before-commit boundary passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
