#!/usr/bin/env python3
"""Build the deterministic Camouhost runtime component package.

This script owns component packaging only. Release Set versioning, aggregate semantics,
content addressing, provider access, publication, and promotion belong elsewhere.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import re
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
RELEASE_ARCHITECTURE = ROOT / "architecture" / "release-architecture-ar11.json"
RUNTIME_CONSUMER = "runtime_bundle.files"
RELEASE_PREFIX = "runtime-bundle-v1-sha256-"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")


class RuntimePackageError(ValueError):
    """Fail-closed deterministic runtime packaging error."""


def fail(message: str) -> None:
    raise RuntimePackageError(message)


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
        fail(f"runtime package input must be a regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_sha(value: str) -> str:
    if COMMIT_RE.fullmatch(value) is None:
        fail("source SHA must be exact 40 lowercase hexadecimal")
    return value


def safe_repo_relative(value: str, label: str) -> Path:
    pure = PurePosixPath(value)
    if (
        not value
        or pure.is_absolute()
        or any(part in {"", ".", ".."} for part in pure.parts)
    ):
        fail(f"{label} must be a safe repository-relative path: {value!r}")
    relative = Path(*pure.parts)
    path = ROOT / relative
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must resolve to a regular repository file: {value}")
    try:
        path.resolve(strict=True).relative_to(ROOT.resolve(strict=True))
    except ValueError as error:
        raise RuntimePackageError(f"{label} escapes repository root: {value}") from error
    return relative


def runtime_files() -> list[Path]:
    try:
        authority = json.loads(RELEASE_ARCHITECTURE.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimePackageError(
            f"cannot read release architecture authority: {error}"
        ) from error
    if (
        not isinstance(authority, dict)
        or authority.get("schema_version") != 1
        or authority.get("kind") != "AR11_RELEASE_ARCHITECTURE_SOURCE"
    ):
        fail("release architecture authority identity/schema mismatch")

    rows = authority.get("release_inputs")
    if not isinstance(rows, list) or not rows:
        fail("canonical release_inputs topology is missing")

    observed_ids: set[str] = set()
    observed_identity_paths: set[str] = set()
    selected: list[Path] = []
    for row in rows:
        if not isinstance(row, dict):
            fail("release_inputs entries must be objects")
        input_id = row.get("input_id")
        identity = row.get("release_identity_source")
        canonical_source = row.get("canonical_source")
        generated_projection = row.get("generated_projection")
        consumers = row.get("consumers")
        if not isinstance(input_id, str) or not input_id:
            fail("release input has invalid input_id")
        if input_id in observed_ids:
            fail(f"duplicate release input id: {input_id}")
        observed_ids.add(input_id)
        if not isinstance(identity, str) or not identity:
            fail(f"release input {input_id} has invalid release_identity_source")
        if identity in observed_identity_paths:
            fail(f"duplicate release identity path: {identity}")
        observed_identity_paths.add(identity)
        sources = [
            item
            for item in (canonical_source, generated_projection)
            if item is not None
        ]
        if len(sources) != 1 or sources[0] != identity:
            fail(
                f"release input {input_id} must bind exactly one source "
                "to release_identity_source"
            )
        if not isinstance(consumers, list) or not all(
            isinstance(item, str) for item in consumers
        ):
            fail(f"release input {input_id} has invalid consumers")
        relative = safe_repo_relative(identity, f"release input {input_id}")
        if RUNTIME_CONSUMER in consumers:
            selected.append(relative)

    selected.sort(key=lambda item: item.as_posix())
    if not selected:
        fail(f"canonical release input topology has no inputs for {RUNTIME_CONSUMER}")
    return selected


def source_file_set_identity(paths: list[Path]) -> dict[str, Any]:
    entries: list[dict[str, Any]] = []
    for relative in paths:
        path = ROOT / relative
        entries.append(
            {
                "path": relative.as_posix(),
                "sha256": sha256_file(path),
                "size_bytes": path.stat().st_size,
            }
        )
    return {
        "files": entries,
        "sha256": sha256_bytes(canonical(entries)),
    }


def runtime_manifest(commit_sha: str, files: list[Path]) -> tuple[dict[str, Any], bytes]:
    manifest: dict[str, Any] = {
        "schema_version": 1,
        "kind": "CAMOUFOX_RUNTIME_COMPONENT",
        "source_commit_sha": source_sha(commit_sha),
        "files": source_file_set_identity(files),
    }
    manifest["release_id"] = RELEASE_PREFIX + sha256_bytes(canonical(manifest))
    return manifest, document(manifest)


def deterministic_archive(
    archive_path: Path,
    manifest_bytes: bytes,
    files: list[Path],
) -> None:
    if archive_path.exists():
        fail(f"runtime package archive already exists: {archive_path}")
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive_path, "w", format=tarfile.PAX_FORMAT) as archive:
        members: list[tuple[str, bytes]] = [("runtime-manifest.json", manifest_bytes)]
        members.extend(
            (relative.as_posix(), (ROOT / relative).read_bytes())
            for relative in files
        )
        for name, data in sorted(members):
            pure = PurePosixPath(name)
            if pure.is_absolute() or ".." in pure.parts or not pure.parts:
                fail(f"unsafe runtime archive path: {name}")
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o644
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mtime = 0
            archive.addfile(info, fileobj=io.BytesIO(data))


def package_runtime(
    *,
    commit_sha: str,
    archive_path: Path,
    manifest_path: Path,
) -> None:
    if manifest_path.exists():
        fail(f"runtime package manifest already exists: {manifest_path}")
    files = runtime_files()
    _, manifest_bytes = runtime_manifest(commit_sha, files)
    deterministic_archive(archive_path, manifest_bytes, files)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_bytes(manifest_bytes)


def self_test() -> None:
    files = runtime_files()
    commit_sha = "1" * 40
    with tempfile.TemporaryDirectory(prefix="camouhost-runtime-package-") as directory:
        temp = Path(directory)
        first_archive = temp / "first.tar"
        first_manifest = temp / "first.json"
        second_archive = temp / "second.tar"
        second_manifest = temp / "second.json"
        package_runtime(
            commit_sha=commit_sha,
            archive_path=first_archive,
            manifest_path=first_manifest,
        )
        package_runtime(
            commit_sha=commit_sha,
            archive_path=second_archive,
            manifest_path=second_manifest,
        )
        if first_archive.read_bytes() != second_archive.read_bytes():
            fail("runtime archive packaging is not deterministic")
        if first_manifest.read_bytes() != second_manifest.read_bytes():
            fail("runtime manifest packaging is not deterministic")

        manifest = json.loads(first_manifest.read_text(encoding="utf-8"))
        if manifest.get("source_commit_sha") != commit_sha:
            fail("runtime manifest source binding self-test failed")
        release_id = manifest.get("release_id")
        if not isinstance(release_id, str) or not release_id.startswith(RELEASE_PREFIX):
            fail("runtime release ID self-test failed")

        expected_names = sorted(
            ["runtime-manifest.json", *(path.as_posix() for path in files)]
        )
        with tarfile.open(first_archive, "r:") as archive:
            members = archive.getmembers()
            if [member.name for member in members] != expected_names:
                fail("runtime archive member inventory/order self-test failed")
            for member in members:
                if (
                    not member.isfile()
                    or member.uid != 0
                    or member.gid != 0
                    or member.mtime != 0
                    or member.mode != 0o644
                    or member.uname != ""
                    or member.gname != ""
                ):
                    fail(
                        f"runtime archive deterministic metadata self-test failed: "
                        f"{member.name}"
                    )

        try:
            source_sha("X" * 40)
        except RuntimePackageError:
            pass
        else:
            fail("invalid source SHA negative self-test unexpectedly passed")

    print("Camouhost runtime component packaging self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)
    package = subcommands.add_parser("package")
    package.add_argument("--source-sha", required=True)
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
            package_runtime(
                commit_sha=args.source_sha,
                archive_path=args.archive,
                manifest_path=args.manifest,
            )
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except RuntimePackageError as error:
        print(f"runtime package error: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
