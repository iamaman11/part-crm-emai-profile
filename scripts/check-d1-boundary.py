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
D1_PLAN = Path("tools/opsctl/src/d1/plan.rs")
STANDARD_OPERATOR = Path(".github/workflows/d1-operator.yml")
COMMENT_ROUTER = Path(".github/workflows/d1-operator-comment-router.yml")
READ_ONLY_OBSERVER = Path(".github/workflows/d1-isolated-target-observation.yml")
MIGRATION_PREPARE = Path(".github/workflows/d1-migration-prepare.yml")
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
    "Pre-Production canonical convergence",
    "pre-Production historical compatibility obligation = 0",
    "PREPROD_REMEDIATION_TARGET = CURRENT_CANONICAL_DESIRED_STATE",
    "CURRENT_TARGET_CANONICAL_FORWARD_CONVERGENCE",
)

CANONICAL_FORWARD_MARKERS = (
    "CURRENT_TARGET_CANONICAL_FORWARD_CONVERGENCE",
    "same_schema_contract(current, target)",
    "canonical_forward_convergence_set(&contracts)",
    "contract.migration_class != MigrationClass::Contract",
    "!contract.destructive",
    "!contract.fail_forward_required",
    "contract.rollout_order != RolloutOrder::SeparateContractRelease",
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
DIRECT_STANDARD_OPERATOR_DISPATCH = re.compile(
    r"actions/workflows/d1-operator\.yml/dispatches"
)


def dispatches_workflow(source: str, workflow_name: str) -> bool:
    direct = f"actions/workflows/{workflow_name}/dispatches"
    if direct in source:
        return True

    assignments = re.findall(
        rf"\b([A-Z][A-Z0-9_]*)\s*=\s*['\"]{re.escape(workflow_name)}['\"]",
        source,
    )
    return any(
        f"actions/workflows/{{{variable}}}/dispatches" in source
        for variable in assignments
    )


def read_text(root: Path, relative: Path, errors: list[str]) -> str:
    path = root / relative
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        errors.append(f"{relative}: missing/unreadable permanent D1 owner: {error}")
        return ""


def workflow_dispatch_block(source: str) -> str | None:
    trigger = source.split("\npermissions:", 1)[0]
    marker = "\n  workflow_dispatch:"
    if marker not in trigger:
        return None
    return trigger.split(marker, 1)[1]


def check_canonical_forward_convergence(source: str) -> list[str]:
    """Protect the narrow current-target forward-convergence exception."""

    errors: list[str] = []
    for marker in CANONICAL_FORWARD_MARKERS:
        if marker not in source:
            errors.append(
                f"{D1_PLAN}: D1_CANONICAL_FORWARD_CONVERGENCE_MARKER_MISSING: {marker}"
            )

    start = source.find("    let current_supports_remote = runtime_supports_remote")
    special = source.find("CURRENT_TARGET_CANONICAL_FORWARD_CONVERGENCE")
    known_good = source.find("    let Some(known_good) = known_good else", max(start, 0))
    recovery = source.find("CURRENT_RUNTIME_ALREADY_SCHEMA_INCOMPATIBLE", max(known_good, 0))
    if min(start, special, known_good, recovery) < 0 or not (
        start < special < known_good < recovery
    ):
        errors.append(
            f"{D1_PLAN}: D1_CANONICAL_FORWARD_CONVERGENCE_ORDER_DRIFT"
        )
    elif "known_good" in source[start:known_good]:
        errors.append(
            f"{D1_PLAN}: D1_CANONICAL_FORWARD_CONVERGENCE_MUST_NOT_SELECT_HISTORICAL_KNOWN_GOOD"
        )

    return errors


def prove_canonical_forward_negative_cases(source: str) -> list[str]:
    """Same-checker mutations prove that exact-target and safety fences are mandatory."""

    errors: list[str] = []
    for marker, label in (
        ("same_schema_contract(current, target)", "current-target identity"),
        ("!contract.destructive", "destructive migration fence"),
        ("!contract.fail_forward_required", "fail-forward fence"),
        (
            "contract.migration_class != MigrationClass::Contract",
            "CONTRACT migration fence",
        ),
    ):
        if marker not in source:
            continue
        mutated = source.replace(marker, "true /* negative fixture removed fence */", 1)
        fixture_errors = check_canonical_forward_convergence(mutated)
        if not fixture_errors:
            errors.append(
                "D1 canonical forward-convergence negative fixture failed: "
                f"{label} removal was not rejected"
            )
    return errors


def check_operator_topology(workflows: dict[Path, str]) -> list[str]:
    """Protect the current D1 operator graph, not historical stage names."""

    errors: list[str] = []
    operator = workflows.get(STANDARD_OPERATOR, "")
    router = workflows.get(COMMENT_ROUTER, "")

    dispatch = workflow_dispatch_block(operator)
    if dispatch is None:
        errors.append(
            f"{STANDARD_OPERATOR}: D1_OPERATOR_TOPOLOGY_MISSING_STANDARD_DISPATCH"
        )
    elif "inputs:" in dispatch:
        errors.append(
            f"{STANDARD_OPERATOR}: D1_OPERATOR_TOPOLOGY_MANUAL_MACHINE_INPUTS_FORBIDDEN"
        )

    ordinary_route_marker = (
        "- name: Dispatch zero-input standard D1 operator as repository owner"
    )
    if ordinary_route_marker not in router:
        errors.append(
            f"{COMMENT_ROUTER}: D1_OPERATOR_TOPOLOGY_MISSING_ZERO_INPUT_COMMENT_ROUTE"
        )
    else:
        ordinary_route = router.split(ordinary_route_marker, 1)[1].split(
            "\n      - name:", 1
        )[0]
        if (
            "\"repos/$GITHUB_REPOSITORY/actions/workflows/d1-operator.yml/dispatches\""
            not in ordinary_route
            or "-f ref=main" not in ordinary_route
            or "--input" in ordinary_route
        ):
            errors.append(
                f"{COMMENT_ROUTER}: D1_OPERATOR_TOPOLOGY_COMMENT_ROUTE_MUST_BE_ZERO_INPUT"
            )

    if "github.event.comment.body == '/d1 operator'" not in router:
        errors.append(
            f"{COMMENT_ROUTER}: D1_OPERATOR_TOPOLOGY_COMMENT_COMMAND_MUST_BE_EXACT"
        )

    standard_operator_dispatchers = sorted(
        path
        for path, text in workflows.items()
        if DIRECT_STANDARD_OPERATOR_DISPATCH.search(text)
    )
    if standard_operator_dispatchers != [COMMENT_ROUTER]:
        errors.append(
            "D1_OPERATOR_TOPOLOGY_STANDARD_OPERATOR_DISPATCHERS "
            f"expected={[COMMENT_ROUTER]} observed={standard_operator_dispatchers}"
        )

    prepare_dispatchers = sorted(
        path
        for path, text in workflows.items()
        if dispatches_workflow(text, MIGRATION_PREPARE.name)
    )
    if prepare_dispatchers != [STANDARD_OPERATOR]:
        errors.append(
            "D1_OPERATOR_TOPOLOGY_PREPARE_DISPATCHERS "
            f"expected={[STANDARD_OPERATOR]} observed={prepare_dispatchers}"
        )

    observer_dispatchers = sorted(
        path
        for path, text in workflows.items()
        if dispatches_workflow(text, READ_ONLY_OBSERVER.name)
    )
    expected_observer_dispatchers = sorted([STANDARD_OPERATOR, COMMENT_ROUTER])
    if observer_dispatchers != expected_observer_dispatchers:
        errors.append(
            "D1_OPERATOR_TOPOLOGY_OBSERVER_DISPATCHERS "
            f"expected={expected_observer_dispatchers} observed={observer_dispatchers}"
        )

    executor_dispatchers = sorted(
        path
        for path, text in workflows.items()
        if dispatches_workflow(text, MIGRATION_EXECUTOR.name)
    )
    expected_executor_dispatchers = sorted([STANDARD_OPERATOR, COMMENT_ROUTER])
    if executor_dispatchers != expected_executor_dispatchers:
        errors.append(
            "D1_OPERATOR_TOPOLOGY_EXECUTOR_DISPATCHERS "
            f"expected={expected_executor_dispatchers} observed={executor_dispatchers}"
        )

    restore_marker = "- name: Dispatch one-shot Time Travel restore through sole D1 mutation owner"
    if restore_marker not in router:
        errors.append(
            f"{COMMENT_ROUTER}: D1_OPERATOR_TOPOLOGY_MISSING_BOUNDED_RECOVERY_ROUTE"
        )
    else:
        restore_route = router.split(restore_marker, 1)[1].split(
            "\n      - name:", 1
        )[0]
        if (
            "operation_mode:\"time_travel_restore\"" not in restore_route
            or "d1-migration-executor.yml/dispatches" not in restore_route
            or "D1_TIME_TRAVEL_RESTORE_AUTHORIZATION" in restore_route
        ):
            errors.append(
                f"{COMMENT_ROUTER}: D1_OPERATOR_TOPOLOGY_RECOVERY_ROUTE_DRIFT"
            )

    return errors


def prove_operator_topology_negative_cases(workflows: dict[Path, str]) -> list[str]:
    """Run small same-checker negative fixtures so the topology guard cannot silently weaken."""

    errors: list[str] = []

    second_router = dict(workflows)
    second_router[Path(".github/workflows/fixture-second-d1-router.yml")] = (
        "name: forbidden second D1 router\n"
        "on:\n  workflow_dispatch:\n"
        "jobs:\n  route:\n    steps:\n"
        "      - run: gh api --method POST "
        "\"repos/$GITHUB_REPOSITORY/actions/workflows/d1-migration-prepare.yml/dispatches\"\n"
    )
    second_router_errors = check_operator_topology(second_router)
    if not any(
        "D1_OPERATOR_TOPOLOGY_PREPARE_DISPATCHERS" in error
        for error in second_router_errors
    ):
        errors.append(
            "D1 operator topology negative fixture failed: second prepare router was not rejected"
        )

    manual_input = dict(workflows)
    source = manual_input.get(STANDARD_OPERATOR, "")
    expected = "  workflow_dispatch:\n\npermissions:"
    if expected not in source:
        errors.append(
            "D1 operator topology negative fixture unavailable: standard zero-input dispatch marker missing"
        )
    else:
        manual_input[STANDARD_OPERATOR] = source.replace(
            expected,
            "  workflow_dispatch:\n"
            "    inputs:\n"
            "      transaction_id:\n"
            "        required: true\n"
            "        type: string\n\n"
            "permissions:",
            1,
        )
        manual_input_errors = check_operator_topology(manual_input)
        if not any(
            "D1_OPERATOR_TOPOLOGY_MANUAL_MACHINE_INPUTS_FORBIDDEN" in error
            for error in manual_input_errors
        ):
            errors.append(
                "D1 operator topology negative fixture failed: manual machine-known input was not rejected"
            )

    second_operator_transport = dict(workflows)
    second_operator_transport[Path(".github/workflows/fixture-second-operator-transport.yml")] = (
        "name: forbidden second operator transport\n"
        "on:\n  issue_comment:\n    types: [created]\n"
        "jobs:\n  route:\n    steps:\n"
        "      - run: gh api --method POST "
        "\"repos/$GITHUB_REPOSITORY/actions/workflows/d1-operator.yml/dispatches\" -f ref=main\n"
    )
    second_operator_errors = check_operator_topology(second_operator_transport)
    if not any(
        "D1_OPERATOR_TOPOLOGY_STANDARD_OPERATOR_DISPATCHERS" in error
        for error in second_operator_errors
    ):
        errors.append(
            "D1 operator topology negative fixture failed: second operator transport was not rejected"
        )

    return errors


def check_operations_contract(root: Path) -> list[str]:
    errors: list[str] = []
    contract = read_text(root, D1_CONTRACT, errors)
    index = read_text(root, D1_INDEX, errors)
    plan = read_text(root, D1_PLAN, errors)
    operator = read_text(root, STANDARD_OPERATOR, errors)
    executor = read_text(root, MIGRATION_EXECUTOR, errors)

    for marker in D1_CONTRACT_MARKERS:
        if marker not in contract:
            errors.append(f"{D1_CONTRACT}: permanent D1 operations marker missing: {marker}")

    errors.extend(check_canonical_forward_convergence(plan))
    errors.extend(prove_canonical_forward_negative_cases(plan))

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
    workflows: dict[Path, str] = {}
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
        workflows[relative] = text
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

    errors.extend(check_operator_topology(workflows))
    errors.extend(prove_operator_topology_negative_cases(workflows))

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