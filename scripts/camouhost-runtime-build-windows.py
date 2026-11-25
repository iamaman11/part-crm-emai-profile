#!/usr/bin/env python3
"""Materialize the exact clean-host Windows Camouhost runtime tree.

Official Windows Camoufox and Python wheels are SHA-256 pinned by the single
runtime-lock and materialized with bounded download/extraction and no fork.
"""
from __future__ import annotations

import argparse
from email.parser import BytesParser
import hashlib
import io
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from unittest.mock import patch
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK = ROOT / "runtime/camouhost/runtime-lock.json"
DEFAULT_RUNTIME_SOURCE = ROOT / "runtime/camouhost/real.py"
DEFAULT_NATIVE_SOURCE = ROOT / "runtime/camouhost/native_process.py"
MAX_DOWNLOAD_BYTES = 2 * 1024 * 1024 * 1024
MAX_EXTRACTED_BYTES = 4 * 1024 * 1024 * 1024
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
    expected = {"fpgen": components.get("fpgen"), "camoufox": components.get("camoufox_python"), "playwright": components.get("playwright")}
    if any(versions.get(name) != version for name, version in expected.items()):
        fail("Windows Python package graph disagrees with runtime component lock")
    return packages


def load_lock(path: Path) -> dict[str, Any]:
    value = load_json(path)
    required = {"browser", "camouhost_ipc_version", "components", "fingerprint_config_schema", "fingerprint_policy_version", "python", "python_source", "runtime_role", "schema_version", "windows_distribution"}
    if set(value) != required or value.get("schema_version") != 1 or value.get("runtime_role") != "real_camoufox":
        fail("runtime lock identity/shape is unsupported")
    components = value.get("components")
    if not isinstance(components, dict) or set(components) != {"camoufox_python", "fpgen", "playwright"}:
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


def public_json(url: str) -> dict[str, Any]:
    """Read only official public release metadata, never an executable or a secret."""
    if not (url.startswith("https://api.github.com/repos/daijro/camoufox/")
            or url.startswith("https://pypi.org/pypi/")):
        fail("untrusted runtime lock metadata origin")
    request = urllib.request.Request(url, headers={"User-Agent": "profile-bridge-runtime-lock/1", "Accept": "application/json"})
    with urllib.request.urlopen(request, timeout=30) as response:
        raw = response.read(2 * 1024 * 1024 + 1)
    if len(raw) > 2 * 1024 * 1024:
        fail("public release metadata exceeds bound")
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeBuildError("public release metadata is invalid") from error
    if not isinstance(value, dict):
        fail("public release metadata is not an object")
    return value


def official_release_pin(browser_tag: str, sdk_version: str) -> tuple[str, str, str]:
    """Require one upstream commit for both the official browser and Python SDK."""
    version = browser_tag.removeprefix("v")
    if re.fullmatch(r"[1-9][0-9]*\.[0-9]+(?:\.[0-9]+)?-beta\.[1-9][0-9]*", version) is None or browser_tag != "v" + version:
        fail("official Camoufox browser tag is invalid")
    if re.fullmatch(r"0\.[0-9]+\.[0-9]+(?:b[1-9][0-9]*)?", sdk_version) is None:
        fail("official Camoufox Python version is invalid")
    prefix = "https://api.github.com/repos/daijro/camoufox/"
    release = public_json(prefix + "releases/tags/" + browser_tag)
    if release.get("tag_name") != browser_tag:
        fail("upstream release tag mismatch")
    browser_commit = public_json(prefix + "commits/" + browser_tag).get("sha")
    sdk_commit = public_json(prefix + "commits/v" + sdk_version).get("sha")
    if not isinstance(browser_commit, str) or re.fullmatch(r"[0-9a-f]{40}", browser_commit) is None or browser_commit != sdk_commit:
        fail("Camoufox browser and Python SDK do not resolve to the same upstream commit")
    expected_url = f"https://github.com/daijro/camoufox/releases/download/{browser_tag}/camoufox-{version}-win.x86_64.zip"
    assets = release.get("assets")
    if not isinstance(assets, list):
        fail("official browser release assets are missing")
    matches = [row for row in assets if isinstance(row, dict) and row.get("browser_download_url") == expected_url]
    if len(matches) != 1:
        fail("official Windows x86_64 browser asset is not unique")
    digest = matches[0].get("digest")
    if not isinstance(digest, str) or not digest.startswith("sha256:") or SHA256_RE.fullmatch(digest[7:]) is None:
        fail("official browser release lacks a valid SHA-256")
    return browser_commit, expected_url, digest[7:]


