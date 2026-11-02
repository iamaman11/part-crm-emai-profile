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
import os
import json
import re
import tarfile
import gzip
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any, BinaryIO

ROOT = Path(__file__).resolve().parents[1]
RELEASE_ARCHITECTURE = ROOT / "architecture" / "release-architecture-ar11.json"
GITATTRIBUTES = ROOT / ".gitattributes"
RUNTIME_CONSUMER = "runtime_bundle.files"
RELEASE_PREFIX = "runtime-bundle-v3-sha256-"
MAX_RUNTIME_FILES = 500_000
MAX_RUNTIME_BYTES = 4 * 1024 * 1024 * 1024
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


def verify_checkout_identity_policy(paths: list[Path]) -> None:
    try:
        lines = {
            line.strip()
            for line in GITATTRIBUTES.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        }
    except OSError as error:
        raise RuntimePackageError(
            f"cannot read runtime checkout identity policy: {error}"
        ) from error

    for relative in paths:
        rule = f"/{relative.as_posix()} text eol=lf"
        if rule not in lines:
            fail(
                "runtime component identity input lacks platform-stable checkout "
                f"policy: {relative.as_posix()}"
            )


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
    verify_checkout_identity_policy(selected)
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


def runtime_input_identity(source_files: list[Path] | None = None) -> dict[str, Any]:
    source_files = runtime_source_files() if source_files is None else source_files
    source_inputs = source_file_set_identity(source_files)
    return {
        "schema_version": 1,
        "kind": "CAMOUFOX_WINDOWS_RUNTIME_INPUT_IDENTITY",
        "platform": "windows-x86_64",
        "source_inputs": source_inputs,
        "release_id": RELEASE_PREFIX + source_inputs["sha256"],
    }


def runtime_manifest(
    source_files: list[Path],
    runtime_files: list[tuple[str, Path]],
) -> tuple[dict[str, Any], bytes]:
    identity = runtime_input_identity(source_files)
    manifest: dict[str, Any] = {
        "schema_version": 3,
        "kind": "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT",
        "platform": "windows-x86_64",
        "source_inputs": identity["source_inputs"],
        "files": runtime_tree_identity(runtime_files),
        "entrypoints": {
            "browser": "browser/camoufox.exe",
            "camouhost": "camouhost/real.py",
            "python": "python/python.exe",
            "runtime_lock": "camouhost/runtime-lock.json",
        },
        "release_id": identity["release_id"],
    }
    return manifest, document(manifest)


def verify_manifest_file(path: Path) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        fail("runtime manifest must be a regular file")
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimePackageError("runtime manifest is invalid JSON") from error
    expected_fields = {
        "schema_version",
        "kind",
        "platform",
        "source_inputs",
        "files",
        "entrypoints",
        "release_id",
    }
    if not isinstance(manifest, dict) or set(manifest) != expected_fields:
        fail("runtime manifest field inventory mismatch")
    if (
        manifest.get("schema_version") != 3
        or manifest.get("kind") != "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT"
        or manifest.get("platform") != "windows-x86_64"
    ):
        fail("runtime manifest identity mismatch")
    expected = runtime_input_identity()
    if manifest.get("source_inputs") != expected["source_inputs"]:
        fail("runtime manifest source input identity differs from current authority")
    if manifest.get("release_id") != expected["release_id"]:
        fail("runtime manifest release ID differs from current input identity")
    files = manifest.get("files")
    if (
        not isinstance(files, dict)
        or set(files) != {"files", "sha256"}
        or not isinstance(files.get("files"), list)
        or not files["files"]
        or not isinstance(files.get("sha256"), str)
        or re.fullmatch(r"[0-9a-f]{64}", files["sha256"]) is None
    ):
        fail("runtime manifest file inventory identity is invalid")
    if manifest.get("entrypoints") != {
        "browser": "browser/camoufox.exe",
        "camouhost": "camouhost/real.py",
        "python": "python/python.exe",
        "runtime_lock": "camouhost/runtime-lock.json",
    }:
        fail("runtime manifest entrypoints mismatch")
    return manifest


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
    # Stream one deterministic gzip member over the same canonical PAX TAR.
    # Windows delivery expands it into bounded temporary storage, then invokes
    # the existing PAX header/path/range validator unchanged.
    with archive_path.open("xb") as output:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=output, mtime=0, compresslevel=1
        ) as compressed:
            with tarfile.open(
                fileobj=compressed, mode="w|", format=tarfile.PAX_FORMAT
            ) as archive:
                ordered = [("runtime-manifest.json", None), *files]
                for name, path in sorted(ordered, key=lambda item: item[0]):
                    if path is None:
                        archive.addfile(
                            deterministic_member(name, len(manifest_bytes)),
                            fileobj=io.BytesIO(manifest_bytes),
                        )
                    else:
                        add_streamed_file(archive, name, path)



