#!/usr/bin/env python3
"""Generate and verify the canonical Architecture Re-baseline v3 inventory.

The proven workspace/migration/route/generated-contract core remains in
`_architecture_inventory_core.py`. Later AR slices extend the same canonical hierarchy;
AR-8B projects the accepted metadata-only credential authority; AR-8C projects its bounded operational lifecycle without creating a competing registry.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

import _architecture_inventory_core as core
import _ar3_application_architecture as ar3

ROOT = Path(__file__).resolve().parents[1]
INVENTORY_PATH = ROOT / "architecture" / "inventory.json"
CURRENT_AUTHORITY = "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md"
TRANSITION = "architecture/architecture-rebaseline-v3-transition.json"
RUNTIME_TOPOLOGY = "architecture/runtime-topology-ar2.json"
AR2_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR2.md"
AR4A_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4A.md"
AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"
AR4C_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"
AR5_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR5.md"
AR6_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR6.md"
AR7_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR7.md"
GOVERNANCE_CONTRACT = "architecture/github-governance-ar7.json"
PYTHON_ESTATE = "architecture/python-estate-ar6.json"
CREDENTIAL_AUTHORITY = "architecture/credential-authority-ar8b.json"
TRACKING_ISSUE = 266
AR8_UMBRELLA_ISSUE = 308
AR8B_IMPLEMENTATION_ISSUE = 309
AR8C_IMPLEMENTATION_ISSUE = 314
ACCEPTED_SLICES = ["AR-0", "AR-1", "AR-2", "AR-3", "AR-4A", "AR-4B", "AR-4C", "AR-5", "AR-6", "AR-7"]
CURRENT_SLICE = "AR-8"
NEXT_SLICE = "AR-9"
AR8_ACCEPTED_SUBSLICES = ["AR-8A", "AR-8B"]
AR8_CURRENT_SUBSLICE = "AR-8C"
AR8_MANDATORY_REMAINING = ["AR-8C", "AR-8D", "AR-8E", "AR-8F"]

CANONICAL_ENVIRONMENTS = {"rehearsal", "staging", "production"}
REQUIRED_CREDENTIAL_ENTRY_FIELDS = {
    "id",
    "class",
    "provider_system",
    "environment_scope",
    "owner",
    "consumers",
    "bindings",
    "protected_value_authority",
    "legitimate_mutable_authority",
    "version_state_source",
    "automation_class",
    "externally_issued",
    "rotation_recovery_policy",
    "future_cutover",
}
FORBIDDEN_CREDENTIAL_VALUE_FIELDS = {
    "value",
    "secret_value",
    "plaintext",
    "plaintext_value",
    "private_key",
    "password",
    "token",
    "token_value",
    "credential_value",
    "key_material",
    "raw_secret",
    "raw_token",
}
HIGH_CONFIDENCE_CREDENTIAL_VALUE_PATTERNS = (
    re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
)
WORKFLOW_SECRET = re.compile(r"\bsecrets\.([A-Z][A-Z0-9_]*)\b")
WRANGLER_REQUIRED = re.compile(r'"required"\s*:\s*\[(.*?)\]', re.DOTALL)
QUOTED_IDENTIFIER = re.compile(r'"([A-Z][A-Z0-9_]*)"')
RUST_WORKER_SECRET = re.compile(r'\.secret\(\s*"([A-Z][A-Z0-9_]*)"\s*\)')
CREDENTIAL_NAME = r"([A-Z][A-Z0-9_]*(?:SECRET|TOKEN|PASSWORD|PRIVATE_KEY|API_KEY|AUTH_KEY|KEYRING)[A-Z0-9_]*)"
PY_ENV_ITEM = re.compile(rf"os\.environ\[\s*[\"']{CREDENTIAL_NAME}[\"']\s*\]")
PY_ENV_GET = re.compile(rf"os\.environ\.get\(\s*[\"']{CREDENTIAL_NAME}[\"']")
PY_GETENV = re.compile(rf"os\.getenv\(\s*[\"']{CREDENTIAL_NAME}[\"']")
JS_ENV_LOOKUP = re.compile(rf"(?:process\.env\.|env\.){CREDENTIAL_NAME}")
ENVIRONMENT_BOUND_CREDENTIAL_SURFACES = {"github_environment_secret", "cloudflare_worker_secret"}
PORTABLE_CREDENTIAL_SCAN_EXCLUSIONS = {"scripts/check-tracked-secrets.sh"}

DOCUMENT_STATUS = [
    {"path": "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "status": "CURRENT_AUTHORITY", "scope": "architecture_program_execution"},
    {"path": "docs/DEVELOPMENT_PLAN.md", "status": "GENERATED_PROJECTION", "scope": "product_program_projection_and_accepted_phase_provenance"},
    {"path": "docs/status.json", "status": "GENERATED_PROJECTION", "scope": "machine_readable_program_and_readiness_state"},
    {"path": "docs/INDEX.md", "status": "GENERATED_PROJECTION", "scope": "documentation_authority_navigation"},
    {"path": "README.md", "status": "GENERATED_PROJECTION", "scope": "repository_entrypoint"},
    {"path": "docs/README.md", "status": "GENERATED_PROJECTION", "scope": "documentation_entrypoint"},
    {"path": "docs/ARCHITECTURE.md", "status": "STABLE_AUTHORITY", "scope": "accepted_architecture_invariants"},
    {"path": "docs/DATA_CLASSIFICATION.md", "status": "STABLE_AUTHORITY", "scope": "data_privacy_classification"},
    {"path": "docs/THREAT_MODEL.md", "status": "STABLE_AUTHORITY", "scope": "repository_security_threat_model"},
    {"path": "docs/UI_ARCHITECTURE.md", "status": "STABLE_AUTHORITY", "scope": "ui_architecture_target"},
    {"path": "docs/DEVELOPER_CAPABILITY_MATRIX.md", "status": "STABLE_AUTHORITY", "scope": "accepted_capability_evidence"},
    {"path": "architecture/accepted-phases.json", "status": "STABLE_AUTHORITY", "scope": "immutable_accepted_product_phase_provenance"},
    {"path": TRANSITION, "status": "GENERATED_PROJECTION", "scope": "architecture_program_transition_state"},
    {"path": "docs/ARCHITECTURE_REBASELINE_V3_AR0.md", "status": "EVIDENCE", "scope": "ar0_research_acceptance"},
    {"path": "docs/ARCHITECTURE_REBASELINE_V3_SECOND_PASS_REVIEW.md", "status": "EVIDENCE", "scope": "ar0_second_pass_research"},
    {"path": AR2_EVIDENCE, "status": "EVIDENCE", "scope": "ar2_runtime_topology_and_d3_compatibility_acceptance"},
    {"path": RUNTIME_TOPOLOGY, "status": "STABLE_AUTHORITY", "scope": "accepted_ar2_runtime_topology_decision_input_for_ar3"},
    {"path": ar3.AR3_EVIDENCE, "status": "EVIDENCE", "scope": "ar3_application_architecture_contract_accepted"},
    {"path": AR4A_EVIDENCE, "status": "EVIDENCE", "scope": "ar4a_composition_root_consolidation_accepted"},
    {"path": AR4B_EVIDENCE, "status": "EVIDENCE", "scope": "ar4b_client_mail_route_ownership_accepted"},
    {"path": AR4C_EVIDENCE, "status": "EVIDENCE", "scope": "ar4c_outbound_mail_composition_accepted"},
    {"path": AR5_EVIDENCE, "status": "EVIDENCE", "scope": "ar5_runtime_authority_cleanup_accepted"},
    {"path": AR6_EVIDENCE, "status": "EVIDENCE", "scope": "ar6_python_estate_and_read_only_opsctl_accepted"},
    {"path": PYTHON_ESTATE, "status": "STABLE_AUTHORITY", "scope": "accepted_ar6_full_python_disposition"},
    {"path": AR7_EVIDENCE, "status": "EVIDENCE", "scope": "ar7_github_governance_and_operational_boundaries_accepted"},
    {"path": GOVERNANCE_CONTRACT, "status": "STABLE_AUTHORITY", "scope": "accepted_ar7_github_governance_contract"},
    {"path": "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md", "status": "ACCEPTED_HISTORICAL", "scope": "superseded_predecessor_forward_execution"},
    {"path": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md", "status": "ACCEPTED_HISTORICAL", "scope": "accepted_r1_r9_closeout"},
    {"path": "IMPLEMENTATION_PLAN.md", "status": "SUPERSEDED", "scope": "compatibility_entrypoint_to_preserved_history"},
    {"path": "PROFILE_LIFECYCLE_PLAN.md", "status": "SUPERSEDED", "scope": "compatibility_entrypoint_to_preserved_history"},
]


def ar8b_acceptance() -> dict[str, object]:
    return {
        "issue": 309,
        "implementation_pr": 312,
        "exact_green_head": "743276b578bb15042e8a42beff0e14f7698e61b0",
        "implementation_merge": "4e4d1c25226384858ca8905377ee155bedabc6d4",
        "applicable_permanent_workflows": "14/14",
    }


def expected_ar8_progress() -> dict[str, object]:
    return {
        "umbrella_issue": AR8_UMBRELLA_ISSUE,
        "accepted_subslices": AR8_ACCEPTED_SUBSLICES,
        "current_subslice": AR8_CURRENT_SUBSLICE,
        "current_implementation_issue": AR8C_IMPLEMENTATION_ISSUE,
        "mandatory_remaining": AR8_MANDATORY_REMAINING,
        "full_ar8_accepted": False,
        "ar9_blocked": True,
        "production_mutation": False,
    }


def validate_source_documents() -> None:
    for item in DOCUMENT_STATUS:
        if not (ROOT / item["path"]).is_file():
            raise SystemExit(f"document-status inventory path missing: {item['path']}")
    status = json.loads((ROOT / "docs/status.json").read_text(encoding="utf-8"))
    current = status.get("current", {})
    program = current.get("architecture_program", {}) if isinstance(current, dict) else {}
    if status.get("production_ready") is not False:
        raise SystemExit("docs/status.json must remain production_ready=false during AR-8")
    if current.get("architecture_complete") is not False or current.get("production_core_gate") != "BLOCKED":
        raise SystemExit("docs/status.json must keep AR-8 architecture/gate state fail closed")
    if program.get("authority") != CURRENT_AUTHORITY or program.get("tracking_issue") != TRACKING_ISSUE:
        raise SystemExit("docs/status.json current architecture authority drifted")
    if program.get("accepted_slices") != ACCEPTED_SLICES or program.get("current_slice") != CURRENT_SLICE or program.get("next_slice_after_acceptance") != NEXT_SLICE:
        raise SystemExit("docs/status.json must project accepted through AR-7 with active AR-8 and AR-9 blocked")
    if program.get("ar8_progress") != expected_ar8_progress():
        raise SystemExit("docs/status.json must project AR-8A/B accepted / AR-8C current / AR-8D..F mandatory")
    if program.get("runtime_topology_decision") != RUNTIME_TOPOLOGY:
        raise SystemExit("docs/status.json must project the accepted AR-2 topology decision")
    if program.get("runtime_authority_cleanup_evidence") != AR5_EVIDENCE:
        raise SystemExit("docs/status.json must project the accepted AR-5 runtime-authority cleanup evidence")
    if program.get("python_operational_evidence") != AR6_EVIDENCE or program.get("python_estate") != PYTHON_ESTATE:
        raise SystemExit("docs/status.json must project accepted AR-6 Python/opsctl authority")
    if program.get("github_governance_evidence") != AR7_EVIDENCE or program.get("github_governance_contract") != GOVERNANCE_CONTRACT:
        raise SystemExit("docs/status.json must project accepted AR-7 GitHub governance authority")
    runtime_gate = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-cloudflare-runtime-bindings.py")],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if runtime_gate.returncode != 0:
        details = "\n".join(value.strip() for value in (runtime_gate.stdout, runtime_gate.stderr) if value.strip())
        raise SystemExit(f"AR-5 runtime authority gate failed:\n{details}")


def validate_full_documentation_authority() -> None:
    authority = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-documentation-authority.py"), "--root", str(ROOT)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if authority.returncode != 0:
        details = "\n".join(value.strip() for value in (authority.stdout, authority.stderr) if value.strip())
        raise SystemExit(f"documentation authority check failed:\n{details}")


def tracked_credential_files() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        details = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError(f"git ls-files failed while discovering credential surfaces: {details or result.returncode}")
    files: list[Path] = []
    for raw in result.stdout.split(b"\0"):
        if not raw:
            continue
        relative = raw.decode("utf-8")
        files.append(ROOT / Path(relative))
    return files


def credential_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return ""


def scan_tracked_credential_material(files: list[Path]) -> None:
    for path in files:
        relative = path.relative_to(ROOT).as_posix()
        if relative in PORTABLE_CREDENTIAL_SCAN_EXCLUSIONS:
            continue
        source = credential_text(path)
        if not source:
            continue
        if any(pattern.search(source) for pattern in HIGH_CONFIDENCE_CREDENTIAL_VALUE_PATTERNS):
            raise ValueError(f"high-confidence credential material found in tracked file: {relative}")


def discover_credential_bindings(files: list[Path]) -> dict[str, set[str]]:
    detected: dict[str, set[str]] = {}

    def add(name: str, path: Path) -> None:
        detected.setdefault(name, set()).add(path.relative_to(ROOT).as_posix())

    for path in files:
        relative = path.relative_to(ROOT).as_posix()
        source = credential_text(path)
        if not source:
            continue
        if relative.startswith(".github/workflows/") and path.suffix in {".yml", ".yaml"}:
            for name in WORKFLOW_SECRET.findall(source):
                add(name, path)
        if relative.startswith("deploy/cloudflare/") and path.suffix in {".json", ".jsonc"}:
            for block in WRANGLER_REQUIRED.findall(source):
                for name in QUOTED_IDENTIFIER.findall(block):
                    add(name, path)
        if relative.startswith(("apps/", "crates/")) and path.suffix == ".rs":
            for name in RUST_WORKER_SECRET.findall(source):
                add(name, path)
        if relative.startswith(("scripts/", "tools/")) and path.suffix == ".py":
            for pattern in (PY_ENV_ITEM, PY_ENV_GET, PY_GETENV):
                for name in pattern.findall(source):
                    add(name, path)
        if relative.startswith(("scripts/", "tools/", ".github/")) and path.suffix in {".js", ".mjs", ".cjs", ".ts"}:
            for name in JS_ENV_LOOKUP.findall(source):
                add(name, path)
    return detected


def walk_credential_payload(value: Any, path: str = "$") -> Iterable[tuple[str, str, Any]]:
    if isinstance(value, dict):
        for key, nested in value.items():
            yield path, str(key), nested
            yield from walk_credential_payload(nested, f"{path}.{key}")
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            yield from walk_credential_payload(nested, f"{path}[{index}]")


def credential_entries(payload: dict[str, Any]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for section in ("credentials", "dynamic_credential_domains", "future_trust_domains"):
        raw = payload.get(section)
        if not isinstance(raw, list):
            raise ValueError(f"{section} must be a list")
        if any(not isinstance(entry, dict) for entry in raw):
            raise ValueError(f"{section} entries must be objects")
        result.extend(raw)
    return result


def validate_credential_authority(payload: dict[str, Any], detected: dict[str, set[str]]) -> None:
    if payload.get("schema_version") != 1:
        raise ValueError("credential authority schema_version must be 1")
    if payload.get("status") != "ACCEPTED_AR8B_CREDENTIAL_METADATA_AUTHORITY":
        raise ValueError("credential authority must be accepted for AR-8B merge")
    if payload.get("parent_issue") != AR8_UMBRELLA_ISSUE or payload.get("implementation_issue") != AR8B_IMPLEMENTATION_ISSUE:
        raise ValueError("AR-8B credential authority issue provenance drifted")
    if payload.get("canonical_inventory") != "architecture/inventory.json":
        raise ValueError("AR-8B must extend the canonical inventory, not create a competing registry")
    if payload.get("metadata_only") is not True:
        raise ValueError("credential authority must be metadata_only=true")
    if set(payload.get("canonical_environments", [])) != CANONICAL_ENVIRONMENTS:
        raise ValueError("canonical_environments must be rehearsal/staging/production exactly")

    invariants = payload.get("invariants")
    if not isinstance(invariants, dict):
        raise ValueError("invariants must be an object")
    if invariants.get("plaintext_in_git") != "FORBIDDEN" or invariants.get("competing_registry") != "FORBIDDEN":
        raise ValueError("plaintext and competing registries must remain forbidden")
    if invariants.get("mutable_authorities_per_concern") != 1:
        raise ValueError("one concern must have exactly one legitimate mutable authority")
    if invariants.get("production_mutation") is not False or invariants.get("ar9_blocked") is not True:
        raise ValueError("AR-8B must keep production mutation disabled and AR-9 blocked")

    for location, key, nested in walk_credential_payload(payload):
        if key.lower() in FORBIDDEN_CREDENTIAL_VALUE_FIELDS:
            raise ValueError(f"forbidden value-bearing field {location}.{key}")
        if isinstance(nested, str) and any(pattern.search(nested) for pattern in HIGH_CONFIDENCE_CREDENTIAL_VALUE_PATTERNS):
            raise ValueError(f"high-confidence credential material found at {location}.{key}")

    logical_entries = credential_entries(payload)
    ids: set[str] = set()
    owners: dict[str, str] = {}
    declaration_only: set[str] = set()
    for entry in logical_entries:
        missing = REQUIRED_CREDENTIAL_ENTRY_FIELDS - set(entry)
        logical_id = entry.get("id", "<missing-id>")
        if missing:
            raise ValueError(f"{logical_id}: missing required fields {sorted(missing)!r}")
        if not isinstance(logical_id, str) or not logical_id or logical_id in ids:
            raise ValueError(f"invalid or duplicate credential authority id: {logical_id!r}")
        ids.add(logical_id)
        if not isinstance(entry.get("externally_issued"), bool) or not isinstance(entry.get("consumers"), list):
            raise ValueError(f"{logical_id}: malformed externally_issued/consumers")
        scope = entry.get("environment_scope")
        if not isinstance(scope, dict):
            raise ValueError(f"{logical_id}: environment_scope must be an object")
        kind = scope.get("kind")
        environments = scope.get("environments", [])
        if kind not in {"repository", "environment", "tenant_dynamic", "release"}:
            raise ValueError(f"{logical_id}: unknown environment scope {kind!r}")
        if kind == "environment":
            if not isinstance(environments, list) or not environments or set(environments) - CANONICAL_ENVIRONMENTS:
                raise ValueError(f"{logical_id}: invalid canonical environment scope")
        elif environments:
            raise ValueError(f"{logical_id}: environments allowed only for kind=environment")
        for field in (
            "class",
            "provider_system",
            "owner",
            "protected_value_authority",
            "legitimate_mutable_authority",
            "version_state_source",
            "automation_class",
            "rotation_recovery_policy",
            "future_cutover",
        ):
            if not isinstance(entry.get(field), str) or not entry[field].strip():
                raise ValueError(f"{logical_id}: {field} must be non-empty")
        bindings = entry.get("bindings")
        if not isinstance(bindings, list):
            raise ValueError(f"{logical_id}: bindings must be a list")
        seen: set[tuple[str, str, str]] = set()
        for binding in bindings:
            if not isinstance(binding, dict):
                raise ValueError(f"{logical_id}: binding must be an object")
            name = binding.get("name")
            surface = binding.get("surface")
            consumer = str(binding.get("consumer", ""))
            if not isinstance(name, str) or not name or not isinstance(surface, str) or not surface:
                raise ValueError(f"{logical_id}: binding name/surface must be non-empty")
            identity = (surface, name, consumer)
            if identity in seen:
                raise ValueError(f"{logical_id}: duplicate binding tuple {identity!r}")
            seen.add(identity)
            if surface in ENVIRONMENT_BOUND_CREDENTIAL_SURFACES:
                binding_envs = binding.get("environments")
                if kind != "environment" or not isinstance(binding_envs, list) or set(binding_envs) != set(environments):
                    raise ValueError(f"{logical_id}/{name}: binding environments must exactly match logical environment scope")
            elif binding.get("environments"):
                raise ValueError(f"{logical_id}/{name}: repository/non-environment binding cannot declare environments")
            previous = owners.get(name)
            if previous is not None and previous != logical_id:
                raise ValueError(f"binding {name} belongs to multiple authorities: {previous}, {logical_id}")
            owners[name] = logical_id
            if binding.get("declaration_only") is True:
                declaration_only.add(name)

    missing = sorted(set(detected) - set(owners))
    if missing:
        details = ", ".join(f"{name} ({sorted(detected[name])})" for name in missing)
        raise ValueError(f"tracked credential bindings missing canonical authority: {details}")
    stale = sorted(set(owners) - set(detected) - declaration_only)
    if stale:
        raise ValueError("authority has non-detected bindings without declaration_only=true: " + ", ".join(stale))


def validate_credential_authority_source() -> tuple[dict[str, Any], dict[str, set[str]]]:
    path = ROOT / CREDENTIAL_AUTHORITY
    if not path.is_file():
        raise SystemExit(f"AR-8B credential source authority missing: {CREDENTIAL_AUTHORITY}")
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise SystemExit("AR-8B credential authority must be one JSON object")
    files = tracked_credential_files()
    scan_tracked_credential_material(files)
    detected = discover_credential_bindings(files)
    validate_credential_authority(payload, detected)
    return payload, detected


def load_credential_authority() -> dict[str, Any]:
    payload, _ = validate_credential_authority_source()
    return payload


def credential_negative_self_test(payload: dict[str, Any], detected: dict[str, set[str]]) -> None:
    def must_reject(label: str, candidate: dict[str, Any], detected_override: dict[str, set[str]] | None = None) -> None:
        try:
            validate_credential_authority(candidate, detected if detected_override is None else detected_override)
        except ValueError:
            return
        raise SystemExit(f"AR-8B negative fixture unexpectedly passed: {label}")

    plaintext = copy.deepcopy(payload)
    plaintext["credentials"][0]["value"] = "forbidden"
    must_reject("plaintext/value-bearing field", plaintext)

    high_confidence = copy.deepcopy(payload)
    high_confidence["credentials"][0]["rotation_recovery_policy"] = "github_pat_" + ("A" * 20)
    must_reject("high-confidence credential material", high_confidence)

    environment_index = next(
        (
            index
            for index, entry in enumerate(payload.get("credentials", []))
            if isinstance(entry, dict)
            and isinstance(entry.get("environment_scope"), dict)
            and entry["environment_scope"].get("kind") == "environment"
        ),
        None,
    )
    if environment_index is None:
        raise SystemExit("AR-8B self-test requires at least one environment-scoped credential fixture")

    unknown_environment = copy.deepcopy(payload)
    unknown_environment["credentials"][environment_index]["environment_scope"]["environments"] = ["prod"]
    must_reject("unknown environment", unknown_environment)

    missing_lifecycle = copy.deepcopy(payload)
    missing_lifecycle["credentials"][0].pop("rotation_recovery_policy")
    must_reject("missing lifecycle metadata", missing_lifecycle)

    duplicate_id = copy.deepcopy(payload)
    duplicate_id["credentials"].append(copy.deepcopy(duplicate_id["credentials"][0]))
    must_reject("duplicate logical authority", duplicate_id)

    binding_sources = [
        index
        for index, entry in enumerate(payload.get("credentials", []))
        if isinstance(entry, dict) and isinstance(entry.get("bindings"), list) and entry["bindings"]
    ]
    if len(binding_sources) < 2:
        raise SystemExit("AR-8B self-test requires two credentials with bindings")
    dual_authority = copy.deepcopy(payload)
    source_index, target_index = binding_sources[0], binding_sources[1]
    dual_authority["credentials"][target_index]["bindings"].append(
        copy.deepcopy(dual_authority["credentials"][source_index]["bindings"][0])
    )
    must_reject("dual authority for one binding", dual_authority)

    environment_binding_index = next(
        (
            index
            for index, binding in enumerate(payload["credentials"][environment_index].get("bindings", []))
            if isinstance(binding, dict) and binding.get("surface") in ENVIRONMENT_BOUND_CREDENTIAL_SURFACES
        ),
        None,
    )
    if environment_binding_index is None:
        raise SystemExit("AR-8B self-test requires one environment-bound credential binding")
    wrong_binding_environment = copy.deepcopy(payload)
    wrong_binding_environment["credentials"][environment_index]["bindings"][environment_binding_index]["environments"] = ["production"]
    must_reject("binding/environment scope mismatch", wrong_binding_environment)

    synthetic = copy.deepcopy(detected)
    synthetic["AR8B_UNKNOWN_TRACKED_SECRET"] = {"tests/synthetic-workflow.yml"}
    must_reject("unknown tracked credential binding", payload, synthetic)


def print_credential_check_summary(detected: dict[str, set[str]], *, self_tested: bool) -> None:
    suffix = " and negative fixtures" if self_tested else ""
    print(
        f"AR-8B portable credential authority covers {len(detected)} tracked static bindings{suffix}; "
        "plaintext and high-confidence credential material remain forbidden."
    )


def git_blob_sha(path: Path) -> str:
    relative = path.relative_to(ROOT).as_posix()
    result = subprocess.run(
        ["git", "hash-object", f"--path={relative}", "--", relative],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    digest = result.stdout.strip().lower()
    if result.returncode != 0 or re.fullmatch(r"[0-9a-f]{40}", digest) is None:
        details = result.stderr.strip()
        raise ValueError(f"git hash-object failed for {relative}: {details or digest or result.returncode}")
    return digest


def build_ar8c_operational_lifecycle_projection(payload: dict[str, Any]) -> dict[str, object]:
    lifecycle = payload.get("ar8c_operational_lifecycle")
    if not isinstance(lifecycle, dict):
        raise SystemExit("AR-8C operational lifecycle source is missing")
    concerns = lifecycle.get("concerns")
    hosted = lifecycle.get("hosted_reconciliation")
    github = hosted.get("github") if isinstance(hosted, dict) else None
    cloudflare = hosted.get("cloudflare") if isinstance(hosted, dict) else None
    if not isinstance(concerns, list) or any(not isinstance(item, dict) for item in concerns):
        raise SystemExit("AR-8C operational lifecycle concerns must be a list of objects")
    if not isinstance(github, dict) or not isinstance(cloudflare, dict):
        raise SystemExit("AR-8C hosted reconciliation source is incomplete")
    concern_ids = sorted(
        str(item.get("id"))
        for item in concerns
        if isinstance(item.get("id"), str) and item.get("id")
    )
    if len(concern_ids) != len(concerns) or len(set(concern_ids)) != len(concern_ids):
        raise SystemExit("AR-8C operational lifecycle requires unique stable concern ids")
    return {
        "schema_version": int(lifecycle["schema_version"]),
        "status": str(lifecycle["status"]),
        "implementation_issue": int(lifecycle["implementation_issue"]),
        "accepted_base": str(lifecycle["accepted_base"]),
        "metadata_only": lifecycle.get("metadata_only") is True,
        "stage_order": lifecycle["stage_order"],
        "concern_ids": concern_ids,
        "hosted_reconciliation": {
            "github": {
                "accepted_main_only": github.get("accepted_main_only") is True,
                "pull_request_exposure": github.get("pull_request_exposure") is True,
                "metadata_only": github.get("metadata_only") is True,
                "readback_values": github.get("readback_values") is True,
                "executor_binding": github.get("executor_binding"),
            },
            "cloudflare": {
                "accepted_main_only": cloudflare.get("accepted_main_only") is True,
                "audit_environment": cloudflare.get("audit_environment"),
                "read_only": cloudflare.get("read_only") is True,
                "api_token_binding": cloudflare.get("api_token_binding"),
                "verify_endpoint": cloudflare.get("verify_endpoint"),
                "required_token_status": cloudflare.get("required_token_status"),
                "worker_secret_contract_source": cloudflare.get("worker_secret_contract_source"),
            },
        },
        "production_mutation": lifecycle.get("production_mutation") is True,
        "opsctl_mutation": lifecycle.get("opsctl_mutation") is True,
    }


def build_credential_projection(payload: dict[str, Any]) -> dict[str, object]:
    path = ROOT / CREDENTIAL_AUTHORITY
    sections = ("credentials", "dynamic_credential_domains", "future_trust_domains")
    entries: list[dict[str, Any]] = []
    counts: dict[str, int] = {}
    for section in sections:
        raw = payload.get(section)
        if not isinstance(raw, list) or any(not isinstance(entry, dict) for entry in raw):
            raise SystemExit(f"AR-8B credential authority {section} must be a list of objects")
        counts[section] = len(raw)
        entries.extend(raw)
    ids = [entry.get("id") for entry in entries]
    if any(not isinstance(value, str) or not value for value in ids):
        raise SystemExit("AR-8B credential authority projection requires stable logical ids")
    binding_names = sorted(
        {
            binding.get("name")
            for entry in entries
            for binding in entry.get("bindings", [])
            if isinstance(binding, dict) and isinstance(binding.get("name"), str) and binding.get("name")
        }
    )
    future_cutovers = {str(entry["id"]): entry.get("future_cutover") for entry in entries}
    if any(not isinstance(value, str) or not value for value in future_cutovers.values()):
        raise SystemExit("AR-8B credential authority projection requires future_cutover for every authority")
    return {
        "schema_version": int(payload["schema_version"]),
        "status": str(payload["status"]),
        "source_authority": CREDENTIAL_AUTHORITY,
        "source_git_blob_sha1": git_blob_sha(path),
        "metadata_only": True,
        "canonical_environments": payload["canonical_environments"],
        "invariants": payload["invariants"],
        "authority_counts": counts,
        "authority_ids": ids,
        "static_binding_names": binding_names,
        "future_cutovers": future_cutovers,
        "operational_lifecycle": build_ar8c_operational_lifecycle_projection(payload),
    }


def build_inventory() -> dict[str, object]:
    core.validate_route_ownership()
    validate_source_documents()
    application_architecture = ar3.build_projection(ROOT)
    credential_authority = load_credential_authority()
    credential_projection = build_credential_projection(credential_authority)
    routes = [
        {
            "route_class": route_class,
            "capability": capability,
            "methods": methods,
            "path_template": template,
            "example_path": example_path,
            "authenticated": authenticated,
        }
        for route_class, capability, methods, template, example_path, authenticated in core.ROUTE_SPECS
    ]
    return {
        "schema_version": 3,
        "workspace_members": core.workspace_members(),
        "d1_migrations": {"directory": "migrations/d1", "files": core.migration_files()},
        "routing": {
            "composed_entrypoint": "crates/control-plane-contract/src/lib.rs::classify_route",
            "dynamic_namespaces": ["/api", "/auth", "/bridge"],
            "classifiers": [{"capability": capability, "module": path} for capability, path in core.CLASSIFIERS],
            "public_routes": routes,
        },
        "generated_contracts": core.GENERATED_CONTRACTS,
        "application_architecture": application_architecture,
        "runtime_authority_cleanup": {
            "schema_version": 1,
            "status": "ACCEPTED_AR5_RUNTIME_AUTHORITY_CLEANUP",
            "topology_decision_source": RUNTIME_TOPOLOGY,
            "evidence": AR5_EVIDENCE,
            "implementation_issue": 290,
            "implementation_pr": 291,
            "exact_green_head": "afed435bb714794d6c4f252be6b44c592ee31b2b",
            "implementation_merge": "82d251a1d6666199c6eace393eedc1766157fcee",
            "applicable_permanent_workflows": "13/13",
            "generation_verification": {
                "topology_decision": "DELETE",
                "wrangler_producer_binding": "ABSENT",
                "runtime_contract_binding": "ABSENT",
                "deployment_manifest_identity": "ABSENT",
                "queue_workload": "ABSENT",
                "verification_authority": "SYNCHRONOUS_APPLICATION_ROUTE",
            },
            "preserved_queue_producers": ["INTEGRATION_EVENTS", "MAILBOX_JOBS"],
            "application_architecture_accepted_through": "AR-4C",
            "ar4d": "NOT_REQUIRED_UNLESS_LATER_ACCEPTED_EVIDENCE_REOPENS",
            "production_mutation": False,
            "next_required_slice": "AR-6",
        },
        "python_operational_authority": {
            "schema_version": 1,
            "status": "ACCEPTED_AR6_PYTHON_OPSCTL_FOUNDATION",
            "evidence": AR6_EVIDENCE,
            "python_estate": PYTHON_ESTATE,
            "implementation_issue": 294,
            "implementation_pr": 295,
            "exact_green_head": "9b06d542873ffa3122e53e107105098e21f5933c",
            "implementation_merge": "d0229fedd81ee870822b6d9394bc4ee313ea3a3c",
            "applicable_permanent_workflows": "13/13",
            "python_summary": {
                "tracked_python_files": 116,
                "KEEP_PYTHON": 108,
                "MIGRATE_TO_RUST": 2,
                "DELETE_AFTER_SEQUENCE": 6,
                "WRAP_WITH_RUST": 0,
            },
            "opsctl": {
                "path": "tools/opsctl",
                "mode": "READ_ONLY_FOUNDATION",
                "commands": ["doctor", "status", "inventory"],
                "third_party_dependencies": False,
                "provider_mutation": False,
            },
            "future_cutovers": {"AR-10": "DELETE_AFTER_SEQUENCE", "AR-11": "MIGRATE_TO_RUST"},
            "application_architecture_accepted_through": "AR-4C",
            "runtime_authority_cleanup_accepted_through": "AR-5",
            "production_mutation": False,
            "next_required_slice": "AR-7",
        },
        "github_governance_authority": {
            "schema_version": 1,
            "status": "ACCEPTED_AR7_GITHUB_GOVERNANCE",
            "evidence": AR7_EVIDENCE,
            "contract": GOVERNANCE_CONTRACT,
            "validator": ".github/scripts/github-governance.mjs",
            "workflow": ".github/workflows/github-governance-gate.yml",
            "implementation_issue": 298,
            "implementation_pr": 299,
            "exact_green_head": "1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7",
            "implementation_merge": "3492273cb9237850e3fa27343cc5edbdb0f66aa1",
            "applicable_permanent_workflows": "14/14",
            "hosted_audit": {"run_id": 31953316327, "contract_job": "success", "hosted_state_job": "success"},
            "direct_main_negative_probe": {
                "result": "HTTP_409_REJECTED",
                "message": "Changes must be made through a pull request. 21 of 21 required status checks are expected.",
                "sentinel_present_after_probe": False,
            },
            "main_protection": {
                "mechanism": "classic_branch_protection",
                "required_check_count": 21,
                "require_pull_request": True,
                "require_conversation_resolution": True,
                "enforce_admins": True,
                "strict_required_status_checks": True,
                "allow_force_pushes": False,
                "allow_deletions": False,
            },
            "environments": {
                "rehearsal": {"allowed_branches": ["main"], "minimum_reviewers": 0},
                "staging": {"allowed_branches": ["main"], "minimum_reviewers": 0},
                "production": {"allowed_branches": ["main"], "minimum_reviewers": 1, "can_admins_bypass": False},
            },
            "production_mutation": False,
            "next_required_slice": "AR-8",
        },
        "credential_authority": credential_projection,
        "documentation_authority": {
            "current_program": CURRENT_AUTHORITY,
            "tracking_issue": TRACKING_ISSUE,
            "current_slice": CURRENT_SLICE,
            "transition": TRANSITION,
            "runtime_topology_decision": RUNTIME_TOPOLOGY,
            "runtime_topology_evidence": AR2_EVIDENCE,
            "runtime_topology_projection_owner": "AR-3",
            "runtime_authority_cleanup_evidence": AR5_EVIDENCE,
            "python_operational_evidence": AR6_EVIDENCE,
            "python_estate": PYTHON_ESTATE,
            "github_governance_evidence": AR7_EVIDENCE,
            "github_governance_contract": GOVERNANCE_CONTRACT,
            "credential_authority_source": CREDENTIAL_AUTHORITY,
            "credential_authority_projection": "architecture/inventory.json::credential_authority",
            "ar8_umbrella_issue": AR8_UMBRELLA_ISSUE,
            "application_architecture_evidence": AR4C_EVIDENCE,
            "application_architecture_base_evidence": ar3.AR3_EVIDENCE,
            "application_architecture_projection": "architecture/inventory.json::application_architecture",
            "development_projection": "docs/DEVELOPMENT_PLAN.md",
            "readiness_projection": "docs/status.json",
            "index": "docs/INDEX.md",
            "architecture": "docs/ARCHITECTURE.md",
            "data_classification": "docs/DATA_CLASSIFICATION.md",
            "ui_target": "docs/UI_ARCHITECTURE.md",
            "accepted_capabilities": "docs/DEVELOPER_CAPABILITY_MATRIX.md",
            "security": "docs/THREAT_MODEL.md",
            "accepted_phase_ledger": "architecture/accepted-phases.json",
            "historical_pre2j_product_readiness": "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md",
            "historical_pre2j_architecture_closeout": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",
        },
        "document_status": DOCUMENT_STATUS,
        "program_state": {
            "accepted_product_phase": "Phase 2I",
            "accepted_architecture_slices": ACCEPTED_SLICES,
            "current_architecture_slice": CURRENT_SLICE,
            "next_architecture_slice_after_acceptance": NEXT_SLICE,
            "ar8_progress": expected_ar8_progress(),
            "ar8b_acceptance": ar8b_acceptance(),
            "architecture_complete": False,
            "production_core_gate": "BLOCKED",
            "production_ready": False,
            "production_mutation_allowed_during_ar0_ar17": False,
            "production_ready_authority": "PC-1_AFTER_AR-17_AUTHORIZATION",
        },
    }


def serialized(inventory: dict[str, object]) -> str:
    return json.dumps(inventory, indent=2, ensure_ascii=False) + "\n"


def check_current(expected: dict[str, object]) -> None:
    if not INVENTORY_PATH.is_file():
        raise SystemExit(f"architecture inventory is missing: {INVENTORY_PATH.relative_to(ROOT)}")
    if INVENTORY_PATH.read_text(encoding="utf-8") != serialized(expected):
        raise SystemExit("architecture/inventory.json is stale; run python scripts/generate-architecture-inventory.py --write")


def self_test(expected: dict[str, object]) -> None:
    workspace = copy.deepcopy(expected)
    workspace["workspace_members"] = [*workspace["workspace_members"], "crates/does-not-exist"]
    if serialized(workspace) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect workspace drift")
    authority = copy.deepcopy(expected)
    authority["documentation_authority"]["current_program"] = "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"
    if serialized(authority) == serialized(expected):
        raise SystemExit("inventory self-test failed to distinguish current/historical program authority")
    state = copy.deepcopy(expected)
    state["program_state"]["current_architecture_slice"] = "AR-7"
    if serialized(state) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect active AR-8 rollback")
    ar8_state = copy.deepcopy(expected)
    ar8_state["program_state"]["ar8_progress"]["current_subslice"] = "AR-8D"
    if serialized(ar8_state) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect AR-8C sequencing drift")
    credential = copy.deepcopy(expected)
    credential["credential_authority"]["metadata_only"] = False
    if serialized(credential) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect credential-authority projection drift")
    credential_identity = copy.deepcopy(expected)
    credential_identity["credential_authority"]["source_git_blob_sha1"] = "0" * 40
    if serialized(credential_identity) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect credential-authority source identity drift")
    topology = copy.deepcopy(expected)
    topology["documentation_authority"]["runtime_topology_decision"] = "architecture/other.json"
    if serialized(topology) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect topology authority drift")
    runtime_cleanup = copy.deepcopy(expected)
    runtime_cleanup["runtime_authority_cleanup"]["status"] = "AR5_CANDIDATE"
    if serialized(runtime_cleanup) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect AR-5 runtime-authority acceptance rollback")
    ownership = copy.deepcopy(expected)
    ownership["application_architecture"]["capability_ownership"][0]["application_owner"] = "apps/control-plane-worker"
    if serialized(ownership) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect application ownership drift")
    missing_resource = copy.deepcopy(expected)
    missing_resource["application_architecture"]["runtime_resources"] = missing_resource["application_architecture"]["runtime_resources"][1:]
    if serialized(missing_resource) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect missing runtime-resource projection")
    ar4d = copy.deepcopy(expected)
    ar4d["application_architecture"]["conditional_ar4d"]["decision"] = "REQUIRED"
    if serialized(ar4d) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect unsupported AR-4D activation")
    gate = copy.deepcopy(expected)
    gate["program_state"]["production_core_gate"] = "AUTHORIZED"
    if serialized(gate) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect premature Production Core authorization")
    credential_payload, detected = validate_credential_authority_source()
    credential_negative_self_test(credential_payload, detected)
    print_credential_check_summary(detected, self_tested=True)
    ar3.negative_self_test(ROOT)
    documentation = subprocess.run(
        [sys.executable, str(ROOT / "scripts/check-documentation-authority.py"), "--root", str(ROOT), "--self-test"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if documentation.returncode != 0:
        details = "\n".join(value.strip() for value in (documentation.stdout, documentation.stderr) if value.strip())
        raise SystemExit(f"documentation authority negative self-test failed:\n{details}")
    if documentation.stdout.strip():
        print(documentation.stdout.strip())
    python_ops = copy.deepcopy(expected)
    python_ops["python_operational_authority"]["status"] = "AR6_CANDIDATE"
    if serialized(python_ops) == serialized(expected):
        raise SystemExit("inventory self-test failed to detect AR-6 Python/opsctl acceptance rollback")
    print("Architecture inventory active AR-8 / current AR-8B negative self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    mode.add_argument("--credential-self-test", action="store_true")
    args = parser.parse_args()

    if args.credential_self_test:
        payload, detected = validate_credential_authority_source()
        credential_negative_self_test(payload, detected)
        print_credential_check_summary(detected, self_tested=True)
        return 0

    expected = build_inventory()
    if args.write:
        INVENTORY_PATH.parent.mkdir(parents=True, exist_ok=True)
        INVENTORY_PATH.write_text(serialized(expected), encoding="utf-8", newline="\n")
        print(f"Wrote {INVENTORY_PATH.relative_to(ROOT)}")
    elif args.check:
        check_current(expected)
        validate_full_documentation_authority()
        print("Architecture inventory projects active AR-8 with AR-8A accepted and AR-8B current.")
    else:
        self_test(expected)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"inventory verification failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
