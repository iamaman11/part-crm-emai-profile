use crate::{ApplicationError, OpenProfileCommand, decide_open_profile};
use application_ports::profile_launch::{
    ProfileLaunchContextPort, ProfileLaunchPortError, ProfileLaunchPortErrorClass,
};
use application_ports::{
    AuthenticatedDevicePort, DeviceExecutionBlocker, DeviceExecutionPreconditionPort,
    DeviceExecutionReadiness, DeviceJobAuthorizationPort, DeviceJobCapability, DeviceJobPortError,
    DeviceJobPortErrorClass, DeviceJobTarget,
};
use contracts::ProblemCode;
use identity_access_domain::{Membership, MembershipRole, MembershipStatus};
use profile_platform_primitives::{ActorContext, DeviceId, GenerationId, ProfileId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedProfileLaunchTarget {
    profile_id: ProfileId,
    generation_id: GenerationId,
    device_id: DeviceId,
}

impl AuthorizedProfileLaunchTarget {
    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
}

pub async fn authorize_profile_launch<C, D, A, P>(
    actor: &ActorContext,
    role: MembershipRole,
    profile_id: &ProfileId,
    context_port: &C,
    authenticated_device: &D,
    device_authorization: &A,
    execution_preconditions: &P,
) -> Result<AuthorizedProfileLaunchTarget, ApplicationError>
where
    C: ProfileLaunchContextPort,
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
{
    let context = context_port
        .load_profile_launch_context(actor.tenant_scope(), actor.actor_id(), profile_id)
        .await
        .map_err(map_launch_context_error)?
        .ok_or_else(|| ApplicationError::new(ProblemCode::NotFound))?;

    let membership = Membership::new(
        actor.tenant_scope().tenant_id().clone(),
        actor.actor_id().clone(),
        role,
        MembershipStatus::Active,
    );
    let device_id = authenticated_device
        .authenticated_device_id(actor)
        .await
        .map_err(map_device_error)?;

    let decision = decide_open_profile(
        actor,
        &membership,
        context.grant(),
        context.profile(),
        OpenProfileCommand::new(profile_id.clone(), device_id.clone()),
    )?;
    let generation_id = context
        .profile()
        .active_generation_id()
        .cloned()
        .ok_or_else(|| ApplicationError::new(ProblemCode::InvalidState))?;
    let target = DeviceJobTarget::new(
        actor.tenant_scope().tenant_id().clone(),
        decision.device_id().clone(),
        decision.profile_id().clone(),
        generation_id.clone(),
    );

    let authorized = device_authorization
        .is_device_job_authorized(actor, &target, DeviceJobCapability::Issue)
        .await
        .map_err(map_device_error)?;
    if !authorized {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    match execution_preconditions
        .evaluate_device_execution(actor, &target)
        .await
        .map_err(map_device_error)?
    {
        DeviceExecutionReadiness::Ready => {}
        DeviceExecutionReadiness::Blocked(DeviceExecutionBlocker::DeviceUnauthorized) => {
            return Err(ApplicationError::new(ProblemCode::NotFound));
        }
        DeviceExecutionReadiness::Blocked(
            DeviceExecutionBlocker::GenerationInactive
            | DeviceExecutionBlocker::CertificationIncomplete,
        ) => return Err(ApplicationError::new(ProblemCode::InvalidState)),
    }

    Ok(AuthorizedProfileLaunchTarget {
        profile_id: decision.profile_id().clone(),
        generation_id,
        device_id: decision.device_id().clone(),
    })
}

fn map_launch_context_error(error: ProfileLaunchPortError) -> ApplicationError {
    ApplicationError::new(match error.class() {
        ProfileLaunchPortErrorClass::IntegrityFailure => ProblemCode::IntegrityFailure,
        ProfileLaunchPortErrorClass::DependencyUnavailable => ProblemCode::DependencyUnavailable,
    })
}

fn map_device_error(error: DeviceJobPortError) -> ApplicationError {
    ApplicationError::new(match error.class() {
        DeviceJobPortErrorClass::AuthenticationFailed => ProblemCode::NotFound,
        DeviceJobPortErrorClass::IntegrityFailure => ProblemCode::IntegrityFailure,
        DeviceJobPortErrorClass::DependencyUnavailable => ProblemCode::DependencyUnavailable,
    })
}

#[cfg(test)]
mod tests {
    use super::authorize_profile_launch;
    use application_ports::profile_launch::{
        ProfileLaunchContext, ProfileLaunchContextPort, ProfileLaunchPortError,
    };
    use application_ports::{
        AuthenticatedDevicePort, DeviceExecutionBlocker, DeviceExecutionPreconditionPort,
        DeviceExecutionReadiness, DeviceJobAuthorizationPort, DeviceJobCapability,
        DeviceJobPortError, DeviceJobTarget,
    };
    use contracts::ProblemCode;
    use identity_access_domain::{MembershipRole, ProfileGrant, ProfileGrantRole};
    use profile_domain::{BrowserProfile, GenerationVerification, ProfileGeneration};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, DeviceId, GenerationId, ProfileId, TenantId,
        TenantScope,
    };
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct LaunchContextFake {
        context: Option<ProfileLaunchContext>,
    }

    impl ProfileLaunchContextPort for LaunchContextFake {
        async fn load_profile_launch_context(
            &self,
            _scope: &TenantScope,
            _actor_id: &ActorId,
            _profile_id: &ProfileId,
        ) -> Result<Option<ProfileLaunchContext>, ProfileLaunchPortError> {
            Ok(self.context.clone())
        }
    }

    struct DeviceFake(DeviceId);

    impl AuthenticatedDevicePort for DeviceFake {
        async fn authenticated_device_id(
            &self,
            _actor: &ActorContext,
        ) -> Result<DeviceId, DeviceJobPortError> {
            Ok(self.0.clone())
        }
    }

    struct AuthorizationFake(bool);

    impl DeviceJobAuthorizationPort for AuthorizationFake {
        async fn is_device_job_authorized(
            &self,
            _actor: &ActorContext,
            _target: &DeviceJobTarget,
            capability: DeviceJobCapability,
        ) -> Result<bool, DeviceJobPortError> {
            assert_eq!(capability, DeviceJobCapability::Issue);
            Ok(self.0)
        }
    }

    struct PreconditionsFake(DeviceExecutionReadiness);

    impl DeviceExecutionPreconditionPort for PreconditionsFake {
        async fn evaluate_device_execution(
            &self,
            _actor: &ActorContext,
            _target: &DeviceJobTarget,
        ) -> Result<DeviceExecutionReadiness, DeviceJobPortError> {
            Ok(self.0)
        }
    }

    fn fixture()
    -> Result<(ActorContext, ProfileId, GenerationId, DeviceId), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JLAUNCH")?;
        Ok((
            ActorContext::new(
                TenantScope::new(tenant_id),
                ActorId::parse("actor_01JLAUNCH")?,
                CorrelationId::parse("corr_01JLAUNCH")?,
            ),
            ProfileId::parse("profile_01JLAUNCH")?,
            GenerationId::parse("generation_01JLAUNCH")?,
            DeviceId::parse("device_01JLAUNCH")?,
        ))
    }

    fn ready_context(
        actor: &ActorContext,
        profile_id: &ProfileId,
        generation_id: &GenerationId,
        grant_role: ProfileGrantRole,
    ) -> Result<ProfileLaunchContext, Box<dyn std::error::Error>> {
        let mut profile =
            BrowserProfile::create(actor.tenant_scope().tenant_id().clone(), profile_id.clone());
        profile.activate_generation(&ProfileGeneration::new(
            actor.tenant_scope().tenant_id().clone(),
            profile_id.clone(),
            generation_id.clone(),
            GenerationVerification::Verified,
        ))?;
        Ok(ProfileLaunchContext::new(
            profile,
            Some(ProfileGrant::new(
                actor.tenant_scope().tenant_id().clone(),
                actor.actor_id().clone(),
                profile_id.clone(),
                grant_role,
            )),
        ))
    }

    #[test]
    fn operator_launch_is_bound_to_server_device_and_active_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, profile_id, generation_id, device_id) = fixture()?;
        let context = LaunchContextFake {
            context: Some(ready_context(
                &actor,
                &profile_id,
                &generation_id,
                ProfileGrantRole::Operator,
            )?),
        };
        let result = block_on(authorize_profile_launch(
            &actor,
            MembershipRole::Member,
            &profile_id,
            &context,
            &DeviceFake(device_id.clone()),
            &AuthorizationFake(true),
            &PreconditionsFake(DeviceExecutionReadiness::Ready),
        ))?;
        assert_eq!(result.profile_id(), &profile_id);
        assert_eq!(result.generation_id(), &generation_id);
        assert_eq!(result.device_id(), &device_id);
        Ok(())
    }

    #[test]
    fn viewer_cannot_launch_even_when_device_authorization_exists()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, profile_id, generation_id, device_id) = fixture()?;
        let context = LaunchContextFake {
            context: Some(ready_context(
                &actor,
                &profile_id,
                &generation_id,
                ProfileGrantRole::Viewer,
            )?),
        };
        let error = block_on(authorize_profile_launch(
            &actor,
            MembershipRole::Member,
            &profile_id,
            &context,
            &DeviceFake(device_id),
            &AuthorizationFake(true),
            &PreconditionsFake(DeviceExecutionReadiness::Ready),
        ))
        .expect_err("viewer launch must fail");
        assert_eq!(error.code(), ProblemCode::NotFound);
        Ok(())
    }

    #[test]
    fn stale_generation_or_unverified_generation_fails_before_authority_issue()
    -> Result<(), Box<dyn std::error::Error>> {
        let (actor, profile_id, generation_id, device_id) = fixture()?;
        let context = LaunchContextFake {
            context: Some(ready_context(
                &actor,
                &profile_id,
                &generation_id,
                ProfileGrantRole::Operator,
            )?),
        };
        for blocker in [
            DeviceExecutionBlocker::GenerationInactive,
            DeviceExecutionBlocker::CertificationIncomplete,
        ] {
            let error = block_on(authorize_profile_launch(
                &actor,
                MembershipRole::Member,
                &profile_id,
                &context,
                &DeviceFake(device_id.clone()),
                &AuthorizationFake(true),
                &PreconditionsFake(DeviceExecutionReadiness::Blocked(blocker)),
            ))
            .expect_err("blocked launch must fail");
            assert_eq!(error.code(), ProblemCode::InvalidState);
        }
        Ok(())
    }
}
