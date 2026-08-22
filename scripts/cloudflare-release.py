#!/usr/bin/env python3
"""Build and verify the immutable Cloudflare Worker + Static Assets release unit.

D2 owns release identity/provenance only. Deployment and production promotion remain D3 concerns.
"""

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
from typing import Any, Iterable, Sequence

import d1_repository_projection as d1_repository

ROOT = Path(__file__).resolve().parents[1]
CANONICAL_REPOSITORY = "iamaman11/part-crm-emai-profile"
SCHEMA_VERSION = 1
CONTRACT_VERSION = "v1"
RELEASE_DIR = ROOT / "artifacts" / "cloudflare-release"
FRONTEND_DIST = Path("frontend/dist")
WORKER_BUILD_DIR = Path("apps/control-plane-worker/build")
DEPLOYMENT_CONFIG = Path("deploy/cloudflare/wrangler.jsonc")
# worker-build 0.8.5 emits the deployable module closure at the build root while
# retaining build/worker/shim.mjs as the Wrangler-compatible entrypoint alias.
# Do not hash .tmp/intermediate files or non-runtime package/type metadata.
WORKER_RUNTIME_FILES = ("index.js", "index_bg.wasm", "worker/shim.mjs")
WORKER_ENTRYPOINT = "worker/shim.mjs"
MANIFEST_NAME = "release-manifest.json"
RELEASE_PREFIX = "cloudflare-v1-sha256-"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SEMVER_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
MIGRATION_RE = re.compile(r"^(?P<number>[0-9]{4})_[a-z0-9_]+\.sql$")
WORKER_BUILD_RE = re.compile(r"worker-build\s+--version\s+([0-9]+\.[0-9]+\.[0-9]+)\s+--locked")
FORBIDDEN_MANIFEST_KEY_PARTS = (
    "password",
    "credential",
    "secret_value",
    "access_key_id",
    "secret_access_key",
    "api_token",
    "account_id",
    "database_id",
)


class ReleaseError(ValueError):
    """Raised when release provenance fails closed."""


