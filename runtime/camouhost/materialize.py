#!/usr/bin/env python3
"""Candidate-generation identity materializer owned by the supported Camouhost runtime."""

from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path

import real

IDENTITY_NAME = "camoufox-identity.json"
IDENTITY_SCHEMA_VERSION = 1


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode("utf-8")


def write_new(path: Path, payload: bytes) -> None:
    if path.exists() or path.is_symlink():
        raise real.RuntimeContractError("candidate identity evidence already exists")
    with path.open("xb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())


def materialize(root: Path) -> dict[str, str]:
    report = real.materialize_candidate_identity(root)
    lock, runtime_lock_sha256 = real.load_runtime_lock()
    if runtime_lock_sha256 != report["runtime_lock_sha256"]:
        raise real.RuntimeContractError("runtime lock changed during identity materialization")
    root = root.resolve(strict=True)
    identity = {
        "browser": lock["browser"],
        "components": lock["components"],
        "fingerprint_config_schema": lock["fingerprint_config_schema"],
        "fingerprint_config_sha256": report["fingerprint_config_sha256"],
        "fingerprint_policy_version": report["fingerprint_policy_version"],
        "profile_stable_probe_sha256": report["profile_stable_probe_sha256"],
        "runtime_lock_sha256": report["runtime_lock_sha256"],
        "schema_version": IDENTITY_SCHEMA_VERSION,
    }
    payload = canonical_json(identity)
    write_new(root / IDENTITY_NAME, payload)
    return {
        **report,
        "identity_file_sha256": hashlib.sha256(payload).hexdigest(),
    }


def main() -> int:
    if len(sys.argv) != 2:
        print("one candidate generation root is required", file=sys.stderr)
        return 2
    try:
        report = materialize(Path(sys.argv[1]))
    except (OSError, real.RuntimeContractError):
        print("candidate generation identity materialization failed", file=sys.stderr)
        return 7
    print(json.dumps(report, ensure_ascii=True, separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
