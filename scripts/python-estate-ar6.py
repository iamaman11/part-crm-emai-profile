#!/usr/bin/env python3
"""AR-6 full Git-tracked Python estate generator and fail-closed checker."""

from __future__ import annotations

import argparse
import ast
import copy
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "architecture" / "python-estate-ar6.json"
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

FUTURE_DECISIONS: dict[str, dict[str, str]] = {
    "check_mail.py": {
        "classification": "DELETE_AFTER_SEQUENCE",
        "role": "retired_legacy_marker",
        "side_effect_class": "none_retired_fail_closed",
        "rust_target": "existing mailbox application/control-plane + Profile Bridge authorities",
        "artifact_type": "retired compatibility marker",
        "cutover_slice": "AR-10",
        "compatibility_requirement": "no accepted workflow or operator contract may depend on this retired launcher marker",
        "retirement_proof": "repository reference scan + permanent mailbox/Profile Bridge gates pass after deletion",
    },
    "profile_manager.py": {
        "classification": "DELETE_AFTER_SEQUENCE",
        "role": "retired_legacy_marker",
        "side_effect_class": "none_retired_fail_closed",
        "rust_target": "apps/profile-bridge",
        "artifact_type": "retired compatibility marker",
        "cutover_slice": "AR-10",
        "compatibility_requirement": "Profile Bridge remains the only accepted local profile lifecycle authority",
        "retirement_proof": "repository reference scan + Windows/Profile Bridge permanent gates pass after deletion",
    },
    "test_fingerprint_consistency.py": {
        "classification": "DELETE_AFTER_SEQUENCE",
        "role": "retired_legacy_marker",
        "side_effect_class": "none_retired_fail_closed",
        "rust_target": "certification application port + accepted Profile Bridge runtime boundary",
        "artifact_type": "retired compatibility marker",
        "cutover_slice": "AR-10",
        "compatibility_requirement": "accepted certification evidence must not depend on this retired launcher marker",
        "retirement_proof": "zero accepted references + certification/Profile Bridge permanent gates green after deletion",
    },
    "tools/cloud_profile_smoke.py": {
        "classification": "DELETE_AFTER_SEQUENCE",
        "role": "legacy_direct_profile_r2_smoke_tool",
        "side_effect_class": "external_profile_object_and_pointer_mutation",
        "rust_target": "Profile Bridge + control-plane generation APIs + bounded R2 canary evidence",
        "artifact_type": "legacy direct profile/R2 smoke executable",
        "cutover_slice": "AR-10",
        "compatibility_requirement": "profile generation, restore and synchronization evidence remains covered without direct Python profile lifecycle or profiles/v1 pointer mutation authority",
        "retirement_proof": "zero accepted workflow/operator references + Profile Bridge/generation/R2 canary gates green after deletion",
    },
    "tools/fingerprint_certify.py": {
        "classification": "DELETE_AFTER_SEQUENCE",
        "role": "legacy_direct_browser_certification_tool",
        "side_effect_class": "disposable_profile_clone_browser_execution",
        "rust_target": "certification application port + accepted Profile Bridge/Camoufox runtime boundary",
        "artifact_type": "legacy direct browser certification executable",
        "cutover_slice": "AR-10",
        "compatibility_requirement": "fingerprint certification remains available through the accepted certification/runtime boundary without importing the legacy direct browser launcher",
        "retirement_proof": "zero accepted workflow/operator references + certification/Profile Bridge permanent gates green after deletion",
    },
    "tools/profile_browser.py": {
        "classification": "DELETE_AFTER_SEQUENCE",
        "role": "legacy_direct_browser_runtime_tool",
        "side_effect_class": "local_profile_runtime_mutation",
        "rust_target": "apps/profile-bridge + accepted Camoufox runtime boundary",
        "artifact_type": "legacy direct runtime executable",
        "cutover_slice": "AR-10",
        "compatibility_requirement": "all accepted browser/profile execution remains available through Profile Bridge without direct Python lifecycle authority",
        "retirement_proof": "zero accepted workflow/runtime references + Profile Bridge/runtime-bundle gates green after deletion",
    },
    "scripts/mailbox-secret-resolver-promotion.py": {
        "classification": "MIGRATE_TO_RUST",
        "role": "legacy_d3_operational_promotion_entrypoint",
        "side_effect_class": "environment_gated_provider_promotion",
        "rust_target": "opsctl release/promotion command family",
        "artifact_type": "typed operational command",
        "cutover_slice": "AR-11",
        "compatibility_requirement": "preserve D3 preflight/attestation/same-bits semantics and AR-2 production fail-closed interlock until AR-11 replacement is accepted",
        "retirement_proof": "GitHub workflows call only accepted Rust operational authority; Python entrypoint has zero legitimate callers and can be removed without weakening D3/AR-11 gates",
    },
    "scripts/_mailbox_secret_resolver_promotion_core.py": {
        "classification": "MIGRATE_TO_RUST",
        "role": "legacy_d3_operational_promotion_core",
        "side_effect_class": "environment_gated_provider_promotion",
        "rust_target": "opsctl release/promotion command family",
        "artifact_type": "typed operational library",
        "cutover_slice": "AR-11",
        "compatibility_requirement": "preserve accepted D3 validation, identity, deployment-closure and same-bits invariants exactly through cutover",
        "retirement_proof": "Rust AR-11 positive/negative fixtures cover accepted D3 invariants and no accepted Python wrapper imports this core",
    },
}

