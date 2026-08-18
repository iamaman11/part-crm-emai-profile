#!/usr/bin/env python3
"""Fail closed if Rust opsctl gains provider/mutation authority or unreviewed runtime dependencies."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIB = Path("tools/opsctl/src/lib.rs")
D1 = Path("tools/opsctl/src/d1.rs")
MAIN = Path("tools/opsctl/src/main.rs")
CARGO = Path("tools/opsctl/Cargo.toml")
LOCK = Path("tools/opsctl/Cargo.lock")
EXPECTED_COMMANDS = {"Doctor", "Status", "Inventory", "CredentialLifecycle", "RotationPlan"}
EXPECTED_PARSE_LITERALS = {"doctor", "status", "inventory", "credential-lifecycle", "rotation-plan"}
EXPECTED_D1_ACTIONS = {"status", "plan", "compatibility", "verify"}
ALLOWED_DEPENDENCIES = {"serde_json": "=1.0.151"}
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
        fail(f"opsctl legacy read command surface must be exactly {sorted(EXPECTED_COMMANDS)}; observed={sorted(commands)}")

    parser = re.search(r"fn parse_command\(.*?\n\}", lib, re.S)
    if parser is None:
        fail("opsctl parse_command is missing")
    literals = set(re.findall(r'^\s*"([a-z][a-z0-9_-]*)"\s*=>', parser.group(0), re.M))
    if literals != EXPECTED_PARSE_LITERALS:
        fail(f"opsctl legacy parser surface drifted: {sorted(literals)}")

    d1_parser = re.search(r"let action = match action_text \{(?P<body>.*?)\n\s*\};", lib, re.S)
    if d1_parser is None:
        fail("opsctl native D1 action parser is missing")
    d1_actions = set(re.findall(r'^\s*"([a-z][a-z0-9_-]*)"\s*=>', d1_parser.group("body"), re.M))
    if d1_actions != EXPECTED_D1_ACTIONS:
        fail(f"opsctl D1 action surface must be exactly {sorted(EXPECTED_D1_ACTIONS)}; observed={sorted(d1_actions)}")

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


def validate(root: Path = ROOT) -> None:
    validate_source(
        read(root, LIB),
        read(root, D1),
        read(root, MAIN),
        read(root, CARGO),
        read(root, LOCK),
    )


def expect_rejected(label: str, lib: str, d1: str, main: str, cargo: str, lock: str) -> None:
    try:
        validate_source(lib, d1, main, cargo, lock)
    except GateError:
        return
    fail(f"{label} negative fixture unexpectedly passed")


def self_test() -> None:
    lib = read(ROOT, LIB)
    d1 = read(ROOT, D1)
    main = read(ROOT, MAIN)
    cargo = read(ROOT, CARGO)
    lock = read(ROOT, LOCK)
    validate_source(lib, d1, main, cargo, lock)

    process_injected = d1.split("#[cfg(test)]", 1)[0] + '\nfn forbidden() { let _ = Command::new("wrangler").arg("deploy"); }\n#[cfg(test)]' + d1.split("#[cfg(test)]", 1)[1]
    expect_rejected("mutable D1 process-spawn", lib, process_injected, main, cargo, lock)

    filesystem_injected = d1.split("#[cfg(test)]", 1)[0] + '\nfn forbidden_write() { let _ = fs::write("state.json", b"mutable"); }\n#[cfg(test)]' + d1.split("#[cfg(test)]", 1)[1]
    expect_rejected("filesystem mutation capability", lib, filesystem_injected, main, cargo, lock)

    expanded = lib.replace('"verify" => d1::D1Action::Verify,', '"verify" => d1::D1Action::Verify,\n        "apply" => d1::D1Action::Verify,', 1)
    expect_rejected("mutable D1 command-surface", expanded, d1, main, cargo, lock)

    ar_specific = lib.replace('architecture/credential-lifecycle.json', 'architecture/ar8-completion-lifecycle.json', 1)
    expect_rejected("historical AR-specific canonical coupling", ar_specific, d1, main, cargo, lock)

    dependency = cargo.replace('serde_json = "=1.0.151"', 'serde_json = "=1.0.151"\nreqwest = "=0.13.1"', 1)
    expect_rejected("unreviewed dependency", lib, d1, main, dependency, lock)

    print("opsctl legacy bridge, native D1 subprocess/mutation, command-surface and dependency negative fixtures rejected as expected.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        validate()
        print("opsctl remains read-only: one accepted AR-6 Python validator bridge, native Rust D1 semantics, exact serde_json dependency, no provider/mutation capability.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except GateError as error:
        print(f"opsctl gate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
