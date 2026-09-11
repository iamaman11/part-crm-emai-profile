#!/usr/bin/env python3
"""Reject raw D1 access and permanent D1 operations-boundary drift."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Keep this list specific to D1 APIs. Generic method names such as `.prepare(`
# and `.batch(` also belong to cryptography, HTTP and application code and
# therefore create false positives without strengthening the D1 boundary.
RAW_D1_TOKENS = (
    "D1Database",
    "D1PreparedStatement",
    "D1Result",
    "worker::d1",
    "query!(",
)

RESOLVER_D1_BOUNDARY_FILES = {
    Path("apps/mailbox-secret-resolver-worker/src/replay.rs"),
    Path("apps/mailbox-secret-resolver-worker/src/storage.rs"),
}

D1_CONTRACT = Path("docs/D1_CATALOG.md")
D1_INDEX = Path("docs/INDEX.md")
STANDARD_OPERATOR = Path(".github/workflows/d1-operator.yml")
MIGRATION_EXECUTOR = Path(".github/workflows/d1-migration-executor.yml")
D1_WORKFLOWS = Path(".github/workflows")

D1_CONTRACT_MARKERS = (
    "PERMANENT NORMATIVE BOUNDED CONTRACT",
    "Permanent Migration Operations Contract",
    "standard operator-facing procedure:",
    "manual internal JSON assembly = 0",
    "second ordinary D1 migration mutation owner = 0",
    "automatic destructive restore = 0",
    "post-authorization replanning = 0",
    "Production authorization implied by migration tooling = 0",
    "Ordinary operation starts from fresh protected",
)

STANDARD_OPERATOR_MARKERS = (
    "name: D1 Standard Operator",
    "Standard D1 operator is transport-only",
    "  workflow_dispatch:\n\npermissions:",
    "D1_TRANSACTION_AUTHORIZATION_V1",
    "D1_OPERATOR_OUTCOME_V1",
    "d1-migration-prepare.yml",
    "d1-isolated-target-observation.yml",
    "d1-migration-executor.yml",
    "d1-execution-receipt-",
    "REPLAY_OR_NOOP",
    "COMPLETED_VERIFIED",
    "SOURCE_TREE_TRANSACTION_DRIFT",
    "TARGET_PRESTATE_DRIFT",
)

STANDARD_OPERATOR_FORBIDDEN = (
    "secrets.CLOUDFLARE_API_TOKEN",
    "secrets.CLOUDFLARE_OBSERVE_API_TOKEN",
    "d1 migrations apply",
    "d1 time-travel restore",
    "0027_pas2_payload_fingerprint_expand.sql",
    "0031_device_binding_governance.sql",
    "0032_pas2_payload_fingerprint_contract.sql",
)

EXECUTOR_MARKERS = (
    "name: Protected D1 Migration Executor",
    "environment: staging",
    "D1_TRANSACTION_AUTHORIZATION_V1",
    "d1 migrations apply",
    "PREWRITE_FENCE_PASS",
    "MUTATION_STARTED",
    "execution-receipt.json",
    "RECOVERY_REQUIRED",
    "FAILED_NO_EFFECT",
    "automatic_restore_executed': False",
)

REMOTE_APPLY = re.compile(r"d1 migrations apply.{0,500}?--remote", re.DOTALL)
TIME_TRAVEL_RESTORE_EFFECT = re.compile(
    r"(?:--request POST.{0,800}?/time_travel/restore\?bookmark=|"
    r"/time_travel/restore\?bookmark=.{0,800}?--request POST)",
    re.DOTALL,
)


def read_text(root: Path, relative: Path, errors: list[str]) -> str:
    path = root / relative
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        errors.append(f"{relative}: missing/unreadable permanent D1 owner: {error}")
        return ""


def check_operations_contract(root: Path) -> list[str]:
    errors: list[str] = []
    contract = read_text(root, D1_CONTRACT, errors)
    index = read_text(root, D1_INDEX, errors)
    operator = read_text(root, STANDARD_OPERATOR, errors)
    executor = read_text(root, MIGRATION_EXECUTOR, errors)

    for marker in D1_CONTRACT_MARKERS:
        if marker not in contract:
            errors.append(f"{D1_CONTRACT}: permanent D1 operations marker missing: {marker}")

    if "D1 catalog" not in index or "D1_CATALOG.md" not in index:
        errors.append(f"{D1_INDEX}: D1 permanent bounded contract is not discoverable")

    for marker in STANDARD_OPERATOR_MARKERS:
        if marker not in operator:
            errors.append(f"{STANDARD_OPERATOR}: standard operator marker missing: {marker}")
    for marker in STANDARD_OPERATOR_FORBIDDEN:
        if marker in operator:
            errors.append(f"{STANDARD_OPERATOR}: operator acquired forbidden policy/effect surface: {marker}")

    for marker in EXECUTOR_MARKERS:
        if marker not in executor:
            errors.append(f"{MIGRATION_EXECUTOR}: sole executor marker missing: {marker}")
    if "          - production" in executor or "environment: ${{ inputs.environment }}" in executor:
        errors.append(f"{MIGRATION_EXECUTOR}: migration executor must remain literal staging-only")
    if "python scripts/d1-prepare.py prepare" in executor:
        errors.append(f"{MIGRATION_EXECUTOR}: post-authorization re-prepare/replan is forbidden")

    apply_owners: list[Path] = []
    restore_owners: list[Path] = []
    workflow_root = root / D1_WORKFLOWS
    try:
        workflow_paths = sorted(
            path
            for path in workflow_root.iterdir()
            if path.is_file() and path.suffix.lower() in {".yml", ".yaml"}
        )
    except OSError as error:
        errors.append(f"{D1_WORKFLOWS}: cannot enumerate workflows: {error}")
        workflow_paths = []

    for path in workflow_paths:
        text = path.read_text(encoding="utf-8")
        relative = path.relative_to(root)
        if REMOTE_APPLY.search(text):
            apply_owners.append(relative)
        if TIME_TRAVEL_RESTORE_EFFECT.search(text):
            restore_owners.append(relative)

    if apply_owners != [MIGRATION_EXECUTOR]:
        errors.append(
            "ordinary remote D1 migration apply must have exactly one workflow owner "
            f"{MIGRATION_EXECUTOR}; observed={apply_owners}"
        )
    if restore_owners != [MIGRATION_EXECUTOR]:
        errors.append(
            "sanctioned D1 Time Travel restore must have exactly one workflow owner "
            f"{MIGRATION_EXECUTOR}; observed={restore_owners}"
        )

    return errors


def check(root: Path) -> list[str]:
    errors: list[str] = []
    repository_root = root.resolve() == Path.cwd().resolve()

    for path in sorted(root.rglob("*.rs")):
        relative = path.relative_to(root)
        text = path.read_text(encoding="utf-8")

        if repository_root and relative.parts[:2] == ("tests", "d1-boundary"):
            continue

        if relative.parts[:2] == ("crates", "cloudflare-adapters"):
            continue

        if repository_root and relative in RESOLVER_D1_BOUNDARY_FILES:
            continue

        forbidden = [token for token in RAW_D1_TOKENS if token in text]
        if forbidden:
            if relative.parts[:3] == ("apps", "control-plane-worker", "src"):
                errors.append(
                    f"{relative}: Worker composition may obtain env.d1 only; raw tokens {forbidden}"
                )
            else:
                errors.append(f"{relative}: raw D1 access outside adapter boundary: {forbidden}")

    # Repository-level permanent migration operations checks are intentionally not
    # applied to isolated raw-D1 negative fixtures passed through --root.
    if repository_root:
        errors.extend(check_operations_contract(root))

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

    print("D1 access and permanent migration operations are confined to their accepted natural owners.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
