use crate::access_session::{correlation_hint, neutral_not_found, problem, resolve_active_request_actor};
use crate::mutation_failure::{MutationFailureClass, classify_mutation_failure, mutation_failure};
use crate::request_evidence::{audit_event_id, outbox_event_id};
use cloudflare_adapters::d1_idempotency::{D1IdempotencyRepository, IdempotencyDecision};
use cloudflare_adapters::d1_identity_acl::{MutationEnvelope, ResolvedActor, ResolvedMembershipRole};
use cloudflare_adapters::d1_mailboxes::{
    CreateMailboxJobMutation, D1MailboxRepository, RunMailboxJobMutation,
};
use cloudflare_adapters::mailbox_provider::MetadataMailboxProviderAdapter;
use control_plane_contract::{D1_CATALOG_BINDING, RouteClass};
use mailbox_domain::{MailboxJobKind, MailboxJobStatus};
use profile_platform_primitives::{
    ActorId, AggregateVersion, AuditEventId, IdempotencyKey, MailboxBindingId, MailboxJobId,
    OutboxEventId, TenantScope, UnixMillis,
};
use serde::{Deserialize, Serialize};
use worker::{Date, Env, Error, Request, Response, Result};

const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const IDEMPOTENCY_TTL_MS: u64 = 86_400_000;
const MAILBOX_JOB_CREATE_COMMAND: &str = "mailbox.job_create";
const MAILBOX_JOB_RUN_COMMAND: &str = "mailbox.job_run";

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let tenant_id = segments.get(3).copied().unwrap_or_default();
    let job_id = segments
        .get(5)
        .and_then(|value| MailboxJobId::parse((*value).to_owned()).ok());

    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    if actor.role() != ResolvedMembershipRole::TenantOwner {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    }

    match route {
        RouteClass::MailboxJobCollectionApi => create_job(request, env, &actor).await,
        RouteClass::MailboxJobResourceApi => {
            let Some(job_id) = job_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            get_job(env, &actor, &job_id).await
        }
        RouteClass::MailboxJobRunApi => {
            let Some(job_id) = job_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            run_job(request, env, &actor, &job_id).await
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

async fn create_job(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
) -> Result<Response> {
    let body = match request.json::<CreateMailboxJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let job_id = match MailboxJobId::parse(body.job_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let binding_id = match MailboxBindingId::parse(body.binding_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let kind = match parse_job_kind(&body.kind) {
        Some(value) => value,
        None => return invalid_request(request),
    };
    if !valid_digest(&body.request_digest) {
        return invalid_request(request);
    }
    let envelope = match EnvelopeOwned::from_actor(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        request,
        env,
        actor,
        MAILBOX_JOB_CREATE_COMMAND,
        &envelope,
        job_id.as_str(),
        AggregateVersion::INITIAL.value(),
        201,
    )
    .await?
    {
        return Ok(response);
    }
    let mutation = CreateMailboxJobMutation {
        job_id: &job_id,
        binding_id: &binding_id,
        kind,
        envelope: envelope.mutation(),
    };
    let result = D1MailboxRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .create_job(actor.actor(), mutation)
        .await;
    match result {
        Ok(_) => mutation_receipt(
            "created",
            job_id.as_str(),
            AggregateVersion::INITIAL.value(),
            201,
        ),
        Err(error) => {
            mutation_failure_or_replay(
                request,
                env,
                actor,
                MAILBOX_JOB_CREATE_COMMAND,
                &envelope,
                job_id.as_str(),
                AggregateVersion::INITIAL.value(),
                201,
                error,
            )
            .await
        }
    }
}

async fn get_job(env: &Env, actor: &ResolvedActor, job_id: &MailboxJobId) -> Result<Response> {
    let repository = D1MailboxRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let Some(job) = repository
        .find_job(actor.actor().tenant_scope(), job_id)
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    Response::from_json(&MailboxJobResponse::from(&job))
}

async fn run_job(
    request: &mut Request,
    env: &Env,
    actor: &ResolvedActor,
    job_id: &MailboxJobId,
) -> Result<Response> {
    let body = match request.json::<RunMailboxJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let expected_job_version = match AggregateVersion::new(body.expected_job_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    let response_version = match next_aggregate_version(expected_job_version) {
        Some(value) => value,
        None => return internal_failure(request),
    };
    if !valid_digest(&body.request_digest) {
        return invalid_request(request);
    }
    let envelope = match EnvelopeOwned::from_actor(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(request),
    };
    if let Some(response) = replay_response(
        request,
        env,
        actor,
        MAILBOX_JOB_RUN_COMMAND,
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
    let Some(job) = repository
        .find_job(actor.actor().tenant_scope(), job_id)
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };
    if job.version() != expected_job_version {
        return version_conflict(request);
    }
    let Some(binding) = repository
        .find_binding(actor.actor().tenant_scope(), job.binding_id())
        .await?
    else {
        return neutral_not_found(actor.actor().correlation_id().as_str());
    };

    let provider = MetadataMailboxProviderAdapter;
    let decision = match provider.execute(&binding, &job, envelope.now) {
        Ok(value) => value,
        Err(_) => return dependency_unavailable(request),
    };
    let mutation = RunMailboxJobMutation {
        job_id,
        expected_job_version,
        next_status: decision.next_status(),
        next_run_at: decision.next_run_at(),
        provider_cursor: decision.provider_cursor(),
        provider_status: decision.provider_status(),
        envelope: envelope.mutation(),
    };
    let result = repository.run_job(actor.actor(), mutation).await;
    match result {
        Ok(_) => mutation_receipt("run", job_id.as_str(), response_version, 200),
        Err(error) => {
            mutation_failure_or_replay(
                request,
                env,
                actor,
                MAILBOX_JOB_RUN_COMMAND,
                &envelope,
                job_id.as_str(),
                response_version,
                200,
                error,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn replay_response(
    request: &Request,
    env: &Env,
    actor: &ResolvedActor,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    success_status: u16,
) -> Result<Option<Response>> {
    let decision = match D1IdempotencyRepository::new(env.d1(D1_CATALOG_BINDING)?)
        .decide(
            actor.actor().tenant_scope(),
            actor.actor().actor_id(),
            &envelope.idempotency_key,
            command_name,
            &envelope.request_digest,
            envelope.now,
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return mutation_failure(request, error).map(Some),
    };
    match decision {
        IdempotencyDecision::Miss => Ok(None),
        IdempotencyDecision::Replay(receipt) => mutation_receipt(
            receipt.result_code(),
            receipt.result_reference().unwrap_or(resource_id),
            aggregate_version,
            success_status,
        )
        .map(Some),
        IdempotencyDecision::Conflict => conflict(request).map(Some),
    }
}

#[allow(clippy::too_many_arguments)]
async fn mutation_failure_or_replay(
    request: &Request,
    env: &Env,
    actor: &ResolvedActor,
    command_name: &str,
    envelope: &EnvelopeOwned,
    resource_id: &str,
    aggregate_version: u64,
    replay_status: u16,
    error: Error,
) -> Result<Response> {
    if classify_mutation_failure(&error.to_string()) == MutationFailureClass::Conflict {
        if let Some(response) = replay_response(
            request,
            env,
            actor,
            command_name,
            envelope,
            resource_id,
            aggregate_version,
            replay_status,
        )
        .await?
        {
            return Ok(response);
        }
    }
    mutation_failure(request, error)
}

fn next_aggregate_version(version: AggregateVersion) -> Option<u64> {
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

fn dependency_unavailable(request: &Request) -> Result<Response> {
    problem(
        &correlation_hint(request),
        503,
        "dependency_unavailable",
        "Dependency Unavailable",
    )
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MailboxJobResponse<'a> {
    job_id: &'a str,
    binding_id: &'a str,
    kind: &'static str,
    status: &'static str,
    attempt_count: u32,
    next_run_at_ms: Option<u64>,
    provider_cursor: Option<&'a str>,
    last_provider_status: Option<&'a str>,
    version: u64,
}

impl<'a> From<&'a mailbox_domain::MailboxJob> for MailboxJobResponse<'a> {
    fn from(job: &'a mailbox_domain::MailboxJob) -> Self {
        Self {
            job_id: job.job_id().as_str(),
            binding_id: job.binding_id().as_str(),
            kind: job_kind_value(job.kind()),
            status: job_status_value(job.status()),
            attempt_count: job.attempt_count(),
            next_run_at_ms: job.next_run_at().map(UnixMillis::value),
            provider_cursor: job.provider_cursor(),
            last_provider_status: job.last_provider_status(),
            version: job.version().value(),
        }
    }
}

fn parse_job_kind(value: &str) -> Option<MailboxJobKind> {
    match value {
        "CHECK" => Some(MailboxJobKind::Check),
        "REFRESH" => Some(MailboxJobKind::Refresh),
        _ => None,
    }
}

const fn job_kind_value(kind: MailboxJobKind) -> &'static str {
    match kind {
        MailboxJobKind::Check => "CHECK",
        MailboxJobKind::Refresh => "REFRESH",
    }
}

const fn job_status_value(status: MailboxJobStatus) -> &'static str {
    match status {
        MailboxJobStatus::Pending => "PENDING",
        MailboxJobStatus::Running => "RUNNING",
        MailboxJobStatus::Succeeded => "SUCCEEDED",
        MailboxJobStatus::Retryable => "RETRYABLE",
        MailboxJobStatus::Failed => "FAILED",
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
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
    fn from_actor(request: &Request, actor: &ResolvedActor, request_digest: String) -> Result<Self> {
        if !valid_digest(&request_digest) {
            return Err(Error::RustError(
                "request digest must be 64 lowercase hex characters".to_owned(),
            ));
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

    fn mutation(&self) -> MutationEnvelope<'_> {
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
    binding_id: String,
    kind: String,
    request_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunMailboxJobRequest {
    expected_job_version: u64,
    request_digest: String,
}

#[cfg(test)]
mod tests {
    use super::{
        CreateMailboxJobRequest, MAILBOX_JOB_CREATE_COMMAND, MAILBOX_JOB_RUN_COMMAND,
        RunMailboxJobRequest, next_aggregate_version, valid_digest,
    };
    use profile_platform_primitives::AggregateVersion;

    #[test]
    fn mailbox_job_payload_rejects_sensitive_fields() {
        let digest = "a".repeat(64);
        let create = format!(
            r#"{{"jobId":"mailjob_01JTEST","bindingId":"mailbox_01JTEST","kind":"CHECK","requestDigest":"{digest}","password":"forbidden"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxJobRequest>(&create).is_err());
        let run = format!(
            r#"{{"expectedJobVersion":1,"requestDigest":"{digest}","messageBody":"forbidden"}}"#
        );
        assert!(serde_json::from_str::<RunMailboxJobRequest>(&run).is_err());
    }

    #[test]
    fn mailbox_request_digest_is_strict_lowercase_sha256_shape() {
        assert!(valid_digest(&"a".repeat(64)));
        assert!(!valid_digest(&"A".repeat(64)));
        assert!(!valid_digest(&"g".repeat(64)));
        assert!(!valid_digest(&"a".repeat(63)));
    }

    #[test]
    fn mailbox_job_command_domains_remain_distinct() {
        assert_ne!(MAILBOX_JOB_CREATE_COMMAND, MAILBOX_JOB_RUN_COMMAND);
    }

    #[test]
    fn mailbox_job_response_versions_never_saturate() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(next_aggregate_version(AggregateVersion::INITIAL), Some(2));
        assert_eq!(
            next_aggregate_version(AggregateVersion::new(u64::MAX)?),
            None
        );
        Ok(())
    }
}
