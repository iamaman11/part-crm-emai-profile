#!/usr/bin/env python3
"""Permanent Phase 2I release-candidate evidence and authority boundary checks."""

from __future__ import annotations

import argparse
import json
import re
from collections.abc import Callable
from pathlib import Path

MANIFEST = Path("tests/cross-component/phase2i-release-candidate.json")

EXPECTED_MATRICES: dict[str, set[str]] = {
    "capabilityFlows": {
        "identity",
        "clients",
        "profiles",
        "mailboxes",
        "devices",
        "realtime",
        "ui",
    },
    "negativeMatrix": {
        "tenantIsolation",
        "idorNeutralDisclosure",
        "revocationBeforeProjection",
        "resultCountNoLeak",
        "mailContentNoTechnicalChannel",
        "realtimeInvalidationOnly",
    },
    "failureMatrix": {
        "duplicateReplayNeutral",
        "staleFenceRejected",
        "terminalFailureVisible",
        "profileBusyNoFalseSuccess",
        "deviceOfflineNoFalseSuccess",
        "providerOutageClassified",
    },
    "recoveryMatrix": {
        "generationFreshness",
        "immutableCandidateBeforeActivation",
        "failedActivationPreservesRecovery",
        "mailboxAuthExpiryRemediation",
        "realtimeReconnectDurableCatchup",
        "d1CatalogBackupRestore",
        "r2ImmutableObjectRecovery",
        "coordinatorReplayRecovery",
        "bridgeDirtyLocalRecovery",
    },
}
EXPECTED_EXTERNAL_EXCLUSIONS = {
    "cloudflare_production_deployment",
    "real_camoufox_execution",
    "real_mailbox_provider_execution",
    "production_device_key_protection",
    "trusted_signing",
    "physical_multi_device_acceptance",
    "remote_r2_key_recovery",
    "independent_cryptographic_review",
}
SOURCE_FILES = {
    "query": Path("crates/use-cases-query/src/lib.rs"),
    "worker_mail": Path("apps/control-plane-worker/src/client_mail_query.rs"),
    "composition": Path("apps/control-plane-worker/src/composition.rs"),
    "realtime": Path("frontend/src/shared/realtime/NotificationRealtimeBridge.tsx"),
    "bridge": Path("apps/profile-bridge/src/operator_flow.rs"),
    "phase2g": Path("scripts/check-phase2g-realtime-boundaries.py"),
    "phase2h": Path("scripts/check-phase2h-ui-boundaries.py"),
    "historical_acceptance": Path("tests/cross-component/standalone-acceptance.json"),
}
ALLOWED_EVIDENCE_ROOTS = {"apps", "crates", "frontend", "scripts", "tests"}
FORBIDDEN_MANIFEST_KEYS = re.compile(
    r"(?:password|access.?token|oauth|cookie|message.?body|credential.?value|raw.?secret)",
    re.IGNORECASE,
)
FORBIDDEN_TECHNICAL_SINKS = ("localStorage", "sessionStorage", "indexedDB", "sendBeacon")


def walk_json(value: object, path: str = "$") -> list[str]:
    errors: list[str] = []
    if isinstance(value, dict):
        for key, nested in value.items():
            if FORBIDDEN_MANIFEST_KEYS.search(str(key)):
                errors.append(f"forbidden sensitive manifest key at {path}.{key}")
            errors.extend(walk_json(nested, f"{path}.{key}"))
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            errors.extend(walk_json(nested, f"{path}[{index}]"))
    elif isinstance(value, str):
        if "@" in value:
            errors.append(f"manifest must not contain email-like data at {path}")
        if "profilebridge://" in value:
            errors.append(f"manifest must not contain live-looking claim URIs at {path}")
    return errors


def safe_evidence_path(root: Path, relative: object, label: str) -> list[str]:
    if not isinstance(relative, str) or not relative:
        return [f"{label} evidence path must be a non-empty string"]
    path = Path(relative)
    if path.is_absolute() or ".." in path.parts:
        return [f"{label} evidence path is unsafe: {relative}"]
    if not path.parts or path.parts[0] not in ALLOWED_EVIDENCE_ROOTS:
        return [f"{label} evidence path is outside allowlisted roots: {relative}"]
    if not (root / path).is_file():
        return [f"{label} evidence file is missing: {relative}"]
    return []