def wheel_identity(path: Path) -> tuple[str, str]:
    """Use the wheel's own metadata; wheel filenames are not semantic authority."""
    with zipfile.ZipFile(path) as wheel:
        manifests = [n for n in wheel.namelist() if n.count("/") == 1 and n.endswith(".dist-info/METADATA")]
        if len(manifests) != 1:
            fail("resolved wheel has ambiguous package metadata")
        metadata = BytesParser().parsebytes(wheel.read(manifests[0]))
    name, version = metadata.get("Name"), metadata.get("Version")
    if not isinstance(name, str) or not isinstance(version, str):
        fail("resolved wheel is missing package identity")
    name = re.sub(r"[-_.]+", "-", name.lower())
    if PACKAGE_NAME_RE.fullmatch(name) is None or not version or any(ch.isspace() for ch in version):
        fail("resolved wheel package identity is malformed")
    return name, version


def update_official_runtime_lock(path: Path, browser_tag: str, sdk_version: str) -> None:
    """One controlled refresh of the existing lock, retaining the native build/installer.

    Pip only *resolves/downloads* Windows CPython 3.12 wheels in a disposable
    directory. The resulting release lock still admits only exact public PyPI
    URLs, SHA-256 and a complete independently reproducible wheel graph.
    """
    lock = load_lock(path)
    upstream_commit, browser_url, browser_digest = official_release_pin(browser_tag, sdk_version)
    existing = {row["filename"]: row for row in lock["windows_distribution"]["python"]["packages"]}
    old_components = lock["components"]
    with tempfile.TemporaryDirectory(prefix="camouhost-pin-refresh-") as folder:
        temp = Path(folder)
        constraints = temp / "constraints.txt"
        constraints.write_text("".join(
            f"{row['name']}=={row['version']}\n"
            for row in sorted(existing.values(), key=lambda row: row["name"])
            if row["name"] != "camoufox"
        ), encoding="utf-8")
        destination = temp / "wheels"
        destination.mkdir()
        command = [
            sys.executable, "-m", "pip", "download", "--disable-pip-version-check",
            "--only-binary=:all:", "--platform", "win_amd64", "--python-version", "3.12",
            "--implementation", "cp", "--abi", "cp312", "--dest", str(destination),
            "--constraint", str(constraints), f"camoufox=={sdk_version}",
            f"fpgen=={old_components['fpgen']}", f"playwright=={old_components['playwright']}",
        ]
        resolved = subprocess.run(command, capture_output=True, text=True, check=False, timeout=240)
        if resolved.returncode != 0:
            fail("official Windows wheel graph could not be resolved: " + " | ".join(resolved.stderr.splitlines()[-4:]))
        rows: list[dict[str, str]] = []
        for wheel in sorted(destination.glob("*.whl")):
            name, version = wheel_identity(wheel)
            digest = sha256_file(wheel)
            previous = existing.get(wheel.name)
            if previous is not None and previous["sha256"] == digest:
                row = previous
            else:
                release = public_json(f"https://pypi.org/pypi/{name}/{version}/json")
                candidates = release.get("urls")
                if not isinstance(candidates, list):
                    fail("PyPI wheel inventory is missing")
                matches = [entry for entry in candidates if isinstance(entry, dict) and entry.get("filename") == wheel.name]
                if len(matches) != 1 or matches[0].get("digests", {}).get("sha256") != digest:
                    fail("resolved Windows wheel does not match public PyPI SHA-256")
                row = {"filename": wheel.name, "name": name, "sha256": digest, "url": matches[0]["url"], "version": version}
            rows.append(row)
        if not rows:
            fail("official Windows wheel resolver returned no wheels")
        # Pip's --platform chooses Windows wheels but evaluates some PEP 508
        # environment markers on the build host. Retain pinned Windows-only
        # packages from the prior verified lock (e.g. ansicon/colorama), while
        # replacing the SDK and all resolved packages by name exactly once.
        resolved_names = {row["name"] for row in rows}
        rows.extend(row for row in existing.values()
                    if row["name"] not in resolved_names and row["name"] != "camoufox")
    lock["browser"] = {"release_commit": upstream_commit, "repository": "daijro/camoufox", "version": browser_tag[1:]}
    lock["python_source"] = {"commit": upstream_commit, "repository": "daijro/camoufox"}
    lock["components"]["camoufox_python"] = sdk_version
    lock["windows_distribution"]["browser"] = {"artifact_sha256": browser_digest, "artifact_url": browser_url, "executable_path": "browser/camoufox.exe"}
    lock["windows_distribution"]["python"]["packages"] = sorted(rows, key=lambda row: (row["name"], row["version"], row["filename"]))
    validate_python_packages(lock["windows_distribution"]["python"], lock["components"])
    # Write only once all externally resolved identities and the complete lock
    # are validated. There is no download/install or runtime mutation here.
    output = canonical(lock)
    with tempfile.NamedTemporaryFile(prefix=".runtime-lock-", dir=path.parent, delete=False) as staged:
        staging_path = Path(staged.name)
        staged.write(output)
        staged.flush()
        os.fsync(staged.fileno())
    try:
        os.replace(staging_path, path)
    finally:
        staging_path.unlink(missing_ok=True)
    load_lock(path)
    print(f"Pinned Camoufox {browser_tag} + Python SDK {sdk_version}; {len(rows)} exact Windows wheels.")


