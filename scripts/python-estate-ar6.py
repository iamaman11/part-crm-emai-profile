#!/usr/bin/env python3
"""Validate the frozen AR-6 Python estate plus explicitly owned later deltas.

AR-6 established the complete Git-tracked Python classification baseline. Later
architecture slices may add or retire Python only through a bounded machine-readable
overlay; they may not silently rewrite the accepted AR-6 baseline.
"""

from __future__ import annotations

import argparse
import ast
import copy
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "architecture" / "python-estate-ar6.json"
AR10_OVERLAY = ROOT / "architecture" / "python-estate-ar10.json"
AR11_OVERLAY = ROOT / "architecture" / "python-estate-ar11.json"
SCHEMA_VERSION = 1
ALLOWED = {"KEEP_PYTHON", "MIGRATE_TO_RUST", "WRAP_WITH_RUST", "DELETE_AFTER_SEQUENCE"}
FUTURE_FIELDS = {
    "rust_target",
    "artifact_type",
    "cutover_slice",
    "compatibility_requirement",
    "retirement_proof",
}
EXPLICIT_ONLY_CAPABILITIES = {
    "browser_runtime",
    "network_io",
    "provider_mutation_subprocess",
    "secret_environment_access",
}
NETWORK_IMPORT_PREFIXES = (
    "aiohttp",
    "http.client",
    "httpx",
    "requests",
    "socket",
    "urllib.request",
)
BROWSER_IMPORT_PREFIXES = ("camoufox", "playwright")
SECRET_ENV_MARKERS = (
    "ACCESS_KEY",
    "API_KEY",
    "API_TOKEN",
    "CLIENT_SECRET",
    "CREDENTIAL",
    "ENCRYPTION_KEY",
    "HMAC_KEY",
    "PASSWORD",
    "PRIVATE_KEY",
    "SECRET",
    "TOKEN",
)
PROVIDER_MUTATION_TERMS = (
    "deploy",
    "delete",
    "d1 execute",
    "d1 migrations apply",
    "kv put",
    "queues create",
    "r2 object put",
    "secret put",
)
REMOTE_MUTATION_METHODS = {"DELETE", "PATCH", "POST", "PUT"}
PROVIDER_SOURCE_MARKERS = (
    "cloudflare",
    "d1",
    "kv",
    "queue",
    "r2",
    "s3",
    "wrangler",
    "workers",
)
SOURCE_DANGEROUS_EFFECTS = {
    "browser_runtime",
    "network_io",
    "provider_mutation",
    "secret_environment_access",
}
GENERIC_KEEP_ROLES = {"validator", "generator", "test", "test_or_fixture"}
AR10_ADDITION_DECISIONS: dict[str, dict[str, str]] = {
    "runtime/camouhost/real.py": {
        "classification": "KEEP_PYTHON",
        "role": "real_runtime_adapter",
        "side_effect_class": "generation_scoped_local_browser_runtime",
        "authority": "PYTHON_ALLOWED_BY_AR10",
        "rationale": "Camoufox is Python-native; native Profile Bridge remains lifecycle and launch authority",
    },
    "scripts/check-ar10-runtime-cutover.py": {
        "classification": "KEEP_PYTHON",
        "role": "validator",
        "side_effect_class": "repository_validation_only",
        "authority": "PYTHON_ALLOWED_BY_AR10",
        "rationale": "AR-10 fail-closed runtime/executable cutover validator",
    },
    "scripts/test-ar10-real-camoufox.py": {
        "classification": "KEEP_PYTHON",
        "role": "test",
        "side_effect_class": "test_only",
        "authority": "PYTHON_ALLOWED_BY_AR10",
        "rationale": "repository-local real Camoufox integration evidence fixture",
    },
    "scripts/test-ar10-firefox-writer-locks.py": {
        "classification": "KEEP_PYTHON",
        "role": "test",
        "side_effect_class": "test_only",
        "authority": "PYTHON_ALLOWED_BY_AR10",
        "rationale": "OS-level Firefox writer-lock ownership and fail-closed regression matrix",
    },
}


class EstateError(ValueError):
    pass


def fail(message: str) -> None:
    raise EstateError(message)


def run_git(args: list[str], root: Path = ROOT) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=root,
        check=False,
        text=True,
        capture_output=True,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        fail(f"git {' '.join(args)} failed: {detail}")
    return completed.stdout


