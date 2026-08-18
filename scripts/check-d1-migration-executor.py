#!/usr/bin/env python3
"""Fail closed if the AR-9 protected D1 mutation authority drifts from its contract."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXECUTOR = Path(".github/workflows/d1-migration-executor.yml")
WORKFLOWS = Path(".github/workflows")
PINNED_WRANGLER = "wrangler@4.94.0"


class ExecutorGateError(ValueError):
    pass


def fail(message: str) -> None:
    raise ExecutorGateError(message)


def read_executor(root: Path = ROOT) -> str:
    path = root / EXECUTOR
    if not path.is_file() or path.is_symlink():
        fail(f"protected D1 executor is missing/not regular: {EXECUTOR}")
    return path.read_text(encoding="utf-8")


def normalized_shell(text: str) -> str:
    return re.sub(r"\\\s*\n\s*", " ", text)


def validate_executor(text: str, root: Path = ROOT) -> None:
    required_markers = (
        "workflow_call:",
        "workflow_dispatch:",
        "environment: ${{ inputs.environment }}",
        "group: d1-migration-${{ inputs.environment }}-${{ inputs.component }}-${{ inputs.database_id }}",
        "cancel-in-progress: false",
        'test "$GITHUB_REF" = "refs/heads/main"',
        'test "$GITHUB_SHA" = "$SOURCE_SHA"',
        'test "$MUTATION_AUTHORIZED" = "true"',
        'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID"',
        "docs/status.json",
        "production_authorized",
        "production D1 mutation remains blocked before Production Core authorization",
        "d1 info",
        "SELECT id, name FROM d1_migrations ORDER BY id",
        "d1 status",
        "d1 plan",
        "d1 compatibility",
        "d1 time-travel info",
        "d1 migrations apply",
        "d1 verify",
        "PRAGMA foreign_key_check",
        "PRAGMA integrity_check",
        "automatic_restore_executed': False",
        "secret_material_recorded': False",
        "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "--experimental-provision=false",
        "--experimental-auto-create=false",
        "env -u CLOUDFLARE_API_TOKEN -u CLOUDFLARE_ACCOUNT_ID cargo run",
    )
    for marker in required_markers:
        if marker not in text:
            fail(f"protected D1 executor lost required contract marker: {marker}")

    if "CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n      CLOUDFLARE_ACCOUNT_ID:" in text:
        fail("Cloudflare API token must not be job-level environment inherited by opsctl")
    job_env_match = re.search(r"(?ms)^    env:\n(?P<body>.*?)(?=^    steps:)", text)
    if job_env_match is None:
        fail("protected D1 executor job-level metadata environment is missing")
    if "CLOUDFLARE_API_TOKEN" in job_env_match.group("body"):
        fail("Cloudflare API token must not exist in the job-level environment")

    forbidden_markers = (
        "d1 time-travel restore",
        "time-travel restore",
        "d1 create",
        "database create",
        "experimental-provision=true",
        "experimental-auto-create=true",
        "cancel-in-progress: true",
    )
    for marker in forbidden_markers:
        if marker in text:
            fail(f"protected D1 executor contains forbidden mutation/recovery marker: {marker}")
    if re.search(r"(?is)create\s+table\s+[^\n;]*(?:d1|migration)[^\n;]*lock", text):
        fail("protected D1 executor must not invent a database-resident migration lock")

    normalized = normalized_shell(text)
    apply_sites = re.findall(rf"npx --yes {re.escape(PINNED_WRANGLER)} d1 migrations apply\b", normalized)
    if len(apply_sites) != 1:
        fail(f"protected D1 executor must contain exactly one pinned Wrangler apply site; observed={len(apply_sites)}")
    remote_apply = re.findall(
        rf"npx --yes {re.escape(PINNED_WRANGLER)} d1 migrations apply\b[^\n]*?--remote\b",
        normalized,
    )
    if len(remote_apply) != 1:
        fail("the sole protected D1 migration apply site must explicitly target --remote")

    provider_sites = re.findall(rf"npx --yes {re.escape(PINNED_WRANGLER)} d1 (?:info|execute|time-travel info|migrations apply)\b", normalized)
    if not provider_sites:
        fail("protected D1 executor contains no pinned Wrangler provider operations")
    if normalized.count("--experimental-provision=false") < len(provider_sites):
        fail("every protected provider operation must explicitly disable experimental provisioning")
    if normalized.count("--experimental-auto-create=false") < len(provider_sites):
        fail("every protected provider operation must explicitly disable experimental auto-create")

    token_steps = text.count("CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}")
    if token_steps < 5:
        fail("provider credential must be scoped to explicit Wrangler steps, not omitted or broadened")

    workflow_files = sorted((root / WORKFLOWS).glob("*.y*ml"), key=lambda item: item.name)
    remote_mutation_paths: list[str] = []
    for path in workflow_files:
        candidate = normalized_shell(path.read_text(encoding="utf-8"))
        if re.search(r"d1 migrations apply\b[^\n]*?--remote\b", candidate):
            remote_mutation_paths.append(str(path.relative_to(root)).replace("\\", "/"))
    if remote_mutation_paths != [str(EXECUTOR).replace("\\", "/")]:
        fail(
            "exactly one workflow may own remote D1 migrations apply; "
            f"observed={remote_mutation_paths}"
        )


def validate(root: Path = ROOT) -> None:
    validate_executor(read_executor(root), root)


def expect_rejected(label: str, text: str) -> None:
    try:
        validate_executor(text, ROOT)
    except ExecutorGateError:
        return
    fail(f"negative protected-executor fixture unexpectedly passed: {label}")


def self_test() -> None:
    text = read_executor(ROOT)
    validate_executor(text, ROOT)

    expect_rejected("automatic restore", text.replace("d1 time-travel info", "d1 time-travel restore", 1))
    expect_rejected("concurrency cancellation", text.replace("cancel-in-progress: false", "cancel-in-progress: true", 1))
    expect_rejected(
        "provider auto-create",
        text.replace("--experimental-auto-create=false", "--experimental-auto-create=true", 1),
    )
    expect_rejected(
        "job-level provider credential",
        text.replace(
            "    env:\n      CLOUDFLARE_ACCOUNT_ID:",
            "    env:\n      CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n      CLOUDFLARE_ACCOUNT_ID:",
            1,
        ),
    )
    expect_rejected(
        "second remote apply",
        text + f"\n# npx --yes {PINNED_WRANGLER} d1 migrations apply X --remote --experimental-provision=false --experimental-auto-create=false\n",
    )
    print("Protected D1 executor negative fixtures rejected as expected.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        validate()
        print(
            "Protected D1 executor contract passed: one remote mutation authority, serialized exact target, "
            "pinned Wrangler only, credential-free opsctl, no auto-provision/auto-create and no automatic restore."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ExecutorGateError as error:
        print(f"D1 executor gate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