def validate_matrix(root: Path, matrix: object, expected: set[str], label: str) -> list[str]:
    if not isinstance(matrix, dict):
        return [f"{label} must be an object"]
    if set(matrix) != expected:
        return [
            f"{label} keys differ from required Phase 2I matrix: "
            f"expected={sorted(expected)} actual={sorted(matrix)}"
        ]

    errors: list[str] = []
    for key, entry in matrix.items():
        if not isinstance(entry, dict) or set(entry) != {"expectedOutcome", "evidence"}:
            errors.append(f"{label}.{key} must contain exactly expectedOutcome and evidence")
            continue
        outcome = entry.get("expectedOutcome")
        evidence = entry.get("evidence")
        if not isinstance(outcome, str) or not outcome or len(outcome) > 180:
            errors.append(f"{label}.{key} expectedOutcome must be bounded non-empty text")
        if not isinstance(evidence, list) or not evidence:
            errors.append(f"{label}.{key} must have evidence references")
            continue
        for relative in evidence:
            errors.extend(safe_evidence_path(root, relative, f"{label}.{key}"))
    return errors


def validate_manifest(root: Path, manifest: object) -> list[str]:
    if not isinstance(manifest, dict):
        return ["Phase 2I manifest must be an object"]

    errors = walk_json(manifest)
    scalar_expectations = {
        "schemaVersion": 1,
        "phase": "Phase 2I",
        "scope": "repository-local-release-candidate",
        "completionState": "accepted",
        "productionReady": False,
        "baseSha": "0449e9f0576f7d26b1e1debd882cfecf92a50c53",
        "issue": 167,
        "historicalAcceptance": "tests/cross-component/standalone-acceptance.json",
    }
    for key, expected in scalar_expectations.items():
        if manifest.get(key) != expected:
            errors.append(f"Phase 2I manifest {key} changed unexpectedly")

    historical = manifest.get("historicalAcceptance")
    if historical == scalar_expectations["historicalAcceptance"]:
        errors.extend(safe_evidence_path(root, historical, "historicalAcceptance"))

    for label, expected in EXPECTED_MATRICES.items():
        errors.extend(validate_matrix(root, manifest.get(label), expected, label))

    exclusions = manifest.get("externalEvidenceExcluded")
    if not isinstance(exclusions, list) or set(exclusions) != EXPECTED_EXTERNAL_EXCLUSIONS:
        errors.append("Phase 2I External-evidence exclusion set changed unexpectedly")
    return errors


def load_sources(root: Path) -> tuple[dict[str, str], list[str]]:
    sources: dict[str, str] = {}
    errors: list[str] = []
    for key, relative in SOURCE_FILES.items():
        path = root / relative
        if not path.is_file():
            errors.append(f"missing Phase 2I governed source: {relative}")
        else:
            sources[key] = path.read_text(encoding="utf-8")
    return sources, errors


def require_all(source: str, markers: tuple[str, ...], label: str) -> list[str]:
    return [
        f"{label} missing release-candidate boundary marker: {marker}"
        for marker in markers
        if marker not in source
    ]


def validate_sources(sources: dict[str, str]) -> list[str]:
    missing = set(SOURCE_FILES) - set(sources)
    if missing:
        return [f"Phase 2I source set incomplete: {sorted(missing)}"]

    errors: list[str] = []
    errors.extend(
        require_all(
            sources["query"],
            (
                "if !authorize(actor, authorization, QueryCapability::Clients).await?",
                "if !authorize(actor, authorization, QueryCapability::Profiles).await?",
                "if !authorize(actor, authorization, QueryCapability::Members).await?",
                "if !authorize(actor, authorization, QueryCapability::Mailboxes).await?",
                "if !authorize(actor, authorization, QueryCapability::Mail).await?",
                "QueryPage::empty()",
                "list_eligible_mailboxes_for_client",
            ),
            "query authorization-before-projection",
        )
    )

    worker_mail = sources["worker_mail"]
    errors.extend(
        require_all(
            worker_mail,
            (
                "resolve_active_request_actor",
                "search_client_mailbox_messages",
                "get_client_mailbox_message",
                "query_repository(env)?",
                "client_mail_eligibility_repository(env)?",
                "client_mail_query_provider(env, actor.actor(), &client_id)?",
            ),
            "Client Mail authenticated application ingress",
        )
    )
    for forbidden in ("SELECT ", "INSERT ", "UPDATE ", "DELETE FROM"):
        if forbidden in worker_mail:
            errors.append(f"Client Mail Worker ingress contains forbidden direct SQL: {forbidden}")
    for forbidden in (
        "D1ClientMailboxEligibilityRepository",
        "D1QueryRepository",
        "CloudMailboxQueryAdapter",
    ):
        if forbidden in worker_mail:
            errors.append(
                f"Client Mail Worker ingress contains forbidden concrete adapter ownership: {forbidden}"
            )

    composition = sources["composition"]
    errors.extend(
        require_all(
            composition,
            (
                "pub fn query_repository",
                "pub fn client_mail_eligibility_repository",
                "pub fn client_mail_query_provider<'a>",
                "D1ClientMailboxEligibilityRepository::new",
                "D1QueryRepository::new",
                "CloudMailboxQueryAdapter::new",
            ),
            "Client Mail composition root",
        )
    )

    realtime = sources["realtime"]
    errors.extend(
        require_all(
            realtime,
            ("invalidateQueries", "POLICY_REVOKED_CLOSE_CODE", "RealtimeEventDeduper"),
            "realtime invalidation-only bridge",
        )
    )
    if "setQueryData" in realtime:
        errors.append("realtime bridge must not become canonical query/business-state writer")
    for sink in FORBIDDEN_TECHNICAL_SINKS:
        if sink in realtime:
            errors.append(f"realtime bridge contains forbidden browser persistence/telemetry sink: {sink}")

    errors.extend(
        require_all(
            sources["bridge"],
            (
                "OperatorFlowError::CleanupRequired",
                "OperatorFlowError::Busy",
                "retained_dirty",
                "GenerationObjectExactVerifyPort",
                "GenerationObjectUploadPort",
            ),
            "Profile Bridge fail-closed recovery",
        )
    )
    errors.extend(
        require_all(
            sources["phase2g"],
            ("invalidateQueries", "setQueryData", "--self-test"),
            "Phase 2G permanent negative policy",
        )
    )
    errors.extend(
        require_all(
            sources["phase2h"],
            ("BROWSER_PERSISTENCE_SINKS", "SafeMailBody", "--self-test"),
            "Phase 2H permanent privacy/UI policy",
        )
    )

    try:
        historical = json.loads(sources["historical_acceptance"])
    except json.JSONDecodeError as error:
        errors.append(f"historical standalone acceptance is invalid JSON: {error}")
    else:
        if historical.get("scope") != "repository-local-synthetic":
            errors.append("historical standalone acceptance scope changed unexpectedly")
        if historical.get("productionReady") is not False:
            errors.append("historical standalone acceptance must remain productionReady=false")
    return errors


