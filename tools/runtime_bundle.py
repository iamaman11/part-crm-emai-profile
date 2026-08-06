#!/usr/bin/env python3
"""Build, verify and extract deterministic synthetic Camouhost runtime bundles."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable

SOURCE_MARKER = ".synthetic-runtime-root"
SOURCE_MARKER_CONTENT = "synthetic-runtime-v1\n"
DESTINATION_MARKER = ".synthetic-runtime-destination"
DESTINATION_MARKER_CONTENT = "synthetic-runtime-destination-v1\n"
MANIFEST_NAME = "bundle-manifest.json"
PAYLOAD_PREFIX = "payload/"
MANIFEST_SCHEMA_VERSION = 1
IPC_VERSION = 1
PLATFORM = "windows-x86_64"
ENTRYPOINT = "camouhost/main.py"
RUNTIME_VERSION = "0.1.0"
PYTHON_VERSION = "3.12"
MAX_PATH_LENGTH = 240
MAX_SEGMENT_LENGTH = 80
FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
RESERVED_NAMES = {"CON", "PRN", "AUX", "NUL"} | {
    f"{prefix}{number}" for prefix in ("COM", "LPT") for number in range(1, 10)
}
MANIFEST_KEYS = {
    "schema_version",
    "runtime_version",
    "python_version",
    "ipc_version",
    "platform",
    "entrypoint",
    "inventory_sha256",
    "entries",
}
ENTRY_KEYS = {"path", "length", "sha256"}


class BundleError(ValueError):
    """Raised when a runtime bundle violates the accepted deterministic format."""


@dataclass(frozen=True)
class InventoryEntry:
    path: str
    length: int
    sha256: str

    def as_dict(self) -> dict[str, object]:
        return {"length": self.length, "path": self.path, "sha256": self.sha256}


@dataclass(frozen=True)
class VerifiedBundle:
    manifest: dict[str, object]
    entries: tuple[InventoryEntry, ...]


def validate_relative_path(value: str) -> str:
    if not value or len(value) > MAX_PATH_LENGTH:
        raise BundleError("bundle path is empty or too long")
    if value.startswith("/") or ":" in value or "\\" in value:
        raise BundleError("bundle path is not a canonical relative POSIX path")
    if "//" in value or value.endswith("/"):
        raise BundleError("bundle path has an invalid separator")

    parts = value.split("/")
    for part in parts:
        if (
            not part
            or part in {".", ".."}
            or len(part) > MAX_SEGMENT_LENGTH
            or part.endswith((".", " "))
            or not all(character.isascii() and (character.isalnum() or character in "._-") for character in part)
        ):
            raise BundleError("bundle path contains an invalid segment")
        stem = part.split(".", 1)[0].upper()
        if stem in RESERVED_NAMES:
            raise BundleError("bundle path contains a Windows-reserved segment")
    return value


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _require_source_marker(source: Path) -> None:
    marker = source / SOURCE_MARKER
    if not source.is_dir() or marker.is_symlink() or not marker.is_file():
        raise BundleError("source is not an explicitly marked synthetic runtime root")
    if marker.read_text(encoding="utf-8") != SOURCE_MARKER_CONTENT:
        raise BundleError("synthetic runtime source marker is invalid")


def _source_entries(source: Path) -> tuple[InventoryEntry, ...]:
    _require_source_marker(source)
    entries: list[InventoryEntry] = []
    case_folded: set[str] = set()

    for candidate in sorted(source.rglob("*"), key=lambda value: value.as_posix()):
        if candidate == source / SOURCE_MARKER:
            continue
        if candidate.is_symlink():
            raise BundleError("runtime source may not contain symbolic links")
        if candidate.is_dir():
            continue
        if not candidate.is_file():
            raise BundleError("runtime source contains an unsupported filesystem entry")

        relative = validate_relative_path(candidate.relative_to(source).as_posix())
        folded = relative.casefold()
        if folded in case_folded:
            raise BundleError("runtime source contains duplicate or case-colliding paths")
        case_folded.add(folded)
        data = candidate.read_bytes()
        entries.append(InventoryEntry(relative, len(data), sha256_bytes(data)))

    entries.sort(key=lambda entry: entry.path.casefold())
    if not entries:
        raise BundleError("runtime source inventory is empty")
    if ENTRYPOINT.casefold() not in case_folded:
        raise BundleError("runtime source does not contain the required entrypoint")
    return tuple(entries)


def inventory_digest(entries: Iterable[InventoryEntry]) -> str:
    return sha256_bytes(canonical_json([entry.as_dict() for entry in entries]))


def build_manifest(entries: tuple[InventoryEntry, ...]) -> dict[str, object]:
    return {
        "entries": [entry.as_dict() for entry in entries],
        "entrypoint": ENTRYPOINT,
        "inventory_sha256": inventory_digest(entries),
        "ipc_version": IPC_VERSION,
        "platform": PLATFORM,
        "python_version": PYTHON_VERSION,
        "runtime_version": RUNTIME_VERSION,
        "schema_version": MANIFEST_SCHEMA_VERSION,
    }


def _zip_info(name: str, mode: int = 0o644) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, FIXED_ZIP_TIME)
    info.compress_type = zipfile.ZIP_STORED
    info.create_system = 3
    info.external_attr = (stat.S_IFREG | mode) << 16
    return info


def build_bundle(source: Path, output: Path) -> dict[str, object]:
    source = source.resolve(strict=True)
    entries = _source_entries(source)
    manifest = build_manifest(entries)
    output.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.NamedTemporaryFile(
        prefix=f".{output.name}.", suffix=".tmp", dir=output.parent, delete=False
    ) as temporary:
        temporary_path = Path(temporary.name)
    try:
        with zipfile.ZipFile(temporary_path, "w", allowZip64=False) as archive:
            archive.writestr(_zip_info(MANIFEST_NAME), canonical_json(manifest))
            for entry in entries:
                archive.writestr(
                    _zip_info(f"{PAYLOAD_PREFIX}{entry.path}"),
                    (source / PurePosixPath(entry.path)).read_bytes(),
                )
        os.replace(temporary_path, output)
    finally:
        temporary_path.unlink(missing_ok=True)
    return manifest


def _parse_entry(value: object) -> InventoryEntry:
    if not isinstance(value, dict) or set(value) != ENTRY_KEYS:
        raise BundleError("runtime inventory entry has an invalid shape")
    path = value.get("path")
    length = value.get("length")
    digest = value.get("sha256")
    if not isinstance(path, str):
        raise BundleError("runtime inventory path is invalid")
    validate_relative_path(path)
    if not isinstance(length, int) or isinstance(length, bool) or length < 0:
        raise BundleError("runtime inventory length is invalid")
    if not isinstance(digest, str) or len(digest) != 64 or any(
        character not in "0123456789abcdef" for character in digest
    ):
        raise BundleError("runtime inventory digest is invalid")
    return InventoryEntry(path, length, digest)


def _validate_archive_name(name: str) -> str:
    if name == MANIFEST_NAME:
        return name
    if not name.startswith(PAYLOAD_PREFIX):
        raise BundleError("runtime archive contains an unexpected top-level entry")
    relative = name.removeprefix(PAYLOAD_PREFIX)
    validate_relative_path(relative)
    return relative


def verify_bundle(bundle: Path) -> VerifiedBundle:
    with zipfile.ZipFile(bundle, "r") as archive:
        infos = archive.infolist()
        names: set[str] = set()
        folded_names: set[str] = set()
        for info in infos:
            if info.is_dir() or info.flag_bits & 0x1:
                raise BundleError("runtime archive contains a directory or encrypted entry")
            mode = (info.external_attr >> 16) & 0o170000
            if mode == stat.S_IFLNK:
                raise BundleError("runtime archive contains a symbolic link")
            _validate_archive_name(info.filename)
            if info.filename in names or info.filename.casefold() in folded_names:
                raise BundleError("runtime archive contains duplicate or case-colliding names")
            names.add(info.filename)
            folded_names.add(info.filename.casefold())
        if MANIFEST_NAME not in names:
            raise BundleError("runtime archive manifest is missing")

        manifest_bytes = archive.read(MANIFEST_NAME)
        try:
            manifest = json.loads(manifest_bytes.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise BundleError("runtime archive manifest is invalid JSON") from error
        if not isinstance(manifest, dict) or set(manifest) != MANIFEST_KEYS:
            raise BundleError("runtime archive manifest shape is invalid")
        if manifest.get("schema_version") != MANIFEST_SCHEMA_VERSION:
            raise BundleError("runtime manifest schema version is unsupported")
        if manifest.get("runtime_version") != RUNTIME_VERSION:
            raise BundleError("runtime version is unsupported")
        if manifest.get("python_version") != PYTHON_VERSION:
            raise BundleError("Python runtime version is unsupported")
        if manifest.get("ipc_version") != IPC_VERSION:
            raise BundleError("runtime IPC version is unsupported")
        if manifest.get("platform") != PLATFORM:
            raise BundleError("runtime platform is unsupported")
        if manifest.get("entrypoint") != ENTRYPOINT:
            raise BundleError("runtime entrypoint is unsupported")

        raw_entries = manifest.get("entries")
        if not isinstance(raw_entries, list) or not raw_entries:
            raise BundleError("runtime inventory is missing")
        entries = tuple(_parse_entry(value) for value in raw_entries)
        if list(entries) != sorted(entries, key=lambda entry: entry.path.casefold()):
            raise BundleError("runtime inventory is not canonically sorted")
        folded_paths = [entry.path.casefold() for entry in entries]
        if len(folded_paths) != len(set(folded_paths)):
            raise BundleError("runtime inventory contains case collisions")
        if ENTRYPOINT.casefold() not in folded_paths:
            raise BundleError("runtime entrypoint is absent from inventory")
        if manifest.get("inventory_sha256") != inventory_digest(entries):
            raise BundleError("runtime inventory digest does not match")

        expected_payload_names = {f"{PAYLOAD_PREFIX}{entry.path}" for entry in entries}
        if names - {MANIFEST_NAME} != expected_payload_names:
            raise BundleError("runtime archive payload does not match the manifest")
        for entry in entries:
            data = archive.read(f"{PAYLOAD_PREFIX}{entry.path}")
            if len(data) != entry.length or sha256_bytes(data) != entry.sha256:
                raise BundleError("runtime payload content does not match the manifest")

        if canonical_json(manifest) != manifest_bytes:
            raise BundleError("runtime manifest is not canonically encoded")
        return VerifiedBundle(manifest=manifest, entries=entries)


def _require_empty_destination(destination: Path) -> None:
    marker = destination / DESTINATION_MARKER
    if not destination.is_dir() or marker.is_symlink() or not marker.is_file():
        raise BundleError("destination is not an explicitly marked synthetic directory")
    if marker.read_text(encoding="utf-8") != DESTINATION_MARKER_CONTENT:
        raise BundleError("synthetic destination marker is invalid")
    if any(candidate != marker for candidate in destination.iterdir()):
        raise BundleError("synthetic destination must be empty except for its marker")


def extract_bundle(bundle: Path, destination: Path) -> VerifiedBundle:
    verified = verify_bundle(bundle)
    destination = destination.resolve(strict=True)
    _require_empty_destination(destination)

    with zipfile.ZipFile(bundle, "r") as archive:
        for entry in verified.entries:
            relative = PurePosixPath(entry.path)
            target = destination.joinpath(*relative.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.exists() or target.is_symlink():
                raise BundleError("runtime extraction target already exists")
            resolved_parent = target.parent.resolve(strict=True)
            if destination != resolved_parent and destination not in resolved_parent.parents:
                raise BundleError("runtime extraction escaped the destination")
            data = archive.read(f"{PAYLOAD_PREFIX}{entry.path}")
            with target.open("xb") as handle:
                handle.write(data)
    return verified


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    build = subparsers.add_parser("build")
    build.add_argument("source", type=Path)
    build.add_argument("output", type=Path)

    verify = subparsers.add_parser("verify")
    verify.add_argument("bundle", type=Path)

    extract = subparsers.add_parser("extract")
    extract.add_argument("bundle", type=Path)
    extract.add_argument("destination", type=Path)
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    try:
        if arguments.command == "build":
            manifest = build_bundle(arguments.source, arguments.output)
            print(manifest["inventory_sha256"])
        elif arguments.command == "verify":
            verified = verify_bundle(arguments.bundle)
            print(verified.manifest["inventory_sha256"])
        else:
            verified = extract_bundle(arguments.bundle, arguments.destination)
            print(verified.manifest["inventory_sha256"])
    except (BundleError, OSError, zipfile.BadZipFile) as error:
        print(f"runtime bundle error: {error}", file=os.sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
