#!/usr/bin/env python3
"""Encrypted R2 profile generation smoke workflow."""

from __future__ import annotations

import argparse
import base64
import hashlib
import io
import json
import os
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import uuid
from pathlib import Path, PurePosixPath
from typing import Any

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

from r2_s3_canary import R2Credentials, R2S3Client


MAGIC = b"BPR2ENC1"
CHUNK_SIZE = 4 * 1024 * 1024
INVENTORY_NAME = ".snapshot_inventory.json"
EXCLUDED_TOP_LEVEL = {
    "cache2",
    "startupCache",
    "crashes",
    "minidumps",
}
EXCLUDED_NAMES = {".parentlock", "lock", "parent.lock"}


def key_from_environment() -> bytes:
    encoded = os.environ.get("PROFILE_ENCRYPTION_KEY_B64")
    if not encoded:
        raise RuntimeError("PROFILE_ENCRYPTION_KEY_B64 is required")
    key = base64.b64decode(encoded.strip(), validate=True)
    if len(key) != 32:
        raise RuntimeError("profile encryption key must contain 32 bytes")
    return key


def credentials_from_environment() -> R2Credentials:
    path = os.environ.get("R2_CREDENTIALS_FILE")
    if not path:
        raise RuntimeError("R2_CREDENTIALS_FILE is required")
    return R2Credentials.from_file(Path(path))


def atomic_write_json(path: Path, data: dict[str, Any]) -> None:
    temporary = path.with_suffix(f"{path.suffix}.tmp")
    temporary.write_text(
        json.dumps(data, indent=2, sort_keys=True),
        encoding="utf-8",
    )
    os.replace(temporary, path)


def set_display_name(workspace: Path, profile_id: str, display_name: str) -> dict[str, Any]:
    if not display_name or len(display_name) > 320 or any(ord(char) < 32 for char in display_name):
        raise ValueError("display name is empty, too long, or contains control characters")
    metadata_path = workspace / "profile.json"
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    if metadata.get("profile_id") != profile_id:
        raise RuntimeError("profile ID does not match metadata")
    metadata["display_name"] = display_name
    atomic_write_json(metadata_path, metadata)
    return {"profile_id": profile_id, "display_name_saved": True}


def is_excluded(relative: PurePosixPath) -> bool:
    parts = relative.parts
    if not parts:
        return False
    if parts[-1] in EXCLUDED_NAMES or parts[-1].endswith(".sqlite-shm"):
        return True
    return len(parts) > 1 and parts[0] == "user_data" and parts[1] in EXCLUDED_TOP_LEVEL


def profile_inventory(workspace: Path) -> list[dict[str, Any]]:
    inventory: list[dict[str, Any]] = []
    for path in sorted(workspace.rglob("*")):
        relative = PurePosixPath(path.relative_to(workspace).as_posix())
        if is_excluded(relative):
            continue
        if path.is_symlink():
            raise RuntimeError(f"unexpected symlink in profile snapshot: {relative}")
        if not path.is_file():
            continue
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
        inventory.append(
            {
                "path": str(relative),
                "size": path.stat().st_size,
                "sha256": digest.hexdigest(),
            }
        )
    return inventory


