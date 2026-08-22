#!/usr/bin/env python3
"""Build and validate the canonical AR-11 immutable Release Set v2.

Release Set v2 is the only executable pre-production contract. Component manifests are
embedded in their immutable component archives; no legacy Release Set or manifest sidecar
compatibility path exists. This script only packages deterministic bytes and has no provider,
network, deployment, secret, or production mutation authority.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import shutil
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

import d1_repository_projection as d1_repository

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "iamaman11/part-crm-emai-profile"
SCHEMA_VERSION = 2
PREFIX = "release-set-v2-sha256-"
COMPONENT_DIR = "components"
RELEASE_ARCHITECTURE_PATH = Path("architecture/release-architecture-ar11.json")
RELEASE_SET_CONTRACT_PATH = Path("architecture/release-set-v2.json")


class ReleaseSetError(ValueError):
    pass


def fail(message: str) -> None:
    raise ReleaseSetError(message)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def document(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def regular(path: Path, label: str) -> None:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")


def safe_repo_relative(value: str, label: str) -> Path:
    pure = PurePosixPath(value)
    if not value or pure.is_absolute() or any(part in ("", ".", "..") for part in pure.parts):
        fail(f"{label} must be a safe repository-relative path: {value!r}")
    relative = Path(*pure.parts)
    path = ROOT / relative
    regular(path, label)
    try:
        path.resolve(strict=True).relative_to(ROOT.resolve(strict=True))
    except ValueError as error:
        raise ReleaseSetError(f"{label} escapes repository root: {value}") from error
    return relative


def load_json(path: Path, label: str) -> dict[str, Any]:
    regular(path, label)
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise ReleaseSetError(f"{label} is invalid JSON: {error}") from error
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    return value


def load_release_set_contract() -> dict[str, Any]:
    value = load_json(ROOT / RELEASE_SET_CONTRACT_PATH, "Release Set v2 contract")
    if (
        value.get("schema_version") != 1
        or value.get("kind") != "AR11_RELEASE_SET_CONTRACT"
        or value.get("release_set_schema_version") != SCHEMA_VERSION
        or value.get("supported_release_set_prefixes") != [PREFIX]
        or value.get("legacy_release_set_compatibility") is not False
    ):
        fail("Release Set v2 contract identity/policy mismatch")
    manifest_policy = value.get("component_manifest_policy")
    if not isinstance(manifest_policy, dict) or manifest_policy.get("embedded_manifest_required") is not True:
        fail("Release Set v2 must require embedded component manifests")
    if manifest_policy.get("external_manifest_sidecar_required") is not False:
        fail("Release Set v2 must not require manifest sidecars")
    return value


def load_release_architecture() -> dict[str, Any]:
    value = load_json(ROOT / RELEASE_ARCHITECTURE_PATH, "release architecture authority")
    if value.get("schema_version") != 1 or value.get("kind") != "AR11_RELEASE_ARCHITECTURE_SOURCE":
        fail("release architecture authority identity/schema mismatch")
    rows = value.get("release_inputs")
    if not isinstance(rows, list) or not rows:
        fail("canonical release_inputs topology is missing")
    return value


def release_input_map(authority: dict[str, Any]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    identity_paths: set[str] = set()
    rows = authority.get("release_inputs")
    if not isinstance(rows, list):
        fail("release_inputs must be an array")
    for row in rows:
        if not isinstance(row, dict):
            fail("release_inputs entries must be objects")
        input_id = row.get("input_id")
        identity = row.get("release_identity_source")
        canonical_source = row.get("canonical_source")
        generated_projection = row.get("generated_projection")
        if not isinstance(input_id, str) or not input_id:
            fail("release input has invalid input_id")
        if input_id in result:
            fail(f"duplicate release input id: {input_id}")
        if not isinstance(identity, str) or not identity:
            fail(f"release input {input_id} has invalid release_identity_source")
        sources = [item for item in (canonical_source, generated_projection) if item is not None]
        if len(sources) != 1 or sources[0] != identity:
            fail(f"release input {input_id} must bind exactly one source to release_identity_source")
        safe_repo_relative(identity, f"release input {input_id}")
        if identity in identity_paths:
            fail(f"duplicate release identity path: {identity}")
        identity_paths.add(identity)
        result[input_id] = row
    return result


def release_input_path(inputs: dict[str, dict[str, Any]], input_id: str) -> Path:
    row = inputs.get(input_id)
    if row is None:
        fail(f"canonical release input is missing: {input_id}")
    identity = row.get("release_identity_source")
    if not isinstance(identity, str):
        fail(f"release input {input_id} has invalid release_identity_source")
    return safe_repo_relative(identity, f"release input {input_id}")


def release_input_paths_for_consumer(inputs: dict[str, dict[str, Any]], consumer: str) -> list[Path]:
    paths: list[Path] = []
    for input_id, row in inputs.items():
        consumers = row.get("consumers")
        if not isinstance(consumers, list) or not all(isinstance(item, str) for item in consumers):
            fail(f"release input {input_id} has invalid consumers")
        if consumer in consumers:
            paths.append(release_input_path(inputs, input_id))
    paths.sort(key=lambda item: item.as_posix())
    if not paths:
        fail(f"canonical release input topology has no inputs for consumer {consumer}")
    return paths


def git_sha(value: str) -> str:
    if len(value) != 40 or any(character not in "0123456789abcdef" for character in value):
        fail("source SHA must be exact 40 lowercase hexadecimal")
    return value


def accepted_main_identity(source_sha: str) -> str:
    return sha256_bytes(canonical({"authority": "accepted-main", "commit_sha": source_sha, "repository": REPOSITORY}))


def file_identity(path: Path) -> dict[str, Any]:
    regular(path, "release artifact")
    return {"sha256": sha256_file(path), "size_bytes": path.stat().st_size}


def source_file_set_identity(paths: list[Path]) -> dict[str, Any]:
    entries = []
    for relative in sorted(paths, key=lambda item: item.as_posix()):
        path = ROOT / relative
        regular(path, "release identity source")
        entries.append({"path": relative.as_posix(), "sha256": sha256_file(path), "size_bytes": path.stat().st_size})
    return {"files": entries, "sha256": sha256_bytes(canonical(entries))}


def load_component_manifest(path: Path, *, source_sha: str) -> tuple[str, dict[str, Any], bytes]:
    value = load_json(path, "component manifest")
    release_id = value.get("release_id")
    if not isinstance(release_id, str) or not release_id:
        fail(f"component manifest lacks release_id: {path}")
    source = value.get("source")
    observed_sha = source.get("commit_sha") if isinstance(source, dict) else value.get("source_commit_sha")
    if observed_sha != source_sha:
        fail(f"component manifest source SHA differs from Release Set source: {path}")
    return release_id, value, path.read_bytes()


def schema_contract(manifest: dict[str, Any], component: str) -> dict[str, Any]:
    value = manifest.get("schema_contract")
    if not isinstance(value, dict) or value.get("database_component") != component:
        fail(f"{component} component manifest lacks canonical schema_contract")
    required = {
        "database_component",
        "target_schema_revision",
        "supported_schema_min",
        "supported_schema_max",
        "migration_history_digest",
        "compatibility_policy_digest",
    }
    if set(value) != required:
        fail(f"{component} schema_contract field inventory drifted")
    return value


def deterministic_runtime_archive(source_sha: str, destination: Path, runtime_files: list[Path]) -> tuple[str, bytes]:
    manifest: dict[str, Any] = {
        "schema_version": 1,
        "kind": "CAMOUFOX_RUNTIME_COMPONENT",
        "source_commit_sha": source_sha,
        "files": source_file_set_identity(runtime_files),
    }
    manifest["release_id"] = "runtime-bundle-v1-sha256-" + sha256_bytes(canonical(manifest))
    manifest_bytes = document(manifest)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(destination, "w", format=tarfile.PAX_FORMAT) as archive:
        members: list[tuple[str, bytes]] = [("runtime-manifest.json", manifest_bytes)]
        members.extend((relative.as_posix(), (ROOT / relative).read_bytes()) for relative in runtime_files)
        for name, data in sorted(members):
            pure = PurePosixPath(name)
            if pure.is_absolute() or ".." in pure.parts:
                fail(f"unsafe runtime archive path: {name}")
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o644
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mtime = 0
            archive.addfile(info, fileobj=io.BytesIO(data))
    return str(manifest["release_id"]), manifest_bytes


def deterministic_profile_bridge_package(source_sha: str, executable: Path, archive: Path, manifest_output: Path) -> None:
    regular(executable, "Profile Bridge executable")
    executable_identity = file_identity(executable)
    payload: dict[str, Any] = {
        "schema_version": 2,
        "kind": "PROFILE_BRIDGE_COMPONENT",
        "source_commit_sha": source_sha,
        "protocol_version": 1,
        "executable": {
            "path": "profile-bridge.exe",
            "sha256": executable_identity["sha256"],
            "size_bytes": executable_identity["size_bytes"],
        },
    }
    payload["release_id"] = "profile-bridge-v2-sha256-" + sha256_bytes(canonical(payload))
    manifest_bytes = document(payload)
    if archive.exists() or manifest_output.exists():
        fail("Profile Bridge package output already exists")
    archive.parent.mkdir(parents=True, exist_ok=True)
    manifest_output.parent.mkdir(parents=True, exist_ok=True)
    manifest_output.write_bytes(manifest_bytes)
    with zipfile.ZipFile(archive, "w", allowZip64=False) as package:
        for name, data, mode in (
            ("profile-bridge-manifest.json", manifest_bytes, 0o100644),
            ("profile-bridge.exe", executable.read_bytes(), 0o100755),
        ):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED
            info.create_system = 3
            info.external_attr = mode << 16
            package.writestr(info, data)


def component_row(release_id: str, source_sha: str, artifact_path: str, artifact: dict[str, Any], manifest_bytes: bytes) -> dict[str, Any]:
    return {
        "release_id": release_id,
        "source_commit_sha": source_sha,
        "artifact_path": artifact_path,
        "artifact_sha256": artifact["sha256"],
        "artifact_size_bytes": artifact["size_bytes"],
        "component_manifest_sha256": sha256_bytes(manifest_bytes),
    }


def inventory(path: str, identity: dict[str, Any]) -> dict[str, Any]:
    return {"path": path, "sha256": identity["sha256"], "size_bytes": identity["size_bytes"], "kind": "component"}


def build(
    *,
    source_sha: str,
    control_archive: Path,
    control_manifest: Path,
    resolver_archive: Path,
    resolver_manifest: Path,
    profile_bridge_archive: Path,
    profile_bridge_manifest: Path,
    output_root: Path,
) -> Path:
    source_sha = git_sha(source_sha)
    load_release_set_contract()
    authority = load_release_architecture()
    inputs = release_input_map(authority)
    contract_paths = release_input_paths_for_consumer(inputs, "release_set.contracts")
    runtime_files = release_input_paths_for_consumer(inputs, "runtime_bundle.files")
    runtime_lock_relative = release_input_path(inputs, "camouhost_runtime_lock")
    cargo_lock_relative = release_input_path(inputs, "cargo_lock")
    rust_toolchain_relative = release_input_path(inputs, "rust_toolchain")
    frontend_lock_relative = release_input_path(inputs, "frontend_lock")
    release_architecture_relative = release_input_path(inputs, "release_architecture_authority")

    contracts_identity = source_file_set_identity(contract_paths)
    runtime_lock = load_json(ROOT / runtime_lock_relative, "runtime lock")
    profiles = sorted(
        item["profile_id"]
        for item in authority.get("release_profiles", [])
        if isinstance(item, dict) and isinstance(item.get("profile_id"), str)
    )
    if not profiles:
        fail("release architecture has no capability profiles")

    try:
        d1_projection = d1_repository.load(ROOT)
        catalog_contract = d1_repository.release_contract_from_projection(
            d1_projection, "catalog"
        )
        resolver_contract = d1_repository.release_contract_from_projection(
            d1_projection, "resolver"
        )
    except d1_repository.D1ProjectionError as error:
        raise ReleaseSetError(f"typed D1 repository projection failed: {error}") from error

    control_release_id, control_value, control_manifest_bytes = load_component_manifest(control_manifest, source_sha=source_sha)
    resolver_release_id, resolver_value, resolver_manifest_bytes = load_component_manifest(resolver_manifest, source_sha=source_sha)
    bridge_release_id, bridge_value, bridge_manifest_bytes = load_component_manifest(profile_bridge_manifest, source_sha=source_sha)
    if bridge_value.get("schema_version") != 2 or bridge_value.get("kind") != "PROFILE_BRIDGE_COMPONENT":
        fail("Profile Bridge must use the embedded-manifest v2 component format")
    if schema_contract(control_value, "catalog") != catalog_contract:
        fail("control-plane component schema contract diverges from typed D1 catalog")
    if schema_contract(resolver_value, "resolver") != resolver_contract:
        fail("resolver component schema contract diverges from typed D1 catalog")

    staging = Path(tempfile.mkdtemp(prefix="ar11-release-set-v2-"))
    try:
        component_root = staging / COMPONENT_DIR
        component_root.mkdir(parents=True)
        destinations = {
            "control": component_root / "control-plane.tar",
            "resolver": component_root / "secret-resolver.tar",
            "runtime": component_root / "runtime-bundle.tar",
            "bridge": component_root / "profile-bridge.zip",
        }
        for source, destination in (
            (control_archive, destinations["control"]),
            (resolver_archive, destinations["resolver"]),
            (profile_bridge_archive, destinations["bridge"]),
        ):
            regular(source, "component archive")
            shutil.copyfile(source, destination, follow_symlinks=False)
        runtime_release_id, runtime_manifest_bytes = deterministic_runtime_archive(source_sha, destinations["runtime"], runtime_files)
        identities = {name: file_identity(path) for name, path in destinations.items()}

        component_rows = {
            "control_plane": component_row(control_release_id, source_sha, "components/control-plane.tar", identities["control"], control_manifest_bytes),
            "frontend": component_row(control_release_id, source_sha, "components/control-plane.tar", identities["control"], control_manifest_bytes),
            "secret_resolver": component_row(resolver_release_id, source_sha, "components/secret-resolver.tar", identities["resolver"], resolver_manifest_bytes),
            "runtime_bundle": component_row(runtime_release_id, source_sha, "components/runtime-bundle.tar", identities["runtime"], runtime_manifest_bytes),
            "profile_bridge": component_row(bridge_release_id, source_sha, "components/profile-bridge.zip", identities["bridge"], bridge_manifest_bytes),
        }
        manifest: dict[str, Any] = {
            "schema_version": SCHEMA_VERSION,
            "source": {
                "repository": REPOSITORY,
                "commit_sha": source_sha,
                "accepted_main": True,
                "accepted_main_evidence_sha256": accepted_main_identity(source_sha),
            },
            "components": component_rows,
            "contracts": contracts_identity,
            "protocols": {
                "public_api_contract_sha256": contracts_identity["sha256"],
                "camouhost_ipc_version": runtime_lock["camouhost_ipc_version"],
                "profile_bridge_protocol_version": bridge_value["protocol_version"],
                "resolver_protocol": "mailbox-secret-resolver-v1",
            },
            "schemas": {
                "d1_repository_identity_sha256": d1_projection["repository_identity_sha256"],
                "catalog": catalog_contract,
                "resolver": resolver_contract,
            },
            "runtime_compatibility": {
                "runtime_lock_sha256": sha256_file(ROOT / runtime_lock_relative),
                "runtime_role": runtime_lock["runtime_role"],
                "profile_format": runtime_lock["fingerprint_config_schema"],
                "browser_identity_policy": runtime_lock["fingerprint_policy_version"],
            },
            "capability_profile_compatibility": profiles,
            "build_provenance": {
                "cargo_lock_sha256": sha256_file(ROOT / cargo_lock_relative),
                "rust_toolchain_sha256": sha256_file(ROOT / rust_toolchain_relative),
                "frontend_lock_sha256": sha256_file(ROOT / frontend_lock_relative),
                "release_architecture_sha256": sha256_file(ROOT / release_architecture_relative),
            },
            "artifact_inventory": [
                inventory("components/control-plane.tar", identities["control"]),
                inventory("components/profile-bridge.zip", identities["bridge"]),
                inventory("components/runtime-bundle.tar", identities["runtime"]),
                inventory("components/secret-resolver.tar", identities["resolver"]),
            ],
        }
        release_set_id = PREFIX + sha256_bytes(canonical(manifest))
        manifest["release_set_id"] = release_set_id
        destination = output_root / release_set_id
        if destination.exists():
            fail(f"immutable Release Set destination already exists: {destination}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(staging.as_posix(), destination.as_posix())
        (destination / "release-set.json").write_bytes(document(manifest))
        print(destination.as_posix())
        return destination
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise


def self_test() -> None:
    contract = load_release_set_contract()
    policy = contract["component_manifest_policy"]
    if policy.get("external_manifest_sidecar_required") is not False:
        fail("sidecar compatibility unexpectedly enabled")
    if PREFIX != "release-set-v2-sha256-" or SCHEMA_VERSION != 2:
        fail("pre-production Release Set contract must be v2-only")
    authority = load_release_architecture()
    inputs = release_input_map(authority)
    contracts = release_input_paths_for_consumer(inputs, "release_set.contracts")
    if Path("openapi/v1/openapi.json") not in contracts:
        fail("canonical public API root is absent from release_set.contracts")
    print("AR-11 Release Set v2 generator self-test passed.")


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    sub = root.add_subparsers(dest="command", required=True)
    build_parser = sub.add_parser("build")
    build_parser.add_argument("--source-sha", required=True)
    build_parser.add_argument("--control-archive", type=Path, required=True)
    build_parser.add_argument("--control-manifest", type=Path, required=True)
    build_parser.add_argument("--resolver-archive", type=Path, required=True)
    build_parser.add_argument("--resolver-manifest", type=Path, required=True)
    build_parser.add_argument("--profile-bridge-archive", type=Path, required=True)
    build_parser.add_argument("--profile-bridge-manifest", type=Path, required=True)
    build_parser.add_argument("--output-root", type=Path, required=True)
    bridge = sub.add_parser("profile-bridge-package")
    bridge.add_argument("--source-sha", required=True)
    bridge.add_argument("--executable", type=Path, required=True)
    bridge.add_argument("--archive", type=Path, required=True)
    bridge.add_argument("--manifest", type=Path, required=True)
    sub.add_parser("self-test")
    return root


def main() -> int:
    try:
        args = parser().parse_args()
        if args.command == "self-test":
            self_test()
        elif args.command == "profile-bridge-package":
            deterministic_profile_bridge_package(git_sha(args.source_sha), args.executable, args.archive, args.manifest)
        elif args.command == "build":
            build(
                source_sha=args.source_sha,
                control_archive=args.control_archive,
                control_manifest=args.control_manifest,
                resolver_archive=args.resolver_archive,
                resolver_manifest=args.resolver_manifest,
                profile_bridge_archive=args.profile_bridge_archive,
                profile_bridge_manifest=args.profile_bridge_manifest,
                output_root=args.output_root,
            )
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except (OSError, KeyError, json.JSONDecodeError, ReleaseSetError) as error:
        print(f"AR-11 Release Set v2 error: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