def fail(message: str) -> None:
    raise ReleaseError(message)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_compact(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def canonical_document(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def require_regular_file(path: Path, label: str) -> None:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")


def require_directory(path: Path, label: str) -> None:
    if path.is_symlink() or not path.is_dir():
        fail(f"{label} must be a real directory: {path}")


def inventory_entries(directory: Path, relative_paths: Sequence[str]) -> dict[str, Any]:
    require_directory(directory, "artifact directory")
    entries: list[dict[str, Any]] = []
    observed: set[str] = set()
    for relative in sorted(relative_paths):
        pure = PurePosixPath(relative)
        if pure.is_absolute() or ".." in pure.parts or not pure.parts:
            fail(f"invalid artifact path: {relative!r}")
        normalized = pure.as_posix()
        if normalized in observed:
            fail(f"duplicate artifact path: {normalized}")
        observed.add(normalized)
        path = directory.joinpath(*pure.parts)
        require_regular_file(path, "artifact file")
        entries.append({"path": normalized, "size": path.stat().st_size, "sha256": sha256_file(path)})
    if not entries:
        fail("artifact inventory must not be empty")
    return {
        "file_count": len(entries),
        "sha256": sha256_bytes(canonical_compact(entries)),
        "files": entries,
    }


def inventory_directory(directory: Path) -> dict[str, Any]:
    require_directory(directory, "artifact directory")
    relative_paths: list[str] = []
    for path in sorted(directory.rglob("*"), key=lambda candidate: candidate.as_posix()):
        if path.is_symlink():
            fail(f"release artifacts must not contain symlinks: {path}")
        if path.is_dir():
            continue
        if not path.is_file():
            fail(f"release artifacts contain unsupported filesystem entry: {path}")
        relative_paths.append(path.relative_to(directory).as_posix())
    return inventory_entries(directory, relative_paths)


def inventory_repo_files(root: Path, files: Iterable[Path]) -> dict[str, Any]:
    entries: list[dict[str, Any]] = []
    observed: set[str] = set()
    for path in sorted(files, key=lambda candidate: candidate.as_posix()):
        require_regular_file(path, "repository provenance input")
        try:
            relative = path.relative_to(root).as_posix()
        except ValueError as exc:
            raise ReleaseError(f"provenance input escapes repository root: {path}") from exc
        if relative in observed:
            fail(f"duplicate provenance input: {relative}")
        observed.add(relative)
        entries.append({"path": relative, "size": path.stat().st_size, "sha256": sha256_file(path)})
    if not entries:
        fail("provenance input set must not be empty")
    return {
        "file_count": len(entries),
        "sha256": sha256_bytes(canonical_compact(entries)),
        "files": entries,
    }


def contract_inventory(root: Path) -> dict[str, Any]:
    roots = (
        root / "openapi" / "v1",
        root / "contracts" / "generated",
        root / "frontend" / "src" / "shared" / "api" / "generated",
    )
    paths: list[Path] = []
    for directory in roots:
        require_directory(directory, "generated contract directory")
        for path in directory.rglob("*"):
            if path.is_symlink():
                fail(f"contract identity must not follow symlinks: {path}")
            if path.is_file():
                paths.append(path)
    inventory = inventory_repo_files(root, paths)
    inventory["version"] = CONTRACT_VERSION
    return inventory


def migration_paths(root: Path) -> list[Path]:
    directory = root / "migrations" / "d1"
    require_directory(directory, "D1 migration directory")
    migrations = sorted(directory.glob("*.sql"), key=lambda path: path.name)
    numbers: list[int] = []
    for migration in migrations:
        match = MIGRATION_RE.fullmatch(migration.name)
        if match is None:
            fail(f"unexpected D1 migration filename: {migration.name}")
        numbers.append(int(match.group("number")))
    if not migrations:
        fail("D1 migration set must not be empty")
    if len(numbers) != len(set(numbers)) or numbers != sorted(numbers):
        fail("D1 migration sequence must be unique and monotonically ordered")
    return migrations


def migration_inventory(root: Path) -> dict[str, Any]:
    migrations = migration_paths(root)
    inventory = inventory_repo_files(root, migrations)
    inventory.update(
        {
            "engine": "cloudflare-d1",
            "directory": "migrations/d1",
            "first": migrations[0].name,
            "latest": migrations[-1].name,
        }
    )
    return inventory


def load_schema_contract(root: Path) -> dict[str, str]:
    try:
        return d1_repository.release_contract(root, "catalog")
    except d1_repository.D1ProjectionError as error:
        raise ReleaseError(f"typed catalog D1 repository projection failed: {error}") from error


def load_toolchain_identity(root: Path) -> dict[str, Any]:
    rust_path = root / "rust-toolchain.toml"
    frontend_package_path = root / "frontend" / "package.json"
    frontend_lock_path = root / "frontend" / "package-lock.json"
    cargo_lock_path = root / "Cargo.lock"
    wrangler_path = root / "deploy" / "cloudflare" / "wrangler.jsonc"
    for path, label in (
        (rust_path, "Rust toolchain authority"),
        (frontend_package_path, "frontend package authority"),
        (frontend_lock_path, "frontend lockfile"),
        (cargo_lock_path, "Cargo lockfile"),
        (wrangler_path, "Wrangler authority"),
    ):
        require_regular_file(path, label)

    rust = tomllib.loads(rust_path.read_text(encoding="utf-8"))
    channel = rust.get("toolchain", {}).get("channel")
    if not isinstance(channel, str) or SEMVER_RE.fullmatch(channel) is None:
        fail("rust-toolchain.toml must pin an exact stable Rust version")

    package = json.loads(frontend_package_path.read_text(encoding="utf-8"))
    engines = package.get("engines")
    if not isinstance(engines, dict):
        fail("frontend/package.json must define exact Node/npm engines")
    node = engines.get("node")
    npm = engines.get("npm")
    if not isinstance(node, str) or SEMVER_RE.fullmatch(node) is None:
        fail("frontend Node engine must be an exact semantic version")
    if not isinstance(npm, str) or SEMVER_RE.fullmatch(npm) is None:
        fail("frontend npm engine must be an exact semantic version")
    if package.get("packageManager") != f"npm@{npm}":
        fail("frontend packageManager must match the exact npm engine pin")

    wrangler = json.loads(wrangler_path.read_text(encoding="utf-8"))
    build = wrangler.get("build")
    if not isinstance(build, dict):
        fail("canonical Wrangler authority must define the Worker build")
    build_command = build.get("command")
    build_cwd = build.get("cwd")
    if not isinstance(build_command, str) or not isinstance(build_cwd, str):
        fail("canonical Wrangler Worker build command/cwd are incomplete")
    worker_match = WORKER_BUILD_RE.search(build_command)
    if worker_match is None:
        fail("canonical Worker build must pin worker-build with --locked")
    worker_build = worker_match.group(1)
    if "worker-build --release" not in build_command:
        fail("canonical Worker build must produce a release artifact")

    main = wrangler.get("main")
    if main != "../../apps/control-plane-worker/build/worker/shim.mjs":
        fail("canonical Wrangler authority must use worker-build's shim.mjs entrypoint")

    return {
        "rust": {
            "channel": channel,
            "cargo_lock_sha256": sha256_file(cargo_lock_path),
            "toolchain_authority_sha256": sha256_file(rust_path),
        },
        "frontend": {
            "node": node,
            "npm": npm,
            "package_lock_sha256": sha256_file(frontend_lock_path),
            "package_authority_sha256": sha256_file(frontend_package_path),
            "install_command": "npm ci",
            "build_command": "npm run build",
        },
        "worker": {
            "worker_build": worker_build,
            "build_command": build_command,
            "build_cwd": build_cwd,
            "entrypoint": WORKER_ENTRYPOINT,
            "runtime_files": list(WORKER_RUNTIME_FILES),
            "wrangler_authority_sha256": sha256_file(wrangler_path),
        },
    }


def git_head(root: Path) -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        fail("release source must be built from a Git checkout")
    head = completed.stdout.strip().lower()
    if COMMIT_RE.fullmatch(head) is None:
        fail(f"Git HEAD is not an exact commit SHA: {head!r}")
    return head


def validate_source_context(
    root: Path,
    *,
    source_sha: str,
    repository: str,
    source_ref: str,
    source_event: str,
    authority: str,
    check_git: bool = True,
) -> dict[str, str]:
    source_sha = source_sha.lower()
    if COMMIT_RE.fullmatch(source_sha) is None:
        fail("source SHA must be exactly 40 lowercase hexadecimal characters")
    if repository != CANONICAL_REPOSITORY:
        fail(f"release repository must be canonical: {CANONICAL_REPOSITORY}")
    if not source_ref.startswith("refs/"):
        fail("source ref must be an explicit Git ref")
    if not source_event:
        fail("source event must be explicit")
    if authority not in {"review-candidate", "accepted-main"}:
        fail("release authority must be review-candidate or accepted-main")
    if authority == "accepted-main":
        if source_event != "push" or source_ref != "refs/heads/main":
            fail("accepted-main authority is allowed only for an exact push to refs/heads/main")
    elif source_event == "push" and source_ref == "refs/heads/main":
        fail("a canonical main push must not be downgraded to review-candidate authority")
    if check_git and git_head(root) != source_sha:
        fail("declared source SHA does not equal the exact checked-out Git HEAD")
    return {
        "repository": repository,
        "commit_sha": source_sha,
        "ref": source_ref,
        "event": source_event,
        "authority": authority,
    }


def validate_frontend_shape(directory: Path) -> None:
    require_regular_file(directory / "index.html", "frontend entrypoint")
    assets = directory / "assets"
    require_directory(assets, "frontend hashed assets directory")
    if not any(path.is_file() for path in assets.rglob("*")):
        fail("frontend assets directory must contain at least one built file")


def validate_worker_shape(directory: Path) -> None:
    for relative in WORKER_RUNTIME_FILES:
        require_regular_file(directory.joinpath(*PurePosixPath(relative).parts), "Worker runtime file")
    shim = (directory / "worker" / "shim.mjs").read_text(encoding="utf-8")
    if "../index.js" not in shim:
        fail("Worker shim must retain worker-build's ../index.js runtime alias")
    index = (directory / "index.js").read_text(encoding="utf-8")
    if "./index_bg.wasm" not in index:
        fail("Worker index.js must reference the packaged index_bg.wasm module")


def worker_runtime_inventory(directory: Path) -> dict[str, Any]:
    validate_worker_shape(directory)
    return inventory_entries(directory, WORKER_RUNTIME_FILES)


def validate_manifest_has_no_sensitive_authority(manifest: Any, path: str = "manifest") -> None:
    if isinstance(manifest, dict):
        for key, value in manifest.items():
            lowered = str(key).lower()
            if any(part in lowered for part in FORBIDDEN_MANIFEST_KEY_PARTS):
                fail(f"release manifest contains forbidden secret/resource authority key: {path}.{key}")
            validate_manifest_has_no_sensitive_authority(value, f"{path}.{key}")
    elif isinstance(manifest, list):
        for index, value in enumerate(manifest):
            validate_manifest_has_no_sensitive_authority(value, f"{path}[{index}]")


def release_id_for(payload: dict[str, Any]) -> str:
    return RELEASE_PREFIX + sha256_bytes(canonical_compact(payload))


def build_manifest_payload(
    root: Path,
    *,
    source: dict[str, str],
    frontend_directory: Path,
    worker_directory: Path,
) -> dict[str, Any]:
    validate_frontend_shape(frontend_directory)
    return {
        "schema_version": SCHEMA_VERSION,
        "source": source,
        "artifacts": {
            "frontend": {"kind": "workers-static-assets", **inventory_directory(frontend_directory)},
            "worker": {
                "kind": "cloudflare-worker",
                "entrypoint": WORKER_ENTRYPOINT,
                **worker_runtime_inventory(worker_directory),
            },
            "deployment_config": {
                "kind": "wrangler-config",
                **inventory_entries(root, [DEPLOYMENT_CONFIG.as_posix()]),
            },
        },
        "contracts": contract_inventory(root),
        "migrations": migration_inventory(root),
        "schema_contract": load_schema_contract(root),
        "build": load_toolchain_identity(root),
    }


def finalized_manifest(payload: dict[str, Any]) -> dict[str, Any]:
    manifest = dict(payload)
    manifest["release_id"] = release_id_for(payload)
    validate_manifest_has_no_sensitive_authority(manifest)
    return manifest


def copy_tree_exact(source: Path, destination: Path) -> None:
    if destination.exists():
        fail(f"immutable release destination already exists: {destination}")
    shutil.copytree(source, destination, symlinks=True)
    inventory_directory(destination)


def copy_worker_runtime(source: Path, destination: Path) -> None:
    if destination.exists():
        fail(f"immutable Worker destination already exists: {destination}")
    validate_worker_shape(source)
    for relative in WORKER_RUNTIME_FILES:
        pure = PurePosixPath(relative)
        source_path = source.joinpath(*pure.parts)
        target = destination.joinpath(*pure.parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source_path, target, follow_symlinks=False)
    if worker_runtime_inventory(source) != worker_runtime_inventory(destination):
        fail("copied Worker runtime differs from the exact worker-build output")


def write_manifest(path: Path, manifest: dict[str, Any]) -> None:
    if path.exists():
        fail(f"immutable release manifest already exists: {path}")
    path.write_bytes(canonical_document(manifest))


def verify_manifest_document(manifest: dict[str, Any]) -> None:
    if manifest.get("schema_version") != SCHEMA_VERSION:
        fail("unsupported release manifest schema version")
    release_id = manifest.get("release_id")
    if not isinstance(release_id, str) or not release_id.startswith(RELEASE_PREFIX):
        fail("release manifest has invalid immutable release identifier")
    payload = dict(manifest)
    del payload["release_id"]
    if release_id_for(payload) != release_id:
        fail("release identifier does not match canonical manifest payload")
    schema_contract = manifest.get("schema_contract")
    if not isinstance(schema_contract, dict) or schema_contract.get("database_component") != "catalog":
        fail("release manifest Catalog schema contract is invalid")
    validate_manifest_has_no_sensitive_authority(manifest)


def compare_inventory(label: str, expected: Any, actual: dict[str, Any]) -> None:
    if not isinstance(expected, dict):
        fail(f"manifest {label} inventory is missing")
    for field in ("file_count", "sha256", "files"):
        if expected.get(field) != actual.get(field):
            fail(f"{label} inventory mismatch: {field}")


def verify_release_directory(
    root: Path,
    release_directory: Path,
    *,
    expected_source_sha: str | None = None,
    expected_repository: str | None = None,
    expected_authority: str | None = None,
    check_git: bool = True,
) -> dict[str, Any]:
    require_directory(release_directory, "release directory")
    manifest_path = release_directory / MANIFEST_NAME
    require_regular_file(manifest_path, "release manifest")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict):
        fail("release manifest root must be an object")
    if canonical_document(manifest) != manifest_path.read_bytes():
        fail("release manifest serialization is not canonical")
    verify_manifest_document(manifest)

    source = manifest.get("source")
    if not isinstance(source, dict):
        fail("release manifest source identity is missing")
    source_sha = source.get("commit_sha")
    repository = source.get("repository")
    authority = source.get("authority")
    source_ref = source.get("ref")
    source_event = source.get("event")
    if not all(isinstance(value, str) for value in (source_sha, repository, authority, source_ref, source_event)):
        fail("release manifest source identity is incomplete")
    validate_source_context(
        root,
        source_sha=source_sha,
        repository=repository,
        source_ref=source_ref,
        source_event=source_event,
        authority=authority,
        check_git=check_git,
    )
    if expected_source_sha is not None and source_sha != expected_source_sha.lower():
        fail("release source SHA does not match the verifier expectation")
    if expected_repository is not None and repository != expected_repository:
        fail("release repository does not match the verifier expectation")
    if expected_authority is not None and authority != expected_authority:
        fail("release authority does not match the verifier expectation")

    frontend_directory = release_directory / "frontend"
    worker_directory = release_directory / "worker"
    validate_frontend_shape(frontend_directory)
    validate_worker_shape(worker_directory)
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, dict):
        fail("release manifest artifact inventories are missing")
    frontend_manifest = artifacts.get("frontend")
    worker_manifest = artifacts.get("worker")
    deployment_config_manifest = artifacts.get("deployment_config")
    compare_inventory("frontend", frontend_manifest, inventory_directory(frontend_directory))
    compare_inventory("worker", worker_manifest, worker_runtime_inventory(worker_directory))
    compare_inventory(
        "deployment config",
        deployment_config_manifest,
        inventory_entries(release_directory, [DEPLOYMENT_CONFIG.as_posix()]),
    )
    if not isinstance(worker_manifest, dict) or worker_manifest.get("entrypoint") != WORKER_ENTRYPOINT:
        fail("release manifest Worker entrypoint is not canonical")

    contracts = manifest.get("contracts")
    migrations = manifest.get("migrations")
    schema_contract = manifest.get("schema_contract")
    build = manifest.get("build")
    actual_contracts = contract_inventory(root)
    actual_migrations = migration_inventory(root)
    release_migrations = migration_inventory(release_directory)
    actual_schema_contract = load_schema_contract(root)
    actual_build = load_toolchain_identity(root)
    if contracts != actual_contracts:
        fail("generated contract identity no longer matches the exact source checkout")
    if migrations != actual_migrations or migrations != release_migrations:
        fail("D1 migration-set identity no longer matches the exact source checkout")
    if schema_contract != actual_schema_contract:
        fail("Catalog schema contract no longer matches the exact D1 evolution authority")
    if build != actual_build:
        fail("build/toolchain identity no longer matches repository authorities")
    return manifest


