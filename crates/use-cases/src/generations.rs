use application_ports::CommandExecutionEvidence;
use application_ports::generations::{
    GenerationApplicationPort, GenerationPortError, GenerationPortErrorClass,
    GenerationProfileVersionWrite, GenerationReadModel, GenerationReplayDecision,
    GenerationReplayReceipt, QuarantineGenerationWrite, RegisterGenerationWrite,
    VerifyGenerationWrite,
};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, AggregateVersion, GenerationId, ProfileId};

const REGISTER_COMMAND: &str = "profile_generation.register";
const VERIFY_COMMAND: &str = "profile_generation.verify";
const ACTIVATE_COMMAND: &str = "profile_generation.activate";
const DEACTIVATE_COMMAND: &str = "profile_generation.deactivate";
const QUARANTINE_COMMAND: &str = "profile_generation.quarantine";
const EVENT_PAYLOAD: &str = "{}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterGenerationCommand {
    pub profile_id: ProfileId,
    pub generation_id: GenerationId,
    pub object_key: String,
    pub metadata_digest: String,
    pub container_digest: String,
    pub evidence: CommandExecutionEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyGenerationCommand {
    pub profile_id: ProfileId,
    pub generation_id: GenerationId,
    pub expected_generation_version: AggregateVersion,
    pub verification_reference: String,
    pub evidence: CommandExecutionEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileGenerationVersionCommand {
    pub profile_id: ProfileId,
    pub generation_id: GenerationId,
    pub expected_profile_version: AggregateVersion,
    pub evidence: CommandExecutionEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuarantineGenerationCommand {
    pub profile_id: ProfileId,
    pub generation_id: GenerationId,
    pub expected_generation_version: AggregateVersion,
    pub evidence: CommandExecutionEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationMutationOutcome {
    result_code: String,
    resource_id: String,
    aggregate_version: AggregateVersion,
    replayed: bool,
}

impl GenerationMutationOutcome {
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
pub enum GenerationOperationError {
    InvalidRequest,
    NotFound,
    VersionConflict,
    InvalidState,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

impl fmt::Display for GenerationOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "generation request is invalid",
            Self::NotFound => "generation not found",
            Self::VersionConflict => "generation version conflict",
            Self::InvalidState => "generation invalid state",
            Self::Conflict => "generation command conflict",
            Self::IntegrityFailure => "generation integrity failure",
            Self::InternalFailure => "generation internal failure",
            Self::DependencyUnavailable => "generation dependency unavailable",
        })
    }
}

impl std::error::Error for GenerationOperationError {}

pub fn authorize_generation_mutation(role: MembershipRole) -> Result<(), GenerationOperationError> {
    if role == MembershipRole::TenantOwner {
        Ok(())
    } else {
        Err(GenerationOperationError::NotFound)
    }
}

pub fn validate_generation_registration(
    object_key: &str,
    metadata_digest: &str,
    container_digest: &str,
) -> Result<(), GenerationOperationError> {
    if !valid_object_key(object_key)
        || !valid_digest(metadata_digest)
        || !valid_digest(container_digest)
    {
        return Err(GenerationOperationError::InvalidRequest);
    }
    Ok(())
}

pub fn validate_generation_verification_reference(
    value: &str,
) -> Result<(), GenerationOperationError> {
    if !(8..=256).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':'))
    {
        return Err(GenerationOperationError::InvalidRequest);
    }
    Ok(())
}

pub fn next_generation_version(
    version: AggregateVersion,
) -> Result<AggregateVersion, GenerationOperationError> {
    version
        .next()
        .map_err(|_| GenerationOperationError::InternalFailure)
}

pub async fn execute_register_generation<P: GenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: RegisterGenerationCommand,
) -> Result<GenerationMutationOutcome, GenerationOperationError> {
    authorize_generation_mutation(role)?;
    validate_generation_registration(
        &command.object_key,
        &command.metadata_digest,
        &command.container_digest,
    )?;
    if let Some(outcome) = replay_outcome(
        port.decide_replay(actor, REGISTER_COMMAND, &command.evidence)
            .await
            .map_err(map_port_error)?,
        &command.generation_id,
        AggregateVersion::INITIAL,
    )? {
        return Ok(outcome);
    }
    let write = RegisterGenerationWrite::new(
        command.profile_id,
        command.generation_id,
        command.object_key,
        command.metadata_digest,
        command.container_digest,
        command.evidence,
        EVENT_PAYLOAD,
    );
    port.register_generation(actor, &write)
        .await
        .map_err(map_port_error)?;
    Ok(fresh_outcome(
        "registered",
        write.generation_id(),
        AggregateVersion::INITIAL,
    ))
}

