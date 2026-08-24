#!/usr/bin/env python3
"""Fail closed if native read-only opsctl or its pure core gains forbidden capability."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SRC = Path("tools/opsctl/src")
CORE_SRC = Path("tools/opsctl/core/src")
CARGO = Path("tools/opsctl/Cargo.toml")
CORE_CARGO = Path("tools/opsctl/core/Cargo.toml")
LOCK = Path("tools/opsctl/Cargo.lock")
HELP = Path("tools/opsctl/src/help.txt")
WORKSPACE_CARGO = Path("Cargo.toml")
REQUIRED_SOURCE_FILES = {
    "architecture.rs",
    "canonical.rs",
    "cli.rs",
    "credentials/mod.rs",
    "d1.rs",
    "d1/authority.rs",
    "d1/catalog.rs",
    "d1/compatibility.rs",
    "d1/model.rs",
    "d1/plan.rs",
    "d1/status.rs",
    "d1/util.rs",
    "d1/verify.rs",
    "doctor.rs",
    "lib.rs",
    "main.rs",
    "repository.rs",
    "status.rs",
}
REQUIRED_CORE_SOURCE_FILES = {
    "architecture.rs",
    "lib.rs",
    "release.rs",
}
FORBIDDEN_DEPENDENCY_CAPABILITIES = {
    "cloudflare",
    "hyper",
    "reqwest",
    "tokio",
    "ureq",
    "worker",
}
FORBIDDEN_CORE_DEPENDENCIES = FORBIDDEN_DEPENDENCY_CAPABILITIES | {
    "serde_json",
    "serde_json_canonicalizer",
    "sha2",
}
FORBIDDEN_PRODUCT_DEPENDENCIES = {"opsctl", "opsctl-core"}
FORBIDDEN_RUNTIME_MARKERS = (
    "Command::new(",
    "std::process::Command",
    "reqwest::",
    "ureq::",
    "worker::",
    "cloudflare::",
    "std::net::",
    "TcpStream",
    "R2Credentials",
    "D1Database",
    "secret_access_key",
    "api_token",
)
FORBIDDEN_MUTATION_MARKERS = (
    "fs::write(",
    "fs::remove_file(",
    "fs::remove_dir(",
    "fs::remove_dir_all(",
    "fs::rename(",
    "fs::copy(",
    "fs::create_dir(",
    "fs::create_dir_all(",
    "File::create(",
    "OpenOptions",
    "std::io::Write",
    ".write_all(",
    "env::set_var(",
    "env::remove_var(",
)
FORBIDDEN_CORE_SOURCE_PATTERNS = (
    (r"\bserde_json\s*::", "serde_json representation"),
    (r"\bstd\s*::\s*(?:fs|process|net|env|path)\b", "filesystem/process/network/environment/path effect"),
    (r"\b(?:Path|PathBuf)\b", "OS path type"),
    (r"\bCommand\s*::\s*new\s*\(", "process execution"),
    (r"\b(?:reqwest|ureq|worker|cloudflare)\s*::", "network/provider SDK"),
    (r"\b(?:SystemTime|Instant)\b", "hidden clock observation"),
)
REQUIRED_D1_MARKERS = (
    '"D1_REPOSITORY_PROJECTION"',
    '"tools/opsctl/src/d1"',
    '"migrations/d1"',
    '"migrations/resolver-d1"',
    '"0026_outbound_mail_intents.sql"',
    '"0004_refresh_owner_hmac_version.sql"',
    '"4d1d8b8d3bba5d0903385d05fc18e0036628ff1123e0e26e9a080a340f7b5e2e"',
    '"98fd6f91a839223b06c441df4901dbd4fda8e69f2f90606f00e43faad91877ec"',
    '"UNKNOWN_FAIL_CLOSED"',
    '"EXACT"',
    '"BEHIND_KNOWN_PREFIX"',
    '"DIVERGED"',
    '"UNKNOWN_MIGRATION"',
    '"CORRUPT_LEDGER"',
    '"FAIL_FORWARD_REQUIRED"',
    '"CONTRACT_BLOCKED"',
)


class GateError(ValueError):
    pass


def fail(message: str) -> None:
    raise GateError(message)


def read(root: Path, relative: Path) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        fail(f"required opsctl file is missing/not regular: {relative}")
    return path.read_text(encoding="utf-8")


def production(text: str) -> str:
    return text.split("#[cfg(test)]", 1)[0]


def collect_rust_sources(root: Path, source_root_relative: Path, required: set[str], label: str) -> dict[str, str]:
    source_root = root / source_root_relative
    if source_root.is_symlink() or not source_root.is_dir():
        fail(f"{label} source root is missing/not regular: {source_root_relative}")
    observed = {
        path.relative_to(source_root).as_posix(): path.read_text(encoding="utf-8")
        for path in sorted(source_root.rglob("*.rs"))
        if path.is_file() and not path.is_symlink()
    }
    missing = sorted(required - set(observed))
    if missing:
        fail(f"{label} source layout is incomplete: missing={missing}")
    return observed


def rust_sources(root: Path) -> dict[str, str]:
    return collect_rust_sources(root, SRC, REQUIRED_SOURCE_FILES, "modular opsctl")


def core_rust_sources(root: Path) -> dict[str, str]:
    return collect_rust_sources(root, CORE_SRC, REQUIRED_CORE_SOURCE_FILES, "opsctl-core")


def exact_registry_version(name: str, spec: object, owner: str = "opsctl") -> str | None:
    if isinstance(spec, str):
        version = spec
    elif isinstance(spec, dict):
        if "git" in spec:
            fail(f"{owner} dependency {name!r} must not use a Git source")
        if "path" in spec:
            return None
        version = spec.get("version")
    else:
        fail(f"{owner} dependency {name!r} has unsupported Cargo declaration")

    if not isinstance(version, str) or not version.startswith("=") or len(version) == 1:
        fail(f"{owner} registry dependency {name!r} must use an exact =version pin")
    return version[1:]


def lock_has_exact_package(lock: str, package_name: str, version: str) -> bool:
    return f'name = "{package_name}"\nversion = "{version}"' in lock


def validate_dependencies(cargo: str, lock: str) -> None:
    try:
        manifest = tomllib.loads(cargo)
    except tomllib.TOMLDecodeError as error:
        raise GateError(f"cannot parse opsctl Cargo.toml: {error}") from error

    dependencies = manifest.get("dependencies", {})
    if not isinstance(dependencies, dict):
        fail("opsctl [dependencies] must be a TOML table")

    for name, spec in dependencies.items():
        if name.lower() in FORBIDDEN_DEPENDENCY_CAPABILITIES:
            fail(f"opsctl dependency grants forbidden runtime/provider capability: {name}")

        if isinstance(spec, dict) and "path" in spec:
            path = spec.get("path")
            if name != "opsctl-core" or path != "core":
                fail(
                    "opsctl local dependency must preserve the approved shell -> opsctl-core "
                    f"direction; observed {name} path={path!r}"
                )
            if "git" in spec or "registry" in spec:
                fail("opsctl-core local dependency must not combine path with Git/registry source")
            if 'name = "opsctl-core"' not in lock:
                fail("opsctl lockfile lost local opsctl-core identity")
            continue

        version = exact_registry_version(name, spec)
        if version is None:
            fail(f"opsctl dependency {name!r} has an unsupported local dependency shape")
        package_name = spec.get("package", name) if isinstance(spec, dict) else name
        if not isinstance(package_name, str):
            fail(f"opsctl dependency {name!r} has invalid package identity")
        if not lock_has_exact_package(lock, package_name, version):
            fail(f"opsctl lockfile lost exact registry identity for {package_name} {version}")


def dependency_package_name(alias: str, spec: object) -> str:
    if isinstance(spec, dict):
        package = spec.get("package", alias)
        if not isinstance(package, str):
            fail(f"dependency {alias!r} has invalid package identity")
        return package
    return alias


def validate_core_dependencies(cargo: str, lock: str, root: Path = ROOT) -> None:
    try:
        manifest = tomllib.loads(cargo)
    except tomllib.TOMLDecodeError as error:
        raise GateError(f"cannot parse opsctl-core Cargo.toml: {error}") from error

    build_dependencies = manifest.get("build-dependencies", {})
    if build_dependencies:
        fail("opsctl-core must not gain build-time dependency/effect authority")

    for table_name in ("dependencies", "dev-dependencies"):
        dependencies = manifest.get(table_name, {})
        if not isinstance(dependencies, dict):
            fail(f"opsctl-core [{table_name}] must be a TOML table")
        for alias, spec in dependencies.items():
            package_name = dependency_package_name(alias, spec)
            if alias.lower() in FORBIDDEN_CORE_DEPENDENCIES or package_name.lower() in FORBIDDEN_CORE_DEPENDENCIES:
                fail(f"opsctl-core dependency crosses representation/effect boundary: {package_name}")
            if isinstance(spec, dict) and "path" in spec:
                if "git" in spec or "registry" in spec:
                    fail("opsctl-core local dependency must not combine path with Git/registry source")
                dependency_path = spec.get("path")
                if not isinstance(dependency_path, str):
                    fail(f"opsctl-core dependency {alias!r} has invalid local path")
                resolved = (root / CORE_CARGO.parent / dependency_path).resolve()
                for product_root in (root / "apps", root / "crates"):
                    if resolved.is_relative_to(product_root.resolve()):
                        fail(
                            "opsctl-core must not depend on Product Runtime/application crates: "
                            f"{alias} -> {dependency_path}"
                        )
                continue
            version = exact_registry_version(alias, spec, "opsctl-core")
            if version is None:
                fail(f"opsctl-core dependency {alias!r} has unsupported local dependency shape")
            if not lock_has_exact_package(lock, package_name, version):
                fail(
                    f"opsctl lockfile lost exact opsctl-core dependency identity for "
                    f"{package_name} {version}"
                )


def validate_core_source(sources: dict[str, str], cargo: str, lock: str, root: Path = ROOT) -> None:
    lib = sources["lib.rs"]
    if "#![forbid(unsafe_code)]" not in lib:
        fail("opsctl-core must forbid unsafe code")
    source = "\n".join(sources.values())
    for pattern, label in FORBIDDEN_CORE_SOURCE_PATTERNS:
        if re.search(pattern, source):
            fail(f"opsctl-core pure boundary contains forbidden {label}: /{pattern}/")
    validate_core_dependencies(cargo, lock, root)


def validate_manifest_no_opsctl_dependency(cargo: str, label: str) -> None:
    try:
        manifest = tomllib.loads(cargo)
    except tomllib.TOMLDecodeError as error:
        raise GateError(f"cannot parse {label}: {error}") from error

    def walk(value: object, path: tuple[str, ...] = ()) -> None:
        if not isinstance(value, dict):
            return
        for key, child in value.items():
            next_path = (*path, str(key))
            if key in {"dependencies", "dev-dependencies", "build-dependencies"}:
                if not isinstance(child, dict):
                    fail(f"{label} dependency table {'.'.join(next_path)} must be a table")
                for alias, spec in child.items():
                    package_name = dependency_package_name(alias, spec)
                    if alias in FORBIDDEN_PRODUCT_DEPENDENCIES or package_name in FORBIDDEN_PRODUCT_DEPENDENCIES:
                        fail(
                            f"Product Runtime/application dependency on {package_name} is forbidden "
                            f"in {label} ({'.'.join(next_path)})"
                        )
            walk(child, next_path)

    walk(manifest)


def validate_product_dependency_direction(root: Path) -> None:
    workspace_text = read(root, WORKSPACE_CARGO)
    validate_manifest_no_opsctl_dependency(workspace_text, str(WORKSPACE_CARGO))
    try:
        workspace = tomllib.loads(workspace_text)
    except tomllib.TOMLDecodeError as error:
        raise GateError(f"cannot parse workspace Cargo.toml: {error}") from error
    members = workspace.get("workspace", {}).get("members", [])
    if not isinstance(members, list) or not all(isinstance(member, str) for member in members):
        fail("root Cargo workspace members must be an explicit string list")
    for member in members:
        manifest_path = Path(member) / "Cargo.toml"
        validate_manifest_no_opsctl_dependency(read(root, manifest_path), str(manifest_path))


def validate_source(sources: dict[str, str], cargo: str, lock: str) -> None:
    production_source = "\n".join(
        "" if path.endswith("tests.rs") else production(text)
        for path, text in sources.items()
    )
    d1_source = "\n".join(
        production(text)
        for path, text in sources.items()
        if path == "d1.rs" or path.startswith("d1/")
    )
    cli = sources["cli.rs"]
    cli_production = production(cli)
    help_text = read(ROOT, HELP)
    lib = sources["lib.rs"]
    main = sources["main.rs"]

    if "#![forbid(unsafe_code)]" not in lib:
        fail("opsctl must forbid unsafe code")
    for marker in ("opsctl::parse_invocation", "opsctl::execute", "error.json()"):
        if marker not in main:
            fail(f"opsctl main.rs lost thin-entrypoint marker: {marker}")
    for marker in FORBIDDEN_RUNTIME_MARKERS + FORBIDDEN_MUTATION_MARKERS:
        if marker in production_source:
            fail(f"opsctl read-only boundary contains forbidden capability: {marker}")
    for marker in REQUIRED_D1_MARKERS:
        if marker not in d1_source:
            fail(f"typed D1 catalog lost required marker: {marker}")
    for marker in ("d1 repository", "d1 status", "d1 plan", "d1 compatibility", "d1 verify"):
        if marker not in help_text:
            fail(f"opsctl help lost required D1 command: {marker}")
    removed_flag = "--" + "authority"
    removed_field = "authority" + "_path"
    removed_default = "DEFAULT_" + "AUTHORITY"
    if removed_flag in cli_production or removed_field in d1_source or removed_default in d1_source:
        fail("opsctl retains the removed D1 authority override/loader surface")
    if ("D1_EVOLUTION_" + "AUTHORITY") in d1_source:
        fail("opsctl retains the removed serialized D1 policy format")
    validate_dependencies(cargo, lock)
    if "[dev-dependencies]" in cargo or "[build-dependencies]" in cargo:
        fail("opsctl must not add dev/build dependency authority")


def load_d1_projection(root: Path) -> dict[str, Any]:
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--locked",
            "--quiet",
            "--manifest-path",
            str(ROOT / CARGO),
            "--",
            "--root",
            str(root),
            "d1",
            "repository",
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        fail(f"typed D1 repository projection failed: {detail}")
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise GateError(f"typed D1 repository projection is malformed: {error}") from error
    if not isinstance(payload, dict):
        fail("typed D1 repository projection must be one JSON object")
    return payload


def validate_d1_projection(payload: dict[str, Any]) -> None:
    if payload.get("schema_version") != 1 or payload.get("kind") != "D1_REPOSITORY_PROJECTION":
        fail("typed D1 repository projection identity/version drifted")
    if payload.get("semantic_authority") != "tools/opsctl/src/d1":
        fail("typed D1 semantic authority drifted")
    if payload.get("executable_schema_authority") != [
        "migrations/d1",
        "migrations/resolver-d1",
    ]:
        fail("D1 executable schema authority drifted")
    if payload.get("production_mutation") is not False:
        fail("typed D1 projection must remain non-production-mutating")
    components = payload.get("components")
    if not isinstance(components, list):
        fail("typed D1 projection components are missing")
    by_id = {
        component.get("component_id"): component
        for component in components
        if isinstance(component, dict)
    }
    if set(by_id) != {"catalog", "resolver"}:
        fail("typed D1 projection must contain exactly Catalog and Resolver")
    for component_id, component in by_id.items():
        contract = component.get("release_schema_contract")
        if not isinstance(contract, dict) or contract.get("database_component") != component_id:
            fail(f"typed D1 {component_id} release contract is missing or mismatched")


def validate(root: Path = ROOT) -> None:
    lock = read(root, LOCK)
    validate_source(rust_sources(root), read(root, CARGO), lock)
    validate_core_source(core_rust_sources(root), read(root, CORE_CARGO), lock, root)
    validate_product_dependency_direction(root)
    validate_d1_projection(load_d1_projection(root))


def expect_source_rejected(label: str, sources: dict[str, str], cargo: str, lock: str) -> None:
    try:
        validate_source(sources, cargo, lock)
    except GateError:
        return
    fail(f"{label} negative fixture unexpectedly passed")


def expect_core_rejected(
    label: str,
    sources: dict[str, str],
    cargo: str,
    lock: str,
) -> None:
    try:
        validate_core_source(sources, cargo, lock)
    except GateError:
        return
    fail(f"{label} opsctl-core negative fixture unexpectedly passed")


def expect_product_manifest_rejected(label: str, cargo: str) -> None:
    try:
        validate_manifest_no_opsctl_dependency(cargo, label)
    except GateError:
        return
    fail(f"{label} Product Runtime dependency negative fixture unexpectedly passed")


def expect_projection_rejected(label: str, root: Path) -> None:
    try:
        load_d1_projection(root)
    except GateError:
        return
    fail(f"{label} D1 repository fixture unexpectedly passed")


def self_test() -> None:
    sources = rust_sources(ROOT)
    core_sources = core_rust_sources(ROOT)
    cargo = read(ROOT, CARGO)
    core_cargo = read(ROOT, CORE_CARGO)
    lock = read(ROOT, LOCK)
    validate_source(sources, cargo, lock)
    validate_core_source(core_sources, core_cargo, lock)
    validate_product_dependency_direction(ROOT)
    validate_d1_projection(load_d1_projection(ROOT))

    process_sources = dict(sources)
    process_sources["d1.rs"] += '\nfn forbidden() { let _ = Command::new("wrangler"); }\n'
    expect_source_rejected("D1 process authority", process_sources, cargo, lock)
    mutation_sources = dict(sources)
    mutation_sources["d1.rs"] += '\nfn forbidden() { let _ = fs::write("state", b"x"); }\n'
    expect_source_rejected("D1 filesystem mutation", mutation_sources, cargo, lock)

    forbidden_dependency = cargo.replace(
        'serde_json = "=1.0.151"',
        'serde_json = "=1.0.151"\nreqwest = "=0.13.1"',
        1,
    )
    expect_source_rejected("forbidden capability dependency", sources, forbidden_dependency, lock)
    floating_dependency = cargo.replace(
        'serde_json = "=1.0.151"',
        'serde_json = "1.0.151"',
        1,
    )
    expect_source_rejected("floating registry dependency", sources, floating_dependency, lock)
    wrong_local_boundary = cargo.replace(
        'opsctl-core = { path = "core" }',
        'opsctl-core = { path = "../../crates/runtime" }',
        1,
    )
    expect_source_rejected("wrong local dependency direction", sources, wrong_local_boundary, lock)

    core_representation = dict(core_sources)
    core_representation["release.rs"] += "\nfn forbidden(value: serde_json::Value) { let _ = value; }\n"
    expect_core_rejected("serde_json representation", core_representation, core_cargo, lock)
    core_effect = dict(core_sources)
    core_effect["release.rs"] += '\nfn forbidden() { let _ = std::fs::read("state"); }\n'
    expect_core_rejected("filesystem effect", core_effect, core_cargo, lock)
    core_dependency = core_cargo.replace(
        "[dependencies]\n",
        '[dependencies]\nserde_json = "=1.0.151"\n',
        1,
    )
    expect_core_rejected("representation dependency", core_sources, core_dependency, lock)
    expect_product_manifest_rejected(
        "synthetic product manifest",
        '[package]\nname = "synthetic-runtime"\nversion = "0.0.0"\n\n[dependencies]\nopsctl-core = { path = "../../tools/opsctl/core" }\n',
    )

    with tempfile.TemporaryDirectory(prefix="opsctl-d1-negative-") as temporary:
        fixture = Path(temporary)
        shutil.copytree(ROOT / "migrations" / "d1", fixture / "migrations" / "d1")
        shutil.copytree(ROOT / "migrations" / "resolver-d1", fixture / "migrations" / "resolver-d1")
        migration = fixture / "migrations" / "d1" / "0001_catalog.sql"
        migration.write_bytes(migration.read_bytes() + b"\n-- tampered\n")
        expect_projection_rejected("historical SQL substitution", fixture)

    print(
        "opsctl shell/core purity, Product Runtime dependency direction, typed D1 catalog, "
        "historical-anchor and dependency negative fixtures passed."
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        validate()
        print(
            "opsctl remains native, read-only and provider-free; opsctl-core stays pure and "
            "Product Runtime remains independent; D1 history is derived from canonical SQL "
            "under compact typed historical anchors."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except GateError as error:
        print(f"opsctl gate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
