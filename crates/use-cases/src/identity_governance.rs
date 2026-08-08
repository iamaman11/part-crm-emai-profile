use application_ports::CommandExecutionEvidence;
use application_ports::identity_governance::{
    ActiveOwnerGovernanceApplicationPort, IdentityGovernancePortError,
    IdentityGovernancePortErrorClass, IdentityReplayDecision, IdentityReplayReceipt,
    InvitationCreateWrite, MembershipStatusTarget, MembershipStatusWrite, OwnerTransferWrite,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, InvitationId, UnixMillis,
};

const OWNER_TRANSFER_COMMAND: &str = "membership.owner_transfer";
const INVITATION_CREATE_COMMAND: &str = "invitation.create";
const EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityGovernanceOperationError {
    InvalidRequest,
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for IdentityGovernanceOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "identity governance request is invalid",
            Self::NotFound => "identity governance resource not found",
            Self::VersionConflict => "identity governance version conflict",
            Self::InvalidState => "identity governance invalid state",
            Self::Conflict => "identity governance conflict",
            Self::IntegrityFailure => "identity governance integrity failure",
            Self::InternalFailure => "identity governance internal failure",
            Self::DependencyUnavailable => "identity governance dependency unavailable",
        })
    }
}

impl std::error::Error for IdentityGovernanceOperationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityMutationOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl IdentityMutationOutcome {
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
pub struct ExecuteOwnerTransferCommand {
    next_owner_actor_id: ActorId,
    current_owner_version: AggregateVersion,
    next_owner_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
}

impl ExecuteOwnerTransferCommand {
    #[must_use]
    pub fn new(
        next_owner_actor_id: ActorId,
        current_owner_version: AggregateVersion,
        next_owner_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            next_owner_actor_id,
            current_owner_version,
            next_owner_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteInvitationCreateCommand {
    invitation_id: InvitationId,
    invited_contact_hmac: String,
    expires_at: UnixMillis,
    tenant_expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
}

impl ExecuteInvitationCreateCommand {
    #[must_use]
    pub fn new(
        invitation_id: InvitationId,
        invited_contact_hmac: impl Into<String>,
        expires_at: UnixMillis,
        tenant_expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            invitation_id,
            invited_contact_hmac: invited_contact_hmac.into(),
            expires_at,
            tenant_expected_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteMembershipStatusCommand {
    target_actor_id: ActorId,
    expected_version: AggregateVersion,
    status: String,
    evidence: CommandExecutionEvidence,
}

impl ExecuteMembershipStatusCommand {
    #[must_use]
    pub fn new(
        target_actor_id: ActorId,
        expected_version: AggregateVersion,
        status: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            target_actor_id,
            expected_version,
            status: status.into(),
            evidence,
        }
    }
}

pub fn authorize_identity_governance(
    role: MembershipRole,
) -> Result<(), IdentityGovernanceOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(IdentityGovernanceOperationError::NotFound)
    }
}

pub fn next_identity_version(
    version: AggregateVersion,
) -> Result<AggregateVersion, IdentityGovernanceOperationError> {
    version
        .next()
        .map_err(|_| IdentityGovernanceOperationError::InternalFailure)
}

pub fn parse_membership_status(
    value: &str,
) -> Result<(MembershipStatusTarget, &'static str), IdentityGovernanceOperationError> {
    match value {
        "ACTIVE" => Ok((MembershipStatusTarget::Active, "membership.activate")),
        "SUSPENDED" => Ok((MembershipStatusTarget::Suspended, "membership.suspend")),
        "REVOKED" => Ok((MembershipStatusTarget::Revoked, "membership.revoke")),
        _ => Err(IdentityGovernanceOperationError::InvalidRequest),
    }
}

pub async fn execute_owner_transfer<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteOwnerTransferCommand,
) -> Result<IdentityMutationOutcome, IdentityGovernanceOperationError> {
    authorize_identity_governance(role)?;
    let response_version = next_identity_version(command.next_owner_version)?;
    let resource_id = command.next_owner_actor_id.as_str().to_owned();

    if let Some(outcome) = prewrite_replay(
        actor,
        port,
        OWNER_TRANSFER_COMMAND,
        &command.evidence,
        &resource_id,
        response_version,
    )
    .await?
    {
        return Ok(outcome);
    }

    let write = OwnerTransferWrite::new(
        command.next_owner_actor_id,
        command.current_owner_version,
        command.next_owner_version,
        command.evidence,
        EVENT_PAYLOAD,
    );
    match port.transfer_owner(actor, &write).await {
        Ok(()) => Ok(fresh_outcome("transferred", resource_id, response_version)),
        Err(error) if error.class() == IdentityGovernancePortErrorClass::Conflict => {
            conflict_replay(
                actor,
                port,
                OWNER_TRANSFER_COMMAND,
                write.evidence(),
                write.next_owner_actor_id().as_str(),
                response_version,
            )
            .await
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn execute_invitation_create<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteInvitationCreateCommand,
) -> Result<IdentityMutationOutcome, IdentityGovernanceOperationError> {
    authorize_identity_governance(role)?;
    let response_version = next_identity_version(command.tenant_expected_version)?;
    let resource_id = command.invitation_id.as_str().to_owned();

    if let Some(outcome) = prewrite_replay(
        actor,
        port,
        INVITATION_CREATE_COMMAND,
        &command.evidence,
        &resource_id,
        response_version,
    )
    .await?
    {
        return Ok(outcome);
    }

    let write = InvitationCreateWrite::new(
        command.invitation_id,
        command.invited_contact_hmac,
        command.expires_at,
        command.tenant_expected_version,
        command.evidence,
        EVENT_PAYLOAD,
    );
    match port.create_invitation(actor, &write).await {
        Ok(()) => Ok(fresh_outcome("created", resource_id, response_version)),
        Err(error) if error.class() == IdentityGovernancePortErrorClass::Conflict => {
            conflict_replay(
                actor,
                port,
                INVITATION_CREATE_COMMAND,
                write.evidence(),
                write.invitation_id().as_str(),
                response_version,
            )
            .await
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn execute_membership_status<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteMembershipStatusCommand,
) -> Result<IdentityMutationOutcome, IdentityGovernanceOperationError> {
    authorize_identity_governance(role)?;
    let response_version = next_identity_version(command.expected_version)?;
    let (next_status, command_name) = parse_membership_status(&command.status)?;
    let resource_id = command.target_actor_id.as_str().to_owned();

    if let Some(outcome) = prewrite_replay(
        actor,
        port,
        command_name,
        &command.evidence,
        &resource_id,
        response_version,
    )
    .await?
    {
        return Ok(outcome);
    }

    let write = MembershipStatusWrite::new(
        command.target_actor_id,
        command.expected_version,
        next_status,
        command.evidence,
        EVENT_PAYLOAD,
    );
    match port.update_membership_status(actor, &write).await {
        Ok(()) => Ok(fresh_outcome("updated", resource_id, response_version)),
        Err(error) if error.class() == IdentityGovernancePortErrorClass::Conflict => {
            conflict_replay(
                actor,
                port,
                command_name,
                write.evidence(),
                write.target_actor_id().as_str(),
                response_version,
            )
            .await
        }
        Err(error) => Err(map_port_error(error)),
    }
}

async fn prewrite_replay<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command_name: &str,
    evidence: &CommandExecutionEvidence,
    resource_id: &str,
    version: AggregateVersion,
) -> Result<Option<IdentityMutationOutcome>, IdentityGovernanceOperationError> {
    match port
        .decide_identity_replay(actor, command_name, evidence)
        .await
        .map_err(map_port_error)?
    {
        IdentityReplayDecision::Miss => Ok(None),
        IdentityReplayDecision::Replay(receipt) => {
            Ok(Some(replay_outcome(resource_id, version, &receipt)))
        }
        IdentityReplayDecision::Conflict => Err(IdentityGovernanceOperationError::Conflict),
    }
}

async fn conflict_replay<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command_name: &str,
    evidence: &CommandExecutionEvidence,
    resource_id: &str,
    version: AggregateVersion,
) -> Result<IdentityMutationOutcome, IdentityGovernanceOperationError> {
    match port
        .decide_identity_replay(actor, command_name, evidence)
        .await
        .map_err(map_port_error)?
    {
        IdentityReplayDecision::Replay(receipt) => Ok(replay_outcome(resource_id, version, &receipt)),
        IdentityReplayDecision::Miss | IdentityReplayDecision::Conflict => {
            Err(IdentityGovernanceOperationError::Conflict)
        }
    }
}

fn fresh_outcome(
    result_code: &str,
    resource_id: String,
    aggregate_version: AggregateVersion,
) -> IdentityMutationOutcome {
    IdentityMutationOutcome {
        result_code: result_code.to_owned(),
        resource_id,
        aggregate_version,
        replayed: false,
    }
}

fn replay_outcome(
    resource_id: &str,
    aggregate_version: AggregateVersion,
    receipt: &IdentityReplayReceipt,
) -> IdentityMutationOutcome {
    IdentityMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt.result_reference().unwrap_or(resource_id).to_owned(),
        aggregate_version,
        replayed: true,
    }
}

fn map_port_error(error: IdentityGovernancePortError) -> IdentityGovernanceOperationError {
    match error.class() {
        IdentityGovernancePortErrorClass::NotFound => IdentityGovernanceOperationError::NotFound,
        IdentityGovernancePortErrorClass::VersionConflict => {
            IdentityGovernanceOperationError::VersionConflict
        }
        IdentityGovernancePortErrorClass::InvalidState => {
            IdentityGovernanceOperationError::InvalidState
        }
        IdentityGovernancePortErrorClass::Conflict => IdentityGovernanceOperationError::Conflict,
        IdentityGovernancePortErrorClass::IntegrityFailure => {
            IdentityGovernanceOperationError::IntegrityFailure
        }
        IdentityGovernancePortErrorClass::InternalFailure => {
            IdentityGovernanceOperationError::InternalFailure
        }
        IdentityGovernancePortErrorClass::DependencyUnavailable => {
            IdentityGovernanceOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use profile_platform_primitives::{
        AuditEventId, CorrelationId, IdempotencyKey, OutboxEventId, TenantId, TenantScope,
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
        replay: RefCell<Vec<IdentityReplayDecision>>,
        commands: RefCell<Vec<String>>,
        replay_calls: Cell<u32>,
        transfer_calls: Cell<u32>,
        invitation_calls: Cell<u32>,
        membership_calls: Cell<u32>,
        write_error: Cell<Option<IdentityGovernancePortErrorClass>>,
    }

    impl FakePort {
        fn new(replay: Vec<IdentityReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                commands: RefCell::new(Vec::new()),
                replay_calls: Cell::new(0),
                transfer_calls: Cell::new(0),
                invitation_calls: Cell::new(0),
                membership_calls: Cell::new(0),
                write_error: Cell::new(None),
            }
        }

        fn write_result(&self) -> Result<(), IdentityGovernancePortError> {
            match self.write_error.get() {
                Some(class) => Err(IdentityGovernancePortError::new(class)),
                None => Ok(()),
            }
        }
    }

    impl ActiveOwnerGovernanceApplicationPort for FakePort {
        async fn decide_identity_replay(
            &self,
            _actor: &ActorContext,
            command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<IdentityReplayDecision, IdentityGovernancePortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            self.commands.borrow_mut().push(command_name.to_owned());
            Ok(if self.replay.borrow().is_empty() {
                IdentityReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            })
        }

        async fn transfer_owner(
            &self,
            _actor: &ActorContext,
            _write: &OwnerTransferWrite,
        ) -> Result<(), IdentityGovernancePortError> {
            self.transfer_calls.set(self.transfer_calls.get() + 1);
            self.write_result()
        }

        async fn create_invitation(
            &self,
            _actor: &ActorContext,
            _write: &InvitationCreateWrite,
        ) -> Result<(), IdentityGovernancePortError> {
            self.invitation_calls.set(self.invitation_calls.get() + 1);
            self.write_result()
        }

        async fn update_membership_status(
            &self,
            _actor: &ActorContext,
            _write: &MembershipStatusWrite,
        ) -> Result<(), IdentityGovernancePortError> {
            self.membership_calls.set(self.membership_calls.get() + 1);
            self.write_result()
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JIDENTITYGOV")?),
            ActorId::parse("actor_01JIDENTITYGOV")?,
            CorrelationId::parse("corr_01JIDENTITYGOV")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JIDENTITYGOV")?,
            "request-digest-01JIDENTITYGOV",
            AuditEventId::parse("audit_01JIDENTITYGOV")?,
            OutboxEventId::parse("outbox_01JIDENTITYGOV")?,
            UnixMillis::new(10),
            UnixMillis::new(100),
        ))
    }

    #[test]
    fn non_owner_stops_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        let command = ExecuteMembershipStatusCommand::new(
            ActorId::parse("actor_01JTARGET")?,
            AggregateVersion::INITIAL,
            "ACTIVE",
            evidence()?,
        );
        assert_eq!(
            block_on(execute_membership_status(
                &actor()?,
                MembershipRole::Member,
                &port,
                command,
            )),
            Err(IdentityGovernanceOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.membership_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn invalid_membership_status_stops_before_replay_or_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        let command = ExecuteMembershipStatusCommand::new(
            ActorId::parse("actor_01JTARGET")?,
            AggregateVersion::INITIAL,
            "UNKNOWN",
            evidence()?,
        );
        assert_eq!(
            block_on(execute_membership_status(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command,
            )),
            Err(IdentityGovernanceOperationError::InvalidRequest)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.membership_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn membership_commands_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
        let actor = actor()?;
        for (status, expected) in [
            ("ACTIVE", "membership.activate"),
            ("SUSPENDED", "membership.suspend"),
            ("REVOKED", "membership.revoke"),
        ] {
            let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
            block_on(execute_membership_status(
                &actor,
                MembershipRole::TenantOwner,
                &port,
                ExecuteMembershipStatusCommand::new(
                    ActorId::parse("actor_01JTARGET")?,
                    AggregateVersion::INITIAL,
                    status,
                    evidence()?,
                ),
            ))?;
            assert_eq!(port.commands.borrow().as_slice(), [expected]);
        }
        Ok(())
    }

    #[test]
    fn conflict_rechecks_replay_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![
            IdentityReplayDecision::Miss,
            IdentityReplayDecision::Replay(IdentityReplayReceipt::new("transferred", None)),
        ]);
        port.write_error
            .set(Some(IdentityGovernancePortErrorClass::Conflict));
        let outcome = block_on(execute_owner_transfer(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ExecuteOwnerTransferCommand::new(
                ActorId::parse("actor_01JNEXTOWNER")?,
                AggregateVersion::INITIAL,
                AggregateVersion::INITIAL,
                evidence()?,
            ),
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.transfer_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn non_conflict_failure_does_not_recheck() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        port.write_error
            .set(Some(IdentityGovernancePortErrorClass::VersionConflict));
        let result = block_on(execute_invitation_create(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ExecuteInvitationCreateCommand::new(
                InvitationId::parse("invitation_01JIDENTITYGOV")?,
                "hmac-token",
                UnixMillis::new(50),
                AggregateVersion::INITIAL,
                evidence()?,
            ),
        ));
        assert_eq!(result, Err(IdentityGovernanceOperationError::VersionConflict));
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.invitation_calls.get(), 1);
        Ok(())
    }
}