def retryable_download_error(error: BaseException) -> bool:
    if isinstance(error, urllib.error.HTTPError):
        return error.code in {408, 425, 429, 500, 502, 503, 504}
    if isinstance(error, urllib.error.URLError):
        return True
    return isinstance(error, (TimeoutError, ConnectionError))


def safe_download_error_detail(error: BaseException) -> str:
    if isinstance(error, urllib.error.HTTPError):
        return f"http_status={error.code}"
    reason = error.reason if isinstance(error, urllib.error.URLError) else error
    return f"reason_type={type(reason).__name__}"


def download_exact(
    url: str,
    expected_sha256: str,
    target: Path,
    cache: Path | None = None,
    *,
    label: str = "locked distribution",
) -> None:
    """Download exact locked bytes; retry only transient transport errors.

    The optional cache contains original public artifact bytes indexed by their
    locked digest. Cache hits are always re-hashed; corruption fails closed.
    Every network attempt writes a disposable target which is removed before a
    retry. A successful download is accepted only after its exact SHA-256 check.
    """
    if SHA256_RE.fullmatch(expected_sha256) is None:
        fail("distribution expected SHA-256 is invalid")
    if target.exists() or target.is_symlink():
        fail("distribution download target already exists")
    cached = None
    if cache is not None:
        if cache.is_symlink():
            fail("distribution cache directory is a symlink")
        cache.mkdir(parents=True, exist_ok=True)
        if not cache.is_dir():
            fail("distribution cache root is not a directory")
        cached = cache / f"{expected_sha256}.blob"
        if cached.exists() or cached.is_symlink():
            if cached.stat().st_size == 0 or cached.stat().st_size > MAX_DOWNLOAD_BYTES or sha256_file(cached) != expected_sha256:
                fail("cached distribution SHA-256 mismatch: fail closed without network fallback")
            with cached.open("rb") as source, target.open("xb") as output:
                shutil.copyfileobj(source, output, CHUNK_BYTES)
            if sha256_file(target) != expected_sha256:
                fail("cached distribution changed during copy")
            return

    request = urllib.request.Request(url, headers={"User-Agent": "profile-bridge-release-build/1"})
    for attempt in range(1, 4):
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
            target.unlink(missing_ok=True)
            raise
        except (OSError, urllib.error.URLError) as error:
            target.unlink(missing_ok=True)
            if not retryable_download_error(error) or attempt == 3:
                raise RuntimeBuildError(
                    f"{label} download failed after {attempt} attempt(s); "
                    f"{safe_download_error_detail(error)}"
                ) from error
            print(
                f"Retrying {label} download after transient transport failure "
                f"(attempt {attempt}/3; {safe_download_error_detail(error)}).",
                file=sys.stderr,
            )
            time.sleep(float(attempt))
            continue

        if total == 0 or digest.hexdigest() != expected_sha256:
            target.unlink(missing_ok=True)
            fail(f"{label} download digest mismatch")
        return

    if cached is not None:
        # Publish only fully verified bytes; no replacement of any prior cache.
        # A crash before the hardlink leaves a .pending entry which is NOT
        # treated as cache authority. The next job still verifies every hit.
        with tempfile.NamedTemporaryFile(prefix=".pending-", dir=cache, delete=False) as staged:
            staged_path = Path(staged.name)
            with target.open("rb") as source:
                shutil.copyfileobj(source, staged, CHUNK_BYTES)
            staged.flush()
            os.fsync(staged.fileno())
        try:
            if sha256_file(staged_path) != expected_sha256:
                fail("distribution cache staging changed")
            try:
                os.link(staged_path, cached)
            except FileExistsError:
                if sha256_file(cached) != expected_sha256:
                    fail("concurrent distribution cache mismatch")
        finally:
            staged_path.unlink(missing_ok=True)


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