pub async fn execute_verify_generation<P: GenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: VerifyGenerationCommand,
) -> Result<GenerationMutationOutcome, GenerationOperationError> {
    authorize_generation_mutation(role)?;
    validate_generation_verification_reference(&command.verification_reference)?;
    let next = next_generation_version(command.expected_generation_version)?;
    if let Some(outcome) = replay_outcome(
        port.decide_replay(actor, VERIFY_COMMAND, &command.evidence)
            .await
            .map_err(map_port_error)?,
        &command.generation_id,
        next,
    )? {
        return Ok(outcome);
    }
    let write = VerifyGenerationWrite::new(
        command.profile_id,
        command.generation_id,
        command.expected_generation_version,
        command.verification_reference,
        command.evidence,
        EVENT_PAYLOAD,
    );
    port.verify_generation(actor, &write)
        .await
        .map_err(map_port_error)?;
    Ok(fresh_outcome("verified", write.generation_id(), next))
}

pub async fn execute_activate_generation<P: GenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ProfileGenerationVersionCommand,
) -> Result<GenerationMutationOutcome, GenerationOperationError> {
    execute_profile_version_mutation(
        actor,
        role,
        port,
        command,
        ACTIVATE_COMMAND,
        "activated",
        true,
    )
    .await
}

pub async fn execute_deactivate_generation<P: GenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ProfileGenerationVersionCommand,
) -> Result<GenerationMutationOutcome, GenerationOperationError> {
    execute_profile_version_mutation(
        actor,
        role,
        port,
        command,
        DEACTIVATE_COMMAND,
        "deactivated",
        false,
    )
    .await
}

async fn execute_profile_version_mutation<P: GenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: ProfileGenerationVersionCommand,
    command_name: &str,
    result_code: &str,
    activate: bool,
) -> Result<GenerationMutationOutcome, GenerationOperationError> {
    authorize_generation_mutation(role)?;
    let next = next_generation_version(command.expected_profile_version)?;
    if let Some(outcome) = replay_outcome(
        port.decide_replay(actor, command_name, &command.evidence)
            .await
            .map_err(map_port_error)?,
        &command.generation_id,
        next,
    )? {
        return Ok(outcome);
    }
    let write = GenerationProfileVersionWrite::new(
        command.profile_id,
        command.generation_id,
        command.expected_profile_version,
        command.evidence,
        EVENT_PAYLOAD,
    );
    if activate {
        port.activate_generation(actor, &write).await
    } else {
        port.deactivate_generation(actor, &write).await
    }
    .map_err(map_port_error)?;
    Ok(fresh_outcome(result_code, write.generation_id(), next))
}

pub async fn execute_quarantine_generation<P: GenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    command: QuarantineGenerationCommand,
) -> Result<GenerationMutationOutcome, GenerationOperationError> {
    authorize_generation_mutation(role)?;
    let next = next_generation_version(command.expected_generation_version)?;
    if let Some(outcome) = replay_outcome(
        port.decide_replay(actor, QUARANTINE_COMMAND, &command.evidence)
            .await
            .map_err(map_port_error)?,
        &command.generation_id,
        next,
    )? {
        return Ok(outcome);
    }
    let write = QuarantineGenerationWrite::new(
        command.profile_id,
        command.generation_id,
        command.expected_generation_version,
        command.evidence,
        EVENT_PAYLOAD,
    );
    port.quarantine_generation(actor, &write)
        .await
        .map_err(map_port_error)?;
    Ok(fresh_outcome("quarantined", write.generation_id(), next))
}

pub async fn get_visible_generation<P: GenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<GenerationReadModel, GenerationOperationError> {
    port.find_visible_generation(
        actor.tenant_scope(),
        actor.actor_id(),
        role,
        profile_id,
        generation_id,
    )
    .await
    .map_err(map_port_error)?
    .ok_or(GenerationOperationError::NotFound)
}

fn valid_object_key(value: &str) -> bool {
    (16..=512).contains(&value.len())
        && !value.starts_with('/')
        && !value.contains("..")
        && !value.contains('\\')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':')
        })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn replay_outcome(
    decision: GenerationReplayDecision,
    generation_id: &GenerationId,
    version: AggregateVersion,
) -> Result<Option<GenerationMutationOutcome>, GenerationOperationError> {
    match decision {
        GenerationReplayDecision::Miss => Ok(None),
        GenerationReplayDecision::Replay(receipt) => {
            Ok(Some(receipt_outcome(&receipt, generation_id, version)))
        }
        GenerationReplayDecision::Conflict => Err(GenerationOperationError::Conflict),
    }
}

fn receipt_outcome(
    receipt: &GenerationReplayReceipt,
    generation_id: &GenerationId,
    version: AggregateVersion,
) -> GenerationMutationOutcome {
    GenerationMutationOutcome {
        result_code: receipt.result_code().to_owned(),
        resource_id: receipt
            .result_reference()
            .unwrap_or(generation_id.as_str())
            .to_owned(),
        aggregate_version: version,
        replayed: true,
    }
}

