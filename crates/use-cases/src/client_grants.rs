use application_ports::CommandExecutionEvidence;
use application_ports::clients::{
    ClientGrantApplicationPort, ClientGrantPortError, ClientGrantPortErrorClass, ClientGrantRole,
    ClientGrantWrite, ClientReplayDecision, ClientReplayReceipt,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ActorId, AggregateVersion, ClientId};

const CLIENT_GRANT_COMMAND: &str = "client.grant";
const CLIENT_GRANT_REVOKE_COMMAND: &str = "client.grant_revoke";
const EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientGrantAction {
    Grant,
    Revoke,
}

impl ClientGrantAction {
    const fn command_name(self) -> &'static str {
        match self {
            Self::Grant => CLIENT_GRANT_COMMAND,
            Self::Revoke => CLIENT_GRANT_REVOKE_COMMAND,
        }
    }

    const fn fresh_result_code(self) -> &'static str {
        match self {
            Self::Grant => "granted",
            Self::Revoke => "revoked",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteClientGrantCommand {
    target_actor_id: ActorId,
    client_id: ClientId,
    expected_client_version: AggregateVersion,
    role: String,
    reason: String,
    evidence: CommandExecutionEvidence,
}

impl ExecuteClientGrantCommand {
    #[must_use]
    pub fn new(
        target_actor_id: ActorId,
        client_id: ClientId,
        expected_client_version: AggregateVersion,
        role: impl Into<String>,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            target_actor_id,
            client_id,
            expected_client_version,
            role: role.into(),
            reason: reason.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientGrantOutcome {
    action: ClientGrantAction,
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl ClientGrantOutcome {
    #[must_use]
    pub const fn action(&self) -> ClientGrantAction {
        self.action
    }

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
pub enum ClientGrantOperationError {
    InvalidRequest,
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for ClientGrantOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "client grant request is invalid",
            Self::NotFound => "client grant not found",
            Self::VersionConflict => "client grant version conflict",
            Self::InvalidState => "client grant invalid state",
            Self::Conflict => "client grant conflict",
            Self::IntegrityFailure => "client grant integrity failure",
            Self::InternalFailure => "client grant internal failure",
            Self::DependencyUnavailable => "client grant dependency unavailable",
        })
    }
}

impl std::error::Error for ClientGrantOperationError {}

pub fn authorize_client_grant(role: MembershipRole) -> Result<(), ClientGrantOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ClientGrantOperationError::NotFound)
    }
}

pub fn next_client_grant_version(
    version: AggregateVersion,
) -> Result<AggregateVersion, ClientGrantOperationError> {
    version
        .next()
        .map_err(|_| ClientGrantOperationError::InternalFailure)
}

pub fn parse_client_grant_role(value: &str) -> Result<ClientGrantRole, ClientGrantOperationError> {
    match value {
        "CLIENT_VIEWER" => Ok(ClientGrantRole::Viewer),
        "CLIENT_EDITOR" => Ok(ClientGrantRole::Editor),
        _ => Err(ClientGrantOperationError::InvalidRequest),
    }
}

