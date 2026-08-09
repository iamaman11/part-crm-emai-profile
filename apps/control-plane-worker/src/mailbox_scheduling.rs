use crate::composition::mailbox_job_application;
use crate::mailbox_queue_evidence::actor_and_evidence;
use cloudflare_adapters::cloud_mailbox_provider::CloudMailboxProviderRouter;
use cloudflare_adapters::d1_mailbox_scheduling::D1MailboxSchedulingRepository;
use cloudflare_adapters::mailbox_job_queue::{MailboxJobQueueMessage, QueueMailboxJobPublisher};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::UnixMillis;
use use_cases::scheduled::{
    ScheduledMailboxProcessingOutcome, dispatch_due_mailbox_jobs, process_scheduled_mailbox_job,
};
use worker::{Date, Env, Error, MessageExt, QueueRetryOptionsBuilder, Result};

pub const MAILBOX_JOBS_QUEUE_BINDING: &str = "MAILBOX_JOBS";
const DISPATCH_BATCH_LIMIT: u32 = 50;
const TRANSPORT_FAILURE_RETRY_SECONDS: u32 = 30;
const MAX_QUEUE_DELAY_SECONDS: u64 = 86_400;

pub async fn dispatch_pending(env: &Env) -> Result<()> {
    let now = UnixMillis::new(Date::now().as_millis());
    let repository =
        D1MailboxSchedulingRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);
    let publisher = QueueMailboxJobPublisher::new(env.queue(MAILBOX_JOBS_QUEUE_BINDING)?);
    dispatch_due_mailbox_jobs(&repository, &publisher, now, DISPATCH_BATCH_LIMIT)
        .await
        .map(|_| ())
        .map_err(operation_error)
}

pub async fn consume_one<T>(
    message: &worker::Message<T>,
    queued: MailboxJobQueueMessage,
    env: &Env,
) -> Result<()> {
    let dispatch = match queued.into_dispatch() {
        Ok(dispatch) => dispatch,
        Err(_) => {
            retry_after_seconds(message, TRANSPORT_FAILURE_RETRY_SECONDS);
            return Ok(());
        }
    };
    let now = UnixMillis::new(Date::now().as_millis());
    let (actor, evidence) = match actor_and_evidence(&dispatch, now) {
        Ok(value) => value,
        Err(_) => {
            retry_after_seconds(message, TRANSPORT_FAILURE_RETRY_SECONDS);
            return Ok(());
        }
    };
    let application = mailbox_job_application(env)?;
    let scheduling =
        D1MailboxSchedulingRepository::new(env.d1(control_plane_contract::D1_CATALOG_BINDING)?);
    let mut provider = CloudMailboxProviderRouter::new(env);
    match process_scheduled_mailbox_job(
        &actor,
        MembershipRole::TenantOwner,
        &application,
        &scheduling,
        &mut provider,
        &dispatch,
        evidence,
        now,
    )
    .await
    {
        Ok(ScheduledMailboxProcessingOutcome::Acknowledged) => message.ack(),
        Ok(ScheduledMailboxProcessingOutcome::RetryAt(retry_at)) => {
            retry_after_seconds(message, queue_delay_seconds(now, retry_at)?);
        }
        Err(_) => retry_after_seconds(message, TRANSPORT_FAILURE_RETRY_SECONDS),
    }
    Ok(())
}

fn queue_delay_seconds(now: UnixMillis, retry_at: UnixMillis) -> Result<u32> {
    if retry_at.value() <= now.value() {
        return Ok(1);
    }
    let delay_ms = retry_at
        .value()
        .checked_sub(now.value())
        .ok_or_else(|| Error::RustError("mailbox retry delay underflow".into()))?;
    let rounded_ms = delay_ms
        .checked_add(999)
        .ok_or_else(|| Error::RustError("mailbox retry delay overflow".into()))?;
    let delay_seconds = rounded_ms / 1_000;
    let bounded_seconds = delay_seconds.clamp(1, MAX_QUEUE_DELAY_SECONDS);
    u32::try_from(bounded_seconds)
        .map_err(|_| Error::RustError("mailbox retry delay conversion failed".into()))
}

fn retry_after_seconds<T>(message: &worker::Message<T>, delay_seconds: u32) {
    let options = QueueRetryOptionsBuilder::new()
        .with_delay_seconds(delay_seconds.max(1))
        .build();
    message.retry_with_options(&options);
}

fn operation_error(error: use_cases::mailbox_jobs::MailboxJobOperationError) -> Error {
    Error::RustError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{MAX_QUEUE_DELAY_SECONDS, queue_delay_seconds};
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn mailbox_queue_retry_delay_is_nonzero_and_bounded() -> Result<(), Box<dyn std::error::Error>>
    {
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(1_000), UnixMillis::new(1_000))?,
            1
        );
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(1_000), UnixMillis::new(2_001))?,
            2
        );
        assert_eq!(
            queue_delay_seconds(UnixMillis::new(0), UnixMillis::new(90_000_000))?,
            u32::try_from(MAX_QUEUE_DELAY_SECONDS)?
        );
        Ok(())
    }
}
