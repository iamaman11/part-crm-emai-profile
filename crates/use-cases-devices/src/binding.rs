use application_ports::CommandExecutionEvidence;
use application_ports::identity_governance::{
    ActiveOwnerGovernanceApplicationPort, DeviceBindingRevokeWrite, DeviceBindingWrite,
    IdentityGovernancePortError, IdentityGovernancePortErrorClass, IdentityReplayDecision,
    IdentityReplayReceipt,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, DeviceId, MachineCertificateFingerprint,
};

const DEVICE_BIND_COMMAND: &str = "device.binding.bind";
const DEVICE_REVOKE_COMMAND: &str = "device.binding.revoke";
const EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceBindingOperationError {
    InvalidRequest,
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for DeviceBindingOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "device binding request is invalid",
            Self::NotFound => "device binding resource not found",
            Self::VersionConflict => "device binding version conflict",
            Self::InvalidState => "device binding invalid state",
            Self::Conflict => "device binding conflict",
            Self::IntegrityFailure => "device binding integrity failure",
            Self::InternalFailure => "device binding internal failure",
            Self::DependencyUnavailable => "device binding dependency unavailable",
        })
    }
}

impl std::error::Error for DeviceBindingOperationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceBindingMutationOutcome {
    result_code: String,
    actor_id: ActorId,
    binding_version: AggregateVersion,
    replayed: bool,
}

impl DeviceBindingMutationOutcome {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn binding_version(&self) -> AggregateVersion {
        self.binding_version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteDeviceBindCommand {
    target_actor_id: ActorId,
    device_id: DeviceId,
    certificate_fingerprint: MachineCertificateFingerprint,
    expected_previous_version: Option<AggregateVersion>,
    evidence: CommandExecutionEvidence,
}

impl ExecuteDeviceBindCommand {
    #[must_use]
    pub fn new(
        target_actor_id: ActorId,
        device_id: DeviceId,
        certificate_fingerprint: MachineCertificateFingerprint,
        expected_previous_version: Option<AggregateVersion>,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            target_actor_id,
            device_id,
            certificate_fingerprint,
            expected_previous_version,
            evidence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteDeviceRevokeCommand {
    target_actor_id: ActorId,
    expected_version: AggregateVersion,
    evidence: CommandExecutionEvidence,
}

impl ExecuteDeviceRevokeCommand {
    #[must_use]
    pub fn new(
        target_actor_id: ActorId,
        expected_version: AggregateVersion,
        evidence: CommandExecutionEvidence,
    ) -> Self {
        Self {
            target_actor_id,
            expected_version,
            evidence,
        }
    }
}

pub async fn execute_device_bind<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteDeviceBindCommand,
) -> Result<DeviceBindingMutationOutcome, DeviceBindingOperationError> {
    authorize_device_binding_governance(role)?;
    let next_version = next_binding_version(command.expected_previous_version)?;
    let actor_id = command.target_actor_id.clone();

    if let Some(outcome) = prewrite_replay(
        actor,
        port,
        DEVICE_BIND_COMMAND,
        &command.evidence,
        &actor_id,
        next_version,
    )
    .await?
    {
        return Ok(outcome);
    }

    let write = DeviceBindingWrite::new(
        command.target_actor_id,
        command.device_id,
        command.certificate_fingerprint,
        command.expected_previous_version,
        next_version,
        command.evidence,
        EVENT_PAYLOAD,
    );
    match port.bind_device(actor, &write).await {
        Ok(()) => Ok(fresh_outcome("bound", actor_id, next_version)),
        Err(error) if error.class() == IdentityGovernancePortErrorClass::Conflict => {
            conflict_replay(
                actor,
                port,
                DEVICE_BIND_COMMAND,
                write.evidence(),
                &actor_id,
                next_version,
            )
            .await
        }
        Err(error) => Err(map_port_error(error)),
    }
}

pub async fn execute_device_revoke<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ExecuteDeviceRevokeCommand,
) -> Result<DeviceBindingMutationOutcome, DeviceBindingOperationError> {
    authorize_device_binding_governance(role)?;
    let actor_id = command.target_actor_id.clone();

    if let Some(outcome) = prewrite_replay(
        actor,
        port,
        DEVICE_REVOKE_COMMAND,
        &command.evidence,
        &actor_id,
        command.expected_version,
    )
    .await?
    {
        return Ok(outcome);
    }

    let write = DeviceBindingRevokeWrite::new(
        command.target_actor_id,
        command.expected_version,
        command.evidence,
        EVENT_PAYLOAD,
    );
    match port.revoke_device_binding(actor, &write).await {
        Ok(()) => Ok(fresh_outcome(
            "revoked",
            actor_id,
            command.expected_version,
        )),
        Err(error) if error.class() == IdentityGovernancePortErrorClass::Conflict => {
            conflict_replay(
                actor,
                port,
                DEVICE_REVOKE_COMMAND,
                write.evidence(),
                &actor_id,
                command.expected_version,
            )
            .await
        }
        Err(error) => Err(map_port_error(error)),
    }
}

fn authorize_device_binding_governance(
    role: MembershipRole,
) -> Result<(), DeviceBindingOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(DeviceBindingOperationError::NotFound)
    }
}

