use application_ports::CommandExecutionEvidence;
use application_ports::client_merge::{ClientMergeApplicationPort, ClientMergeWrite};
use application_ports::clients::{
    ClientPortError, ClientPortErrorClass, ClientReplayDecision, ClientReplayReceipt,
};
use client_domain::{ClientMergeError, merge_clients};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId};

const CLIENT_MERGE_COMMAND: &str = "client.merge";
const EVENT_PAYLOAD: &str = "{}";
const MAX_REASON_LENGTH: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeClientCommand {
    source_client_id: ClientId,
    target_client_id: ClientId,
    expected_source_version: AggregateVersion,
    expected_target_version: AggregateVersion,
    reason: String,
    evidence: CommandExecutionEvidence,
}

impl MergeClientCommand {
    #[must_use]
    pub fn new(
        source_client_id: ClientId,
        target_client_id: ClientId,
        expected_source_version: AggregateVersion,
        expected_target_version: AggregateVersion,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            source_client_id,
            target_client_id,
            expected_source_version,
            expected_target_version,
            reason: reason.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientMergeOutcome {
    source_client_id: ClientId,
    target_client_id: ClientId,
    source_version: AggregateVersion,
    result_code: String,
    replayed: bool,
}

impl ClientMergeOutcome {
    #[must_use]
    pub const fn source_client_id(&self) -> &ClientId {
        &self.source_client_id
    }

    #[must_use]
    pub const fn target_client_id(&self) -> &ClientId {
        &self.target_client_id
    }

    #[must_use]
    pub const fn source_version(&self) -> AggregateVersion {
        self.source_version
    }

    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientMergeApplicationError {
    NotFound,
    InvalidRequest,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for ClientMergeApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "client merge resource not found",
            Self::InvalidRequest => "client merge request is invalid",
            Self::VersionConflict => "client merge version conflict",
            Self::InvalidState => "client merge state is invalid",
            Self::Conflict => "client merge command conflict",
            Self::IntegrityFailure => "client merge data integrity failure",
            Self::InternalFailure => "client merge internal failure",
            Self::DependencyUnavailable => "client merge dependency unavailable",
        })
    }
}

impl std::error::Error for ClientMergeApplicationError {}