def deterministic_tar(release_directory: Path, archive_path: Path) -> None:
    if archive_path.exists():
        fail(f"immutable release archive already exists: {archive_path}")
    release_id = release_directory.name
    with tarfile.open(archive_path, mode="w", format=tarfile.PAX_FORMAT) as archive:
        directories: set[PurePosixPath] = {
            PurePosixPath("cloudflare-release"),
            PurePosixPath("cloudflare-release") / release_id,
        }
        file_paths: list[tuple[Path, PurePosixPath]] = []
        for path in sorted(release_directory.rglob("*"), key=lambda candidate: candidate.as_posix()):
            if path.is_symlink():
                fail(f"release archive must not contain symlinks: {path}")
            relative = (
                PurePosixPath("cloudflare-release")
                / release_id
                / PurePosixPath(path.relative_to(release_directory).as_posix())
            )
            if path.is_dir():
                directories.add(relative)
            elif path.is_file():
                file_paths.append((path, relative))
            else:
                fail(f"unsupported release archive entry: {path}")
        for relative in sorted(directories, key=lambda value: value.as_posix()):
            info = tarfile.TarInfo(relative.as_posix() + "/")
            info.type = tarfile.DIRTYPE
            info.mode = 0o755
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mtime = 0
            archive.addfile(info)
        for path, relative in file_paths:
            data = path.read_bytes()
            info = tarfile.TarInfo(relative.as_posix())
            info.size = len(data)
            info.mode = 0o644
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))


