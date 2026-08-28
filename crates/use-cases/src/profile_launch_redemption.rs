use crate::profile_launch::authorize_profile_launch;
use crate::ApplicationError;
use application_ports::identity::{
    ActiveMembershipPort, ActiveMembershipPortError, ActiveMembershipPortErrorClass,
};
use application_ports::profile_launch::{
    ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError, ProfileLaunchAuthorityErrorClass,
    ProfileLaunchAuthorityPort, ProfileLaunchContextPort,
};
use application_ports::{
    AuthenticatedDevicePort, DeviceExecutionPreconditionPort, DeviceJobAuthorizationPort,
};
use contracts::ProblemCode;
use profile_platform_primitives::{CorrelationId, DeviceId, TenantScope, UnixMillis};

/// Redeem a launch authority after the Bridge-facing ingress has authenticated the local machine.
///
/// The claim is deliberately inspected without mutation first. Current membership, profile/grant,
/// server-owned device selection, active generation, device authorization and execution
/// preconditions are then revalidated through the same canonical launch authorization use-case
/// used at issuance. Only an exact unchanged binding may reach the final one-time CAS consume.
pub async fn redeem_profile_launch_authority<M, L, C, D, A, P>(
    correlation_id: &CorrelationId,
    claim_code: &str,
    authenticated_machine_device_id: &DeviceId,
    now: UnixMillis,
    memberships: &M,
    authority: &L,
    context_port: &C,
    authenticated_device: &D,
    device_authorization: &A,
    execution_preconditions: &P,
) -> Result<ProfileLaunchAuthorityBinding, ApplicationError>
where
    M: ActiveMembershipPort,
    L: ProfileLaunchAuthorityPort,
    C: ProfileLaunchContextPort,
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
{
    let binding = authority
        .inspect_profile_launch_authority(claim_code, authenticated_machine_device_id, now)
        .await
        .map_err(map_redemption_authority_error)?;

    if binding.device_id() != authenticated_machine_device_id {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    let scope = TenantScope::new(binding.tenant_id().clone());
    let role = memberships
        .active_membership_role(&scope, binding.actor_id())
        .await
        .map_err(map_membership_error)?
        .ok_or_else(|| ApplicationError::new(ProblemCode::NotFound))?;
    let actor = profile_platform_primitives::ActorContext::new(
        scope,
        binding.actor_id().clone(),
        correlation_id.clone(),
    );

    let target = authorize_profile_launch(
        &actor,
        role,
        binding.profile_id(),
        context_port,
        authenticated_device,
        device_authorization,
        execution_preconditions,
    )
    .await?;

    if target.profile_id() != binding.profile_id()
        || target.generation_id() != binding.generation_id()
        || target.device_id() != binding.device_id()
    {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    let consumed = authority
        .consume_profile_launch_authority(claim_code, authenticated_machine_device_id, now)
        .await
        .map_err(map_redemption_authority_error)?;
    if consumed != binding {
        return Err(ApplicationError::new(ProblemCode::IntegrityFailure));
    }
    Ok(consumed)
}

fn map_membership_error(error: ActiveMembershipPortError) -> ApplicationError {
    ApplicationError::new(match error.class() {
        ActiveMembershipPortErrorClass::IntegrityFailure => ProblemCode::IntegrityFailure,
        ActiveMembershipPortErrorClass::DependencyUnavailable => ProblemCode::DependencyUnavailable,
    })
}

fn map_redemption_authority_error(error: ProfileLaunchAuthorityError) -> ApplicationError {
    ApplicationError::new(match error.class() {
        // Redemption is deliberately neutral for malformed, absent, expired, replayed, or
        // device-mismatched bearer material. The Bridge receives no claim-existence oracle.
        ProfileLaunchAuthorityErrorClass::Conflict
        | ProfileLaunchAuthorityErrorClass::NotFound
        | ProfileLaunchAuthorityErrorClass::ReplayRejected => ProblemCode::NotFound,
        ProfileLaunchAuthorityErrorClass::IntegrityFailure => ProblemCode::IntegrityFailure,
        ProfileLaunchAuthorityErrorClass::DependencyUnavailable => ProblemCode::DependencyUnavailable,
    })
}