EXPLICIT_KEEP: dict[str, tuple[str, str, str]] = {
    "scripts/check-external-review-attestations.py": (
        "external_review_attestation_verifier",
        "github_api_read_only",
        "terminal external-evidence verification performs bounded GitHub API GETs and may consume a workflow-scoped token; it has no mutation surface",
    ),
    "runtime/camouhost/main.py": (
        "synthetic_runtime_fixture",
        "synthetic_local_fixture_state_only",
        "repository contract evidence fake; not production Camoufox authority",
    ),
    "runtime/camouhost/real.py": (
        "supported_camoufox_outer_adapter",
        "governed_local_profile_runtime_only",
        "Python-native Camoufox adapter is an implementation detail behind native Profile Bridge preflight/lifecycle authority; it does not own cloud/profile lifecycle or provider mutation",
    ),
    "tools/r2_s3_canary.py": (
        "external_provider_canary",
        "disposable_canary_object_put_delete",
        "bounded canary mutation is test/evidence only and must never become production object lifecycle authority",
    ),
    "tools/runtime_bundle.py": (
        "runtime_bundle_generator",
        "artifact_generation_only",
        "deterministic artifact generator remains legitimate Python",
    ),
    "scripts/accepted_phase_provenance.py": (
        "provenance_validator",
        "repository_read_only",
        "accepted provenance validation remains legitimate Python",
    ),
    "scripts/cloudflare-d1-bootstrap.py": (
        "bootstrap_sql_generator",
        "artifact_generation_and_local_sqlite_only",
        "builds/verifies SQL; remote execution remains Wrangler/workflow authority",
    ),
    "scripts/cloudflare-deploy-config.py": (
        "deployment_config_generator",
        "artifact_generation_only",
        "renderer/validator remains legitimate Python; it does not deploy",
    ),
    "scripts/cloudflare-release.py": (
        "immutable_release_generator",
        "artifact_generation_only",
        "release artifact/provenance generator remains Python; AR-11 owns later operational release-set semantics",
    ),
    "scripts/mailbox-secret-resolver-d1-bootstrap.py": (
        "bootstrap_sql_generator",
        "artifact_generation_and_local_sqlite_only",
        "builds/verifies resolver bootstrap SQL; remote execution remains Wrangler/workflow authority",
    ),
    "scripts/mailbox-secret-resolver-release.py": (
        "immutable_release_generator",
        "artifact_generation_only",
        "resolver artifact/provenance generator remains Python; AR-11 owns later operational release-set semantics",
    ),
    "scripts/prepare-external-evidence.py": (
        "evidence_preparer",
        "evidence_artifact_only",
        "research/evidence helper remains legitimate Python",
    ),
    "scripts/python-estate-ar6.py": (
        "estate_generator",
        "repository_validation_and_inventory_generation",
        "AR-6 canonical Python estate generator/checker remains legitimate Python",
    ),
    "scripts/render-openapi.py": (
        "contract_generator",
        "artifact_generation_only",
        "deterministic contract rendering remains legitimate Python",
    ),
    "scripts/verify-fast.py": (
        "developer_verifier",
        "repository_validation_only",
        "developer verification orchestration is non-authoritative/read-only",
    ),
}


