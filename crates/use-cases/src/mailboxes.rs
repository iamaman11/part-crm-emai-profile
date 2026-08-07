use application_ports::CommandExecutionEvidence;
use application_ports::mailboxes::{
    MailboxBinding, MailboxBindingApplicationPort, MailboxBindingCreateWrite,
    MailboxBindingPortError, MailboxBindingPortErrorClass, MailboxBindingReadModel,
    MailboxBindingRevokeWrite, MailboxBindingStatus, MailboxProvider, MailboxReplayDecision,
    MailboxReplayReceipt,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, MailboxBindingId, SecretHandle,
};

const MAILBOX_BINDING_CREATE_COMMAND: &str = "mailbox.binding_create";
const MAILBOX_BINDING_REVOKE_COMMAND: &str = "mailbox.binding_revoke";
const MAILBOX_BINDING_EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteCreateMailboxBindingCommand {
    binding_id: MailboxBindingId,
    provider: MailboxProvider,
    secret_handle: SecretHandle,
    evidence: CommandExecutionEvidence,
}

impl ExecuteCreateMailboxBindingCommand {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        provider: MailboxProvider,
        secret_handle: SecretHandle,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            provider,
            secret_handle,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteRevokeMailboxBindingCommand {
    binding_id: MailboxBindingId,
    expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
}

impl ExecuteRevokeMailboxBindingCommand {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            binding_id,
            expected_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxBindingMutationOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl MailboxBindingMutationOutcome {
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
pub struct MailboxBindingDetails {
    binding_id: MailboxBindingId,
    provider: MailboxProvider,
    status: MailboxBindingStatus,
    version: AggregateVersion,
}

impl MailboxBindingDetails {
    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn provider(&self) -> MailboxProvider {
        self.provider
    }

    #[must_use]
    pub const fn status(&self) -> MailboxBindingStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

impl From<MailboxBindingReadModel> for MailboxBindingDetails {
    fn from(value: MailboxBindingReadModel) -> Self {
        Self {
            binding_id: value.binding_id().clone(),
            provider: value.provider(),
            status: value.status(),
            version: value.version(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxBindingOperationError {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for MailboxBindingOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "mailbox binding not found",
            Self::VersionConflict => "mailbox binding version conflict",
            Self::InvalidState => "mailbox binding invalid state",
            Self::Conflict => "mailbox binding command conflict",
            Self::IntegrityFailure => "mailbox binding integrity failure",
            Self::InternalFailure => "mailbox binding internal failure",
            Self::DependencyUnavailable => "mailbox binding dependency unavailable",
        })
    }
}

impl std::error::Error for MailboxBindingOperationError {}

pub fn authorize_mailbox_binding(
    role: MembershipRole,
) -> Result<(), MailboxBindingOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(MailboxBindingOperationError::NotFound)
    }
}

pub async fn execute_create_mailbox_binding<P: MailboxBindingApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteCreateMailboxBindingCommand,
) -> Result<MailboxBindingMutationOutcome, MailboxBindingOperationError> {
    authorize_mailbox_binding(role)?;

    let binding = MailboxBinding::create(
        actor.tenant_scope().tenant_id().clone(),
        command.binding_id,
        command.provider,
        command.secret_handle,
    );

    match port
        .decide_replay(actor, MAILBOX_BINDING_CREATE_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        MailboxReplayDecision::Miss => {}
        MailboxReplayDecision::Replay(receipt) => {
            return Ok(create_replay_outcome(&binding, &receipt));
        }
        MailboxReplayDecision::Conflict => return Err(MailboxBindingOperationError::Conflict),
    }

    let write = MailboxBindingCreateWrite::new(
        binding,
        command.evidence,
        MAILBOX_BINDING_EVENT_PAYLOAD,
    );
    match port.create_binding(actor, &write).await {
        Ok(()) => Ok(MailboxBindingMutationOutcome {
            result_code: "created".to_owned(),
            resource_id: write.binding().binding_id().as_str().to_owned(),
            aggregate_version: AggregateVersion::INITIAL,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxBindingPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, MAILBOX_BINDING_CREATE_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxReplayDecision::Replay(receipt) => {
                    Ok(create_replay_outcome(write.binding(), &receipt))
                }
                MailboxReplayDecision::Miss | MailboxReplayDecision::Conflict => {
                    Err(MailboxBindingOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn execute_revoke_mailbox_binding<P: MailboxBindingApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteRevokeMailboxBindingCommand,
) -> Result<MailboxBindingMutationOutcome, MailboxBindingOperationError> {
    authorize_mailbox_binding(role)?;
    let next_version = command
        .expected_version
        .next()
        .map_err(|_| MailboxBindingOperationError::InternalFailure)?;

    match port
        .decide_replay(actor, MAILBOX_BINDING_REVOKE_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        MailboxReplayDecision::Miss => {}
        MailboxReplayDecision::Replay(receipt) => {
            return Ok(revoke_replay_outcome(
                &command.binding_id,
                next_version,
                &receipt,
            ));
        }
        MailboxReplayDecision::Conflict => return Err(MailboxBindingOperationError::Conflict),
    }

    let write = MailboxBindingRevokeWrite::new(
        command.binding_id,
        command.expected_version,
        command.evidence,
        MAILBOX_BINDING_EVENT_PAYLOAD,
    );
    match port.revoke_binding(actor, &write).await {
        Ok(()) => Ok(MailboxBindingMutationOutcome {
            result_code: "revoked".to_owned(),
            resource_id: write.binding_id().as_str().to_owned(),
            aggregate_version: next_version,
            replayed: false,
        }),
        Err(error) if error.class() == MailboxBindingPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, MAILBOX_BINDING_REVOKE_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                MailboxReplayDecision::Replay(receipt) => Ok(revoke_replay_outcome(
                    write.binding_id(),
                    next_version,
                    &receipt,
                )),
                MailboxReplayDecision::Miss | MailboxReplayDecision::Conflict => {
                    Err(MailboxBindingOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn get_mailbox_binding<P: MailboxBindingApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    binding_id: &MailboxBindingId,
) -> Result<MailboxBindingDetails, MailboxBindingOperationError> {
    authorize_mailbox_binding(role)?;
    port.find_binding(actor.tenant_scope(), binding_id)
        .await
        .map_err(map_port_error)?
        .map(MailboxBindingDetails::from)
        .ok_or(MailboxBindingOperationError::NotFound)
}

fn create_replay_outcome(
    binding: &MailboxBinding,
    receipt: &MailboxReplayReceipt,
) -> MailboxBindingMutationOutcome {
    MailboxBindingMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(binding.binding_id().as_str())
            .to_owned(),
        aggregate_version: AggregateVersion::INITIAL,
        replayed: true,
    }
}

fn revoke_replay_outcome(
    binding_id: &MailboxBindingId,
    next_version: AggregateVersion,
    receipt: &MailboxReplayReceipt,
) -> MailboxBindingMutationOutcome {
    MailboxBindingMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(binding_id.as_str())
            .to_owned(),
        aggregate_version: next_version,
        replayed: true,
    }
}

fn map_port_error(error: MailboxBindingPortError) -> MailboxBindingOperationError {
    match error.class() {
        MailboxBindingPortErrorClass::NotFound => MailboxBindingOperationError::NotFound,
        MailboxBindingPortErrorClass::VersionConflict => {
            MailboxBindingOperationError::VersionConflict
        }
        MailboxBindingPortErrorClass::InvalidState => MailboxBindingOperationError::InvalidState,
        MailboxBindingPortErrorClass::Conflict => MailboxBindingOperationError::Conflict,
        MailboxBindingPortErrorClass::IntegrityFailure => {
            MailboxBindingOperationError::IntegrityFailure
        }
        MailboxBindingPortErrorClass::InternalFailure => {
            MailboxBindingOperationError::InternalFailure
        }
        MailboxBindingPortErrorClass::DependencyUnavailable => {
            MailboxBindingOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExecuteCreateMailboxBindingCommand, ExecuteRevokeMailboxBindingCommand,
        MailboxBindingOperationError, authorize_mailbox_binding, execute_create_mailbox_binding,
        execute_revoke_mailbox_binding, get_mailbox_binding,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::mailboxes::{
        MailboxBindingApplicationPort, MailboxBindingCreateWrite, MailboxBindingPortError,
        MailboxBindingPortErrorClass, MailboxBindingReadModel, MailboxBindingRevokeWrite,
        MailboxBindingStatus, MailboxProvider, MailboxReplayDecision, MailboxReplayReceipt,
    };
    use identity_access_domain::MembershipRole;
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, AuditEventId, CorrelationId, IdempotencyKey,
        MailboxBindingId, OutboxEventId, SecretHandle, TenantId, TenantScope, UnixMillis,
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

    struct FakeMailboxBindingPort {
        replay: RefCell<Vec<MailboxReplayDecision>>,
        replay_calls: Cell<u32>,
        create_error: Cell<Option<MailboxBindingPortErrorClass>>,
        revoke_error: Cell<Option<MailboxBindingPortErrorClass>>,
        create_calls: Cell<u32>,
        revoke_calls: Cell<u32>,
        visible: RefCell<Option<MailboxBindingReadModel>>,
    }

    impl FakeMailboxBindingPort {
        fn new(replay: Vec<MailboxReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                replay_calls: Cell::new(0),
                create_error: Cell::new(None),
                revoke_error: Cell::new(None),
                create_calls: Cell::new(0),
                revoke_calls: Cell::new(0),
                visible: RefCell::new(None),
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

    impl MailboxBindingApplicationPort for FakeMailboxBindingPort {
        async fn decide_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<MailboxReplayDecision, MailboxBindingPortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            Ok(self.next_replay())
        }

        async fn create_binding(
            &self,
            _actor: &ActorContext,
            _write: &MailboxBindingCreateWrite,
        ) -> Result<(), MailboxBindingPortError> {
            self.create_calls.set(self.create_calls.get() + 1);
            match self.create_error.get() {
                Some(class) => Err(MailboxBindingPortError::new(class)),
                None => Ok(()),
            }
        }

        async fn revoke_binding(
            &self,
            _actor: &ActorContext,
            _write: &MailboxBindingRevokeWrite,
        ) -> Result<(), MailboxBindingPortError> {
            self.revoke_calls.set(self.revoke_calls.get() + 1);
            match self.revoke_error.get() {
                Some(class) => Err(MailboxBindingPortError::new(class)),
                None => Ok(()),
            }
        }

        async fn find_binding(
            &self,
            _scope: &TenantScope,
            _binding_id: &MailboxBindingId,
        ) -> Result<Option<MailboxBindingReadModel>, MailboxBindingPortError> {
            Ok(self.visible.borrow().clone())
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JMAILBOXAPP")?),
            ActorId::parse("actor_01JMAILBOXAPP")?,
            CorrelationId::parse("corr_01JMAILBOXAPP")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JMAILBOXAPP")?,
            "digest_01JMAILBOXAPP",
            AuditEventId::parse("audit_01JMAILBOXAPP")?,
            OutboxEventId::parse("outbox_01JMAILBOXAPP")?,
            UnixMillis::new(10),
            UnixMillis::new(20),
        ))
    }

    fn create_command() -> Result<ExecuteCreateMailboxBindingCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteCreateMailboxBindingCommand::new(
            MailboxBindingId::parse("mailbox_01JMAILBOXAPP")?,
            MailboxProvider::Imap,
            SecretHandle::parse("secret://mailbox/01JMAILBOXAPP")?,
            evidence()?,
        ))
    }

    #[test]
    fn owner_only_authorization_is_disclosure_neutral() {
        assert_eq!(authorize_mailbox_binding(MembershipRole::TenantOwner), Ok(()));
        assert_eq!(
            authorize_mailbox_binding(MembershipRole::Member),
            Err(MailboxBindingOperationError::NotFound)
        );
    }

    #[test]
    fn create_exact_replay_skips_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeMailboxBindingPort::new(vec![MailboxReplayDecision::Replay(
            MailboxReplayReceipt::new("created", Some("mailbox_existing".to_owned())),
        )]);
        let outcome = block_on(execute_create_mailbox_binding(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            create_command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "mailbox_existing");
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn non_owner_never_reads_replay_or_writes() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeMailboxBindingPort::new(vec![MailboxReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_create_mailbox_binding(
                &actor()?,
                MembershipRole::Member,
                &port,
                create_command()?,
            )),
            Err(MailboxBindingOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn create_unique_conflict_rechecks_exact_replay()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeMailboxBindingPort::new(vec![
            MailboxReplayDecision::Miss,
            MailboxReplayDecision::Replay(MailboxReplayReceipt::new(
                "created",
                Some("mailbox_01JMAILBOXAPP".to_owned()),
            )),
        ]);
        port.create_error
            .set(Some(MailboxBindingPortErrorClass::Conflict));
        let outcome = block_on(execute_create_mailbox_binding(
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
    fn revoke_version_overflow_fails_before_replay() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeMailboxBindingPort::new(vec![MailboxReplayDecision::Miss]);
        let command = ExecuteRevokeMailboxBindingCommand::new(
            MailboxBindingId::parse("mailbox_01JMAILBOXAPP")?,
            AggregateVersion::new(u64::MAX)?,
            evidence()?,
        );
        assert_eq!(
            block_on(execute_revoke_mailbox_binding(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command,
            )),
            Err(MailboxBindingOperationError::InternalFailure)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.revoke_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn visible_binding_view_omits_secret_handle() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeMailboxBindingPort::new(Vec::new());
        port.visible.replace(Some(MailboxBindingReadModel::new(
            MailboxBindingId::parse("mailbox_01JMAILBOXAPP")?,
            MailboxProvider::GmailApi,
            MailboxBindingStatus::Active,
            AggregateVersion::new(3)?,
        )));
        let binding_id = MailboxBindingId::parse("mailbox_01JMAILBOXAPP")?;
        let details = block_on(get_mailbox_binding(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            &binding_id,
        ))?;
        assert_eq!(details.binding_id(), &binding_id);
        assert_eq!(details.provider(), MailboxProvider::GmailApi);
        assert_eq!(details.status(), MailboxBindingStatus::Active);
        assert_eq!(details.version().value(), 3);
        Ok(())
    }
}
