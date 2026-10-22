use crate::generations::{
    GenerationOperationError, map_port_error as map_error, validate_generation_registration,
};
use application_ports::generation_initialization::{
    InitialGenerationApplicationPort, InitialGenerationContext, InitialGenerationWrite,
};
use application_ports::generation_objects::{
    GenerationObjectDescriptor, GenerationObjectDescriptorVerifyPort,
};
use application_ports::generations::GenerationReplayDecision;
use identity_access_domain::{
    AuthorizationDecision, Membership, MembershipRole, MembershipStatus, ProfileCapability,
    authorize_profile,
};
use profile_domain::ProfileStatus;
use profile_platform_primitives::{ActorContext, AggregateVersion, GenerationId, ProfileId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialGenerationOutcome {
    pub generation_id: GenerationId,
    pub profile_version: AggregateVersion,
    pub replayed: bool,
}

pub async fn authorize_initial_generation_access<P: InitialGenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: &ProfileId,
    port: &P,
) -> Result<InitialGenerationContext, GenerationOperationError> {
    let context = port
        .initialization_context(actor, profile_id)
        .await
        .map_err(map_error)?
        .ok_or(GenerationOperationError::NotFound)?;
    let membership = Membership::new(
        actor.tenant_scope().tenant_id().clone(),
        actor.actor_id().clone(),
        role,
        MembershipStatus::Active,
    );
    if &context.tenant_id != actor.tenant_scope().tenant_id()
        || &context.profile_id != profile_id
        || authorize_profile(
            actor,
            &membership,
            profile_id,
            context.grant.as_ref(),
            ProfileCapability::Operate,
        ) != AuthorizationDecision::Allowed
    {
        return Err(GenerationOperationError::NotFound);
    }
    Ok(context)
}

pub async fn authorize_initial_generation<P: InitialGenerationApplicationPort>(
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: &ProfileId,
    expected_version: AggregateVersion,
    port: &P,
) -> Result<(), GenerationOperationError> {
    expected_version
        .next()
        .map_err(|_| GenerationOperationError::InvalidRequest)?;
    let context = authorize_initial_generation_access(actor, role, profile_id, port).await?;
    if context.version != expected_version {
        return Err(GenerationOperationError::VersionConflict);
    }
    if context.status != ProfileStatus::Draft || context.active_generation_id.is_some() {
        return Err(GenerationOperationError::InvalidState);
    }
    Ok(())
}

pub fn validate_initial_generation_object(
    actor: &ActorContext,
    object: &GenerationObjectDescriptor,
) -> Result<(), GenerationOperationError> {
    validate_generation_registration(
        object.object_key(),
        object.metadata_digest(),
        object.container_digest(),
    )?;
    let canonical = format!(
        "tenants/{}/profiles/{}/generations/{}.bpgc",
        actor.tenant_scope().tenant_id().as_str(),
        object.profile_id().as_str(),
        object.generation_id().as_str()
    );
    if object.object_key() != canonical || object.container_bytes() == 0 {
        return Err(GenerationOperationError::InvalidRequest);
    }
    Ok(())
}