def tracked_python(root: Path = ROOT) -> list[str]:
    output = run_git(["ls-files", "-z", "--", "*.py"], root)
    paths = sorted(path for path in output.split("\0") if path)
    if len(paths) != len(set(paths)):
        fail("Git-tracked Python inventory contains duplicate paths")
    return paths


def read_regular(path: Path) -> bytes:
    if path.is_symlink() or not path.is_file():
        fail(f"required Python-estate authority is missing/not regular: {path.relative_to(ROOT)}")
    return path.read_bytes()


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(read_regular(path).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"cannot parse {path.relative_to(ROOT)}: {error}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain one JSON object")
    return value


def git_blob_sha1(raw: bytes) -> str:
    header = f"blob {len(raw)}\0".encode("ascii")
    return hashlib.sha1(header + raw, usedforsecurity=False).hexdigest()


def rows_by_path(rows: object, label: str) -> dict[str, dict[str, Any]]:
    if not isinstance(rows, list):
        fail(f"{label} files must be an array")
    result: dict[str, dict[str, Any]] = {}
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str) or not row["path"]:
            fail(f"{label} contains a malformed row")
        path = row["path"]
        if path in result:
            fail(f"{label} contains duplicate path: {path}")
        result[path] = row
    return result


def validate_summary(document: dict[str, Any], rows: list[dict[str, Any]], label: str) -> None:
    counts = {name: 0 for name in sorted(ALLOWED)}
    for row in rows:
        classification = row.get("classification")
        if classification not in ALLOWED:
            fail(f"{label} contains invalid classification for {row.get('path')}: {classification!r}")
        counts[classification] += 1
    expected = {"tracked_python_files": len(rows), **counts}
    if document.get("summary") != expected:
        fail(f"{label} summary drifted: expected={expected}, observed={document.get('summary')}")


def validate_frozen_baseline(
    baseline: dict[str, Any], overlay: dict[str, Any]
) -> dict[str, dict[str, Any]]:
    if baseline.get("schema_version") != SCHEMA_VERSION:
        fail("AR-6 Python estate schema version drifted")
    if baseline.get("status") != "AR6_ACCEPTED_PYTHON_ESTATE":
        fail("AR-6 Python estate accepted status drifted")
    if baseline.get("accepted_program_checkpoint") != "AR-6" or baseline.get("owning_slice") != "AR-6":
        fail("AR-6 Python estate checkpoint/ownership drifted")
    if set(baseline.get("classification_vocabulary", [])) != ALLOWED:
        fail("AR-6 Python estate classification vocabulary drifted")
    baseline_rows = rows_by_path(baseline.get("files"), "AR-6 Python estate")
    for path, row in baseline_rows.items():
        classification = row.get("classification")
        if classification not in ALLOWED:
            fail(f"invalid frozen Python classification for {path}: {classification!r}")
        if classification != "KEEP_PYTHON":
            missing = sorted(
                field
                for field in FUTURE_FIELDS
                if not isinstance(row.get(field), str) or not row[field]
            )
            if missing:
                fail(f"frozen future-cutover row {path} lacks metadata: {missing}")
    validate_summary(baseline, list(baseline_rows.values()), "AR-6 Python estate")

    baseline_contract = overlay.get("baseline")
    if not isinstance(baseline_contract, dict):
        fail("AR-10 Python estate overlay lacks frozen baseline contract")
    if baseline_contract.get("path") != "architecture/python-estate-ar6.json":
        fail("AR-10 Python estate overlay points at the wrong baseline")
    if baseline_contract.get("accepted_program_checkpoint") != "AR-6":
        fail("AR-10 Python estate overlay changed the accepted baseline checkpoint")
    if baseline_contract.get("immutable") is not True:
        fail("AR-10 Python estate baseline must remain immutable")
    expected_blob = baseline_contract.get("git_blob_sha1")
    actual_blob = git_blob_sha1(read_regular(INVENTORY))
    if not isinstance(expected_blob, str) or expected_blob != actual_blob:
        fail(
            "accepted AR-6 Python estate bytes changed; later slices must use an overlay, "
            f"expected_blob={expected_blob!r} actual_blob={actual_blob}"
        )
    return baseline_rows