def verify_official_archive(archive: Path, lock: dict[str, Any]) -> dict[str, Any]:
    """Verify exact official release bytes without any browser patch."""
    browser = lock["windows_distribution"]["browser"]
    if archive.is_symlink() or not archive.is_file():
        fail("official Camoufox archive missing/not regular")
    if not 0 < archive.stat().st_size <= MAX_DOWNLOAD_BYTES:
        fail("official Camoufox archive exceeds bounded size")
    if sha256_file(archive) != browser["artifact_sha256"]:
        fail("official Camoufox SHA-256 mismatch")
    release = lock["browser"]
    return {
        "artifact_sha256": browser["artifact_sha256"],
        "artifact_url": browser["artifact_url"],
        "release_commit": release["release_commit"],
        "repository": release["repository"],
        "version": release["version"],
    }


def rewrite_embedded_python_path(root: Path) -> None:
    path = root / "python312._pth"
    if path.is_symlink() or not path.is_file(): fail("embedded Python path configuration is missing")
    path.write_text("python312.zip\n.\nLib\\site-packages\n..\\camouhost\nimport site\n", encoding="utf-8", newline="\n")


def install_python_components(build_python: Path, lock: dict[str, Any], python_root: Path, distribution_cache: Path | None = None) -> None:
    packages = validate_python_packages(lock["windows_distribution"]["python"], lock["components"])
    site = python_root / "Lib/site-packages"; site.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="camouhost-wheels-") as directory:
        wheels: list[str] = []
        for row in packages:
            target = Path(directory) / row["filename"]
            download_exact(
                row["url"],
                row["sha256"],
                target,
                distribution_cache,
                label=f"locked Python wheel {row['filename']}",
            )
            wheels.append(str(target))
        command = [str(build_python), "-m", "pip", "install", "--disable-pip-version-check", "--no-cache-dir", "--no-compile", "--no-deps", "--no-index", "--no-warn-script-location", "--target", str(site), *wheels]
        if subprocess.run(command, cwd=ROOT, check=False).returncode != 0: fail("exact locked Python wheel installation failed")



def pin_official_fpgen_model(python_root: Path) -> None:
    """Prepare Camoufox's own checksum-pinned fpgen model at build time, never on user Open."""
    python = python_root / "python.exe"
    program = (
        "from camoufox.fpgen_model import ensure_fpgen_model,is_pinned;"
        "ensure_fpgen_model();"
        "assert is_pinned(), 'pinned Camoufox fpgen model verification failed'"
    )
    build_env = os.environ.copy()
    custom_model_override = bool(build_env.pop("FPGEN_MODEL_URL", None))
    if custom_model_override:
        print("Ignoring FPGEN_MODEL_URL for the checksum-locked runtime build.", file=sys.stderr)
    last_diagnostic = ""
    for attempt in range(1, 4):
        try:
            result = subprocess.run(
                [str(python), "-I", "-c", program],
                cwd=python_root,
                text=True,
                capture_output=True,
                check=False,
                timeout=90,
                env=build_env,
            )
        except subprocess.TimeoutExpired as error:
            last_diagnostic = f"attempt {attempt}: pinned fpgen model install timed out"
        else:
            if result.returncode == 0:
                return
            last_diagnostic = (result.stderr or result.stdout or "").strip()
            last_diagnostic = "\n".join(last_diagnostic.splitlines()[-12:])
            if not last_diagnostic:
                last_diagnostic = f"attempt {attempt}: no subprocess diagnostic output"
        if attempt < 3:
            time.sleep(float(attempt))
    fail(
        "official Camoufox fpgen model pre-install failed after 3 attempts; "
        f"custom_model_override={int(custom_model_override)}; {last_diagnostic}"
    )



