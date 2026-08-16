#!/usr/bin/env python3
"""AR-6 full Git-tracked Python estate generator and fail-closed checker."""

from __future__ import annotations

import argparse
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

# These paths are architecture decisions, not pattern guesses.
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
    "runtime/camouhost/main.py": (
        "synthetic_runtime_fixture",
        "synthetic_local_fixture_state_only",
        "repository contract evidence fake; not production Camoufox authority",
    ),
    "test_fingerprint_consistency.py": ("test", "test_only", "test/evidence remains legitimate Python"),
    "tools/cloud_profile_smoke.py": (
        "external_smoke_evidence",
        "disposable_external_test_objects",
        "live evidence helper is not production lifecycle authority; credentials remain workflow-scoped",
    ),
    "tools/fingerprint_certify.py": (
        "external_certification_evidence",
        "disposable_profile_clone_only",
        "fingerprint certification/research remains legitimate Python evidence",
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
    if path == "scripts/python-estate-ar6.py":
        return keep(path, "estate_generator", "repository_validation_and_inventory_generation", "AR-6 canonical Python estate generator/checker remains legitimate Python")
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


def build_inventory(root: Path = ROOT) -> dict[str, Any]:
    rows = [classify(path) for path in tracked_python(root)]
    summary = {name: 0 for name in sorted(ALLOWED)}
    for row in rows:
        summary[row["classification"]] += 1
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "AR6_CANDIDATE_PYTHON_ESTATE",
        "scope": "ALL_GIT_TRACKED_PYTHON_STRONGER_THAN_EXECUTABLE_ONLY",
        "accepted_program_checkpoint_remains": "AR-5",
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
    if document.get("schema_version") != SCHEMA_VERSION or document.get("status") != "AR6_CANDIDATE_PYTHON_ESTATE":
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

    print("AR-6 Python estate negative fixtures rejected as expected.")


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
    print(f"AR-6 Python estate is current: {len(expected['files'])} tracked files, zero unclassified.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(f"AR-6 Python estate error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