pub async fn execute_merge_client<P: ClientMergeApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: MergeClientCommand,
) -> Result<ClientMergeOutcome, ClientMergeApplicationError> {
    authorize_merge(role)?;
    let reason = normalize_reason(&command.reason)?;
    let next_source_version = command
        .expected_source_version
        .next()
        .map_err(|_| ClientMergeApplicationError::InternalFailure)?;

    if let Some(outcome) = decide_replay(actor, port, &command, next_source_version).await? {
        return Ok(outcome);
    }

    let mut source = port
        .load_client_for_merge(actor.tenant_scope(), &command.source_client_id)
        .await
        .map_err(map_port_error)?
        .ok_or(ClientMergeApplicationError::NotFound)?;
    let target = port
        .load_client_for_merge(actor.tenant_scope(), &command.target_client_id)
        .await
        .map_err(map_port_error)?
        .ok_or(ClientMergeApplicationError::NotFound)?;

    if port
        .source_has_active_assignment(actor.tenant_scope(), &command.source_client_id)
        .await
        .map_err(map_port_error)?
    {
        return Err(ClientMergeApplicationError::InvalidState);
    }

    let plan = merge_clients(
        &mut source,
        &target,
        command.expected_source_version,
        command.expected_target_version,
    )
    .map_err(map_merge_error)?;
    debug_assert_eq!(plan.source_next_version(), next_source_version);

    let write = ClientMergeWrite::new(plan, reason, command.evidence, EVENT_PAYLOAD);
    match port.persist_client_merge(actor, &write).await {
        Ok(()) => Ok(ClientMergeOutcome {
            source_client_id: write.plan().source_client_id().clone(),
            target_client_id: write.plan().target_client_id().clone(),
            source_version: write.plan().source_next_version(),
            result_code: "merged".to_owned(),
            replayed: false,
        }),
        Err(error) if error.class() == ClientPortErrorClass::Conflict => {
            match port
                .decide_client_merge_replay(actor, CLIENT_MERGE_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                ClientReplayDecision::Replay(receipt) => Ok(replay_outcome(
                    write.plan().source_client_id(),
                    write.plan().target_client_id(),
                    next_source_version,
                    &receipt,
                )),
                ClientReplayDecision::Miss | ClientReplayDecision::Conflict => {
                    Err(ClientMergeApplicationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn authorize_merge(role: MembershipRole) -> Result<(), ClientMergeApplicationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ClientMergeApplicationError::NotFound)
    }
}

fn normalize_reason(reason: &str) -> Result<String, ClientMergeApplicationError> {
    let reason = reason.trim();
    if reason.is_empty() || reason.len() > MAX_REASON_LENGTH {
        return Err(ClientMergeApplicationError::InvalidRequest);
    }
    Ok(reason.to_owned())
}

async fn decide_replay<P: ClientMergeApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command: &MergeClientCommand,
    source_version: AggregateVersion,
) -> Result<Option<ClientMergeOutcome>, ClientMergeApplicationError> {
    match port
        .decide_client_merge_replay(actor, CLIENT_MERGE_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        ClientReplayDecision::Miss => Ok(None),
        ClientReplayDecision::Replay(receipt) => Ok(Some(replay_outcome(
            &command.source_client_id,
            &command.target_client_id,
            source_version,
            &receipt,
        ))),
        ClientReplayDecision::Conflict => Err(ClientMergeApplicationError::Conflict),
    }
}

fn replay_outcome(
    source_client_id: &ClientId,
    target_client_id: &ClientId,
    source_version: AggregateVersion,
    receipt: &ClientReplayReceipt,
) -> ClientMergeOutcome {
    ClientMergeOutcome {
        source_client_id: source_client_id.clone(),
        target_client_id: receipt
            .result_reference()
            .and_then(|value| ClientId::parse(value).ok())
            .unwrap_or_else(|| target_client_id.clone()),
        source_version,
        result_code: receipt.result_code().to_owned(),
        replayed: true,
    }
}

fn map_merge_error(error: ClientMergeError) -> ClientMergeApplicationError {
    match error {
        ClientMergeError::TenantMismatch | ClientMergeError::SelfMerge => {
            ClientMergeApplicationError::InvalidRequest
        }
        ClientMergeError::SourceVersionConflict | ClientMergeError::TargetVersionConflict => {
            ClientMergeApplicationError::VersionConflict
        }
        ClientMergeError::SourceNotActive
        | ClientMergeError::SourceAlreadyMerged
        | ClientMergeError::TargetNotActive
        | ClientMergeError::MergeCycle => ClientMergeApplicationError::InvalidState,
        ClientMergeError::VersionOverflow | ClientMergeError::InvalidSourceState => {
            ClientMergeApplicationError::InternalFailure
        }
    }
}

fn map_port_error(error: ClientPortError) -> ClientMergeApplicationError {
    match error.class() {
        ClientPortErrorClass::Conflict => ClientMergeApplicationError::Conflict,
        ClientPortErrorClass::IntegrityFailure => ClientMergeApplicationError::IntegrityFailure,
        ClientPortErrorClass::InternalFailure => ClientMergeApplicationError::InternalFailure,
        ClientPortErrorClass::DependencyUnavailable => {
            ClientMergeApplicationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClientMergeApplicationError, ClientMergeOutcome, MergeClientCommand, execute_merge_client,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::client_merge::{ClientMergeApplicationPort, ClientMergeWrite};
    use application_ports::clients::{
        ClientPortError, ClientPortErrorClass, ClientReplayDecision, ClientReplayReceipt,
    };
    use client_domain::{ClientKind, ClientRecord};
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
        source: ClientRecord,
        target: ClientRecord,
        active_assignment: Cell<bool>,
        replay: RefCell<Vec<ClientReplayDecision>>,
        load_calls: Cell<u32>,
        assignment_checks: Cell<u32>,
        write_calls: Cell<u32>,
        write_error: Cell<Option<ClientPortErrorClass>>,
        last_write: RefCell<Option<ClientMergeWrite>>,
    }

    impl FakePort {
        fn new(source: ClientRecord, target: ClientRecord) -> Self {
            Self {
                source,
                target,
                active_assignment: Cell::new(false),
                replay: RefCell::new(Vec::new()),
                load_calls: Cell::new(0),
                assignment_checks: Cell::new(0),
                write_calls: Cell::new(0),
                write_error: Cell::new(None),
                last_write: RefCell::new(None),
            }
        }

        fn push_replay(&self, decision: ClientReplayDecision) {
            self.replay.borrow_mut().push(decision);
        }

        fn next_replay(&self) -> ClientReplayDecision {
            if self.replay.borrow().is_empty() {
                ClientReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            }
        }
    }

    impl ClientMergeApplicationPort for FakePort {
        async fn load_client_for_merge(
            &self,
            _scope: &TenantScope,
            client_id: &ClientId,
        ) -> Result<Option<ClientRecord>, ClientPortError> {
            self.load_calls.set(self.load_calls.get() + 1);
            if client_id == self.source.client_id() {
                Ok(Some(self.source.clone()))
            } else if client_id == self.target.client_id() {
                Ok(Some(self.target.clone()))
            } else {
                Ok(None)
            }
        }

        async fn source_has_active_assignment(
            &self,
            _scope: &TenantScope,
            _source_client_id: &ClientId,
        ) -> Result<bool, ClientPortError> {
            self.assignment_checks.set(self.assignment_checks.get() + 1);
            Ok(self.active_assignment.get())
        }

        async fn decide_client_merge_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ClientReplayDecision, ClientPortError> {
            Ok(self.next_replay())
        }

        async fn persist_client_merge(
            &self,
            _actor: &ActorContext,
            write: &ClientMergeWrite,
        ) -> Result<(), ClientPortError> {
            self.write_calls.set(self.write_calls.get() + 1);
            self.last_write.replace(Some(write.clone()));
            match self.write_error.get() {
                Some(class) => Err(ClientPortError::new(class)),
                None => Ok(()),
            }
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JMERGEAPP")?),
            ActorId::parse("actor_01JMERGEAPP")?,
            CorrelationId::parse("corr_01JMERGEAPP")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JMERGEAPP")?,
            "digest_01JMERGEAPP",
            AuditEventId::parse("audit_01JMERGEAPP")?,
            OutboxEventId::parse("outbox_01JMERGEAPP")?,
            UnixMillis::new(100),
            UnixMillis::new(1000),
        ))
    }

    fn client(id: &str) -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::create(
            TenantId::parse("tenant_01JMERGEAPP")?,
            ClientId::parse(id)?,
            ClientKind::Person,
            id,
        )?)
    }

    fn command() -> Result<MergeClientCommand, Box<dyn std::error::Error>> {
        Ok(MergeClientCommand::new(
            ClientId::parse("client_01JMERGEAPP")?,
            ClientId::parse("client_02JMERGEAPP")?,
            AggregateVersion::INITIAL,
            AggregateVersion::INITIAL,
            "deduplicate clients",
            evidence()?,
        ))
    }

    #[test]
    fn unauthorized_stops_before_replay_load_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(client("client_01JMERGEAPP")?, client("client_02JMERGEAPP")?);
        let result = block_on(execute_merge_client(
            &actor()?,
            MembershipRole::Member,
            &port,
            command()?,
        ));
        assert_eq!(result, Err(ClientMergeApplicationError::NotFound));
        assert_eq!(port.load_calls.get(), 0);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_replay_short_circuits_before_load_and_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(client("client_01JMERGEAPP")?, client("client_02JMERGEAPP")?);
        port.push_replay(ClientReplayDecision::Replay(ClientReplayReceipt::new(
            "merged",
            Some("client_02JMERGEAPP".to_owned()),
        )));
        let outcome = block_on(execute_merge_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.load_calls.get(), 0);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn active_assignment_blocks_merge_before_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(client("client_01JMERGEAPP")?, client("client_02JMERGEAPP")?);
        port.active_assignment.set(true);
        let result = block_on(execute_merge_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command()?,
        ));
        assert_eq!(result, Err(ClientMergeApplicationError::InvalidState));
        assert_eq!(port.assignment_checks.get(), 1);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn successful_merge_persists_checked_plan_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(client("client_01JMERGEAPP")?, client("client_02JMERGEAPP")?);
        let outcome = block_on(execute_merge_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command()?,
        ))?;
        assert!(matches!(outcome, ClientMergeOutcome { .. }));
        assert!(!outcome.replayed());
        assert_eq!(outcome.source_version().value(), 2);
        assert_eq!(outcome.target_client_id().as_str(), "client_02JMERGEAPP");
        assert_eq!(port.write_calls.get(), 1);
        let write = port.last_write.borrow();
        let write = write.as_ref().ok_or("missing merge write")?;
        assert_eq!(write.plan().source_expected_version().value(), 1);
        assert_eq!(write.plan().target_expected_version().value(), 1);
        assert_eq!(write.plan().source_next_version().value(), 2);
        assert_eq!(write.reason(), "deduplicate clients");
        Ok(())
    }

    #[test]
    fn post_write_conflict_resolves_exact_replay_without_second_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(client("client_01JMERGEAPP")?, client("client_02JMERGEAPP")?);
        port.write_error.set(Some(ClientPortErrorClass::Conflict));
        port.push_replay(ClientReplayDecision::Miss);
        port.push_replay(ClientReplayDecision::Replay(ClientReplayReceipt::new(
            "merged",
            Some("client_02JMERGEAPP".to_owned()),
        )));
        let outcome = block_on(execute_merge_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.write_calls.get(), 1);
        Ok(())
    }
}