def remove_python_caches(root: Path) -> None:
    for path in sorted(root.rglob("__pycache__"), reverse=True):
        if path.is_symlink() or not path.is_dir(): fail("Python cache path is unsafe")
        shutil.rmtree(path)
    for path in root.rglob("*.pyc"):
        if path.is_symlink() or not path.is_file(): fail("Python bytecode path is unsafe")
        path.unlink()


def verify_runtime_tree(output: Path, lock: dict[str, Any], candidate_provenance: dict[str, Any]) -> dict[str, Any]:
    required = [output/"python/python.exe", output/"browser/camoufox.exe", output/"camouhost/real.py", output/"camouhost/native_process.py", output/"camouhost/runtime-lock.json", output/"camouhost/browser-official-provenance.json"]
    for path in required:
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0: fail(f"materialized runtime file missing/not regular: {path}")
    components = lock["components"]
    probe = "import importlib.metadata,json,sys;print(json.dumps({'python':'.'.join(map(str,sys.version_info[:3])),'camoufox':importlib.metadata.version('camoufox'),'fpgen':importlib.metadata.version('fpgen'),'playwright':importlib.metadata.version('playwright')},sort_keys=True,separators=(',',':')))"
    completed = subprocess.run([str(output/"python/python.exe"), "-I", "-c", probe], cwd=output, text=True, capture_output=True, check=False)
    expected = {"python":"3.12.10", "camoufox":components["camoufox_python"], "fpgen":components["fpgen"], "playwright":components["playwright"]}
    try: observed = json.loads(completed.stdout.strip())
    except json.JSONDecodeError as error: raise RuntimeBuildError("embedded Python verification output is invalid") from error
    if completed.returncode != 0 or observed != expected: fail("materialized Python component identity mismatch")
    # The embeddable interpreter must resolve the packaged native adapter by
    # its normal ._pth search path. This proves the shipping import contract
    # with the actual isolated interpreter, not the build host's Python path.
    native_probe = subprocess.run(
        [
            str(output / "python/python.exe"),
            "-I", "-c",
            "import json,native_process,pathlib; print(json.dumps(str(pathlib.Path(native_process.__file__).resolve())))",
        ],
        cwd=output,
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
    )
    expected_native = str((output / "camouhost/native_process.py").resolve())
    try:
        observed_native = json.loads(native_probe.stdout.strip())
    except json.JSONDecodeError as error:
        raise RuntimeBuildError("embedded Python native adapter import output is invalid") from error
    if native_probe.returncode != 0 or observed_native != expected_native:
        fail("embedded Python cannot import the packaged native Camouhost adapter")
    model_probe = subprocess.run(
        [
            str(output / "python/python.exe"),
            "-I", "-c",
            "from camoufox.fpgen_model import is_pinned; "
            "raise SystemExit(0 if is_pinned() else 1)",
        ],
        cwd=output,
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
    )
    if model_probe.returncode != 0:
        fail("pinned official fpgen model is missing or corrupt")
    inventory=[]; total=0
    for path in sorted(output.rglob("*")):
        if path.is_symlink(): fail("materialized runtime contains symlink")
        if path.is_dir(): continue
        if not path.is_file(): fail("materialized runtime contains special entry")
        size=path.stat().st_size; total += size
        if total > MAX_EXTRACTED_BYTES: fail("materialized runtime exceeds bounded size")
        inventory.append({"path":path.relative_to(output).as_posix(),"sha256":sha256_file(path),"size_bytes":size})
    return {"schema_version":1,"kind":"CAMOUHOST_WINDOWS_RESOLVED_RUNTIME","components":observed,"browser_candidate_sha256":candidate_provenance["artifact_sha256"],"browser_candidate_provenance_sha256":hashlib.sha256(canonical(candidate_provenance)).hexdigest(),"files":len(inventory),"inventory_sha256":hashlib.sha256(canonical(inventory)).hexdigest(),"total_size_bytes":total}