def self_test(root: Path, manifest: dict[str, object], sources: dict[str, str]) -> None:
    manifest_fixtures: list[tuple[str, Callable[[dict[str, object]], None]]] = [
        ("production readiness promotion", lambda value: value.__setitem__("productionReady", True)),
        ("missing device flow", lambda value: value["capabilityFlows"].pop("devices")),  # type: ignore[union-attr]
        (
            "unsafe evidence path",
            lambda value: value["negativeMatrix"]["tenantIsolation"]["evidence"].__setitem__(0, "../secret.txt"),  # type: ignore[index,union-attr]
        ),
        (
            "missing D1 recovery drill",
            lambda value: value["recoveryMatrix"].pop("d1CatalogBackupRestore"),  # type: ignore[union-attr]
        ),
        (
            "external evidence collapse",
            lambda value: value.__setitem__("externalEvidenceExcluded", ["real_camoufox_execution"]),
        ),
    ]
    for label, mutate in manifest_fixtures:
        candidate = json.loads(json.dumps(manifest))
        mutate(candidate)
        if not validate_manifest(root, candidate):
            raise ValueError(f"Phase 2I manifest negative fixture was not rejected: {label}")

    source_fixtures = [
        ("authorization bypass", "query", "if !authorize(actor, authorization, QueryCapability::Clients).await?", "if false"),
        ("realtime authority", "realtime", "invalidateQueries", "setQueryData"),
        ("Bridge busy fail-open", "bridge", "OperatorFlowError::Busy", "OperatorFlowError::Stage"),
    ]
    for label, key, needle, replacement in source_fixtures:
        if needle not in sources[key]:
            raise ValueError(f"Phase 2I self-test fixture marker missing for {label}: {needle}")
        mutated = dict(sources)
        mutated[key] = mutated[key].replace(needle, replacement)
        if not validate_sources(mutated):
            raise ValueError(f"Phase 2I source negative fixture was not rejected: {label}")

    leaked_adapter = dict(sources)
    leaked_adapter["worker_mail"] = (
        leaked_adapter["worker_mail"] + "\nD1ClientMailboxEligibilityRepository"
    )
    if not validate_sources(leaked_adapter):
        raise ValueError(
            "Phase 2I source negative fixture was not rejected: Client Mail concrete adapter leakage"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    manifest_path = args.root / MANIFEST
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"cannot load Phase 2I release-candidate manifest: {error}")
        return 1

    sources, load_errors = load_sources(args.root)
    errors = load_errors + validate_manifest(args.root, manifest) + validate_sources(sources)
    if errors:
        for error in errors:
            print(error)
        return 1

    if args.self_test:
        try:
            self_test(args.root, manifest, sources)
        except (KeyError, TypeError, ValueError) as error:
            print(error)
            return 1
        print("Phase 2I release-candidate negative fixtures rejected as expected.")
        return 0

    print("Phase 2I release-candidate evidence and authority boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