def safe_archive_members(archive: tarfile.TarFile) -> list[tarfile.TarInfo]:
    members = archive.getmembers()
    if not members:
        fail("release archive is empty")
    observed: set[str] = set()
    for member in members:
        pure = PurePosixPath(member.name)
        normalized = pure.as_posix()
        if pure.is_absolute() or ".." in pure.parts or not pure.parts:
            fail(f"release archive contains unsafe path: {member.name}")
        if normalized in observed:
            fail(f"release archive contains duplicate path: {member.name}")
        observed.add(normalized)
        if member.issym() or member.islnk():
            fail(f"release archive contains a link: {member.name}")
        if not (member.isdir() or member.isfile()):
            fail(f"release archive contains unsupported entry: {member.name}")
        if member.uid != 0 or member.gid != 0 or member.mtime != 0:
            fail(f"release archive metadata is not deterministic: {member.name}")
    return members


def verify_archive(
    root: Path,
    archive_path: Path,
    *,
    expected_source_sha: str | None,
    expected_repository: str | None,
    expected_authority: str | None,
    check_git: bool = True,
) -> dict[str, Any]:
    require_regular_file(archive_path, "release archive")
    with tempfile.TemporaryDirectory(prefix="cloudflare-release-verify-") as temporary:
        destination = Path(temporary)
        with tarfile.open(archive_path, mode="r:") as archive:
            members = safe_archive_members(archive)
            for member in members:
                target = destination / member.name
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                extracted = archive.extractfile(member)
                if extracted is None:
                    fail(f"release archive file cannot be read: {member.name}")
                target.write_bytes(extracted.read())
        base = destination / "cloudflare-release"
        require_directory(base, "release archive root")
        releases = [path for path in base.iterdir() if path.is_dir() and not path.is_symlink()]
        if len(releases) != 1:
            fail("release archive must contain exactly one immutable release directory")
        manifest = verify_release_directory(
            root,
            releases[0],
            expected_source_sha=expected_source_sha,
            expected_repository=expected_repository,
            expected_authority=expected_authority,
            check_git=check_git,
        )
        if releases[0].name != manifest["release_id"]:
            fail("release archive directory name does not match immutable release identifier")
        return manifest