def materialize(
    *,
    runtime_lock: Path,
    runtime_source: Path,
    output: Path,
    build_python: Path,
    distribution_cache: Path | None = None,
) -> None:
    if os.name != "nt": fail("Windows runtime materialization requires Windows")
    if output.exists() or output.is_symlink(): fail("runtime output must not already exist")
    if runtime_source.is_symlink() or not runtime_source.is_file():
        fail("real Camouhost source missing/not regular")
    # One repository-owned native launch primitive is packaged alongside the
    # supported real.py entrypoint. Never download executable Python at Open.
    if DEFAULT_NATIVE_SOURCE.is_symlink() or not DEFAULT_NATIVE_SOURCE.is_file():
        fail("native Camoufox source missing/not regular")
    build_python = build_python.resolve(strict=True)
    lock = load_lock(runtime_lock)
    with tempfile.TemporaryDirectory(prefix="camouhost-windows-build-") as directory:
        temp = Path(directory)
        python_archive = temp / "python.zip"
        browser_archive = temp / "official-camoufox.zip"
        python_lock = lock["windows_distribution"]["python"]
        browser_lock = lock["windows_distribution"]["browser"]
        download_exact(
            browser_lock["artifact_url"],
            browser_lock["artifact_sha256"],
            browser_archive,
            distribution_cache,
            label="official Camoufox browser archive",
        )
        provenance = verify_official_archive(browser_archive, lock)
        download_exact(
            python_lock["artifact_url"],
            python_lock["artifact_sha256"],
            python_archive,
            distribution_cache,
            label="official Python embeddable distribution",
        )
        output.mkdir(parents=True, exist_ok=False)
        try:
            extract_exact_zip(python_archive, output / "python")
            extract_exact_zip(browser_archive, output / "browser")
            rewrite_embedded_python_path(output / "python")
            install_python_components(build_python, lock, output / "python", distribution_cache)
            pin_official_fpgen_model(output / "python")
            camouhost = output / "camouhost"
            camouhost.mkdir()
            shutil.copyfile(runtime_source, camouhost / "real.py")
            shutil.copyfile(DEFAULT_NATIVE_SOURCE, camouhost / "native_process.py")
            shutil.copyfile(runtime_lock, camouhost / "runtime-lock.json")
            (camouhost / "browser-official-provenance.json").write_bytes(canonical(provenance))
            remove_python_caches(output)
            resolved = verify_runtime_tree(output, lock, provenance)
            (camouhost / "resolved-runtime.json").write_bytes(canonical(resolved))
            verify_runtime_tree(output, lock, provenance)
        except BaseException:
            shutil.rmtree(output, ignore_errors=True)
            raise