def validate_overlay_shape(overlay: dict[str, Any]) -> tuple[dict[str, dict[str, Any]], set[str]]:
    if overlay.get("schema_version") != 1 or overlay.get("status") != "AR10_CURRENT_PYTHON_ESTATE_OVERLAY":
        fail("AR-10 Python estate overlay schema/status drifted")
    if overlay.get("owning_slice") != "AR-10":
        fail("AR-10 Python estate overlay ownership drifted")
    policy = overlay.get("policy")
    if not isinstance(policy, dict):
        fail("AR-10 Python estate overlay policy is missing")
    for field, expected in {
        "global_python_to_rust_rewrite": False,
        "baseline_rewrite_allowed": False,
        "unknown_delta_policy": "FAIL_CLOSED",
        "production_mutation": False,
    }.items():
        if policy.get(field) != expected:
            fail(f"AR-10 Python estate overlay policy drifted: {field}")

    additions = overlay.get("additions")
    if not isinstance(additions, list):
        fail("AR-10 Python estate additions must be an array")
    addition_by_path: dict[str, dict[str, Any]] = {}
    for row in additions:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str) or not row["path"]:
            fail("AR-10 Python estate addition is malformed")
        path = row["path"]
        if path in addition_by_path:
            fail(f"duplicate AR-10 Python estate addition: {path}")
        if row.get("classification") != "KEEP_PYTHON":
            fail(f"AR-10 addition must remain KEEP_PYTHON: {path}")
        if not isinstance(row.get("reason"), str) or not row["reason"]:
            fail(f"AR-10 addition requires an explicit reason: {path}")
        if path not in AR10_ADDITION_DECISIONS:
            fail(f"AR-10 addition lacks explicit semantic review in checker: {path}")
        addition_by_path[path] = row
    if set(addition_by_path) != set(AR10_ADDITION_DECISIONS):
        fail(
            "AR-10 Python addition cohort drifted: "
            f"overlay={sorted(addition_by_path)} expected={sorted(AR10_ADDITION_DECISIONS)}"
        )

    retirements = overlay.get("retirements")
    if not isinstance(retirements, list) or any(
        not isinstance(path, str) or not path for path in retirements
    ):
        fail("AR-10 Python retirements must be an array of non-empty paths")
    retirement_set = set(retirements)
    if len(retirement_set) != len(retirements):
        fail("AR-10 Python estate retirements contain duplicates")
    return addition_by_path, retirement_set


def addition_row(path: str) -> dict[str, Any]:
    decision = AR10_ADDITION_DECISIONS.get(path)
    if decision is None:
        fail(f"unclassified AR-10 Python addition: {path}")
    return {"path": path, **decision}


def build_current_rows(
    baseline_rows: dict[str, dict[str, Any]],
    additions: dict[str, dict[str, Any]],
    retirements: set[str],
) -> list[dict[str, Any]]:
    for path in retirements:
        row = baseline_rows.get(path)
        if row is None:
            fail(f"AR-10 retirement is not present in frozen AR-6 baseline: {path}")
        if row.get("classification") != "DELETE_AFTER_SEQUENCE" or row.get("cutover_slice") != "AR-10":
            fail(f"AR-10 may retire only its accepted DELETE_AFTER_SEQUENCE cohort: {path}")
    for path in additions:
        if path in baseline_rows:
            fail(f"AR-10 addition already exists in frozen AR-6 baseline: {path}")

    rows = [copy.deepcopy(row) for path, row in baseline_rows.items() if path not in retirements]
    rows.extend(addition_row(path) for path in additions)
    rows.sort(key=lambda row: row["path"])
    return rows


