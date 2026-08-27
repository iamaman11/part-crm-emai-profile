use application_ports::CommandExecutionEvidence;
use application_ports::profile_assignment_context::{
    ProfileAssignmentContext, ProfileAssignmentContextPort, ProfileDetachmentContext,
};
use application_ports::profiles::{
    ProfileAssignmentApplicationPort, ProfileAssignmentPortError, ProfileAssignmentPortErrorClass,
    ProfileAssignmentWrite, ProfileDetachmentWrite, ProfileReplayDecision, ProfileReplayReceipt,
};
use client_domain::{
    AssignmentError, PrimaryReassignmentIntent, ProfileClientAssignment, plan_primary_detachment,
    plan_primary_reassignment,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, AggregateVersion, AssignmentId, ClientId, ProfileId,
};

const PROFILE_ASSIGN_COMMAND: &str = "profile.assign_client";
const PROFILE_DETACH_COMMAND: &str = "profile.detach_client";
const EVENT_PAYLOAD: &str = "{}";
const MAX_RELATIONSHIP_REASON_LENGTH: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteAssignProfileCommand {
    assignment_id: AssignmentId,
    profile_id: ProfileId,
    client_id: ClientId,
    expected_profile_version: AggregateVersion,
    reason: String,
    evidence: CommandExecutionEvidence,
}

impl ExecuteAssignProfileCommand {
    #[must_use]
    pub fn new(
        assignment_id: AssignmentId,
        profile_id: ProfileId,
        client_id: ClientId,
        expected_profile_version: AggregateVersion,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            assignment_id,
            profile_id,
            client_id,
            expected_profile_version,
            reason: reason.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteDetachProfileCommand {
    profile_id: ProfileId,
    expected_profile_version: AggregateVersion,
    reason: String,
    evidence: CommandExecutionEvidence,
}

impl ExecuteDetachProfileCommand {
    #[must_use]
    pub fn new(
        profile_id: ProfileId,
        expected_profile_version: AggregateVersion,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            profile_id,
            expected_profile_version,
            reason: reason.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileAssignmentOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl ProfileAssignmentOutcome {
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
pub enum ProfileAssignmentOperationError {
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for ProfileAssignmentOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "profile assignment not found",
            Self::VersionConflict => "profile assignment version conflict",
            Self::InvalidState => "profile assignment invalid state",
            Self::Conflict => "profile assignment conflict",
            Self::IntegrityFailure => "profile assignment integrity failure",
            Self::InternalFailure => "profile assignment internal failure",
            Self::DependencyUnavailable => "profile assignment dependency unavailable",
        })
    }
}

impl std::error::Error for ProfileAssignmentOperationError {}

pub fn authorize_profile_assignment(
    role: MembershipRole,
) -> Result<(), ProfileAssignmentOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ProfileAssignmentOperationError::NotFound)
    }
}

pub fn next_profile_assignment_version(
    version: AggregateVersion,
) -> Result<AggregateVersion, ProfileAssignmentOperationError> {
    version
        .next()
        .map_err(|_| ProfileAssignmentOperationError::InternalFailure)
}

