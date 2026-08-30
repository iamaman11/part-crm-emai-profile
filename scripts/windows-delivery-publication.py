#!/usr/bin/env python3
"""Project one exact Release Set v3 into the canonical Windows delivery manifest.

Release Set v3 remains the candidate/publication owner. This adapter has no signing key,
provider mutation, updater, activation or browser authority. It only validates the exact
Release Set/component/evidence observations and emits the byte shape owned by the Profile
Bridge Windows delivery contract.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
from pathlib import Path
from typing import Any, Callable

REPOSITORY = "iamaman11/part-crm-emai-profile"
RELEASE_SET_PREFIX = "release-set-v3-sha256-"
BRIDGE_PREFIX = "profile-bridge-v2-sha256-"
RUNTIME_PREFIX = "runtime-bundle-v2-sha256-"
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
WINDOWS_SBOM_PATH = "windows/windows-sbom-v1.json"
WINDOWS_PROVENANCE_PATH = "windows/windows-provenance-v1.json"
MAX_RELEASE_SET_BYTES = 8 * 1024 * 1024
MAX_EVIDENCE_BYTES = 256 * 1024 * 1024


class WindowsDeliveryPublicationError(ValueError):
    """Fail-closed publication projection error."""


def fail(message: str) -> None:
    raise WindowsDeliveryPublicationError(message)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def load_json(path: Path, label: str, max_bytes: int) -> tuple[dict[str, Any], bytes]:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file")
    size = path.stat().st_size
    if size <= 0 or size > max_bytes:
        fail(f"{label} size is invalid")
    raw = path.read_bytes()
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise WindowsDeliveryPublicationError(f"{label} is invalid JSON") from error
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value, raw


def exact_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        fail(f"{label} must be a non-empty string")
    return value


def exact_positive_int(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        fail(f"{label} must be a positive integer")
    return value


def digest(value: Any, label: str) -> str:
    value = exact_string(value, label)
    if SHA256_RE.fullmatch(value) is None:
        fail(f"{label} must be exact lowercase SHA-256")
    return value


def prefixed_digest(value: Any, prefix: str, label: str) -> str:
    value = exact_string(value, label)
    if not value.startswith(prefix) or SHA256_RE.fullmatch(value[len(prefix) :]) is None:
        fail(f"{label} content-address format mismatch")
    return value


def object_field(value: dict[str, Any], key: str, label: str) -> dict[str, Any]:
    result = value.get(key)
    if not isinstance(result, dict):
        fail(f"{label}.{key} must be an object")
    return result


def array_field(value: dict[str, Any], key: str, label: str) -> list[Any]:
    result = value.get(key)
    if not isinstance(result, list):
        fail(f"{label}.{key} must be an array")
    return result


def artifact_identity(
    release_set: dict[str, Any],
    *,
    path: str,
    kind: str,
    evidence_bytes: bytes,
) -> dict[str, Any]:
    matches = [
        row
        for row in array_field(release_set, "artifact_inventory", "Release Set")
        if isinstance(row, dict) and row.get("path") == path
    ]
    if len(matches) != 1:
        fail(f"Release Set must contain exactly one evidence artifact {path}")
    row = matches[0]
    if row.get("kind") != kind:
        fail(f"Release Set evidence kind mismatch for {path}")
    expected_sha = sha256_bytes(evidence_bytes)
    if row.get("sha256") != expected_sha or row.get("size_bytes") != len(evidence_bytes):
        fail(f"Release Set evidence identity mismatch for {path}")
    return row


def component_identity(
    release_set: dict[str, Any],
    *,
    key: str,
    expected_path: str,
    prefix: str,
    source_sha: str,
) -> dict[str, Any]:
    components = object_field(release_set, "components", "Release Set")
    component = components.get(key)
    if not isinstance(component, dict):
        fail(f"Release Set component missing: {key}")
    if component.get("component_id") != key or component.get("artifact_path") != expected_path:
        fail(f"Release Set component identity mismatch: {key}")
    if component.get("source_commit_sha") != source_sha:
        fail(f"Release Set component source mismatch: {key}")
    prefixed_digest(component.get("release_id"), prefix, f"{key} release_id")
    digest(component.get("artifact_sha256"), f"{key} artifact_sha256")
    exact_positive_int(component.get("artifact_size_bytes"), f"{key} artifact_size_bytes")
    digest(component.get("component_manifest_sha256"), f"{key} component_manifest_sha256")
    return component


def render_manifest(
    *,
    release_set_path: Path,
    sbom_path: Path,
    provenance_path: Path,
    sequence: int,
) -> bytes:
    sequence = exact_positive_int(sequence, "delivery sequence")
    release_set, _ = load_json(release_set_path, "Release Set", MAX_RELEASE_SET_BYTES)
    sbom, sbom_bytes = load_json(sbom_path, "Windows SBOM", MAX_EVIDENCE_BYTES)
    provenance, provenance_bytes = load_json(
        provenance_path, "Windows provenance", MAX_EVIDENCE_BYTES
    )

    if release_set.get("schema_version") != 3:
        fail("Release Set schema_version must be 3")
    release_set_id = prefixed_digest(
        release_set.get("release_set_id"), RELEASE_SET_PREFIX, "Release Set ID"
    )
    source = object_field(release_set, "source", "Release Set")
    source_sha = exact_string(source.get("commit_sha"), "Release Set source SHA")
    if COMMIT_RE.fullmatch(source_sha) is None:
        fail("Release Set source SHA format mismatch")
    if source.get("repository") != REPOSITORY or source.get("accepted_main") is not True:
        fail("Release Set source authority mismatch")
    if sbom.get("schema_version") != 1 or sbom.get("kind") != "WINDOWS_DELIVERY_SBOM":
        fail("Windows SBOM identity mismatch")
    if sbom.get("source_commit_sha") != source_sha:
        fail("Windows SBOM source mismatch")
    provenance_source = object_field(provenance, "source", "Windows provenance")
    if (
        provenance.get("schema_version") != 1
        or provenance.get("kind") != "WINDOWS_DELIVERY_PROVENANCE"
        or provenance_source.get("repository") != REPOSITORY
        or provenance_source.get("commit_sha") != source_sha
        or provenance_source.get("accepted_main") is not True
    ):
        fail("Windows provenance identity mismatch")

    artifact_identity(
        release_set,
        path=WINDOWS_SBOM_PATH,
        kind="windows-delivery-sbom",
        evidence_bytes=sbom_bytes,
    )
    artifact_identity(
        release_set,
        path=WINDOWS_PROVENANCE_PATH,
        kind="windows-delivery-provenance",
        evidence_bytes=provenance_bytes,
    )
    bridge = component_identity(
        release_set,
        key="profile_bridge",
        expected_path="components/profile-bridge.zip",
        prefix=BRIDGE_PREFIX,
        source_sha=source_sha,
    )
    runtime = component_identity(
        release_set,
        key="runtime_bundle",
        expected_path="components/runtime-bundle.tar",
        prefix=RUNTIME_PREFIX,
        source_sha=source_sha,
    )
    protocols = object_field(release_set, "protocols", "Release Set")
    bridge_protocol = exact_positive_int(
        protocols.get("profile_bridge_protocol_version"),
        "profile_bridge_protocol_version",
    )
    camouhost_ipc = exact_positive_int(
        protocols.get("camouhost_ipc_version"), "camouhost_ipc_version"
    )
    if bridge_protocol != 1:
        fail("Profile Bridge protocol is incompatible with Windows delivery v1")

    # Field order is intentionally identical to the Rust `WindowsDeliveryManifest` serde order.
    manifest = {
        "schema_version": 1,
        "kind": "WINDOWS_PROFILE_BRIDGE_DELIVERY",
        "release_set_id": release_set_id,
        "sequence": sequence,
        "source_commit_sha": source_sha,
        "components": {
            "profile_bridge": {
                "release_id": bridge["release_id"],
                "artifact_sha256": bridge["artifact_sha256"],
                "artifact_size_bytes": bridge["artifact_size_bytes"],
                "component_manifest_sha256": bridge["component_manifest_sha256"],
            },
            "runtime_bundle": {
                "release_id": runtime["release_id"],
                "artifact_sha256": runtime["artifact_sha256"],
                "artifact_size_bytes": runtime["artifact_size_bytes"],
                "component_manifest_sha256": runtime["component_manifest_sha256"],
            },
        },
        "evidence": {
            "sbom_sha256": sha256_bytes(sbom_bytes),
            "provenance_sha256": sha256_bytes(provenance_bytes),
        },
        "compatibility": {
            "profile_bridge_protocol_version": bridge_protocol,
            "camouhost_ipc_version": camouhost_ipc,
            "runtime_bundle_version": "2.0.0",
        },
    }
    return json.dumps(manifest, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def write_new(path: Path, data: bytes) -> None:
    if path.exists():
        fail("delivery manifest output already exists")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def expect_failure(action: Callable[[], None], marker: str) -> None:
    try:
        action()
    except WindowsDeliveryPublicationError as error:
        if marker not in str(error):
            fail(f"negative self-test expected {marker!r}, observed {error}")
    else:
        fail(f"negative self-test unexpectedly passed: {marker}")


def self_test() -> None:
    source_sha = "1" * 40
    release_set_id = RELEASE_SET_PREFIX + "2" * 64
    bridge_id = BRIDGE_PREFIX + "3" * 64
    runtime_id = RUNTIME_PREFIX + "4" * 64
    sbom = {
        "schema_version": 1,
        "kind": "WINDOWS_DELIVERY_SBOM",
        "source_commit_sha": source_sha,
    }
    provenance = {
        "schema_version": 1,
        "kind": "WINDOWS_DELIVERY_PROVENANCE",
        "source": {
            "repository": REPOSITORY,
            "commit_sha": source_sha,
            "accepted_main": True,
        },
    }
    sbom_bytes = (json.dumps(sbom, sort_keys=True, indent=2) + "\n").encode()
    provenance_bytes = (json.dumps(provenance, sort_keys=True, indent=2) + "\n").encode()
    release_set = {
        "schema_version": 3,
        "release_set_id": release_set_id,
        "source": {
            "repository": REPOSITORY,
            "commit_sha": source_sha,
            "accepted_main": True,
        },
        "components": {
            "profile_bridge": {
                "component_id": "profile_bridge",
                "release_id": bridge_id,
                "source_commit_sha": source_sha,
                "artifact_path": "components/profile-bridge.zip",
                "artifact_sha256": "5" * 64,
                "artifact_size_bytes": 10,
                "component_manifest_sha256": "6" * 64,
            },
            "runtime_bundle": {
                "component_id": "runtime_bundle",
                "release_id": runtime_id,
                "source_commit_sha": source_sha,
                "artifact_path": "components/runtime-bundle.tar",
                "artifact_sha256": "7" * 64,
                "artifact_size_bytes": 11,
                "component_manifest_sha256": "8" * 64,
            },
        },
        "protocols": {
            "profile_bridge_protocol_version": 1,
            "camouhost_ipc_version": 1,
        },
        "artifact_inventory": [
            {
                "path": WINDOWS_SBOM_PATH,
                "sha256": sha256_bytes(sbom_bytes),
                "size_bytes": len(sbom_bytes),
                "kind": "windows-delivery-sbom",
            },
            {
                "path": WINDOWS_PROVENANCE_PATH,
                "sha256": sha256_bytes(provenance_bytes),
                "size_bytes": len(provenance_bytes),
                "kind": "windows-delivery-provenance",
            },
        ],
    }
    with tempfile.TemporaryDirectory(prefix="windows-delivery-publication-") as directory:
        root = Path(directory)
        release_path = root / "release-set.json"
        sbom_path = root / "windows-sbom-v1.json"
        provenance_path = root / "windows-provenance-v1.json"
        release_path.write_text(json.dumps(release_set), encoding="utf-8")
        sbom_path.write_bytes(sbom_bytes)
        provenance_path.write_bytes(provenance_bytes)
        first = render_manifest(
            release_set_path=release_path,
            sbom_path=sbom_path,
            provenance_path=provenance_path,
            sequence=42,
        )
        second = render_manifest(
            release_set_path=release_path,
            sbom_path=sbom_path,
            provenance_path=provenance_path,
            sequence=42,
        )
        if first != second:
            fail("delivery manifest projection is not deterministic")
        rendered = json.loads(first)
        if (
            rendered["release_set_id"] != release_set_id
            or rendered["components"]["profile_bridge"]["release_id"] != bridge_id
            or rendered["components"]["runtime_bundle"]["release_id"] != runtime_id
            or rendered["sequence"] != 42
        ):
            fail("delivery manifest projection self-test failed")

        tampered = sbom_path.read_bytes() + b" "
        sbom_path.write_bytes(tampered)
        expect_failure(
            lambda: render_manifest(
                release_set_path=release_path,
                sbom_path=sbom_path,
                provenance_path=provenance_path,
                sequence=42,
            ),
            "evidence identity mismatch",
        )

    print("Windows delivery publication self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    commands = result.add_subparsers(dest="command", required=True)
    manifest = commands.add_parser("manifest")
    manifest.add_argument("--release-set", type=Path, required=True)
    manifest.add_argument("--sbom", type=Path, required=True)
    manifest.add_argument("--provenance", type=Path, required=True)
    manifest.add_argument("--sequence", type=int, required=True)
    manifest.add_argument("--output", type=Path, required=True)
    commands.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "manifest":
            write_new(
                args.output,
                render_manifest(
                    release_set_path=args.release_set,
                    sbom_path=args.sbom,
                    provenance_path=args.provenance,
                    sequence=args.sequence,
                ),
            )
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except WindowsDeliveryPublicationError as error:
        print(f"Windows delivery publication error: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
