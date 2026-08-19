#!/usr/bin/env python3
"""Build and validate the canonical AR-11 immutable Release Set manifest.

This is a deterministic packaging generator only. It has no provider credentials,
network access, deployment authority, mutable release registry, production mutation,
or independent knowledge of release-critical repository paths. Those paths are owned
by the canonical AR-11 release input topology and verified by native `opsctl`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "iamaman11/part-crm-emai-profile"
SCHEMA_VERSION = 1
PREFIX = "release-set-v1-sha256-"
COMPONENT_DIR = "components"
AUTHORITY_PATH = Path("architecture/release-architecture-ar11.json")


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


def load_release_authority() -> dict[str, Any]:
    path = ROOT / AUTHORITY_PATH
    regular(path, "release architecture authority")
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        fail("release architecture authority must be an object")
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
        sources = [value for value in (canonical_source, generated_projection) if value is not None]
        if len(sources) != 1 or sources[0] != identity:
            fail(
                f"release input {input_id} must bind exactly one canonical/generated source "
                "to release_identity_source"
            )
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


def release_input_paths_for_consumer(
    inputs: dict[str, dict[str, Any]], consumer: str
) -> list[Path]:
    paths: list[Path] = []
    for input_id, row in inputs.items():
        consumers = row.get("consumers")
        if not isinstance(consumers, list) or not all(isinstance(value, str) for value in consumers):
            fail(f"release input {input_id} has invalid consumers")
        if consumer in consumers:
            paths.append(release_input_path(inputs, input_id))
    paths.sort(key=lambda value: value.as_posix())
    if not paths:
        fail(f"canonical release input topology has no inputs for consumer {consumer}")
    return paths


def git_sha(value: str) -> str:
    if len(value) != 40 or any(character not in "0123456789abcdef" for character in value):
        fail("source SHA must be exact 40 lowercase hexadecimal")
    return value


def accepted_main_evidence(source_sha: str) -> str:
    return sha256_bytes(
        canonical(
            {
                "authority": "accepted-main",
                "commit_sha": source_sha,
                "repository": REPOSITORY,
            }
        )
    )


def file_identity(path: Path) -> dict[str, Any]:
    regular(path, "component artifact")
    return {"sha256": sha256_file(path), "size_bytes": path.stat().st_size}


def source_file_set_identity(paths: list[Path]) -> dict[str, Any]:
    entries = []
    for relative in sorted(paths, key=lambda value: value.as_posix()):
        path = ROOT / relative
        regular(path, "release identity source")
        entries.append(
            {
                "path": relative.as_posix(),
                "sha256": sha256_file(path),
                "size_bytes": path.stat().st_size,
            }
        )
    return {"files": entries, "sha256": sha256_bytes(canonical(entries))}


def deterministic_runtime_archive(
    source_sha: str, destination: Path, runtime_files: list[Path]
) -> tuple[str, str]:
    if destination.exists():
        fail(f"runtime component destination already exists: {destination}")
    manifest = {
        "schema_version": 1,
        "kind": "CAMOUFOX_RUNTIME_COMPONENT",
        "source_commit_sha": source_sha,
        "files": source_file_set_identity(runtime_files),
    }
    manifest["release_id"] = "runtime-bundle-v1-sha256-" + sha256_bytes(canonical(manifest))
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(destination, "w", format=tarfile.PAX_FORMAT) as archive:
        members: list[tuple[str, bytes]] = [("runtime-manifest.json", document(manifest))]
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
            archive.addfile(info, fileobj=__import__("io").BytesIO(data))
    return str(manifest["release_id"]), sha256_bytes(document(manifest))


def load_component_manifest(path: Path, *, source_sha: str, release_key: str = "release_id") -> tuple[str, str]:
    regular(path, "component manifest")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise ReleaseSetError(f"component manifest is invalid JSON: {path}: {error}") from error
    if not isinstance(value, dict):
        fail(f"component manifest must be an object: {path}")
    release_id = value.get(release_key)
    if not isinstance(release_id, str) or not release_id:
        fail(f"component manifest lacks {release_key}: {path}")
    source = value.get("source")
    observed_sha = source.get("commit_sha") if isinstance(source, dict) else value.get("source_commit_sha")
    if observed_sha != source_sha:
        fail(f"component manifest source SHA differs from Release Set source: {path}")
    return release_id, sha256_file(path)


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
    for path, label in (
        (control_archive, "control-plane archive"),
        (resolver_archive, "resolver archive"),
        (profile_bridge_archive, "Profile Bridge archive"),
    ):
        regular(path, label)

    authority = load_release_authority()
    inputs = release_input_map(authority)
    contract_paths = release_input_paths_for_consumer(inputs, "release_set.contracts")
    runtime_files = release_input_paths_for_consumer(inputs, "runtime_bundle.files")
    runtime_lock_relative = release_input_path(inputs, "camouhost_runtime_lock")
    d1_authority_relative = release_input_path(inputs, "d1_evolution_authority")
    cargo_lock_relative = release_input_path(inputs, "cargo_lock")
    rust_toolchain_relative = release_input_path(inputs, "rust_toolchain")
    frontend_lock_relative = release_input_path(inputs, "frontend_lock")
    release_architecture_relative = release_input_path(inputs, "release_architecture_authority")

    contracts_identity = source_file_set_identity(contract_paths)
    runtime_lock = json.loads((ROOT / runtime_lock_relative).read_text(encoding="utf-8"))
    if not isinstance(runtime_lock, dict):
        fail("runtime lock must be an object")

    profiles = sorted(
        item["profile_id"]
        for item in authority.get("release_profiles", [])
        if isinstance(item, dict) and isinstance(item.get("profile_id"), str)
    )
    if not profiles:
        fail("release architecture has no capability profiles")

    control_release_id, control_manifest_sha = load_component_manifest(
        control_manifest, source_sha=source_sha
    )
    resolver_release_id, resolver_manifest_sha = load_component_manifest(
        resolver_manifest, source_sha=source_sha
    )
    bridge_release_id, bridge_manifest_sha = load_component_manifest(
        profile_bridge_manifest, source_sha=source_sha
    )

    staging = Path(tempfile.mkdtemp(prefix="ar11-release-set-"))
    try:
        component_root = staging / COMPONENT_DIR
        component_root.mkdir(parents=True)
        destinations = {
            "control": component_root / "control-plane.tar",
            "resolver": component_root / "secret-resolver.tar",
            "runtime": component_root / "runtime-bundle.tar",
            "bridge": component_root / "profile-bridge.zip",
        }
        shutil.copyfile(control_archive, destinations["control"], follow_symlinks=False)
        shutil.copyfile(resolver_archive, destinations["resolver"], follow_symlinks=False)
        shutil.copyfile(profile_bridge_archive, destinations["bridge"], follow_symlinks=False)
        runtime_release_id, runtime_manifest_sha = deterministic_runtime_archive(
            source_sha, destinations["runtime"], runtime_files
        )

        identities = {name: file_identity(path) for name, path in destinations.items()}
        component_rows = {
            "control_plane": component_row(
                control_release_id,
                source_sha,
                "components/control-plane.tar",
                identities["control"],
                control_manifest_sha,
            ),
            "frontend": component_row(
                f"{control_release_id}:frontend",
                source_sha,
                "components/control-plane.tar",
                identities["control"],
                control_manifest_sha,
            ),
            "secret_resolver": component_row(
                resolver_release_id,
                source_sha,
                "components/secret-resolver.tar",
                identities["resolver"],
                resolver_manifest_sha,
            ),
            "runtime_bundle": component_row(
                runtime_release_id,
                source_sha,
                "components/runtime-bundle.tar",
                identities["runtime"],
                runtime_manifest_sha,
            ),
            "profile_bridge": component_row(
                bridge_release_id,
                source_sha,
                "components/profile-bridge.zip",
                identities["bridge"],
                bridge_manifest_sha,
            ),
        }

        manifest: dict[str, Any] = {
            "schema_version": SCHEMA_VERSION,
            "source": {
                "repository": REPOSITORY,
                "commit_sha": source_sha,
                "accepted_main": True,
                "accepted_main_evidence_sha256": accepted_main_evidence(source_sha),
            },
            "components": component_rows,
            "contracts": contracts_identity,
            "protocols": {
                "public_api_contract_sha256": contracts_identity["sha256"],
                "camouhost_ipc_version": runtime_lock["camouhost_ipc_version"],
                "resolver_protocol": "mailbox-secret-resolver-v1",
            },
            "schemas": {
                "d1_evolution_authority_sha256": sha256_file(ROOT / d1_authority_relative),
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


def component_row(
    release_id: str,
    source_sha: str,
    artifact_path: str,
    identity: dict[str, Any],
    manifest_sha: str,
) -> dict[str, Any]:
    return {
        "release_id": release_id,
        "source_commit_sha": source_sha,
        "artifact_path": artifact_path,
        "artifact_sha256": identity["sha256"],
        "artifact_size_bytes": identity["size_bytes"],
        "component_manifest_sha256": manifest_sha,
    }


def inventory(path: str, identity: dict[str, Any]) -> dict[str, Any]:
    return {
        "path": path,
        "sha256": identity["sha256"],
        "size_bytes": identity["size_bytes"],
        "kind": "component",
    }


def profile_bridge_manifest(source_sha: str, archive: Path, output: Path) -> None:
    source_sha = git_sha(source_sha)
    identity = file_identity(archive)
    manifest: dict[str, Any] = {
        "schema_version": 1,
        "kind": "PROFILE_BRIDGE_COMPONENT",
        "source_commit_sha": source_sha,
        "artifact_sha256": identity["sha256"],
        "artifact_size_bytes": identity["size_bytes"],
    }
    manifest["release_id"] = "profile-bridge-v1-sha256-" + sha256_bytes(canonical(manifest))
    if output.exists():
        fail(f"Profile Bridge manifest already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(document(manifest))


def self_test() -> None:
    source = "a" * 40
    first = accepted_main_evidence(source)
    second = accepted_main_evidence(source)
    if first != second or len(first) != 64:
        fail("accepted-main evidence identity is not deterministic")
    if accepted_main_evidence("b" * 40) == first:
        fail("accepted-main evidence does not bind source SHA")

    authority = load_release_authority()
    inputs = release_input_map(authority)
    contracts = release_input_paths_for_consumer(inputs, "release_set.contracts")
    if Path("openapi/v1/openapi.json") not in contracts:
        fail("canonical public API root is absent from release_set.contracts")
    if any(path.as_posix().endswith("control-plane.yaml") for path in contracts):
        fail("retired/nonexistent control-plane.yaml leaked into release identity")
    if release_input_path(inputs, "camouhost_runtime_lock") not in release_input_paths_for_consumer(
        inputs, "runtime_bundle.files"
    ):
        fail("runtime lock is not part of runtime bundle identity")
    print("AR-11 Release Set generator self-test passed.")


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
    bridge = sub.add_parser("profile-bridge-manifest")
    bridge.add_argument("--source-sha", required=True)
    bridge.add_argument("--archive", type=Path, required=True)
    bridge.add_argument("--output", type=Path, required=True)
    sub.add_parser("self-test")
    return root


def main() -> int:
    try:
        args = parser().parse_args()
        if args.command == "self-test":
            self_test()
        elif args.command == "profile-bridge-manifest":
            profile_bridge_manifest(args.source_sha, args.archive, args.output)
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
        print(f"AR-11 Release Set error: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
