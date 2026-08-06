use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::request_evidence::{audit_event_id, outbox_event_id};
use cloudflare_adapters::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use cloudflare_adapters::d1_identity_acl::{
    MutationEnvelope, ResolvedActor, ResolvedMembershipRole,
};
use cloudflare_adapters::d1_profile_generations::{
    ActivateGenerationMutation, D1ProfileGenerationRepository, DeactivateGenerationMutation,
    GenerationProjection, GenerationStatus, QuarantineGenerationMutation,
    RegisterGenerationMutation, VerifyGenerationMutation,
};
use control_plane_contract::{D1_CATALOG_BINDING, RouteClass};
use profile_platform_primitives::{
    AggregateVersion, AuditEventId, GenerationId, IdempotencyKey, OutboxEventId, ProfileId,
    UnixMillis,
};
use serde::{Deserialize, Serialize};
use worker::{Date, Env, Error, Request, Response, Result};

const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const IDEMPOTENCY_TTL_MS: u64 = 86_400_000;

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let profile_id = match segments
        .get(5)
        .and_then(|value| ProfileId::parse((*value).to_owned()).ok())
    {
        Some(value) => value,
        None => return neutral_not_found(&correlation_hint(request)),
    };
    let generation_id = segments
        .get(7)
        .and_then(|value| GenerationId::parse((*value).to_owned()).ok());
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };

    match route {
        RouteClass::ProfileGenerationResourceApi => {
            let Some(generation_id) = generation_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            get_generation(env, &actor, &profile_id, &generation_id).await
        }
        RouteClass::ProfileGenerationCollectionApi => {
            if actor.role() != ResolvedMembershipRole::TenantOwner {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            }
            register_generation(request, env, &actor, &profile_id).await
        }
        RouteClass::ProfileGenerationVerifyApi => {
            if actor.role() != ResolvedMembershipRole::TenantOwner {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            }
            let Some(generation_id) = generation_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            verify_generation(request, env, &actor, &profile_id, &generation_id).await
        }
        RouteClass::ProfileGenerationActivateApi => {
            if actor.role() != ResolvedMembershipRole::TenantOwner {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            }
            let Some(generation_id) = generation_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            activate_generation(request, env, &actor, &profile_id, &generation_id).await
        }
        RouteClass::ProfileGenerationDeactivateApi => {
            if actor.role() != ResolvedMembershipRole::TenantOwner {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            }
            let Some(generation_id) = generation_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            deactivate_generation(request, env, &actor, &profile_id, &generation_id).await
        }
        RouteClass::ProfileGenerationQuarantineApi => {
            if actor.role() != ResolvedMembershipRole::TenantOwner {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            }
            let Some(generation_id) = generation_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            quarantine_generation(request, env, &actor, &profile_id, &generation_id).await
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

async fn get_generation(
    env: &Env,
    actor: &ResolvedActor,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<Response> {
    let repository = D1ProfileGenerationRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let Some(generation) = repository
        .find_visible(
            actor.actor().tenant_scope(),
            actor.actor().actor_id(),
            actor.role(),
            profile_id,
            generation_id,
        )
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    Response::from_json(&GenerationResponse::from(&generation))
}

async fn register_generation(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    profile_id: &ProfileId,
) -> Result<Response> {
    let body = match request.json::<RegisterGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let generation_id = match GenerationId::parse(body.generation_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if !valid_object_key(&body.object_key)
        || !valid_digest(&body.metadata_digest)
        || !valid_digest(&body.container_digest)
    {
        return invalid_request(request);
    }
    let envelope = match EnvelopeOwned::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        env,
        actor,
        "profile_generation.register",
        &envelope,
        generation_id.as_str(),
        1,
        201,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = RegisterGenerationMutation {
        profile_id,
        generation_id: &generation_id,
        object_key: &body.object_key,
        metadata_digest: &body.metadata_digest,
        container_digest: &body.container_digest,
        envelope: envelope.identity(),
    };
    match D1ProfileGenerationRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .register(actor.actor(), mutation)
        .await
    {
        Ok(_) => mutation_receipt("registered", generation_id.as_str(), 1, 201),
        Err(error) => mutation_failure(request, error),
    }
}

async fn verify_generation(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<Response> {
    let body = match request.json::<VerifyGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_version = match AggregateVersion::new(body.expected_generation_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if !valid_verification_reference(&body.verification_reference) {
        return invalid_request(request);
    }
    let response_version = match next_version(expected_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        env,
        actor,
        "profile_generation.verify",
        &envelope,
        generation_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = VerifyGenerationMutation {
        profile_id,
        generation_id,
        expected_generation_version: expected_version,
        verification_reference: &body.verification_reference,
        envelope: envelope.identity(),
    };
    match D1ProfileGenerationRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .verify(actor.actor(), mutation)
        .await
    {
        Ok(_) => mutation_receipt("verified", generation_id.as_str(), response_version, 200),
        Err(error) => mutation_failure(request, error),
    }
}

async fn activate_generation(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<Response> {
    let body = match request.json::<ActivateGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_version = match AggregateVersion::new(body.expected_profile_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_version(expected_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        env,
        actor,
        "profile_generation.activate",
        &envelope,
        generation_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = ActivateGenerationMutation {
        profile_id,
        generation_id,
        expected_profile_version: expected_version,
        envelope: envelope.identity(),
    };
    match D1ProfileGenerationRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .activate(actor.actor(), mutation)
        .await
    {
        Ok(_) => mutation_receipt("activated", generation_id.as_str(), response_version, 200),
        Err(error) => mutation_failure(request, error),
    }
}

async fn deactivate_generation(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<Response> {
    let body = match request.json::<DeactivateGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_version = match AggregateVersion::new(body.expected_profile_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_version(expected_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        env,
        actor,
        "profile_generation.deactivate",
        &envelope,
        generation_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = DeactivateGenerationMutation {
        profile_id,
        generation_id,
        expected_profile_version: expected_version,
        envelope: envelope.identity(),
    };
    match D1ProfileGenerationRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .deactivate(actor.actor(), mutation)
        .await
    {
        Ok(_) => mutation_receipt("deactivated", generation_id.as_str(), response_version, 200),
        Err(error) => mutation_failure(request, error),
    }
}

async fn quarantine_generation(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<Response> {
    let body = match request.json::<QuarantineGenerationRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_version = match AggregateVersion::new(body.expected_generation_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_version(expected_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        env,
        actor,
        "profile_generation.quarantine",
        &envelope,
        generation_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = QuarantineGenerationMutation {
        profile_id,
        generation_id,
        expected_generation_version: expected_version,
        envelope: envelope.identity(),
    };
    match D1ProfileGenerationRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .quarantine(actor.actor(), mutation)
        .await
    {
        Ok(_) => mutation_receipt("quarantined", generation_id.as_str(), response_version, 200),
        Err(error) => mutation_failure(request, error),
    }
}

async fn replay_response(
    env: &Env,
    actor: &ResolvedActor,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    success_status: u16,
) -> Result<Option<Response>> {
    let decision = D1IdempotencyRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .decide(
            actor.actor().tenant_scope(),
            actor.actor().actor_id(),
            &envelope.idempotency_key,
            command_name,
            &envelope.request_digest,
            envelope.now,
        )
        .await?;
    match decision {
        IdempotencyDecision::Miss => Ok(None),
        IdempotencyDecision::Replay(receipt) => mutation_receipt(
            receipt.result_code(),
            receipt.result_reference().unwrap_or(resource_id),
            aggregate_version,
            success_status,
        )
        .map(Some),
        IdempotencyDecision::Conflict => problem(
            actor.actor().correlation_id().as_str(),
            409,
            "conflict",
            "Conflict",
        )
        .map(Some),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MutationFailureClass {
    NeutralNotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    DependencyUnavailable,
}

fn classify_mutation_failure(message: &str) -> MutationFailureClass {
    if message.contains("owner_required") || message.contains("profile_missing") {
        return MutationFailureClass::NeutralNotFound;
    }
    if message.contains("state_mismatch") {
        return MutationFailureClass::VersionConflict;
    }
    if message.contains("not_verified")
        || message.contains("active_profile_generation_cannot_be_quarantined")
        || message.contains("time_regression")
    {
        return MutationFailureClass::InvalidState;
    }
    if message.contains("UNIQUE constraint failed") {
        return MutationFailureClass::Conflict;
    }
    if message.contains("CHECK constraint failed")
        || message.contains("FOREIGN KEY constraint failed")
        || message.contains("not_governed")
        || message.contains("identity_immutable")
    {
        return MutationFailureClass::IntegrityFailure;
    }
    MutationFailureClass::DependencyUnavailable
}

fn mutation_failure(request: &Request, error: Error) -> Result<Response> {
    match classify_mutation_failure(&error.to_string()) {
        MutationFailureClass::NeutralNotFound => neutral_not_found(&correlation_hint(request)),
        MutationFailureClass::VersionConflict => problem(
            &correlation_hint(request),
            409,
            "version_conflict",
            "Version Conflict",
        ),
        MutationFailureClass::InvalidState => problem(
            &correlation_hint(request),
            409,
            "invalid_state",
            "Invalid State",
        ),
        MutationFailureClass::Conflict => conflict(request),
        MutationFailureClass::IntegrityFailure => problem(
            &correlation_hint(request),
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        MutationFailureClass::DependencyUnavailable => problem(
            &correlation_hint(request),
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationResponse<'a> {
    generation_id: &'a str,
    metadata_digest: &'a str,
    container_digest: &'a str,
    status: &'static str,
    version: u64,
    verification_reference: Option<&'a str>,
}

impl<'a> From<&'a GenerationProjection> for GenerationResponse<'a> {
    fn from(generation: &'a GenerationProjection) -> Self {
        Self {
            generation_id: generation.generation_id().as_str(),
            metadata_digest: generation.metadata_digest(),
            container_digest: generation.container_digest(),
            status: match generation.status() {
                GenerationStatus::Registered => "REGISTERED",
                GenerationStatus::Verified => "VERIFIED",
                GenerationStatus::Quarantined => "QUARANTINED",
            },
            version: generation.version().value(),
            verification_reference: generation.verification_reference(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MutationReceipt<'a> {
    result_code: &'a str,
    resource_id: &'a str,
    aggregate_version: u64,
}

fn mutation_receipt(
    result_code: &str,
    resource_id: &str,
    aggregate_version: u64,
    status: u16,
) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code,
        resource_id,
        aggregate_version,
    })
    .map(|response| response.with_status(status))
}

struct EnvelopeOwned {
    idempotency_key: IdempotencyKey,
    request_digest: String,
    audit_event_id: AuditEventId,
    outbox_event_id: OutboxEventId,
    now: UnixMillis,
    expires_at: UnixMillis,
    payload_json: String,
}

impl EnvelopeOwned {
    fn from_request(
        request: &Request,
        actor: &ResolvedActor,
        request_digest: String,
    ) -> Result<Self> {
        if !valid_digest(&request_digest) {
            return Err(Error::RustError("request digest is invalid".to_owned()));
        }
        let key = request
            .headers()
            .get(IDEMPOTENCY_HEADER)?
            .ok_or_else(|| Error::RustError("idempotency key missing".to_owned()))?;
        let idempotency_key =
            IdempotencyKey::parse(key).map_err(|error| Error::RustError(error.to_string()))?;
        let audit_event_id = audit_event_id(
            actor.actor().tenant_scope().tenant_id(),
            actor.actor().actor_id(),
            &idempotency_key,
        )?;
        let outbox_event_id = outbox_event_id(
            actor.actor().tenant_scope().tenant_id(),
            actor.actor().actor_id(),
            &idempotency_key,
        )?;
        let now = Date::now().as_millis();
        let expires_at = now
            .checked_add(IDEMPOTENCY_TTL_MS)
            .ok_or_else(|| Error::RustError("idempotency expiry overflow".to_owned()))?;
        Ok(Self {
            idempotency_key,
            request_digest,
            audit_event_id,
            outbox_event_id,
            now: UnixMillis::new(now),
            expires_at: UnixMillis::new(expires_at),
            payload_json: "{}".to_owned(),
        })
    }

    fn identity(&self) -> MutationEnvelope<'_> {
        MutationEnvelope {
            idempotency_key: &self.idempotency_key,
            request_digest: &self.request_digest,
            audit_event_id: &self.audit_event_id,
            outbox_event_id: &self.outbox_event_id,
            payload_json: &self.payload_json,
            now: self.now,
            idempotency_expires_at: self.expires_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegisterGenerationRequest {
    generation_id: String,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VerifyGenerationRequest {
    expected_generation_version: u64,
    verification_reference: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivateGenerationRequest {
    expected_profile_version: u64,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeactivateGenerationRequest {
    expected_profile_version: u64,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuarantineGenerationRequest {
    expected_generation_version: u64,
    request_digest: String,
}

fn valid_object_key(value: &str) -> bool {
    (16..=512).contains(&value.len())
        && !value.starts_with('/')
        && !value.contains("..")
        && !value.contains('\\')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':')
        })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_verification_reference(value: &str) -> bool {
    (8..=256).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':'))
}

fn next_version(version: AggregateVersion) -> Option<u64> {
    version.next().ok().map(AggregateVersion::value)
}

fn invalid_request(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        400,
        "invalid_request",
        "Invalid Request",
    )
}

fn conflict(request: &Request) -> Result<Response> {
    problem(&correlation_hint(request), 409, "conflict", "Conflict")
}

fn internal_failure(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        500,
        "internal_failure",
        "Internal Failure",
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ActivateGenerationRequest, MutationFailureClass, classify_mutation_failure, next_version,
        valid_digest, valid_object_key, valid_verification_reference,
    };
    use profile_platform_primitives::AggregateVersion;

    #[test]
    fn request_metadata_is_canonical_and_versions_never_saturate()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(valid_object_key("profiles/v1/generation.enc"));
        assert!(!valid_object_key("../generation.enc"));
        assert!(!valid_object_key("profiles\\generation.enc"));
        assert!(valid_digest(&"a".repeat(64)));
        assert!(!valid_digest(&"A".repeat(64)));
        assert!(valid_verification_reference("review:generation_01"));
        assert!(!valid_verification_reference("review generation"));
        assert_eq!(next_version(AggregateVersion::INITIAL), Some(2));
        assert_eq!(next_version(AggregateVersion::new(u64::MAX)?), None);
        Ok(())
    }

    #[test]
    fn request_dtos_reject_unknown_fields() {
        let digest = "a".repeat(64);
        let payload = format!(
            r#"{{"expectedProfileVersion":1,"requestDigest":"{digest}","unexpected":true}}"#
        );
        assert!(serde_json::from_str::<ActivateGenerationRequest>(&payload).is_err());
    }

    #[test]
    fn d1_failures_are_classified_without_public_provider_details() {
        assert_eq!(
            classify_mutation_failure("profile_generation_activate_profile_state_mismatch"),
            MutationFailureClass::VersionConflict
        );
        assert_eq!(
            classify_mutation_failure("profile_generation_not_verified"),
            MutationFailureClass::InvalidState
        );
        assert_eq!(
            classify_mutation_failure("profile_generation_activation_not_governed"),
            MutationFailureClass::IntegrityFailure
        );
        assert_eq!(
            classify_mutation_failure("network request failed"),
            MutationFailureClass::DependencyUnavailable
        );
    }
}
