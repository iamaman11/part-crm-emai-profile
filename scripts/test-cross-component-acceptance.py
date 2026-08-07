#!/usr/bin/env python3
"""Validate the deterministic repository-local cross-component acceptance contract."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tests/cross-component/standalone-acceptance.json"

EXPECTED_PHASES = [
    (1, "identity_tenant_membership", "active_actor_context"),
    (2, "client_profile_acl_assignment", "assignment_separate_from_explicit_grants"),
    (3, "immutable_generation", "verified_generation_required_for_activation"),
    (4, "coordinator_bridge", "synthetic_operator_closes_dirty_local_without_cleanup_failure"),
    (5, "mailbox_metadata_job", "metadata_only_synthetic_provider_job"),
    (6, "react_operator_projection", "same_origin_typed_ui_projection"),
]
EXPECTED_NEGATIVE_EVIDENCE = {
    "assignmentDoesNotAuthorize",
    "foreignResourceDisclosureIsNeutral",
    "unverifiedGenerationActivationRejected",
    "coordinatorLeaseMismatchFailsClosed",
    "mailboxSensitivePayloadRejected",
    "unknownDynamicRoutesFailClosed",
    "browserCredentialPersistenceRejected",
}
EXPECTED_EXTERNAL_EXCLUSIONS = {
    "cloudflare_production_deployment",
    "real_camoufox_execution",
    "real_mailbox_provider_execution",
    "production_device_key_protection",
    "trusted_signing",
    "physical_multi_device_acceptance",
}
ID_PREFIXES = {
    "tenantId": "tenant_",
    "actorId": "actor_",
    "clientId": "client_",
    "profileId": "profile_",
    "generationId": "generation_",
    "deviceId": "device_",
    "mailboxBindingId": "mailbox_",
    "mailboxJobId": "mailjob_",
}
FORBIDDEN_MANIFEST_KEYS = re.compile(
    r"(?:password|access.?token|oauth|cookie|message.?body|credential.?value|raw.?secret)",
    re.IGNORECASE,
)


def fail(message: str) -> None:
    raise AssertionError(message)


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def require_all(text: str, snippets: list[str], label: str) -> None:
    missing = [snippet for snippet in snippets if snippet not in text]
    if missing:
        fail(f"{label} is missing required acceptance surfaces: {missing}")


def walk_json(value: object, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, nested in value.items():
            if FORBIDDEN_MANIFEST_KEYS.search(str(key)):
                fail(f"forbidden sensitive manifest key at {path}.{key}")
            walk_json(nested, f"{path}.{key}")
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            walk_json(nested, f"{path}[{index}]")
    elif isinstance(value, str):
        if "@" in value:
            fail(f"manifest must not contain email-like data at {path}")
        if "profilebridge://" in value:
            fail(f"manifest must not contain live-looking claim URIs at {path}")


def validate_manifest() -> dict[str, object]:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if manifest.get("schemaVersion") != 1:
        fail("unexpected cross-component manifest schema version")
    if manifest.get("scope") != "repository-local-synthetic":
        fail("cross-component scope must remain repository-local-synthetic")
    if manifest.get("productionReady") is not False:
        fail("cross-component evidence must not promote production readiness")

    walk_json(manifest)

    identity = manifest.get("disposableIdentity")
    if not isinstance(identity, dict) or set(identity) != set(ID_PREFIXES):
        fail("disposable identity fields are incomplete or unexpected")
    for field, prefix in ID_PREFIXES.items():
        value = identity[field]
        if not isinstance(value, str) or not value.startswith(prefix) or "/" in value or "\\" in value:
            fail(f"invalid opaque disposable identifier: {field}")

    phases = manifest.get("phases")
    if not isinstance(phases, list) or len(phases) != len(EXPECTED_PHASES):
        fail("cross-component phases are incomplete")
    for actual, expected in zip(phases, EXPECTED_PHASES, strict=True):
        if not isinstance(actual, dict):
            fail("phase entry must be an object")
        order, name, outcome = expected
        if (actual.get("order"), actual.get("name"), actual.get("expectedOutcome")) != expected:
            fail(f"phase {order} does not match the accepted ordered flow")
        evidence = actual.get("evidence")
        if not isinstance(evidence, list) or not evidence:
            fail(f"phase {order} has no evidence references")
        for relative in evidence:
            if not isinstance(relative, str) or relative.startswith("/") or ".." in Path(relative).parts:
                fail(f"unsafe evidence path in phase {order}")
            if not (ROOT / relative).is_file():
                fail(f"missing evidence file for phase {order}: {relative}")

    negative = manifest.get("negativeEvidence")
    if not isinstance(negative, dict) or set(negative) != EXPECTED_NEGATIVE_EVIDENCE:
        fail("negative evidence matrix is incomplete")
    if any(value is not True for value in negative.values()):
        fail("every required negative-evidence claim must be explicitly enabled")

    exclusions = manifest.get("externalEvidenceExcluded")
    if not isinstance(exclusions, list) or set(exclusions) != EXPECTED_EXTERNAL_EXCLUSIONS:
        fail("external-evidence exclusions changed unexpectedly")
    return manifest


def validate_composition_surfaces() -> None:
    contract = read("crates/control-plane-contract/src/lib.rs")
    require_all(
        contract,
        [
            "DynamicRouteNotFound",
            "BridgeDeniedByDefault",
            'path == "/api"',
            'path == "/auth"',
            "MailboxBindingCollectionApi",
            "MailboxJobRunApi",
            "ProfileGenerationActivateApi",
            "ProfileCoordinatorApi",
        ],
        "Worker route contract",
    )

    api = read("apps/control-plane-worker/src/api.rs")
    require_all(
        api,
        [
            "OwnerBootstrapApi",
            "ProfileCollectionApi",
            "ProfileAssignmentApi",
            "ProfileGrantApi",
            "find_visible_profile",
        ],
        "identity/profile Worker composition",
    )

    worker_lib = read("apps/control-plane-worker/src/lib.rs")
    require_all(
        worker_lib,
        [
            "RouteClass::ClientCollectionApi | RouteClass::ClientResourceApi",
            "clients::dispatch(route, &mut request, &env).await",
        ],
        "client Worker routing composition",
    )
    client_transport = read("apps/control-plane-worker/src/clients.rs")
    require_all(
        client_transport,
        ["execute_create_client", "get_visible_client", "client_application(env)"],
        "client Worker application transport",
    )
    client_use_cases = read("crates/use-cases/src/clients.rs")
    require_all(
        client_use_cases,
        ["pub async fn execute_create_client", "pub async fn get_visible_client"],
        "client application use cases",
    )

    generation = read("crates/cloudflare-adapters/src/d1_profile_generations.rs")
    require_all(
        generation,
        [
            "GenerationStatus::Verified",
            "profile_generation_activate_commands",
            "expected_profile_version",
            "profile_generation.verify",
        ],
        "immutable generation adapter",
    )

    operator = read("apps/profile-bridge/src/operator_flow.rs")
    require_all(
        operator,
        [
            "lease.tenant_id() != enrollment.actor().tenant_scope().tenant_id()",
            "lease.profile_id() != enrollment.profile_id()",
            "lease.device_id() != &device_id",
            "RuntimeSessionOrchestrator::launch",
            "LocalGenerationState::MaterializedClean",
        ],
        "Profile Bridge operator flow",
    )
    synthetic_bridge = read("apps/profile-bridge/src/bin/profile-bridge-synthetic.rs")
    require_all(
        synthetic_bridge,
        [
            "synthetic-operator-complete state=DIRTY_LOCAL",
            "LocalGenerationState::DirtyLocal",
            "cleanup_failures().any()",
            "ProfileBridgeOperator::new",
        ],
        "synthetic Profile Bridge executable",
    )

    mailbox = read("apps/control-plane-worker/src/mailboxes.rs")
    require_all(
        mailbox,
        [
            "deny_unknown_fields",
            '"password":"forbidden"',
            '"messageBody":"forbidden"',
            "SecretHandle::parse",
            "MetadataMailboxProviderAdapter",
        ],
        "mailbox metadata-only boundary",
    )

    endpoints = read("frontend/src/shared/api/endpoints.ts")
    require_all(
        endpoints,
        [
            "getSession",
            "createClient",
            "setClientGrant",
            "createProfile",
            "assignProfile",
            "setProfileGrant",
            "registerGeneration",
            "changeGenerationActivation",
            "getCoordinator",
            "createMailboxBinding",
            "runMailboxJob",
        ],
        "React endpoint composition",
    )
    browser_client = read("frontend/src/shared/api/client.ts")
    require_all(
        browser_client,
        [
            "same-origin /api/v1/ path",
            "credentials: 'same-origin'",
            "MAX_RESPONSE_BYTES",
            "ApiProblem",
        ],
        "React transport boundary",
    )
    for forbidden in ("localStorage", "sessionStorage", "Cf-Access-Jwt-Assertion"):
        for path in (ROOT / "frontend/src").rglob("*"):
            if path.suffix in {".ts", ".tsx"} and forbidden in path.read_text(encoding="utf-8"):
                fail(f"forbidden browser credential/token surface in application source: {path}")

    neutral = read("frontend/src/shared/ui/StatusMessage.test.tsx")
    require_all(neutral, ["not_found", "forbidden", "Resource unavailable"], "neutral UI disclosure test")


def main() -> int:
    try:
        manifest = validate_manifest()
        validate_composition_surfaces()
    except (AssertionError, json.JSONDecodeError, OSError) as error:
        print(f"cross-component acceptance failed: {error}", file=sys.stderr)
        return 1

    phases = manifest["phases"]
    print(
        json.dumps(
            {
                "result": "accepted-by-validator",
                "scope": manifest["scope"],
                "productionReady": manifest["productionReady"],
                "phaseCount": len(phases) if isinstance(phases, list) else 0,
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