def build_release(
    root: Path,
    *,
    source_sha: str,
    repository: str,
    source_ref: str,
    source_event: str,
    authority: str,
    release_root: Path,
    check_git: bool = True,
) -> tuple[dict[str, Any], Path, Path]:
    source = validate_source_context(
        root,
        source_sha=source_sha,
        repository=repository,
        source_ref=source_ref,
        source_event=source_event,
        authority=authority,
        check_git=check_git,
    )
    frontend = root / FRONTEND_DIST
    worker = root / WORKER_BUILD_DIR
    payload = build_manifest_payload(root, source=source, frontend_directory=frontend, worker_directory=worker)
    manifest = finalized_manifest(payload)
    release_root.mkdir(parents=True, exist_ok=True)
    release_directory = release_root / manifest["release_id"]
    if release_directory.exists():
        fail(f"immutable release already exists: {release_directory}")
    release_directory.mkdir()
    try:
        copy_tree_exact(frontend, release_directory / "frontend")
        copy_worker_runtime(worker, release_directory / "worker")
        copy_tree_exact(root / "migrations" / "d1", release_directory / "migrations" / "d1")
        deployment_config = root / DEPLOYMENT_CONFIG
        deployment_config_target = release_directory / DEPLOYMENT_CONFIG
        deployment_config_target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(deployment_config, deployment_config_target, follow_symlinks=False)
        write_manifest(release_directory / MANIFEST_NAME, manifest)
        verify_release_directory(
            root,
            release_directory,
            expected_source_sha=source_sha,
            expected_repository=repository,
            expected_authority=authority,
            check_git=check_git,
        )
        archive_path = release_root / f"{manifest['release_id']}.tar"
        deterministic_tar(release_directory, archive_path)
        verify_archive(
            root,
            archive_path,
            expected_source_sha=source_sha,
            expected_repository=repository,
            expected_authority=authority,
            check_git=check_git,
        )
    except Exception:
        shutil.rmtree(release_directory, ignore_errors=True)
        archive_candidate = release_root / f"{manifest['release_id']}.tar"
        archive_candidate.unlink(missing_ok=True)
        raise
    return manifest, release_directory, archive_path


