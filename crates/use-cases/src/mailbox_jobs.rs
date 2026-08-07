use application_ports::CommandExecutionEvidence;
use application_ports::mailbox_jobs::{
    MailboxJobApplicationPort, MailboxJobCreateWrite, MailboxJobPortError,
    MailboxJobPortErrorClass, MailboxJobPreparedRun, MailboxJobReadModel, MailboxJobRunWrite,
    MailboxJobStatus,
};
use application_ports::mailboxes::{MailboxReplayDecision, MailboxReplayReceipt};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, MailboxJobId, UnixMillis,
};

const MAILBOX_JOB_CREATE_COMMAND: &str = "mailbox.job_create";
const MAILBOX_JOB_RUN_COMMAND: &str = "mailbox.job_run";
const MAILBOX_JOB_EVENT_PAYLOAD: &str = "{}";
const MAX_JOB_DELAY_MS: u64 = 604_800_000;
const MAX_CURSOR_LENGTH: usize = 512;
const MAX_JOB_ATTEMPTS: u32 = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteCreateMailboxJobCommand {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    cursor: Option<String>,
    delay_ms: u64,
    max_attempts: u32,
    evidence: CommandExecutionEvidence,
}

impl ExecuteCreateMailboxJobCommand {
    #[must_use]
    pub fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        cursor: Option<String>,
        delay_ms: u64,
        max_attempts: u32,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            cursor,
            delay_ms,
            max_attempts,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteRunMailboxJobCommand {
    binding_id: MailboxBindingId,
    job_id: MailboxJobId,
    expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
}

impl ExecuteRunMailboxJobCommand {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        job_id: MailboxJobId,
        expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            job_id,
            expected_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobMutationOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl MailboxJobMutationOutcome {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxJobDetails {
    job_id: MailboxJobId,
    status: MailboxJobStatus,
    attempt: u32,
    max_attempts: u32,
    next_run_at: UnixMillis,
    provider_status: Option<String>,
    bounded_item_count: u32,
    version: AggregateVersion,
}

impl MailboxJobDetails {
    #[must_use]
    pub const fn job_id(&self) -> &MailboxJobId {
        &self.job_id
    }

    #[must_use]
    pub const fn status(&self) -> MailboxJobStatus {
        self.status
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    #[must_use]
    pub const fn next_run_at(&self) -> UnixMillis {
        self.next_run_at
    }

    #[must_use]
    pub fn provider_status(&self) -> Option<&str> {
        self.provider_status.as_deref()
    }

    #[must_use]
    pub const fn bounded_item_count(&self) -> u32 {
        self.bounded_item_count
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

impl From<MailboxJobReadModel> for MailboxJobDetails {
    fn from(value: MailboxJobReadModel) -> Self {
        Self {
            job_id: value.job().job_id().clone(),
            status: value.job().status(),
            attempt: value.job().attempt(),
            max_attempts: value.job().max_attempts(),
            next_run_at: value.job().next_run_at(),
            provider_status: value.provider_status().map(str::to_owned),
            bounded_item_count: value.bounded_item_count(),
            version: value.job().version(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxJobOperationError {
    InvalidRequest,
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for MailboxJobOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "mailbox job request is invalid",
            Self::NotFound => "mailbox job not found",
            Self::VersionConflict => "mailbox job version conflict",
            Self::InvalidState => "mailbox job invalid state",
            Self::Conflict => "mailbox job command conflict",
            Self::IntegrityFailure => "mailbox job integrity failure",
            Self::InternalFailure => "mailbox job internal failure",
            Self::DependencyUnavailable => "mailbox job dependency unavailable",
        })
    }
}

impl std::error::Error for MailboxJobOperationError {}

pub fn authorize_mailbox_job(role: MembershipRole) -> Result<(), MailboxJobOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(MailboxJobOperationError::NotFound)
    }
}

pub fn validate_create_mailbox_job_request(
    delay_ms: u64,
    max_attempts: u32,
    cursor: Option<&str>,
) -> Result<(), MailboxJobOperationError> {
    if delay_ms > MAX_JOB_DELAY_MS
        || max_attempts == 0
        || max_attempts > MAX_JOB_ATTEMPTS
        || cursor.is_some_and(|value| value.len() > MAX_CURSOR_LENGTH)
    {
        return Err(MailboxJobOperationError::InvalidRequest);
    }
    Ok(())
}

pub fn validate_mailbox_job_run_version(
    expected_version: AggregateVersion,
) -> Result<AggregateVersion, MailboxJobOperationError> {
    expected_version
        .next()
        .and_then(AggregateVersion::next)
        .map_err(|_| MailboxJobOperationError::InternalFailure)
}

pub async fn execute_create_mailbox_job<P: MailboxJobApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteCreateMailboxJobCommand,
) -> Result<MailboxJobMutationOutcome, MailboxJobOperationError> {
    authorize_mailbox_job(role)?;
    validate_create_mailbox_job_request(
        command.delay_ms,
        command.max_attempts,
        command.cursor.as_deref(),
    )?;
    let scheduled_at = UnixMillis::new(
        command
            .evidence
            .now()
            .value()
            .checked_add(command.delay_ms)
            .ok_or(MailboxJobOperationError::InternalFailure)?,
    );

    match port
        .decide_replay(actor, MAILBOX_JOB_CREATE_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        MailboxReplayDecision::Miss => {}
        MailboxReplayDecision::Replay(receipt) => {
            return Ok(create_replay_outcome(&command.job_id, &receipt));
        }
        MailboxReplayDecision::Conflict => return Err(MailboxJobOperationError::Conflict),
    }

    let write = MailboxJobCreateWrite::new(
        command.binding_id,
        command.job_id,
        command.cursor,
        scheduled_at,
        command.max_attempts,
        command.evidence,
        MAILBOX_JOB_EVENT_PAYLOAD,
    );
    match port.create_job(actor, &write).await {
        Ok(()) => Ok(MailboxJobMutationOutcome {
            result_code: "created".to_owned(),
            resource_id: write.job_id().as_str().to_owned(),
            aggregate_version: AggregateVersion::INITIAL,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxJobPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, MAILBOX_JOB_CREATE_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxReplayDecision::Replay(receipt) => {
                    Ok(create_replay_outcome(write.job_id(), &receipt))
                }
                MailboxReplayDecision::Miss | MailboxReplayDecision::Conflict => {
                    Err(MailboxJobOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn get_mailbox_job<P: MailboxJobApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    binding_id: &MailboxBindingId,
    job_id: &MailboxJobId,
) -> Result<MailboxJobDetails, MailboxJobOperationError> {
    authorize_mailbox_job(role)?;
    port.find_job(actor.tenant_scope(), binding_id, job_id)
        .await
        .map_err(map_port_error)?
        .map(MailboxJobDetails::from)
        .ok_or(MailboxJobOperationError::NotFound)
}

pub async fn execute_run_mailbox_job<P: MailboxJobApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &mut P,
    command: ExecuteRunMailboxJobCommand,
) -> Result<MailboxJobMutationOutcome, MailboxJobOperationError> {
    authorize_mailbox_job(role)?;
    let response_version = validate_mailbox_job_run_version(command.expected_version)?;

    match port
        .decide_replay(actor, MAILBOX_JOB_RUN_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        MailboxReplayDecision::Miss => {}
        MailboxReplayDecision::Replay(receipt) => {
            return Ok(run_replay_outcome(
                &command.job_id,
                response_version,
                &receipt,
            ));
        }
        MailboxReplayDecision::Conflict => return Err(MailboxJobOperationError::Conflict),
    }

    let binding = port
        .find_binding(actor.tenant_scope(), &command.binding_id)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxJobOperationError::NotFound)?;
    let job = port
        .find_job(actor.tenant_scope(), &command.binding_id, &command.job_id)
        .await
        .map_err(map_port_error)?
        .ok_or(MailboxJobOperationError::NotFound)?;
    if job.job().version() != command.expected_version {
        return Err(MailboxJobOperationError::VersionConflict);
    }

    let prepared = port
        .prepare_run(&binding, job.job(), command.evidence.now())
        .map_err(map_port_error)?;
    if prepared.version() != response_version {
        return Err(MailboxJobOperationError::IntegrityFailure);
    }
    let result_code = result_code(&prepared)?;
    let write = MailboxJobRunWrite::new(
        command.binding_id,
        command.job_id,
        command.expected_version,
        prepared,
        command.evidence,
        MAILBOX_JOB_EVENT_PAYLOAD,
    );
    match port.run_job(actor, &write).await {
        Ok(()) => Ok(MailboxJobMutationOutcome {
            result_code: result_code.to_owned(),
            resource_id: write.job_id().as_str().to_owned(),
            aggregate_version: response_version,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxJobPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, MAILBOX_JOB_RUN_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxReplayDecision::Replay(receipt) => Ok(run_replay_outcome(
                    write.job_id(),
                    response_version,
                    &receipt,
                )),
                MailboxReplayDecision::Miss | MailboxReplayDecision::Conflict => {
                    Err(MailboxJobOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn result_code<D>(
    prepared: &MailboxJobPreparedRun<D>,
) -> Result<&'static str, MailboxJobOperationError> {
    match prepared.status() {
        MailboxJobStatus::Succeeded => Ok("succeeded"),
        MailboxJobStatus::RetryPending => Ok("retry_pending"),
        MailboxJobStatus::Failed => Ok("failed"),
        MailboxJobStatus::Pending | MailboxJobStatus::Running => {
            Err(MailboxJobOperationError::IntegrityFailure)
        }
    }
}

fn create_replay_outcome(
    job_id: &MailboxJobId,
    receipt: &MailboxReplayReceipt,
) -> MailboxJobMutationOutcome {
    MailboxJobMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(job_id.as_str())
            .to_owned(),
        aggregate_version: AggregateVersion::INITIAL,
        replayed: true,
    }
}

fn run_replay_outcome(
    job_id: &MailboxJobId,
    response_version: AggregateVersion,
    receipt: &MailboxReplayReceipt,
) -> MailboxJobMutationOutcome {
    MailboxJobMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(job_id.as_str())
            .to_owned(),
        aggregate_version: response_version,
        replayed: true,
    }
}

fn map_port_error(error: MailboxJobPortError) -> MailboxJobOperationError {
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
    use super::{
        ExecuteCreateMailboxJobCommand, ExecuteRunMailboxJobCommand, MailboxJobOperationError,
        authorize_mailbox_job, execute_create_mailbox_job, execute_run_mailbox_job,
        get_mailbox_job, validate_create_mailbox_job_request, validate_mailbox_job_run_version,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::mailbox_jobs::{
        MailboxBinding, MailboxJob, MailboxJobApplicationPort, MailboxJobCreateWrite,
        MailboxJobPortError, MailboxJobPortErrorClass, MailboxJobPreparedRun, MailboxJobReadModel,
        MailboxJobRunWrite, MailboxJobStatus,
    };
    use application_ports::mailboxes::{
        MailboxProvider, MailboxReplayDecision, MailboxReplayReceipt,
    };
    use identity_access_domain::MembershipRole;
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, AuditEventId, CorrelationId, IdempotencyKey,
        MailboxBindingId, MailboxJobId, OutboxEventId, SecretHandle, TenantId, TenantScope,
        UnixMillis,
    };
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct FakeRunToken;

    struct FakeJobPort {
        replay: RefCell<Vec<MailboxReplayDecision>>,
        replay_calls: Cell<u32>,
        create_calls: Cell<u32>,
        run_calls: Cell<u32>,
        binding_reads: Cell<u32>,
        job_reads: Cell<u32>,
        prepare_calls: Cell<u32>,
        create_error: Cell<Option<MailboxJobPortErrorClass>>,
        run_error: Cell<Option<MailboxJobPortErrorClass>>,
        prepare_error: Cell<Option<MailboxJobPortErrorClass>>,
        binding: RefCell<Option<MailboxBinding>>,
        job: RefCell<Option<MailboxJobReadModel>>,
        prepared: RefCell<Option<MailboxJobPreparedRun<FakeRunToken>>>,
    }

    impl FakeJobPort {
        fn new(replay: Vec<MailboxReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                replay_calls: Cell::new(0),
                create_calls: Cell::new(0),
                run_calls: Cell::new(0),
                binding_reads: Cell::new(0),
                job_reads: Cell::new(0),
                prepare_calls: Cell::new(0),
                create_error: Cell::new(None),
                run_error: Cell::new(None),
                prepare_error: Cell::new(None),
                binding: RefCell::new(None),
                job: RefCell::new(None),
                prepared: RefCell::new(None),
            }
        }

        fn next_replay(&self) -> MailboxReplayDecision {
            let mut replay = self.replay.borrow_mut();
            if replay.is_empty() {
                MailboxReplayDecision::Miss
            } else {
                replay.remove(0)
            }
        }
    }

    impl MailboxJobApplicationPort for FakeJobPort {
        type RunDecision = FakeRunToken;

        async fn decide_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<MailboxReplayDecision, MailboxJobPortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            Ok(self.next_replay())
        }

        async fn create_job(
            &self,
            _actor: &ActorContext,
            _write: &MailboxJobCreateWrite,
        ) -> Result<(), MailboxJobPortError> {
            self.create_calls.set(self.create_calls.get() + 1);
            match self.create_error.get() {
                Some(class) => Err(MailboxJobPortError::new(class)),
                None => Ok(()),
            }
        }

        async fn run_job(
            &self,
            _actor: &ActorContext,
            _write: &MailboxJobRunWrite<Self::RunDecision>,
        ) -> Result<(), MailboxJobPortError> {
            self.run_calls.set(self.run_calls.get() + 1);
            match self.run_error.get() {
                Some(class) => Err(MailboxJobPortError::new(class)),
                None => Ok(()),
            }
        }

        async fn find_binding(
            &self,
            _scope: &TenantScope,
            _binding_id: &MailboxBindingId,
        ) -> Result<Option<MailboxBinding>, MailboxJobPortError> {
            self.binding_reads.set(self.binding_reads.get() + 1);
            Ok(self.binding.borrow().clone())
        }

        async fn find_job(
            &self,
            _scope: &TenantScope,
            _binding_id: &MailboxBindingId,
            _job_id: &MailboxJobId,
        ) -> Result<Option<MailboxJobReadModel>, MailboxJobPortError> {
            self.job_reads.set(self.job_reads.get() + 1);
            Ok(self.job.borrow().clone())
        }

        fn prepare_run(
            &mut self,
            _binding: &MailboxBinding,
            _job: &MailboxJob,
            _now: UnixMillis,
        ) -> Result<MailboxJobPreparedRun<Self::RunDecision>, MailboxJobPortError> {
            self.prepare_calls.set(self.prepare_calls.get() + 1);
            if let Some(class) = self.prepare_error.get() {
                return Err(MailboxJobPortError::new(class));
            }
            self.prepared
                .borrow()
                .clone()
                .ok_or_else(|| MailboxJobPortError::new(MailboxJobPortErrorClass::InternalFailure))
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JMAILJOBAPP")?),
            ActorId::parse("actor_01JMAILJOBAPP")?,
            CorrelationId::parse("corr_01JMAILJOBAPP")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JMAILJOBAPP")?,
            "a".repeat(64),
            AuditEventId::parse("audit_01JMAILJOBAPP")?,
            OutboxEventId::parse("outbox_01JMAILJOBAPP")?,
            UnixMillis::new(100),
            UnixMillis::new(1_000),
        ))
    }

    fn binding() -> Result<MailboxBinding, Box<dyn std::error::Error>> {
        Ok(MailboxBinding::create(
            TenantId::parse("tenant_01JMAILJOBAPP")?,
            MailboxBindingId::parse("mailbox_01JMAILJOBAPP")?,
            MailboxProvider::Imap,
            SecretHandle::parse("secret_01JMAILJOBAPP")?,
        ))
    }

    fn job(binding: &MailboxBinding) -> Result<MailboxJob, Box<dyn std::error::Error>> {
        Ok(MailboxJob::create(
            binding,
            MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
            None,
            UnixMillis::new(100),
            3,
        )?)
    }

    fn prepared_success() -> Result<MailboxJobPreparedRun<FakeRunToken>, Box<dyn std::error::Error>>
    {
        Ok(MailboxJobPreparedRun::new(
            FakeRunToken,
            MailboxJobStatus::Succeeded,
            1,
            AggregateVersion::new(3)?,
            Some("meta_mailjob_01JMAILJOBAPP_1".to_owned()),
            "SYNTHETIC_OK",
            0,
            None,
        ))
    }

    fn create_command() -> Result<ExecuteCreateMailboxJobCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteCreateMailboxJobCommand::new(
            MailboxBindingId::parse("mailbox_01JMAILJOBAPP")?,
            MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
            None,
            0,
            3,
            evidence()?,
        ))
    }

    #[test]
    fn owner_only_authorization_is_disclosure_neutral() {
        assert_eq!(authorize_mailbox_job(MembershipRole::TenantOwner), Ok(()));
        assert_eq!(
            authorize_mailbox_job(MembershipRole::Member),
            Err(MailboxJobOperationError::NotFound)
        );
    }

    #[test]
    fn transport_intent_validation_is_pure_and_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(validate_create_mailbox_job_request(0, 3, None), Ok(()));
        assert_eq!(
            validate_create_mailbox_job_request(0, 3, Some(&"x".repeat(513))),
            Err(MailboxJobOperationError::InvalidRequest)
        );
        assert_eq!(
            validate_mailbox_job_run_version(AggregateVersion::INITIAL)?.value(),
            3
        );
        assert_eq!(
            validate_mailbox_job_run_version(AggregateVersion::new(u64::MAX)?),
            Err(MailboxJobOperationError::InternalFailure)
        );
        Ok(())
    }

    #[test]
    fn invalid_create_request_fails_before_replay_or_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeJobPort::new(vec![MailboxReplayDecision::Miss]);
        let command = ExecuteCreateMailboxJobCommand::new(
            MailboxBindingId::parse("mailbox_01JMAILJOBAPP")?,
            MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
            Some("x".repeat(513)),
            0,
            3,
            evidence()?,
        );
        assert_eq!(
            block_on(execute_create_mailbox_job(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command,
            )),
            Err(MailboxJobOperationError::InvalidRequest)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn create_exact_replay_skips_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeJobPort::new(vec![MailboxReplayDecision::Replay(
            MailboxReplayReceipt::new("created", Some("mailjob_existing".to_owned())),
        )]);
        let outcome = block_on(execute_create_mailbox_job(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            create_command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "mailjob_existing");
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn create_unique_conflict_rechecks_exact_replay() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeJobPort::new(vec![
            MailboxReplayDecision::Miss,
            MailboxReplayDecision::Replay(MailboxReplayReceipt::new(
                "created",
                Some("mailjob_01JMAILJOBAPP".to_owned()),
            )),
        ]);
        port.create_error
            .set(Some(MailboxJobPortErrorClass::Conflict));
        let outcome = block_on(execute_create_mailbox_job(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            create_command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.create_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn query_projects_existing_job_without_payload_data() -> Result<(), Box<dyn std::error::Error>>
    {
        let binding = binding()?;
        let job = job(&binding)?;
        let port = FakeJobPort::new(Vec::new());
        port.job.replace(Some(MailboxJobReadModel::new(
            job,
            Some("SYNTHETIC_OK".to_owned()),
            2,
        )));
        let details = block_on(get_mailbox_job(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            binding.binding_id(),
            &MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
        ))?;
        assert_eq!(details.status(), MailboxJobStatus::Pending);
        assert_eq!(details.provider_status(), Some("SYNTHETIC_OK"));
        assert_eq!(details.bounded_item_count(), 2);
        Ok(())
    }

    #[test]
    fn run_exact_replay_skips_reads_prepare_and_write() -> Result<(), Box<dyn std::error::Error>> {
        let mut port = FakeJobPort::new(vec![MailboxReplayDecision::Replay(
            MailboxReplayReceipt::new("succeeded", Some("mailjob_existing".to_owned())),
        )]);
        port.prepared.replace(Some(prepared_success()?));
        let command = ExecuteRunMailboxJobCommand::new(
            MailboxBindingId::parse("mailbox_01JMAILJOBAPP")?,
            MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
            AggregateVersion::INITIAL,
            evidence()?,
        );
        let outcome = block_on(execute_run_mailbox_job(
            &actor()?,
            MembershipRole::TenantOwner,
            &mut port,
            command,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.aggregate_version().value(), 3);
        assert_eq!(port.binding_reads.get(), 0);
        assert_eq!(port.job_reads.get(), 0);
        assert_eq!(port.prepare_calls.get(), 0);
        assert_eq!(port.run_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn run_version_mismatch_stops_before_prepare_and_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding)?;
        let mut port = FakeJobPort::new(vec![MailboxReplayDecision::Miss]);
        port.binding.replace(Some(binding.clone()));
        port.job
            .replace(Some(MailboxJobReadModel::new(job, None, 0)));
        port.prepared.replace(Some(prepared_success()?));
        let command = ExecuteRunMailboxJobCommand::new(
            binding.binding_id().clone(),
            MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
            AggregateVersion::new(2)?,
            evidence()?,
        );
        assert_eq!(
            block_on(execute_run_mailbox_job(
                &actor()?,
                MembershipRole::TenantOwner,
                &mut port,
                command,
            )),
            Err(MailboxJobOperationError::VersionConflict)
        );
        assert_eq!(port.prepare_calls.get(), 0);
        assert_eq!(port.run_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn successful_run_prepares_once_and_persists_version_plus_two()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding)?;
        let mut port = FakeJobPort::new(vec![MailboxReplayDecision::Miss]);
        port.binding.replace(Some(binding.clone()));
        port.job
            .replace(Some(MailboxJobReadModel::new(job, None, 0)));
        port.prepared.replace(Some(prepared_success()?));
        let command = ExecuteRunMailboxJobCommand::new(
            binding.binding_id().clone(),
            MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
            AggregateVersion::INITIAL,
            evidence()?,
        );
        let outcome = block_on(execute_run_mailbox_job(
            &actor()?,
            MembershipRole::TenantOwner,
            &mut port,
            command,
        ))?;
        assert_eq!(outcome.result_code(), "succeeded");
        assert_eq!(outcome.aggregate_version().value(), 3);
        assert!(!outcome.replayed());
        assert_eq!(port.prepare_calls.get(), 1);
        assert_eq!(port.run_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn prepare_invalid_state_keeps_public_invalid_state_class()
    -> Result<(), Box<dyn std::error::Error>> {
        let binding = binding()?;
        let job = job(&binding)?;
        let mut port = FakeJobPort::new(vec![MailboxReplayDecision::Miss]);
        port.binding.replace(Some(binding.clone()));
        port.job
            .replace(Some(MailboxJobReadModel::new(job, None, 0)));
        port.prepared.replace(Some(prepared_success()?));
        port.prepare_error
            .set(Some(MailboxJobPortErrorClass::InvalidState));
        let command = ExecuteRunMailboxJobCommand::new(
            binding.binding_id().clone(),
            MailboxJobId::parse("mailjob_01JMAILJOBAPP")?,
            AggregateVersion::INITIAL,
            evidence()?,
        );
        assert_eq!(
            block_on(execute_run_mailbox_job(
                &actor()?,
                MembershipRole::TenantOwner,
                &mut port,
                command,
            )),
            Err(MailboxJobOperationError::InvalidState)
        );
        assert_eq!(port.run_calls.get(), 0);
        Ok(())
    }
}
