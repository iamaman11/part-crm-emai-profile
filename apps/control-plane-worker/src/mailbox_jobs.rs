use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::mailbox_job_application;
use cloudflare_adapters::cloud_mailbox_provider::CloudMailboxProviderRouter;
use control_plane_contract::RouteClass;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, MailboxBindingId, MailboxJobId};
use serde::{Deserialize, Serialize};
use use_cases::mailbox_jobs::{
    ExecuteCreateMailboxJobCommand, ExecuteRunMailboxJobCommand, MailboxJobDetails,
    MailboxJobMutationOutcome, MailboxJobOperationError, authorize_mailbox_job,
    execute_create_mailbox_job, execute_run_mailbox_job, get_mailbox_job,
    validate_create_mailbox_job_request, validate_mailbox_job_run_version,
};
use worker::{Env, Request, Response, Result};

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
    let role = membership_role(&actor);
    if let Err(error) = authorize_mailbox_job(role) {
        return operation_failure(actor.actor().correlation_id().as_str(), error);
    }

    match route {
        RouteClass::MailboxJobCollectionApi => {
            let Some(binding_id) = binding_id else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            create_job(request, env, actor.actor(), role, binding_id).await
        }
        RouteClass::MailboxJobResourceApi => {
            let (Some(binding_id), Some(job_id)) = (binding_id, job_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            get_job(env, actor.actor(), role, &binding_id, &job_id).await
        }
        RouteClass::MailboxJobRunApi => {
            let (Some(binding_id), Some(job_id)) = (binding_id, job_id) else {
                return neutral_not_found(actor.actor().correlation_id().as_str());
            };
            run_job(request, env, actor.actor(), role, binding_id, job_id).await
        }
        _ => neutral_not_found(actor.actor().correlation_id().as_str()),
    }
}

async fn create_job(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: MailboxBindingId,
) -> Result<Response> {
    let body = match request.json::<CreateMailboxJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if let Err(error) = validate_create_mailbox_job_request(
        body.delay_ms,
        body.max_attempts,
        body.cursor.as_deref(),
    ) {
        return operation_failure(actor.correlation_id().as_str(), error);
    }
    let job_id = match MailboxJobId::parse(body.job_id) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if !valid_digest(&body.request_digest) {
        return invalid_request(actor.correlation_id().as_str());
    }
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = mailbox_job_application(env)?;
    match execute_create_mailbox_job(
        actor,
        role,
        &application,
        ExecuteCreateMailboxJobCommand::new(
            binding_id,
            job_id,
            body.cursor,
            body.delay_ms,
            body.max_attempts,
            evidence,
        ),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 201),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn get_job(
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: &MailboxBindingId,
    job_id: &MailboxJobId,
) -> Result<Response> {
    let application = mailbox_job_application(env)?;
    match get_mailbox_job(actor, role, &application, binding_id, job_id).await {
        Ok(job) => Response::from_json(&MailboxJobResponse::from(&job)),
        Err(MailboxJobOperationError::NotFound) => {
            neutral_not_found(actor.correlation_id().as_str())
        }
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

async fn run_job(
    request: &mut Request,
    env: &Env,
    actor: &ActorContext,
    role: MembershipRole,
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
) -> Result<Response> {
    let body = match request.json::<RunMailboxJobRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let expected_version = match AggregateVersion::new(body.expected_job_version) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    if let Err(error) = validate_mailbox_job_run_version(expected_version) {
        return operation_failure(actor.correlation_id().as_str(), error);
    }
    if !valid_digest(&body.request_digest) {
        return invalid_request(actor.correlation_id().as_str());
    }
    let evidence = match command_evidence::from_request(request, actor, body.request_digest) {
        Ok(value) => value,
        Err(_) => return invalid_request(actor.correlation_id().as_str()),
    };
    let application = mailbox_job_application(env)?;
    let mut provider = CloudMailboxProviderRouter::new(env);
    match execute_run_mailbox_job(
        actor,
        role,
        &application,
        &mut provider,
        ExecuteRunMailboxJobCommand::new(binding_id, job_id, expected_version, evidence),
    )
    .await
    {
        Ok(outcome) => mutation_receipt(&outcome, 200),
        Err(error) => operation_failure(actor.correlation_id().as_str(), error),
    }
}

fn operation_failure(correlation_id: &str, error: MailboxJobOperationError) -> Result<Response> {
    match error {
        MailboxJobOperationError::InvalidRequest => {
            problem(correlation_id, 400, "invalid_request", "Invalid Request")
        }
        MailboxJobOperationError::NotFound => neutral_not_found(correlation_id),
        MailboxJobOperationError::VersionConflict => {
            problem(correlation_id, 409, "version_conflict", "Version Conflict")
        }
        MailboxJobOperationError::InvalidState => {
            problem(correlation_id, 409, "invalid_state", "Invalid State")
        }
        MailboxJobOperationError::Conflict => problem(correlation_id, 409, "conflict", "Conflict"),
        MailboxJobOperationError::IntegrityFailure => problem(
            correlation_id,
            500,
            "integrity_failure",
            "Integrity Failure",
        ),
        MailboxJobOperationError::InternalFailure => {
            problem(correlation_id, 500, "internal_failure", "Internal Failure")
        }
        MailboxJobOperationError::DependencyUnavailable => problem(
            correlation_id,
            503,
            "dependency_unavailable",
            "Dependency Unavailable",
        ),
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
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

impl<'a> From<&'a MailboxJobDetails> for MailboxJobResponse<'a> {
    fn from(job: &'a MailboxJobDetails) -> Self {
        Self {
            job_id: job.job_id().as_str(),
            status: job.status().storage_value(),
            attempt: job.attempt(),
            max_attempts: job.max_attempts(),
            next_run_at_ms: job.next_run_at().value(),
            provider_status: job.provider_status(),
            bounded_item_count: job.bounded_item_count(),
            version: job.version().value(),
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

fn mutation_receipt(outcome: &MailboxJobMutationOutcome, status: u16) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code(),
        resource_id: outcome.resource_id(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
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

#[cfg(test)]
mod tests {
    use super::{CreateMailboxJobRequest, MailboxJobResponse, MutationReceipt, valid_digest};

    #[test]
    fn mailbox_job_transport_preserves_shape_and_privacy()
    -> Result<(), Box<dyn std::error::Error>> {
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
        assert!(!valid_digest(&"a".repeat(63)));

        let receipt = serde_json::to_value(MutationReceipt {
            result_code: "created",
            resource_id: "mailjob_01JTEST",
            aggregate_version: 1,
        })?;
        assert!(receipt.get("resultCode").is_some());
        assert!(receipt.get("resourceId").is_some());
        assert!(receipt.get("aggregateVersion").is_some());

        let response = serde_json::to_value(MailboxJobResponse {
            job_id: "mailjob_01JTEST",
            status: "SCHEDULED",
            attempt: 0,
            max_attempts: 3,
            next_run_at_ms: 0,
            provider_status: None,
            bounded_item_count: 0,
            version: 1,
        })?;
        for key in [
            "jobId",
            "status",
            "attempt",
            "maxAttempts",
            "nextRunAtMs",
            "providerStatus",
            "boundedItemCount",
            "version",
        ] {
            assert!(response.get(key).is_some(), "missing {key}");
        }
        assert!(response.get("messageBody").is_none());
        assert!(response.get("secretHandle").is_none());
        Ok(())
    }
}
