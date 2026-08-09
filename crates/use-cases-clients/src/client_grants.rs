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
    use super::{
        ClientGrantAction, ClientGrantOperationError, ExecuteClientGrantCommand,
        authorize_client_grant, execute_client_grant,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::clients::{
        ClientGrantApplicationPort, ClientGrantPortError, ClientGrantWrite, ClientReplayDecision,
        ClientReplayReceipt,
    };
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
        replay: RefCell<Vec<ClientReplayDecision>>,
        grant_calls: Cell<u32>,
        revoke_calls: Cell<u32>,
    }

    impl FakePort {
        fn new(replay: Vec<ClientReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                grant_calls: Cell::new(0),
                revoke_calls: Cell::new(0),
            }
        }
    }

    impl ClientGrantApplicationPort for FakePort {
        async fn decide_client_grant_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ClientReplayDecision, ClientGrantPortError> {
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
            Ok(())
        }

        async fn revoke_client_grant(
            &self,
            _actor: &ActorContext,
            _write: &ClientGrantWrite,
        ) -> Result<(), ClientGrantPortError> {
            self.revoke_calls.set(self.revoke_calls.get() + 1);
            Ok(())
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

    fn command() -> Result<ExecuteClientGrantCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteClientGrantCommand::new(
            ActorId::parse("actor_01JCLIENTTARGET")?,
            ClientId::parse("client_01JCLIENTGRANT")?,
            AggregateVersion::INITIAL,
            "CLIENT_VIEWER",
            "operator requested client grant",
            evidence()?,
        ))
    }

    #[test]
    fn non_owner_stops_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(Vec::new());
        assert_eq!(
            block_on(execute_client_grant(
                &actor()?,
                MembershipRole::Member,
                &port,
                ClientGrantAction::Grant,
                command()?,
            )),
            Err(ClientGrantOperationError::NotFound)
        );
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_replay_skips_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Replay(
            ClientReplayReceipt::new("granted", Some("client_existing".to_owned())),
        )]);
        let outcome = block_on(execute_client_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ClientGrantAction::Grant,
            command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "client_existing");
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn owner_grant_preserves_version_contract() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ClientReplayDecision::Miss]);
        let outcome = block_on(execute_client_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ClientGrantAction::Grant,
            command()?,
        ))?;
        assert_eq!(outcome.aggregate_version().value(), 2);
        assert_eq!(port.grant_calls.get(), 1);
        assert_eq!(port.revoke_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn authorization_is_disclosure_neutral() {
        assert_eq!(authorize_client_grant(MembershipRole::TenantOwner), Ok(()));
        assert_eq!(
            authorize_client_grant(MembershipRole::Member),
            Err(ClientGrantOperationError::NotFound)
        );
    }
}
