#!/usr/bin/env python3
"""Project a pip resolver report into an exact Windows runtime dependency candidate.

This is an update-time ceremony helper only. `runtime/camouhost/runtime-lock.json`
remains the canonical runtime identity. Shipping/Release Set builds must consume
the committed exact package URLs and SHA-256 values and must never resolve
transitive dependency versions themselves.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_RUNTIME_LOCK = ROOT / "runtime" / "camouhost" / "runtime-lock.json"
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
PACKAGE_NAME_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
PYTHON_VERSION = "3.12.10"
ARCHITECTURE = "x86_64"


class DependencyLockError(ValueError):
    """Fail-closed dependency-lock ceremony error."""


def fail(message: str) -> None:
    raise DependencyLockError(message)


def canonical(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
        + "\n"
    ).encode("utf-8")


def normalized_package_name(value: str) -> str:
    normalized = re.sub(r"[-_.]+", "-", value).lower()
    if PACKAGE_NAME_RE.fullmatch(normalized) is None:
        fail(f"invalid normalized package name: {value!r}")
    return normalized


def load_json(path: Path, label: str) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} is missing/not regular")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise DependencyLockError(f"{label} is invalid JSON") from error
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def required_roots(runtime_lock: dict[str, Any]) -> dict[str, str]:
    components = runtime_lock.get("components")
    distribution = runtime_lock.get("windows_distribution")
    if not isinstance(components, dict) or not isinstance(distribution, dict):
        fail("runtime lock is missing component/distribution identity")
    python = distribution.get("python")
    if not isinstance(python, dict) or python.get("version") != PYTHON_VERSION:
        fail("runtime lock Windows Python version is not the lock-ceremony target")
    expected = {
        "camoufox": components.get("camoufox_python"),
        "browserforge": components.get("browserforge"),
        "playwright": components.get("playwright"),
    }
    if not all(isinstance(value, str) and value for value in expected.values()):
        fail("runtime lock root Python component pins are invalid")
    return {name: str(version) for name, version in expected.items()}


def package_from_report_row(row: object) -> dict[str, str]:
    if not isinstance(row, dict):
        fail("pip report install row must be an object")
    metadata = row.get("metadata")
    download = row.get("download_info")
    if not isinstance(metadata, dict) or not isinstance(download, dict):
        fail("pip report row is missing metadata/download_info")
    name = metadata.get("name")
    version = metadata.get("version")
    url = download.get("url")
    archive = download.get("archive_info")
    if not isinstance(name, str) or not isinstance(version, str) or not version:
        fail("pip report package identity is invalid")
    if not isinstance(url, str) or not url.startswith("https://files.pythonhosted.org/"):
        fail(f"runtime dependency must be an exact PyPI HTTPS artifact: {name}")
    if not isinstance(archive, dict):
        fail(f"pip report package archive identity is missing: {name}")
    hashes = archive.get("hashes")
    digest = hashes.get("sha256") if isinstance(hashes, dict) else None
    if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
        fail(f"pip report package SHA-256 is invalid: {name}")
    filename = Path(unquote(urlparse(url).path)).name
    if not filename or not filename.lower().endswith(".whl"):
        fail(f"runtime dependency must resolve to a wheel: {name}")
    return {
        "name": normalized_package_name(name),
        "version": version,
        "filename": filename,
        "url": url,
        "sha256": digest,
    }


def candidate(runtime_lock: dict[str, Any], report: dict[str, Any]) -> dict[str, Any]:
    roots = required_roots(runtime_lock)
    if report.get("version") != "1" or not isinstance(report.get("install"), list):
        fail("unsupported pip report schema")
    packages = [package_from_report_row(row) for row in report["install"]]
    packages.sort(key=lambda row: row["name"])
    if not packages:
        fail("pip report resolved no runtime dependencies")
    names = [row["name"] for row in packages]
    if len(names) != len(set(names)):
        fail("pip report contains duplicate normalized package identities")
    by_name = {row["name"]: row for row in packages}
    for name, version in roots.items():
        row = by_name.get(name)
        if row is None or row["version"] != version:
            fail(f"pip report root dependency drifted: {name}")
    return {
        "schema_version": 1,
        "kind": "CAMOUHOST_WINDOWS_PYTHON_DEPENDENCY_LOCK_CANDIDATE",
        "python_version": PYTHON_VERSION,
        "architecture": ARCHITECTURE,
        "packages": packages,
    }


def self_test() -> None:
    runtime_lock = {
        "components": {
            "camoufox_python": "0.5.5",
            "browserforge": "1.2.4",
            "playwright": "1.60.0",
        },
        "windows_distribution": {"python": {"version": PYTHON_VERSION}},
    }
    report = {
        "version": "1",
        "install": [
            {
                "download_info": {
                    "url": "https://files.pythonhosted.org/packages/aa/camoufox-0.5.5-py3-none-any.whl",
                    "archive_info": {"hashes": {"sha256": "a" * 64}},
                },
                "metadata": {"name": "Camoufox", "version": "0.5.5"},
            },
            {
                "download_info": {
                    "url": "https://files.pythonhosted.org/packages/bb/browserforge-1.2.4-py3-none-any.whl",
                    "archive_info": {"hashes": {"sha256": "b" * 64}},
                },
                "metadata": {"name": "browserforge", "version": "1.2.4"},
            },
            {
                "download_info": {
                    "url": "https://files.pythonhosted.org/packages/cc/playwright-1.60.0-py3-none-win_amd64.whl",
                    "archive_info": {"hashes": {"sha256": "c" * 64}},
                },
                "metadata": {"name": "playwright", "version": "1.60.0"},
            },
        ],
    }
    projected = candidate(runtime_lock, report)
    if [row["name"] for row in projected["packages"]] != [
        "browserforge",
        "camoufox",
        "playwright",
    ]:
        fail("dependency candidate ordering self-test failed")

    wrong_hash = json.loads(json.dumps(report))
    wrong_hash["install"][0]["download_info"]["archive_info"]["hashes"]["sha256"] = "BAD"
    try:
        candidate(runtime_lock, wrong_hash)
    except DependencyLockError:
        pass
    else:
        fail("invalid dependency hash negative self-test unexpectedly passed")

    sdist = json.loads(json.dumps(report))
    sdist["install"][0]["download_info"]["url"] = (
        "https://files.pythonhosted.org/packages/aa/camoufox-0.5.5.tar.gz"
    )
    try:
        candidate(runtime_lock, sdist)
    except DependencyLockError:
        pass
    else:
        fail("sdist dependency negative self-test unexpectedly passed")

    missing_root = json.loads(json.dumps(report))
    missing_root["install"] = missing_root["install"][1:]
    try:
        candidate(runtime_lock, missing_root)
    except DependencyLockError:
        pass
    else:
        fail("missing root dependency negative self-test unexpectedly passed")
    print("Windows runtime dependency lock ceremony self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)
    project = subcommands.add_parser("project")
    project.add_argument("--runtime-lock", type=Path, default=DEFAULT_RUNTIME_LOCK)
    project.add_argument("--pip-report", type=Path, required=True)
    subcommands.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "project":
            runtime_lock = load_json(args.runtime_lock, "runtime lock")
            report = load_json(args.pip_report, "pip report")
            sys.stdout.buffer.write(canonical(candidate(runtime_lock, report)))
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except DependencyLockError as error:
        print(f"Windows dependency lock error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
