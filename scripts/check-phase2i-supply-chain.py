#!/usr/bin/env python3
"""Repository-local Phase 2I dependency, license and supply-chain source policy."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import tomllib
from pathlib import Path

WORKFLOW_ROOT = Path(".github/workflows")
CARGO_ROOT = Path("Cargo.toml")
CARGO_LOCK = Path("Cargo.lock")
FRONTEND_PACKAGE = Path("frontend/package.json")
FRONTEND_LOCK = Path("frontend/package-lock.json")
FRONTEND_NODE_MODULES = Path("frontend/node_modules")

DEPENDENCY_SECTIONS = {"dependencies", "dev-dependencies", "build-dependencies"}
PINNED_ACTION = re.compile(r"^([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)@([0-9a-f]{40})$")
ACTION_LINE = re.compile(r"^\s*-?\s*uses:\s*([^\s#]+)", re.MULTILINE)
EXACT_SEMVER = re.compile(r"^=?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")
UNSAFE_VERSION_TOKENS = ("*", "^", "~", ">", "<", "||", " ")
ALLOWED_REGISTRY_PREFIXES = (
    "registry+https://github.com/rust-lang/crates.io-index",
    "registry+https://index.crates.io/",
)
DISALLOWED_LICENSE = re.compile(
    r"(?:\b(?:AGPL|GPL|LGPL|SSPL|BUSL)(?:[-.0-9]|\b)|COMMONS[ -]CLAUSE)",
    re.IGNORECASE,
)


def workflow_errors(root: Path) -> list[str]:
    errors: list[str] = []
    workflows = sorted((root / WORKFLOW_ROOT).glob("*.y*ml"))
    if not workflows:
        return ["no GitHub Actions workflows found"]
    for workflow in workflows:
        text = workflow.read_text(encoding="utf-8")
        for value in ACTION_LINE.findall(text):
            if value.startswith("./"):
                continue
            if PINNED_ACTION.fullmatch(value) is None:
                errors.append(f"workflow action is not pinned to a full commit SHA: {workflow}:{value}")
    return errors


def dependency_tables(document: dict[str, object]) -> list[tuple[str, dict[str, object]]]:
    tables: list[tuple[str, dict[str, object]]] = []
    for section in DEPENDENCY_SECTIONS:
        value = document.get(section)
        if isinstance(value, dict):
            tables.append((section, value))
    target = document.get("target")
    if isinstance(target, dict):
        for target_name, target_value in target.items():
            if not isinstance(target_value, dict):
                continue
            for section in DEPENDENCY_SECTIONS:
                value = target_value.get(section)
                if isinstance(value, dict):
                    tables.append((f"target.{target_name}.{section}", value))
    return tables


def cargo_manifest_errors(root: Path) -> list[str]:
    errors: list[str] = []
    manifests = [
        root / CARGO_ROOT,
        *sorted((root / "apps").glob("*/Cargo.toml")),
        *sorted((root / "crates").glob("*/Cargo.toml")),
    ]
    for manifest in manifests:
        if not manifest.is_file():
            errors.append(f"missing Cargo manifest: {manifest.relative_to(root)}")
            continue
        with manifest.open("rb") as handle:
            document = tomllib.load(handle)
        for section, dependencies in dependency_tables(document):
            for name, spec in dependencies.items():
                label = f"{manifest.relative_to(root)}:{section}:{name}"
                if isinstance(spec, str):
                    if any(token in spec for token in UNSAFE_VERSION_TOKENS) or not EXACT_SEMVER.fullmatch(spec):
                        errors.append(f"dependency version is not exact: {label}={spec}")
                    continue
                if not isinstance(spec, dict):
                    errors.append(f"dependency specification has unsupported shape: {label}")
                    continue
                if "git" in spec:
                    errors.append(f"git dependency is forbidden in release-candidate source: {label}")
                if "path" in spec or spec.get("workspace") is True:
                    continue
                version = spec.get("version")
                if not isinstance(version, str) or any(token in version for token in UNSAFE_VERSION_TOKENS) or not EXACT_SEMVER.fullmatch(version):
                    errors.append(f"external dependency version is not exact: {label}={version!r}")
    return errors


def cargo_lock_errors(root: Path) -> list[str]:
    lock_path = root / CARGO_LOCK
    if not lock_path.is_file():
        return ["Cargo.lock is required"]
    with lock_path.open("rb") as handle:
        document = tomllib.load(handle)
    errors: list[str] = []
    packages = document.get("package")
    if not isinstance(packages, list) or not packages:
        return ["Cargo.lock contains no packages"]
    for package in packages:
        if not isinstance(package, dict):
            errors.append("Cargo.lock package entry has invalid shape")
            continue
        source = package.get("source")
        if source is None:
            continue
        if not isinstance(source, str) or not source.startswith(ALLOWED_REGISTRY_PREFIXES):
            errors.append(f"Cargo.lock contains non-crates.io source: {package.get('name')}:{source}")
        checksum = package.get("checksum")
        if not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
            errors.append(f"Cargo.lock registry package lacks a SHA-256 checksum: {package.get('name')}")
    return errors


def frontend_errors(root: Path) -> list[str]:
    package_path = root / FRONTEND_PACKAGE
    lock_path = root / FRONTEND_LOCK
    if not package_path.is_file() or not lock_path.is_file():
        return ["frontend package.json and package-lock.json are required"]
    package = json.loads(package_path.read_text(encoding="utf-8"))
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    errors: list[str] = []
    if package.get("packageManager") != "npm@11.17.0":
        errors.append("frontend npm version is not exact")
    for section in ("dependencies", "devDependencies"):
        dependencies = package.get(section, {})
        if not isinstance(dependencies, dict):
            errors.append(f"frontend {section} must be an object")
            continue
        for name, version in dependencies.items():
            if not isinstance(version, str) or not EXACT_SEMVER.fullmatch(version):
                errors.append(f"frontend dependency is not exact: {section}:{name}={version!r}")
    if lock.get("lockfileVersion") != 3:
        errors.append("frontend package-lock must remain lockfileVersion 3")
    packages = lock.get("packages")
    if not isinstance(packages, dict):
        return errors + ["frontend package-lock packages map is missing"]
    for path, entry in packages.items():
        if path == "" or not isinstance(entry, dict):
            continue
        resolved = entry.get("resolved")
        integrity = entry.get("integrity")
        if resolved is not None:
            if not isinstance(resolved, str) or not resolved.startswith("https://registry.npmjs.org/"):
                errors.append(f"frontend lock contains non-registry source: {path}")
            if not isinstance(integrity, str) or not integrity.startswith(("sha512-", "sha256-")):
                errors.append(f"frontend lock registry package lacks integrity: {path}")
    return errors


def cargo_license_errors(root: Path) -> tuple[list[str], int]:
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        return [f"cargo license metadata could not be resolved: {detail}"], 0
    try:
        metadata = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        return [f"cargo metadata returned invalid JSON: {error}"], 0
    packages = metadata.get("packages")
    if not isinstance(packages, list):
        return ["cargo metadata packages list is missing"], 0
    errors: list[str] = []
    checked = 0
    for package in packages:
        if not isinstance(package, dict) or package.get("source") is None:
            continue
        checked += 1
        name = package.get("name", "unknown")
        license_expression = package.get("license")
        license_file = package.get("license_file")
        if not isinstance(license_expression, str) or not license_expression.strip():
            if not isinstance(license_file, str) or not license_file.strip():
                errors.append(f"Cargo dependency has no declared license metadata: {name}")
            continue
        if DISALLOWED_LICENSE.search(license_expression):
            errors.append(f"Cargo dependency license requires explicit review: {name}:{license_expression}")
    if checked == 0:
        errors.append("cargo license scan did not inspect any external dependencies")
    return errors, checked


def is_node_package_root(path: Path) -> bool:
    parts = path.parts
    node_indices = [index for index, part in enumerate(parts) if part == "node_modules"]
    if not node_indices:
        return False
    tail = parts[node_indices[-1] + 1 :]
    if not tail or tail[-1] != "package.json":
        return False
    return len(tail) == 2 or (len(tail) == 3 and tail[0].startswith("@"))


def node_license_errors(root: Path) -> tuple[list[str], int]:
    node_modules = root / FRONTEND_NODE_MODULES
    if not node_modules.is_dir():
        return ["frontend/node_modules is required for installed npm license scan"], 0
    errors: list[str] = []
    checked = 0
    seen: set[tuple[str, str]] = set()
    for package_json in sorted(node_modules.rglob("package.json")):
        if not is_node_package_root(package_json):
            continue
        try:
            package = json.loads(package_json.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            errors.append(f"installed npm package metadata is unreadable: {package_json}:{error}")
            continue
        name = package.get("name")
        version = package.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            errors.append(f"installed npm package lacks name/version metadata: {package_json}")
            continue
        identity = (name, version)
        if identity in seen:
            continue
        seen.add(identity)
        checked += 1
        license_expression = package.get("license")
        if not isinstance(license_expression, str) or not license_expression.strip():
            errors.append(f"npm dependency has no declared license metadata: {name}@{version}")
            continue
        if DISALLOWED_LICENSE.search(license_expression):
            errors.append(f"npm dependency license requires explicit review: {name}@{version}:{license_expression}")
    if checked == 0:
        errors.append("npm license scan did not inspect any installed dependencies")
    return errors, checked


def validate(root: Path) -> list[str]:
    return workflow_errors(root) + cargo_manifest_errors(root) + cargo_lock_errors(root) + frontend_errors(root)


def self_test() -> None:
    if PINNED_ACTION.fullmatch("actions/checkout@main"):
        raise ValueError("unpinned action fixture unexpectedly passed")
    for version in ("^1.2.3", "*", ">=1.2.3", "1.2"):
        if EXACT_SEMVER.fullmatch(version) and not any(token in version for token in UNSAFE_VERSION_TOKENS):
            raise ValueError(f"floating dependency fixture unexpectedly passed: {version}")
    unsafe_sources = (
        "git+https://example.invalid/repository",
        "registry+https://example.invalid/index",
    )
    if any(source.startswith(ALLOWED_REGISTRY_PREFIXES) for source in unsafe_sources):
        raise ValueError("unsafe Cargo source fixture unexpectedly passed")
    for expression in ("GPL-3.0-only", "AGPL-3.0-or-later", "LGPL-2.1-only", "BUSL-1.1"):
        if DISALLOWED_LICENSE.search(expression) is None:
            raise ValueError(f"restricted license fixture unexpectedly passed: {expression}")
    for expression in ("MIT", "Apache-2.0", "MIT OR Apache-2.0", "BSD-3-Clause", "MPL-2.0"):
        if DISALLOWED_LICENSE.search(expression) is not None:
            raise ValueError(f"permissive/weak-copyleft license fixture unexpectedly failed: {expression}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--cargo-licenses", action="store_true")
    parser.add_argument("--node-licenses", action="store_true")
    args = parser.parse_args()

    errors = validate(args.root)
    cargo_checked = 0
    node_checked = 0
    if args.cargo_licenses:
        license_errors, cargo_checked = cargo_license_errors(args.root)
        errors.extend(license_errors)
    if args.node_licenses:
        license_errors, node_checked = node_license_errors(args.root)
        errors.extend(license_errors)
    if errors:
        for error in errors:
            print(error)
        return 1
    if args.self_test:
        try:
            self_test()
        except ValueError as error:
            print(error)
            return 1
        print("Phase 2I supply-chain and license negative fixtures rejected as expected.")
        return 0
    if args.cargo_licenses or args.node_licenses:
        print(f"Phase 2I installed dependency license scan passed: cargo={cargo_checked} npm={node_checked}.")
    else:
        print("Phase 2I dependency and supply-chain source policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