def tracked_python(root: Path = ROOT) -> list[str]:
    completed = subprocess.run(
        ["git", "ls-files", "--", "*.py"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise ValueError("AR-6 Python estate requires a Git checkout")
    paths = sorted(line.strip() for line in completed.stdout.splitlines() if line.strip())
    if not paths or len(paths) != len(set(paths)):
        raise ValueError("tracked Python path inventory must be non-empty and unique")
    return paths


def keep(path: str, role: str, side_effect_class: str, rationale: str) -> dict[str, Any]:
    return {
        "path": path,
        "classification": "KEEP_PYTHON",
        "role": role,
        "side_effect_class": side_effect_class,
        "authority": "PYTHON_ALLOWED_BY_AR6",
        "rationale": rationale,
    }


def classify(path: str) -> dict[str, Any]:
    if path in FUTURE_DECISIONS:
        return {"path": path, "authority": "FUTURE_CUTOVER_REQUIRED", **FUTURE_DECISIONS[path]}
    if path in EXPLICIT_KEEP:
        role, side_effect, rationale = EXPLICIT_KEEP[path]
        return keep(path, role, side_effect, rationale)
    if path.startswith("tests/"):
        return keep(path, "test_or_fixture", "test_only", "tests and fixtures remain legitimate Python")
    if path.startswith("scripts/check-") or path.startswith("scripts/check_"):
        return keep(path, "validator", "repository_validation_only", "validators remain legitimate Python")
    if path.startswith("scripts/test-") or path.startswith("scripts/test_"):
        return keep(path, "test", "test_only", "tests remain legitimate Python")
    if path.startswith("scripts/generate-"):
        return keep(path, "generator", "artifact_generation_only", "deterministic generators remain legitimate Python")
    if path in {
        "scripts/_ar3_application_architecture.py",
        "scripts/_architecture_inventory_core.py",
        "scripts/_cloudflare_runtime_bindings_core.py",
    }:
        return keep(path, "validator_core", "repository_validation_only", "validator core remains legitimate Python")
    raise ValueError(f"unclassified tracked Python path: {path}")


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


def semantic_capabilities(source: str, *, label: str) -> tuple[set[str], set[str]]:
    try:
        tree = ast.parse(source, filename=label)
    except SyntaxError as error:
        raise ValueError(f"tracked Python must parse as Python: {label}: {error}") from error

    imports = _import_names(tree)
    capabilities: set[str] = set()
    if any(name == prefix or name.startswith(prefix + ".") for name in imports for prefix in NETWORK_IMPORT_PREFIXES):
        capabilities.add("network_io")
    if any(name == prefix or name.startswith(prefix + ".") for name in imports for prefix in BROWSER_IMPORT_PREFIXES):
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
            or name in {
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

    return capabilities, imported_modules


def semantic_validate(path: str, row: dict[str, Any], root: Path = ROOT) -> None:
    source_path = root / path
    if source_path.is_symlink() or not source_path.is_file():
        raise ValueError(f"tracked Python path must be a regular file for semantic review: {path}")
    try:
        source = source_path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise ValueError(f"cannot read tracked Python as UTF-8 for semantic review: {path}") from error

    capabilities, imported_modules = semantic_capabilities(source, label=path)
    classification = row["classification"]
    role = row["role"]

    if classification == "KEEP_PYTHON":
        explicit_only = sorted(capabilities & EXPLICIT_ONLY_CAPABILITIES)
        if explicit_only and path not in EXPLICIT_KEEP:
            raise ValueError(
                f"Python KEEP decision requires explicit semantic review for {path}: capabilities={explicit_only}"
            )

        future_modules = {Path(future_path).stem: future_path for future_path in FUTURE_DECISIONS}
        dependencies = sorted(
            future_modules[module]
            for module in imported_modules & set(future_modules)
        )
        if dependencies and role not in {"test", "test_or_fixture"}:
            raise ValueError(
                f"KEEP_PYTHON path depends directly on future-cutover Python: {path}: {dependencies}"
            )

    if "provider_mutation_subprocess" in capabilities and classification == "KEEP_PYTHON":
        raise ValueError(
            f"KEEP_PYTHON may not execute Wrangler/provider mutation semantics: {path}"
        )


def semantic_validate_all(rows: list[dict[str, Any]], root: Path = ROOT) -> None:
    for row in rows:
        semantic_validate(row["path"], row, root)


def build_inventory(root: Path = ROOT) -> dict[str, Any]:
    rows = [classify(path) for path in tracked_python(root)]
    semantic_validate_all(rows, root)
    summary = {name: 0 for name in sorted(ALLOWED)}
    for row in rows:
        summary[row["classification"]] += 1
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "AR6_ACCEPTED_PYTHON_ESTATE",
        "scope": "ALL_GIT_TRACKED_PYTHON_STRONGER_THAN_EXECUTABLE_ONLY",
        "accepted_program_checkpoint": "AR-6",
        "owning_slice": "AR-6",
        "classification_vocabulary": sorted(ALLOWED),
        "policy": {
            "global_python_to_rust_rewrite": False,
            "unknown_path_policy": "FAIL_CLOSED",
            "python_allowed_for": ["validators", "generators", "tests_fixtures", "research_evidence", "explicit_helpers"],
            "mutable_authority_rule": "ONE_MUTABLE_CONCERN_ONE_LEGITIMATE_AUTHORITY",
            "production_mutation": False,
        },
        "summary": {"tracked_python_files": len(rows), **summary},
        "files": rows,
    }


def validate(document: dict[str, Any], root: Path = ROOT) -> None:
    if document.get("schema_version") != SCHEMA_VERSION or document.get("status") != "AR6_ACCEPTED_PYTHON_ESTATE":
        raise ValueError("AR-6 Python estate schema/status drifted")
    rows = document.get("files")
    if not isinstance(rows, list):
        raise ValueError("AR-6 Python estate files must be an array")
    paths = [row.get("path") for row in rows if isinstance(row, dict)]
    if len(paths) != len(rows) or any(not isinstance(path, str) or not path for path in paths):
        raise ValueError("every AR-6 Python estate row requires a path")
    if len(paths) != len(set(paths)):
        raise ValueError("AR-6 Python estate contains duplicate paths")
    tracked = tracked_python(root)
    if paths != tracked:
        missing = sorted(set(tracked) - set(paths))
        stale = sorted(set(paths) - set(tracked))
        raise ValueError(f"AR-6 Python estate drift: missing={missing}, stale={stale}")
    for row in rows:
        classification = row.get("classification")
        if classification not in ALLOWED:
            raise ValueError(f"invalid Python classification for {row.get('path')}: {classification!r}")
        expected = classify(row["path"])
        if row != expected:
            raise ValueError(f"Python classification decision drifted for {row['path']}")
        if classification != "KEEP_PYTHON":
            missing = sorted(field for field in FUTURE_FIELDS if not isinstance(row.get(field), str) or not row[field])
            if missing:
                raise ValueError(f"future-cutover row {row['path']} lacks metadata: {missing}")
    semantic_validate_all(rows, root)
    expected_summary = build_inventory(root)["summary"]
    if document.get("summary") != expected_summary:
        raise ValueError("AR-6 Python estate summary drifted")


def serialized(document: dict[str, Any]) -> str:
    return json.dumps(document, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def self_test() -> None:
    expected = build_inventory()
    validate(expected)

    missing = copy.deepcopy(expected)
    missing["files"] = missing["files"][1:]
    try:
        validate(missing)
    except ValueError:
        pass
    else:
        raise ValueError("missing Python path negative fixture unexpectedly passed")

    duplicate = copy.deepcopy(expected)
    duplicate["files"].append(copy.deepcopy(duplicate["files"][0]))
    try:
        validate(duplicate)
    except ValueError:
        pass
    else:
        raise ValueError("duplicate Python path negative fixture unexpectedly passed")

    migration = copy.deepcopy(expected)
    row = next(item for item in migration["files"] if item["path"] == "scripts/mailbox-secret-resolver-promotion.py")
    row.pop("retirement_proof")
    try:
        validate(migration)
    except ValueError:
        pass
    else:
        raise ValueError("incomplete migration metadata negative fixture unexpectedly passed")

    try:
        classify("scripts/future-unclassified-operational.py")
    except ValueError:
        pass
    else:
        raise ValueError("unknown Python path fail-closed fixture unexpectedly passed")

    capabilities, _ = semantic_capabilities(
        "import urllib.request\nurllib.request.urlopen('https://example.invalid')\n",
        label="scripts/check-future-network.py",
    )
    if "network_io" not in capabilities or not (capabilities & EXPLICIT_ONLY_CAPABILITIES):
        raise ValueError("semantic capability detector lost network negative fixture")

    _, imported = semantic_capabilities(
        "from profile_browser import browser_environment\n",
        label="tools/future-keeper.py",
    )
    if "profile_browser" not in imported:
        raise ValueError("semantic dependency detector lost future-cutover negative fixture")

    provider_capabilities, _ = semantic_capabilities(
        "import subprocess\nsubprocess.run(['npx', 'wrangler', 'deploy'], check=True)\n",
        label="scripts/check-future-provider.py",
    )
    if "provider_mutation_subprocess" not in provider_capabilities:
        raise ValueError("semantic capability detector lost provider-mutation negative fixture")

    print("AR-6 Python estate semantic and inventory negative fixtures rejected as expected.")


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true")
    group.add_argument("--check", action="store_true")
    group.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    expected = build_inventory()
    if args.write:
        INVENTORY.parent.mkdir(parents=True, exist_ok=True)
        INVENTORY.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print(f"Wrote {INVENTORY.relative_to(ROOT)} with {len(expected['files'])} tracked Python files.")
        return 0
    try:
        actual = json.loads(INVENTORY.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read AR-6 Python estate: {error}") from error
    validate(actual)
    if serialized(actual) != serialized(expected):
        raise ValueError("AR-6 Python estate is not deterministic/current; run --write")
    print(
        f"AR-6 Python estate is current: {len(expected['files'])} tracked files, "
        "zero unclassified; semantic capability audit passed."
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(f"AR-6 Python estate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
