#!/usr/bin/env python3
"""Build and verify the immutable accepted-main mailbox resolver release."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CANONICAL_REPOSITORY = "iamaman11/part-crm-emai-profile"
CONFIG = Path("deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc")
MIGRATIONS = Path("migrations/resolver-d1")
WORKER_BUILD = Path("apps/mailbox-secret-resolver-worker/build")
RELEASE_ROOT = Path("artifacts/mailbox-secret-resolver-release")
MANIFEST_NAME = "release-manifest.json"
RELEASE_PREFIX = "mailbox-secret-resolver-v1-sha256-"
WORKER_FILES = ("index.js", "index_bg.wasm", "worker/shim.mjs")
GENERATED_METADATA_FILES = {".gitignore", "index.d.ts", "package.json"}
IDENTITY_FIELDS = (
    "release_id",
    "source_commit_sha",
    "resolver_worker_sha256",
    "resolver_migration_manifest_sha256",
    "resolver_config_sha256",
    "build_toolchain",
)
WORKER_BUILD_VERSION = "0.8.5"
WRANGLER_VERSION = "4.30.0"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
MIGRATION_RE = re.compile(r"^[0-9]{4}_[a-z0-9_]+\.sql$")
BUILD_RE = re.compile(r"worker-build\s+--version\s+([0-9]+\.[0-9]+\.[0-9]+)\s+--locked")


class ReleaseError(ValueError):
    """Fail-closed resolver release validation error."""


def fail(message: str) -> None:
    raise ReleaseError(message)


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def canonical_document(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"expected regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_directory(path: Path) -> None:
    if path.is_symlink() or not path.is_dir():
        fail(f"expected real directory: {path}")


def file_inventory(base: Path, relative_paths: list[str]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for relative in sorted(relative_paths):
        pure = PurePosixPath(relative)
        if pure.is_absolute() or ".." in pure.parts or not pure.parts:
            fail(f"unsafe release path: {relative}")
        path = base.joinpath(*pure.parts)
        entries.append(
            {"path": pure.as_posix(), "size": path.stat().st_size, "sha256": sha256_file(path)}
        )
    if not entries or len({entry["path"] for entry in entries}) != len(entries):
        fail("release inventory must be non-empty and unique")
    return entries


def inventory_digest(entries: list[dict[str, Any]]) -> str:
    return sha256_bytes(canonical(entries))


def migration_paths(root: Path) -> list[Path]:
    directory = root / MIGRATIONS
    require_directory(directory)
    paths = sorted(directory.glob("*.sql"), key=lambda path: path.name)
    if not paths or any(MIGRATION_RE.fullmatch(path.name) is None for path in paths):
        fail("resolver migrations must be a non-empty canonical append-only sequence")
    numbers = [int(path.name[:4]) for path in paths]
    if len(numbers) != len(set(numbers)) or numbers != sorted(numbers):
        fail("resolver migration numbers must be unique and ordered")
    return paths


def migration_digest(root: Path) -> str:
    paths = migration_paths(root)
    return inventory_digest(file_inventory(root / MIGRATIONS, [path.name for path in paths]))


def worker_digest(directory: Path) -> str:
    require_directory(directory)
    entries = file_inventory(directory, list(WORKER_FILES))
    actual = sorted(
        path.relative_to(directory).as_posix()
        for path in directory.rglob("*")
        if path.is_file()
        and path.relative_to(directory).parts[0] != ".tmp"
        and path.name not in GENERATED_METADATA_FILES
    )
    if actual != sorted(WORKER_FILES):
        fail(f"resolver Worker runtime inventory drifted: {actual}")
    if "./index_bg.wasm" not in (directory / "index.js").read_text(encoding="utf-8"):
        fail("resolver Worker index does not load the packaged WASM module")
    return inventory_digest(entries)


def load_config(root: Path) -> dict[str, Any]:
    path = root / CONFIG
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseError(f"cannot read canonical resolver config: {error}") from error
    if not isinstance(value, dict):
        fail("canonical resolver config must be an object")
    if value.get("name") != "mailbox-secret-resolver":
        fail("canonical resolver Worker name drifted")
    if value.get("main") != "../../apps/mailbox-secret-resolver-worker/build/worker/shim.mjs":
        fail("canonical resolver Worker entrypoint drifted")
    if value.get("workers_dev") is not False or value.get("triggers") != {"crons": ["17 * * * *"]}:
        fail("resolver must remain private with the bounded reconciliation schedule")
    build = value.get("build")
    if not isinstance(build, dict) or build.get("cwd") != "../../apps/mailbox-secret-resolver-worker":
        fail("resolver build root drifted")
    command = build.get("command")
    match = BUILD_RE.search(command) if isinstance(command, str) else None
    if match is None or match.group(1) != WORKER_BUILD_VERSION or "worker-build --release" not in command:
        fail("resolver worker-build command is not exactly pinned")
    environments = value.get("env")
    if not isinstance(environments, dict) or set(environments) != {"staging", "production"}:
        fail("resolver config must define exactly staging and production")
    expected_names = {
        "staging": "mailbox-secret-resolver-staging",
        "production": "mailbox-secret-resolver-production",
    }
    observed_databases: set[str] = set()
    for environment, expected_name in expected_names.items():
        item = environments.get(environment)
        if not isinstance(item, dict) or item.get("name") != expected_name:
            fail(f"resolver {environment} Worker name drifted")
        if item.get("workers_dev") is not False or item.get("routes") != []:
            fail(f"resolver {environment} must have no public route")
        databases = item.get("d1_databases")
        if not isinstance(databases, list) or len(databases) != 1:
            fail(f"resolver {environment} must bind one dedicated D1")
        database = databases[0]
        if not isinstance(database, dict) or database.get("binding") != "RESOLVER_DB":
            fail(f"resolver {environment} D1 binding drifted")
        identity = database.get("database_id")
        if not isinstance(identity, str) or identity in observed_databases:
            fail("resolver staging and production D1 identities must be isolated")
        observed_databases.add(identity)
    return value


def toolchain(root: Path) -> dict[str, str]:
    rust_path = root / "rust-toolchain.toml"
    cargo_lock = root / "Cargo.lock"
    rust = tomllib.loads(rust_path.read_text(encoding="utf-8"))
    channel = rust.get("toolchain", {}).get("channel")
    if not isinstance(channel, str) or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", channel) is None:
        fail("Rust toolchain must be exactly pinned")
    load_config(root)
    return {
        "cargo_lock_sha256": sha256_file(cargo_lock),
        "rust": channel,
        "worker_build": WORKER_BUILD_VERSION,
        "wrangler": WRANGLER_VERSION,
    }


def validate_source(root: Path, source_sha: str, repository: str, *, check_git: bool) -> str:
    source_sha = source_sha.lower()
    if COMMIT_RE.fullmatch(source_sha) is None:
        fail("release source must be a full commit SHA")
    if repository != CANONICAL_REPOSITORY:
        fail("resolver releases are restricted to the canonical repository")
    if check_git:
        completed = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True, capture_output=True, check=True
        )
        if completed.stdout.strip() != source_sha:
            fail("release source SHA does not match the exact checkout")
    return source_sha


def manifest_for(root: Path, source_sha: str, worker_directory: Path) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "source_commit_sha": source_sha,
        "resolver_worker_sha256": worker_digest(worker_directory),
        "resolver_migration_manifest_sha256": migration_digest(root),
        "resolver_config_sha256": sha256_file(root / CONFIG),
        "build_toolchain": toolchain(root),
    }
    release_id = RELEASE_PREFIX + sha256_bytes(canonical(payload))
    return {"release_id": release_id, **payload}


def verify_manifest(manifest: Any) -> dict[str, Any]:
    if not isinstance(manifest, dict) or set(manifest) != set(IDENTITY_FIELDS):
        fail("resolver release identity field inventory drifted")
    release_id = manifest.get("release_id")
    if not isinstance(release_id, str) or not release_id.startswith(RELEASE_PREFIX):
        fail("resolver release ID is invalid")
    payload = dict(manifest)
    del payload["release_id"]
    if release_id != RELEASE_PREFIX + sha256_bytes(canonical(payload)):
        fail("resolver release ID does not authenticate its identity fields")
    if COMMIT_RE.fullmatch(str(manifest.get("source_commit_sha"))) is None:
        fail("resolver release source SHA is invalid")
    return manifest


def copy_file(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination, follow_symlinks=False)


def verify_directory(root: Path, release_directory: Path, expected_source_sha: str) -> dict[str, Any]:
    require_directory(release_directory)
    manifest_path = release_directory / MANIFEST_NAME
    manifest = verify_manifest(json.loads(manifest_path.read_text(encoding="utf-8")))
    if canonical_document(manifest) != manifest_path.read_bytes():
        fail("resolver release manifest is not canonically serialized")
    if manifest["source_commit_sha"] != expected_source_sha:
        fail("resolver release source SHA differs from the expected accepted main")
    if manifest["resolver_worker_sha256"] != worker_digest(release_directory / "worker"):
        fail("resolver Worker bits differ from the immutable release identity")
    if manifest["resolver_migration_manifest_sha256"] != migration_digest(release_directory):
        fail("resolver migration set differs from the immutable release identity")
    if manifest["resolver_migration_manifest_sha256"] != migration_digest(root):
        fail("resolver migration set differs from the exact source authority")
    if manifest["resolver_config_sha256"] != sha256_file(release_directory / CONFIG):
        fail("resolver config differs from the immutable release identity")
    if manifest["resolver_config_sha256"] != sha256_file(root / CONFIG):
        fail("resolver config differs from the exact source authority")
    if manifest["build_toolchain"] != toolchain(root):
        fail("resolver build toolchain differs from the exact source authority")
    if release_directory.name != manifest["release_id"]:
        fail("resolver release directory does not match its immutable ID")
    return manifest


def deterministic_tar(release_directory: Path, archive_path: Path) -> None:
    if archive_path.exists():
        fail(f"immutable resolver release already exists: {archive_path}")
    prefix = PurePosixPath("mailbox-secret-resolver-release") / release_directory.name
    directories = {PurePosixPath("mailbox-secret-resolver-release"), prefix}
    files: list[tuple[Path, PurePosixPath]] = []
    for path in sorted(release_directory.rglob("*"), key=lambda item: item.as_posix()):
        if path.is_symlink():
            fail(f"resolver release must not contain links: {path}")
        relative = prefix / PurePosixPath(path.relative_to(release_directory).as_posix())
        if path.is_dir():
            directories.add(relative)
        elif path.is_file():
            files.append((path, relative))
        else:
            fail(f"unsupported resolver release entry: {path}")
    with tarfile.open(archive_path, "w", format=tarfile.PAX_FORMAT) as archive:
        for relative in sorted(directories, key=lambda item: item.as_posix()):
            info = tarfile.TarInfo(relative.as_posix() + "/")
            info.type = tarfile.DIRTYPE
            info.mode = 0o755
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ""
            archive.addfile(info)
        for path, relative in files:
            data = path.read_bytes()
            info = tarfile.TarInfo(relative.as_posix())
            info.size = len(data)
            info.mode = 0o644
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ""
            archive.addfile(info, io.BytesIO(data))


def safe_extract(archive_path: Path, destination: Path) -> Path:
    if archive_path.is_symlink() or not archive_path.is_file():
        fail("resolver release archive is missing")
    observed: set[str] = set()
    with tarfile.open(archive_path, "r:") as archive:
        members = archive.getmembers()
        if not members:
            fail("resolver release archive is empty")
        for member in members:
            pure = PurePosixPath(member.name)
            if pure.is_absolute() or ".." in pure.parts or not pure.parts:
                fail(f"unsafe resolver archive path: {member.name}")
            if pure.as_posix() in observed or member.issym() or member.islnk():
                fail(f"duplicate or linked resolver archive entry: {member.name}")
            observed.add(pure.as_posix())
            if not (member.isdir() or member.isfile()):
                fail(f"unsupported resolver archive entry: {member.name}")
            if member.uid != 0 or member.gid != 0 or member.mtime != 0:
                fail(f"non-deterministic resolver archive metadata: {member.name}")
            target = destination.joinpath(*pure.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                source = archive.extractfile(member)
                if source is None:
                    fail(f"cannot read resolver archive entry: {member.name}")
                target.write_bytes(source.read())
    base = destination / "mailbox-secret-resolver-release"
    require_directory(base)
    releases = [path for path in base.iterdir() if path.is_dir() and not path.is_symlink()]
    if len(releases) != 1:
        fail("resolver archive must contain exactly one release directory")
    return releases[0]


def verify_archive(root: Path, archive_path: Path, expected_source_sha: str) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="resolver-release-verify-") as temporary:
        directory = safe_extract(archive_path, Path(temporary))
        return verify_directory(root, directory, expected_source_sha)


def build_release(
    root: Path,
    source_sha: str,
    repository: str,
    release_root: Path,
    *,
    check_git: bool = True,
) -> tuple[dict[str, Any], Path]:
    source_sha = validate_source(root, source_sha, repository, check_git=check_git)
    worker = root / WORKER_BUILD
    manifest = manifest_for(root, source_sha, worker)
    output = release_root / manifest["release_id"]
    if output.exists():
        fail(f"immutable resolver release directory already exists: {output}")
    output.mkdir(parents=True)
    try:
        for relative in WORKER_FILES:
            copy_file(worker / relative, output / "worker" / relative)
        for migration in migration_paths(root):
            copy_file(migration, output / MIGRATIONS / migration.name)
        copy_file(root / CONFIG, output / CONFIG)
        (output / MANIFEST_NAME).write_bytes(canonical_document(manifest))
        verify_directory(root, output, source_sha)
        archive = release_root / f"{manifest['release_id']}.tar"
        deterministic_tar(output, archive)
        verify_archive(root, archive, source_sha)
    except Exception:
        shutil.rmtree(output, ignore_errors=True)
        (release_root / f"{manifest['release_id']}.tar").unlink(missing_ok=True)
        raise
    return manifest, archive


def check_repository(root: Path) -> None:
    load_config(root)
    migration_digest(root)
    toolchain(root)
    print("Mailbox resolver immutable release policy is valid.")


def fixture_root(root: Path) -> None:
    worker = root / WORKER_BUILD
    (worker / "worker").mkdir(parents=True)
    (worker / "index.js").write_text("import wasm from './index_bg.wasm';\n", encoding="utf-8")
    (worker / "index_bg.wasm").write_bytes(b"\x00asmfixture")
    (worker / "worker" / "shim.mjs").write_text("export * from '../index.js';\n", encoding="utf-8")
    (worker / ".gitignore").write_text(".tmp/\n", encoding="utf-8")
    (root / MIGRATIONS).mkdir(parents=True)
    (root / MIGRATIONS / "0001_fixture.sql").write_text("SELECT 1;\n", encoding="utf-8")
    (root / CONFIG).parent.mkdir(parents=True)
    (root / CONFIG).write_text(
        json.dumps(
            {
                "name": "mailbox-secret-resolver",
                "main": "../../apps/mailbox-secret-resolver-worker/build/worker/shim.mjs",
                "compatibility_date": "2026-08-05",
                "workers_dev": False,
                "triggers": {"crons": ["17 * * * *"]},
                "build": {
                    "command": "cargo install worker-build --version 0.8.5 --locked && worker-build --release",
                    "cwd": "../../apps/mailbox-secret-resolver-worker",
                },
                "env": {
                    "staging": {
                        "name": "mailbox-secret-resolver-staging",
                        "workers_dev": False,
                        "routes": [],
                        "d1_databases": [{"binding": "RESOLVER_DB", "database_id": "${STAGING}"}],
                    },
                    "production": {
                        "name": "mailbox-secret-resolver-production",
                        "workers_dev": False,
                        "routes": [],
                        "d1_databases": [{"binding": "RESOLVER_DB", "database_id": "${PRODUCTION}"}],
                    },
                },
            }
        )
        + "\n",
        encoding="utf-8",
    )
    (root / "Cargo.lock").write_text("# fixture\n", encoding="utf-8")
    (root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.97.1"\n', encoding="utf-8")


def expect_rejected(label: str, operation: Any) -> None:
    try:
        operation()
    except (ReleaseError, json.JSONDecodeError, tarfile.TarError):
        return
    fail(f"negative resolver release fixture unexpectedly passed: {label}")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="resolver-release-self-test-") as temporary:
        root = Path(temporary) / "repo"
        root.mkdir()
        fixture_root(root)
        source_sha = "a" * 40
        first, first_archive = build_release(
            root, source_sha, CANONICAL_REPOSITORY, root / "release-a", check_git=False
        )
        second, second_archive = build_release(
            root, source_sha, CANONICAL_REPOSITORY, root / "release-b", check_git=False
        )
        if first != second or first_archive.read_bytes() != second_archive.read_bytes():
            fail("identical resolver inputs produced different immutable releases")
        unexpected_worker_file = root / WORKER_BUILD / "unexpected.js"
        unexpected_worker_file.write_text("export {};\n", encoding="utf-8")
        expect_rejected(
            "unexpected Worker build file",
            lambda: worker_digest(root / WORKER_BUILD),
        )
        unexpected_worker_file.unlink()
        release_directory = first_archive.parent / first["release_id"]
        worker = release_directory / "worker" / "index_bg.wasm"
        original = worker.read_bytes()
        worker.write_bytes(original + b"tampered")
        expect_rejected("Worker substitution", lambda: verify_directory(root, release_directory, source_sha))
        worker.write_bytes(original)
        migration = release_directory / MIGRATIONS / "0001_fixture.sql"
        original_migration = migration.read_bytes()
        migration.write_text("SELECT 2;\n", encoding="utf-8")
        expect_rejected(
            "migration substitution", lambda: verify_directory(root, release_directory, source_sha)
        )
        migration.write_bytes(original_migration)
        config = release_directory / CONFIG
        original_config = config.read_bytes()
        config.write_bytes(original_config + b"\n")
        expect_rejected(
            "config substitution", lambda: verify_directory(root, release_directory, source_sha)
        )
        config.write_bytes(original_config)
        expect_rejected(
            "wrong accepted-main source",
            lambda: verify_directory(root, release_directory, "b" * 40),
        )
        manifest_path = release_directory / MANIFEST_NAME
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["unexpected"] = "field"
        expect_rejected("identity field drift", lambda: verify_manifest(manifest))
        malicious = root / "malicious.tar"
        with tarfile.open(malicious, "w") as archive:
            info = tarfile.TarInfo("../escape")
            info.size = 1
            archive.addfile(info, io.BytesIO(b"x"))
        expect_rejected("archive traversal", lambda: safe_extract(malicious, root / "extract"))
        expect_rejected(
            "fork repository",
            lambda: validate_source(root, source_sha, "example/fork", check_git=False),
        )
    print("Mailbox resolver release positive and negative self-tests passed.")


def github_output(manifest: dict[str, Any], archive: Path) -> None:
    output = os.environ.get("GITHUB_OUTPUT")
    if not output:
        fail("--github-output requires GITHUB_OUTPUT")
    with Path(output).open("a", encoding="utf-8") as handle:
        handle.write(f"release_id={manifest['release_id']}\n")
        handle.write(f"archive={archive.as_posix()}\n")
        handle.write(f"archive_sha256={sha256_file(archive)}\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check")
    commands.add_parser("self-test")
    build = commands.add_parser("build")
    build.add_argument("--source-sha", required=True)
    build.add_argument("--repository", required=True)
    build.add_argument("--github-output", action="store_true")
    verify = commands.add_parser("verify-archive")
    verify.add_argument("--archive", type=Path, required=True)
    verify.add_argument("--expected-source-sha", required=True)
    args = parser.parse_args()
    if args.command == "check":
        check_repository(ROOT)
    elif args.command == "self-test":
        self_test()
    elif args.command == "build":
        manifest, archive = build_release(
            ROOT, args.source_sha, args.repository, ROOT / RELEASE_ROOT
        )
        if args.github_output:
            github_output(manifest, archive)
        print(f"Built immutable mailbox resolver release {manifest['release_id']}.")
    elif args.command == "verify-archive":
        manifest = verify_archive(ROOT, args.archive, args.expected_source_sha.lower())
        print(f"Verified immutable mailbox resolver release {manifest['release_id']}.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ReleaseError as error:
        raise SystemExit(f"mailbox resolver release rejected: {error}") from error
