use crate::ApplicationError;
use crate::profile_launch::AuthorizedProfileLaunchTarget;
use application_ports::CommandExecutionEvidence;
use application_ports::profile_launch::{
    IssuedProfileLaunchAuthority, ProfileLaunchAuthorityError, ProfileLaunchAuthorityErrorClass,
    ProfileLaunchAuthorityPort,
};
use contracts::ProblemCode;
use profile_platform_primitives::ActorContext;

pub async fn issue_profile_launch_authority<A: ProfileLaunchAuthorityPort>(
    actor: &ActorContext,
    target: &AuthorizedProfileLaunchTarget,
    evidence: &CommandExecutionEvidence,
    authority: &A,
) -> Result<IssuedProfileLaunchAuthority, ApplicationError> {
    authority
        .issue_profile_launch_authority(
            actor,
            target.profile_id(),
            target.generation_id(),
            target.device_id(),
            evidence,
        )
        .await
        .map_err(map_authority_error)
}

fn map_authority_error(error: ProfileLaunchAuthorityError) -> ApplicationError {
    ApplicationError::new(match error.class() {
        ProfileLaunchAuthorityErrorClass::Conflict => ProblemCode::VersionConflict,
        ProfileLaunchAuthorityErrorClass::NotFound => ProblemCode::NotFound,
        ProfileLaunchAuthorityErrorClass::ReplayRejected => ProblemCode::ReplayRejected,
        ProfileLaunchAuthorityErrorClass::IntegrityFailure => ProblemCode::IntegrityFailure,
        ProfileLaunchAuthorityErrorClass::DependencyUnavailable => ProblemCode::DependencyUnavailable,
    })
}
