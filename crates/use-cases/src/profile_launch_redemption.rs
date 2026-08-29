use crate::ApplicationError;
use crate::profile_launch::authorize_profile_launch;
use application_ports::identity::{
    ActiveMembershipPort, ActiveMembershipPortError, ActiveMembershipPortErrorClass,
};
use application_ports::profile_launch::{
    ProfileLaunchAuthorityBinding, ProfileLaunchAuthorityError, ProfileLaunchAuthorityErrorClass,
    ProfileLaunchAuthorityPort, ProfileLaunchContextPort, ProfileLaunchMachineBinding,
};
use application_ports::{
    AuthenticatedDevicePort, DeviceExecutionPreconditionPort, DeviceJobAuthorizationPort,
};
use contracts::ProblemCode;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{ActorContext, CorrelationId, TenantScope, UnixMillis};

pub struct ProfileLaunchRedemptionInput<'a> {
    correlation_id: &'a CorrelationId,
    claim_code: &'a str,
    authenticated_machine: &'a ProfileLaunchMachineBinding,
    now: UnixMillis,
}

impl<'a> ProfileLaunchRedemptionInput<'a> {
    #[must_use]
    pub const fn new(
        correlation_id: &'a CorrelationId,
        claim_code: &'a str,
        authenticated_machine: &'a ProfileLaunchMachineBinding,
        now: UnixMillis,
    ) -> Self {
        Self {
            correlation_id,
            claim_code,
            authenticated_machine,
            now,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedProfileLaunchRedemption {
    binding: ProfileLaunchAuthorityBinding,
    actor: ActorContext,
    role: MembershipRole,
}

impl ValidatedProfileLaunchRedemption {
    #[must_use]
    pub const fn binding(&self) -> &ProfileLaunchAuthorityBinding {
        &self.binding
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorContext {
        &self.actor
    }

    #[must_use]
    pub const fn role(&self) -> MembershipRole {
        self.role
    }
}

/// Validate a still-live launch authority after the Bridge-facing ingress has authenticated the
/// actual machine. This phase is deliberately read-only: it proves the exact machine binding and
/// reuses the canonical launch authorization/precondition owners without consuming the bearer.
pub async fn validate_profile_launch_redemption<M, L, C, D, A, P>(
    input: ProfileLaunchRedemptionInput<'_>,
    memberships: &M,
    authority: &L,
    context_port: &C,
    authenticated_device: &D,
    device_authorization: &A,
    execution_preconditions: &P,
) -> Result<ValidatedProfileLaunchRedemption, ApplicationError>
where
    M: ActiveMembershipPort,
    L: ProfileLaunchAuthorityPort,
    C: ProfileLaunchContextPort,
    D: AuthenticatedDevicePort,
    A: DeviceJobAuthorizationPort,
    P: DeviceExecutionPreconditionPort,
{
    let binding = authority
        .inspect_profile_launch_authority(
            input.claim_code,
            input.authenticated_machine.device_id(),
            input.now,
        )
        .await
        .map_err(map_redemption_authority_error)?;

    if binding.tenant_id() != input.authenticated_machine.tenant_id()
        || binding.actor_id() != input.authenticated_machine.actor_id()
        || binding.device_id() != input.authenticated_machine.device_id()
    {
        return Err(ApplicationError::new(ProblemCode::NotFound));
    }

    let scope = TenantScope::new(binding.tenant_id().clone());
    let role = memberships
        .active_membership_role(&scope, binding.actor_id())
        .await
        .map_err(map_membership_error)?
        .ok_or_else(|| ApplicationError::new(ProblemCode::NotFound))?;
    let actor = ActorContext::new(
        scope,
        binding.actor_id().clone(),
        input.correlation_id.clone(),
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

    Ok(ValidatedProfileLaunchRedemption {
        binding,
        actor,
        role,
    })
}

/// Final one-time consume after all security-sensitive current state has been revalidated and the
/// bounded coordinator continuation has been prepared. The consumed binding must be byte-for-byte
/// equivalent to the validated authority or the operation fails closed.
pub async fn consume_validated_profile_launch_redemption<L: ProfileLaunchAuthorityPort>(
    validated: &ValidatedProfileLaunchRedemption,
    claim_code: &str,
    now: UnixMillis,
    authority: &L,
) -> Result<ProfileLaunchAuthorityBinding, ApplicationError> {
    let consumed = authority
        .consume_profile_launch_authority(claim_code, validated.binding().device_id(), now)
        .await
        .map_err(map_redemption_authority_error)?;
    if &consumed != validated.binding() {
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
        // machine/device-mismatched bearer material. The Bridge receives no claim-existence oracle.
        ProfileLaunchAuthorityErrorClass::Conflict
        | ProfileLaunchAuthorityErrorClass::NotFound
        | ProfileLaunchAuthorityErrorClass::ReplayRejected => ProblemCode::NotFound,
        ProfileLaunchAuthorityErrorClass::IntegrityFailure => ProblemCode::IntegrityFailure,
        ProfileLaunchAuthorityErrorClass::DependencyUnavailable => {
            ProblemCode::DependencyUnavailable
        }
    })
}