def create_archive(workspace: Path, archive_path: Path) -> list[dict[str, Any]]:
    inventory = profile_inventory(workspace)
    with tarfile.open(archive_path, mode="w:gz", compresslevel=6) as archive:
        for item in inventory:
            archive.add(
                workspace / item["path"],
                arcname=item["path"],
                recursive=False,
            )
        inventory_bytes = json.dumps(
            inventory,
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
        info = tarfile.TarInfo(INVENTORY_NAME)
        info.size = len(inventory_bytes)
        info.mode = 0o600
        archive.addfile(info, io.BytesIO(inventory_bytes))
    return inventory


def derive_generation_key(master_key: bytes, salt: bytes) -> bytes:
    return HKDF(
        algorithm=hashes.SHA256(),
        length=32,
        salt=salt,
        info=b"part-crm-browser-profile-generation-v1",
    ).derive(master_key)


def encrypt_file(source: Path, destination: Path, master_key: bytes) -> None:
    salt = os.urandom(16)
    nonce_prefix = os.urandom(8)
    header = MAGIC + salt + nonce_prefix + struct.pack(">I", CHUNK_SIZE)
    cipher = AESGCM(derive_generation_key(master_key, salt))
    with source.open("rb") as plain, destination.open("wb") as encrypted:
        encrypted.write(header)
        index = 0
        while True:
            chunk = plain.read(CHUNK_SIZE)
            nonce = nonce_prefix + struct.pack(">I", index)
            length = struct.pack(">I", len(chunk))
            encrypted.write(length)
            encrypted.write(cipher.encrypt(nonce, chunk, header + struct.pack(">I", index) + length))
            index += 1
            if not chunk:
                break


def decrypt_file(source: Path, destination: Path, master_key: bytes) -> None:
    with source.open("rb") as encrypted, destination.open("wb") as plain:
        header = encrypted.read(len(MAGIC) + 16 + 8 + 4)
        if len(header) != 36 or not header.startswith(MAGIC):
            raise RuntimeError("invalid encrypted profile header")
        salt = header[8:24]
        nonce_prefix = header[24:32]
        chunk_size = struct.unpack(">I", header[32:36])[0]
        if chunk_size != CHUNK_SIZE:
            raise RuntimeError("unsupported encrypted profile chunk size")
        cipher = AESGCM(derive_generation_key(master_key, salt))
        index = 0
        while True:
            raw_length = encrypted.read(4)
            if len(raw_length) != 4:
                raise RuntimeError("encrypted profile is truncated")
            length = struct.unpack(">I", raw_length)[0]
            if length > CHUNK_SIZE:
                raise RuntimeError("encrypted profile chunk is too large")
            ciphertext = encrypted.read(length + 16)
            if len(ciphertext) != length + 16:
                raise RuntimeError("encrypted profile is truncated")
            nonce = nonce_prefix + struct.pack(">I", index)
            plain.write(
                cipher.decrypt(
                    nonce,
                    ciphertext,
                    header + struct.pack(">I", index) + raw_length,
                )
            )
            index += 1
            if length == 0:
                if encrypted.read(1):
                    raise RuntimeError("encrypted profile has trailing data")
                break


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def upload_generation(workspace: Path, profile_id: str) -> dict[str, Any]:
    metadata_path = workspace / "profile.json"
    if not metadata_path.exists():
        raise RuntimeError("browser profile metadata is missing")
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    if metadata.get("profile_id") != profile_id:
        raise RuntimeError("profile ID does not match metadata")

    client = R2S3Client(credentials_from_environment())
    generation_id = uuid.uuid4().hex
    prefix = f"profiles/v1/{profile_id}/{generation_id}"
    with tempfile.TemporaryDirectory(prefix="profile-snapshot-") as temporary:
        temporary_path = Path(temporary)
        archive_path = temporary_path / "profile.tar.gz"
        encrypted_path = temporary_path / "profile.tar.gz.enc"
        inventory = create_archive(workspace, archive_path)
        archive_digest = sha256_file(archive_path)
        encrypt_file(archive_path, encrypted_path, key_from_environment())
        encrypted_bytes = encrypted_path.read_bytes()
        ciphertext_digest = hashlib.sha256(encrypted_bytes).hexdigest()
        object_key = f"{prefix}/profile.tar.gz.enc"
        manifest_key = f"{prefix}/manifest.json"
        manifest = {
            "schema_version": 1,
            "profile_id": profile_id,
            "generation_id": generation_id,
            "object_key": object_key,
            "archive_sha256": archive_digest,
            "ciphertext_sha256": ciphertext_digest,
            "ciphertext_bytes": len(encrypted_bytes),
            "inventory_files": len(inventory),
            "runtime": metadata.get("runtime"),
            "fingerprint_config_sha256": metadata.get("fingerprint_config_sha256"),
            "fingerprint_probe_sha256": metadata.get("fingerprint_probe_sha256"),
            "exclusion_policy": {
                "top_level": sorted(EXCLUDED_TOP_LEVEL),
                "names": sorted(EXCLUDED_NAMES),
                "suffixes": [".sqlite-shm"],
            },
        }
        manifest_bytes = json.dumps(
            manifest,
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
        pointer_bytes = json.dumps(
            {
                "schema_version": 1,
                "profile_id": profile_id,
                "generation_id": generation_id,
                "manifest_key": manifest_key,
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
        client.request("PUT", object_key, body=encrypted_bytes)
        client.request("PUT", manifest_key, body=manifest_bytes)
        client.request("PUT", f"profiles/v1/{profile_id}/current.json", body=pointer_bytes)
        restored_manifest = client.request("GET", manifest_key)
        restored_ciphertext = client.request("GET", object_key)
        if restored_manifest != manifest_bytes:
            raise RuntimeError("remote manifest verification failed")
        if hashlib.sha256(restored_ciphertext).hexdigest() != ciphertext_digest:
            raise RuntimeError("remote ciphertext verification failed")

    return {
        "profile_id": profile_id,
        "generation_id": generation_id,
        "ciphertext_bytes": manifest["ciphertext_bytes"],
        "inventory_files": manifest["inventory_files"],
        "remote_verified": True,
    }


def safe_extract(archive_path: Path, destination: Path) -> None:
    with tarfile.open(archive_path, mode="r:gz") as archive:
        for member in archive.getmembers():
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts:
                raise RuntimeError("unsafe path in profile archive")
            if member.issym() or member.islnk() or member.isdev():
                raise RuntimeError("unsupported link or device in profile archive")
        archive.extractall(destination, filter="data")


def verify_inventory(workspace: Path) -> int:
    inventory_path = workspace / INVENTORY_NAME
    inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
    for item in inventory:
        path = workspace / item["path"]
        if not path.is_file() or path.stat().st_size != item["size"]:
            raise RuntimeError(f"restored profile inventory mismatch: {item['path']}")
        if sha256_file(path) != item["sha256"]:
            raise RuntimeError(f"restored profile digest mismatch: {item['path']}")
    inventory_path.unlink()
    return len(inventory)


def restore_generation(profile_id: str, destination: Path) -> dict[str, Any]:
    if destination.exists():
        raise RuntimeError("restore destination already exists")
    client = R2S3Client(credentials_from_environment())
    pointer_key = f"profiles/v1/{profile_id}/current.json"
    pointer = json.loads(client.request("GET", pointer_key))
    if pointer.get("profile_id") != profile_id:
        raise RuntimeError("remote pointer profile ID mismatch")
    manifest_key = str(pointer["manifest_key"])
    expected_prefix = f"profiles/v1/{profile_id}/{pointer['generation_id']}/"
    if not manifest_key.startswith(expected_prefix):
        raise RuntimeError("remote manifest key is outside generation prefix")
    manifest_bytes = client.request("GET", manifest_key)
    manifest = json.loads(manifest_bytes)
    object_key = str(manifest["object_key"])
    if not object_key.startswith(expected_prefix):
        raise RuntimeError("remote object key is outside generation prefix")
    ciphertext = client.request("GET", object_key)
    if hashlib.sha256(ciphertext).hexdigest() != manifest["ciphertext_sha256"]:
        raise RuntimeError("downloaded ciphertext digest mismatch")

    staging = destination.with_name(f".{destination.name}.staging-{uuid.uuid4().hex}")
    try:
        staging.mkdir(parents=True)
        with tempfile.TemporaryDirectory(prefix="profile-restore-") as temporary:
            temporary_path = Path(temporary)
            encrypted_path = temporary_path / "profile.tar.gz.enc"
            archive_path = temporary_path / "profile.tar.gz"
            encrypted_path.write_bytes(ciphertext)
            decrypt_file(encrypted_path, archive_path, key_from_environment())
            if sha256_file(archive_path) != manifest["archive_sha256"]:
                raise RuntimeError("decrypted archive digest mismatch")
            safe_extract(archive_path, staging)
        files = verify_inventory(staging)
        metadata = json.loads((staging / "profile.json").read_text(encoding="utf-8"))
        if metadata.get("profile_id") != profile_id:
            raise RuntimeError("restored metadata profile ID mismatch")
        os.replace(staging, destination)
    finally:
        if staging.exists():
            shutil.rmtree(staging)

    return {
        "profile_id": profile_id,
        "generation_id": manifest["generation_id"],
        "inventory_files": files,
        "restored": True,
    }


def browser_subprocess_environment() -> dict[str, str]:
    allowed = {
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "GDK_BACKEND",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOGNAME",
        "PATH",
        "SHELL",
        "USER",
        "WAYLAND_DISPLAY",
        "WSL_DISTRO_NAME",
        "WSL_INTEROP",
        "XDG_RUNTIME_DIR",
    }
    return {key: value for key, value in os.environ.items() if key in allowed}


def open_and_sync(
    workspace: Path,
    profile_id: str,
    target_url: str,
    auto_close_seconds: float | None,
) -> dict[str, Any]:
    camoufox_python = os.environ.get(
        "CAMOUFOX_PYTHON",
        "/home/bose/projects/camoufox/.venv/bin/python",
    )
    browser_script = Path(__file__).with_name("profile_browser.py")
    command = [
        camoufox_python,
        str(browser_script),
        "--workspace",
        str(workspace),
        "--profile-id",
        profile_id,
        "--url",
        target_url,
    ]
    if auto_close_seconds is not None:
        command.extend(["--auto-close-seconds", str(auto_close_seconds)])
    subprocess.run(
        command,
        check=True,
        env=browser_subprocess_environment(),
    )
    return upload_generation(workspace, profile_id)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    new = subparsers.add_parser("new")
    new.add_argument("--root", type=Path, default=Path("state/cloud-smoke"))

    open_sync = subparsers.add_parser("open-and-sync")
    open_sync.add_argument("--workspace", type=Path, required=True)
    open_sync.add_argument("--profile-id", required=True)
    open_sync.add_argument("--url", default="https://e.mail.ru/inbox/")
    open_sync.add_argument("--auto-close-seconds", type=float)

    upload = subparsers.add_parser("upload")
    upload.add_argument("--workspace", type=Path, required=True)
    upload.add_argument("--profile-id", required=True)

    restore = subparsers.add_parser("restore")
    restore.add_argument("--profile-id", required=True)
    restore.add_argument("--destination", type=Path, required=True)

    set_name = subparsers.add_parser("set-display-name")
    set_name.add_argument("--workspace", type=Path, required=True)
    set_name.add_argument("--profile-id", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "new":
        profile_id = uuid.uuid4().hex
        workspace = (args.root / profile_id / "source").resolve()
        workspace.mkdir(parents=True)
        result = {"profile_id": profile_id, "workspace": str(workspace)}
    elif args.command == "open-and-sync":
        result = open_and_sync(
            args.workspace.resolve(),
            args.profile_id,
            args.url,
            args.auto_close_seconds,
        )
    elif args.command == "upload":
        result = upload_generation(args.workspace.resolve(), args.profile_id)
    elif args.command == "restore":
        result = restore_generation(args.profile_id, args.destination.resolve())
    else:
        display_name = sys.stdin.readline().rstrip("\r\n")
        result = set_display_name(args.workspace.resolve(), args.profile_id, display_name)
    print(json.dumps({"success": True, **result}, separators=(",", ":")), flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(
            json.dumps(
                {"success": False, "error": type(error).__name__, "message": str(error)},
                separators=(",", ":"),
            ),
            file=sys.stderr,
        )
        raise