fn next_binding_version(
    previous: Option<AggregateVersion>,
) -> Result<AggregateVersion, DeviceBindingOperationError> {
    match previous {
        Some(version) => version
            .next()
            .map_err(|_| DeviceBindingOperationError::InternalFailure),
        None => Ok(AggregateVersion::INITIAL),
    }
}

async fn prewrite_replay<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command_name: &str,
    evidence: &CommandExecutionEvidence,
    actor_id: &ActorId,
    version: AggregateVersion,
) -> Result<Option<DeviceBindingMutationOutcome>, DeviceBindingOperationError> {
    match port
        .decide_identity_replay(actor, command_name, evidence)
        .await
        .map_err(map_port_error)?
    {
        IdentityReplayDecision::Miss => Ok(None),
        IdentityReplayDecision::Replay(receipt) => {
            Ok(Some(replay_outcome(actor_id, version, &receipt)))
        }
        IdentityReplayDecision::Conflict => Err(DeviceBindingOperationError::Conflict),
    }
}

async fn conflict_replay<P: ActiveOwnerGovernanceApplicationPort>(
    actor: &ActorContext,
    port: &P,
    command_name: &str,
    evidence: &CommandExecutionEvidence,
    actor_id: &ActorId,
    version: AggregateVersion,
) -> Result<DeviceBindingMutationOutcome, DeviceBindingOperationError> {
    match port
        .decide_identity_replay(actor, command_name, evidence)
        .await
        .map_err(map_port_error)?
    {
        IdentityReplayDecision::Replay(receipt) => Ok(replay_outcome(actor_id, version, &receipt)),
        IdentityReplayDecision::Miss | IdentityReplayDecision::Conflict => {
            Err(DeviceBindingOperationError::Conflict)
        }
    }
}

fn fresh_outcome(
    result_code: &str,
    actor_id: ActorId,
    binding_version: AggregateVersion,
) -> DeviceBindingMutationOutcome {
    DeviceBindingMutationOutcome {
        result_code: result_code.to_owned(),
        actor_id,
        binding_version,
        replayed: false,
    }
}

fn replay_outcome(
    actor_id: &ActorId,
    binding_version: AggregateVersion,
    receipt: &IdentityReplayReceipt,
) -> DeviceBindingMutationOutcome {
    let replayed_actor_id = receipt
        .result_reference()
        .and_then(|value| ActorId::parse(value.to_owned()).ok())
        .unwrap_or_else(|| actor_id.clone());
    DeviceBindingMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        actor_id: replayed_actor_id,
        binding_version,
        replayed: true,
    }
}

