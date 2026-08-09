use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_jobs::{MailboxJobApplicationPort, MailboxJobPortErrorClass};
use application_ports::mailbox_scheduling::{
    MailboxDispatchPublisherPort, MailboxExecutionClaimOutcome, MailboxExecutionClaimWrite,
    MailboxExecutionCompletionOutcome, MailboxJobDispatch, MailboxSchedulingRepositoryPort,
};
use application_ports::mailboxes::MailboxProviderPort;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, UnixMillis};

use crate::mailbox_jobs::{
    ExecuteRunMailboxJobCommand, MailboxJobOperationError, authorize_mailbox_job,
    execute_run_mailbox_job,
};

const MAX_DISPATCH_BATCH: u32 = 100;
const EXECUTION_LEASE_MS: u64 = 300_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduledMailboxProcessingOutcome {
    Acknowledged,
    RetryAt(UnixMillis),
}

pub async fn dispatch_due_mailbox_jobs<R, P>(
    repository: &R,
    publisher: &P,
    now: UnixMillis,
    limit: u32,
) -> Result<u32, MailboxJobOperationError>
where
    R: MailboxSchedulingRepositoryPort,
    P: MailboxDispatchPublisherPort,
{
    if limit == 0 || limit > MAX_DISPATCH_BATCH {
        return Err(MailboxJobOperationError::InvalidRequest);
    }
    let dispatches = repository
        .load_due_dispatches(now, limit)
        .await
        .map_err(map_scheduling_port_error)?;
    let mut published = 0_u32;
    for dispatch in dispatches {
        publisher
            .publish(&dispatch)
            .await
            .map_err(map_scheduling_port_error)?;
        repository
            .mark_dispatched(&dispatch, now)
            .await
            .map_err(map_scheduling_port_error)?;
        published = published
            .checked_add(1)
            .ok_or(MailboxJobOperationError::InternalFailure)?;
    }
    Ok(published)
}

pub async fn process_scheduled_mailbox_job<A, S, P>(
    actor: &ActorContext,
    role: MembershipRole,
    application: &A,
    scheduling: &S,
    provider: &mut P,
    dispatch: &MailboxJobDispatch,
    evidence: CommandExecutionEvidence,
    now: UnixMillis,
) -> Result<ScheduledMailboxProcessingOutcome, MailboxJobOperationError>
where
    A: MailboxJobApplicationPort,
    S: MailboxSchedulingRepositoryPort,
    P: MailboxProviderPort,
{
    authorize_mailbox_job(role)?;
    if actor.tenant_scope().tenant_id() != dispatch.tenant_id()
        || actor.actor_id() != dispatch.actor_id()
        || now < dispatch.due_at()
    {
        return Err(MailboxJobOperationError::InvalidRequest);
    }

    let lease_expires_at = execution_lease_expires_at(now)?;
    let claim = scheduling
        .acquire_execution(
            dispatch,
            MailboxExecutionClaimWrite::new(now, lease_expires_at),
        )
        .await
        .map_err(map_scheduling_port_error)?;
    let lease = match claim {
        MailboxExecutionClaimOutcome::Acquired(lease) => lease,
        MailboxExecutionClaimOutcome::InFlight { retry_at } => {
            return Ok(ScheduledMailboxProcessingOutcome::RetryAt(retry_at));
        }
        MailboxExecutionClaimOutcome::Completed | MailboxExecutionClaimOutcome::Stale => {
            return Ok(ScheduledMailboxProcessingOutcome::Acknowledged);
        }
    };

    let run = execute_run_mailbox_job(
        actor,
        role,
        application,
        provider,
        ExecuteRunMailboxJobCommand::new(
            dispatch.binding_id().clone(),
            dispatch.job_id().clone(),
            dispatch.expected_version(),
            evidence,
        ),
    )
    .await;

    match run {
        Ok(_) => match scheduling
            .complete_execution(dispatch, lease.fence(), now)
            .await
            .map_err(map_scheduling_port_error)?
        {
            MailboxExecutionCompletionOutcome::Applied
            | MailboxExecutionCompletionOutcome::Stale => {
                Ok(ScheduledMailboxProcessingOutcome::Acknowledged)
            }
        },
        Err(
            MailboxJobOperationError::NotFound
            | MailboxJobOperationError::VersionConflict
            | MailboxJobOperationError::InvalidState,
        ) => Ok(ScheduledMailboxProcessingOutcome::Acknowledged),
        Err(error) => Err(error),
    }
}

fn execution_lease_expires_at(now: UnixMillis) -> Result<UnixMillis, MailboxJobOperationError> {
    now.value()
        .checked_add(EXECUTION_LEASE_MS)
        .map(UnixMillis::new)
        .ok_or(MailboxJobOperationError::InternalFailure)
}

fn map_scheduling_port_error(
    error: application_ports::mailbox_jobs::MailboxJobPortError,
) -> MailboxJobOperationError {
    match error.class() {
        MailboxJobPortErrorClass::NotFound => MailboxJobOperationError::NotFound,
        MailboxJobPortErrorClass::VersionConflict => MailboxJobOperationError::VersionConflict,
        MailboxJobPortErrorClass::InvalidState => MailboxJobOperationError::InvalidState,
        MailboxJobPortErrorClass::Conflict => MailboxJobOperationError::Conflict,
        MailboxJobPortErrorClass::IntegrityFailure => MailboxJobOperationError::IntegrityFailure,
        MailboxJobPortErrorClass::InternalFailure => MailboxJobOperationError::InternalFailure,
        MailboxJobPortErrorClass::DependencyUnavailable => {
            MailboxJobOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EXECUTION_LEASE_MS, execution_lease_expires_at};
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn execution_lease_is_bounded_and_checked() -> Result<(), Box<dyn std::error::Error>> {
        let now = UnixMillis::new(1_000);
        assert_eq!(
            execution_lease_expires_at(now)?.value(),
            now.value() + EXECUTION_LEASE_MS
        );
        assert!(execution_lease_expires_at(UnixMillis::new(u64::MAX)).is_err());
        Ok(())
    }
}
