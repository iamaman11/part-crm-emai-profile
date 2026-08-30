#!/usr/bin/env python3
"""Materialize the exact clean-host Windows Camouhost runtime tree.

This script is a build-time materializer only. The existing
`camouhost-runtime-package.py` remains the component archive/manifest owner and
Release Set v3 remains the aggregate publication owner. Nothing in this script
is intended to run on a customer host.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK = ROOT / "runtime" / "camouhost" / "runtime-lock.json"
DEFAULT_RUNTIME_SOURCE = ROOT / "runtime" / "camouhost" / "real.py"
MAX_DOWNLOAD_BYTES = 1024 * 1024 * 1024
MAX_EXTRACTED_BYTES = 2 * 1024 * 1024 * 1024
MAX_ARCHIVE_FILES = 500_000
MAX_PYTHON_PACKAGES = 256
CHUNK_BYTES = 1024 * 1024
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
PACKAGE_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
PYPI_FILES_PREFIX = "https://files.pythonhosted.org/packages/"
WINDOWS_RESERVED = {
    "con",
    "prn",
    "aux",
    "nul",
    *(f"com{index}" for index in range(1, 10)),
    *(f"lpt{index}" for index in range(1, 10)),
}


class RuntimeBuildError(ValueError):
    """Fail-closed Windows runtime materialization error."""


def fail(message: str) -> None:
    raise RuntimeBuildError(message)


def canonical(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
        + "\n"
    ).encode("utf-8")


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"required file is not regular: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(CHUNK_BYTES), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_python_packages(python: dict[str, Any], components: dict[str, Any]) -> None:
    packages = python.get("packages")
    if not isinstance(packages, list) or not packages or len(packages) > MAX_PYTHON_PACKAGES:
        fail("Windows Python package graph is invalid")
    observed_names: set[str] = set()
    observed_filenames: set[str] = set()
    ordering: list[tuple[str, str, str]] = []
    versions: dict[str, str] = {}
    for row in packages:
        if not isinstance(row, dict) or set(row) != {
            "filename",
            "name",
            "sha256",
            "url",
            "version",
        }:
            fail("Windows Python package row shape is invalid")
        filename = row.get("filename")
        name = row.get("name")
        digest = row.get("sha256")
        url = row.get("url")
        version = row.get("version")
        if (
            not isinstance(filename, str)
            or not filename.endswith(".whl")
            or PurePosixPath(filename).name != filename
            or "\\" in filename
            or ":" in filename
            or "\x00" in filename
        ):
            fail("Windows Python package filename is invalid")
        if not isinstance(name, str) or PACKAGE_NAME_RE.fullmatch(name) is None:
            fail("Windows Python package name is invalid")
        if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
            fail("Windows Python package SHA-256 is invalid")
        if (
            not isinstance(url, str)
            or not url.startswith(PYPI_FILES_PREFIX)
            or url.rsplit("/", 1)[-1] != filename
        ):
            fail("Windows Python package URL is invalid")
        if (
            not isinstance(version, str)
            or not version
            or len(version) > 64
            or any(character.isspace() for character in version)
        ):
            fail("Windows Python package version is invalid")
        if name in observed_names or filename.casefold() in observed_filenames:
            fail("Windows Python package graph contains duplicate identity")
        observed_names.add(name)
        observed_filenames.add(filename.casefold())
        ordering.append((name, version, filename))
        versions[name] = version
    if ordering != sorted(ordering):
        fail("Windows Python package graph is not deterministically ordered")
    required_versions = {
        "browserforge": components.get("browserforge"),
        "camoufox": components.get("camoufox_python"),
        "playwright": components.get("playwright"),
    }
    if any(not isinstance(version, str) or not version for version in required_versions.values()):
        fail("runtime component version lock is invalid")
    if any(versions.get(name) != version for name, version in required_versions.items()):
        fail("Windows Python package graph disagrees with runtime component lock")


def load_lock(path: Path) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail("runtime lock is missing/not regular")
    raw = path.read_bytes()
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeBuildError("runtime lock is invalid JSON") from error
    if not isinstance(value, dict) or canonical(value) != raw:
        fail("runtime lock must be canonical JSON")
    required = {
        "browser",
        "camouhost_ipc_version",
        "components",
        "fingerprint_config_schema",
        "fingerprint_policy_version",
        "python",
        "python_source",
        "runtime_role",
        "schema_version",
        "windows_distribution",
    }
    if set(value) != required:
        fail("runtime lock shape is unsupported")
    if value.get("schema_version") != 1 or value.get("runtime_role") != "real_camoufox":
        fail("runtime lock identity is unsupported")
    components = value.get("components")
    if not isinstance(components, dict) or set(components) != {
        "browserforge",
        "camoufox_python",
        "playwright",
    }:
        fail("runtime component lock shape is invalid")
    python_source = value.get("python_source")
    if not isinstance(python_source, dict) or set(python_source) != {"commit", "repository"}:
        fail("Camoufox Python source lock shape is invalid")
    source_commit = python_source.get("commit")
    if (
        python_source.get("repository") != "daijro/camoufox"
        or not isinstance(source_commit, str)
        or re.fullmatch(r"[0-9a-f]{40}", source_commit) is None
    ):
        fail("Camoufox Python source identity is invalid")
    distribution = value.get("windows_distribution")
    if not isinstance(distribution, dict) or set(distribution) != {
        "architecture",
        "browser",
        "python",
    }:
        fail("Windows distribution lock shape is invalid")
    if distribution.get("architecture") != "x86_64":
        fail("Windows runtime architecture is unsupported")
    browser = distribution.get("browser")
    python = distribution.get("python")
    if not isinstance(browser, dict) or set(browser) != {
        "artifact_sha256",
        "artifact_url",
        "executable_path",
    }:
        fail("Windows browser distribution lock shape is invalid")
    if not isinstance(python, dict) or set(python) != {
        "artifact_sha256",
        "artifact_url",
        "packages",
        "version",
    }:
        fail("Windows Python distribution lock shape is invalid")
    for row in (browser, python):
        digest = row.get("artifact_sha256")
        url = row.get("artifact_url")
        if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
            fail("Windows distribution SHA-256 is invalid")
        if not isinstance(url, str) or not url.startswith("https://"):
            fail("Windows distribution URL must be HTTPS")
    if python.get("version") != "3.12.10" or value.get("python") != "3.12":
        fail("Windows Python distribution/version contract is unsupported")
    if browser.get("executable_path") != "browser/camoufox.exe":
        fail("Windows browser executable contract is unsupported")
    validate_python_packages(python, components)
    return value


def download_exact(url: str, expected_sha256: str, target: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "profile-bridge-release-build/1"})
    digest = hashlib.sha256()
    total = 0
    try:
        with urllib.request.urlopen(request, timeout=60) as response, target.open("xb") as output:
            while True:
                chunk = response.read(CHUNK_BYTES)
                if not chunk:
                    break
                total += len(chunk)
                if total > MAX_DOWNLOAD_BYTES:
                    fail("distribution download exceeds bounded size")
                digest.update(chunk)
                output.write(chunk)
    except RuntimeBuildError:
        raise
    except (OSError, urllib.error.URLError) as error:
        raise RuntimeBuildError("distribution download failed") from error
    if total == 0 or digest.hexdigest() != expected_sha256:
        fail("distribution download digest mismatch")


def safe_member_path(name: str) -> PurePosixPath:
    if not name or "\\" in name or ":" in name or "\x00" in name:
        fail(f"unsafe ZIP member path: {name!r}")
    pure = PurePosixPath(name)
    if pure.is_absolute() or any(part in {"", ".", ".."} for part in pure.parts):
        fail(f"unsafe ZIP member path: {name!r}")
    for part in pure.parts:
        if part.endswith((".", " ")):
            fail(f"Windows-ambiguous ZIP member path: {name!r}")
        stem = part.split(".", 1)[0].casefold()
        if stem in WINDOWS_RESERVED:
            fail(f"Windows-reserved ZIP member path: {name!r}")
    return pure


def zip_member_kind(info: zipfile.ZipInfo) -> str:
    if info.flag_bits & 0x1:
        fail(f"encrypted ZIP member is forbidden: {info.filename}")
    mode = (info.external_attr >> 16) & 0xFFFF
    file_type = stat.S_IFMT(mode)
    if info.is_dir():
        if file_type not in (0, stat.S_IFDIR):
            fail(f"special ZIP directory is forbidden: {info.filename}")
        return "directory"
    if file_type not in (0, stat.S_IFREG):
        fail(f"link/special ZIP member is forbidden: {info.filename}")
    return "file"


def extract_exact_zip(archive: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    observed: set[str] = set()
    total = 0
    files = 0
    try:
        with zipfile.ZipFile(archive, "r") as source:
            for info in source.infolist():
                pure = safe_member_path(info.filename.rstrip("/") if info.is_dir() else info.filename)
                alias = pure.as_posix().casefold()
                if alias in observed:
                    fail(f"duplicate/case-alias ZIP member: {info.filename}")
                observed.add(alias)
                kind = zip_member_kind(info)
                target = destination.joinpath(*pure.parts)
                if kind == "directory":
                    target.mkdir(parents=True, exist_ok=False)
                    continue
                files += 1
                total += info.file_size
                if files > MAX_ARCHIVE_FILES or total > MAX_EXTRACTED_BYTES:
                    fail("ZIP extraction exceeds bounded inventory")
                target.parent.mkdir(parents=True, exist_ok=True)
                if target.exists() or target.is_symlink():
                    fail(f"ZIP extraction target already exists: {pure.as_posix()}")
                with source.open(info, "r") as input_handle, target.open("xb") as output:
                    copied = 0
                    while True:
                        chunk = input_handle.read(CHUNK_BYTES)
                        if not chunk:
                            break
                        copied += len(chunk)
                        if copied > info.file_size:
                            fail("ZIP member exceeded declared size")
                        output.write(chunk)
                if copied != info.file_size:
                    fail("ZIP member size mismatch")
    except RuntimeBuildError:
        raise
    except (OSError, zipfile.BadZipFile, RuntimeError) as error:
        raise RuntimeBuildError("ZIP extraction failed") from error
    if files == 0:
        fail("ZIP distribution contains no files")


def rewrite_embedded_python_path(python_root: Path) -> None:
    path = python_root / "python312._pth"
    if path.is_symlink() or not path.is_file():
        fail("embedded Python path configuration is missing")
    path.write_text("python312.zip\n.\nLib\\site-packages\nimport site\n", encoding="utf-8", newline="\n")


def install_python_components(build_python: Path, lock: dict[str, Any], python_root: Path) -> None:
    packages = lock["windows_distribution"]["python"]["packages"]
    site_packages = python_root / "Lib" / "site-packages"
    site_packages.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="camouhost-windows-wheels-") as directory:
        wheel_root = Path(directory)
        wheel_paths: list[Path] = []
        for row in packages:
            wheel = wheel_root / row["filename"]
            download_exact(row["url"], row["sha256"], wheel)
            wheel_paths.append(wheel)
        command = [
            str(build_python),
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "--no-cache-dir",
            "--no-compile",
            "--no-deps",
            "--no-index",
            "--no-warn-script-location",
            "--target",
            str(site_packages),
            *(str(path) for path in wheel_paths),
        ]
        completed = subprocess.run(command, cwd=ROOT, check=False)
        if completed.returncode != 0:
            fail("exact locked Python wheel installation failed")


def remove_python_caches(root: Path) -> None:
    for path in sorted(root.rglob("__pycache__"), reverse=True):
        if path.is_symlink() or not path.is_dir():
            fail("Python cache path is unsafe")
        shutil.rmtree(path)
    for path in root.rglob("*.pyc"):
        if path.is_symlink() or not path.is_file():
            fail("Python bytecode path is unsafe")
        path.unlink()


def verify_runtime_tree(output: Path, lock: dict[str, Any]) -> dict[str, Any]:
    python_executable = output / "python" / "python.exe"
    browser_executable = output / "browser" / "camoufox.exe"
    runtime_source = output / "camouhost" / "real.py"
    runtime_lock = output / "camouhost" / "runtime-lock.json"
    for path in (python_executable, browser_executable, runtime_source, runtime_lock):
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            fail(f"materialized runtime file is missing/not regular: {path.relative_to(output)}")

    components = lock["components"]
    probe = (
        "import importlib.metadata,json,sys;"
        "value={'python':'.'.join(map(str,sys.version_info[:3])),"
        "'camoufox':importlib.metadata.version('camoufox'),"
        "'browserforge':importlib.metadata.version('browserforge'),"
        "'playwright':importlib.metadata.version('playwright')};"
        "print(json.dumps(value,sort_keys=True,separators=(',',':')))"
    )
    completed = subprocess.run(
        [str(python_executable), "-I", "-c", probe],
        cwd=output,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        fail("embedded Python runtime verification failed")
    try:
        observed = json.loads(completed.stdout.strip())
    except json.JSONDecodeError as error:
        raise RuntimeBuildError("embedded Python verification output is invalid") from error
    expected = {
        "python": lock["windows_distribution"]["python"]["version"],
        "camoufox": components["camoufox_python"],
        "browserforge": components["browserforge"],
        "playwright": components["playwright"],
    }
    if observed != expected:
        fail(f"materialized Python component identity mismatch: {observed}")

    inventory: list[dict[str, Any]] = []
    total = 0
    for path in sorted(output.rglob("*")):
        if path.is_symlink():
            fail(f"materialized runtime contains symlink: {path.relative_to(output)}")
        if path.is_dir():
            continue
        if not path.is_file():
            fail(f"materialized runtime contains special entry: {path.relative_to(output)}")
        relative = path.relative_to(output).as_posix()
        size = path.stat().st_size
        total += size
        if total > MAX_EXTRACTED_BYTES:
            fail("materialized runtime exceeds bounded size")
        inventory.append({"path": relative, "sha256": sha256_file(path), "size_bytes": size})
    if not inventory or len(inventory) > MAX_ARCHIVE_FILES:
        fail("materialized runtime inventory is invalid")
    packages = lock["windows_distribution"]["python"]["packages"]
    return {
        "schema_version": 1,
        "kind": "CAMOUHOST_WINDOWS_RESOLVED_RUNTIME",
        "components": observed,
        "dependency_graph_sha256": hashlib.sha256(canonical(packages)).hexdigest(),
        "files": len(inventory),
        "inventory_sha256": hashlib.sha256(canonical(inventory)).hexdigest(),
        "package_count": len(packages),
        "total_size_bytes": total,
    }


def materialize(
    *,
    runtime_lock: Path,
    runtime_source: Path,
    output: Path,
    build_python: Path,
) -> None:
    if os.name != "nt":
        fail("Windows runtime materialization requires Windows")
    if output.exists() or output.is_symlink():
        fail("runtime output must not already exist")
    if runtime_source.is_symlink() or not runtime_source.is_file():
        fail("real Camouhost source is missing/not regular")
    build_python = build_python.resolve(strict=True)
    if build_python.is_symlink() or not build_python.is_file():
        fail("build Python must be a regular executable")
    lock = load_lock(runtime_lock)
    distribution = lock["windows_distribution"]

    with tempfile.TemporaryDirectory(prefix="camouhost-windows-build-") as directory:
        temp = Path(directory)
        python_archive = temp / "python.zip"
        browser_archive = temp / "browser.zip"
        download_exact(
            distribution["python"]["artifact_url"],
            distribution["python"]["artifact_sha256"],
            python_archive,
        )
        download_exact(
            distribution["browser"]["artifact_url"],
            distribution["browser"]["artifact_sha256"],
            browser_archive,
        )

        output.mkdir(parents=True, exist_ok=False)
        try:
            python_root = output / "python"
            browser_root = output / "browser"
            extract_exact_zip(python_archive, python_root)
            extract_exact_zip(browser_archive, browser_root)
            rewrite_embedded_python_path(python_root)
            install_python_components(build_python, lock, python_root)
            camouhost_root = output / "camouhost"
            camouhost_root.mkdir()
            shutil.copyfile(runtime_source, camouhost_root / "real.py")
            shutil.copyfile(runtime_lock, camouhost_root / "runtime-lock.json")
            remove_python_caches(output)
            resolved = verify_runtime_tree(output, lock)
            (camouhost_root / "resolved-runtime.json").write_bytes(canonical(resolved))
            verify_runtime_tree(output, lock)
        except BaseException:
            shutil.rmtree(output, ignore_errors=True)
            raise


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="camouhost-windows-build-selftest-") as directory:
        root = Path(directory)
        archive = root / "safe.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_STORED) as handle:
            handle.writestr("alpha/file.txt", b"payload")
        extract_exact_zip(archive, root / "safe")
        if (root / "safe" / "alpha" / "file.txt").read_bytes() != b"payload":
            fail("safe ZIP extraction self-test failed")

        traversal = root / "traversal.zip"
        with zipfile.ZipFile(traversal, "w", compression=zipfile.ZIP_STORED) as handle:
            handle.writestr("../escape.txt", b"escape")
        try:
            extract_exact_zip(traversal, root / "traversal")
        except RuntimeBuildError:
            pass
        else:
            fail("ZIP traversal negative self-test unexpectedly passed")

        aliases = root / "aliases.zip"
        with zipfile.ZipFile(aliases, "w", compression=zipfile.ZIP_STORED) as handle:
            handle.writestr("Alpha.txt", b"first")
            handle.writestr("alpha.txt", b"second")
        try:
            extract_exact_zip(aliases, root / "aliases")
        except RuntimeBuildError:
            pass
        else:
            fail("ZIP case-alias negative self-test unexpectedly passed")
    print("Windows Camouhost runtime materializer self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)
    build = subcommands.add_parser("build")
    build.add_argument("--runtime-lock", type=Path, default=DEFAULT_LOCK)
    build.add_argument("--runtime-source", type=Path, default=DEFAULT_RUNTIME_SOURCE)
    build.add_argument("--output", type=Path, required=True)
    build.add_argument("--build-python", type=Path, required=True)
    subcommands.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "build":
            materialize(
                runtime_lock=args.runtime_lock,
                runtime_source=args.runtime_source,
                output=args.output,
                build_python=args.build_python,
            )
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except (RuntimeBuildError, OSError, subprocess.SubprocessError) as error:
        print(f"Windows runtime build error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
