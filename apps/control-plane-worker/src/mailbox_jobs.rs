use crate::access_session::{
    correlation_hint, membership_role, neutral_not_found, problem, resolve_active_request_actor,
};
use crate::command_evidence;
use crate::composition::{mailbox_job_application, microsoft_graph_mailbox_authorization};
use application_ports::mailbox_jobs::MailboxJobStatus;
use cloudflare_adapters::cloud_mailbox_provider::CloudMailboxProviderRouter;
use control_plane_contract::{
    RouteClass,
    mailbox_api::{
        CreateMailboxJobRequestDto, MailboxJobProjectionDto, MailboxJobStatusDto,
        RunMailboxJobRequestDto,
    },
    public_api::MutationReceipt,
};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, MailboxBindingId, MailboxJobId};
use use_cases_mailboxes::mailbox_jobs::{
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
    let body = match request.json::<CreateMailboxJobRequestDto>().await {
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
        Ok(job) => Response::from_json(&mailbox_job_projection(&job)),
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
    let body = match request.json::<RunMailboxJobRequestDto>().await {
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
    let mut provider = CloudMailboxProviderRouter::new(env).with_microsoft_graph_authorization(
        microsoft_graph_mailbox_authorization(env)?,
        actor,
    );
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

fn mailbox_job_projection(job: &MailboxJobDetails) -> MailboxJobProjectionDto {
    MailboxJobProjectionDto {
        job_id: job.job_id().as_str().to_owned(),
        status: mailbox_job_status(job.status()),
        attempt: job.attempt(),
        max_attempts: job.max_attempts(),
        next_run_at_ms: job.next_run_at().value(),
        provider_status: job.provider_status().map(str::to_owned),
        bounded_item_count: job.bounded_item_count(),
        version: job.version().value(),
    }
}

const fn mailbox_job_status(status: MailboxJobStatus) -> MailboxJobStatusDto {
    match status {
        MailboxJobStatus::Scheduled => MailboxJobStatusDto::Scheduled,
        MailboxJobStatus::Queued => MailboxJobStatusDto::Queued,
        MailboxJobStatus::Running => MailboxJobStatusDto::Running,
        MailboxJobStatus::RetryPending => MailboxJobStatusDto::RetryPending,
        MailboxJobStatus::AuthRequired => MailboxJobStatusDto::AuthRequired,
        MailboxJobStatus::Suspended => MailboxJobStatusDto::Suspended,
        MailboxJobStatus::Succeeded => MailboxJobStatusDto::Succeeded,
        MailboxJobStatus::Failed => MailboxJobStatusDto::Failed,
    }
}

fn mutation_receipt(outcome: &MailboxJobMutationOutcome, status: u16) -> Result<Response> {
    Response::from_json(&MutationReceipt {
        result_code: outcome.result_code().to_owned(),
        resource_id: outcome.resource_id().to_owned(),
        aggregate_version: outcome.aggregate_version().value(),
    })
    .map(|response| response.with_status(status))
}

#[cfg(test)]
mod tests {
    use super::{mailbox_job_status, valid_digest};
    use application_ports::mailbox_jobs::MailboxJobStatus;
    use control_plane_contract::{
        mailbox_api::{CreateMailboxJobRequestDto, MailboxJobProjectionDto},
        public_api::MutationReceipt,
    };

    #[test]
    fn mailbox_job_transport_uses_canonical_shape_and_keeps_domain_validation_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let digest = "a".repeat(64);
        let valid = format!(
            r#"{{"jobId":"mailjob_01JTEST","cursor":null,"delayMs":0,"maxAttempts":3,"requestDigest":"{digest}"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxJobRequestDto>(&valid).is_ok());
        let unknown = format!(
            r#"{{"jobId":"mailjob_01JTEST","cursor":null,"delayMs":0,"maxAttempts":3,"requestDigest":"{digest}","messageBody":"forbidden"}}"#
        );
        assert!(serde_json::from_str::<CreateMailboxJobRequestDto>(&unknown).is_err());
        let domain_invalid_but_transport_valid = format!(
            r#"{{"jobId":"mailjob_01JTEST","cursor":null,"delayMs":604800001,"maxAttempts":3,"requestDigest":"{digest}"}}"#
        );
        assert!(
            serde_json::from_str::<CreateMailboxJobRequestDto>(&domain_invalid_but_transport_valid)
                .is_ok(),
            "job bounds must remain enforced by existing Worker/use-case validation sequencing"
        );
        assert!(valid_digest(&digest));
        assert!(!valid_digest(&"A".repeat(64)));
        assert!(!valid_digest(&"a".repeat(63)));

        let receipt = serde_json::to_value(MutationReceipt {
            result_code: "created".to_owned(),
            resource_id: "mailjob_01JTEST".to_owned(),
            aggregate_version: 1,
        })?;
        assert!(receipt.get("resultCode").is_some());
        assert!(receipt.get("resourceId").is_some());
        assert!(receipt.get("aggregateVersion").is_some());

        let response = serde_json::to_value(MailboxJobProjectionDto {
            job_id: "mailjob_01JTEST".to_owned(),
            status: mailbox_job_status(MailboxJobStatus::Scheduled),
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
        assert_eq!(response["status"], "SCHEDULED");
        assert!(response.get("messageBody").is_none());
        assert!(response.get("secretHandle").is_none());
        Ok(())
    }
}
