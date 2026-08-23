#!/usr/bin/env python3
"""Build the deterministic Windows Profile Bridge component package.

This adapter owns Profile Bridge component packaging only. Release Set versioning,
aggregate semantics, content addressing, provider access, publication, and promotion
belong elsewhere.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
import zipfile
from pathlib import Path
from typing import Any, Callable

RELEASE_PREFIX = "profile-bridge-v2-sha256-"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")


class ProfileBridgePackageError(ValueError):
    """Fail-closed deterministic Profile Bridge packaging error."""


def fail(message: str) -> None:
    raise ProfileBridgePackageError(message)


def canonical(value: Any) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ).encode("utf-8")


def document(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"Profile Bridge executable must be a regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_sha(value: str) -> str:
    if COMMIT_RE.fullmatch(value) is None:
        fail("source SHA must be exact 40 lowercase hexadecimal")
    return value


def component_manifest(
    *,
    commit_sha: str,
    executable: Path,
) -> tuple[dict[str, Any], bytes]:
    if executable.is_symlink() or not executable.is_file():
        fail(f"Profile Bridge executable must be a regular file: {executable}")
    executable_identity = {
        "sha256": sha256_file(executable),
        "size_bytes": executable.stat().st_size,
    }
    payload: dict[str, Any] = {
        "schema_version": 2,
        "kind": "PROFILE_BRIDGE_COMPONENT",
        "source_commit_sha": source_sha(commit_sha),
        "protocol_version": 1,
        "executable": {
            "path": "profile-bridge.exe",
            "sha256": executable_identity["sha256"],
            "size_bytes": executable_identity["size_bytes"],
        },
    }
    payload["release_id"] = RELEASE_PREFIX + sha256_bytes(canonical(payload))
    return payload, document(payload)


def package_profile_bridge(
    *,
    commit_sha: str,
    executable: Path,
    archive_path: Path,
    manifest_path: Path,
) -> None:
    _, manifest_bytes = component_manifest(
        commit_sha=commit_sha,
        executable=executable,
    )
    if archive_path.exists() or manifest_path.exists():
        fail("Profile Bridge package output already exists")

    archive_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_bytes(manifest_bytes)

    with zipfile.ZipFile(archive_path, "w", allowZip64=False) as package:
        for name, data, mode in (
            ("profile-bridge-manifest.json", manifest_bytes, 0o100644),
            ("profile-bridge.exe", executable.read_bytes(), 0o100755),
        ):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED
            info.create_system = 3
            info.external_attr = mode << 16
            package.writestr(info, data)


def expect_failure(action: Callable[[], None], marker: str) -> None:
    try:
        action()
    except ProfileBridgePackageError as error:
        if marker not in str(error):
            fail(
                f"negative self-test failed with unexpected error: {error}; "
                f"expected marker {marker!r}"
            )
    else:
        fail(f"negative self-test unexpectedly passed: {marker}")


def self_test() -> None:
    commit_sha = "1" * 40
    executable_bytes = b"profile-bridge-self-test\n"
    with tempfile.TemporaryDirectory(prefix="profile-bridge-package-") as directory:
        temp = Path(directory)
        executable = temp / "profile-bridge.exe"
        executable.write_bytes(executable_bytes)

        first_archive = temp / "first" / "profile-bridge.zip"
        first_manifest = temp / "first" / "profile-bridge-manifest.json"
        second_archive = temp / "second" / "profile-bridge.zip"
        second_manifest = temp / "second" / "profile-bridge-manifest.json"

        package_profile_bridge(
            commit_sha=commit_sha,
            executable=executable,
            archive_path=first_archive,
            manifest_path=first_manifest,
        )
        package_profile_bridge(
            commit_sha=commit_sha,
            executable=executable,
            archive_path=second_archive,
            manifest_path=second_manifest,
        )

        if first_archive.read_bytes() != second_archive.read_bytes():
            fail("Profile Bridge archive packaging is not deterministic")
        if first_manifest.read_bytes() != second_manifest.read_bytes():
            fail("Profile Bridge manifest packaging is not deterministic")

        manifest = json.loads(first_manifest.read_text(encoding="utf-8"))
        if set(manifest) != {
            "schema_version",
            "kind",
            "source_commit_sha",
            "protocol_version",
            "executable",
            "release_id",
        }:
            fail("Profile Bridge manifest field inventory self-test failed")
        if (
            manifest["schema_version"] != 2
            or manifest["kind"] != "PROFILE_BRIDGE_COMPONENT"
            or manifest["source_commit_sha"] != commit_sha
            or manifest["protocol_version"] != 1
        ):
            fail("Profile Bridge manifest identity self-test failed")
        executable_identity = manifest.get("executable")
        if executable_identity != {
            "path": "profile-bridge.exe",
            "sha256": sha256_bytes(executable_bytes),
            "size_bytes": len(executable_bytes),
        }:
            fail("Profile Bridge executable identity self-test failed")

        identity_payload = dict(manifest)
        release_id = identity_payload.pop("release_id")
        expected_release_id = RELEASE_PREFIX + sha256_bytes(canonical(identity_payload))
        if release_id != expected_release_id:
            fail("Profile Bridge component release ID self-test failed")

        with zipfile.ZipFile(first_archive, "r") as package:
            infos = package.infolist()
            if [info.filename for info in infos] != [
                "profile-bridge-manifest.json",
                "profile-bridge.exe",
            ]:
                fail("Profile Bridge archive member inventory/order self-test failed")
            expected_modes = {
                "profile-bridge-manifest.json": 0o100644,
                "profile-bridge.exe": 0o100755,
            }
            for info in infos:
                if (
                    info.compress_type != zipfile.ZIP_STORED
                    or info.date_time != (1980, 1, 1, 0, 0, 0)
                    or info.create_system != 3
                    or (info.external_attr >> 16) != expected_modes[info.filename]
                ):
                    fail(
                        "Profile Bridge deterministic ZIP metadata self-test failed: "
                        f"{info.filename}"
                    )
            if package.read("profile-bridge-manifest.json") != first_manifest.read_bytes():
                fail("Profile Bridge embedded manifest self-test failed")
            if package.read("profile-bridge.exe") != executable_bytes:
                fail("Profile Bridge embedded executable self-test failed")

        expect_failure(
            lambda: component_manifest(commit_sha="X" * 40, executable=executable),
            "source SHA",
        )
        expect_failure(
            lambda: component_manifest(
                commit_sha=commit_sha,
                executable=temp / "missing-profile-bridge.exe",
            ),
            "regular file",
        )
        expect_failure(
            lambda: package_profile_bridge(
                commit_sha=commit_sha,
                executable=executable,
                archive_path=first_archive,
                manifest_path=temp / "third" / "profile-bridge-manifest.json",
            ),
            "output already exists",
        )

    print("Profile Bridge component packaging self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)
    package = subcommands.add_parser("package")
    package.add_argument("--source-sha", required=True)
    package.add_argument("--executable", type=Path, required=True)
    package.add_argument("--archive", type=Path, required=True)
    package.add_argument("--manifest", type=Path, required=True)
    subcommands.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "package":
            package_profile_bridge(
                commit_sha=args.source_sha,
                executable=args.executable,
                archive_path=args.archive,
                manifest_path=args.manifest,
            )
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except ProfileBridgePackageError as error:
        print(
            f"Profile Bridge package error: {error}",
            file=__import__("sys").stderr,
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