def apply_ar11_overlay(rows: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    overlay = load_json(AR11_OVERLAY)
    if (
        overlay.get("schema_version") != 1
        or overlay.get("status") != "AR11_CURRENT_PYTHON_ESTATE_OVERLAY"
        or overlay.get("owning_slice") != "AR-11"
        or overlay.get("parent_overlay") != "architecture/python-estate-ar10.json"
    ):
        fail("AR-11 Python estate overlay identity/version drifted")
    policy = overlay.get("policy")
    if not isinstance(policy, dict):
        fail("AR-11 Python estate overlay policy is missing")
    for field, expected in {
        "global_python_to_rust_rewrite": False,
        "baseline_rewrite_allowed": False,
        "unknown_delta_policy": "FAIL_CLOSED",
        "production_mutation": False,
        "operational_provider_mutator_must_be_rust": True,
    }.items():
        if policy.get(field) != expected:
            fail(f"AR-11 Python estate overlay policy drifted: {field}")
    by_path = {row["path"]: copy.deepcopy(row) for row in rows}
    retirements = overlay.get("retirements")
    if not isinstance(retirements, list) or len(retirements) != len(set(retirements)):
        fail("AR-11 Python retirements must be a unique array")
    for path in retirements:
        row = by_path.get(path)
        if (
            not isinstance(path, str)
            or row is None
            or row.get("classification") != "MIGRATE_TO_RUST"
            or row.get("cutover_slice") != "AR-11"
        ):
            fail(f"AR-11 may retire only its accepted MIGRATE_TO_RUST cohort: {path}")
        del by_path[path]
    additions = overlay.get("additions")
    if not isinstance(additions, list):
        fail("AR-11 Python additions must be an array")
    for row in additions:
        if not isinstance(row, dict):
            fail("AR-11 Python addition must be an object")
        path = row.get("path")
        required = ("path", "classification", "role", "side_effect_class", "authority", "rationale")
        if (
            not isinstance(path, str)
            or not path
            or path in by_path
            or row.get("classification") != "KEEP_PYTHON"
            or any(not isinstance(row.get(field), str) or not row[field] for field in required)
        ):
            fail(f"AR-11 Python addition is malformed/unreviewed: {path!r}")
        by_path[path] = copy.deepcopy(row)
    current = sorted(by_path.values(), key=lambda row: row["path"])
    counts = {name: 0 for name in sorted(ALLOWED)}
    for row in current:
        counts[row["classification"]] += 1
    expected_summary = {"tracked_python_files": len(current), **counts}
    if overlay.get("current_summary") != expected_summary:
        fail(
            "AR-11 Python estate current_summary drifted: "
            f"expected={expected_summary}, observed={overlay.get('current_summary')}"
        )
    return current, overlay


def _import_names(tree: ast.AST) -> set[str]:
    names: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            names.update(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module:
            names.add(node.module)
    return names


def _call_name(node: ast.Call) -> str:
    parts: list[str] = []
    value: ast.AST = node.func
    while isinstance(value, ast.Attribute):
        parts.append(value.attr)
        value = value.value
    if isinstance(value, ast.Name):
        parts.append(value.id)
    return ".".join(reversed(parts))


def _string_literals(node: ast.AST) -> set[str]:
    return {
        child.value
        for child in ast.walk(node)
        if isinstance(child, ast.Constant) and isinstance(child.value, str)
    }


def _has_main_guard(tree: ast.AST) -> bool:
    for node in ast.walk(tree):
        if not isinstance(node, ast.If):
            continue
        test = node.test
        if not isinstance(test, ast.Compare) or len(test.ops) != 1 or len(test.comparators) != 1:
            continue
        left, right = test.left, test.comparators[0]
        values = (left, right)
        if any(isinstance(value, ast.Name) and value.id == "__name__" for value in values) and any(
            isinstance(value, ast.Constant) and value.value == "__main__" for value in values
        ):
            return True
    return False


def semantic_capabilities(source: str, *, label: str) -> tuple[set[str], set[str]]:
    try:
        tree = ast.parse(source, filename=label)
    except SyntaxError as error:
        fail(f"tracked Python must parse as Python: {label}: {error}")
    imports = _import_names(tree)
    capabilities: set[str] = set()
    if any(
        name == prefix or name.startswith(prefix + ".")
        for name in imports
        for prefix in NETWORK_IMPORT_PREFIXES
    ):
        capabilities.add("network_io")
    if any(
        name == prefix or name.startswith(prefix + ".")
        for name in imports
        for prefix in BROWSER_IMPORT_PREFIXES
    ):
        capabilities.add("browser_runtime")
    if "subprocess" in imports:
        capabilities.add("process_exec")
    if "sqlite3" in imports:
        capabilities.add("local_database")
    if "cryptography" in imports or any(name.startswith("cryptography.") for name in imports):
        capabilities.add("cryptography")

    imported_modules = {name.rsplit(".", 1)[-1] for name in imports}
    for node in ast.walk(tree):
        if isinstance(node, ast.Subscript):
            target = node.value
            if (
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id == "os"
                and target.attr == "environ"
                and isinstance(node.slice, ast.Constant)
                and isinstance(node.slice.value, str)
                and any(marker in node.slice.value.upper() for marker in SECRET_ENV_MARKERS)
            ):
                capabilities.add("secret_environment_access")
            continue
        if not isinstance(node, ast.Call):
            continue
        name = _call_name(node)
        literals = _string_literals(node)
        upper_literals = {value.upper() for value in literals}
        if name in {"os.getenv", "os.environ.get"} and any(
            marker in value for value in upper_literals for marker in SECRET_ENV_MARKERS
        ):
            capabilities.add("secret_environment_access")
        if name.startswith("subprocess."):
            capabilities.add("process_exec")
            lowered = " ".join(sorted(value.lower() for value in literals))
            if "wrangler" in lowered and any(term in lowered for term in PROVIDER_MUTATION_TERMS):
                capabilities.add("provider_mutation_subprocess")
        if (
            name.endswith(".write_text")
            or name.endswith(".write_bytes")
            or name.endswith(".mkdir")
            or name.endswith(".unlink")
            or name.endswith(".rename")
            or name.endswith(".replace")
            or name.endswith(".touch")
            or name
            in {
                "os.remove",
                "os.replace",
                "os.rename",
                "shutil.copy",
                "shutil.copy2",
                "shutil.copyfile",
                "shutil.copytree",
                "shutil.move",
                "shutil.rmtree",
            }
        ):
            capabilities.add("filesystem_mutation")

    all_literals = _string_literals(tree)
    upper_literals = {value.upper() for value in all_literals}
    lowered_literals = " ".join(sorted(value.lower() for value in all_literals))
    if "network_io" in capabilities and REMOTE_MUTATION_METHODS & upper_literals:
        capabilities.add("remote_mutation")
    if "provider_mutation_subprocess" in capabilities or (
        "remote_mutation" in capabilities
        and any(marker in lowered_literals for marker in PROVIDER_SOURCE_MARKERS)
    ):
        capabilities.add("provider_mutation")
    return capabilities, imported_modules


def source_role(path: str, source: str, capabilities: set[str]) -> str:
    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError as error:
        fail(f"tracked Python must parse as Python: {path}: {error}")
    relative = Path(path)
    name = relative.name.lower()
    parts = {part.lower() for part in relative.parts}
    doc = (ast.get_docstring(tree) or "").lower()

    if (
        "tests" in parts
        or "fixtures" in parts
        or name.startswith("test_")
        or name.startswith("test-")
    ):
        return "test_or_fixture"
    if "runtime" in parts and ("synthetic" in doc or "fake" in doc):
        return "synthetic_runtime_fixture"
    if "runtime" in parts or "browser_runtime" in capabilities:
        return "runtime_adapter"
    if name.startswith("check_") or name.startswith("check-"):
        return "validator"
    if name.startswith("generate_") or name.startswith("generate-") or "generator" in name:
        return "generator"
    if relative.parts and relative.parts[0] == "tools" and _has_main_guard(tree):
        return "operational_tool"
    if relative.parts and relative.parts[0] == "scripts" and _has_main_guard(tree):
        return "repository_script"
    if _has_main_guard(tree):
        return "entrypoint"
    return "library_module"


def observe_python_source(path: str, source: str) -> dict[str, Any]:
    capabilities, _ = semantic_capabilities(source, label=path)
    role = source_role(path, source, capabilities)
    effects = sorted(capabilities)
    return {
        "path": path,
        "role": role,
        "effects": effects,
        "dangerous_effects": sorted(set(effects) & SOURCE_DANGEROUS_EFFECTS),
    }


def build_source_observation(root: Path = ROOT) -> dict[str, Any]:
    observations: list[dict[str, Any]] = []
    for path in tracked_python(root):
        source_path = root / path
        if source_path.is_symlink() or not source_path.is_file():
            fail(f"tracked Python source must be a regular file: {path}")
        try:
            source = source_path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            fail(f"cannot read tracked Python as UTF-8 for source observation: {path}: {error}")
        observations.append(observe_python_source(path, source))

    effect_counts: dict[str, int] = {}
    role_counts: dict[str, int] = {}
    dangerous_files = 0
    for observation in observations:
        role = observation["role"]
        role_counts[role] = role_counts.get(role, 0) + 1
        if observation["dangerous_effects"]:
            dangerous_files += 1
        for effect in observation["effects"]:
            effect_counts[effect] = effect_counts.get(effect, 0) + 1

    return {
        "schema_version": 1,
        "kind": "PYTHON_ROLE_EFFECT_OBSERVATION",
        "authority": "OBSERVATION_ONLY",
        "source": "GIT_TRACKED_PYTHON_SOURCE",
        "summary": {
            "tracked_python_files": len(observations),
            "dangerous_effect_files": dangerous_files,
            "roles": dict(sorted(role_counts.items())),
            "effects": dict(sorted(effect_counts.items())),
        },
        "observations": observations,
    }


def semantic_validate_all(rows: list[dict[str, Any]], root: Path = ROOT) -> None:
    future_modules = {
        Path(row["path"]).stem: row["path"]
        for row in rows
        if row.get("classification") != "KEEP_PYTHON"
    }
    for row in rows:
        path = row["path"]
        source_path = root / path
        if source_path.is_symlink() or not source_path.is_file():
            fail(f"current Python estate path must be a regular tracked file: {path}")
        try:
            source = source_path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            fail(f"cannot read tracked Python as UTF-8 for semantic review: {path}: {error}")
        capabilities, imported_modules = semantic_capabilities(source, label=path)
        classification = row["classification"]
        role = row.get("role")
        if classification == "KEEP_PYTHON":
            explicit_only = sorted(capabilities & EXPLICIT_ONLY_CAPABILITIES)
            if explicit_only and role in GENERIC_KEEP_ROLES:
                fail(
                    f"KEEP_PYTHON requires explicit semantic role for {path}: "
                    f"capabilities={explicit_only} role={role!r}"
                )
            dependencies = sorted(
                future_modules[module]
                for module in imported_modules & set(future_modules)
            )
            if dependencies and role not in {"test", "test_or_fixture"}:
                fail(f"KEEP_PYTHON path depends on future-cutover Python: {path}: {dependencies}")
        if "provider_mutation_subprocess" in capabilities and classification == "KEEP_PYTHON":
            fail(f"KEEP_PYTHON may not execute Wrangler/provider mutation semantics: {path}")


def build_inventory(root: Path = ROOT) -> dict[str, Any]:
    baseline = load_json(INVENTORY)
    overlay = load_json(AR10_OVERLAY)
    baseline_rows = validate_frozen_baseline(baseline, overlay)
    additions, retirements = validate_overlay_shape(overlay)
    rows = build_current_rows(baseline_rows, additions, retirements)
    rows, ar11_overlay = apply_ar11_overlay(rows)
    tracked = tracked_python(root)
    row_paths = [row["path"] for row in rows]
    if row_paths != tracked:
        missing = sorted(set(tracked) - set(row_paths))
        stale = sorted(set(row_paths) - set(tracked))
        fail(f"current Python estate drift: missing={missing}, stale={stale}")
    semantic_validate_all(rows, root)
    counts = {name: 0 for name in sorted(ALLOWED)}
    for row in rows:
        counts[row["classification"]] += 1
    summary = {"tracked_python_files": len(rows), **counts}
    if ar11_overlay.get("current_summary") != summary:
        fail(
            "AR-11 Python estate overlay current_summary drifted: "
            f"expected={summary}, observed={ar11_overlay.get('current_summary')}"
        )
    return {
        "schema_version": 1,
        "status": "CURRENT_PYTHON_ESTATE_VIA_AR6_BASELINE_PLUS_AR10_AR11_OVERLAYS",
        "accepted_program_checkpoint": "AR-6",
        "current_owning_slice": "AR-11",
        "summary": summary,
        "files": rows,
    }


def validate(document: dict[str, Any], root: Path = ROOT) -> None:
    """Compatibility helper: validate a generated current-estate document."""
    expected = build_inventory(root)
    if document != expected:
        fail("current Python estate document drifted from frozen baseline + AR-10 overlay")


def serialized(document: dict[str, Any]) -> str:
    return json.dumps(document, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def self_test() -> None:
    expected = build_inventory()
    validate(expected)

    baseline_raw = read_regular(INVENTORY)
    if git_blob_sha1(baseline_raw) != load_json(AR10_OVERLAY)["baseline"]["git_blob_sha1"]:
        fail("frozen baseline identity self-test failed")

    unknown_overlay = copy.deepcopy(load_json(AR10_OVERLAY))
    unknown_overlay["additions"].append(
        {
            "path": "scripts/check-unreviewed-future.py",
            "classification": "KEEP_PYTHON",
            "reason": "negative fixture",
        }
    )
    try:
        validate_overlay_shape(unknown_overlay)
    except EstateError:
        pass
    else:
        fail("unknown Python overlay addition negative fixture unexpectedly passed")

    capabilities, _ = semantic_capabilities(
        "import urllib.request\nurllib.request.urlopen('https://example.invalid')\n",
        label="scripts/check-future-network.py",
    )
    if "network_io" not in capabilities or not (capabilities & EXPLICIT_ONLY_CAPABILITIES):
        fail("semantic capability detector lost network negative fixture")

    _, imported = semantic_capabilities(
        "from profile_browser import browser_environment\n",
        label="tools/future-keeper.py",
    )
    if "profile_browser" not in imported:
        fail("semantic dependency detector lost future-cutover negative fixture")

    provider_capabilities, _ = semantic_capabilities(
        "import subprocess\nsubprocess.run(['npx', 'wrangler', 'deploy'], check=True)\n",
        label="scripts/check-future-provider.py",
    )
    if not {"provider_mutation_subprocess", "provider_mutation"} <= provider_capabilities:
        fail("semantic capability detector lost provider-mutation negative fixture")

    direct_provider = observe_python_source(
        "tools/r2_canary.py",
        "\"\"\"R2 S3 operational canary.\"\"\"\n"
        "import urllib.request\n"
        "request = urllib.request.Request('https://example.invalid', method='DELETE')\n",
    )
    if direct_provider["role"] != "library_module":
        fail("source-derived role detector returned an unexpected direct-provider role")
    if not {"network_io", "provider_mutation"} <= set(direct_provider["effects"]):
        fail("source-derived effect detector lost direct provider mutation")

    try:
        observe_python_source("scripts/broken.py", "def broken(:\n")
    except EstateError:
        pass
    else:
        fail("source-derived observation accepted malformed Python")

    r2_source = (ROOT / "tools" / "r2_s3_canary.py").read_text(encoding="utf-8")
    r2_observation = observe_python_source("tools/r2_s3_canary.py", r2_source)
    if r2_observation["role"] != "operational_tool":
        fail(f"R2 canary role drifted: {r2_observation['role']!r}")
    if not {"network_io", "provider_mutation"} <= set(r2_observation["effects"]):
        fail(f"R2 canary effects drifted: {r2_observation['effects']}")

    real_source = (ROOT / "runtime" / "camouhost" / "real.py").read_text(encoding="utf-8")
    real_observation = observe_python_source("runtime/camouhost/real.py", real_source)
    if real_observation["role"] != "runtime_adapter" or "browser_runtime" not in real_observation["effects"]:
        fail(f"real Camouhost source-derived role/effects drifted: {real_observation}")

    synthetic_source = (ROOT / "runtime" / "camouhost" / "main.py").read_text(encoding="utf-8")
    synthetic_observation = observe_python_source("runtime/camouhost/main.py", synthetic_source)
    if synthetic_observation["role"] != "synthetic_runtime_fixture":
        fail(f"synthetic Camouhost role drifted: {synthetic_observation['role']!r}")
    if "browser_runtime" in synthetic_observation["effects"]:
        fail("synthetic Camouhost unexpectedly acquired browser-runtime effects")

    print(
        "Frozen AR-6 Python estate compatibility plus source-derived N2 role/effect "
        "negative fixtures passed."
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true")
    group.add_argument("--check", action="store_true")
    group.add_argument("--self-test", action="store_true")
    group.add_argument("--observe", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.observe:
        print(serialized(build_source_observation()), end="")
        return 0
    if args.write:
        fail(
            "architecture/python-estate-ar6.json is an immutable accepted baseline; "
            "later Python deltas must be recorded in the owning architecture overlay"
        )
    current = build_inventory()
    print(
        f"Python estate current via frozen AR-6 baseline + AR-10 overlay: "
        f"{current['summary']['tracked_python_files']} tracked files, zero unclassified; "
        "semantic capability audit passed."
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (EstateError, OSError, UnicodeError, json.JSONDecodeError) as error:
        print(f"Python estate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