def verify_staged_archive(
    archive_path: Path,
    manifest_bytes: bytes,
) -> None:
    """Read every archived byte, enforce inventory/digests and reach gzip CRC/EOF."""
    manifest = json.loads(manifest_bytes)
    entries = {
        row["path"]: (row["size_bytes"], row["sha256"])
        for row in manifest["files"]["files"]
    }
    entries["runtime-manifest.json"] = (
        len(manifest_bytes), sha256_bytes(manifest_bytes)
    )
    try:
        with gzip.open(archive_path, "rb") as decoded:
            with tarfile.open(fileobj=decoded, mode="r|") as archive:
                for name, (size, digest) in sorted(entries.items()):
                    member = archive.next()
                    if (
                        member is None
                        or member.name != name
                        or not member.isfile()
                        or member.size != size
                        or member.uid != 0
                        or member.gid != 0
                        or member.mtime != 0
                        or member.mode != 0o644
                        or member.uname != ""
                        or member.gname != ""
                    ):
                        fail(f"runtime staged archive entry mismatch: {name}")
                    source = archive.extractfile(member)
                    if source is None:
                        fail(f"runtime staged archive entry unreadable: {name}")
                    observed = hashlib.sha256()
                    remaining = size
                    while remaining:
                        block = source.read(min(1024 * 1024, remaining))
                        if not block:
                            fail(f"runtime staged archive entry truncated: {name}")
                        observed.update(block)
                        remaining -= len(block)
                    if observed.hexdigest() != digest:
                        fail(f"runtime staged archive digest mismatch: {name}")
                if archive.next() is not None:
                    fail("runtime staged archive has unexpected trailing entries")
            # TAR ends before gzip EOF; read through the gzip footer to prove CRC.
            while decoded.read(1024 * 1024):
                pass
    except (OSError, EOFError, tarfile.TarError) as error:
        raise RuntimePackageError(
            f"runtime staged archive gzip/TAR validation failed: {error}"
        ) from error


