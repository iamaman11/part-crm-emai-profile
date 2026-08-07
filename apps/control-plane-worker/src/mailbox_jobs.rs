use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::mutation_failure::mutation_failure;
use crate::request_evidence::{audit_event_id, outbox_event_id};
use cloudflare_adapters::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use cloudflare_adapters::d1_identity_acl::{
    MutationEnvelope, ResolvedActor, ResolvedMembershipRole,
};
use cloudflare_adapters::d1_mailboxes::{
    CreateMailboxJobMutation, D1MailboxRepository, MailboxJobProjection, RunMailboxJobMutation,
};
use cloudflare_adapters::mailbox_provider::{MetadataMailboxProviderAdapter, decide_mailbox_run};
use control_plane_contract::{D1_CATALOG_BINDING, RouteClass};
use profile_platform_primitives::{
    AggregateVersion, AuditEventId, IdempotencyKey, MailboxBindingId, MailboxJobId, OutboxEventId,
    UnixMillis,
};
use serde::{Deserialize, Serialize};
use worker::{Date, Env, Error, Request, Response, Result};

const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const IDEMPOTENCY_TTL_MS: u64 = 86_400_000;
const MAX_JOB_DELAY_MS: u64 = 604_800_000;

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let binding_id = segments
        .get(5)
        .and_then(|value| MailboxBindingId::parse((*value).to_owned()).ok());
    let job_id = segments
        .get(7)
        .and_then(|value| MailboxJobId::parse((*value).to_owned()).ok());
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    if actor.role() != ResolvedMembershipRole::TenantOwner {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    }

    match route {
        RouteClass::MailboxJobCollectionApi => {
            let Some(binding_id) = binding_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            create_job(request, env, &actor, &binding_id).await
        }
        RouteClass::MailboxJobResourceApi => {
            let (Some(binding_id), Some(job_id)) = (binding_id, job_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            get_job(env, &actor, &binding_id, &job_id).await
        }
        RouteClass::MailboxJobRunApi => {
            let (Some(binding_id), Some(job_id)) = (binding_id, job_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            run_job(request, env, &actor, &binding_id, &job_id).await
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

async fn get_job(
    env: &Env,
    actor: &ResolvedActor,
    binding_id: &MailboxBindingId,
    job_id: &MailboxJobId,
) -> Result<Response> {
    let repository = D1MailboxRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let Some(job) = repository
        .find_job(actor.actor().tenant_scope(), binding_id, job_id)
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    Response::from_json(&MailboxJobResponse::from(&job))
}

async fn create_job(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    binding_id: &MailboxBindingId,
) -> Result<Response> {
    let body = match request.json::<CreateMailboxJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if body.delay_ms > MAX_JOB_DELAY_MS || body.max_attempts == 0 || body.max_attempts > 10 {
        return invalid_request(request);
    }
    if body
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.len() > 512)
    {
        return invalid_request(request);
    }
    let job_id = match MailboxJobId::parse(body.job_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let envelope = match EnvelopeOwned::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let scheduled_at = match envelope.now.value().checked_add(body.delay_ms) {
        Some(value) => UnixMillis::new(value),
        None => return internal_failure(request),
    };
    if let Some(response) = replay_response(
        env,
        actor,
        "mailbox.job_create",
        &envelope,
        job_id.as_str(),
        1,
        201,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = CreateMailboxJobMutation {
        binding_id,
        job_id: &job_id,
        cursor: body.cursor.as_deref(),
        scheduled_at,
        max_attempts: body.max_attempts,
        envelope: envelope.identity(),
    };
    match D1MailboxRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .create_job(actor.actor(), mutation)
        .await
    {
        Ok(_) => mutation_receipt("created", job_id.as_str(), 1, 201),
        Err(error) => {
            mutation_failure_or_replay(
                request,
                env,
                actor,
                error,
                "mailbox.job_create",
                &envelope,
                job_id.as_str(),
                1,
                201,
            )
            .await
        }
    }
}

async fn run_job(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    binding_id: &MailboxBindingId,
    job_id: &MailboxJobId,
) -> Result<Response> {
    let body = match request.json::<RunMailboxJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_version = match AggregateVersion::new(body.expected_job_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match expected_version.next().and_then(AggregateVersion::next) {
        Ok(value) => value.value(),
        Err(_) => return internal_failure(request),
    };
    let envelope = match EnvelopeOwned::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        env,
        actor,
        "mailbox.job_run",
        &envelope,
        job_id.as_str(),
        response_version,
        200,
    )
    .await?
    {
        return Ok(response);
    }
    let repository = D1MailboxRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let Some(binding) = repository
        .find_binding(actor.actor().tenant_scope(), binding_id)
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    let Some(projection) = repository
        .find_job(actor.actor().tenant_scope(), binding_id, job_id)
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    if projection.job().version() != expected_version {
        return version_conflict(request);
    }
    let next_attempt = match projection.job().attempt().checked_add(1) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    let next_cursor = format!("meta_{}_{}", job_id.as_str(), next_attempt);
    let mut provider = match MetadataMailboxProviderAdapter::new(
        binding.provider(),
        "SYNTHETIC_OK",
        0,
        Some(next_cursor),
    ) {
        Ok(value) => value,
        Err(_) => return internal_failure(request),
    };
    let decision = match decide_mailbox_run(&binding, projection.job(), envelope.now, &mut provider)
    {
        Ok(value) => value,
        Err(error) => {
            return if error.to_string().contains("invalid job state") {
                problem(
                    &correlation_hint(request),
                    409,
                    "invalid_state",
                    "Invalid State",
                )
            } else {
                problem(
                    &correlation_hint(request),
                    503,
                    "dependency_unavailable",
                    "Dependency Unavailable",
                )
            };
        }
    };
    let result_code = match decision.status().storage_value() {
        "SUCCEEDED" => "succeeded",
        "RETRY_PENDING" => "retry_pending",
        "FAILED" => "failed",
        _ => return internal_failure(request),
    };
    let mutation = RunMailboxJobMutation {
        binding_id,
        job_id,
        expected_job_version: expected_version,
        decision: &decision,
        envelope: envelope.identity(),
    };
    match repository.run_job(actor.actor(), mutation).await {
        Ok(_) => mutation_receipt(result_code, job_id.as_str(), response_version, 200),
        Err(error) => {
            mutation_failure_or_replay(
                request,
                env,
                actor,
                error,
                "mailbox.job_run",
                &envelope,
                job_id.as_str(),
                response_version,
                200,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn mutation_failure_or_replay(
    request: &Request,
    env: &Env,
    actor: &ResolvedActor,
    error: Error,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    success_status: u16,
) -> Result<Response> {
    if error.to_string().contains("UNIQUE constraint failed") {
        if let Some(response) = replay_response(
            env,
            actor,
            command_name,
            envelope,
            resource_id,
            aggregate_version,
            success_status,
        )
        .await?
        {
            return Ok(response);
        }
    }
    mutation_failure(request, error)
}

#[allow(clippy::too_many_arguments)]
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxJobResponse<'a> {
    job_id: &'a str,
    status: &'static str,
    attempt: u32,
    max_attempts: u32,
    next_run_at_ms: u64,
    provider_status: Option<&'a str>,
    bounded_item_count: u32,
    version: u64,
}

impl<'a> From<&'a MailboxJobProjection> for MailboxJobResponse<'a> {
    fn from(projection: &'a MailboxJobProjection) -> Self {
        Self {
            job_id: projection.job().job_id().as_str(),
            status: projection.job().status().storage_value(),
            attempt: projection.job().attempt(),
            max_attempts: projection.job().max_attempts(),
            next_run_at_ms: projection.job().next_run_at().value(),
            provider_status: projection.provider_status(),
            bounded_item_count: projection.bounded_item_count(),
            version: projection.job().version().value(),
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
struct CreateMailboxJobRequest {
    job_id: String,
    cursor: Option<String>,
    delay_ms: u64,
    max_attempts: u32,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunMailboxJobRequest {
    expected_job_version: u64,
    request_digest: String,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn invalid_request(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        400,
        "invalid_request",
        "Invalid Request",
    )
}

fn version_conflict(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        409,
        "version_conflict",
        "Version Conflict",
    )
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
    use super::{CreateMailboxJobRequest, valid_digest};

    #[test]
    fn mailbox_job_request_shape_and_digest_validation_remain_strict() {
        let digest = "a".repeat(64);
        let valid = format!(
            r#"{{"jobId":"mailjob_01JTEST","cursor":null,"delayMs":0,"maxAttempts":3,"requestDigest":"{digest}"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxJobRequest>(&valid).is_ok());
        let unknown = format!(
            r#"{{"jobId":"mailjob_01JTEST","cursor":null,"delayMs":0,"maxAttempts":3,"requestDigest":"{digest}","messageBody":"forbidden"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxJobRequest>(&unknown).is_err());
        assert!(valid_digest(&digest));
        assert!(!valid_digest(&"A".repeat(64)));
    }
}
