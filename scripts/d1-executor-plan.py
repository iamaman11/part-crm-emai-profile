#!/usr/bin/env python3
"""Materialize exactly the admitted D1 plan for the protected Wrangler executor.

This adapter owns no migration policy. It proves that the fresh remote ledger still equals
the sealed predecessor for an admitted ordinary transaction, that the already-evaluated
plan is the exact contiguous suffix of the typed repository projection, that authorized
SQL content digests still match, and that Wrangler observes the same pending set.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any

MIGRATION_RE = re.compile(r"\b[0-9]{4}_[a-z0-9_]+\.sql\b")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
CONTRACT_REVISION = "0032_pas2_payload_fingerprint_contract.sql"


class PlanAdapterError(ValueError):
    pass


def fail(message: str) -> None:
    raise PlanAdapterError(message)


def strict_object_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load(path: Path, label: str) -> Any:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=strict_object_pairs)
    except json.JSONDecodeError as error:
        raise PlanAdapterError(f"{label} is invalid JSON: {error}") from error


def write_json(path: Path, value: Any) -> None:
    if path.exists() or path.is_symlink():
        fail(f"output already exists: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def migration_name(value: Any, label: str) -> str:
    if not isinstance(value, str) or MIGRATION_RE.fullmatch(value) is None:
        fail(f"{label} is not a canonical migration filename: {value!r}")
    return value


def ledger_names(value: Any) -> list[str]:
    if not isinstance(value, list) or len(value) != 1 or not isinstance(value[0], dict):
        fail("Wrangler D1 ledger must be exactly one result envelope")
    envelope = value[0]
    if envelope.get("success") is not True:
        fail("Wrangler D1 ledger observation did not succeed")
    rows = envelope.get("results")
    if not isinstance(rows, list):
        fail("Wrangler D1 ledger results must be an array")
    names: list[str] = []
    expected_id = 1
    for row in rows:
        if not isinstance(row, dict) or set(row) != {"id", "name"}:
            fail("Wrangler D1 ledger rows must contain exactly id and name")
        if row.get("id") != expected_id:
            fail(f"Wrangler D1 ledger id sequence drifted at {expected_id}: {row!r}")
        names.append(migration_name(row.get("name"), "ledger migration"))
        expected_id += 1
    if len(names) != len(set(names)):
        fail("Wrangler D1 ledger contains duplicate migration names")
    return names


def component_projection(repository: Any, component: str) -> dict[str, Any]:
    if not isinstance(repository, dict):
        fail("typed repository projection must be one object")
    if repository.get("schema_version") != 1 or repository.get("kind") != "D1_REPOSITORY_PROJECTION":
        fail("typed repository projection identity/version drifted")
    components = repository.get("components")
    if not isinstance(components, list):
        fail("typed repository projection has no components array")
    matches = [item for item in components if isinstance(item, dict) and item.get("component_id") == component]
    if len(matches) != 1:
        fail(f"typed repository projection must contain exactly one {component} component")
    return matches[0]


def source_inventory(repo_root: Path, component: dict[str, Any]) -> list[tuple[str, Path]]:
    explicit = component.get("executable_migration_sources")
    inventory: list[tuple[str, Path]] = []
    if explicit is not None:
        if not isinstance(explicit, list) or not explicit:
            fail("executable_migration_sources must be a non-empty array")
        allowed_roots = {"migrations/d1", "migrations/d1-successor", "migrations/resolver-d1"}
        for item in explicit:
            if not isinstance(item, dict) or set(item) != {"migration_file", "source_root"}:
                fail("executable_migration_sources entries must contain exactly migration_file/source_root")
            name = migration_name(item.get("migration_file"), "projected migration")
            source_root = item.get("source_root")
            if source_root not in allowed_roots:
                fail(f"projected migration source root is outside governed authority: {source_root!r}")
            inventory.append((name, repo_root / source_root / name))
    else:
        root_value = component.get("migration_root")
        if not isinstance(root_value, str) or root_value not in {"migrations/d1", "migrations/resolver-d1"}:
            fail("component migration_root is outside governed authority")
        directory = repo_root / root_value
        if directory.is_symlink() or not directory.is_dir():
            fail(f"component migration root is missing/not regular: {directory}")
        for path in sorted(directory.glob("*.sql")):
            if path.is_symlink() or not path.is_file():
                fail(f"migration source is not a regular file: {path}")
            inventory.append((migration_name(path.name, "migration source"), path))
    names = [name for name, _ in inventory]
    if len(names) != len(set(names)):
        fail("typed executable migration inventory contains duplicates")
    expected_count = component.get("migration_count")
    if not isinstance(expected_count, int) or isinstance(expected_count, bool) or expected_count != len(inventory):
        fail("typed executable migration inventory cardinality drifted")
    for name, path in inventory:
        if path.is_symlink() or not path.is_file():
            fail(f"projected migration source is missing/not regular: {name} -> {path}")
    return inventory


def planned_names(plan: Any, mode: str, component: str) -> list[str]:
    if not isinstance(plan, dict):
        fail("native D1 plan must be one object")
    expected_command = "d1 plan" if mode == "ordinary" else "d1 contract-transition"
    if plan.get("schema_version") != 1 or plan.get("command") != expected_command:
        fail(f"native D1 plan command must be {expected_command!r}")
    if plan.get("mode") != "read-only" or plan.get("mutation_executed") is not False:
        fail("native D1 plan lost the read-only effect boundary")
    if plan.get("component") != component or plan.get("allowed") is not True:
        fail("native D1 plan is blocked or belongs to another component")
    values = plan.get("planned_migrations")
    if not isinstance(values, list):
        fail("native D1 plan planned_migrations must be an array")
    names = [migration_name(value, "planned migration") for value in values]
    if len(names) != len(set(names)):
        fail("native D1 plan contains duplicate migrations")
    if mode == "ordinary" and CONTRACT_REVISION in names:
        fail("ordinary d1 plan must never authorize the separate fail-forward CONTRACT")
    if mode == "contract" and (component != "catalog" or names != [CONTRACT_REVISION]):
        fail("contract-transition must authorize exactly the sole Catalog 0032 CONTRACT")
    return names


def planned_digests(plan: Any, planned: list[str], require: bool) -> dict[str, str]:
    if not isinstance(plan, dict):
        fail("native D1 plan must be one object")
    values = plan.get("planned_migration_digests")
    if values is None:
        if require:
            fail("authorized ordinary execution plan must contain planned_migration_digests")
        return {}
    if not isinstance(values, list) or len(values) != len(planned):
        fail("planned_migration_digests cardinality must exactly match planned_migrations")
    result: dict[str, str] = {}
    for index, (item, expected_name) in enumerate(zip(values, planned, strict=True)):
        if not isinstance(item, dict) or set(item) != {"migration", "sha256"}:
            fail("planned_migration_digests entries must contain exactly migration/sha256")
        name = migration_name(item.get("migration"), f"planned migration digest[{index}].migration")
        if name != expected_name:
            fail(
                "planned_migration_digests order/name differs from planned_migrations: "
                f"index={index}, expected={expected_name}, observed={name}"
            )
        digest = item.get("sha256")
        if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
            fail(f"planned migration digest must be canonical lowercase sha256: {digest!r}")
        result[name] = digest
    return result


def sealed_predecessor(plan: Any, require: bool) -> list[str] | None:
    if not isinstance(plan, dict):
        fail("native D1 plan must be one object")
    values = plan.get("predecessor_migrations")
    digest = plan.get("predecessor_ledger_sha256")
    if values is None and digest is None:
        if require:
            fail("authorized ordinary execution plan must contain sealed predecessor state")
        return None
    if not isinstance(values, list):
        fail("predecessor_migrations must be an array")
    names = [migration_name(value, "predecessor migration") for value in values]
    if len(names) != len(set(names)):
        fail("predecessor_migrations contains duplicate migrations")
    if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
        fail("predecessor_ledger_sha256 must be canonical lowercase sha256")
    return names


def materialize(
    repo_root: Path,
    repository_path: Path,
    plan_path: Path,
    ledger_path: Path,
    component_id: str,
    mode: str,
    output_dir: Path,
    expected_pending_path: Path,
    normalized_ledger_path: Path,
    require_plan_digests: bool = False,
    require_sealed_predecessor: bool = False,
) -> None:
    if output_dir.exists() or output_dir.is_symlink():
        fail(f"bounded migrations output must not pre-exist: {output_dir}")
    repository = load(repository_path, "typed repository projection")
    plan = load(plan_path, "native D1 plan")
    remote = ledger_names(load(ledger_path, "Wrangler D1 ledger"))
    component = component_projection(repository, component_id)
    inventory = source_inventory(repo_root, component)
    canonical = [name for name, _ in inventory]
    planned = planned_names(plan, mode, component_id)
    digests = planned_digests(plan, planned, require_plan_digests)
    predecessor = sealed_predecessor(plan, require_sealed_predecessor or require_plan_digests)
    if remote != canonical[: len(remote)]:
        fail("remote ledger is not an exact prefix of the typed executable lineage")
    if predecessor is not None and remote != predecessor:
        fail(
            "fresh remote ledger differs from the sealed authorized predecessor: "
            f"expected={predecessor}, observed={remote}"
        )
    expected_plan = canonical[len(remote) : len(remote) + len(planned)]
    if planned != expected_plan:
        fail(
            "native planned_migrations is not the exact contiguous suffix after the remote ledger: "
            f"planned={planned}, expected={expected_plan}"
        )
    if len(remote) + len(planned) > len(canonical):
        fail("native plan exceeds the typed executable lineage")
    source_by_name = dict(inventory)
    for name in planned:
        expected_digest = digests.get(name)
        if expected_digest is None:
            continue
        observed_digest = hashlib.sha256(source_by_name[name].read_bytes()).hexdigest()
        if observed_digest != expected_digest:
            fail(
                "materialized migration content digest differs from authorized execution plan: "
                f"migration={name}, expected={expected_digest}, observed={observed_digest}"
            )
    output_dir.mkdir(parents=True)
    bounded = remote + planned
    for name in bounded:
        source = source_by_name[name]
        target = output_dir / name
        if target.exists() or target.is_symlink():
            fail(f"duplicate bounded migration materialization: {target}")
        shutil.copyfile(source, target)
    observed = sorted(path.name for path in output_dir.glob("*.sql"))
    if observed != sorted(bounded):
        fail(f"bounded migration directory drifted: expected={sorted(bounded)}, observed={observed}")
    if component_id == "catalog" and mode == "ordinary" and (output_dir / CONTRACT_REVISION).exists():
        fail("ordinary Catalog materialization leaked the trailing 0032 CONTRACT")
    write_json(expected_pending_path, planned)
    write_json(normalized_ledger_path, remote)


def parse_wrangler_pending(text: str) -> list[str]:
    matches = MIGRATION_RE.findall(text)
    if len(matches) != len(set(matches)):
        fail(f"Wrangler pending-list output is ambiguous/duplicated: {matches}")
    return matches


def verify_pending(expected_path: Path, wrangler_output_path: Path) -> None:
    expected_value = load(expected_path, "expected pending migrations")
    if not isinstance(expected_value, list):
        fail("expected pending migrations must be an array")
    expected = [migration_name(value, "expected pending migration") for value in expected_value]
    if wrangler_output_path.is_symlink() or not wrangler_output_path.is_file():
        fail(f"Wrangler pending-list output must be a regular file: {wrangler_output_path}")
    observed = parse_wrangler_pending(wrangler_output_path.read_text(encoding="utf-8"))
    if observed != expected:
        fail(f"Wrangler pending list differs from native planned_migrations: expected={expected}, observed={observed}")


def normalize_ledger(ledger_path: Path, output_path: Path) -> None:
    write_json(output_path, ledger_names(load(ledger_path, "Wrangler D1 ledger")))


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="d1-executor-plan-") as directory:
        root = Path(directory)
        (root / "migrations/d1").mkdir(parents=True)
        (root / "migrations/d1-successor").mkdir(parents=True)
        files = [
            ("0001_base.sql", "migrations/d1"),
            ("0002_expand.sql", "migrations/d1-successor"),
            (CONTRACT_REVISION, "migrations/d1-successor"),
        ]
        for name, source_root in files:
            (root / source_root / name).write_text(f"-- {name}\n", encoding="utf-8")
        repository = {
            "schema_version": 1,
            "kind": "D1_REPOSITORY_PROJECTION",
            "components": [{
                "component_id": "catalog",
                "migration_count": 3,
                "executable_migration_sources": [
                    {"migration_file": name, "source_root": source_root}
                    for name, source_root in files
                ],
            }],
        }
        ledger = [{"success": True, "results": [{"id": 1, "name": "0001_base.sql"}]}]
        expand_digest = hashlib.sha256((root / "migrations/d1-successor/0002_expand.sql").read_bytes()).hexdigest()
        ordinary_plan = {
            "schema_version": 1,
            "command": "d1 plan",
            "mode": "read-only",
            "mutation_executed": False,
            "component": "catalog",
            "allowed": True,
            "predecessor_ledger_sha256": "1" * 64,
            "predecessor_migrations": ["0001_base.sql"],
            "planned_migrations": ["0002_expand.sql"],
            "planned_migration_digests": [{"migration": "0002_expand.sql", "sha256": expand_digest}],
        }
        for name, value in (("repository.json", repository), ("ledger.json", ledger), ("plan.json", ordinary_plan)):
            (root / name).write_text(json.dumps(value), encoding="utf-8")
        materialize(
            root,
            root / "repository.json",
            root / "plan.json",
            root / "ledger.json",
            "catalog",
            "ordinary",
            root / "bounded",
            root / "expected.json",
            root / "ledger-names.json",
            True,
            True,
        )
        if sorted(path.name for path in (root / "bounded").glob("*.sql")) != ["0001_base.sql", "0002_expand.sql"]:
            fail("self-test ordinary materialization was not bounded")
        (root / "wrangler.txt").write_text("Migrations to be applied:\n0002_expand.sql\n", encoding="utf-8")
        verify_pending(root / "expected.json", root / "wrangler.txt")

        leaked = dict(ordinary_plan)
        leaked["planned_migrations"] = ["0002_expand.sql", CONTRACT_REVISION]
        try:
            planned_names(leaked, "ordinary", "catalog")
        except PlanAdapterError:
            pass
        else:
            fail("self-test accepted CONTRACT leakage through ordinary d1 plan")

        missing_digest = dict(ordinary_plan)
        missing_digest.pop("planned_migration_digests")
        (root / "missing-digest.json").write_text(json.dumps(missing_digest), encoding="utf-8")
        try:
            materialize(
                root,
                root / "repository.json",
                root / "missing-digest.json",
                root / "ledger.json",
                "catalog",
                "ordinary",
                root / "missing-digest-bounded",
                root / "missing-digest-expected.json",
                root / "missing-digest-ledger.json",
                True,
                True,
            )
        except PlanAdapterError:
            pass
        else:
            fail("self-test accepted missing authorized planned_migration_digests")

        wrong_digest = dict(ordinary_plan)
        wrong_digest["planned_migration_digests"] = [{"migration": "0002_expand.sql", "sha256": "0" * 64}]
        (root / "wrong-digest.json").write_text(json.dumps(wrong_digest), encoding="utf-8")
        try:
            materialize(
                root,
                root / "repository.json",
                root / "wrong-digest.json",
                root / "ledger.json",
                "catalog",
                "ordinary",
                root / "wrong-digest-bounded",
                root / "wrong-digest-expected.json",
                root / "wrong-digest-ledger.json",
                True,
                True,
            )
        except PlanAdapterError:
            pass
        else:
            fail("self-test accepted migration content digest drift from authorized execution plan")

        missing_predecessor = dict(ordinary_plan)
        missing_predecessor.pop("predecessor_migrations")
        missing_predecessor.pop("predecessor_ledger_sha256")
        (root / "missing-predecessor.json").write_text(json.dumps(missing_predecessor), encoding="utf-8")
        try:
            materialize(
                root,
                root / "repository.json",
                root / "missing-predecessor.json",
                root / "ledger.json",
                "catalog",
                "ordinary",
                root / "missing-predecessor-bounded",
                root / "missing-predecessor-expected.json",
                root / "missing-predecessor-ledger.json",
                True,
                False,
            )
        except PlanAdapterError:
            pass
        else:
            fail("self-test accepted ordinary execution plan without sealed predecessor state")

        no_op_plan = dict(ordinary_plan)
        no_op_plan["planned_migrations"] = []
        no_op_plan["planned_migration_digests"] = []
        (root / "no-op.json").write_text(json.dumps(no_op_plan), encoding="utf-8")
        drifted_ledger = [{"success": True, "results": [
            {"id": 1, "name": "0001_base.sql"},
            {"id": 2, "name": "0002_expand.sql"},
        ]}]
        (root / "drifted-ledger.json").write_text(json.dumps(drifted_ledger), encoding="utf-8")
        try:
            materialize(
                root,
                root / "repository.json",
                root / "no-op.json",
                root / "drifted-ledger.json",
                "catalog",
                "ordinary",
                root / "drifted-no-op-bounded",
                root / "drifted-no-op-expected.json",
                root / "drifted-no-op-ledger.json",
                True,
                False,
            )
        except PlanAdapterError:
            pass
        else:
            fail("self-test accepted fresh ledger drift for a sealed no-op transaction")

        (root / "wrong.txt").write_text(f"{CONTRACT_REVISION}\n", encoding="utf-8")
        try:
            verify_pending(root / "expected.json", root / "wrong.txt")
        except PlanAdapterError:
            pass
        else:
            fail("self-test accepted Wrangler/native pending-list disagreement")
    print("D1 executor exact-plan adapter self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subparsers = result.add_subparsers(dest="command", required=True)
    materialize_parser = subparsers.add_parser("materialize")
    materialize_parser.add_argument("--repo-root", type=Path, required=True)
    materialize_parser.add_argument("--repository", type=Path, required=True)
    materialize_parser.add_argument("--plan", type=Path, required=True)
    materialize_parser.add_argument("--ledger", type=Path, required=True)
    materialize_parser.add_argument("--component", choices=("catalog", "resolver"), required=True)
    materialize_parser.add_argument("--mode", choices=("ordinary", "contract"), required=True)
    materialize_parser.add_argument("--output-dir", type=Path, required=True)
    materialize_parser.add_argument("--expected-pending", type=Path, required=True)
    materialize_parser.add_argument("--normalized-ledger", type=Path, required=True)
    materialize_parser.add_argument("--require-plan-digests", action="store_true")
    materialize_parser.add_argument("--require-sealed-predecessor", action="store_true")
    pending_parser = subparsers.add_parser("verify-pending")
    pending_parser.add_argument("--expected", type=Path, required=True)
    pending_parser.add_argument("--wrangler-output", type=Path, required=True)
    ledger_parser = subparsers.add_parser("normalize-ledger")
    ledger_parser.add_argument("--ledger", type=Path, required=True)
    ledger_parser.add_argument("--output", type=Path, required=True)
    subparsers.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "materialize":
            materialize(
                args.repo_root,
                args.repository,
                args.plan,
                args.ledger,
                args.component,
                args.mode,
                args.output_dir,
                args.expected_pending,
                args.normalized_ledger,
                args.require_plan_digests,
                args.require_sealed_predecessor,
            )
        elif args.command == "verify-pending":
            verify_pending(args.expected, args.wrangler_output)
        elif args.command == "normalize-ledger":
            normalize_ledger(args.ledger, args.output)
        elif args.command == "self-test":
            self_test()
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except (OSError, PlanAdapterError) as error:
        print(f"D1 executor exact-plan adapter error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
