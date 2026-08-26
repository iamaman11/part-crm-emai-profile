use crate::{
    CanonicalEnvironment, EffectiveProfile, PolicyError, ProfileDigest, ProfileId, identity,
    profile, profile_definition, validate_policy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationState {
    NotAuthorized,
    Authorized,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmissionRequest {
    pub environment: CanonicalEnvironment,
    pub profile_id: ProfileId,
    pub presented_digest: ProfileDigest,
    pub authorization: AuthorizationState,
}

pub fn admit(request: AdmissionRequest) -> Result<EffectiveProfile, PolicyError> {
    validate_policy()?;
    let definition = profile_definition(request.profile_id);
    let expected_digest = identity::semantic_digest_v1(definition);
    if request.presented_digest != expected_digest {
        return Err(PolicyError::DigestMismatch);
    }
    if !definition
        .allowed_environments
        .contains(&request.environment)
    {
        return Err(PolicyError::EnvironmentNotAllowed);
    }
    if request.environment == CanonicalEnvironment::Production
        && definition.production_authorization_required
        && request.authorization != AuthorizationState::Authorized
    {
        return Err(PolicyError::ProductionNotAuthorized);
    }
    profile::effective_profile_validated(request.profile_id, request.environment)
}

#[cfg(test)]
mod tests {
    use super::{AdmissionRequest, AuthorizationState, admit};
    use crate::identity::semantic_digest_v1;
    use crate::{CanonicalEnvironment, PolicyError, ProfileDigest, ProfileId, profile_definition};

    #[test]
    fn production_fails_closed_without_authorization() {
        let definition = profile_definition(ProfileId::ProductionCoreV1);
        assert_eq!(
            admit(AdmissionRequest {
                environment: CanonicalEnvironment::Production,
                profile_id: ProfileId::ProductionCoreV1,
                presented_digest: semantic_digest_v1(definition),
                authorization: AuthorizationState::NotAuthorized,
            }),
            Err(PolicyError::ProductionNotAuthorized)
        );
    }

    #[test]
    fn wrong_digest_and_wrong_environment_fail_closed() {
        let wrong_digest = ProfileDigest::parse_hex(
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(wrong_digest.is_ok());
        if let Ok(wrong_digest) = wrong_digest {
            assert_eq!(
                admit(AdmissionRequest {
                    environment: CanonicalEnvironment::Staging,
                    profile_id: ProfileId::RehearsalCoreV1,
                    presented_digest: wrong_digest,
                    authorization: AuthorizationState::NotAuthorized,
                }),
                Err(PolicyError::DigestMismatch)
            );
        }

        let definition = profile_definition(ProfileId::RehearsalCoreV1);
        assert_eq!(
            admit(AdmissionRequest {
                environment: CanonicalEnvironment::Production,
                profile_id: ProfileId::RehearsalCoreV1,
                presented_digest: semantic_digest_v1(definition),
                authorization: AuthorizationState::NotAuthorized,
            }),
            Err(PolicyError::EnvironmentNotAllowed)
        );
    }

    #[test]
    fn rehearsal_profile_is_admitted_in_staging() {
        let definition = profile_definition(ProfileId::RehearsalCoreV1);
        assert!(
            admit(AdmissionRequest {
                environment: CanonicalEnvironment::Staging,
                profile_id: ProfileId::RehearsalCoreV1,
                presented_digest: semantic_digest_v1(definition),
                authorization: AuthorizationState::NotAuthorized,
            })
            .is_ok()
        );
    }
}
