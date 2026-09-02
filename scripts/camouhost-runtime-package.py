#!/usr/bin/env python3
"""Build the deterministic clean-host Windows Camouhost runtime component package.

This script is the single runtime component archive/manifest owner. Build-time
materialization belongs to `camouhost-runtime-build-windows.py`; Release Set
versioning, aggregate semantics, publication and promotion remain separate.
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
from typing import Any, BinaryIO

ROOT = Path(__file__).resolve().parents[1]
RELEASE_ARCHITECTURE = ROOT / "architecture" / "release-architecture-ar11.json"
RUNTIME_CONSUMER = "runtime_bundle.files"
RELEASE_PREFIX = "runtime-bundle-v2-sha256-"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
MAX_RUNTIME_FILES = 500_000
MAX_RUNTIME_BYTES = 2 * 1024 * 1024 * 1024
REQUIRED_RUNTIME_PATHS = {
    "browser/camoufox.exe",
    "camouhost/real.py",
    "camouhost/resolved-runtime.json",
    "camouhost/runtime-lock.json",
    "python/python.exe",
}


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


def runtime_source_files() -> list[Path]:
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


def safe_runtime_relative(root: Path, path: Path) -> str:
    try:
        relative = path.relative_to(root).as_posix()
    except ValueError as error:
        raise RuntimePackageError("runtime file escapes runtime root") from error
    pure = PurePosixPath(relative)
    if pure.is_absolute() or any(part in {"", ".", ".."} for part in pure.parts):
        fail(f"unsafe runtime package path: {relative}")
    return relative


def register_casefold_unique_path(relative: str, observed: set[str]) -> None:
    alias = relative.casefold()
    if alias in observed:
        fail(f"materialized runtime contains case-alias path: {relative}")
    observed.add(alias)


def runtime_tree_files(runtime_root: Path) -> list[tuple[str, Path]]:
    if runtime_root.is_symlink() or not runtime_root.is_dir():
        fail("materialized runtime root must be a real directory")
    runtime_root = runtime_root.resolve(strict=True)
    files: list[tuple[str, Path]] = []
    observed: set[str] = set()
    total = 0
    for path in sorted(runtime_root.rglob("*")):
        if path.is_symlink():
            fail(f"materialized runtime contains symlink: {path}")
        if path.is_dir():
            continue
        if not path.is_file():
            fail(f"materialized runtime contains special entry: {path}")
        relative = safe_runtime_relative(runtime_root, path)
        register_casefold_unique_path(relative, observed)
        total += path.stat().st_size
        if total > MAX_RUNTIME_BYTES:
            fail("materialized runtime exceeds bounded size")
        files.append((relative, path))
        if len(files) > MAX_RUNTIME_FILES:
            fail("materialized runtime exceeds bounded file count")
    names = {relative for relative, _ in files}
    missing = sorted(REQUIRED_RUNTIME_PATHS.difference(names))
    if missing:
        fail(f"materialized runtime is missing required files: {missing}")
    return files


def runtime_tree_identity(files: list[tuple[str, Path]]) -> dict[str, Any]:
    entries = [
        {
            "path": relative,
            "sha256": sha256_file(path),
            "size_bytes": path.stat().st_size,
        }
        for relative, path in files
    ]
    return {
        "files": entries,
        "sha256": sha256_bytes(canonical(entries)),
    }


def runtime_manifest(
    commit_sha: str,
    source_files: list[Path],
    runtime_files: list[tuple[str, Path]],
) -> tuple[dict[str, Any], bytes]:
    manifest: dict[str, Any] = {
        "schema_version": 2,
        "kind": "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT",
        "platform": "windows-x86_64",
        "source_commit_sha": source_sha(commit_sha),
        "source_inputs": source_file_set_identity(source_files),
        "files": runtime_tree_identity(runtime_files),
        "entrypoints": {
            "browser": "browser/camoufox.exe",
            "camouhost": "camouhost/real.py",
            "python": "python/python.exe",
            "runtime_lock": "camouhost/runtime-lock.json",
        },
    }
    manifest["release_id"] = RELEASE_PREFIX + sha256_bytes(canonical(manifest))
    return manifest, document(manifest)


def deterministic_member(name: str, size: int) -> tarfile.TarInfo:
    pure = PurePosixPath(name)
    if pure.is_absolute() or ".." in pure.parts or not pure.parts:
        fail(f"unsafe runtime archive path: {name}")
    info = tarfile.TarInfo(name)
    info.size = size
    info.mode = 0o644
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    info.mtime = 0
    return info


def add_streamed_file(
    archive: tarfile.TarFile,
    name: str,
    path: Path,
) -> None:
    size = path.stat().st_size
    with path.open("rb") as handle:
        archive.addfile(deterministic_member(name, size), fileobj=handle)


def deterministic_archive(
    archive_path: Path,
    manifest_bytes: bytes,
    files: list[tuple[str, Path]],
) -> None:
    if archive_path.exists():
        fail(f"runtime package archive already exists: {archive_path}")
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive_path, "w", format=tarfile.PAX_FORMAT) as archive:
        ordered = [("runtime-manifest.json", None), *files]
        for name, path in sorted(ordered, key=lambda item: item[0]):
            if path is None:
                archive.addfile(
                    deterministic_member(name, len(manifest_bytes)),
                    fileobj=io.BytesIO(manifest_bytes),
                )
            else:
                add_streamed_file(archive, name, path)


def package_runtime(
    *,
    commit_sha: str,
    runtime_root: Path,
    archive_path: Path,
    manifest_path: Path,
) -> None:
    if manifest_path.exists():
        fail(f"runtime package manifest already exists: {manifest_path}")
    source_files = runtime_source_files()
    files = runtime_tree_files(runtime_root)
    _, manifest_bytes = runtime_manifest(commit_sha, source_files, files)
    deterministic_archive(archive_path, manifest_bytes, files)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_bytes(manifest_bytes)


def write_fixture_runtime(root: Path) -> None:
    fixtures = {
        "browser/camoufox.exe": b"browser",
        "camouhost/real.py": b"print('fixture')\n",
        "camouhost/resolved-runtime.json": b"{}\n",
        "camouhost/runtime-lock.json": b"{}\n",
        "python/python.exe": b"python",
        "python/python312.zip": b"stdlib",
    }
    for relative, content in fixtures.items():
        path = root / Path(*PurePosixPath(relative).parts)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)


def assert_archive(
    archive_path: Path,
    expected_files: list[str],
) -> None:
    with tarfile.open(archive_path, "r:") as archive:
        members = archive.getmembers()
        expected_names = sorted(["runtime-manifest.json", *expected_files])
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
                    "runtime archive deterministic metadata self-test failed: "
                    f"{member.name}"
                )


def self_test() -> None:
    commit_sha = "1" * 40
    with tempfile.TemporaryDirectory(prefix="camouhost-runtime-package-") as directory:
        temp = Path(directory)
        runtime_root = temp / "runtime"
        write_fixture_runtime(runtime_root)
        expected_files = [relative for relative, _ in runtime_tree_files(runtime_root)]
        first_archive = temp / "first.tar"
        first_manifest = temp / "first.json"
        second_archive = temp / "second.tar"
        second_manifest = temp / "second.json"
        package_runtime(
            commit_sha=commit_sha,
            runtime_root=runtime_root,
            archive_path=first_archive,
            manifest_path=first_manifest,
        )
        package_runtime(
            commit_sha=commit_sha,
            runtime_root=runtime_root,
            archive_path=second_archive,
            manifest_path=second_manifest,
        )
        if first_archive.read_bytes() != second_archive.read_bytes():
            fail("runtime archive packaging is not deterministic")
        if first_manifest.read_bytes() != second_manifest.read_bytes():
            fail("runtime manifest packaging is not deterministic")

        manifest = json.loads(first_manifest.read_text(encoding="utf-8"))
        if (
            manifest.get("schema_version") != 2
            or manifest.get("kind") != "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT"
            or manifest.get("source_commit_sha") != commit_sha
            or manifest.get("platform") != "windows-x86_64"
        ):
            fail("runtime manifest identity self-test failed")
        release_id = manifest.get("release_id")
        if not isinstance(release_id, str) or not release_id.startswith(RELEASE_PREFIX):
            fail("runtime release ID self-test failed")
        assert_archive(first_archive, expected_files)

        try:
            source_sha("X" * 40)
        except RuntimePackageError:
            pass
        else:
            fail("invalid source SHA negative self-test unexpectedly passed")

        observed = set()
        register_casefold_unique_path("alias.txt", observed)
        try:
            register_casefold_unique_path("ALIAS.TXT", observed)
        except RuntimePackageError:
            pass
        else:
            fail("runtime case-alias negative self-test unexpectedly passed")

    print("Camouhost Windows runtime component packaging self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)
    package = subcommands.add_parser("package")
    package.add_argument("--source-sha", required=True)
    package.add_argument("--runtime-root", type=Path, required=True)
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
                runtime_root=args.runtime_root,
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
