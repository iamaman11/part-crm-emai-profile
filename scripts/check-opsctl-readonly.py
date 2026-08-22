#!/usr/bin/env python3
"""Fail closed if native read-only opsctl or its typed D1 catalog gains capability."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SRC = Path("tools/opsctl/src")
CARGO = Path("tools/opsctl/Cargo.toml")
LOCK = Path("tools/opsctl/Cargo.lock")
HELP = Path("tools/opsctl/src/help.txt")
ALLOWED_DEPENDENCIES = {"serde_json": "=1.0.151"}
REQUIRED_SOURCE_FILES = {
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
    "inventory.rs",
    "lib.rs",
    "main.rs",
    "repository.rs",
    "status.rs",
}
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


def rust_sources(root: Path) -> dict[str, str]:
    source_root = root / SRC
    if source_root.is_symlink() or not source_root.is_dir():
        fail(f"opsctl source root is missing/not regular: {SRC}")
    observed = {
        path.relative_to(source_root).as_posix(): path.read_text(encoding="utf-8")
        for path in sorted(source_root.rglob("*.rs"))
        if path.is_file() and not path.is_symlink()
    }
    missing = sorted(REQUIRED_SOURCE_FILES - set(observed))
    if missing:
        fail(f"modular opsctl source layout is incomplete: missing={missing}")
    return observed


def parse_dependencies(cargo: str) -> dict[str, str]:
    match = re.search(r"(?ms)^\[dependencies\]\n(?P<body>.*?)(?=^\[|\Z)", cargo)
    if match is None:
        return {}
    dependencies: dict[str, str] = {}
    for raw in match.group("body").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        item = re.fullmatch(r'([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"', line)
        if item is None:
            fail(f"opsctl dependency declaration must be exact-pinned: {line}")
        dependencies[item.group(1)] = item.group(2)
    return dependencies


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
    if len(main.encode("utf-8")) > 1024:
        fail("opsctl main.rs must remain a thin adapter")
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
    if parse_dependencies(cargo) != ALLOWED_DEPENDENCIES:
        fail(f"opsctl dependency set drifted: {parse_dependencies(cargo)}")
    if "[dev-dependencies]" in cargo or "[build-dependencies]" in cargo:
        fail("opsctl must not add dev/build dependency authority")
    if 'name = "serde_json"\nversion = "1.0.151"' not in lock:
        fail("opsctl lockfile lost exact serde_json identity")


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
    validate_source(rust_sources(root), read(root, CARGO), read(root, LOCK))
    validate_d1_projection(load_d1_projection(root))


def expect_source_rejected(
    label: str,
    sources: dict[str, str],
    cargo: str,
    lock: str,
) -> None:
    try:
        validate_source(sources, cargo, lock)
    except GateError:
        return
    fail(f"{label} negative fixture unexpectedly passed")


def expect_projection_rejected(label: str, root: Path) -> None:
    try:
        load_d1_projection(root)
    except GateError:
        return
    fail(f"{label} D1 repository fixture unexpectedly passed")


def self_test() -> None:
    sources = rust_sources(ROOT)
    cargo = read(ROOT, CARGO)
    lock = read(ROOT, LOCK)
    validate_source(sources, cargo, lock)
    validate_d1_projection(load_d1_projection(ROOT))

    process_sources = dict(sources)
    process_sources["d1.rs"] += '\nfn forbidden() { let _ = Command::new("wrangler"); }\n'
    expect_source_rejected("D1 process authority", process_sources, cargo, lock)
    mutation_sources = dict(sources)
    mutation_sources["d1.rs"] += '\nfn forbidden() { let _ = fs::write("state", b"x"); }\n'
    expect_source_rejected("D1 filesystem mutation", mutation_sources, cargo, lock)
    dependency = cargo.replace(
        'serde_json = "=1.0.151"',
        'serde_json = "=1.0.151"\nreqwest = "=0.13.1"',
        1,
    )
    expect_source_rejected("unreviewed dependency", sources, dependency, lock)

    with tempfile.TemporaryDirectory(prefix="opsctl-d1-negative-") as temporary:
        fixture = Path(temporary)
        shutil.copytree(ROOT / "migrations" / "d1", fixture / "migrations" / "d1")
        shutil.copytree(
            ROOT / "migrations" / "resolver-d1",
            fixture / "migrations" / "resolver-d1",
        )
        migration = fixture / "migrations" / "d1" / "0001_catalog.sql"
        migration.write_bytes(migration.read_bytes() + b"\n-- tampered\n")
        expect_projection_rejected("historical SQL substitution", fixture)

    print(
        "opsctl read-only capability, typed D1 catalog, historical-anchor and dependency "
        "negative fixtures passed."
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
            "opsctl remains native, read-only and provider-free; D1 history is derived from "
            "canonical SQL under compact typed historical anchors."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except GateError as error:
        print(f"opsctl gate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