def check_repository_policy(root: Path) -> None:
    contracts = contract_inventory(root)
    migrations = migration_inventory(root)
    schema_contract = load_schema_contract(root)
    build = load_toolchain_identity(root)
    if contracts.get("version") != CONTRACT_VERSION:
        fail("generated contract version identity is not canonical v1")
    if migrations.get("latest") is None:
        fail("D1 migration latest identity is missing")
    if schema_contract.get("target_schema_revision") != migrations.get("latest"):
        fail("Catalog schema target must equal the exact current migration revision")
    if build["worker"]["worker_build"] == "latest":
        fail("mutable worker-build identity is prohibited")
    print(
        "Cloudflare D2 release policy passed: "
        f"contracts={contracts['sha256']} migrations={migrations['sha256']} "
        f"schema={schema_contract['target_schema_revision']} rust={build['rust']['channel']} "
        f"node={build['frontend']['node']} worker-build={build['worker']['worker_build']}."
    )


def create_mock_repo(root: Path) -> None:
    (root / "frontend" / "dist" / "assets").mkdir(parents=True)
    (root / "frontend" / "dist" / "index.html").write_text("<html>fixture</html>\n", encoding="utf-8")
    (root / "frontend" / "dist" / "assets" / "app-123.js").write_text("fixture();\n", encoding="utf-8")

    worker = root / "apps" / "control-plane-worker" / "build"
    (worker / "worker").mkdir(parents=True)
    (worker / "worker" / "shim.mjs").write_text(
        "export * from '../index.js';\nexport { default } from '../index.js';\n", encoding="utf-8"
    )
    (worker / "index.js").write_text(
        "import wasm from './index_bg.wasm';\nexport default wasm;\n", encoding="utf-8"
    )
    (worker / "index_bg.wasm").write_bytes(b"\x00asmfixture")
    # These are intentionally excluded from the runtime identity.
    (worker / "package.json").write_text('{"files":[]}\n', encoding="utf-8")
    (worker / "index.d.ts").write_text("export {};\n", encoding="utf-8")

    (root / "openapi" / "v1" / "fragments").mkdir(parents=True)
    (root / "openapi" / "v1" / "openapi.json").write_text('{"openapi":"3.1.0"}\n', encoding="utf-8")
    (root / "openapi" / "v1" / "fragments" / "fixture.json").write_text('{"fixture":true}\n', encoding="utf-8")
    (root / "contracts" / "generated").mkdir(parents=True)
    (root / "contracts" / "generated" / "fixture.openapi.json").write_text('{"fixture":true}\n', encoding="utf-8")
    (root / "frontend" / "src" / "shared" / "api" / "generated").mkdir(parents=True)
    (root / "frontend" / "src" / "shared" / "api" / "generated" / "fixture.ts").write_text(
        "export type Fixture = true;\n", encoding="utf-8"
    )
    shutil.copytree(ROOT / "migrations" / "d1", root / "migrations" / "d1")
    shutil.copytree(ROOT / "migrations" / "resolver-d1", root / "migrations" / "resolver-d1")
    (root / "frontend" / "package.json").write_text(
        json.dumps(
            {
                "packageManager": "npm@11.17.0",
                "engines": {"node": "24.19.0", "npm": "11.17.0"},
            }
        )
        + "\n",
        encoding="utf-8",
    )
    (root / "frontend" / "package-lock.json").write_text('{"lockfileVersion":3}\n', encoding="utf-8")
    (root / "Cargo.lock").write_text("# fixture lock\n", encoding="utf-8")
    (root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.97.1"\n', encoding="utf-8")
    (root / "deploy" / "cloudflare").mkdir(parents=True)
    (root / "deploy" / "cloudflare" / "wrangler.jsonc").write_text(
        json.dumps(
            {
                "main": "../../apps/control-plane-worker/build/worker/shim.mjs",
                "build": {
                    "command": "cargo install worker-build --version 0.8.5 --locked && worker-build --release",
                    "cwd": "../../apps/control-plane-worker",
                },
            }
        )
        + "\n",
        encoding="utf-8",
    )


def expect_rejected(label: str, operation: Any) -> None:
    try:
        operation()
    except (ReleaseError, json.JSONDecodeError, tarfile.TarError):
        return
    fail(f"negative D2 fixture unexpectedly passed: {label}")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="cloudflare-release-self-test-") as temporary:
        root = Path(temporary) / "repo"
        root.mkdir()
        create_mock_repo(root)
        release_root_a = root / ".release-a"
        release_root_b = root / ".release-b"
        source_sha = "a" * 40
        kwargs = dict(
            source_sha=source_sha,
            repository=CANONICAL_REPOSITORY,
            source_ref="refs/pull/1/head",
            source_event="pull_request",
            authority="review-candidate",
            check_git=False,
        )
        first, first_dir, first_archive = build_release(root, release_root=release_root_a, **kwargs)
        second, _second_dir, second_archive = build_release(root, release_root=release_root_b, **kwargs)
        if canonical_document(first) != canonical_document(second):
            fail("identical D2 inputs produced non-deterministic manifests")
        if first_archive.read_bytes() != second_archive.read_bytes():
            fail("identical D2 inputs produced non-deterministic release archives")
        schema_contract = first.get("schema_contract")
        if not isinstance(schema_contract, dict) or not (
            schema_contract.get("database_component") == "catalog"
            and schema_contract.get("target_schema_revision") == "0026_outbound_mail_intents.sql"
            and schema_contract.get("supported_schema_min") == "0026_outbound_mail_intents.sql"
            and schema_contract.get("supported_schema_max") == "0026_outbound_mail_intents.sql"
        ):
            fail("Catalog fixture release did not bind the exact conservative schema contract")

        mutated_frontend = first_dir / "frontend" / "index.html"
        mutated_frontend.write_text("mutated\n", encoding="utf-8")
        expect_rejected(
            "frontend artifact substitution",
            lambda: verify_release_directory(
                root,
                first_dir,
                expected_source_sha=source_sha,
                expected_repository=CANONICAL_REPOSITORY,
                expected_authority="review-candidate",
                check_git=False,
            ),
        )
        mutated_frontend.write_text("<html>fixture</html>\n", encoding="utf-8")

        mutated_worker = first_dir / "worker" / "index_bg.wasm"
        original_worker = mutated_worker.read_bytes()
        mutated_worker.write_bytes(original_worker + b"mutated")
        expect_rejected(
            "Worker artifact substitution",
            lambda: verify_release_directory(root, first_dir, check_git=False),
        )
        mutated_worker.write_bytes(original_worker)
        mutated_worker.unlink()
        expect_rejected(
            "missing Worker artifact",
            lambda: verify_release_directory(root, first_dir, check_git=False),
        )

        expect_rejected(
            "source SHA mismatch",
            lambda: validate_source_context(
                root,
                source_sha="b" * 40,
                repository=CANONICAL_REPOSITORY,
                source_ref="refs/heads/main",
                source_event="push",
                authority="accepted-main",
                check_git=True,
            ),
        )
        expect_rejected(
            "fork repository authority",
            lambda: validate_source_context(
                root,
                source_sha=source_sha,
                repository="example/fork",
                source_ref="refs/pull/1/head",
                source_event="pull_request",
                authority="review-candidate",
                check_git=False,
            ),
        )
        expect_rejected(
            "false accepted-main authority",
            lambda: validate_source_context(
                root,
                source_sha=source_sha,
                repository=CANONICAL_REPOSITORY,
                source_ref="refs/pull/1/head",
                source_event="pull_request",
                authority="accepted-main",
                check_git=False,
            ),
        )
        expect_rejected(
            "secret-bearing manifest",
            lambda: validate_manifest_has_no_sensitive_authority({"secret_access_key": "fixture"}),
        )

        malicious = root / "malicious.tar"
        with tarfile.open(malicious, "w") as archive:
            info = tarfile.TarInfo("../escape")
            data = b"x"
            info.size = len(data)
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))
        expect_rejected(
            "archive path traversal",
            lambda: verify_archive(
                root,
                malicious,
                expected_source_sha=None,
                expected_repository=None,
                expected_authority=None,
                check_git=False,
            ),
        )

        clean_dir = release_root_b / second["release_id"]
        (root / "migrations" / "d1" / "0001_initial.sql").write_text("SELECT 1;\n", encoding="utf-8")
        expect_rejected(
            "migration-set substitution",
            lambda: verify_release_directory(root, clean_dir, check_git=False),
        )

    print("Cloudflare D2 deterministic and negative provenance self-tests passed.")


