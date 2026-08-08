use application_ports::CommandExecutionEvidence;
use application_ports::clients::{
    ClientLifecycleApplicationPort, ClientLifecycleWrite, ClientPortError, ClientPortErrorClass,
    ClientReplayDecision, ClientReplayReceipt,
};
use client_domain::{ClientError, ClientRecord};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId};

const CLIENT_UPDATE_COMMAND: &str = "client.update";
const CLIENT_ARCHIVE_COMMAND: &str = "client.archive";
const EVENT_PAYLOAD: &str = "{}";
const MAX_REQUESTED_DISPLAY_NAME_LEN: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateClientCommand {
    client_id: ClientId,
    expected_version: AggregateVersion,
    display_name: String,
    evidence: CommandExecutionEvidence,
}

impl UpdateClientCommand {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        expected_version: AggregateVersion,
        display_name: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            client_id,
            expected_version,
            display_name: display_name.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveClientCommand {
    client_id: ClientId,
    expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
}

impl ArchiveClientCommand {
    #[must_use]
    pub const fn new(
        client_id: ClientId,
        expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            client_id,
            expected_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientLifecycleOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl ClientLifecycleOutcome {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientLifecycleError {
    NotFound,
    InvalidRequest,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for ClientLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "client not found",
            Self::InvalidRequest => "client lifecycle request is invalid",
            Self::VersionConflict => "client version conflict",
            Self::InvalidState => "client lifecycle state is invalid",
            Self::Conflict => "client lifecycle command conflict",
            Self::IntegrityFailure => "client lifecycle data integrity failure",
            Self::InternalFailure => "client lifecycle internal failure",
            Self::DependencyUnavailable => "client lifecycle dependency unavailable",
        })
    }
}

impl std::error::Error for ClientLifecycleError {}

pub fn authorize_client_lifecycle(role: MembershipRole) -> Result<(), ClientLifecycleError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ClientLifecycleError::NotFound)
    }
}

pub async fn execute_update_client<P: ClientLifecycleApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: UpdateClientCommand,
) -> Result<ClientLifecycleOutcome, ClientLifecycleError> {
    authorize_client_lifecycle(role)?;
    if command.display_name.trim().is_empty()
        || command.display_name.len() > MAX_REQUESTED_DISPLAY_NAME_LEN
    {
        return Err(ClientLifecycleError::InvalidRequest);
    }

    let next_version = next_version(command.expected_version)?;
    if let Some(outcome) = decide_replay(
        actor,
        port,
        CLIENT_UPDATE_COMMAND,
        &command.client_id,
        next_version,
        &command.evidence,
    )
    .await?
    {
        return Ok(outcome);
    }

    let mut client = load_exact_version(
        actor,
        port,
        &command.client_id,
        command.expected_version,
    )
    .await?;
    client.rename(command.display_name).map_err(map_client_error)?;
    persist_lifecycle(
        actor,
        port,
        CLIENT_UPDATE_COMMAND,
        "updated",
        command.expected_version,
        next_version,
        client,
        command.evidence,
    )
    .await
}

pub async fn execute_archive_client<P: ClientLifecycleApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ArchiveClientCommand,
) -> Result<ClientLifecycleOutcome, ClientLifecycleError> {
    authorize_client_lifecycle(role)?;

    let next_version = next_version(command.expected_version)?;
    if let Some(outcome) = decide_replay(
        actor,
        port,
        CLIENT_ARCHIVE_COMMAND,
        &command.client_id,
        next_version,
        &command.evidence,
    )
    .await?
    {
        return Ok(outcome);
    }

    let mut client = load_exact_version(
        actor,
        port,
        &command.client_id,
        command.expected_version,
    )
    .await?;
    client.archive().map_err(map_client_error)?;
    persist_lifecycle(
        actor,
        port,
        CLIENT_ARCHIVE_COMMAND,
        "archived",
        command.expected_version,
        next_version,
        client,
        command.evidence,
    )
    .await
}

fn next_version(version: AggregateVersion) -> Result<AggregateVersion, ClientLifecycleError> {
    version
        .next()
        .map_err(|_| ClientLifecycleError::InternalFailure)
}