pub async fn execute_assign_profile<P>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteAssignProfileCommand,
) -> Result<ProfileAssignmentOutcome, ProfileAssignmentOperationError>
where
    P: ProfileAssignmentApplicationPort + ProfileAssignmentContextPort,
{
    authorize_profile_assignment(role)?;

    match port
        .decide_assignment_replay(actor, PROFILE_ASSIGN_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        ProfileReplayDecision::Miss => {}
        ProfileReplayDecision::Replay(receipt) => {
            return Ok(replay_outcome(
                &command.assignment_id,
                next_profile_assignment_version(command.expected_profile_version)?,
                &receipt,
            ));
        }
        ProfileReplayDecision::Conflict => return Err(ProfileAssignmentOperationError::Conflict),
    }

    let context = port
        .load_profile_assignment_context(
            actor.tenant_scope(),
            &command.profile_id,
            &command.client_id,
        )
        .await
        .map_err(map_port_error)?
        .ok_or(ProfileAssignmentOperationError::NotFound)?;

    if context.profile_version() != command.expected_profile_version {
        return Err(ProfileAssignmentOperationError::VersionConflict);
    }

    let current = restore_current_assignment(actor, &command.profile_id, &context)?;
    let transition = plan_primary_reassignment(
        actor.tenant_scope().tenant_id(),
        &command.profile_id,
        current.as_ref(),
        context.target_client(),
        PrimaryReassignmentIntent::new(
            command.assignment_id,
            actor.actor_id().clone(),
            command.evidence.now(),
            command.reason,
        ),
    )
    .map_err(map_transition_error)?;
    let next = transition.next();
    let next_version = next_profile_assignment_version(context.profile_version())?;
    let write = ProfileAssignmentWrite::new(
        next.assignment_id().clone(),
        next.profile_id().clone(),
        next.client_id().clone(),
        context.profile_version(),
        next.reason(),
        command.evidence,
        EVENT_PAYLOAD,
    );

    match port.assign_profile(actor, &write).await {
        Ok(()) => Ok(ProfileAssignmentOutcome {
            result_code: "assigned".to_owned(),
            resource_id: write.assignment_id().as_str().to_owned(),
            aggregate_version: next_version,
            replayed: false,
        }),
        Err(error) if error.class() == ProfileAssignmentPortErrorClass::Conflict => {
            match port
                .decide_assignment_replay(actor, PROFILE_ASSIGN_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                ProfileReplayDecision::Replay(receipt) => Ok(replay_outcome(
                    write.assignment_id(),
                    next_version,
                    &receipt,
                )),
                ProfileReplayDecision::Miss | ProfileReplayDecision::Conflict => {
                    Err(ProfileAssignmentOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn execute_detach_profile<P>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteDetachProfileCommand,
) -> Result<ProfileAssignmentOutcome, ProfileAssignmentOperationError>
where
    P: ProfileAssignmentApplicationPort + ProfileAssignmentContextPort,
{
    authorize_profile_assignment(role)?;
    let reason = normalize_relationship_reason(command.reason)?;
    let next_version = next_profile_assignment_version(command.expected_profile_version)?;

    match port
        .decide_assignment_replay(actor, PROFILE_DETACH_COMMAND, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        ProfileReplayDecision::Miss => {}
        ProfileReplayDecision::Replay(receipt) => {
            return replay_detachment_outcome(next_version, &receipt);
        }
        ProfileReplayDecision::Conflict => return Err(ProfileAssignmentOperationError::Conflict),
    }

    let context = port
        .load_profile_detachment_context(actor.tenant_scope(), &command.profile_id)
        .await
        .map_err(map_port_error)?
        .ok_or(ProfileAssignmentOperationError::NotFound)?;

    if context.profile_version() != command.expected_profile_version {
        return Err(ProfileAssignmentOperationError::VersionConflict);
    }

    let current_snapshot = context
        .current()
        .ok_or(ProfileAssignmentOperationError::InvalidState)?;
    let current = restore_current_snapshot(actor, &command.profile_id, current_snapshot)?;
    let transition = plan_primary_detachment(
        actor.tenant_scope().tenant_id(),
        &command.profile_id,
        &current,
        command.evidence.now(),
    )
    .map_err(map_transition_error)?;
    let closed = transition.closed();
    let write = ProfileDetachmentWrite::new(
        closed.assignment_id().clone(),
        closed.profile_id().clone(),
        closed.client_id().clone(),
        context.profile_version(),
        reason,
        command.evidence,
        EVENT_PAYLOAD,
    );

    match port.detach_profile(actor, &write).await {
        Ok(()) => Ok(ProfileAssignmentOutcome {
            result_code: "detached".to_owned(),
            resource_id: write.assignment_id().as_str().to_owned(),
            aggregate_version: next_version,
            replayed: false,
        }),
        Err(error) if error.class() == ProfileAssignmentPortErrorClass::Conflict => {
            match port
                .decide_assignment_replay(actor, PROFILE_DETACH_COMMAND, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                ProfileReplayDecision::Replay(receipt) => {
                    replay_detachment_outcome(next_version, &receipt)
                }
                ProfileReplayDecision::Miss | ProfileReplayDecision::Conflict => {
                    Err(ProfileAssignmentOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn restore_current_assignment(
    actor: &ActorContext,
    profile_id: &ProfileId,
    context: &ProfileAssignmentContext,
) -> Result<Option<ProfileClientAssignment>, ProfileAssignmentOperationError> {
    context
        .current()
        .map(|current| restore_current_snapshot(actor, profile_id, current))
        .transpose()
}

fn restore_current_snapshot(
    actor: &ActorContext,
    profile_id: &ProfileId,
    current: &application_ports::profile_assignment_context::CurrentProfileAssignmentSnapshot,
) -> Result<ProfileClientAssignment, ProfileAssignmentOperationError> {
    ProfileClientAssignment::assign(
        actor.tenant_scope().tenant_id(),
        current.assignment_id().clone(),
        profile_id.clone(),
        current.client(),
        current.assigned_by().clone(),
        current.assigned_at(),
        current.reason(),
    )
    .map_err(|_| ProfileAssignmentOperationError::IntegrityFailure)
}

fn normalize_relationship_reason(
    reason: String,
) -> Result<String, ProfileAssignmentOperationError> {
    let reason = reason.trim();
    if reason.is_empty() || reason.len() > MAX_RELATIONSHIP_REASON_LENGTH {
        return Err(ProfileAssignmentOperationError::InvalidState);
    }
    Ok(reason.to_owned())
}

fn map_transition_error(error: AssignmentError) -> ProfileAssignmentOperationError {
    match error {
        AssignmentError::TenantMismatch | AssignmentError::ClientNotActive => {
            ProfileAssignmentOperationError::NotFound
        }
        AssignmentError::InvalidReason | AssignmentError::InvalidCloseTime => {
            ProfileAssignmentOperationError::InvalidState
        }
        AssignmentError::AlreadyPrimaryClient => ProfileAssignmentOperationError::Conflict,
        AssignmentError::AlreadyClosed
        | AssignmentError::CurrentScopeMismatch
        | AssignmentError::CurrentNotActivePrimary => {
            ProfileAssignmentOperationError::IntegrityFailure
        }
    }
}

fn replay_outcome(
    assignment_id: &AssignmentId,
    version: AggregateVersion,
    receipt: &ProfileReplayReceipt,
) -> ProfileAssignmentOutcome {
    ProfileAssignmentOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(assignment_id.as_str())
            .to_owned(),
        aggregate_version: version,
        replayed: true,
    }
}

fn replay_detachment_outcome(
    version: AggregateVersion,
    receipt: &ProfileReplayReceipt,
) -> Result<ProfileAssignmentOutcome, ProfileAssignmentOperationError> {
    let resource_id = receipt
        .result_reference()
        .ok_or(ProfileAssignmentOperationError::IntegrityFailure)?;
    if receipt.result_code() != "detached" {
        return Err(ProfileAssignmentOperationError::IntegrityFailure);
    }
    Ok(ProfileAssignmentOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: resource_id.to_owned(),
        aggregate_version: version,
        replayed: true,
    })
}

fn map_port_error(error: ProfileAssignmentPortError) -> ProfileAssignmentOperationError {
    match error.class() {
        ProfileAssignmentPortErrorClass::NotFound => ProfileAssignmentOperationError::NotFound,
        ProfileAssignmentPortErrorClass::VersionConflict => {
            ProfileAssignmentOperationError::VersionConflict
        }
        ProfileAssignmentPortErrorClass::InvalidState => {
            ProfileAssignmentOperationError::InvalidState
        }
        ProfileAssignmentPortErrorClass::Conflict => ProfileAssignmentOperationError::Conflict,
        ProfileAssignmentPortErrorClass::IntegrityFailure => {
            ProfileAssignmentOperationError::IntegrityFailure
        }
        ProfileAssignmentPortErrorClass::InternalFailure => {
            ProfileAssignmentOperationError::InternalFailure
        }
        ProfileAssignmentPortErrorClass::DependencyUnavailable => {
            ProfileAssignmentOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application_ports::profile_assignment_context::CurrentProfileAssignmentSnapshot;
    use client_domain::{ClientKind, ClientRecord, ClientStatus};
    use profile_platform_primitives::{
        ActorId, AuditEventId, CorrelationId, IdempotencyKey, OutboxEventId, PayloadFingerprint,
        TenantId, TenantScope, UnixMillis,
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
        replay: RefCell<Vec<ProfileReplayDecision>>,
        context: RefCell<Option<ProfileAssignmentContext>>,
        detachment_context: RefCell<Option<ProfileDetachmentContext>>,
        replay_calls: Cell<u32>,
        context_calls: Cell<u32>,
        detachment_context_calls: Cell<u32>,
        write_calls: Cell<u32>,
        detach_write_calls: Cell<u32>,
        write_error: Cell<Option<ProfileAssignmentPortErrorClass>>,
        detach_write_error: Cell<Option<ProfileAssignmentPortErrorClass>>,
        context_error: Cell<Option<ProfileAssignmentPortErrorClass>>,
        detachment_context_error: Cell<Option<ProfileAssignmentPortErrorClass>>,
    }

    impl FakePort {
        fn new(
            replay: Vec<ProfileReplayDecision>,
            context: Option<ProfileAssignmentContext>,
        ) -> Self {
            Self {
                replay: RefCell::new(replay),
                context: RefCell::new(context),
                detachment_context: RefCell::new(None),
                replay_calls: Cell::new(0),
                context_calls: Cell::new(0),
                detachment_context_calls: Cell::new(0),
                write_calls: Cell::new(0),
                detach_write_calls: Cell::new(0),
                write_error: Cell::new(None),
                detach_write_error: Cell::new(None),
                context_error: Cell::new(None),
                detachment_context_error: Cell::new(None),
            }
        }
    }

    impl ProfileAssignmentApplicationPort for FakePort {
        async fn decide_assignment_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ProfileReplayDecision, ProfileAssignmentPortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            Ok(if self.replay.borrow().is_empty() {
                ProfileReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            })
        }

        async fn assign_profile(
            &self,
            _actor: &ActorContext,
            _write: &ProfileAssignmentWrite,
        ) -> Result<(), ProfileAssignmentPortError> {
            self.write_calls.set(self.write_calls.get() + 1);
            match self.write_error.get() {
                Some(class) => Err(ProfileAssignmentPortError::new(class)),
                None => Ok(()),
            }
        }

        async fn detach_profile(
            &self,
            _actor: &ActorContext,
            _write: &ProfileDetachmentWrite,
        ) -> Result<(), ProfileAssignmentPortError> {
            self.detach_write_calls
                .set(self.detach_write_calls.get() + 1);
            match self.detach_write_error.get() {
                Some(class) => Err(ProfileAssignmentPortError::new(class)),
                None => Ok(()),
            }
        }
    }

    impl ProfileAssignmentContextPort for FakePort {
        async fn load_profile_assignment_context(
            &self,
            _scope: &TenantScope,
            _profile_id: &ProfileId,
            _target_client_id: &ClientId,
        ) -> Result<Option<ProfileAssignmentContext>, ProfileAssignmentPortError> {
            self.context_calls.set(self.context_calls.get() + 1);
            match self.context_error.get() {
                Some(class) => Err(ProfileAssignmentPortError::new(class)),
                None => Ok(self.context.borrow().clone()),
            }
        }

        async fn load_profile_detachment_context(
            &self,
            _scope: &TenantScope,
            _profile_id: &ProfileId,
        ) -> Result<Option<ProfileDetachmentContext>, ProfileAssignmentPortError> {
            self.detachment_context_calls
                .set(self.detachment_context_calls.get() + 1);
            match self.detachment_context_error.get() {
                Some(class) => Err(ProfileAssignmentPortError::new(class)),
                None => Ok(self.detachment_context.borrow().clone()),
            }
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JASSIGNAPP")?),
            ActorId::parse("actor_01JASSIGNAPP")?,
            CorrelationId::parse("corr_01JASSIGNAPP")?,
        ))
    }

    fn evidence(now: u64) -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JASSIGNAPP")?,
            PayloadFingerprint::parse("a".repeat(64))?,
            AuditEventId::parse("audit_01JASSIGNAPP")?,
            OutboxEventId::parse("outbox_01JASSIGNAPP")?,
            UnixMillis::new(now),
            UnixMillis::new(100),
        ))
    }

    fn command(
        version: AggregateVersion,
        client_id: &str,
        now: u64,
    ) -> Result<ExecuteAssignProfileCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteAssignProfileCommand::new(
            AssignmentId::parse("assignment_02JASSIGNAPP")?,
            ProfileId::parse("profile_01JASSIGNAPP")?,
            ClientId::parse(client_id)?,
            version,
            "  operator requested association  ",
            evidence(now)?,
        ))
    }

    fn detach_command(
        version: AggregateVersion,
        now: u64,
        reason: &str,
    ) -> Result<ExecuteDetachProfileCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteDetachProfileCommand::new(
            ProfileId::parse("profile_01JASSIGNAPP")?,
            version,
            reason,
            evidence(now)?,
        ))
    }

    fn client(
        client_id: &str,
        status: ClientStatus,
    ) -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::restore(
            TenantId::parse("tenant_01JASSIGNAPP")?,
            ClientId::parse(client_id)?,
            AggregateVersion::INITIAL,
            ClientKind::Person,
            client_id,
            status,
        )?)
    }

    fn current_snapshot(
        client_id: &str,
    ) -> Result<CurrentProfileAssignmentSnapshot, Box<dyn std::error::Error>> {
        Ok(CurrentProfileAssignmentSnapshot::new(
            AssignmentId::parse("assignment_01JASSIGNAPP")?,
            client(client_id, ClientStatus::Active)?,
            ActorId::parse("actor_01JASSIGNAPP")?,
            UnixMillis::new(10),
            "initial assignment",
        ))
    }

    fn context(
        version: AggregateVersion,
        target_client_id: &str,
        current_client_id: Option<&str>,
    ) -> Result<ProfileAssignmentContext, Box<dyn std::error::Error>> {
        let current = current_client_id.map(current_snapshot).transpose()?;
        Ok(ProfileAssignmentContext::new(
            version,
            client(target_client_id, ClientStatus::Active)?,
            current,
        ))
    }

    fn detachment_context(
        version: AggregateVersion,
        current_client_id: Option<&str>,
    ) -> Result<ProfileDetachmentContext, Box<dyn std::error::Error>> {
        Ok(ProfileDetachmentContext::new(
            version,
            current_client_id.map(current_snapshot).transpose()?,
        ))
    }

    #[test]
    fn member_is_rejected_before_replay_context_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Miss],
            Some(context(
                AggregateVersion::INITIAL,
                "client_02JASSIGNAPP",
                None,
            )?),
        );
        assert_eq!(
            block_on(execute_assign_profile(
                &actor()?,
                MembershipRole::Member,
                &port,
                command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
            )),
            Err(ProfileAssignmentOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.context_calls.get(), 0);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_prewrite_replay_skips_context_and_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Replay(ProfileReplayReceipt::new(
                "assigned",
                Some("assignment_existing".to_owned()),
            ))],
            None,
        );
        let outcome = block_on(execute_assign_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "assignment_existing");
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.context_calls.get(), 0);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn fresh_assignment_loads_context_and_writes_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Miss],
            Some(context(
                AggregateVersion::INITIAL,
                "client_02JASSIGNAPP",
                None,
            )?),
        );
        let outcome = block_on(execute_assign_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
        ))?;
        assert_eq!(outcome.result_code(), "assigned");
        assert_eq!(outcome.resource_id(), "assignment_02JASSIGNAPP");
        assert_eq!(outcome.aggregate_version(), AggregateVersion::new(2)?);
        assert!(!outcome.replayed());
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.context_calls.get(), 1);
        assert_eq!(port.write_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn stale_expected_profile_version_fails_before_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Miss],
            Some(context(
                AggregateVersion::new(2)?,
                "client_02JASSIGNAPP",
                None,
            )?),
        );
        assert_eq!(
            block_on(execute_assign_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
            )),
            Err(ProfileAssignmentOperationError::VersionConflict)
        );
        assert_eq!(port.context_calls.get(), 1);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn reassignment_to_same_primary_client_is_domain_conflict() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Miss],
            Some(context(
                AggregateVersion::INITIAL,
                "client_01JASSIGNAPP",
                Some("client_01JASSIGNAPP"),
            )?),
        );
        assert_eq!(
            block_on(execute_assign_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command(AggregateVersion::INITIAL, "client_01JASSIGNAPP", 20)?,
            )),
            Err(ProfileAssignmentOperationError::Conflict)
        );
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn reassignment_time_regression_is_rejected_before_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Miss],
            Some(context(
                AggregateVersion::INITIAL,
                "client_02JASSIGNAPP",
                Some("client_01JASSIGNAPP"),
            )?),
        );
        assert_eq!(
            block_on(execute_assign_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 5)?,
            )),
            Err(ProfileAssignmentOperationError::InvalidState)
        );
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn inactive_target_is_neutral_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Miss],
            Some(ProfileAssignmentContext::new(
                AggregateVersion::INITIAL,
                client("client_02JASSIGNAPP", ClientStatus::Archived)?,
                None,
            )),
        );
        assert_eq!(
            block_on(execute_assign_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
            )),
            Err(ProfileAssignmentOperationError::NotFound)
        );
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn persisted_current_assignment_inconsistency_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let current = CurrentProfileAssignmentSnapshot::new(
            AssignmentId::parse("assignment_01JASSIGNAPP")?,
            client("client_01JASSIGNAPP", ClientStatus::Archived)?,
            ActorId::parse("actor_01JASSIGNAPP")?,
            UnixMillis::new(10),
            "initial assignment",
        );
        let port = FakePort::new(
            vec![ProfileReplayDecision::Miss],
            Some(ProfileAssignmentContext::new(
                AggregateVersion::INITIAL,
                client("client_02JASSIGNAPP", ClientStatus::Active)?,
                Some(current),
            )),
        );
        assert_eq!(
            block_on(execute_assign_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
            )),
            Err(ProfileAssignmentOperationError::IntegrityFailure)
        );
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn write_conflict_rechecks_exact_replay_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![
                ProfileReplayDecision::Miss,
                ProfileReplayDecision::Replay(ProfileReplayReceipt::new(
                    "assigned",
                    Some("assignment_replayed".to_owned()),
                )),
            ],
            Some(context(
                AggregateVersion::INITIAL,
                "client_02JASSIGNAPP",
                None,
            )?),
        );
        port.write_error
            .set(Some(ProfileAssignmentPortErrorClass::Conflict));
        let outcome = block_on(execute_assign_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "assignment_replayed");
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.context_calls.get(), 1);
        assert_eq!(port.write_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn context_dependency_failure_never_attempts_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss], None);
        port.context_error
            .set(Some(ProfileAssignmentPortErrorClass::DependencyUnavailable));
        assert_eq!(
            block_on(execute_assign_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command(AggregateVersion::INITIAL, "client_02JASSIGNAPP", 20)?,
            )),
            Err(ProfileAssignmentOperationError::DependencyUnavailable)
        );
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.context_calls.get(), 1);
        assert_eq!(port.write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn member_detach_is_rejected_before_replay_or_context() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss], None);
        assert_eq!(
            block_on(execute_detach_profile(
                &actor()?,
                MembershipRole::Member,
                &port,
                detach_command(AggregateVersion::INITIAL, 20, "detach")?,
            )),
            Err(ProfileAssignmentOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.detachment_context_calls.get(), 0);
        assert_eq!(port.detach_write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn fresh_detach_uses_server_owned_assignment_and_writes_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss], None);
        *port.detachment_context.borrow_mut() = Some(detachment_context(
            AggregateVersion::INITIAL,
            Some("client_01JASSIGNAPP"),
        )?);
        let outcome = block_on(execute_detach_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            detach_command(AggregateVersion::INITIAL, 20, "  operator detach  ")?,
        ))?;
        assert_eq!(outcome.result_code(), "detached");
        assert_eq!(outcome.resource_id(), "assignment_01JASSIGNAPP");
        assert_eq!(outcome.aggregate_version(), AggregateVersion::new(2)?);
        assert!(!outcome.replayed());
        assert_eq!(port.detachment_context_calls.get(), 1);
        assert_eq!(port.detach_write_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn detach_without_active_assignment_is_invalid_state_without_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss], None);
        *port.detachment_context.borrow_mut() = Some(detachment_context(
            AggregateVersion::INITIAL,
            None,
        )?);
        assert_eq!(
            block_on(execute_detach_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                detach_command(AggregateVersion::INITIAL, 20, "detach")?,
            )),
            Err(ProfileAssignmentOperationError::InvalidState)
        );
        assert_eq!(port.detach_write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn stale_detach_version_fails_before_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss], None);
        *port.detachment_context.borrow_mut() = Some(detachment_context(
            AggregateVersion::new(2)?,
            Some("client_01JASSIGNAPP"),
        )?);
        assert_eq!(
            block_on(execute_detach_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                detach_command(AggregateVersion::INITIAL, 20, "detach")?,
            )),
            Err(ProfileAssignmentOperationError::VersionConflict)
        );
        assert_eq!(port.detach_write_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn detachment_time_regression_and_invalid_reason_fail_before_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss], None);
        *port.detachment_context.borrow_mut() = Some(detachment_context(
            AggregateVersion::INITIAL,
            Some("client_01JASSIGNAPP"),
        )?);
        assert_eq!(
            block_on(execute_detach_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                detach_command(AggregateVersion::INITIAL, 5, "detach")?,
            )),
            Err(ProfileAssignmentOperationError::InvalidState)
        );
        assert_eq!(port.detach_write_calls.get(), 0);

        let invalid = FakePort::new(vec![ProfileReplayDecision::Miss], None);
        assert_eq!(
            block_on(execute_detach_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &invalid,
                detach_command(AggregateVersion::INITIAL, 20, "   ")?,
            )),
            Err(ProfileAssignmentOperationError::InvalidState)
        );
        assert_eq!(invalid.replay_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_detach_replay_is_context_free_and_requires_detached_receipt() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![ProfileReplayDecision::Replay(ProfileReplayReceipt::new(
                "detached",
                Some("assignment_01JASSIGNAPP".to_owned()),
            ))],
            None,
        );
        let outcome = block_on(execute_detach_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            detach_command(AggregateVersion::INITIAL, 20, "detach")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "assignment_01JASSIGNAPP");
        assert_eq!(port.detachment_context_calls.get(), 0);
        assert_eq!(port.detach_write_calls.get(), 0);

        let malformed = FakePort::new(
            vec![ProfileReplayDecision::Replay(ProfileReplayReceipt::new(
                "assigned",
                Some("assignment_01JASSIGNAPP".to_owned()),
            ))],
            None,
        );
        assert_eq!(
            block_on(execute_detach_profile(
                &actor()?,
                MembershipRole::TenantOwner,
                &malformed,
                detach_command(AggregateVersion::INITIAL, 20, "detach")?,
            )),
            Err(ProfileAssignmentOperationError::IntegrityFailure)
        );
        Ok(())
    }

    #[test]
    fn detach_write_conflict_rechecks_exact_replay_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(
            vec![
                ProfileReplayDecision::Miss,
                ProfileReplayDecision::Replay(ProfileReplayReceipt::new(
                    "detached",
                    Some("assignment_01JASSIGNAPP".to_owned()),
                )),
            ],
            None,
        );
        *port.detachment_context.borrow_mut() = Some(detachment_context(
            AggregateVersion::INITIAL,
            Some("client_01JASSIGNAPP"),
        )?);
        port.detach_write_error
            .set(Some(ProfileAssignmentPortErrorClass::Conflict));
        let outcome = block_on(execute_detach_profile(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            detach_command(AggregateVersion::INITIAL, 20, "detach")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.detachment_context_calls.get(), 1);
        assert_eq!(port.detach_write_calls.get(), 1);
        Ok(())
    }
}