fn map_port_error(error: IdentityGovernancePortError) -> DeviceBindingOperationError {
    match error.class() {
        IdentityGovernancePortErrorClass::NotFound => DeviceBindingOperationError::NotFound,
        IdentityGovernancePortErrorClass::VersionConflict => {
            DeviceBindingOperationError::VersionConflict
        }
        IdentityGovernancePortErrorClass::InvalidState => DeviceBindingOperationError::InvalidState,
        IdentityGovernancePortErrorClass::Conflict => DeviceBindingOperationError::Conflict,
        IdentityGovernancePortErrorClass::IntegrityFailure => {
            DeviceBindingOperationError::IntegrityFailure
        }
        IdentityGovernancePortErrorClass::InternalFailure => {
            DeviceBindingOperationError::InternalFailure
        }
        IdentityGovernancePortErrorClass::DependencyUnavailable => {
            DeviceBindingOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application_ports::identity_governance::{
        InvitationCreateWrite, MembershipStatusWrite, OwnerTransferWrite,
    };
    use profile_platform_primitives::{
        AuditEventId, CorrelationId, IdempotencyKey, OutboxEventId, PayloadFingerprint, TenantId,
        TenantScope, UnixMillis,
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
        bind_calls: Cell<u32>,
        revoke_calls: Cell<u32>,
        write_error: Cell<Option<IdentityGovernancePortErrorClass>>,
        observed_bind_version: Cell<Option<u64>>,
    }

    impl FakePort {
        fn new(replay: Vec<IdentityReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                commands: RefCell::new(Vec::new()),
                bind_calls: Cell::new(0),
                revoke_calls: Cell::new(0),
                write_error: Cell::new(None),
                observed_bind_version: Cell::new(None),
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
            self.write_result()
        }

        async fn create_invitation(
            &self,
            _actor: &ActorContext,
            _write: &InvitationCreateWrite,
        ) -> Result<(), IdentityGovernancePortError> {
            self.write_result()
        }

        async fn update_membership_status(
            &self,
            _actor: &ActorContext,
            _write: &MembershipStatusWrite,
        ) -> Result<(), IdentityGovernancePortError> {
            self.write_result()
        }

        async fn bind_device(
            &self,
            _actor: &ActorContext,
            write: &DeviceBindingWrite,
        ) -> Result<(), IdentityGovernancePortError> {
            self.bind_calls.set(self.bind_calls.get() + 1);
            self.observed_bind_version
                .set(Some(write.next_version().value()));
            self.write_result()
        }

        async fn revoke_device_binding(
            &self,
            _actor: &ActorContext,
            _write: &DeviceBindingRevokeWrite,
        ) -> Result<(), IdentityGovernancePortError> {
            self.revoke_calls.set(self.revoke_calls.get() + 1);
            self.write_result()
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JDEVICEBIND")?),
            ActorId::parse("actor_01JDEVICEOWNER")?,
            CorrelationId::parse("corr_01JDEVICEBIND")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JDEVICEBIND")?,
            PayloadFingerprint::parse("a".repeat(64))?,
            AuditEventId::parse("audit_01JDEVICEBIND")?,
            OutboxEventId::parse("outbox_01JDEVICEBIND")?,
            UnixMillis::new(10),
            UnixMillis::new(100),
        ))
    }

    fn bind_command(
        previous: Option<AggregateVersion>,
    ) -> Result<ExecuteDeviceBindCommand, Box<dyn std::error::Error>> {
        Ok(ExecuteDeviceBindCommand::new(
            ActorId::parse("actor_01JDEVICETARGET")?,
            DeviceId::parse("device_01JDEVICEBIND")?,
            MachineCertificateFingerprint::parse("ab".repeat(32))?,
            previous,
            evidence()?,
        ))
    }

    #[test]
    fn non_owner_stops_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_device_bind(
                &actor()?,
                MembershipRole::Member,
                &port,
                bind_command(None)?,
            )),
            Err(DeviceBindingOperationError::NotFound)
        );
        assert!(port.commands.borrow().is_empty());
        assert_eq!(port.bind_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn initial_bind_uses_version_one_and_rebind_advances_exactly_once()
    -> Result<(), Box<dyn std::error::Error>> {
        let initial = FakePort::new(vec![IdentityReplayDecision::Miss]);
        let outcome = block_on(execute_device_bind(
            &actor()?,
            MembershipRole::TenantOwner,
            &initial,
            bind_command(None)?,
        ))?;
        assert_eq!(outcome.binding_version(), AggregateVersion::INITIAL);
        assert_eq!(initial.observed_bind_version.get(), Some(1));

        let rebind = FakePort::new(vec![IdentityReplayDecision::Miss]);
        let outcome = block_on(execute_device_bind(
            &actor()?,
            MembershipRole::TenantOwner,
            &rebind,
            bind_command(Some(AggregateVersion::new(3)?))?,
        ))?;
        assert_eq!(outcome.binding_version(), AggregateVersion::new(4)?);
        assert_eq!(rebind.observed_bind_version.get(), Some(4));
        Ok(())
    }

    #[test]
    fn exact_replay_skips_device_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Replay(
            IdentityReplayReceipt::new("bound", Some("actor_01JDEVICETARGET".to_owned())),
        )]);
        let outcome = block_on(execute_device_bind(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            bind_command(None)?,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(port.bind_calls.get(), 0);
        assert_eq!(port.commands.borrow().as_slice(), [DEVICE_BIND_COMMAND]);
        Ok(())
    }

    #[test]
    fn stale_version_is_not_reclassified_as_replay() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        port.write_error
            .set(Some(IdentityGovernancePortErrorClass::VersionConflict));
        assert_eq!(
            block_on(execute_device_bind(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                bind_command(Some(AggregateVersion::INITIAL))?,
            )),
            Err(DeviceBindingOperationError::VersionConflict)
        );
        assert_eq!(port.bind_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn revoke_uses_exact_current_binding_version() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        let expected = AggregateVersion::new(5)?;
        let outcome = block_on(execute_device_revoke(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            ExecuteDeviceRevokeCommand::new(
                ActorId::parse("actor_01JDEVICETARGET")?,
                expected,
                evidence()?,
            ),
        ))?;
        assert_eq!(outcome.result_code(), "revoked");
        assert_eq!(outcome.binding_version(), expected);
        assert_eq!(port.revoke_calls.get(), 1);
        assert_eq!(port.commands.borrow().as_slice(), [DEVICE_REVOKE_COMMAND]);
        Ok(())
    }

    #[test]
    fn version_overflow_fails_before_replay_or_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![IdentityReplayDecision::Miss]);
        assert_eq!(
            block_on(execute_device_bind(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                bind_command(Some(AggregateVersion::new(u64::MAX)?))?,
            )),
            Err(DeviceBindingOperationError::InternalFailure)
        );
        assert!(port.commands.borrow().is_empty());
        assert_eq!(port.bind_calls.get(), 0);
        Ok(())
    }
}
