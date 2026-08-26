pub use ::capability_policy::{
    ActivationGate, ActivationUnit, CanonicalEnvironment, CapabilityPolicySnapshotV1,
    EffectiveProfile, PolicyError, ProfileId,
};

#[must_use]
pub fn profile_definition(
    profile_id: ProfileId,
) -> ::capability_policy::CapabilityProfileDefinition {
    ::capability_policy::profile_definition(profile_id)
}

pub fn effective_profile(
    profile_id: ProfileId,
    environment: CanonicalEnvironment,
) -> Result<EffectiveProfile, PolicyError> {
    ::capability_policy::effective_profile(profile_id, environment)
}

#[must_use]
pub fn snapshot_v1() -> CapabilityPolicySnapshotV1 {
    ::capability_policy::snapshot_v1()
}

#[cfg(test)]
mod tests {
    use super::{ActivationUnit, CanonicalEnvironment, ProfileId, effective_profile, snapshot_v1};

    #[test]
    fn release_tooling_consumes_the_typed_policy_owner() {
        assert_eq!(snapshot_v1().profiles.len(), 5);
        assert_eq!(
            effective_profile(ProfileId::RehearsalCoreV1, CanonicalEnvironment::Rehearsal)
                .map(|profile| profile.capabilities.enabled(ActivationUnit::Camoufox)),
            Ok(true)
        );
    }
}
