#!/usr/bin/env python3
"""Verify a patched Camoufox candidate against the canonical patch lock."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK = ROOT / "runtime/camouhost/camoufox-patch-lock.json"
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")


class CandidateError(ValueError):
    pass


def canonical(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode("utf-8")


def load_canonical(path: Path) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        raise CandidateError(f"required JSON file is missing/not regular: {path}")
    raw = path.read_bytes()
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise CandidateError(f"invalid JSON: {path}") from error
    if not isinstance(value, dict) or canonical(value) != raw:
        raise CandidateError(f"JSON must be canonical: {path}")
    return value


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        raise CandidateError("candidate archive is missing/not regular")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify(archive: Path, provenance_path: Path, lock_path: Path, target: str, build_source_commit: str | None) -> dict[str, Any]:
    lock = load_canonical(lock_path)
    provenance = load_canonical(provenance_path)
    required = {
        "schema_version", "kind", "upstream_repository", "upstream_commit", "camoufox_version",
        "patch_path", "patch_sha256", "upstream_target", "upstream_target_sha256",
        "target_operating_system", "target_architecture", "artifact_sha256", "build_source_commit",
    }
    if set(provenance) != required:
        raise CandidateError("candidate provenance shape is unsupported")
    if provenance["schema_version"] != 1 or provenance["kind"] != "CAMOUFOX_PATCHED_CANDIDATE_PROVENANCE":
        raise CandidateError("candidate provenance identity is unsupported")
    expected = {
        "upstream_repository": lock["browser"]["repository"],
        "upstream_commit": lock["browser"]["release_commit"],
        "camoufox_version": lock["browser"]["version"],
        "patch_path": lock["patch"]["path"],
        "patch_sha256": lock["patch"]["sha256"],
        "upstream_target": lock["patch"]["upstream_target"],
        "upstream_target_sha256": lock["patch"]["upstream_target_sha256"],
        "target_operating_system": target,
        "target_architecture": "x86_64",
    }
    for key, value in expected.items():
        if provenance.get(key) != value:
            raise CandidateError(f"candidate provenance differs from lock: {key}")
    artifact_sha = provenance.get("artifact_sha256")
    source_sha = provenance.get("build_source_commit")
    if not isinstance(artifact_sha, str) or SHA256_RE.fullmatch(artifact_sha) is None:
        raise CandidateError("candidate artifact SHA-256 is invalid")
    if not isinstance(source_sha, str) or GIT_SHA_RE.fullmatch(source_sha) is None:
        raise CandidateError("candidate build source commit is invalid")
    if build_source_commit is not None and source_sha != build_source_commit:
        raise CandidateError("candidate build source commit differs from expected source")
    if sha256_file(archive) != artifact_sha:
        raise CandidateError("candidate archive SHA-256 differs from provenance")
    return provenance


def self_test() -> None:
    import tempfile
    with tempfile.TemporaryDirectory(prefix="camoufox-candidate-verify-") as directory:
        root = Path(directory)
        archive = root / "candidate.zip"
        archive.write_bytes(b"candidate")
        lock = load_canonical(DEFAULT_LOCK)
        provenance = {
            "schema_version": 1,
            "kind": "CAMOUFOX_PATCHED_CANDIDATE_PROVENANCE",
            "upstream_repository": lock["browser"]["repository"],
            "upstream_commit": lock["browser"]["release_commit"],
            "camoufox_version": lock["browser"]["version"],
            "patch_path": lock["patch"]["path"],
            "patch_sha256": lock["patch"]["sha256"],
            "upstream_target": lock["patch"]["upstream_target"],
            "upstream_target_sha256": lock["patch"]["upstream_target_sha256"],
            "target_operating_system": "windows",
            "target_architecture": "x86_64",
            "artifact_sha256": sha256_file(archive),
            "build_source_commit": "0" * 40,
        }
        provenance_path = root / "provenance.json"
        provenance_path.write_bytes(canonical(provenance))
        verify(archive, provenance_path, DEFAULT_LOCK, "windows", "0" * 40)
        archive.write_bytes(b"tampered")
        try:
            verify(archive, provenance_path, DEFAULT_LOCK, "windows", "0" * 40)
        except CandidateError:
            pass
        else:
            raise CandidateError("tampered archive negative self-test unexpectedly passed")
        archive.write_bytes(b"candidate")
        changed = dict(provenance)
        changed["patch_sha256"] = "1" * 64
        provenance_path.write_bytes(canonical(changed))
        try:
            verify(archive, provenance_path, DEFAULT_LOCK, "windows", "0" * 40)
        except CandidateError:
            pass
        else:
            raise CandidateError("wrong patch digest negative self-test unexpectedly passed")
    print("Camoufox patched candidate verifier self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--provenance", type=Path)
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--target", choices=("linux", "windows"))
    parser.add_argument("--build-source-commit")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        if args.archive is None or args.provenance is None or args.target is None:
            parser.error("--archive, --provenance and --target are required unless --self-test is used")
        provenance = verify(args.archive, args.provenance, args.lock, args.target, args.build_source_commit)
        print(canonical(provenance).decode("utf-8"), end="")
        return 0
    except (CandidateError, OSError, KeyError, TypeError, ValueError) as error:
        print(f"Camoufox candidate verification error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