def write_github_output(manifest: dict[str, Any], archive: Path) -> None:
    target = os.environ.get("GITHUB_OUTPUT")
    if not target:
        fail("--github-output requires the GITHUB_OUTPUT environment variable")
    with Path(target).open("a", encoding="utf-8") as handle:
        handle.write(f"release_id={manifest['release_id']}\n")
        handle.write(f"archive={archive.as_posix()}\n")
        handle.write(f"archive_sha256={sha256_file(archive)}\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("check")
    subparsers.add_parser("self-test")

    build = subparsers.add_parser("build")
    build.add_argument("--source-sha", required=True)
    build.add_argument("--repository", required=True)
    build.add_argument("--source-ref", required=True)
    build.add_argument("--source-event", required=True)
    build.add_argument("--authority", choices=("review-candidate", "accepted-main"), required=True)
    build.add_argument("--github-output", action="store_true")

    verify = subparsers.add_parser("verify-archive")
    verify.add_argument("--archive", type=Path, required=True)
    verify.add_argument("--expected-source-sha", required=True)
    verify.add_argument("--repository", required=True)
    verify.add_argument("--authority", choices=("review-candidate", "accepted-main"), required=True)

    args = parser.parse_args()
    if args.command == "check":
        check_repository_policy(ROOT)
        return 0
    if args.command == "self-test":
        self_test()
        return 0
    if args.command == "build":
        manifest, release_directory, archive = build_release(
            ROOT,
            source_sha=args.source_sha,
            repository=args.repository,
            source_ref=args.source_ref,
            source_event=args.source_event,
            authority=args.authority,
            release_root=RELEASE_DIR,
        )
        if args.github_output:
            write_github_output(manifest, archive)
        print(
            f"Built immutable Cloudflare release {manifest['release_id']} from {args.source_sha}: "
            f"directory={release_directory.relative_to(ROOT)} archive={archive.relative_to(ROOT)} "
            f"archive_sha256={sha256_file(archive)}"
        )
        return 0
    if args.command == "verify-archive":
        manifest = verify_archive(
            ROOT,
            args.archive,
            expected_source_sha=args.expected_source_sha,
            expected_repository=args.repository,
            expected_authority=args.authority,
        )
        print(f"Verified immutable Cloudflare release archive {manifest['release_id']}.")
        return 0
    fail(f"unsupported command: {args.command}")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ReleaseError as error:
        raise SystemExit(f"Cloudflare D2 release provenance rejected: {error}") from error
