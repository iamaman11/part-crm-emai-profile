#!/usr/bin/env python3
"""Materialize the exact clean-host Windows Camouhost runtime tree.

The browser is never downloaded here. A trusted build workflow must provide the
patched Camoufox candidate archive plus canonical provenance. This script verifies
that pair against the repository-owned patch lock before materialization.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK = ROOT / "runtime/camouhost/runtime-lock.json"
DEFAULT_PATCH_LOCK = ROOT / "runtime/camouhost/camoufox-patch-lock.json"
DEFAULT_RUNTIME_SOURCE = ROOT / "runtime/camouhost/real.py"
VERIFY_CANDIDATE = ROOT / "scripts/verify-camoufox-patched-candidate.py"
MAX_DOWNLOAD_BYTES = 1024 * 1024 * 1024
MAX_EXTRACTED_BYTES = 2 * 1024 * 1024 * 1024
MAX_ARCHIVE_FILES = 500_000
MAX_PYTHON_PACKAGES = 256
CHUNK_BYTES = 1024 * 1024
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
PACKAGE_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
PYPI_FILES_PREFIX = "https://files.pythonhosted.org/packages/"
WINDOWS_RESERVED = {"con", "prn", "aux", "nul", *(f"com{i}" for i in range(1, 10)), *(f"lpt{i}" for i in range(1, 10))}


class RuntimeBuildError(ValueError):
    pass


def fail(message: str) -> None:
    raise RuntimeBuildError(message)


def canonical(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode("utf-8")


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"required file is not regular: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(CHUNK_BYTES), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail(f"required JSON file missing/not regular: {path}")
    raw = path.read_bytes()
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeBuildError(f"invalid JSON: {path}") from error
    if not isinstance(value, dict) or canonical(value) != raw:
        fail(f"JSON must be canonical: {path}")
    return value


def validate_python_packages(python: dict[str, Any], components: dict[str, Any]) -> list[dict[str, Any]]:
    packages = python.get("packages")
    if not isinstance(packages, list) or not packages or len(packages) > MAX_PYTHON_PACKAGES:
        fail("Windows Python package graph is invalid")
    seen_names: set[str] = set()
    seen_files: set[str] = set()
    ordering: list[tuple[str, str, str]] = []
    versions: dict[str, str] = {}
    for row in packages:
        if not isinstance(row, dict) or set(row) != {"filename", "name", "sha256", "url", "version"}:
            fail("Windows Python package row shape is invalid")
        filename, name, digest, url, version = (row.get(k) for k in ("filename", "name", "sha256", "url", "version"))
        if not isinstance(filename, str) or not filename.endswith(".whl") or PurePosixPath(filename).name != filename or "\\" in filename or ":" in filename or "\x00" in filename:
            fail("Windows Python package filename is invalid")
        if not isinstance(name, str) or PACKAGE_NAME_RE.fullmatch(name) is None:
            fail("Windows Python package name is invalid")
        if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
            fail("Windows Python package SHA-256 is invalid")
        if not isinstance(url, str) or not url.startswith(PYPI_FILES_PREFIX) or url.rsplit("/", 1)[-1] != filename:
            fail("Windows Python package URL is invalid")
        if not isinstance(version, str) or not version or any(ch.isspace() for ch in version):
            fail("Windows Python package version is invalid")
        if name in seen_names or filename.casefold() in seen_files:
            fail("Windows Python package graph contains duplicate identity")
        seen_names.add(name); seen_files.add(filename.casefold()); ordering.append((name, version, filename)); versions[name] = version
    if ordering != sorted(ordering):
        fail("Windows Python package graph is not deterministically ordered")
    expected = {"browserforge": components.get("browserforge"), "camoufox": components.get("camoufox_python"), "playwright": components.get("playwright")}
    if any(versions.get(name) != version for name, version in expected.items()):
        fail("Windows Python package graph disagrees with runtime component lock")
    return packages


def load_lock(path: Path) -> dict[str, Any]:
    value = load_json(path)
    required = {"browser", "camouhost_ipc_version", "components", "fingerprint_config_schema", "fingerprint_policy_version", "python", "python_source", "runtime_role", "schema_version", "windows_distribution"}
    if set(value) != required or value.get("schema_version") != 1 or value.get("runtime_role") != "real_camoufox":
        fail("runtime lock identity/shape is unsupported")
    components = value.get("components")
    if not isinstance(components, dict) or set(components) != {"browserforge", "camoufox_python", "playwright"}:
        fail("runtime component lock shape is invalid")
    distribution = value.get("windows_distribution")
    if not isinstance(distribution, dict) or set(distribution) != {"architecture", "browser", "python"} or distribution.get("architecture") != "x86_64":
        fail("Windows distribution lock shape is invalid")
    browser = distribution.get("browser")
    if not isinstance(browser, dict) or browser.get("executable_path") != "browser/camoufox.exe":
        fail("Windows browser executable contract is unsupported")
    python = distribution.get("python")
    if not isinstance(python, dict) or set(python) != {"artifact_sha256", "artifact_url", "packages", "version"}:
        fail("Windows Python distribution lock shape is invalid")
    if python.get("version") != "3.12.10" or value.get("python") != "3.12":
        fail("Windows Python distribution/version contract is unsupported")
    if not isinstance(python.get("artifact_sha256"), str) or SHA256_RE.fullmatch(python["artifact_sha256"]) is None:
        fail("Windows Python distribution SHA-256 is invalid")
    if not isinstance(python.get("artifact_url"), str) or not python["artifact_url"].startswith("https://www.python.org/"):
        fail("Windows Python distribution URL is invalid")
    validate_python_packages(python, components)
    return value


def download_exact(url: str, expected_sha256: str, target: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "profile-bridge-release-build/1"})
    digest = hashlib.sha256(); total = 0
    try:
        with urllib.request.urlopen(request, timeout=60) as response, target.open("xb") as output:
            while True:
                chunk = response.read(CHUNK_BYTES)
                if not chunk: break
                total += len(chunk)
                if total > MAX_DOWNLOAD_BYTES: fail("distribution download exceeds bounded size")
                digest.update(chunk); output.write(chunk)
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
        if part.endswith((".", " ")) or part.split(".", 1)[0].casefold() in WINDOWS_RESERVED:
            fail(f"Windows-ambiguous ZIP member path: {name!r}")
    return pure


def extract_exact_zip(archive: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    observed: set[str] = set(); total = 0; files = 0
    try:
        with zipfile.ZipFile(archive, "r") as source:
            for info in source.infolist():
                pure = safe_member_path(info.filename.rstrip("/") if info.is_dir() else info.filename)
                alias = pure.as_posix().casefold()
                if alias in observed: fail(f"duplicate/case-alias ZIP member: {info.filename}")
                observed.add(alias)
                if info.flag_bits & 0x1: fail(f"encrypted ZIP member is forbidden: {info.filename}")
                mode = (info.external_attr >> 16) & 0xFFFF; file_type = stat.S_IFMT(mode)
                target = destination.joinpath(*pure.parts)
                if info.is_dir():
                    if file_type not in (0, stat.S_IFDIR): fail("special ZIP directory is forbidden")
                    target.mkdir(parents=True, exist_ok=False); continue
                if file_type not in (0, stat.S_IFREG): fail("link/special ZIP member is forbidden")
                files += 1; total += info.file_size
                if files > MAX_ARCHIVE_FILES or total > MAX_EXTRACTED_BYTES: fail("ZIP extraction exceeds bounded inventory")
                target.parent.mkdir(parents=True, exist_ok=True)
                if target.exists() or target.is_symlink(): fail("ZIP extraction target already exists")
                with source.open(info, "r") as src, target.open("xb") as dst:
                    copied = shutil.copyfileobj(src, dst, CHUNK_BYTES)
    except RuntimeBuildError:
        raise
    except (OSError, zipfile.BadZipFile, RuntimeError) as error:
        raise RuntimeBuildError("ZIP extraction failed") from error
    if files == 0: fail("ZIP distribution contains no files")


def verify_candidate(archive: Path, provenance: Path, patch_lock: Path, build_source_commit: str) -> dict[str, Any]:
    spec = importlib.util.spec_from_file_location("candidate_verify", VERIFY_CANDIDATE)
    if spec is None or spec.loader is None: fail("candidate verifier cannot be loaded")
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    try:
        return module.verify(archive, provenance, patch_lock, "windows", build_source_commit)
    except Exception as error:
        raise RuntimeBuildError(f"patched browser candidate verification failed: {error}") from error


def rewrite_embedded_python_path(root: Path) -> None:
    path = root / "python312._pth"
    if path.is_symlink() or not path.is_file(): fail("embedded Python path configuration is missing")
    path.write_text("python312.zip\n.\nLib\\site-packages\nimport site\n", encoding="utf-8", newline="\n")


def install_python_components(build_python: Path, lock: dict[str, Any], python_root: Path) -> None:
    packages = validate_python_packages(lock["windows_distribution"]["python"], lock["components"])
    site = python_root / "Lib/site-packages"; site.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="camouhost-wheels-") as directory:
        wheels: list[str] = []
        for row in packages:
            target = Path(directory) / row["filename"]
            download_exact(row["url"], row["sha256"], target); wheels.append(str(target))
        command = [str(build_python), "-m", "pip", "install", "--disable-pip-version-check", "--no-cache-dir", "--no-compile", "--no-deps", "--no-index", "--no-warn-script-location", "--target", str(site), *wheels]
        if subprocess.run(command, cwd=ROOT, check=False).returncode != 0: fail("exact locked Python wheel installation failed")


def remove_python_caches(root: Path) -> None:
    for path in sorted(root.rglob("__pycache__"), reverse=True):
        if path.is_symlink() or not path.is_dir(): fail("Python cache path is unsafe")
        shutil.rmtree(path)
    for path in root.rglob("*.pyc"):
        if path.is_symlink() or not path.is_file(): fail("Python bytecode path is unsafe")
        path.unlink()


def verify_runtime_tree(output: Path, lock: dict[str, Any], candidate_provenance: dict[str, Any]) -> dict[str, Any]:
    required = [output/"python/python.exe", output/"browser/camoufox.exe", output/"camouhost/real.py", output/"camouhost/runtime-lock.json", output/"camouhost/browser-candidate-provenance.json"]
    for path in required:
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0: fail(f"materialized runtime file missing/not regular: {path}")
    components = lock["components"]
    probe = "import importlib.metadata,json,sys;print(json.dumps({'python':'.'.join(map(str,sys.version_info[:3])),'camoufox':importlib.metadata.version('camoufox'),'browserforge':importlib.metadata.version('browserforge'),'playwright':importlib.metadata.version('playwright')},sort_keys=True,separators=(',',':')))"
    completed = subprocess.run([str(output/"python/python.exe"), "-I", "-c", probe], cwd=output, text=True, capture_output=True, check=False)
    expected = {"python":"3.12.10", "camoufox":components["camoufox_python"], "browserforge":components["browserforge"], "playwright":components["playwright"]}
    try: observed = json.loads(completed.stdout.strip())
    except json.JSONDecodeError as error: raise RuntimeBuildError("embedded Python verification output is invalid") from error
    if completed.returncode != 0 or observed != expected: fail("materialized Python component identity mismatch")
    inventory=[]; total=0
    for path in sorted(output.rglob("*")):
        if path.is_symlink(): fail("materialized runtime contains symlink")
        if path.is_dir(): continue
        if not path.is_file(): fail("materialized runtime contains special entry")
        size=path.stat().st_size; total += size
        if total > MAX_EXTRACTED_BYTES: fail("materialized runtime exceeds bounded size")
        inventory.append({"path":path.relative_to(output).as_posix(),"sha256":sha256_file(path),"size_bytes":size})
    return {"schema_version":1,"kind":"CAMOUHOST_WINDOWS_RESOLVED_RUNTIME","components":observed,"browser_candidate_sha256":candidate_provenance["artifact_sha256"],"browser_candidate_provenance_sha256":hashlib.sha256(canonical(candidate_provenance)).hexdigest(),"files":len(inventory),"inventory_sha256":hashlib.sha256(canonical(inventory)).hexdigest(),"total_size_bytes":total}


def materialize(*, runtime_lock: Path, patch_lock: Path, runtime_source: Path, browser_candidate: Path, browser_provenance: Path, build_source_commit: str, output: Path, build_python: Path) -> None:
    if os.name != "nt": fail("Windows runtime materialization requires Windows")
    if output.exists() or output.is_symlink(): fail("runtime output must not already exist")
    if runtime_source.is_symlink() or not runtime_source.is_file(): fail("real Camouhost source missing/not regular")
    build_python = build_python.resolve(strict=True)
    lock = load_lock(runtime_lock)
    provenance = verify_candidate(browser_candidate, browser_provenance, patch_lock, build_source_commit)
    with tempfile.TemporaryDirectory(prefix="camouhost-windows-build-") as directory:
        temp=Path(directory); python_archive=temp/"python.zip"
        python_lock=lock["windows_distribution"]["python"]
        download_exact(python_lock["artifact_url"], python_lock["artifact_sha256"], python_archive)
        output.mkdir(parents=True, exist_ok=False)
        try:
            extract_exact_zip(python_archive, output/"python")
            extract_exact_zip(browser_candidate, output/"browser")
            rewrite_embedded_python_path(output/"python")
            install_python_components(build_python, lock, output/"python")
            camouhost=output/"camouhost"; camouhost.mkdir()
            shutil.copyfile(runtime_source, camouhost/"real.py"); shutil.copyfile(runtime_lock, camouhost/"runtime-lock.json")
            (camouhost/"browser-candidate-provenance.json").write_bytes(canonical(provenance))
            remove_python_caches(output)
            resolved=verify_runtime_tree(output, lock, provenance)
            (camouhost/"resolved-runtime.json").write_bytes(canonical(resolved))
            verify_runtime_tree(output, lock, provenance)
        except BaseException:
            shutil.rmtree(output, ignore_errors=True); raise


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="camouhost-runtime-selftest-") as directory:
        root=Path(directory); archive=root/"safe.zip"
        with zipfile.ZipFile(archive,"w",compression=zipfile.ZIP_STORED) as z: z.writestr("alpha/file.txt",b"payload")
        extract_exact_zip(archive,root/"safe")
        for name in ("traversal.zip","aliases.zip"):
            bad=root/name
            with zipfile.ZipFile(bad,"w",compression=zipfile.ZIP_STORED) as z:
                if name.startswith("traversal"): z.writestr("../escape.txt",b"x")
                else: z.writestr("Alpha.txt",b"1"); z.writestr("alpha.txt",b"2")
            try: extract_exact_zip(bad,root/(name+".out"))
            except RuntimeBuildError: pass
            else: fail("unsafe ZIP negative self-test unexpectedly passed")
    print("Windows Camouhost runtime materializer self-test passed.")


def parser() -> argparse.ArgumentParser:
    result=argparse.ArgumentParser(); commands=result.add_subparsers(dest="command",required=True)
    build=commands.add_parser("build")
    build.add_argument("--runtime-lock",type=Path,default=DEFAULT_LOCK); build.add_argument("--patch-lock",type=Path,default=DEFAULT_PATCH_LOCK); build.add_argument("--runtime-source",type=Path,default=DEFAULT_RUNTIME_SOURCE)
    build.add_argument("--browser-candidate",type=Path,required=True); build.add_argument("--browser-provenance",type=Path,required=True); build.add_argument("--build-source-commit",required=True); build.add_argument("--output",type=Path,required=True); build.add_argument("--build-python",type=Path,required=True)
    commands.add_parser("self-test"); return result


def main() -> int:
    args=parser().parse_args()
    try:
        if args.command=="self-test": self_test()
        else: materialize(runtime_lock=args.runtime_lock,patch_lock=args.patch_lock,runtime_source=args.runtime_source,browser_candidate=args.browser_candidate,browser_provenance=args.browser_provenance,build_source_commit=args.build_source_commit,output=args.output,build_python=args.build_python)
        return 0
    except (RuntimeBuildError,OSError,subprocess.SubprocessError) as error:
        print(f"Windows runtime build error: {error}",file=sys.stderr); return 1


if __name__=="__main__": raise SystemExit(main())
