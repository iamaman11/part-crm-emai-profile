#!/usr/bin/env python3
"""Build one fail-closed patched Camoufox candidate outside customer hosts."""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "runtime/camouhost/camoufox-patch-lock.json"
VERIFY = ROOT / "scripts/check-camoufox-webgl-patch.py"
SHA256_HEX = 64


def canonical(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode("utf-8")


def run(*args: str, cwd: Path) -> None:
    subprocess.run(args, cwd=cwd, check=True)


def sha256(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        raise SystemExit(f"candidate input is not a regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_lock() -> dict[str, Any]:
    raw = LOCK.read_bytes()
    value = json.loads(raw.decode("utf-8"))
    if canonical(value) != raw:
        raise SystemExit("Camoufox patch lock is not canonical JSON")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream-root", type=Path, required=True)
    parser.add_argument("--target", choices=("linux", "windows"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--provenance-output", type=Path, required=True)
    parser.add_argument("--build-source-commit", required=True)
    args = parser.parse_args()

    if len(args.build_source_commit) != 40 or any(ch not in "0123456789abcdef" for ch in args.build_source_commit):
        raise SystemExit("build source commit must be an exact lowercase Git SHA")

    lock = load_lock()
    source = args.upstream_root.resolve(strict=True)
    actual = subprocess.check_output(("git", "rev-parse", "HEAD"), cwd=source, text=True).strip()
    if actual != lock["browser"]["release_commit"]:
        raise SystemExit("candidate source commit differs from pinned runtime patch lock")

    run(sys.executable, str(VERIFY), "--upstream-root", str(source), cwd=ROOT)
    run("patch", "--batch", "-p1", "-i", str(ROOT / lock["patch"]["path"]), cwd=source)
    run("make", "fetch", cwd=source)
    run("make", "setup-minimal", cwd=source)
    run("make", "mozbootstrap", cwd=source)
    run("python3", "multibuild.py", "--target", args.target, "--arch", "x86_64", cwd=source)

    candidates = sorted(source.glob(f"dist/camoufox-*-{args.target}.x86_64.zip"))
    if len(candidates) != 1 or candidates[0].is_symlink() or not candidates[0].is_file():
        raise SystemExit("candidate build did not produce exactly one regular browser archive")
    if args.output.exists() or args.output.is_symlink():
        raise SystemExit("candidate output already exists")
    if args.provenance_output.exists() or args.provenance_output.is_symlink():
        raise SystemExit("candidate provenance output already exists")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.provenance_output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(candidates[0], args.output)
    artifact_sha = sha256(args.output)
    if len(artifact_sha) != SHA256_HEX:
        raise SystemExit("candidate artifact SHA-256 is invalid")

    provenance = {
        "schema_version": 1,
        "kind": "CAMOUFOX_PATCHED_CANDIDATE_PROVENANCE",
        "upstream_repository": lock["browser"]["repository"],
        "upstream_commit": actual,
        "camoufox_version": lock["browser"]["version"],
        "patch_path": lock["patch"]["path"],
        "patch_sha256": lock["patch"]["sha256"],
        "upstream_target": lock["patch"]["upstream_target"],
        "upstream_target_sha256": lock["patch"]["upstream_target_sha256"],
        "target_operating_system": args.target,
        "target_architecture": "x86_64",
        "artifact_sha256": artifact_sha,
        "build_source_commit": args.build_source_commit,
    }
    args.provenance_output.write_bytes(canonical(provenance))
    print(canonical(provenance).decode("utf-8"), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
