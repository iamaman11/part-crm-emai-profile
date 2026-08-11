#!/usr/bin/env python3
"""Fail when provider/runtime dependencies enter pure domain/application boundaries."""

from __future__ import annotations

import argparse
import subprocess
import sys
import tomllib
from pathlib import Path

from accepted_phase_provenance import load_ledger, provenance_self_test, validate_plan_provenance

PURE_CRATE_ALLOWLISTS: dict[str, set[str]] = {
    "profile-platform-primitives": set(),
    "contracts": {"profile-platform-primitives"},
    "control-plane-contract": {"serde", "serde_json"},
    "identity-access-domain": {"profile-platform-primitives", "contracts"},
    "client-domain": {"profile-platform-primitives", "contracts", "zeroize"},
    "device-domain": {"profile-platform-primitives"},
    "browser-execution-domain": {"profile-platform-primitives"},
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
        "device-domain",
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
    "use-cases-devices": {
        "profile-platform-primitives",
        "device-domain",
        "application-ports",
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
PHASE2G_POLICY = Path("scripts/check-phase2g-realtime-boundaries.py")
PHASE2H_POLICY = Path("scripts/check-phase2h-ui-boundaries.py")
PHASE2I_POLICY = Path("scripts/check-phase2i-hardening.py")
PHASE2I_OPERATIONAL_POLICY = Path("scripts/check-phase2i-operational-bounds.py")


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


def check_policy(root: Path, policy_path: Path, label: str) -> list[str]:
    policy = root / policy_path
    if not policy.is_file():
        return [f"missing permanent {label} policy: {policy}"]

    errors: list[str] = []
    for extra_args, mode in (((), "policy"), (("--self-test",), "negative fixtures")):
        result = subprocess.run(
            [sys.executable, str(policy), "--root", str(root), *extra_args],
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).strip()
            errors.append(f"{label} {mode} failed: {detail}")
    return errors


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
        try:
            ledger = load_ledger(root / "architecture" / "accepted-phases.json")
            plan = (root / "docs" / "DEVELOPMENT_PLAN.md").read_text(encoding="utf-8")
            errors.extend(validate_plan_provenance(plan, ledger))
            provenance_self_test(plan, ledger)
        except (OSError, ValueError) as error:
            errors.append(f"accepted phase provenance validation failed: {error}")
        errors.extend(check_policy(root, PHASE2G_POLICY, "Phase 2G realtime"))
        errors.extend(check_policy(root, PHASE2H_POLICY, "Phase 2H UI"))
        errors.extend(check_policy(root, PHASE2I_POLICY, "Phase 2I hardening"))
        errors.extend(
            check_policy(root, PHASE2I_OPERATIONAL_POLICY, "Phase 2I operational bounds")
        )
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

    print("Architecture dependency boundaries and accepted phase provenance are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