async fn decide_replay<P: ClientLifecycleApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command_name: &str,
    client_id: &ClientId,
    next_version: AggregateVersion,
    evidence: &CommandExecutionEvidence,
) -> Result<Option<ClientLifecycleOutcome>, ClientLifecycleError> {
    match port
        .decide_client_lifecycle_replay(actor, command_name, evidence)
        .await
        .map_err(map_port_error)?
    {
        ClientReplayDecision::Miss => Ok(None),
        ClientReplayDecision::Replay(receipt) => {
            Ok(Some(replay_outcome(client_id, next_version, &receipt)))
        }
        ClientReplayDecision::Conflict => Err(ClientLifecycleError::Conflict),
    }
}

async fn load_exact_version<P: ClientLifecycleApplicationPort>(
    actor: &ActorContext,
    port: &P,
    client_id: &ClientId,
    expected_version: AggregateVersion,
) -> Result<ClientRecord, ClientLifecycleError> {
    let client = port
        .load_client_for_mutation(actor.tenant_scope(), client_id)
        .await
        .map_err(map_port_error)?
        .ok_or(ClientLifecycleError::NotFound)?;
    if client.version() != expected_version {
        return Err(ClientLifecycleError::VersionConflict);
    }
    Ok(client)
}

#[allow(clippy::too_many_arguments)]
async fn persist_lifecycle<P: ClientLifecycleApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command_name: &str,
    fresh_result_code: &str,
    expected_version: AggregateVersion,
    next_version: AggregateVersion,
    client: ClientRecord,
    evidence: CommandExecutionEvidence,
) -> Result<ClientLifecycleOutcome, ClientLifecycleError> {
    debug_assert_eq!(client.version(), next_version);
    let write = ClientLifecycleWrite::new(client, expected_version, evidence, EVENT_PAYLOAD);
    match port.persist_client_lifecycle(actor, &write).await {
        Ok(()) => Ok(ClientLifecycleOutcome {
            result_code: fresh_result_code.to_owned(),
            resource_id: write.client().client_id().as_str().to_owned(),
            aggregate_version: next_version,
            replayed: false,
        }),
        Err(error) if error.class() == ClientPortErrorClass::Conflict => {
            match port
                .decide_client_lifecycle_replay(actor, command_name, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                ClientReplayDecision::Replay(receipt) => Ok(replay_outcome(
                    write.client().client_id(),
                    next_version,
                    &receipt,
                )),
                ClientReplayDecision::Miss | ClientReplayDecision::Conflict => {
                    Err(ClientLifecycleError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn replay_outcome(
    client_id: &ClientId,
    version: AggregateVersion,
    receipt: &ClientReplayReceipt,
) -> ClientLifecycleOutcome {
    ClientLifecycleOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(client_id.as_str())
            .to_owned(),
        aggregate_version: version,
        replayed: true,
    }
}

fn map_client_error(error: ClientError) -> ClientLifecycleError {
    match error {
        ClientError::InvalidDisplayName => ClientLifecycleError::InvalidRequest,
        ClientError::InvalidStatusTransition => ClientLifecycleError::InvalidState,
        ClientError::VersionOverflow => ClientLifecycleError::InternalFailure,
    }
}

fn map_port_error(error: ClientPortError) -> ClientLifecycleError {
    match error.class() {
        ClientPortErrorClass::Conflict => ClientLifecycleError::Conflict,
        ClientPortErrorClass::IntegrityFailure => ClientLifecycleError::IntegrityFailure,
        ClientPortErrorClass::InternalFailure => ClientLifecycleError::InternalFailure,
        ClientPortErrorClass::DependencyUnavailable => ClientLifecycleError::DependencyUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArchiveClientCommand, ClientLifecycleError, UpdateClientCommand, execute_archive_client,
        execute_update_client,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::clients::{
        ClientLifecycleApplicationPort, ClientLifecycleWrite, ClientPortError, ClientReplayDecision,
        ClientReplayReceipt,
    };
    use client_domain::{ClientKind, ClientRecord, ClientStatus};
    use identity_access_domain::MembershipRole;
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, AuditEventId, ClientId, CorrelationId,
        IdempotencyKey, OutboxEventId, TenantId, TenantScope, UnixMillis,
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
                Poll::Ready(value) => return value,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct FakePort {
        loaded: RefCell<Option<ClientRecord>>,
        replay: RefCell<Vec<ClientReplayDecision>>,
        load_calls: Cell<u32>,
        replay_calls: Cell<u32>,
        write_calls: Cell<u32>,
        last_write: RefCell<Option<ClientLifecycleWrite>>,
    }

    impl FakePort {
        fn new(client: Option<ClientRecord>, replay: Vec<ClientReplayDecision>) -> Self {
            Self {
                loaded: RefCell::new(client),
                replay: RefCell::new(replay),
                load_calls: Cell::new(0),
                replay_calls: Cell::new(0),
                write_calls: Cell::new(0),
                last_write: RefCell::new(None),
            }
        }
    }

    impl ClientLifecycleApplicationPort for FakePort {
        async fn load_client_for_mutation(
            &self,
            _scope: &TenantScope,
            _client_id: &ClientId,
        ) -> Result<Option<ClientRecord>, ClientPortError> {
            self.load_calls.set(self.load_calls.get() + 1);
            Ok(self.loaded.borrow().clone())
        }

        async fn decide_client_lifecycle_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ClientReplayDecision, ClientPortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            Ok(if self.replay.borrow().is_empty() {
                ClientReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            })
        }

        async fn persist_client_lifecycle(
            &self,
            _actor: &ActorContext,
            write: &ClientLifecycleWrite,
        ) -> Result<(), ClientPortError> {
            self.write_calls.set(self.write_calls.get() + 1);
            self.last_write.replace(Some(write.clone()));
            Ok(())
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JLIFECYCLE")?),
            ActorId::parse("actor_01JLIFECYCLE")?,
            CorrelationId::parse("corr_01JLIFECYCLE")?,
        ))
    }

    fn client(version: AggregateVersion) -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::restore(
            TenantId::parse("tenant_01JLIFECYCLE")?,
            ClientId::parse("client_01JLIFECYCLE")?,
            version,
            ClientKind::Person,
            "Client Before",
            ClientStatus::Active,
        )?)
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JLIFECYCLE")?,
            "digest_01JLIFECYCLE",
            AuditEventId::parse("audit_01JLIFECYCLE")?,
            OutboxEventId::parse("outbox_01JLIFECYCLE")?,
            UnixMillis::new(10),
            UnixMillis::new(20),
        ))
    }

    #[test]
    fn authorization_precedes_replay_load_and_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(Some(client(AggregateVersion::INITIAL)?), Vec::new());
        let command = ArchiveClientCommand::new(
            ClientId::parse("client_01JLIFECYCLE")?,
            AggregateVersion::INITIAL,
            evidence()?,
        );
        assert_eq!(
            block_on(execute_archive_client(
                &actor()?,
                MembershipRole::Member,
                &port,
                command,
            )),
            Err(ClientLifecycleError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.load_calls.get(), 0);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_replay_precedes_current_version_load_and_skips_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            Some(client(AggregateVersion::new(2)?)?),
            vec![ClientReplayDecision::Replay(ClientReplayReceipt::new(
                "updated",
                None,
            ))],
        );
        let command = UpdateClientCommand::new(
            ClientId::parse("client_01JLIFECYCLE")?,
            AggregateVersion::INITIAL,
            "Client After",
            evidence()?,
        );
        let outcome = block_on(execute_update_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.aggregate_version().value(), 2);
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.load_calls.get(), 0);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn version_validation_follows_replay_miss_and_precedes_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            Some(client(AggregateVersion::new(2)?)?),
            vec![ClientReplayDecision::Miss],
        );
        let command = ArchiveClientCommand::new(
            ClientId::parse("client_01JLIFECYCLE")?,
            AggregateVersion::INITIAL,
            evidence()?,
        );
        assert_eq!(
            block_on(execute_archive_client(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command,
            )),
            Err(ClientLifecycleError::VersionConflict)
        );
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.load_calls.get(), 1);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn archive_intent_writes_only_mutated_typed_record() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            Some(client(AggregateVersion::INITIAL)?),
            vec![ClientReplayDecision::Miss],
        );
        let command = ArchiveClientCommand::new(
            ClientId::parse("client_01JLIFECYCLE")?,
            AggregateVersion::INITIAL,
            evidence()?,
        );
        let outcome = block_on(execute_archive_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command,
        ))?;
        assert_eq!(outcome.result_code(), "archived");
        assert_eq!(outcome.aggregate_version().value(), 2);
        let write = port.last_write.borrow();
        let write = write.as_ref().ok_or("missing lifecycle write")?;
        assert_eq!(write.expected_version(), AggregateVersion::INITIAL);
        assert_eq!(write.client().status(), ClientStatus::Archived);
        assert_eq!(write.client().version().value(), 2);
        Ok(())
    }
}