def package_runtime(
    *,
    runtime_root: Path,
    archive_path: Path,
    manifest_path: Path,
) -> None:
    if archive_path.exists() or manifest_path.exists():
        fail("runtime package output already exists; no accepted artifact may be overwritten")
    source_files = runtime_source_files()
    files = runtime_tree_files(runtime_root)
    _, manifest_bytes = runtime_manifest(source_files, files)
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)

    # Stage on the destination filesystems. The final archive name becomes
    # visible only after the entire gzip member and its manifest are verified.
    with tempfile.TemporaryDirectory(
        prefix=".camouhost-package-", dir=archive_path.parent
    ) as archive_stage, tempfile.TemporaryDirectory(
        prefix=".camouhost-manifest-", dir=manifest_path.parent
    ) as manifest_stage:
        staged_archive = Path(archive_stage) / "runtime.tar.gz"
        staged_manifest = Path(manifest_stage) / "runtime-manifest.json"
        deterministic_archive(staged_archive, manifest_bytes, files)
        with staged_archive.open("rb+") as output:
            os.fsync(output.fileno())
        if staged_archive.stat().st_size >= 2 * 1024 * 1024 * 1024:
            fail("compressed runtime archive exceeds 2 GiB release asset bound")
        verify_staged_archive(staged_archive, manifest_bytes)
        with staged_manifest.open("xb") as output:
            output.write(manifest_bytes)
            output.flush()
            os.fsync(output.fileno())

        # Hard-link publication is atomic on NTFS/POSIX and refuses to replace
        # an existing accepted artifact. Publish the archive LAST.
        try:
            os.link(staged_manifest, manifest_path)
        except OSError as error:
            raise RuntimePackageError(
                f"runtime manifest atomic publication failed: {error}"
            ) from error
        try:
            os.link(staged_archive, archive_path)
        except OSError as error:
            # This manifest was created exclusively by this call.
            manifest_path.unlink()
            raise RuntimePackageError(
                f"runtime archive atomic publication failed: {error}"
            ) from error


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
    with tarfile.open(archive_path, "r:gz") as archive:
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
    with tempfile.TemporaryDirectory(prefix="camouhost-runtime-package-") as directory:
        temp = Path(directory)
        runtime_root = temp / "runtime"
        write_fixture_runtime(runtime_root)
        expected_files = [relative for relative, _ in runtime_tree_files(runtime_root)]
        first_archive = temp / "first.tar.gz"
        first_manifest = temp / "first.json"
        second_archive = temp / "second.tar.gz"
        second_manifest = temp / "second.json"
        package_runtime(
            runtime_root=runtime_root,
            archive_path=first_archive,
            manifest_path=first_manifest,
        )
        package_runtime(
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
            manifest.get("schema_version") != 3
            or manifest.get("kind") != "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT"
            or manifest.get("platform") != "windows-x86_64"
        ):
            fail("runtime manifest identity self-test failed")
        release_id = manifest.get("release_id")
        if (
            not isinstance(release_id, str)
            or release_id != RELEASE_PREFIX + manifest["source_inputs"]["sha256"]
        ):
            fail("runtime release ID self-test failed")
        if verify_manifest_file(first_manifest)["release_id"] != release_id:
            fail("runtime manifest current-input verification self-test failed")
        assert_archive(first_archive, expected_files)
        # Truncated gzip must never be eligible for final-name publication.
        corrupted = temp / "truncated.tar.gz"
        corrupted.write_bytes(first_archive.read_bytes()[:-8])
        try:
            verify_staged_archive(corrupted, first_manifest.read_bytes())
        except RuntimePackageError:
            pass
        else:
            fail("truncated gzip negative self-test unexpectedly passed")
        # A previously published archive/manifest must not be overwritten.
        original_archive = first_archive.read_bytes()
        try:
            package_runtime(
                runtime_root=runtime_root,
                archive_path=first_archive,
                manifest_path=first_manifest,
            )
        except RuntimePackageError:
            pass
        else:
            fail("existing-output negative self-test unexpectedly passed")
        if first_archive.read_bytes() != original_archive:
            fail("existing accepted runtime archive changed")

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
    identity = subcommands.add_parser("identity")
    identity.add_argument("--output", type=Path)
    verify = subcommands.add_parser("verify-manifest")
    verify.add_argument("--manifest", type=Path, required=True)
    package = subcommands.add_parser("package")
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
        elif args.command == "identity":
            payload = document(runtime_input_identity())
            if args.output is None:
                print(payload.decode("utf-8"), end="")
            else:
                if args.output.exists():
                    fail("runtime input identity output already exists")
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_bytes(payload)
        elif args.command == "verify-manifest":
            manifest = verify_manifest_file(args.manifest)
            print(document(manifest).decode("utf-8"), end="")
        elif args.command == "package":
            package_runtime(
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
