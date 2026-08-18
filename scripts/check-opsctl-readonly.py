#!/usr/bin/env python3
"""Fail closed if Rust opsctl or its AR-9 D1 authority gains unreviewed capability."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LIB = Path("tools/opsctl/src/lib.rs")
D1 = Path("tools/opsctl/src/d1.rs")
MAIN = Path("tools/opsctl/src/main.rs")
CARGO = Path("tools/opsctl/Cargo.toml")
LOCK = Path("tools/opsctl/Cargo.lock")
D1_AUTHORITY = Path("architecture/d1-evolution-ar9.json")
EXPECTED_COMMANDS = {"Doctor", "Status", "Inventory", "CredentialLifecycle", "RotationPlan"}
EXPECTED_PARSE_LITERALS = {"doctor", "status", "inventory", "credential-lifecycle", "rotation-plan"}
EXPECTED_D1_ACTIONS = {"status", "plan", "compatibility", "verify"}
EXPECTED_MIGRATION_CLASSES = {"EXPAND", "BACKFILL", "CONTRACT", "REPAIR"}
EXPECTED_LEDGER_STATES = {
    "EXACT",
    "BEHIND_KNOWN_PREFIX",
    "AHEAD_KNOWN_COMPATIBLE",
    "AHEAD_KNOWN_INCOMPATIBLE",
    "DIVERGED",
    "UNKNOWN_MIGRATION",
    "CORRUPT_LEDGER",
}
COMPONENT_ROOTS = {
    "catalog": "migrations/d1",
    "resolver": "migrations/resolver-d1",
}
ALLOWED_DEPENDENCIES = {"serde_json": "=1.0.151"}
MIGRATION_RE = re.compile(r"^(?P<number>[0-9]{4})_[a-z0-9_]+\.sql$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_BLOB_SHA1_RE = re.compile(r"^[0-9a-f]{40}$")
FORBIDDEN_RUNTIME_MARKERS = (
    'Command::new("wrangler")',
    'Command::new("node")',
    'Command::new("npx")',
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
FORBIDDEN_MUTATION_CAPABILITIES = (
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
    "std::fs::File",
    "std::io::Write",
    ".write_all(",
    "env::set_var(",
    "env::remove_var(",
)
FORBIDDEN_AR_CANONICAL_PATHS = (
    'architecture/ar8-completion-lifecycle.json',
    'architecture/ar8-operator-rehearsal.json',
)
REQUIRED_SUBJECT_PATHS = (
    'architecture/credential-authority.json',
    'architecture/credential-lifecycle.json',
    'architecture/profile-security.json',
    'architecture/operator-contract.json',
)


class GateError(ValueError):
    pass


def fail(message: str) -> None:
    raise GateError(message)


def read(root: Path, relative: Path) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        fail(f"required opsctl file is missing/not regular: {relative}")
    return path.read_text(encoding="utf-8")


def production(text: str) -> str:
    return text.split("#[cfg(test)]", 1)[0]


def parse_dependency_table(cargo: str) -> dict[str, str]:
    match = re.search(r"(?ms)^\[dependencies\]\n(?P<body>.*?)(?=^\[|\Z)", cargo)
    if match is None:
        return {}
    result: dict[str, str] = {}
    for raw in match.group("body").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        item = re.fullmatch(r'([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"', line)
        if item is None:
            fail(f"opsctl dependency declaration must be simple and exact-pinned: {line}")
        result[item.group(1)] = item.group(2)
    return result


def validate_source(lib: str, d1: str, main: str, cargo: str, lock: str) -> None:
    if "#![forbid(unsafe_code)]" not in lib:
        fail("opsctl must forbid unsafe code")
    if "mod d1;" not in lib:
        fail("opsctl must retain the AR-9 native D1 policy module")

    enum = re.search(r"pub enum ReadCommand\s*\{(?P<body>.*?)\n\}", lib, re.S)
    if enum is None:
        fail("opsctl ReadCommand enum is missing")
    commands = {line.strip().rstrip(",") for line in enum.group("body").splitlines() if line.strip()}
    if commands != EXPECTED_COMMANDS:
        fail(
            f"opsctl legacy read command surface must be exactly {sorted(EXPECTED_COMMANDS)}; "
            f"observed={sorted(commands)}"
        )

    parser = re.search(r"fn parse_command\(.*?\n\}", lib, re.S)
    if parser is None:
        fail("opsctl parse_command is missing")
    literals = set(re.findall(r'^\s*"([a-z][a-z0-9_-]*)"\s*=>', parser.group(0), re.M))
    if literals != EXPECTED_PARSE_LITERALS:
        fail(f"opsctl legacy parser surface drifted: {sorted(literals)}")

    d1_parser = re.search(r"let action = match action_text \{(?P<body>.*?)\n\s*\};", lib, re.S)
    if d1_parser is None:
        fail("opsctl native D1 action parser is missing")
    d1_actions = set(
        re.findall(r'^\s*"([a-z][a-z0-9_-]*)"\s*=>', d1_parser.group("body"), re.M)
    )
    if d1_actions != EXPECTED_D1_ACTIONS:
        fail(
            f"opsctl D1 action surface must be exactly {sorted(EXPECTED_D1_ACTIONS)}; "
            f"observed={sorted(d1_actions)}"
        )

    production_source = production(lib) + "\n" + production(d1) + "\n" + main
    for marker in FORBIDDEN_RUNTIME_MARKERS:
        if marker in production_source:
            fail(f"opsctl read-only boundary contains forbidden runtime marker: {marker}")
    for marker in FORBIDDEN_MUTATION_CAPABILITIES:
        if marker in production_source:
            fail(f"opsctl read-only boundary contains forbidden mutation capability: {marker}")
    if production_source.count("Command::new(") != 1 or production_source.count("Command::new(&python)") != 1:
        fail("opsctl may spawn exactly one accepted AR-6 canonical Python validator process site")
    if "Command::new(" in production(d1):
        fail("native AR-9 opsctl D1 semantics must not spawn child processes")

    for path in FORBIDDEN_AR_CANONICAL_PATHS:
        if path in lib:
            fail(f"opsctl must not depend on historical AR-specific canonical path: {path}")
    for path in REQUIRED_SUBJECT_PATHS:
        if f'"{path}"' not in lib:
            fail(f"opsctl lost subject-domain authority path: {path}")

    for required in (
        '"scripts/generate-architecture-inventory.py"',
        '"scripts/python-estate-ar6.py"',
        'canonical_json_document(&repo_root, "docs/status.json", "status")',
        'canonical_json_document(&repo_root, "architecture/inventory.json", "inventory")',
        '"credential-lifecycle"',
        '"rotation-plan"',
        '"mutation_executed\\\":false',
        "permanently read-only and metadata-only",
        "d1 status",
        "d1 plan",
        "d1 compatibility",
        "d1 verify",
    ):
        if required not in lib:
            fail(f"opsctl lost required read-only marker: {required}")

    for required in (
        'DEFAULT_AUTHORITY: &str = "architecture/d1-evolution-ar9.json"',
        '"EXACT"',
        '"BEHIND_KNOWN_PREFIX"',
        '"AHEAD_KNOWN_COMPATIBLE"',
        '"AHEAD_KNOWN_INCOMPATIBLE"',
        '"DIVERGED"',
        '"UNKNOWN_MIGRATION"',
        '"CORRUPT_LEDGER"',
        '"mutation_executed": false',
    ):
        if required not in d1:
            fail(f"opsctl D1 policy engine lost required marker: {required}")

    dependencies = parse_dependency_table(cargo)
    if dependencies != ALLOWED_DEPENDENCIES:
        fail(f"opsctl dependency set must be exactly {ALLOWED_DEPENDENCIES}; observed={dependencies}")
    if "[dev-dependencies]" in cargo or "[build-dependencies]" in cargo:
        fail("opsctl must not add dev/build dependency authority")
    if "[workspace]" not in cargo or 'name = "opsctl"' not in cargo:
        fail("opsctl must remain a standalone Cargo workspace/package")
    if 'name = "opsctl"' not in lock or "version = 4" not in lock:
        fail("opsctl lockfile is missing its exact package identity")
    if 'name = "serde_json"\nversion = "1.0.151"' not in lock:
        fail("opsctl lockfile must pin serde_json 1.0.151")


def canonical_freeze(root: Path, migration_root: str) -> tuple[list[dict[str, str]], str]:
    directory = root / migration_root
    if not directory.is_dir() or directory.is_symlink():
        fail(f"D1 migration root must be a real directory: {migration_root}")
    files = sorted(directory.glob("*.sql"), key=lambda item: item.name)
    if not files:
        fail(f"D1 migration root is empty: {migration_root}")
    numbers: list[int] = []
    entries: list[dict[str, str]] = []
    for path in files:
        if not path.is_file() or path.is_symlink():
            fail(f"D1 migration must be a regular file: {path.relative_to(root)}")
        match = MIGRATION_RE.fullmatch(path.name)
        if match is None:
            fail(f"invalid D1 migration filename: {path.name}")
        if path.name.endswith("_down.sql"):
            fail(f"down migration is forbidden as rollback authority: {path.name}")
        numbers.append(int(match.group("number")))
        entries.append({"name": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    expected_numbers = list(range(1, len(files) + 1))
    if numbers != expected_numbers:
        fail(
            f"D1 migration history must be contiguous from 0001: root={migration_root} "
            f"observed={numbers}"
        )
    canonical = json.dumps(entries, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    return entries, digest


def load_d1_authority(root: Path) -> dict[str, Any]:
    path = root / D1_AUTHORITY
    if not path.is_file() or path.is_symlink():
        fail(f"D1 evolution authority is missing/not regular: {D1_AUTHORITY}")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise GateError(f"D1 evolution authority is malformed: {error}") from error
    if not isinstance(payload, dict):
        fail("D1 evolution authority must be one JSON object")
    return payload


def validate_d1_authority_document(root: Path, payload: dict[str, Any]) -> None:
    if payload.get("kind") != "D1_EVOLUTION_AUTHORITY" or payload.get("schema_version") != 1:
        fail("D1 evolution authority identity/version is invalid")
    if payload.get("production_mutation") is not False:
        fail("AR-9 D1 authority must not claim production mutation")

    global_policy = payload.get("global_policy")
    if not isinstance(global_policy, dict):
        fail("D1 global policy is missing")
    if set(global_policy.get("migration_classes", [])) != EXPECTED_MIGRATION_CLASSES:
        fail("D1 migration class vocabulary drifted")
    if set(global_policy.get("ledger_states", [])) != EXPECTED_LEDGER_STATES:
        fail("D1 ledger-state vocabulary drifted")
    if global_policy.get("new_opsctl_process_spawn_sites") != 0:
        fail("D1 authority must require zero new opsctl process-spawn sites")
    if global_policy.get("opsctl_provider_credentials") is not False:
        fail("D1 authority must forbid provider credentials in opsctl")
    if global_policy.get("database_lock_required_by_default") is not False:
        fail("D1 authority must not invent a default DB lock")
    if global_policy.get("resource_auto_provisioning_allowed") is not False:
        fail("D1 authority must forbid automatic resource provisioning")

    components = payload.get("components")
    if not isinstance(components, list):
        fail("D1 components must be a list")
    by_id: dict[str, dict[str, Any]] = {}
    for component in components:
        if not isinstance(component, dict) or not isinstance(component.get("component_id"), str):
            fail("D1 component record is malformed")
        component_id = component["component_id"]
        if component_id in by_id:
            fail(f"duplicate D1 component: {component_id}")
        by_id[component_id] = component
    if set(by_id) != set(COMPONENT_ROOTS):
        fail(f"D1 component set must be exactly {sorted(COMPONENT_ROOTS)}")

    for component_id, migration_root in COMPONENT_ROOTS.items():
        component = by_id[component_id]
        if component.get("migration_root") != migration_root:
            fail(f"{component_id} migration root drifted")
        historical = component.get("historical_epoch")
        if not isinstance(historical, dict):
            fail(f"{component_id} historical epoch is missing")
        ordered = historical.get("ordered_history")
        if not isinstance(ordered, list) or not ordered:
            fail(f"{component_id} ordered history is missing")

        expected_entries, expected_digest = canonical_freeze(root, migration_root)
        expected_names = [entry["name"] for entry in expected_entries]
        observed_names: list[str] = []
        observed_hashes: list[str | None] = []
        for entry in ordered:
            if not isinstance(entry, dict) or not isinstance(entry.get("name"), str):
                fail(f"{component_id} historical entry is malformed")
            blob = entry.get("git_blob_sha1")
            if not isinstance(blob, str) or GIT_BLOB_SHA1_RE.fullmatch(blob) is None:
                fail(f"{component_id} historical entry has malformed Git blob identity: {entry.get('name')}")
            sha256 = entry.get("sha256")
            if sha256 is not None and (not isinstance(sha256, str) or SHA256_RE.fullmatch(sha256) is None):
                fail(f"{component_id} historical entry has malformed SHA-256: {entry.get('name')}")
            observed_names.append(entry["name"])
            observed_hashes.append(sha256 if isinstance(sha256, str) else None)

        if observed_names != expected_names:
            fail(
                f"{component_id} historical migration names differ from exact repository order: "
                f"observed={observed_names} expected={expected_names}"
            )
        expected_hashes = [entry["sha256"] for entry in expected_entries]
        freeze = historical.get("per_file_sha256_freeze")
        freeze_complete = (
            observed_hashes == expected_hashes
            and historical.get("ordered_set_identity_algorithm")
            == "sha256(canonical-json(name+sha256))"
            and historical.get("ordered_set_identity") == expected_digest
            and isinstance(freeze, dict)
            and freeze.get("status") == "FROZEN"
            and freeze.get("algorithm") == "sha256"
            and freeze.get("count") == len(expected_entries)
        )
        if not freeze_complete:
            expected = {
                "component_id": component_id,
                "ordered_history": expected_entries,
                "ordered_set_identity_algorithm": "sha256(canonical-json(name+sha256))",
                "ordered_set_identity": expected_digest,
                "per_file_sha256_freeze": {
                    "status": "FROZEN",
                    "algorithm": "sha256",
                    "count": len(expected_entries),
                },
            }
            fail(
                f"D1 historical freeze incomplete or stale for {component_id}; "
                f"expected_freeze={json.dumps(expected, sort_keys=True, separators=(',', ':'))}"
            )
        if component.get("current_repository_revision") != expected_names[-1]:
            fail(f"{component_id} current repository revision must equal the final frozen migration")
        if historical.get("retroactive_runtime_compatibility_claims") is not False:
            fail(f"{component_id} historical epoch must not invent retroactive compatibility claims")


def validate(root: Path = ROOT) -> None:
    validate_source(
        read(root, LIB),
        read(root, D1),
        read(root, MAIN),
        read(root, CARGO),
        read(root, LOCK),
    )
    validate_d1_authority_document(root, load_d1_authority(root))


def expect_rejected(label: str, lib: str, d1: str, main: str, cargo: str, lock: str) -> None:
    try:
        validate_source(lib, d1, main, cargo, lock)
    except GateError:
        return
    fail(f"{label} negative fixture unexpectedly passed")


def expect_d1_rejected(label: str, payload: dict[str, Any]) -> None:
    try:
        validate_d1_authority_document(ROOT, payload)
    except GateError:
        return
    fail(f"{label} D1 authority negative fixture unexpectedly passed")


def self_test() -> None:
    lib = read(ROOT, LIB)
    d1 = read(ROOT, D1)
    main = read(ROOT, MAIN)
    cargo = read(ROOT, CARGO)
    lock = read(ROOT, LOCK)
    validate_source(lib, d1, main, cargo, lock)
    authority = load_d1_authority(ROOT)
    validate_d1_authority_document(ROOT, authority)

    process_injected = (
        d1.split("#[cfg(test)]", 1)[0]
        + '\nfn forbidden() { let _ = Command::new("wrangler").arg("deploy"); }\n#[cfg(test)]'
        + d1.split("#[cfg(test)]", 1)[1]
    )
    expect_rejected("mutable D1 process-spawn", lib, process_injected, main, cargo, lock)

    filesystem_injected = (
        d1.split("#[cfg(test)]", 1)[0]
        + '\nfn forbidden_write() { let _ = fs::write("state.json", b"mutable"); }\n#[cfg(test)]'
        + d1.split("#[cfg(test)]", 1)[1]
    )
    expect_rejected("filesystem mutation capability", lib, filesystem_injected, main, cargo, lock)

    expanded = lib.replace(
        '"verify" => d1::D1Action::Verify,',
        '"verify" => d1::D1Action::Verify,\n        "apply" => d1::D1Action::Verify,',
        1,
    )
    expect_rejected("mutable D1 command-surface", expanded, d1, main, cargo, lock)

    ar_specific = lib.replace(
        "architecture/credential-lifecycle.json",
        "architecture/ar8-completion-lifecycle.json",
        1,
    )
    expect_rejected("historical AR-specific canonical coupling", ar_specific, d1, main, cargo, lock)

    dependency = cargo.replace(
        'serde_json = "=1.0.151"',
        'serde_json = "=1.0.151"\nreqwest = "=0.13.1"',
        1,
    )
    expect_rejected("unreviewed dependency", lib, d1, main, dependency, lock)

    hash_mutation = copy.deepcopy(authority)
    hash_mutation["components"][0]["historical_epoch"]["ordered_history"][0]["sha256"] = "0" * 64
    expect_d1_rejected("historical SQL hash rewrite", hash_mutation)

    class_mutation = copy.deepcopy(authority)
    class_mutation["global_policy"]["migration_classes"].append("DOWN")
    expect_d1_rejected("unknown migration class", class_mutation)

    lock_mutation = copy.deepcopy(authority)
    lock_mutation["global_policy"]["database_lock_required_by_default"] = True
    expect_d1_rejected("unproven database lock", lock_mutation)

    print(
        "opsctl legacy bridge, native D1 subprocess/mutation, historical-freeze, command-surface "
        "and dependency negative fixtures rejected as expected."
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
            "opsctl remains read-only: one accepted AR-6 Python validator bridge, native Rust D1 "
            "semantics, frozen D1 histories, exact serde_json dependency, no provider/mutation capability."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except GateError as error:
        print(f"opsctl gate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
