#!/usr/bin/env python3
"""Generate deterministic Windows delivery SBOM/provenance evidence.

This is a build-time projection adapter only. Component packaging remains owned by the
Profile Bridge/runtime packagers and aggregate candidate identity/publication remains
owned by Release Set v3. The generated evidence is content-addressed by Release Set v3
before a signed Windows delivery manifest can reference it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "iamaman11/part-crm-emai-profile"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
BRIDGE_PREFIX = "profile-bridge-v2-sha256-"
RUNTIME_PREFIX = "runtime-bundle-v2-sha256-"
MAX_COMPONENT_MANIFEST_BYTES = 128 * 1024 * 1024

CANONICAL_INPUTS = {
    "cargo_lock": ROOT / "Cargo.lock",
    "rust_toolchain": ROOT / "rust-toolchain.toml",
    "runtime_lock": ROOT / "runtime/camouhost/runtime-lock.json",
    "release_architecture": ROOT / "architecture/release-architecture-ar11.json",
}


class WindowsDeliveryEvidenceError(ValueError):
    """Fail-closed evidence projection error."""


def fail(message: str) -> None:
    raise WindowsDeliveryEvidenceError(message)


def document(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"evidence input must be a regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def file_identity(path: Path, repo_relative: str | None = None) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail(f"evidence input must be a regular file: {path}")
    result: dict[str, Any] = {
        "sha256": sha256_file(path),
        "size_bytes": path.stat().st_size,
    }
    if repo_relative is not None:
        result["path"] = repo_relative
    return result


def exact_object(value: Any, fields: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != fields:
        fail(f"{label} field inventory mismatch")
    return value


def exact_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        fail(f"{label} must be a non-empty string")
    return value


def exact_positive_int(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        fail(f"{label} must be a positive integer")
    return value


def load_manifest(path: Path, label: str) -> tuple[dict[str, Any], bytes]:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} manifest must be a regular file")
    if path.stat().st_size <= 0 or path.stat().st_size > MAX_COMPONENT_MANIFEST_BYTES:
        fail(f"{label} manifest size is invalid")
    raw = path.read_bytes()
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise WindowsDeliveryEvidenceError(f"{label} manifest is invalid JSON") from error
    if not isinstance(value, dict):
        fail(f"{label} manifest must be an object")
    return value, raw


def validate_source_sha(value: str) -> str:
    if COMMIT_RE.fullmatch(value) is None:
        fail("source SHA must be exact 40 lowercase hexadecimal")
    return value


def validate_digest(value: Any, label: str) -> str:
    value = exact_string(value, label)
    if SHA256_RE.fullmatch(value) is None:
        fail(f"{label} must be exact lowercase SHA-256")
    return value


def validate_bridge_manifest(manifest: dict[str, Any], source_sha: str) -> dict[str, Any]:
    exact_object(
        manifest,
        {
            "schema_version",
            "kind",
            "source_commit_sha",
            "protocol_version",
            "executable",
            "release_id",
        },
        "Profile Bridge manifest",
    )
    if (
        manifest["schema_version"] != 2
        or manifest["kind"] != "PROFILE_BRIDGE_COMPONENT"
        or manifest["source_commit_sha"] != source_sha
        or manifest["protocol_version"] != 1
    ):
        fail("Profile Bridge manifest identity mismatch")
    release_id = exact_string(manifest["release_id"], "Profile Bridge release_id")
    if not release_id.startswith(BRIDGE_PREFIX) or SHA256_RE.fullmatch(
        release_id[len(BRIDGE_PREFIX) :]
    ) is None:
        fail("Profile Bridge release_id format mismatch")
    executable = exact_object(
        manifest["executable"],
        {"path", "sha256", "size_bytes"},
        "Profile Bridge executable",
    )
    if executable["path"] != "profile-bridge.exe":
        fail("Profile Bridge executable path mismatch")
    validate_digest(executable["sha256"], "Profile Bridge executable digest")
    exact_positive_int(executable["size_bytes"], "Profile Bridge executable size")
    return manifest


def validate_runtime_manifest(manifest: dict[str, Any], source_sha: str) -> dict[str, Any]:
    exact_object(
        manifest,
        {
            "schema_version",
            "kind",
            "platform",
            "source_commit_sha",
            "source_inputs",
            "files",
            "entrypoints",
            "release_id",
        },
        "runtime manifest",
    )
    if (
        manifest["schema_version"] != 2
        or manifest["kind"] != "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT"
        or manifest["platform"] != "windows-x86_64"
        or manifest["source_commit_sha"] != source_sha
    ):
        fail("runtime manifest identity mismatch")
    release_id = exact_string(manifest["release_id"], "runtime release_id")
    if not release_id.startswith(RUNTIME_PREFIX) or SHA256_RE.fullmatch(
        release_id[len(RUNTIME_PREFIX) :]
    ) is None:
        fail("runtime release_id format mismatch")
    for label in ("source_inputs", "files"):
        identity = exact_object(manifest[label], {"files", "sha256"}, f"runtime {label}")
        validate_digest(identity["sha256"], f"runtime {label} digest")
        if not isinstance(identity["files"], list) or not identity["files"]:
            fail(f"runtime {label} files must be a non-empty list")
    entrypoints = exact_object(
        manifest["entrypoints"],
        {"browser", "camouhost", "python", "runtime_lock"},
        "runtime entrypoints",
    )
    if entrypoints != {
        "browser": "browser/camoufox.exe",
        "camouhost": "camouhost/real.py",
        "python": "python/python.exe",
        "runtime_lock": "camouhost/runtime-lock.json",
    }:
        fail("runtime entrypoint identity mismatch")
    return manifest


def canonical_input_identities() -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for key, path in CANONICAL_INPUTS.items():
        relative = path.relative_to(ROOT).as_posix()
        result[key] = file_identity(path, relative)
    return result


def build_evidence(
    *,
    source_sha: str,
    bridge_manifest_path: Path,
    bridge_archive_path: Path,
    runtime_manifest_path: Path,
    runtime_archive_path: Path,
) -> tuple[bytes, bytes]:
    source_sha = validate_source_sha(source_sha)
    bridge_manifest, bridge_manifest_bytes = load_manifest(
        bridge_manifest_path, "Profile Bridge"
    )
    runtime_manifest, runtime_manifest_bytes = load_manifest(runtime_manifest_path, "runtime")
    validate_bridge_manifest(bridge_manifest, source_sha)
    validate_runtime_manifest(runtime_manifest, source_sha)
    canonical_inputs = canonical_input_identities()

    bridge_archive = file_identity(bridge_archive_path)
    runtime_archive = file_identity(runtime_archive_path)
    bridge_manifest_digest = sha256_bytes(bridge_manifest_bytes)
    runtime_manifest_digest = sha256_bytes(runtime_manifest_bytes)

    sbom = {
        "schema_version": 1,
        "kind": "WINDOWS_DELIVERY_SBOM",
        "source_commit_sha": source_sha,
        "scope": "profile_bridge+runtime_bundle",
        "components": {
            "profile_bridge": {
                "release_id": bridge_manifest["release_id"],
                "component_manifest_sha256": bridge_manifest_digest,
                "executable": bridge_manifest["executable"],
            },
            "runtime_bundle": {
                "release_id": runtime_manifest["release_id"],
                "component_manifest_sha256": runtime_manifest_digest,
                "source_inputs": runtime_manifest["source_inputs"],
                "files": runtime_manifest["files"],
                "entrypoints": runtime_manifest["entrypoints"],
            },
        },
        "dependency_inputs": {
            "cargo_lock": canonical_inputs["cargo_lock"],
            "rust_toolchain": canonical_inputs["rust_toolchain"],
            "runtime_lock": canonical_inputs["runtime_lock"],
        },
    }
    provenance = {
        "schema_version": 1,
        "kind": "WINDOWS_DELIVERY_PROVENANCE",
        "source": {
            "repository": REPOSITORY,
            "commit_sha": source_sha,
            "accepted_main": True,
        },
        "builder": {
            "authority": "accepted-main-release-set-v3",
            "profile_bridge_platform": "windows-latest",
            "runtime_bundle_platform": "windows-2025",
        },
        "components": {
            "profile_bridge": {
                "release_id": bridge_manifest["release_id"],
                "artifact_sha256": bridge_archive["sha256"],
                "artifact_size_bytes": bridge_archive["size_bytes"],
                "component_manifest_sha256": bridge_manifest_digest,
            },
            "runtime_bundle": {
                "release_id": runtime_manifest["release_id"],
                "artifact_sha256": runtime_archive["sha256"],
                "artifact_size_bytes": runtime_archive["size_bytes"],
                "component_manifest_sha256": runtime_manifest_digest,
            },
        },
        "canonical_inputs": canonical_inputs,
    }
    return document(sbom), document(provenance)


def write_new(path: Path, data: bytes, label: str) -> None:
    if path.exists():
        fail(f"{label} output already exists")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def expect_failure(action: Callable[[], None], marker: str) -> None:
    try:
        action()
    except WindowsDeliveryEvidenceError as error:
        if marker not in str(error):
            fail(f"negative self-test expected {marker!r}, observed {error}")
    else:
        fail(f"negative self-test unexpectedly passed: {marker}")


def self_test() -> None:
    source_sha = "1" * 40
    with tempfile.TemporaryDirectory(prefix="windows-delivery-evidence-") as directory:
        root = Path(directory)
        bridge_archive = root / "profile-bridge.zip"
        runtime_archive = root / "runtime-bundle.tar"
        bridge_archive.write_bytes(b"bridge-archive")
        runtime_archive.write_bytes(b"runtime-archive")
        bridge_manifest = {
            "schema_version": 2,
            "kind": "PROFILE_BRIDGE_COMPONENT",
            "source_commit_sha": source_sha,
            "protocol_version": 1,
            "executable": {
                "path": "profile-bridge.exe",
                "sha256": "2" * 64,
                "size_bytes": 123,
            },
            "release_id": BRIDGE_PREFIX + "3" * 64,
        }
        runtime_manifest = {
            "schema_version": 2,
            "kind": "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT",
            "platform": "windows-x86_64",
            "source_commit_sha": source_sha,
            "source_inputs": {
                "files": [{"path": "runtime/camouhost/runtime-lock.json", "sha256": "4" * 64, "size_bytes": 1}],
                "sha256": "5" * 64,
            },
            "files": {
                "files": [{"path": "browser/camoufox.exe", "sha256": "6" * 64, "size_bytes": 1}],
                "sha256": "7" * 64,
            },
            "entrypoints": {
                "browser": "browser/camoufox.exe",
                "camouhost": "camouhost/real.py",
                "python": "python/python.exe",
                "runtime_lock": "camouhost/runtime-lock.json",
            },
            "release_id": RUNTIME_PREFIX + "8" * 64,
        }
        bridge_manifest_path = root / "profile-bridge-manifest.json"
        runtime_manifest_path = root / "runtime-manifest.json"
        bridge_manifest_path.write_bytes(document(bridge_manifest))
        runtime_manifest_path.write_bytes(document(runtime_manifest))

        first = build_evidence(
            source_sha=source_sha,
            bridge_manifest_path=bridge_manifest_path,
            bridge_archive_path=bridge_archive,
            runtime_manifest_path=runtime_manifest_path,
            runtime_archive_path=runtime_archive,
        )
        second = build_evidence(
            source_sha=source_sha,
            bridge_manifest_path=bridge_manifest_path,
            bridge_archive_path=bridge_archive,
            runtime_manifest_path=runtime_manifest_path,
            runtime_archive_path=runtime_archive,
        )
        if first != second:
            fail("Windows delivery evidence is not deterministic")
        sbom = json.loads(first[0])
        provenance = json.loads(first[1])
        if sbom["kind"] != "WINDOWS_DELIVERY_SBOM" or provenance["kind"] != "WINDOWS_DELIVERY_PROVENANCE":
            fail("Windows delivery evidence kind self-test failed")
        if provenance["source"]["commit_sha"] != source_sha:
            fail("Windows delivery provenance source self-test failed")

        wrong = dict(bridge_manifest)
        wrong["source_commit_sha"] = "9" * 40
        wrong_path = root / "wrong-bridge.json"
        wrong_path.write_bytes(document(wrong))
        expect_failure(
            lambda: build_evidence(
                source_sha=source_sha,
                bridge_manifest_path=wrong_path,
                bridge_archive_path=bridge_archive,
                runtime_manifest_path=runtime_manifest_path,
                runtime_archive_path=runtime_archive,
            ),
            "identity mismatch",
        )

    print("Windows delivery evidence self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    commands = result.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build")
    build.add_argument("--source-sha", required=True)
    build.add_argument("--profile-bridge-manifest", type=Path, required=True)
    build.add_argument("--profile-bridge-archive", type=Path, required=True)
    build.add_argument("--runtime-manifest", type=Path, required=True)
    build.add_argument("--runtime-archive", type=Path, required=True)
    build.add_argument("--sbom", type=Path, required=True)
    build.add_argument("--provenance", type=Path, required=True)
    commands.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "build":
            sbom, provenance = build_evidence(
                source_sha=args.source_sha,
                bridge_manifest_path=args.profile_bridge_manifest,
                bridge_archive_path=args.profile_bridge_archive,
                runtime_manifest_path=args.runtime_manifest,
                runtime_archive_path=args.runtime_archive,
            )
            write_new(args.sbom, sbom, "SBOM")
            write_new(args.provenance, provenance, "provenance")
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except WindowsDeliveryEvidenceError as error:
        print(f"Windows delivery evidence error: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
