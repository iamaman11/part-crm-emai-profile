#!/usr/bin/env python3
"""Build and validate the canonical AR-11 immutable Release Set manifest.

This is a deterministic generator only. It has no provider credentials, network
access, deployment authority, mutable release registry, or production mutation.
The resulting directory is verified by native `opsctl release verify` before any
publication or promotion workflow may consume it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
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
RUNTIME_FILES = (
    Path("runtime/camouhost/main.py"),
    Path("runtime/camouhost/real.py"),
    Path("runtime/camouhost/runtime-lock.json"),
)


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


def deterministic_runtime_archive(source_sha: str, destination: Path) -> tuple[str, str]:
    if destination.exists():
        fail(f"runtime component destination already exists: {destination}")
    manifest = {
        "schema_version": 1,
        "kind": "CAMOUFOX_RUNTIME_COMPONENT",
        "source_commit_sha": source_sha,
        "files": source_file_set_identity(list(RUNTIME_FILES)),
    }
    manifest["release_id"] = "runtime-bundle-v1-sha256-" + sha256_bytes(canonical(manifest))
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(destination, "w", format=tarfile.PAX_FORMAT) as archive:
        members: list[tuple[str, bytes]] = [("runtime-manifest.json", document(manifest))]
        members.extend((relative.as_posix(), (ROOT / relative).read_bytes()) for relative in RUNTIME_FILES)
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
            source_sha, destinations["runtime"]
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
        runtime_lock = json.loads((ROOT / "runtime/camouhost/runtime-lock.json").read_text(encoding="utf-8"))
        authority = json.loads((ROOT / "architecture/release-architecture-ar11.json").read_text(encoding="utf-8"))
        profiles = sorted(
            item["profile_id"]
            for item in authority.get("release_profiles", [])
            if isinstance(item, dict) and isinstance(item.get("profile_id"), str)
        )
        if not profiles:
            fail("release architecture has no capability profiles")

        manifest: dict[str, Any] = {
            "schema_version": SCHEMA_VERSION,
            "source": {
                "repository": REPOSITORY,
                "commit_sha": source_sha,
                "accepted_main": True,
                "accepted_main_evidence_sha256": accepted_main_evidence(source_sha),
            },
            "components": component_rows,
            "contracts": source_file_set_identity(
                [Path("openapi/v1/control-plane.yaml"), Path("contracts/generated/control-plane.openapi.json")]
            ),
            "protocols": {
                "control_plane_contract_sha256": sha256_file(ROOT / "crates/control-plane-contract/src/lib.rs"),
                "camouhost_ipc_version": runtime_lock["camouhost_ipc_version"],
                "resolver_protocol": "mailbox-secret-resolver-v1",
            },
            "schemas": {
                "d1_evolution_authority_sha256": sha256_file(ROOT / "architecture/d1-evolution-ar9.json"),
            },
            "runtime_compatibility": {
                "runtime_lock_sha256": sha256_file(ROOT / "runtime/camouhost/runtime-lock.json"),
                "runtime_role": runtime_lock["runtime_role"],
                "profile_format": runtime_lock["fingerprint_config_schema"],
                "browser_identity_policy": runtime_lock["fingerprint_policy_version"],
            },
            "capability_profile_compatibility": profiles,
            "build_provenance": {
                "cargo_lock_sha256": sha256_file(ROOT / "Cargo.lock"),
                "rust_toolchain_sha256": sha256_file(ROOT / "rust-toolchain.toml"),
                "frontend_lock_sha256": sha256_file(ROOT / "frontend/package-lock.json"),
                "release_architecture_sha256": sha256_file(ROOT / "architecture/release-architecture-ar11.json"),
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
