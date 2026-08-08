use application_ports::CommandExecutionEvidence;
use application_ports::profiles::{
    ProfileGrantApplicationPort, ProfileGrantPortError, ProfileGrantPortErrorClass, ProfileGrantRole,
    ProfileGrantWrite, ProfileReplayDecision, ProfileReplayReceipt,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, ActorId, AggregateVersion, ProfileId};

const PROFILE_GRANT_COMMAND: &str = "profile.grant";
const PROFILE_GRANT_REVOKE_COMMAND: &str = "profile.grant_revoke";
const EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileGrantAction {
    Grant,
    Revoke,
}

impl ProfileGrantAction {
    const fn command_name(self) -> &'static str {
        match self {
            Self::Grant => PROFILE_GRANT_COMMAND,
            Self::Revoke => PROFILE_GRANT_REVOKE_COMMAND,
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
pub struct ExecuteProfileGrantCommand {
    target_actor_id: ActorId,
    profile_id: ProfileId,
    expected_profile_version: AggregateVersion,
    role: String,
    reason: String,
    evidence: CommandExecutionEvidence,
}

impl ExecuteProfileGrantCommand {
    #[must_use]
    pub fn new(
        target_actor_id: ActorId,
        profile_id: ProfileId,
        expected_profile_version: AggregateVersion,
        role: impl Into<String>,
        reason: impl Into<String>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            target_actor_id,
            profile_id,
            expected_profile_version,
            role: role.into(),
            reason: reason.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileGrantOutcome {
    action: ProfileGrantAction,
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl ProfileGrantOutcome {
    #[must_use]
    pub const fn action(&self) -> ProfileGrantAction {
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
pub enum ProfileGrantOperationError {
    InvalidRequest,
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for ProfileGrantOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "profile grant request is invalid",
            Self::NotFound => "profile grant not found",
            Self::VersionConflict => "profile grant version conflict",
            Self::InvalidState => "profile grant invalid state",
            Self::Conflict => "profile grant conflict",
            Self::IntegrityFailure => "profile grant integrity failure",
            Self::InternalFailure => "profile grant internal failure",
            Self::DependencyUnavailable => "profile grant dependency unavailable",
        })
    }
}

impl std::error::Error for ProfileGrantOperationError {}

pub fn authorize_profile_grant(role: MembershipRole) -> Result<(), ProfileGrantOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(ProfileGrantOperationError::NotFound)
    }
}

pub fn next_profile_grant_version(
    version: AggregateVersion,
) -> Result<AggregateVersion, ProfileGrantOperationError> {
    version
        .next()
        .map_err(|_| ProfileGrantOperationError::InternalFailure)
}

pub fn parse_profile_grant_role(value: &str) -> Result<ProfileGrantRole, ProfileGrantOperationError> {
    match value {
        "PROFILE_VIEWER" => Ok(ProfileGrantRole::Viewer),
        "PROFILE_OPERATOR" => Ok(ProfileGrantRole::Operator),
        _ => Err(ProfileGrantOperationError::InvalidRequest),
    }
}

pub async fn execute_profile_grant<P: ProfileGrantApplicationPort>(
    actor: &ActorContext,
    membership_role: MembershipRole,
    port: &P,
    action: ProfileGrantAction,
    command: ExecuteProfileGrantCommand,
) -> Result<ProfileGrantOutcome, ProfileGrantOperationError> {
    authorize_profile_grant(membership_role)?;
    let next_version = next_profile_grant_version(command.expected_profile_version)?;
    let grant_role = parse_profile_grant_role(&command.role)?;
    let command_name = action.command_name();

    match port
        .decide_profile_grant_replay(actor, command_name, &command.evidence)
        .await
        .map_err(map_port_error)?
    {
        ProfileReplayDecision::Miss => {}
        ProfileReplayDecision::Replay(receipt) => {
            return Ok(replay_outcome(
                action,
                &command.profile_id,
                next_version,
                &receipt,
            ));
        }
        ProfileReplayDecision::Conflict => return Err(ProfileGrantOperationError::Conflict),
    }

    let write = ProfileGrantWrite::new(
        command.target_actor_id,
        command.profile_id,
        command.expected_profile_version,
        grant_role,
        command.reason,
        command.evidence,
        EVENT_PAYLOAD,
    );
    let result = match action {
        ProfileGrantAction::Grant => port.grant_profile(actor, &write).await,
        ProfileGrantAction::Revoke => port.revoke_profile_grant(actor, &write).await,
    };
    match result {
        Ok(()) => Ok(ProfileGrantOutcome {
            action,
            result_code: action.fresh_result_code().to_owned(),
            resource_id: write.profile_id().as_str().to_owned(),
            aggregate_version: next_version,
            replayed: false,
        }),
        Err(error) if error.class() == ProfileGrantPortErrorClass::Conflict => {
            match port
                .decide_profile_grant_replay(actor, command_name, write.evidence())
                .await
                .map_err(map_port_error)?
            {
                ProfileReplayDecision::Replay(receipt) => Ok(replay_outcome(
                    action,
                    write.profile_id(),
                    next_version,
                    &receipt,
                )),
                ProfileReplayDecision::Miss | ProfileReplayDecision::Conflict => {
                    Err(ProfileGrantOperationError::Conflict)
                }
            }
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn replay_outcome(
    action: ProfileGrantAction,
    profile_id: &ProfileId,
    version: AggregateVersion,
    receipt: &ProfileReplayReceipt,
) -> ProfileGrantOutcome {
    ProfileGrantOutcome {
        action,
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(profile_id.as_str())
            .to_owned(),
        aggregate_version: version,
        replayed: true,
    }
}

fn map_port_error(error: ProfileGrantPortError) -> ProfileGrantOperationError {
    match error.class() {
        ProfileGrantPortErrorClass::NotFound => ProfileGrantOperationError::NotFound,
        ProfileGrantPortErrorClass::VersionConflict => ProfileGrantOperationError::VersionConflict,
        ProfileGrantPortErrorClass::InvalidState => ProfileGrantOperationError::InvalidState,
        ProfileGrantPortErrorClass::Conflict => ProfileGrantOperationError::Conflict,
        ProfileGrantPortErrorClass::IntegrityFailure => {
            ProfileGrantOperationError::IntegrityFailure
        }
        ProfileGrantPortErrorClass::InternalFailure => {
            ProfileGrantOperationError::InternalFailure
        }
        ProfileGrantPortErrorClass::DependencyUnavailable => {
            ProfileGrantOperationError::DependencyUnavailable
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
        replay: RefCell<Vec<ProfileReplayDecision>>,
        replay_commands: RefCell<Vec<String>>,
        replay_calls: Cell<u32>,
        grant_calls: Cell<u32>,
        revoke_calls: Cell<u32>,
        write_error: Cell<Option<ProfileGrantPortErrorClass>>,
    }

    impl FakePort {
        fn new(replay: Vec<ProfileReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                replay_commands: RefCell::new(Vec::new()),
                replay_calls: Cell::new(0),
                grant_calls: Cell::new(0),
                revoke_calls: Cell::new(0),
                write_error: Cell::new(None),
            }
        }

        fn write_result(&self) -> Result<(), ProfileGrantPortError> {
            match self.write_error.get() {
                Some(class) => Err(ProfileGrantPortError::new(class)),
                None => Ok(()),
            }
        }
    }

    impl ProfileGrantApplicationPort for FakePort {
        async fn decide_profile_grant_replay(
            &self,
            _actor: &ActorContext,
            command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<ProfileReplayDecision, ProfileGrantPortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            self.replay_commands
                .borrow_mut()
                .push(command_name.to_owned());
            Ok(if self.replay.borrow().is_empty() {
                ProfileReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            })
        }

        async fn grant_profile(
            &self,
            _actor: &ActorContext,
            _write: &ProfileGrantWrite,
        ) -> Result<(), ProfileGrantPortError> {
            self.grant_calls.set(self.grant_calls.get() + 1);
            self.write_result()
        }

        async fn revoke_profile_grant(
            &self,
            _actor: &ActorContext,
            _write: &ProfileGrantWrite,
        ) -> Result<(), ProfileGrantPortError> {
            self.revoke_calls.set(self.revoke_calls.get() + 1);
            self.write_result()
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JGRANTAPP")?),
            ActorId::parse("actor_01JGRANTAPP")?,
            CorrelationId::parse("corr_01JGRANTAPP")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JGRANTAPP")?,
            "request-digest-01JGRANTAPP",
            AuditEventId::parse("audit_01JGRANTAPP")?,
            OutboxEventId::parse("outbox_01JGRANTAPP")?,
            UnixMillis::new(10),
            UnixMillis::new(100),
        ))
    }

    fn command(
        version: AggregateVersion,
        role: &str,
    ) -> Result<ExecuteProfileGrantCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteProfileGrantCommand::new(
            ActorId::parse("actor_01JGRANTTARGET")?,
            ProfileId::parse("profile_01JGRANTAPP")?,
            version,
            role,
            "operator requested grant",
            evidence()?,
        ))
    }

    #[test]
    fn non_owner_stops_before_role_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_profile_grant(
                &actor()?,
                MembershipRole::Member,
                &port,
                ProfileGrantAction::Grant,
                command(AggregateVersion::INITIAL, "INVALID")?,
            )),
            Err(ProfileGrantOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn overflow_stops_before_role_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_profile_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ProfileGrantAction::Grant,
                command(AggregateVersion::new(u64::MAX)?, "INVALID")?,
            )),
            Err(ProfileGrantOperationError::InternalFailure)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn invalid_role_stops_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_profile_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ProfileGrantAction::Grant,
                command(AggregateVersion::INITIAL, "PROFILE_EDITOR")?,
            )),
            Err(ProfileGrantOperationError::InvalidRequest)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn grant_and_revoke_use_distinct_idempotency_domains() -> Result<(), Box<dyn std::error::Error>> {
        let grant = FakePort::new(vec![ProfileReplayDecision::Miss]);
        block_on(execute_profile_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &grant,
            ProfileGrantAction::Grant,
            command(AggregateVersion::INITIAL, "PROFILE_VIEWER")?,
        ))?;
        assert_eq!(
            grant.replay_commands.borrow().as_slice(),
            [PROFILE_GRANT_COMMAND]
        );
        assert_eq!(grant.grant_calls.get(), 1);

        let revoke = FakePort::new(vec![ProfileReplayDecision::Miss]);
        block_on(execute_profile_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &revoke,
            ProfileGrantAction::Revoke,
            command(AggregateVersion::INITIAL, "PROFILE_OPERATOR")?,
        ))?;
        assert_eq!(
            revoke.replay_commands.borrow().as_slice(),
            [PROFILE_GRANT_REVOKE_COMMAND]
        );
        assert_eq!(revoke.revoke_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn exact_replay_skips_grant_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Replay(
            ProfileReplayReceipt::new("granted", Some("profile_existing".to_owned())),
        )]);
        let outcome = block_on(execute_profile_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ProfileGrantAction::Grant,
            command(AggregateVersion::INITIAL, "PROFILE_VIEWER")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.result_code(), "granted");
        assert_eq!(outcome.resource_id(), "profile_existing");
        assert_eq!(outcome.aggregate_version(), AggregateVersion::new(2)?);
        assert_eq!(port.grant_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn revoke_exact_replay_preserves_revoke_action() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![ProfileReplayDecision::Replay(
            ProfileReplayReceipt::new("revoked", Some("profile_existing".to_owned())),
        )]);
        let outcome = block_on(execute_profile_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ProfileGrantAction::Revoke,
            command(AggregateVersion::INITIAL, "PROFILE_VIEWER")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.action(), ProfileGrantAction::Revoke);
        assert_eq!(port.revoke_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn conflict_rechecks_exact_replay_once() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![
            ProfileReplayDecision::Miss,
            ProfileReplayDecision::Replay(ProfileReplayReceipt::new("granted", None)),
        ]);
        port.write_error
            .set(Some(ProfileGrantPortErrorClass::Conflict));
        let outcome = block_on(execute_profile_grant(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ProfileGrantAction::Grant,
            command(AggregateVersion::INITIAL, "PROFILE_OPERATOR")?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.grant_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn conflict_recheck_miss_remains_conflict() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![
            ProfileReplayDecision::Miss,
            ProfileReplayDecision::Miss,
        ]);
        port.write_error
            .set(Some(ProfileGrantPortErrorClass::Conflict));
        assert_eq!(
            block_on(execute_profile_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ProfileGrantAction::Revoke,
                command(AggregateVersion::INITIAL, "PROFILE_VIEWER")?,
            )),
            Err(ProfileGrantOperationError::Conflict)
        );
        assert_eq!(port.replay_calls.get(), 2);
        assert_eq!(port.revoke_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn non_conflict_failure_never_rechecks_replay() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![
            ProfileReplayDecision::Miss,
            ProfileReplayDecision::Replay(ProfileReplayReceipt::new("granted", None)),
        ]);
        port.write_error
            .set(Some(ProfileGrantPortErrorClass::VersionConflict));
        assert_eq!(
            block_on(execute_profile_grant(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                ProfileGrantAction::Grant,
                command(AggregateVersion::INITIAL, "PROFILE_VIEWER")?,
            )),
            Err(ProfileGrantOperationError::VersionConflict)
        );
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.grant_calls.get(), 1);
        Ok(())
    }
}
