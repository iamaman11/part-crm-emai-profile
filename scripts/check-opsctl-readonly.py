#!/usr/bin/env python3
"""Fail closed if Rust opsctl or its AR-9 D1 authority gains unreviewed capability."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
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
EXPECTED_DECISIONS = {
    "SAFE",
    "MIGRATION_REQUIRED",
    "DEPLOY_FIRST",
    "MIGRATE_FIRST",
    "CODE_ROLLBACK_SAFE",
    "CODE_ROLLBACK_BLOCKED",
    "FAIL_FORWARD_REQUIRED",
    "CONTRACT_BLOCKED",
    "RECOVERY_REQUIRED",
}
EXPECTED_ROLLOUT_ORDERS = {
    "MIGRATE_BEFORE_CODE",
    "CODE_BEFORE_MIGRATE",
    "EITHER",
    "SEPARATE_CONTRACT_RELEASE",
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
    "architecture/ar8-completion-lifecycle.json",
    "architecture/ar8-operator-rehearsal.json",
)
REQUIRED_SUBJECT_PATHS = (
    "architecture/credential-authority.json",
    "architecture/credential-lifecycle.json",
    "architecture/profile-security.json",
    "architecture/operator-contract.json",
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
    if "pub mod d1;" not in lib:
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
        "--current-manifest",
        "--known-good-manifest",
        "--preconditions-json",
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
        '"DEPLOY_FIRST"',
        '"MIGRATE_FIRST"',
        '"FAIL_FORWARD_REQUIRED"',
        '"CONTRACT_BLOCKED"',
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


def verify_lockfile_reproducible(root: Path) -> None:
    primed = subprocess.run(
        [
            "cargo",
            "fetch",
            "--locked",
            "--manifest-path",
            str(root / CARGO),
        ],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if primed.returncode != 0:
        detail = primed.stderr.strip() or primed.stdout.strip()
        fail(f"cannot prime exact standalone opsctl dependency cache from Cargo.lock: {detail}")

    with tempfile.TemporaryDirectory(prefix="opsctl-lock-") as temporary:
        workspace = Path(temporary)
        shutil.copyfile(root / CARGO, workspace / "Cargo.toml")
        (workspace / "src").mkdir()
        (workspace / "src" / "main.rs").write_text("fn main() {}\n", encoding="utf-8")
        completed = subprocess.run(
            [
                "cargo",
                "generate-lockfile",
                "--offline",
                "--manifest-path",
                str(workspace / "Cargo.toml"),
            ],
            cwd=root,
            text=True,
            capture_output=True,
            check=False,
        )
        if completed.returncode != 0:
            detail = completed.stderr.strip() or completed.stdout.strip()
            fail(f"cannot reproduce standalone opsctl Cargo.lock offline: {detail}")
        expected = (workspace / "Cargo.lock").read_text(encoding="utf-8")
        observed = read(root, LOCK)
        if observed != expected:
            fail(f"standalone opsctl Cargo.lock is not reproducible; expected_lock={expected!r}")


def canonical_entries(root: Path, migration_root: str) -> list[dict[str, str]]:
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
    return entries


def identity_digest(entries: list[dict[str, str]]) -> str:
    canonical = json.dumps(entries, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


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


def require_bool(record: dict[str, Any], field: str, label: str) -> bool:
    value = record.get(field)
    if not isinstance(value, bool):
        fail(f"{label}.{field} must be boolean")
    return value


def require_text(record: dict[str, Any], field: str, label: str) -> str:
    value = record.get(field)
    if not isinstance(value, str) or not value:
        fail(f"{label}.{field} must be a non-empty string")
    return value


def require_text_list(record: dict[str, Any], field: str, label: str, *, nonempty: bool = False) -> list[str]:
    value = record.get(field)
    if not isinstance(value, list) or (nonempty and not value):
        fail(f"{label}.{field} must be {'a non-empty' if nonempty else 'an'} array")
    if any(not isinstance(item, str) or not item for item in value):
        fail(f"{label}.{field} must contain non-empty strings only")
    if len(value) != len(set(value)):
        fail(f"{label}.{field} must not contain duplicates")
    return list(value)


def derived_rollout(record: dict[str, Any]) -> str:
    migration_class = require_text(record, "migration_class", "migration")
    old_after = require_bool(record, "old_runtime_compatible_after", "migration")
    new_before = require_bool(record, "new_runtime_compatible_before", "migration")
    fail_forward = require_bool(record, "fail_forward_required", "migration")
    if migration_class == "CONTRACT":
        return "SEPARATE_CONTRACT_RELEASE"
    if old_after and new_before:
        return "EITHER"
    if old_after and not new_before:
        return "MIGRATE_BEFORE_CODE"
    if not old_after and new_before:
        return "CODE_BEFORE_MIGRATE"
    if migration_class == "REPAIR" and fail_forward:
        return "SEPARATE_CONTRACT_RELEASE"
    fail("migration compatibility flags have no safe derived rollout order")
    raise AssertionError("unreachable")


def validate_post_epoch_records(
    component_id: str,
    records: list[Any],
    actual_tail: list[dict[str, str]],
    required_fields: set[str],
    failure_modes: set[str],
    recovery_modes: set[str],
    precondition_vocabulary: set[str],
) -> None:
    if len(records) != len(actual_tail):
        fail(
            f"{component_id} post-epoch contract count differs from appended SQL: "
            f"contracts={len(records)} files={len(actual_tail)}"
        )
    prior_classes: list[str] = []
    for index, (raw, actual) in enumerate(zip(records, actual_tail, strict=True)):
        label = f"{component_id}.post_epoch_migrations[{index}]"
        if not isinstance(raw, dict):
            fail(f"{label} must be an object")
        if not required_fields.issubset(raw):
            fail(f"{label} missing required fields: {sorted(required_fields - set(raw))}")
        if raw.get("component") != component_id:
            fail(f"{label}.component mismatch")
        if raw.get("migration_file") != actual["name"] or raw.get("migration_revision") != actual["name"]:
            fail(f"{label} migration identity differs from exact append order")
        if raw.get("sha256") != actual["sha256"]:
            fail(f"{label}.sha256 differs from exact SQL bytes")
        migration_class = require_text(raw, "migration_class", label)
        if migration_class not in EXPECTED_MIGRATION_CLASSES:
            fail(f"{label} uses unknown migration class: {migration_class}")
        rollout = require_text(raw, "rollout_order", label)
        expected_rollout = derived_rollout(raw)
        if rollout != expected_rollout:
            fail(f"{label}.rollout_order must be derived as {expected_rollout}, observed={rollout}")
        if rollout not in EXPECTED_ROLLOUT_ORDERS:
            fail(f"{label}.rollout_order is unknown")
        backfill_required = require_bool(raw, "backfill_required", label)
        backfill_authority = raw.get("backfill_authority")
        backfill_predicate = raw.get("backfill_completion_predicate")
        if backfill_required:
            if not isinstance(backfill_authority, str) or not backfill_authority:
                fail(f"{label} requires explicit backfill_authority")
            if not isinstance(backfill_predicate, str) or not backfill_predicate:
                fail(f"{label} requires explicit backfill_completion_predicate")
        invariants = require_text_list(raw, "verification_invariants", label, nonempty=True)
        if not invariants:
            fail(f"{label} must define verification invariants")
        failure_mode = require_text(raw, "failure_mode", label)
        recovery_mode = require_text(raw, "recovery_mode", label)
        if failure_mode not in failure_modes:
            fail(f"{label}.failure_mode is unknown: {failure_mode}")
        if recovery_mode not in recovery_modes:
            fail(f"{label}.recovery_mode is unknown: {recovery_mode}")
        code_rollback_allowed = require_bool(raw, "code_rollback_allowed", label)
        fail_forward_required = require_bool(raw, "fail_forward_required", label)
        destructive = require_bool(raw, "destructive", label)
        preconditions = require_text_list(raw, "contract_preconditions", label)
        if not set(preconditions).issubset(precondition_vocabulary):
            fail(f"{label} contains unknown contract precondition")
        if destructive and code_rollback_allowed:
            fail(f"{label} destructive migration cannot be marked code-rollback-safe")
        if fail_forward_required and not (
            failure_mode == "FAIL_FORWARD_ONLY" and recovery_mode == "FAIL_FORWARD"
        ):
            fail(f"{label} fail-forward migration requires FAIL_FORWARD_ONLY + FAIL_FORWARD")
        if migration_class == "CONTRACT":
            required_contract = {
                "replacement_active",
                "backfill_complete",
                "old_readers_writers_retired",
                "known_good_compatible",
            }
            if not required_contract.issubset(preconditions):
                fail(f"{label} CONTRACT is missing mechanical preconditions")
            if "EXPAND" not in prior_classes:
                fail(f"{label} CONTRACT has no prior post-epoch EXPAND migration")
        if migration_class == "REPAIR":
            for field in ("repair_reason", "bad_state_predicate", "target_invariant"):
                require_text(raw, field, label)
        prior_classes.append(migration_class)


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
    if set(global_policy.get("rollout_decisions", [])) != EXPECTED_DECISIONS:
        fail("D1 rollout-decision vocabulary drifted")
    failure_modes = set(global_policy.get("failure_modes", []))
    recovery_modes = set(global_policy.get("rollback_authority", []))
    if not failure_modes or not recovery_modes:
        fail("D1 failure/recovery vocabularies must be explicit")
    if global_policy.get("new_opsctl_process_spawn_sites") != 0:
        fail("D1 authority must require zero new opsctl process-spawn sites")
    if global_policy.get("opsctl_provider_credentials") is not False:
        fail("D1 authority must forbid provider credentials in opsctl")
    if global_policy.get("database_lock_required_by_default") is not False:
        fail("D1 authority must not invent a default DB lock")
    if global_policy.get("resource_auto_provisioning_allowed") is not False:
        fail("D1 authority must forbid automatic resource provisioning")

    contract_authority = payload.get("new_migration_contract")
    if not isinstance(contract_authority, dict):
        fail("new migration contract authority is missing")
    required_fields = set(contract_authority.get("required_fields", []))
    if not required_fields:
        fail("new migration contract required_fields must be explicit")
    if set(contract_authority.get("rollout_order_vocabulary", [])) != EXPECTED_ROLLOUT_ORDERS:
        fail("migration rollout-order vocabulary drifted")
    if contract_authority.get("rollout_order_is_derived") is not True:
        fail("migration rollout_order must remain a derived property")
    precondition_vocabulary = set(contract_authority.get("contract_precondition_vocabulary", []))
    if not precondition_vocabulary:
        fail("CONTRACT precondition vocabulary is missing")

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

        actual = canonical_entries(root, migration_root)
        historical_count = len(ordered)
        if historical_count > len(actual):
            fail(f"{component_id} frozen history exceeds actual migration history")
        expected_frozen = actual[:historical_count]
        observed_identity: list[dict[str, str]] = []
        for entry in ordered:
            if not isinstance(entry, dict) or not isinstance(entry.get("name"), str):
                fail(f"{component_id} historical entry is malformed")
            blob = entry.get("git_blob_sha1")
            if not isinstance(blob, str) or GIT_BLOB_SHA1_RE.fullmatch(blob) is None:
                fail(f"{component_id} historical entry has malformed Git blob identity: {entry.get('name')}")
            sha256 = entry.get("sha256")
            if not isinstance(sha256, str) or SHA256_RE.fullmatch(sha256) is None:
                fail(f"{component_id} historical entry has malformed SHA-256: {entry.get('name')}")
            observed_identity.append({"name": entry["name"], "sha256": sha256})
        if observed_identity != expected_frozen:
            fail(f"{component_id} frozen epoch differs from exact historical SQL bytes/order")

        frozen_digest = identity_digest(expected_frozen)
        freeze = historical.get("per_file_sha256_freeze")
        if not (
            historical.get("ordered_set_identity_algorithm") == "sha256(canonical-json(name+sha256))"
            and historical.get("ordered_set_identity") == frozen_digest
            and isinstance(freeze, dict)
            and freeze.get("status") == "FROZEN"
            and freeze.get("algorithm") == "sha256"
            and freeze.get("count") == historical_count
        ):
            fail(f"{component_id} frozen epoch digest/status is incomplete or stale")
        if historical.get("retroactive_runtime_compatibility_claims") is not False:
            fail(f"{component_id} historical epoch must not invent retroactive compatibility claims")

        post_epoch = component.get("post_epoch_migrations")
        if not isinstance(post_epoch, list):
            fail(f"{component_id} post_epoch_migrations must be explicit")
        validate_post_epoch_records(
            component_id,
            post_epoch,
            actual[historical_count:],
            required_fields,
            failure_modes,
            recovery_modes,
            precondition_vocabulary,
        )
        full_digest = identity_digest(actual)
        if component.get("history_digest_algorithm") != "sha256(canonical-json(name+sha256))":
            fail(f"{component_id} full history digest algorithm drifted")
        if component.get("history_digest") != full_digest:
            fail(f"{component_id} full history digest is stale")
        if component.get("current_repository_revision") != actual[-1]["name"]:
            fail(f"{component_id} current repository revision must equal final canonical migration")


def validate(root: Path = ROOT) -> None:
    validate_source(
        read(root, LIB),
        read(root, D1),
        read(root, MAIN),
        read(root, CARGO),
        read(root, LOCK),
    )
    verify_lockfile_reproducible(root)
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

    synthetic_actual = [{"name": "0005_expand.sql", "sha256": "a" * 64}]
    valid_expand = {
        "component": "resolver",
        "migration_file": "0005_expand.sql",
        "migration_revision": "0005_expand.sql",
        "sha256": "a" * 64,
        "migration_class": "EXPAND",
        "old_runtime_compatible_after": True,
        "new_runtime_compatible_before": True,
        "rollout_order": "EITHER",
        "backfill_required": False,
        "backfill_authority": "NONE",
        "backfill_completion_predicate": "NOT_REQUIRED",
        "verification_invariants": ["new representation exists"],
        "failure_mode": "RETRY_SAFE",
        "recovery_mode": "CODE_ROLLBACK",
        "code_rollback_allowed": True,
        "fail_forward_required": False,
        "contract_preconditions": [],
        "destructive": False,
    }
    required_fields = set(authority["new_migration_contract"]["required_fields"])
    validate_post_epoch_records(
        "resolver",
        [valid_expand],
        synthetic_actual,
        required_fields,
        set(authority["global_policy"]["failure_modes"]),
        set(authority["global_policy"]["rollback_authority"]),
        set(authority["new_migration_contract"]["contract_precondition_vocabulary"]),
    )
    invalid_rollout = dict(valid_expand)
    invalid_rollout["rollout_order"] = "CODE_BEFORE_MIGRATE"
    try:
        validate_post_epoch_records(
            "resolver",
            [invalid_rollout],
            synthetic_actual,
            required_fields,
            set(authority["global_policy"]["failure_modes"]),
            set(authority["global_policy"]["rollback_authority"]),
            set(authority["new_migration_contract"]["contract_precondition_vocabulary"]),
        )
    except GateError:
        pass
    else:
        fail("contradictory derived rollout negative fixture unexpectedly passed")

    print(
        "opsctl legacy bridge, native D1 subprocess/mutation, append-only history, derived rollout, "
        "historical-freeze and dependency negative fixtures rejected as expected."
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
            "semantics, frozen+append-only D1 histories, reproducible exact serde_json dependency, "
            "no provider/mutation capability."
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except GateError as error:
        print(f"opsctl gate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
