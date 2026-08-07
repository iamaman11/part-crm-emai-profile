use crate::error::ApplicationError;
use application_ports::CommandExecutionEvidence;
use application_ports::clients::{
    ClientApplicationPort, ClientCreateWrite, ClientPortError, ClientPortErrorClass, ClientReadModel,
    ClientReplayDecision, ClientReplayReceipt,
};
use client_domain::{ClientError, ClientKind, ClientRecord, ClientStatus};
use contracts::ProblemCode;
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, ClientId, TenantId};

const CLIENT_CREATE_COMMAND: &str = "client.create";
const CLIENT_CREATED_EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateClientCommand {
    tenant_id: TenantId,
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
}

impl CreateClientCommand {
    #[must_use]
    pub fn new(
        tenant_id: TenantId,
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            client_id,
            kind,
            display_name: display_name.into(),
        }
    }
}

pub fn decide_create_client(
    actor: &ActorContext,
    command: CreateClientCommand,
) -> Result<ClientRecord, ApplicationError> {
    if actor.tenant_scope().tenant_id() != &command.tenant_id {
        return Err(ApplicationError::new(ProblemCode::Forbidden));
    }

    ClientRecord::create(
        command.tenant_id,
        command.client_id,
        command.kind,
        command.display_name,
    )
    .map_err(map_client_error)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteCreateClientCommand {
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
    evidence: CommandExecutionEvidence,
}

impl ExecuteCreateClientCommand {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            client_id,
            kind,
            display_name: display_name.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientMutationOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl ClientMutationOutcome {
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
pub struct ClientDetails {
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
    status: ClientStatus,
    version: AggregateVersion,
}

impl ClientDetails {
    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn kind(&self) -> ClientKind {
        self.kind
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn status(&self) -> ClientStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

impl From<ClientReadModel> for ClientDetails {
    fn from(value: ClientReadModel) -> Self {
        Self {
            client_id: value.client_id().clone(),
            kind: value.kind(),
            display_name: value.display_name().to_owned(),
            status: value.status(),
            version: value.version(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientOperationError {
    NotFound,
    InvalidRequest,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for ClientOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "client not found",
            Self::InvalidRequest => "client request is invalid",
            Self::Conflict => "client command conflict",
            Self::IntegrityFailure => "client data integrity failure",
            Self::InternalFailure => "client application internal failure",
            Self::DependencyUnavailable => "client dependency unavailable",
        })
    }
}

impl std::error::Error for ClientOperationError {}

pub fn authorize_client_create(role: MembershipRole) -> Result<(), ClientOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ClientOperationError::NotFound)
    }
}

pub async fn execute_create_client<P: ClientApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteCreateClientCommand,
) -> Result<ClientMutationOutcome, ClientOperationError> {
    authorize_client_create(role)?;

    let client = ClientRecord::create(
        actor.tenant_scope().tenant_id().clone(),
        command.client_id,
        command.kind,
        command.display_name,
    )
    .map_err(map_client_operation_error)?;

    match port
        .decide_replay(actor, CLIENT_CREATE_COMMAND, &command.evidence)
        .await
        .map_err(map_client_port_error)?
    {
        ClientReplayDecision::Miss => {}
        ClientReplayDecision::Replay(receipt) => {
            return Ok(replay_outcome(&client, &receipt));
        }
        ClientReplayDecision::Conflict => return Err(ClientOperationError::Conflict),
    }

    let write = ClientCreateWrite::new(
        client,
        command.evidence,
        CLIENT_CREATED_EVENT_PAYLOAD,
    );
    match port.create_client(actor, &write).await {
        Ok(()) => Ok(ClientMutationOutcome {
            result_code: "created".to_owned(),
            resource_id: write.client().client_id().as_str().to_owned(),
            aggregate_version: AggregateVersion::INITIAL,
            replayed: false,
        }),
        Err(error) if error.class() == ClientPortErrorClass::Conflict => {
            match port
                .decide_replay(actor, CLIENT_CREATE_COMMAND, write.evidence())
                .await
                .map_err(map_client_port_error)?
            {
                ClientReplayDecision::Replay(receipt) => {
                    Ok(replay_outcome(write.client(), &receipt))
                }
                ClientReplayDecision::Miss | ClientReplayDecision::Conflict => {
                    Err(ClientOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_client_port_error(error)),
    }
}

pub async fn get_visible_client<P: ClientApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    client_id: &ClientId,
) -> Result<ClientDetails, ClientOperationError> {
    port.find_visible_client(
        actor.tenant_scope(),
        actor.actor_id(),
        role,
        client_id,
    )
    .await
    .map_err(map_client_port_error)?
    .map(ClientDetails::from)
    .ok_or(ClientOperationError::NotFound)
}

fn replay_outcome(client: &ClientRecord, receipt: &ClientReplayReceipt) -> ClientMutationOutcome {
    ClientMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(client.client_id().as_str())
            .to_owned(),
        aggregate_version: AggregateVersion::INITIAL,
        replayed: true,
    }
}

fn map_client_operation_error(error: ClientError) -> ClientOperationError {
    match error {
        ClientError::InvalidDisplayName => ClientOperationError::InvalidRequest,
        ClientError::InvalidStatusTransition | ClientError::VersionOverflow => {
            ClientOperationError::InternalFailure
        }
    }
}

fn map_client_port_error(error: ClientPortError) -> ClientOperationError {
    match error.class() {
        ClientPortErrorClass::Conflict => ClientOperationError::Conflict,
        ClientPortErrorClass::IntegrityFailure => ClientOperationError::IntegrityFailure,
        ClientPortErrorClass::InternalFailure => ClientOperationError::InternalFailure,
        ClientPortErrorClass::DependencyUnavailable => ClientOperationError::DependencyUnavailable,
    }
}

fn map_client_error(error: ClientError) -> ApplicationError {
    match error {
        ClientError::InvalidDisplayName => ApplicationError::new(ProblemCode::InvalidRequest),
        ClientError::InvalidStatusTransition => ApplicationError::new(ProblemCode::InvalidState),
        ClientError::VersionOverflow => ApplicationError::new(ProblemCode::InternalFailure),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClientOperationError, ExecuteCreateClientCommand, authorize_client_create,
        execute_create_client, get_visible_client,
    };
    use application_ports::CommandExecutionEvidence;
    use application_ports::clients::{
        ClientApplicationPort, ClientCreateWrite, ClientPortError, ClientPortErrorClass,
        ClientReadModel, ClientReplayDecision, ClientReplayReceipt,
    };
    use client_domain::{ClientKind, ClientStatus};
    use identity_access_domain::MembershipRole;
    use profile_platform_primitives::{
        ActorContext, ActorId, AggregateVersion, AuditEventId, ClientId, CorrelationId,
        IdempotencyKey, OutboxEventId, TenantId, TenantScope, UnixMillis,
    };
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct FakeClientPort {
        replay: RefCell<Vec<ClientReplayDecision>>,
        create_error: Cell<Option<ClientPortErrorClass>>,
        create_calls: Cell<u32>,
        visible: RefCell<Option<ClientReadModel>>,
        seen_display_name: RefCell<Option<String>>,
    }

    impl FakeClientPort {
        fn new(replay: Vec<ClientReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                create_error: Cell::new(None),
                create_calls: Cell::new(0),
                visible: RefCell::new(None),
                seen_display_name: RefCell::new(None),
            }
        }

        fn next_replay(&self) -> ClientReplayDecision {
            let mut replay = self.replay.borrow_mut();
            if replay.is_empty() {
                ClientReplayDecision::Miss
            } else {
                replay.remove(0)
            }
        }
    }

    impl ClientApplicationPort for FakeClientPort {
        async fn decide_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ClientReplayDecision, ClientPortError> {
            Ok(self.next_replay())
        }

        async fn create_client(
            &self,
            _actor: &ActorContext,
            write: &ClientCreateWrite,
        ) -> Result<(), ClientPortError> {
            self.create_calls.set(self.create_calls.get() + 1);
            self.seen_display_name
                .replace(Some(write.client().display_name().to_owned()));
            match self.create_error.get() {
                Some(class) => Err(ClientPortError::new(class)),
                None => Ok(()),
            }
        }

        async fn find_visible_client(
            &self,
            _scope: &TenantScope,
            _actor_id: &ActorId,
            _role: MembershipRole,
            _client_id: &ClientId,
        ) -> Result<Option<ClientReadModel>, ClientPortError> {
            Ok(self.visible.borrow().clone())
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JCLIENTAPP")?),
            ActorId::parse("actor_01JCLIENTAPP")?,
            CorrelationId::parse("corr_01JCLIENTAPP")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JCLIENTAPP")?,
            "digest_01JCLIENTAPP",
            AuditEventId::parse("audit_01JCLIENTAPP")?,
            OutboxEventId::parse("outbox_01JCLIENTAPP")?,
            UnixMillis::new(10),
            UnixMillis::new(20),
        ))
    }

    fn command() -> Result<ExecuteCreateClientCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteCreateClientCommand::new(
            ClientId::parse("client_01JCLIENTAPP")?,
            ClientKind::Person,
            "  Synthetic Client  ",
            evidence()?,
        ))
    }

    #[test]
    fn create_authorization_is_disclosure_neutral() {
        assert_eq!(authorize_client_create(MembershipRole::TenantOwner), Ok(()));
        assert_eq!(
            authorize_client_create(MembershipRole::Member),
            Err(ClientOperationError::NotFound)
        );
    }

    #[test]
    fn owner_create_normalizes_domain_input_before_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeClientPort::new(vec![ClientReplayDecision::Miss]);
        let outcome = block_on(execute_create_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command()?,
        ))?;
        assert_eq!(outcome.result_code(), "created");
        assert!(!outcome.replayed());
        assert_eq!(port.create_calls.get(), 1);
        assert_eq!(
            port.seen_display_name.borrow().as_deref(),
            Some("Synthetic Client")
        );
        Ok(())
    }

    #[test]
    fn member_create_is_disclosure_neutral_and_never_writes()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeClientPort::new(vec![ClientReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_create_client(
                &actor()?,
                MembershipRole::Member,
                &port,
                command()?,
            )),
            Err(ClientOperationError::NotFound)
        );
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_replay_skips_write_and_preserves_receipt()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeClientPort::new(vec![ClientReplayDecision::Replay(
            ClientReplayReceipt::new("created", Some("client_existing".to_owned())),
        )]);
        let outcome = block_on(execute_create_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "client_existing");
        assert_eq!(port.create_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn concurrent_unique_conflict_replays_only_after_exact_recheck()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeClientPort::new(vec![
            ClientReplayDecision::Miss,
            ClientReplayDecision::Replay(ClientReplayReceipt::new(
                "created",
                Some("client_01JCLIENTAPP".to_owned()),
            )),
        ]);
        port.create_error.set(Some(ClientPortErrorClass::Conflict));
        let outcome = block_on(execute_create_client(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command()?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.create_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn unique_conflict_without_exact_replay_remains_conflict()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeClientPort::new(vec![
            ClientReplayDecision::Miss,
            ClientReplayDecision::Miss,
        ]);
        port.create_error.set(Some(ClientPortErrorClass::Conflict));
        assert_eq!(
            block_on(execute_create_client(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command()?,
            )),
            Err(ClientOperationError::Conflict)
        );
        Ok(())
    }

    #[test]
    fn visible_query_returns_typed_application_view()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakeClientPort::new(Vec::new());
        port.visible.replace(Some(ClientReadModel::new(
            ClientId::parse("client_01JCLIENTAPP")?,
            ClientKind::Organization,
            "Visible Client",
            ClientStatus::Active,
            AggregateVersion::INITIAL,
        )));
        let client_id = ClientId::parse("client_01JCLIENTAPP")?;
        let details = block_on(get_visible_client(
            &actor()?,
            MembershipRole::Member,
            &port,
            &client_id,
        ))?;
        assert_eq!(details.client_id(), &client_id);
        assert_eq!(details.kind(), ClientKind::Organization);
        assert_eq!(details.status(), ClientStatus::Active);
        assert_eq!(details.version(), AggregateVersion::INITIAL);
        Ok(())
    }
}
