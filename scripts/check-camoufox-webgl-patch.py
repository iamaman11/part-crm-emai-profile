#!/usr/bin/env python3
"""Fail-closed verification for the pinned Camoufox native WebGL correction.

This is a build-time guard only. It binds the repository patch to one exact
upstream source tree before any candidate runtime is compiled; it is never
installed or executed on a customer host.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "runtime/camouhost/camoufox-patch-lock.json"
SHA256 = __import__("re").compile(r"^[0-9a-f]{64}$")
COMMIT = __import__("re").compile(r"^[0-9a-f]{40}$")


class PatchContractError(ValueError):
    pass


def fail(message: str) -> None:
    raise PatchContractError(message)


def canonical(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode()


def digest(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail("patch contract input is not a regular file")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def relative(path: str) -> Path:
    candidate = Path(path)
    if not path or candidate.is_absolute() or ".." in candidate.parts or "." in candidate.parts:
        fail("patch contract path is unsafe")
    return candidate


def load_lock(path: Path = LOCK_PATH) -> dict[str, Any]:
    raw = path.read_bytes()
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PatchContractError("patch lock is not JSON") from error
    if not isinstance(value, dict) or raw != canonical(value):
        fail("patch lock must be canonical JSON")
    if set(value) != {"browser", "patch", "schema_version"} or value["schema_version"] != 1:
        fail("patch lock schema is invalid")
    browser, patch = value["browser"], value["patch"]
    if not isinstance(browser, dict) or set(browser) != {"release_commit", "repository", "version"}:
        fail("patch lock browser identity is invalid")
    if (not isinstance(browser["repository"], str) or browser["repository"] != "daijro/camoufox"
            or not isinstance(browser["release_commit"], str) or COMMIT.fullmatch(browser["release_commit"]) is None
            or not isinstance(browser["version"], str) or not browser["version"]):
        fail("patch lock browser identity is invalid")
    required = {"path", "sha256", "upstream_target", "upstream_target_sha256"}
    if not isinstance(patch, dict) or set(patch) != required:
        fail("patch lock patch identity is invalid")
    for key in ("path", "upstream_target"):
        if not isinstance(patch[key], str):
            fail("patch lock path is invalid")
        relative(patch[key])
    for key in ("sha256", "upstream_target_sha256"):
        if not isinstance(patch[key], str) or SHA256.fullmatch(patch[key]) is None:
            fail("patch lock digest is invalid")
    return value


def verify(upstream_root: Path) -> None:
    lock = load_lock()
    patch = lock["patch"]
    patch_path = ROOT / relative(patch["path"])
    if digest(patch_path) != patch["sha256"]:
        fail("repository WebGL patch digest mismatch")
    target = upstream_root / relative(patch["upstream_target"])
    if digest(target) != patch["upstream_target_sha256"]:
        fail("pinned upstream WebGL target digest mismatch")
    result = subprocess.run(
        ["patch", "--dry-run", "--batch", "-p1", "-i", str(patch_path)],
        cwd=upstream_root,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        fail("pinned WebGL patch does not apply cleanly")
    print("Camoufox WebGL checked-array patch contract passed.")


def self_test() -> None:
    lock = load_lock()
    if lock["browser"]["release_commit"] != "5d06ec1629ac7843508f1e683f83e404fde8db76":
        fail("unexpected beta.30 source pin")
    if lock["patch"]["sha256"] != digest(ROOT / lock["patch"]["path"]):
        fail("patch digest self-test failed")
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        target = root / lock["patch"]["upstream_target"]
        target.parent.mkdir(parents=True)
        target.write_text("wrong\n", encoding="utf-8")
        try:
            verify(root)
        except PatchContractError:
            pass
        else:
            fail("wrong upstream source negative self-test unexpectedly passed")
    print("Camoufox WebGL patch contract negative self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream-root", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
        elif args.upstream_root:
            verify(args.upstream_root)
        else:
            fail("either --self-test or --upstream-root is required")
        return 0
    except PatchContractError as error:
        print(f"Camoufox patch contract error: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
