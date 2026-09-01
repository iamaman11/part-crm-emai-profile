#!/usr/bin/env python3
"""Install a verified Camoufox candidate into the ephemeral CI cache.

This is CI transport only. It is not a customer installer, updater, shipping runtime
source, or publication channel. Candidate identity is verified before extraction.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import stat
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
VERIFY = ROOT / "scripts/verify-camoufox-patched-candidate.py"
PATCH_LOCK = ROOT / "runtime/camouhost/camoufox-patch-lock.json"
MAX_FILES = 500_000
MAX_BYTES = 2 * 1024 * 1024 * 1024


class InstallError(ValueError):
    pass


def fail(message: str) -> None:
    raise InstallError(message)


def load_verifier():
    spec = importlib.util.spec_from_file_location("candidate_verify", VERIFY)
    if spec is None or spec.loader is None:
        fail("candidate verifier cannot be loaded")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def safe_extract(archive: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    seen: set[str] = set()
    files = 0
    total = 0
    try:
        with zipfile.ZipFile(archive, "r") as source:
            for info in source.infolist():
                if info.flag_bits & 0x1:
                    fail("encrypted candidate ZIP member is forbidden")
                name = info.filename.rstrip("/") if info.is_dir() else info.filename
                if not name or "\\" in name or ":" in name or "\x00" in name:
                    fail("unsafe candidate ZIP path")
                pure = PurePosixPath(name)
                if pure.is_absolute() or any(part in {"", ".", ".."} for part in pure.parts):
                    fail("unsafe candidate ZIP path")
                alias = pure.as_posix().casefold()
                if alias in seen:
                    fail("candidate ZIP contains duplicate/case-alias path")
                seen.add(alias)
                mode = (info.external_attr >> 16) & 0xFFFF
                file_type = stat.S_IFMT(mode)
                target = destination.joinpath(*pure.parts)
                if info.is_dir():
                    if file_type not in (0, stat.S_IFDIR):
                        fail("special candidate ZIP directory is forbidden")
                    target.mkdir(parents=True, exist_ok=False)
                    continue
                if file_type not in (0, stat.S_IFREG):
                    fail("link/special candidate ZIP member is forbidden")
                files += 1
                total += info.file_size
                if files > MAX_FILES or total > MAX_BYTES:
                    fail("candidate ZIP exceeds bounded inventory")
                target.parent.mkdir(parents=True, exist_ok=True)
                if target.exists() or target.is_symlink():
                    fail("candidate ZIP target already exists")
                with source.open(info, "r") as src, target.open("xb") as dst:
                    shutil.copyfileobj(src, dst, 1024 * 1024)
    except InstallError:
        raise
    except (OSError, zipfile.BadZipFile, RuntimeError) as error:
        raise InstallError("candidate ZIP extraction failed") from error
    if files == 0:
        fail("candidate ZIP contains no files")


def install(archive: Path, provenance: Path, build_source_commit: str) -> Path:
    verifier = load_verifier()
    try:
        verified = verifier.verify(archive, provenance, PATCH_LOCK, "linux", build_source_commit)
    except Exception as error:
        raise InstallError(f"candidate verification failed: {error}") from error

    from camoufox.multiversion import BROWSERS_DIR, COMPAT_FLAG, set_active

    version = verified["camoufox_version"]
    if "-" not in version:
        fail("Camoufox version does not expose build")
    firefox_version, build = version.rsplit("-", 1)
    repository = verified["upstream_repository"].split("/", 1)[0].lower()
    install_path = BROWSERS_DIR / repository / version
    if install_path.exists() or install_path.is_symlink():
        fail("candidate install path already exists")

    with tempfile.TemporaryDirectory(prefix="camoufox-ci-stage-") as directory:
        staged = Path(directory) / "candidate"
        safe_extract(archive, staged)
        executable = staged / "camoufox"
        if executable.is_symlink() or not executable.is_file():
            fail("Linux candidate executable is missing")
        install_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(staged), str(install_path))

    metadata = {
        "version": firefox_version,
        "build": build,
        "prerelease": False,
        "sha256": verified["artifact_sha256"],
    }
    (install_path / "version.json").write_text(json.dumps(metadata, sort_keys=True, separators=(",", ":")), encoding="utf-8")
    os.chmod(install_path / "camoufox", 0o755)
    set_active(f"browsers/{repository}/{version}")
    COMPAT_FLAG.parent.mkdir(parents=True, exist_ok=True)
    COMPAT_FLAG.touch()
    return install_path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--build-source-commit", required=True)
    args = parser.parse_args()
    try:
        path = install(args.archive, args.provenance, args.build_source_commit)
        print(path)
        return 0
    except (InstallError, OSError, ValueError) as error:
        print(f"Camoufox CI candidate install error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