pub async fn execute_client_grant<P: ClientGrantApplicationPort>(
    actor: &ActorContext,
    membership_role: MembershipRole,
    port: &P,
    action: ClientGrantAction,
    command: ExecuteClientGrantCommand,
) -> Result<ClientGrantOutcome, ClientGrantOperationError> {
    authorize_client_grant(membership_role)?;
    let next_version = next_client_grant_version(command.expected_client_version)?;
    let grant_role = parse_client_grant_role(&command.role)?;
    let command_name = action.command_name();

    match port
        .decide_client_grant_replay(actor, command_name, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        ClientReplayDecision::Miss => {}
        ClientReplayDecision::Replay(receipt) => {
            return Ok(replay_outcome(
                action,
                &command.client_id,
                next_version,
                &receipt,
            ));
        }
        ClientReplayDecision::Conflict => return Err(ClientGrantOperationError::Conflict),
    }

    let write = ClientGrantWrite::new(
        command.target_actor_id,
        command.client_id,
        command.expected_client_version,
        grant_role,
        command.reason,
        command.evidence,
        EVENT_PAYLOAD,
    );
    let result = match action {
        ClientGrantAction::Grant => port.grant_client(actor, &write).await,
        ClientGrantAction::Revoke => port.revoke_client_grant(actor, &write).await,
    };
    match result {
        Ok(()) => Ok(ClientGrantOutcome {
            action,
            result_code: action.fresh_result_code().to_owned(),
            resource_id: write.client_id().as_str().to_owned(),
            aggregate_version: next_version,
            replayed: false,
        }),
        Err(error) if error.class() == ClientGrantPortErrorClass::Conflict => {
            match port
                .decide_client_grant_replay(actor, command_name, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                ClientReplayDecision::Replay(receipt) => Ok(replay_outcome(
                    action,
                    write.client_id(),
                    next_version,
                    &receipt,
                )),
                ClientReplayDecision::Miss | ClientReplayDecision::Conflict => {
                    Err(ClientGrantOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn replay_outcome(
    action: ClientGrantAction,
    client_id: &ClientId,
    version: AggregateVersion,
    receipt: &ClientReplayReceipt,
) -> ClientGrantOutcome {
    ClientGrantOutcome {
        action,
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(client_id.as_str())
            .to_owned(),
        aggregate_version: version,
        replayed: true,
    }
}

fn map_port_error(error: ClientGrantPortError) -> ClientGrantOperationError {
    match error.class() {
        ClientGrantPortErrorClass::NotFound => ClientGrantOperationError::NotFound,
        ClientGrantPortErrorClass::VersionConflict => ClientGrantOperationError::VersionConflict,
        ClientGrantPortErrorClass::InvalidState => ClientGrantOperationError::InvalidState,
        ClientGrantPortErrorClass::Conflict => ClientGrantOperationError::Conflict,
        ClientGrantPortErrorClass::IntegrityFailure => ClientGrantOperationError::IntegrityFailure,
        ClientGrantPortErrorClass::InternalFailure => ClientGrantOperationError::InternalFailure,
        ClientGrantPortErrorClass::DependencyUnavailable => {
            ClientGrantOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use profile_platform_primitives::{
        AuditEventId, CorrelationId, IdempotencyKey, OutboxEventId, TenantId, TenantScope,
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
                Poll::Ready(value) => return value,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct FakePort {
        replay: RefCell<Vec<ClientReplayDecision>>,
        replay_commands: RefCell<Vec<String>>,
        replay_calls: Cell<u32>,
        grant_calls: Cell<u32>,
        revoke_calls: Cell<u32>,
        write_error: Cell<Option<ClientGrantPortErrorClass>>,
    }

    impl FakePort {
        fn new(replay: Vec<ClientReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                replay_commands: RefCell::new(Vec::new()),
                replay_calls: Cell::new(0),
                grant_calls: Cell::new(0),
                revoke_calls: Cell::new(0),
                write_error: Cell::new(None),
            }
        }

        fn write_result(&self) -> Result<(), ClientGrantPortError> {
            match self.write_error.get() {
                Some(class) => Err(ClientGrantPortError::new(class)),
                None => Ok(()),
            }
        }
    }

    impl ClientGrantApplicationPort for FakePort {
        async fn decide_client_grant_replay(
            &self,
            _actor: &ActorContext,
            command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ClientReplayDecision, ClientGrantPortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            self.replay_commands
                .borrow_mut()
                .push(command_name.to_owned());
            Ok(if self.replay.borrow().is_empty() {
                ClientReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            })
        }

        async fn grant_client(
            &self,
            _actor: &ActorContext,
            _write: &ClientGrantWrite,
        ) -> Result<(), ClientGrantPortError> {
            self.grant_calls.set(self.grant_calls.get() + 1);
            self.write_result()
        }

        async fn revoke_client_grant(
            &self,
            _actor: &ActorContext,
            _write: &ClientGrantWrite,
        ) -> Result<(), ClientGrantPortError> {
            self.revoke_calls.set(self.revoke_calls.get() + 1);
            self.write_result()
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JCLIENTGRANT")?),
            ActorId::parse("actor_01JCLIENTGRANT")?,
            CorrelationId::parse("corr_01JCLIENTGRANT")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JCLIENTGRANT")?,
            "request-digest-01JCLIENTGRANT",
            AuditEventId::parse("audit_01JCLIENTGRANT")?,
            OutboxEventId::parse("outbox_01JCLIENTGRANT")?,
            UnixMillis::new(10),
            UnixMillis::new(100),
        ))
    }

    fn command(
        version: AggregateVersion,
        role: &str,
    ) -> Result<ExecuteClientGrantCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteClientGrantCommand::new(
            ActorId::parse("actor_01JCLIENTTARGET")?,
            ClientId::parse("client_01JCLIENTGRANT")?,
            version,
            role,
            "operator requested client grant",
            evidence()?,
        ))
    }

    #[test]
    fn non_owner_stops_before_role_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_client_grant(
                &actor()?,
                MembershipRole::Member,
                &port,
                ClientGrantAction::Grant,
                command(AggregateVersion::INITIAL, "INVALID")?,
            )),
            Err(ClientGrantOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn overflow_stops_before_role_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_client_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ClientGrantAction::Grant,
                command(AggregateVersion::new(u64::MAX)?, "INVALID")?,
            )),
            Err(ClientGrantOperationError::InternalFailure)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn invalid_role_stops_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_client_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ClientGrantAction::Grant,
                command(AggregateVersion::INITIAL, "CLIENT_OPERATOR")?,
            )),
            Err(ClientGrantOperationError::InvalidRequest)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn grant_and_revoke_use_distinct_idempotency_domains() -> Result<(), Box<dyn std::error::Error>>
    {
        let grant = FakePort::new(vec![ClientReplayDecision::Miss]);
        block_on(execute_client_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &grant,
            ClientGrantAction::Grant,
            command(AggregateVersion::INITIAL, "CLIENT_VIEWER")?,
        ))?;
        assert_eq!(
            grant.replay_commands.borrow().as_slice(),
            [CLIENT_GRANT_COMMAND]
        );
        assert_eq!(grant.grant_calls.get(), 1);

        let revoke = FakePort::new(vec![ClientReplayDecision::Miss]);
        block_on(execute_client_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &revoke,
            ClientGrantAction::Revoke,
            command(AggregateVersion::INITIAL, "CLIENT_EDITOR")?,
        ))?;
        assert_eq!(
            revoke.replay_commands.borrow().as_slice(),
            [CLIENT_GRANT_REVOKE_COMMAND]
        );
        assert_eq!(revoke.revoke_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn exact_replay_skips_grant_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Replay(
            ClientReplayReceipt::new("granted", Some("client_existing".to_owned())),
        )]);
        let outcome = block_on(execute_client_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ClientGrantAction::Grant,
            command(AggregateVersion::INITIAL, "CLIENT_VIEWER")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.result_code(), "granted");
        assert_eq!(outcome.resource_id(), "client_existing");
        assert_eq!(outcome.aggregate_version(), AggregateVersion::new(2)?);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn revoke_exact_replay_preserves_revoke_action() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Replay(
            ClientReplayReceipt::new("revoked", Some("client_existing".to_owned())),
        )]);
        let outcome = block_on(execute_client_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ClientGrantAction::Revoke,
            command(AggregateVersion::INITIAL, "CLIENT_VIEWER")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.action(), ClientGrantAction::Revoke);
        assert_eq!(port.revoke_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn conflict_rechecks_exact_replay_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![
            ClientReplayDecision::Miss,
            ClientReplayDecision::Replay(ClientReplayReceipt::new("granted", None)),
        ]);
        port.write_error
            .set(Some(ClientGrantPortErrorClass::Conflict));
        let outcome = block_on(execute_client_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ClientGrantAction::Grant,
            command(AggregateVersion::INITIAL, "CLIENT_EDITOR")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.grant_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn conflict_recheck_miss_remains_conflict() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Miss, ClientReplayDecision::Miss]);
        port.write_error
            .set(Some(ClientGrantPortErrorClass::Conflict));
        assert_eq!(
            block_on(execute_client_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ClientGrantAction::Revoke,
                command(AggregateVersion::INITIAL, "CLIENT_VIEWER")?,
            )),
            Err(ClientGrantOperationError::Conflict)
        );
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.revoke_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn non_conflict_failure_never_rechecks_replay() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![
            ClientReplayDecision::Miss,
            ClientReplayDecision::Replay(ClientReplayReceipt::new("granted", None)),
        ]);
        port.write_error
            .set(Some(ClientGrantPortErrorClass::VersionConflict));
        assert_eq!(
            block_on(execute_client_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ClientGrantAction::Grant,
                command(AggregateVersion::INITIAL, "CLIENT_VIEWER")?,
            )),
            Err(ClientGrantOperationError::VersionConflict)
        );
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.grant_calls.get(), 1);
        Ok(())
    }
}