def self_test() -> None:
    for invalid_tag in ("159.0-beta.43", "v159.0", "v159.0-beta.0", "v159.0-beta.43-extra"):
        try:
            official_release_pin(invalid_tag, "0.5.10b2")
        except RuntimeBuildError:
            pass
        else:
            fail(f"invalid official browser release tag accepted: {invalid_tag}")
    valid_url = "https://github.com/daijro/camoufox/releases/download/v159.0-beta.43/camoufox-159.0-beta.43-win.x86_64.zip"
    valid_release = {"tag_name": "v159.0-beta.43", "assets": [{"browser_download_url": valid_url, "digest": "sha256:" + "a" * 64}]}
    with patch(__name__ + ".public_json", side_effect=[valid_release, {"sha": "b" * 40}, {"sha": "b" * 40}]):
        if official_release_pin("v159.0-beta.43", "0.5.10b2") != ("b" * 40, valid_url, "a" * 64):
            fail("paired official release pin is not exact")
    with patch(__name__ + ".public_json", side_effect=[valid_release, {"sha": "b" * 40}, {"sha": "c" * 40}]):
        try:
            official_release_pin("v159.0-beta.43", "0.5.10b2")
        except RuntimeBuildError:
            pass
        else:
            fail("mismatched browser/SDK release commits were accepted")
    with tempfile.TemporaryDirectory(prefix="camouhost-wheel-meta-") as directory:
        wheel = Path(directory) / "sample.whl"
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr("sample-1.2.0.dist-info/METADATA", "Metadata-Version: 2.1\nName: Sample_Pkg\nVersion: 1.2.0\n")
        if wheel_identity(wheel) != ("sample-pkg", "1.2.0"):
            fail("resolved wheel metadata identity mismatch")
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
    # A transient transport failure is retried, but only checksum-matching
    # bytes can become the resulting artifact. The test itself makes no network call.
    with tempfile.TemporaryDirectory(prefix="camouhost-download-retry-test-") as directory:
        target = Path(directory) / "retry.blob"
        payload = b"exact-locked-artifact-after-transient-failure"
        attempts = 0

        def flaky_urlopen(_request, timeout):
            nonlocal attempts
            if timeout != 60:
                fail("distribution download timeout contract changed")
            attempts += 1
            if attempts == 1:
                raise urllib.error.URLError("synthetic transient transport failure")
            return io.BytesIO(payload)

        with (
            patch.object(urllib.request, "urlopen", side_effect=flaky_urlopen),
            patch.object(time, "sleep"),
        ):
            download_exact(
                "https://invalid.example/retry",
                hashlib.sha256(payload).hexdigest(),
                target,
                label="self-test artifact",
            )
        if attempts != 2 or target.read_bytes() != payload:
            fail("transient download retry did not preserve exact artifact bytes")

    # Cache hits cannot grant trust or bypass an exact digest. No network
    # needed to prove a valid hit, nor to reject a corrupted prior cache.
    with tempfile.TemporaryDirectory(prefix="camouhost-locked-cache-test-") as directory:
        cache = Path(directory) / "distributions"
        cache.mkdir()
        data = b"immutable-locked-wheel-bytes"
        digest = hashlib.sha256(data).hexdigest()
        (cache / f"{digest}.blob").write_bytes(data)
        first = Path(directory) / "restored.whl"
        download_exact("https://invalid.example/no-download", digest, first, cache)
        if first.read_bytes() != data:
            fail("verified distribution cache hit did not preserve bytes")
        (cache / f"{digest}.blob").write_bytes(b"corrupt-cache-entry")
        try:
            download_exact("https://invalid.example/unsafe-fallback", digest, Path(directory)/"must-not-exist.whl", cache)
        except RuntimeBuildError:
            if (Path(directory)/"must-not-exist.whl").exists():
                fail("cache corruption produced a target artifact")
        else:
            fail("corrupt cache hit bypassed SHA-256")
    print("Windows Camouhost runtime materializer self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    commands = result.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build")
    build.add_argument("--runtime-lock", type=Path, default=DEFAULT_LOCK)
    build.add_argument("--runtime-source", type=Path, default=DEFAULT_RUNTIME_SOURCE)
    build.add_argument("--output", type=Path, required=True)
    build.add_argument("--build-python", type=Path, required=True)
    build.add_argument("--distribution-cache", type=Path, default=None)
    refresh = commands.add_parser("refresh-lock", help="pin a paired official Camoufox browser/SDK via GitHub and PyPI")
    refresh.add_argument("--runtime-lock", type=Path, default=DEFAULT_LOCK)
    refresh.add_argument("--browser-tag", required=True)
    refresh.add_argument("--sdk-version", required=True)
    commands.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "refresh-lock":
            update_official_runtime_lock(args.runtime_lock, args.browser_tag, args.sdk_version)
        else:
            materialize(
                runtime_lock=args.runtime_lock,
                runtime_source=args.runtime_source,
                output=args.output,
                build_python=args.build_python,
                distribution_cache=args.distribution_cache,
            )
        return 0
    except (RuntimeBuildError, OSError, subprocess.SubprocessError) as error:
        print(f"Windows runtime build error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
