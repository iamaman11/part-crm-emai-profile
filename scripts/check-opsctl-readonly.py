#!/usr/bin/env python3
"""Fail closed if the AR-6 Rust opsctl grows mutable authority."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIB = Path("tools/opsctl/src/lib.rs")
MAIN = Path("tools/opsctl/src/main.rs")
CARGO = Path("tools/opsctl/Cargo.toml")
LOCK = Path("tools/opsctl/Cargo.lock")
EXPECTED_COMMANDS = {"Doctor", "Status", "Inventory"}
EXPECTED_PARSE_LITERALS = {"doctor", "status", "inventory"}
FORBIDDEN_RUNTIME_MARKERS = (
    'Command::new("wrangler")',
    'Command::new("git")',
    "reqwest::",
    "ureq::",
    "worker::",
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


class GateError(ValueError):
    pass


def fail(message: str) -> None:
    raise GateError(message)


def read(root: Path, relative: Path) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        fail(f"required opsctl file is missing/not regular: {relative}")
    return path.read_text(encoding="utf-8")


def validate_source(lib: str, main: str, cargo: str, lock: str) -> None:
    if "#![forbid(unsafe_code)]" not in lib:
        fail("opsctl must forbid unsafe code")
    enum = re.search(r"pub enum ReadCommand\s*\{(?P<body>.*?)\n\}", lib, re.S)
    if enum is None:
        fail("opsctl ReadCommand enum is missing")
    commands = {
        line.strip().rstrip(",")
        for line in enum.group("body").splitlines()
        if line.strip()
    }
    if commands != EXPECTED_COMMANDS:
        fail(f"opsctl command surface must be exactly {sorted(EXPECTED_COMMANDS)}; observed={sorted(commands)}")

    parser = re.search(r"fn parse_command\(.*?\n\}", lib, re.S)
    if parser is None:
        fail("opsctl parse_command is missing")
    literals = set(re.findall(r'^\s*"([a-z][a-z0-9_-]*)"\s*=>', parser.group(0), re.M))
    if literals != EXPECTED_PARSE_LITERALS:
        fail(f"opsctl parser surface drifted: {sorted(literals)}")

    production_source = lib.split("#[cfg(test)]", 1)[0] + "\n" + main
    for marker in FORBIDDEN_RUNTIME_MARKERS:
        if marker in production_source:
            fail(f"opsctl read-only boundary contains forbidden runtime marker: {marker}")
    for marker in FORBIDDEN_MUTATION_CAPABILITIES:
        if marker in production_source:
            fail(f"opsctl read-only boundary contains forbidden mutation capability: {marker}")

    if production_source.count("Command::new(") != 1:
        fail("opsctl may contain exactly one process-spawn site")
    if production_source.count("Command::new(&python)") != 1:
        fail("opsctl process-spawn site must target only the canonical Python validator runner")

    for required in (
        '"scripts/generate-architecture-inventory.py"',
        '"scripts/python-estate-ar6.py"',
        'canonical_json_document(&repo_root, "docs/status.json", "status")',
        'canonical_json_document(&repo_root, "architecture/inventory.json", "inventory")',
        '"mutation_executed\\\":false',
    ):
        if required not in lib:
            fail(f"opsctl lost required read-only marker: {required}")

    if "[dependencies]" in cargo or "[dev-dependencies]" in cargo or "[build-dependencies]" in cargo:
        fail("AR-6 opsctl must remain dependency-free")
    if '[workspace]' not in cargo or 'name = "opsctl"' not in cargo:
        fail("opsctl must remain a standalone Cargo workspace/package")
    if 'name = "opsctl"' not in lock or "version = 4" not in lock:
        fail("opsctl lockfile is missing its exact package identity")


def validate(root: Path = ROOT) -> None:
    validate_source(read(root, LIB), read(root, MAIN), read(root, CARGO), read(root, LOCK))


def self_test() -> None:
    lib = read(ROOT, LIB)
    main = read(ROOT, MAIN)
    cargo = read(ROOT, CARGO)
    lock = read(ROOT, LOCK)
    validate_source(lib, main, cargo, lock)

    process_injected = (
        lib.split("#[cfg(test)]", 1)[0]
        + '\nfn forbidden() { let _ = Command::new("wrangler").arg("deploy"); }\n#[cfg(test)]'
        + lib.split("#[cfg(test)]", 1)[1]
    )
    try:
        validate_source(process_injected, main, cargo, lock)
    except GateError:
        pass
    else:
        fail("mutable process-spawn negative fixture unexpectedly passed")

    filesystem_injected = (
        lib.split("#[cfg(test)]", 1)[0]
        + '\nfn forbidden_write() { let _ = fs::write("state.json", b"mutable"); }\n#[cfg(test)]'
        + lib.split("#[cfg(test)]", 1)[1]
    )
    try:
        validate_source(filesystem_injected, main, cargo, lock)
    except GateError:
        pass
    else:
        fail("filesystem mutation capability negative fixture unexpectedly passed")

    environment_injected = (
        lib.split("#[cfg(test)]", 1)[0]
        + '\nfn forbidden_env() { unsafe { env::set_var("TOKEN", "mutable"); } }\n#[cfg(test)]'
        + lib.split("#[cfg(test)]", 1)[1]
    )
    try:
        validate_source(environment_injected, main, cargo, lock)
    except GateError:
        pass
    else:
        fail("environment mutation capability negative fixture unexpectedly passed")

    expanded = lib.replace("    Inventory,\n}", "    Inventory,\n    Deploy,\n}", 1)
    try:
        validate_source(expanded, main, cargo, lock)
    except GateError:
        pass
    else:
        fail("mutable command-surface negative fixture unexpectedly passed")

    dependency = cargo + '\n[dependencies]\nreqwest = "1"\n'
    try:
        validate_source(lib, main, dependency, lock)
    except GateError:
        pass
    else:
        fail("opsctl dependency negative fixture unexpectedly passed")

    print("AR-6 opsctl capability, mutation and dependency negative fixtures rejected as expected.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        validate()
        print("AR-6 opsctl is dependency-free, typed and capability-bounded read-only.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except GateError as error:
        print(f"AR-6 opsctl gate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
