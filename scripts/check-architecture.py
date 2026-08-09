#!/usr/bin/env python3
"""Fail when provider/runtime dependencies enter pure domain/application boundaries."""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

PURE_CRATE_ALLOWLISTS: dict[str, set[str]] = {
    "profile-platform-primitives": set(),
    "contracts": {"profile-platform-primitives"},
    "control-plane-contract": {"serde", "serde_json"},
    "identity-access-domain": {"profile-platform-primitives", "contracts"},
    "client-domain": {"profile-platform-primitives", "contracts", "zeroize"},
    "device-domain": {"profile-platform-primitives"},
    "profile-domain": {"profile-platform-primitives", "contracts"},
    "session-domain": {"profile-platform-primitives", "contracts"},
    "mailbox-domain": {"profile-platform-primitives", "contracts"},
    "notification-domain": {"profile-platform-primitives"},
    "bridge-domain": {"profile-platform-primitives"},
    "runtime-bundle-domain": set(),
    "certification-domain": {
        "profile-platform-primitives",
        "sha2",
    },
    "encrypted-generation-domain": {
        "profile-platform-primitives",
        "chacha20poly1305",
        "sha2",
        "zeroize",
    },
    "application-ports": {
        "profile-platform-primitives",
        "contracts",
        "identity-access-domain",
        "client-domain",
        "profile-domain",
        "session-domain",
        "mailbox-domain",
        "notification-domain",
    },
    "use-cases-clients": {
        "profile-platform-primitives",
        "contracts",
        "identity-access-domain",
        "client-domain",
        "application-ports",
        "zeroize",
    },
    "use-cases-identity": {
        "profile-platform-primitives",
        "identity-access-domain",
        "application-ports",
    },
    "use-cases-mailboxes": {
        "profile-platform-primitives",
        "identity-access-domain",
        "mailbox-domain",
        "application-ports",
    },
    "use-cases-notifications": {
        "profile-platform-primitives",
        "contracts",
        "notification-domain",
        "application-ports",
    },
    "use-cases": {
        "profile-platform-primitives",
        "contracts",
        "identity-access-domain",
        "client-domain",
        "profile-domain",
        "session-domain",
        "mailbox-domain",
        "application-ports",
        "use-cases-clients",
        "use-cases-identity",
        "use-cases-mailboxes",
        "use-cases-notifications",
    },
}

FORBIDDEN_DEPENDENCIES = {
    "worker",
    "worker-sys",
    "wasm-bindgen",
    "tokio",
    "axum",
    "sqlx",
    "windows",
    "windows-sys",
    "pyo3",
    "playwright",
    "reqwest",
    "rusqlite",
}

DEPENDENCY_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")


def dependency_names(document: dict[str, object]) -> set[str]:
    names: set[str] = set()
    for section in DEPENDENCY_SECTIONS:
        value = document.get(section, {})
        if isinstance(value, dict):
            names.update(str(name) for name in value)
    for target in document.get("target", {}).values() if isinstance(document.get("target"), dict) else ():
        if not isinstance(target, dict):
            continue
        for section in DEPENDENCY_SECTIONS:
            value = target.get(section, {})
            if isinstance(value, dict):
                names.update(str(name) for name in value)
    return names


def check(root: Path) -> list[str]:
    errors: list[str] = []
    manifests = sorted((root / "crates").glob("*/Cargo.toml"))
    if not manifests:
        return [f"no crate manifests found below {root / 'crates'}"]

    seen: set[str] = set()
    for manifest in manifests:
        with manifest.open("rb") as handle:
            document = tomllib.load(handle)
        package = document.get("package", {})
        if not isinstance(package, dict) or not isinstance(package.get("name"), str):
            errors.append(f"{manifest}: missing package.name")
            continue

        name = package["name"]
        if name not in PURE_CRATE_ALLOWLISTS:
            continue
        seen.add(name)
        dependencies = dependency_names(document)
        forbidden = dependencies & FORBIDDEN_DEPENDENCIES
        if forbidden:
            errors.append(
                f"{manifest}: forbidden provider/runtime dependencies: {sorted(forbidden)}"
            )
        unexpected = dependencies - PURE_CRATE_ALLOWLISTS[name]
        if unexpected:
            errors.append(
                f"{manifest}: dependencies outside pure allowlist: {sorted(unexpected)}"
            )

    if root.resolve() == Path.cwd().resolve():
        missing = set(PURE_CRATE_ALLOWLISTS) - seen
        if missing:
            errors.append(f"missing governed pure crates: {sorted(missing)}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()

    errors = check(args.root)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Architecture dependency boundaries are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