fn fresh_outcome(
    result_code: &str,
    generation_id: &GenerationId,
    version: AggregateVersion,
) -> GenerationMutationOutcome {
    GenerationMutationOutcome {
        result_code: result_code.to_owned(),
        resource_id: generation_id.as_str().to_owned(),
        aggregate_version: version,
        replayed: false,
    }
}

fn map_port_error(error: GenerationPortError) -> GenerationOperationError {
    match error.class() {
        GenerationPortErrorClass::NotFound => GenerationOperationError::NotFound,
        GenerationPortErrorClass::VersionConflict => GenerationOperationError::VersionConflict,
        GenerationPortErrorClass::InvalidState => GenerationOperationError::InvalidState,
        GenerationPortErrorClass::Conflict => GenerationOperationError::Conflict,
        GenerationPortErrorClass::IntegrityFailure => GenerationOperationError::IntegrityFailure,
        GenerationPortErrorClass::InternalFailure => GenerationOperationError::InternalFailure,
        GenerationPortErrorClass::DependencyUnavailable => {
            GenerationOperationError::DependencyUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application_ports::generations::{GenerationReplayReceipt, GenerationStatus};
    use profile_platform_primitives::{
        ActorId, AuditEventId, CorrelationId, IdempotencyKey, OutboxEventId, TenantId, TenantScope,
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
        replay: RefCell<Vec<GenerationReplayDecision>>,
        replay_calls: Cell<u32>,
        writes: Cell<u32>,
        visible: RefCell<Option<GenerationReadModel>>,
        write_error: Cell<Option<GenerationPortErrorClass>>,
    }

    impl FakePort {
        fn new(replay: Vec<GenerationReplayDecision>) -> Self {
            Self {
                replay: RefCell::new(replay),
                replay_calls: Cell::new(0),
                writes: Cell::new(0),
                visible: RefCell::new(None),
                write_error: Cell::new(None),
            }
        }
    }

    impl GenerationApplicationPort for FakePort {
        async fn decide_replay(
            &self,
            _actor: &ActorContext,
            _command_name: &str,
            _evidence: &CommandExecutionEvidence,
        ) -> Result<GenerationReplayDecision, GenerationPortError> {
            self.replay_calls.set(self.replay_calls.get() + 1);
            Ok(if self.replay.borrow().is_empty() {
                GenerationReplayDecision::Miss
            } else {
                self.replay.borrow_mut().remove(0)
            })
        }

        async fn register_generation(
            &self,
            _actor: &ActorContext,
            _write: &RegisterGenerationWrite,
        ) -> Result<(), GenerationPortError> {
            self.write()
        }

        async fn verify_generation(
            &self,
            _actor: &ActorContext,
            _write: &VerifyGenerationWrite,
        ) -> Result<(), GenerationPortError> {
            self.write()
        }

        async fn activate_generation(
            &self,
            _actor: &ActorContext,
            _write: &GenerationProfileVersionWrite,
        ) -> Result<(), GenerationPortError> {
            self.write()
        }

        async fn deactivate_generation(
            &self,
            _actor: &ActorContext,
            _write: &GenerationProfileVersionWrite,
        ) -> Result<(), GenerationPortError> {
            self.write()
        }

        async fn quarantine_generation(
            &self,
            _actor: &ActorContext,
            _write: &QuarantineGenerationWrite,
        ) -> Result<(), GenerationPortError> {
            self.write()
        }

        async fn find_visible_generation(
            &self,
            _scope: &TenantScope,
            _actor_id: &ActorId,
            _role: MembershipRole,
            _profile_id: &ProfileId,
            _generation_id: &GenerationId,
        ) -> Result<Option<GenerationReadModel>, GenerationPortError> {
            Ok(self.visible.borrow().clone())
        }
    }

    impl FakePort {
        fn write(&self) -> Result<(), GenerationPortError> {
            self.writes.set(self.writes.get() + 1);
            match self.write_error.get() {
                Some(class) => Err(GenerationPortError::new(class)),
                None => Ok(()),
            }
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JGENAPP")?),
            ActorId::parse("actor_01JGENAPP")?,
            CorrelationId::parse("corr_01JGENAPP")?,
        ))
    }

    fn evidence() -> Result<CommandExecutionEvidence, Box<dyn std::error::Error>> {
        Ok(CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_01JGENAPP")?,
            "a".repeat(64),
            AuditEventId::parse("audit_01JGENAPP")?,
            OutboxEventId::parse("outbox_01JGENAPP")?,
            UnixMillis::new(10),
            UnixMillis::new(100),
        ))
    }

    fn profile_id() -> Result<ProfileId, Box<dyn std::error::Error>> {
        Ok(ProfileId::parse("profile_01JGENAPP")?)
    }

    fn generation_id() -> Result<GenerationId, Box<dyn std::error::Error>> {
        Ok(GenerationId::parse("generation_01JGENAPP")?)
    }

    #[test]
    fn metadata_validation_matches_legacy_transport() {
        assert_eq!(
            validate_generation_registration(
                "profiles/v1/generation.enc",
                &"a".repeat(64),
                &"b".repeat(64)
            ),
            Ok(())
        );
        assert_eq!(
            validate_generation_registration("../bad", &"a".repeat(64), &"b".repeat(64)),
            Err(GenerationOperationError::InvalidRequest)
        );
        assert_eq!(
            validate_generation_verification_reference("review:generation_01"),
            Ok(())
        );
        assert_eq!(
            validate_generation_verification_reference("review generation"),
            Err(GenerationOperationError::InvalidRequest)
        );
    }

    #[test]
    fn non_owner_mutation_never_reads_replay_or_writes() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![GenerationReplayDecision::Miss]);
        let command = RegisterGenerationCommand {
            profile_id: profile_id()?,
            generation_id: generation_id()?,
            object_key: "profiles/v1/generation.enc".to_owned(),
            metadata_digest: "a".repeat(64),
            container_digest: "b".repeat(64),
            evidence: evidence()?,
        };
        assert_eq!(
            block_on(execute_register_generation(
                &actor()?,
                MembershipRole::Member,
                &port,
                command
            )),
            Err(GenerationOperationError::NotFound)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.writes.get(), 0);
        Ok(())
    }

    #[test]
    fn exact_replay_skips_generation_write() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![GenerationReplayDecision::Replay(
            GenerationReplayReceipt::new("registered", Some("generation_existing".to_owned())),
        )]);
        let command = RegisterGenerationCommand {
            profile_id: profile_id()?,
            generation_id: generation_id()?,
            object_key: "profiles/v1/generation.enc".to_owned(),
            metadata_digest: "a".repeat(64),
            container_digest: "b".repeat(64),
            evidence: evidence()?,
        };
        let outcome = block_on(execute_register_generation(
            &actor()?,
            MembershipRole::TenantOwner,
            &port,
            command,
        ))?;
        assert!(outcome.replayed());
        assert_eq!(outcome.resource_id(), "generation_existing");
        assert_eq!(port.writes.get(), 0);
        Ok(())
    }

    #[test]
    fn version_overflow_fails_before_replay() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![GenerationReplayDecision::Miss]);
        let command = QuarantineGenerationCommand {
            profile_id: profile_id()?,
            generation_id: generation_id()?,
            expected_generation_version: AggregateVersion::new(u64::MAX)?,
            evidence: evidence()?,
        };
        assert_eq!(
            block_on(execute_quarantine_generation(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command
            )),
            Err(GenerationOperationError::InternalFailure)
        );
        assert_eq!(port.replay_calls.get(), 0);
        assert_eq!(port.writes.get(), 0);
        Ok(())
    }

    #[test]
    fn write_conflict_is_not_replayed_after_failure() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(vec![
            GenerationReplayDecision::Miss,
            GenerationReplayDecision::Replay(GenerationReplayReceipt::new("registered", None)),
        ]);
        port.write_error
            .set(Some(GenerationPortErrorClass::Conflict));
        let command = RegisterGenerationCommand {
            profile_id: profile_id()?,
            generation_id: generation_id()?,
            object_key: "profiles/v1/generation.enc".to_owned(),
            metadata_digest: "a".repeat(64),
            container_digest: "b".repeat(64),
            evidence: evidence()?,
        };
        assert_eq!(
            block_on(execute_register_generation(
                &actor()?,
                MembershipRole::TenantOwner,
                &port,
                command
            )),
            Err(GenerationOperationError::Conflict)
        );
        assert_eq!(port.replay_calls.get(), 1);
        assert_eq!(port.writes.get(), 1);
        Ok(())
    }

    #[test]
    fn visible_query_can_project_for_member() -> Result<(), Box<dyn std::error::Error>> {
        let port = FakePort::new(Vec::new());
        port.visible.replace(Some(GenerationReadModel::new(
            generation_id()?,
            "a".repeat(64),
            "b".repeat(64),
            GenerationStatus::Verified,
            AggregateVersion::new(2)?,
            Some("review:generation_01".to_owned()),
        )));
        let result = block_on(get_visible_generation(
            &actor()?,
            MembershipRole::Member,
            &port,
            &profile_id()?,
            &generation_id()?,
        ))?;
        assert_eq!(result.status(), GenerationStatus::Verified);
        Ok(())
    }
}
