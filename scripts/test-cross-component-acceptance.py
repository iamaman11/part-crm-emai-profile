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
        if (
            not isinstance(value, str)
            or not value.startswith(prefix)
            or "/" in value
            or "\\" in value
        ):
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
            if (
                not isinstance(relative, str)
                or relative.startswith("/")
                or ".." in Path(relative).parts
            ):
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
            "OwnerBootstrapApi",
            "OwnerTransferApi",
            "InvitationCollectionApi",
            "InvitationAcceptApi",
            "MembershipStatusApi",
            "ClientGrantApi",
            "ProfileAssignmentApi",
            "ProfileGrantApi",
        ],
        "Worker route contract",
    )

    if (ROOT / "apps/control-plane-worker/src/api.rs").exists():
        fail("legacy api.rs must remain removed after identity application-boundary migration")

    worker_lib = read("apps/control-plane-worker/src/lib.rs")
    require_all(
        worker_lib,
        [
            "RouteClass::OwnerBootstrapApi",
            "RouteClass::OwnerTransferApi",
            "RouteClass::InvitationCollectionApi",
            "RouteClass::InvitationAcceptApi",
            "RouteClass::MembershipStatusApi",
            "identity::dispatch(route, &mut request, &env).await",
            "RouteClass::ClientCollectionApi",
            "RouteClass::ClientResourceApi",
            "RouteClass::ClientGrantApi",
            "clients::dispatch(route, &mut request, &env).await",
            "RouteClass::ProfileCollectionApi",
            "RouteClass::ProfileResourceApi",
            "RouteClass::ProfileAssignmentApi",
            "RouteClass::ProfileGrantApi",
            "profiles::dispatch(route, &mut request, &env).await",
            "RouteClass::ProfileGenerationCollectionApi",
            "profile_generations::dispatch(route, &mut request, &env).await",
            "RouteClass::MailboxBindingCollectionApi",
            "mailbox_bindings::dispatch(route, &mut request, &env).await",
            "RouteClass::MailboxJobCollectionApi",
            "mailbox_jobs::dispatch(route, &mut request, &env).await",
        ],
        "application-boundary Worker routing composition",
    )

    identity_transport = read("apps/control-plane-worker/src/identity.rs")
    require_all(
        identity_transport,
        [
            "RouteClass::OwnerBootstrapApi",
            "RouteClass::OwnerTransferApi",
            "RouteClass::InvitationCollectionApi",
            "RouteClass::InvitationAcceptApi",
            "RouteClass::MembershipStatusApi",
            "execute_owner_bootstrap",
            "execute_owner_transfer",
            "execute_invitation_create",
            "execute_invitation_accept",
            "execute_membership_status",
            "authorize_identity_governance",
            "identity_governance_application(env)",
            "identity_ceremony_application(env, verified.identity().clone())",
        ],
        "identity Worker application transport",
    )
    for forbidden in (
        "cloudflare_adapters::d1_",
        "D1GovernedCommandRepository",
        "D1IdempotencyRepository",
        "D1IdentityAclRepository",
        "D1InvitationAcceptanceRepository",
        "OwnerTransferMutation",
        "CreateInvitationMutation",
        "MembershipStatusMutation",
        "BootstrapOwnerMutation",
        "AcceptInvitationMutation",
    ):
        if forbidden in identity_transport:
            fail(f"identity Worker transport regained provider orchestration: {forbidden}")

    identity_governance_use_cases = read(
        "crates/use-cases-identity/src/identity_governance.rs"
    )
    require_all(
        identity_governance_use_cases,
        [
            "pub async fn execute_owner_transfer",
            "pub async fn execute_invitation_create",
            "pub async fn execute_membership_status",
            "pub fn authorize_identity_governance",
            "decide_identity_replay",
            '"membership.activate"',
            '"membership.suspend"',
            '"membership.revoke"',
            "IdentityGovernancePortErrorClass::Conflict",
        ],
        "identity governance application use cases",
    )
    identity_ceremony_use_cases = read(
        "crates/use-cases-identity/src/identity_ceremonies.rs"
    )
    require_all(
        identity_ceremony_use_cases,
        [
            "pub async fn execute_owner_bootstrap",
            "pub async fn execute_invitation_accept",
            "find_active_identity_binding",
            "tenant_identity_boundary",
            "decide_ceremony_replay",
            '"tenant.owner_bootstrap"',
            '"invitation.accept"',
            "IdentityGovernancePortErrorClass::Conflict",
        ],
        "identity ceremony application use cases",
    )
    identity_governance_adapter = read(
        "crates/cloudflare-adapters/src/d1_identity_governance.rs"
    )
    require_all(
        identity_governance_adapter,
        [
            "impl ActiveOwnerGovernanceApplicationPort for D1IdentityGovernanceApplicationRepository",
            "OwnerTransferMutation",
            "CreateInvitationMutation",
            "MembershipStatusMutation",
            ".transfer_owner(",
            ".create_invitation(",
            ".update_membership_status(",
            "D1IdempotencyRepository",
        ],
        "identity governance D1 application adapter",
    )
    identity_ceremony_adapter = read(
        "crates/cloudflare-adapters/src/d1_identity_ceremonies.rs"
    )
    require_all(
        identity_ceremony_adapter,
        [
            "impl IdentityCeremonyApplicationPort for D1IdentityCeremonyApplicationRepository",
            "VerifiedBootstrapContext::from_verified_identity",
            "BootstrapOwnerMutation",
            "AcceptInvitationMutation",
            ".bootstrap_owner(",
            ".accept(",
            "D1IdempotencyRepository",
        ],
        "identity ceremony D1 application adapter",
    )

    client_transport = read("apps/control-plane-worker/src/clients.rs")
    require_all(
        client_transport,
        [
            "RouteClass::ClientGrantApi",
            "execute_create_client",
            "get_visible_client",
            "execute_client_grant",
            "authorize_client_grant",
            "client_application(env)",
        ],
        "client Worker application transport",
    )
    client_use_cases = read("crates/use-cases-clients/src/clients.rs")
    require_all(
        client_use_cases,
        ["pub async fn execute_create_client", "pub async fn get_visible_client"],
        "client create/query application use cases",
    )
    client_grant_use_cases = read("crates/use-cases-clients/src/client_grants.rs")
    require_all(
        client_grant_use_cases,
        [
            "pub async fn execute_client_grant",
            "pub fn authorize_client_grant",
            "pub fn next_client_grant_version",
            "decide_client_grant_replay",
            "ClientGrantPortErrorClass::Conflict",
        ],
        "client grant application use cases",
    )
    client_adapter = read("crates/cloudflare-adapters/src/d1_clients.rs")
    require_all(
        client_adapter,
        [
            "impl ClientApplicationPort for D1ClientApplicationRepository",
            "impl ClientGrantApplicationPort for D1ClientApplicationRepository",
            "ClientGrantMutation",
            ".grant_client(actor, mutation)",
            ".revoke_client_grant(actor, mutation)",
            "D1IdempotencyRepository",
        ],
        "client D1 application adapter",
    )

    profile_transport = read("apps/control-plane-worker/src/profiles.rs")
    require_all(
        profile_transport,
        [
            "RouteClass::ProfileGrantApi",
            "execute_create_profile",
            "get_visible_profile",
            "execute_assign_profile",
            "authorize_profile_assignment",
            "next_profile_assignment_version",
            "execute_profile_grant",
            "authorize_profile_grant",
            "profile_application(env)",
        ],
        "profile Worker application transport",
    )
    profile_use_cases = read("crates/use-cases/src/profiles.rs")
    require_all(
        profile_use_cases,
        ["pub async fn execute_create_profile", "pub async fn get_visible_profile"],
        "profile create/query application use cases",
    )
    assignment_use_cases = read("crates/use-cases/src/profile_assignments.rs")
    require_all(
        assignment_use_cases,
        [
            "pub async fn execute_assign_profile",
            "pub fn authorize_profile_assignment",
            "pub fn next_profile_assignment_version",
            "decide_assignment_replay",
            "ProfileAssignmentPortErrorClass::Conflict",
        ],
        "profile assignment application use cases",
    )
    grant_use_cases = read("crates/use-cases/src/profile_grants.rs")
    require_all(
        grant_use_cases,
        [
            "pub async fn execute_profile_grant",
            "pub fn authorize_profile_grant",
            "pub fn next_profile_grant_version",
            "decide_profile_grant_replay",
            "ProfileGrantPortErrorClass::Conflict",
        ],
        "profile grant application use cases",
    )
    profile_adapter = read("crates/cloudflare-adapters/src/d1_profiles.rs")
    require_all(
        profile_adapter,
        [
            "impl ProfileApplicationPort for D1ProfileApplicationRepository",
            "impl ProfileAssignmentApplicationPort for D1ProfileApplicationRepository",
            "impl ProfileGrantApplicationPort for D1ProfileApplicationRepository",
            "AssignProfileMutation",
            ".assign_profile(actor, mutation)",
            "ProfileGrantMutation",
            ".grant_profile(actor, mutation)",
            ".revoke_profile_grant(actor, mutation)",
            "D1IdempotencyRepository",
        ],
        "profile D1 application adapter",
    )
    governed_commands = read("crates/cloudflare-adapters/src/d1_governed_commands.rs")
    require_all(
        governed_commands,
        [
            "owner_transfer_commands",
            "membership_status_commands",
            "invitation_create_commands",
            "client_grant_commands",
            "pub async fn grant_client",
            "pub async fn revoke_client_grant",
            "expected_client_version",
            "profile_assignment_commands",
            "pub async fn assign_profile",
            "profile_grant_commands",
            "pub async fn grant_profile",
            "pub async fn revoke_profile_grant",
            "expected_profile_version",
            '"profile.assign_client"',
        ],
        "identity/client/profile governed atomic D1 commands",
    )

    generation_transport = read("apps/control-plane-worker/src/profile_generations.rs")
    require_all(
        generation_transport,
        [
            "execute_register_generation",
            "get_visible_generation",
            "execute_verify_generation",
            "execute_activate_generation",
            "execute_deactivate_generation",
            "execute_quarantine_generation",
            "profile_generation_application(env)",
            "authorize_generation_mutation(role)",
        ],
        "generation Worker application transport",
    )
    generation_use_cases = read("crates/use-cases/src/generations.rs")
    require_all(
        generation_use_cases,
        [
            "pub async fn execute_register_generation",
            "pub async fn get_visible_generation",
            "pub async fn execute_verify_generation",
            "pub async fn execute_activate_generation",
            "pub async fn execute_deactivate_generation",
            "pub async fn execute_quarantine_generation",
            "GenerationReplayDecision::Replay",
            "next_generation_version",
        ],
        "generation application use cases",
    )
    generation_application_adapter = read(
        "crates/cloudflare-adapters/src/d1_profile_generation_application.rs"
    )
    require_all(
        generation_application_adapter,
        [
            "impl GenerationApplicationPort for D1ProfileGenerationApplicationRepository",
            ".register(",
            ".verify(",
            ".activate(",
            ".deactivate(",
            ".quarantine(",
            ".find_visible(",
            "D1IdempotencyRepository",
        ],
        "generation D1 application adapter",
    )
    generation = read("crates/cloudflare-adapters/src/d1_profile_generations.rs")
    require_all(
        generation,
        [
            "GenerationStatus::Verified",
            "profile_generation_activate_commands",
            "expected_profile_version",
            "profile_generation.verify",
            "profile_generation.quarantine",
            "expected_generation_version",
        ],
        "immutable generation atomic D1 adapter",
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

    mailbox_binding = read("apps/control-plane-worker/src/mailbox_bindings.rs")
    require_all(
        mailbox_binding,
        [
            "deny_unknown_fields",
            '"password":"forbidden"',
            '"messageBody":"forbidden"',
            "SecretHandle::parse",
            "execute_create_mailbox_binding",
            "execute_revoke_mailbox_binding",
            "get_mailbox_binding",
            "mailbox_binding_application(env)",
        ],
        "mailbox binding metadata-only application boundary",
    )
    mailbox_jobs = read("apps/control-plane-worker/src/mailbox_jobs.rs")
    require_all(
        mailbox_jobs,
        [
            "deny_unknown_fields",
            '"messageBody":"forbidden"',
            "execute_create_mailbox_job",
            "get_mailbox_job",
            "execute_run_mailbox_job",
            "mailbox_job_application(env)",
            "validate_create_mailbox_job_request",
            "validate_mailbox_job_run_version",
        ],
        "mailbox job application transport",
    )
    mailbox_job_use_cases = read("crates/use-cases/src/mailbox_jobs.rs")
    require_all(
        mailbox_job_use_cases,
        [
            "pub async fn execute_create_mailbox_job",
            "pub async fn get_mailbox_job",
            "pub async fn execute_run_mailbox_job",
            "pub fn validate_create_mailbox_job_request",
            "pub fn validate_mailbox_job_run_version",
            "prepare_run",
        ],
        "mailbox job application use cases",
    )
    mailbox_job_adapter = read("crates/cloudflare-adapters/src/d1_mailbox_jobs.rs")
    require_all(
        mailbox_job_adapter,
        [
            "D1MailboxJobApplicationRepository",
            "MetadataMailboxProviderAdapter",
            "decide_mailbox_run",
            "CreateMailboxJobMutation",
            "RunMailboxJobMutation",
            "type RunDecision = MailboxRunDecision",
        ],
        "mailbox job D1/provider application adapter",
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
    require_all(
        neutral,
        ["not_found", "forbidden", "Resource unavailable"],
        "neutral UI disclosure test",
    )


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