pub async fn execute_initialize_generation<P, V>(
    actor: &ActorContext,
    role: MembershipRole,
    port: &P,
    verifier: &V,
    write: InitialGenerationWrite,
) -> Result<InitialGenerationOutcome, GenerationOperationError>
where
    P: InitialGenerationApplicationPort,
    V: GenerationObjectDescriptorVerifyPort,
{
    let object = &write.object;
    let expected_version = write.expected_profile_version;
    let evidence = &write.evidence;
    authorize_initial_generation_access(actor, role, object.profile_id(), port).await?;
    validate_initial_generation_object(actor, object)?;
    let next = expected_version
        .next()
        .map_err(|_| GenerationOperationError::InvalidRequest)?;
    match port
        .initialization_replay(actor, evidence)
        .await
        .map_err(map_error)?
    {
        GenerationReplayDecision::Conflict => return Err(GenerationOperationError::Conflict),
        GenerationReplayDecision::Replay(receipt) => {
            if receipt.result_code() != "initialized"
                || receipt.result_reference() != Some(object.generation_id().as_str())
            {
                return Err(GenerationOperationError::IntegrityFailure);
            }
            let context = port
                .initialization_context(actor, object.profile_id())
                .await
                .map_err(map_error)?
                .ok_or(GenerationOperationError::NotFound)?;
            if context.version != next
                || context.status != ProfileStatus::Ready
                || context.active_generation_id.as_ref() != Some(object.generation_id())
            {
                return Err(GenerationOperationError::VersionConflict);
            }
            return Ok(InitialGenerationOutcome {
                generation_id: object.generation_id().clone(),
                profile_version: next,
                replayed: true,
            });
        }
        GenerationReplayDecision::Miss => {}
    }
    authorize_initial_generation(actor, role, object.profile_id(), expected_version, port).await?;
    if !verifier
        .verify_generation_object_descriptor_exact(actor.tenant_scope(), object)
        .await
        .map_err(map_error)?
    {
        return Err(GenerationOperationError::IntegrityFailure);
    }
    let generation_id = object.generation_id().clone();
    port.initialize_generation(actor, &write)
        .await
        .map_err(map_error)?;
    Ok(InitialGenerationOutcome {
        generation_id,
        profile_version: next,
        replayed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use application_ports::{
        CommandExecutionEvidence,
        generations::{GenerationPortError, GenerationReplayReceipt},
    };
    use identity_access_domain::{ProfileGrant, ProfileGrantRole};
    use profile_platform_primitives::*;
    use std::cell::Cell;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    fn run<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut context = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
        }
    }
    struct Port {
        context: InitialGenerationContext,
        replay: GenerationReplayDecision,
        writes: Cell<u32>,
    }
    impl InitialGenerationApplicationPort for Port {
        async fn initialization_context(
            &self,
            _: &ActorContext,
            _: &ProfileId,
        ) -> Result<Option<InitialGenerationContext>, GenerationPortError> {
            Ok(Some(self.context.clone()))
        }
        async fn initialization_replay(
            &self,
            _: &ActorContext,
            _: &CommandExecutionEvidence,
        ) -> Result<GenerationReplayDecision, GenerationPortError> {
            Ok(self.replay.clone())
        }
        async fn initialize_generation(
            &self,
            _: &ActorContext,
            _: &InitialGenerationWrite,
        ) -> Result<(), GenerationPortError> {
            self.writes.set(self.writes.get() + 1);
            Ok(())
        }
    }
    struct Verifier {
        exact: bool,
        calls: Cell<u32>,
    }
    impl GenerationObjectDescriptorVerifyPort for Verifier {
        async fn verify_generation_object_descriptor_exact(
            &self,
            _: &TenantScope,
            _: &GenerationObjectDescriptor,
        ) -> Result<bool, GenerationPortError> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.exact)
        }
    }
    fn fixture()
    -> Result<(ActorContext, Port, InitialGenerationWrite, Verifier), Box<dyn std::error::Error>>
    {
        let tenant = TenantId::parse("tenant_initial_member")?;
        let actor_id = ActorId::parse("actor_initial_member")?;
        let profile = ProfileId::parse("profile_initial_member")?;
        let generation = GenerationId::parse("generation_initial_member")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant.clone()),
            actor_id.clone(),
            CorrelationId::parse("corr_initial_member")?,
        );
        let port = Port {
            context: InitialGenerationContext {
                tenant_id: tenant.clone(),
                profile_id: profile.clone(),
                grant: Some(ProfileGrant::new(
                    tenant.clone(),
                    actor_id,
                    profile.clone(),
                    ProfileGrantRole::Operator,
                )),
                status: ProfileStatus::Draft,
                version: AggregateVersion::new(1)?,
                active_generation_id: None,
            },
            replay: GenerationReplayDecision::Miss,
            writes: Cell::new(0),
        };
        let object = GenerationObjectDescriptor::new(
            profile,
            generation,
            "tenants/tenant_initial_member/profiles/profile_initial_member/generations/generation_initial_member.bpgc",
            "a".repeat(64),
            "b".repeat(64),
            128,
        );
        let evidence = CommandExecutionEvidence::new(
            IdempotencyKey::parse("idem_initial_member")?,
            PayloadFingerprint::parse("c".repeat(64))?,
            AuditEventId::parse("audit_initial_member")?,
            OutboxEventId::parse("outbox_initial_member")?,
            UnixMillis::new(20),
            UnixMillis::new(100),
        );
        Ok((
            actor,
            port,
            InitialGenerationWrite {
                device_id: DeviceId::parse("device_initial_member")?,
                object,
                expected_profile_version: AggregateVersion::new(1)?,
                evidence,
            },
            Verifier {
                exact: true,
                calls: Cell::new(0),
            },
        ))
    }
    #[test]
    fn member_operator_initializes_only_exactly_verified_object()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, port, write, verifier) = fixture()?;
        assert!(
            !run(execute_initialize_generation(
                &actor,
                MembershipRole::Member,
                &port,
                &verifier,
                write
            ))?
            .replayed
        );
        assert_eq!(port.writes.get(), 1);
        assert_eq!(verifier.calls.get(), 1);
        Ok(())
    }
    #[test]
    fn rejected_access_stale_state_and_bad_object_never_write()
    -> Result<(), Box<dyn std::error::Error>> {
        for case in 0..5 {
            let (actor, mut port, write, mut verifier) = fixture()?;
            match case {
                0 => port.context.grant = None,
                1 => {
                    port.context.grant = Some(ProfileGrant::new(
                        port.context.tenant_id.clone(),
                        actor.actor_id().clone(),
                        port.context.profile_id.clone(),
                        ProfileGrantRole::Viewer,
                    ))
                }
                2 => port.context.version = AggregateVersion::new(2)?,
                3 => port.context.status = ProfileStatus::Suspended,
                _ => verifier.exact = false,
            }
            assert!(
                run(execute_initialize_generation(
                    &actor,
                    MembershipRole::Member,
                    &port,
                    &verifier,
                    write
                ))
                .is_err()
            );
            assert_eq!(port.writes.get(), 0);
            assert_eq!(verifier.calls.get(), u32::from(case == 4));
        }
        Ok(())
    }
    #[test]
    fn replay_requires_exact_current_commit_and_cannot_reverify_or_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, mut port, write, verifier) = fixture()?;
        port.replay = GenerationReplayDecision::Replay(GenerationReplayReceipt::new(
            "initialized",
            Some(write.object.generation_id().as_str().to_owned()),
        ));
        port.context.status = ProfileStatus::Ready;
        port.context.version = AggregateVersion::new(2)?;
        port.context.active_generation_id = Some(write.object.generation_id().clone());
        assert!(
            run(execute_initialize_generation(
                &actor,
                MembershipRole::Member,
                &port,
                &verifier,
                write.clone()
            ))?
            .replayed
        );
        port.context.version = AggregateVersion::new(3)?;
        assert_eq!(
            run(execute_initialize_generation(
                &actor,
                MembershipRole::Member,
                &port,
                &verifier,
                write
            )),
            Err(GenerationOperationError::VersionConflict)
        );
        assert_eq!(verifier.calls.get(), 0);
        assert_eq!(port.writes.get(), 0);
        Ok(())
    }
}
