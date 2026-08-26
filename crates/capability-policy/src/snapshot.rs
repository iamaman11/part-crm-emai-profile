use crate::{
    ALL_ACTIVATION_UNITS, ALL_PROFILE_IDS, ALL_RUNTIME_SURFACES, ActivationGate, ActivationUnit,
    CanonicalEnvironment, ProfileDigest, ProfileId, RuntimeSurface, identity, profile_definition,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivationUnitSnapshotV1 {
    pub unit: ActivationUnit,
    pub dependencies: Vec<ActivationUnit>,
    pub incompatible_with: Vec<ActivationUnit>,
    pub requires_windows_profile_bridge: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSnapshotV1 {
    pub profile_id: ProfileId,
    pub profile_version: u16,
    pub semantic_digest: ProfileDigest,
    pub allowed_environments: Vec<CanonicalEnvironment>,
    pub extends: Option<ProfileId>,
    pub enabled_activation_units: Vec<ActivationUnit>,
    pub disabled_activation_units: Vec<ActivationUnit>,
    pub activation_gate: ActivationGate,
    pub production_authorization_required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeSurfaceSnapshotV1 {
    pub surface: RuntimeSurface,
    pub activation_unit: ActivationUnit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityPolicySnapshotV1 {
    pub activation_units: Vec<ActivationUnitSnapshotV1>,
    pub profiles: Vec<ProfileSnapshotV1>,
    pub runtime_surfaces: Vec<RuntimeSurfaceSnapshotV1>,
}

#[must_use]
pub fn snapshot_v1() -> CapabilityPolicySnapshotV1 {
    let activation_units = ALL_ACTIVATION_UNITS
        .iter()
        .copied()
        .map(|unit| ActivationUnitSnapshotV1 {
            unit,
            dependencies: unit.dependencies().to_vec(),
            incompatible_with: unit.incompatible_with().to_vec(),
            requires_windows_profile_bridge: unit.requires_windows_profile_bridge(),
        })
        .collect();
    let profiles = ALL_PROFILE_IDS
        .iter()
        .copied()
        .map(|profile_id| {
            let definition = profile_definition(profile_id);
            ProfileSnapshotV1 {
                profile_id,
                profile_version: definition.version,
                semantic_digest: identity::semantic_digest_v1(definition),
                allowed_environments: definition.allowed_environments.to_vec(),
                extends: definition.extends,
                enabled_activation_units: definition.enabled_activation_units.to_vec(),
                disabled_activation_units: definition.disabled_activation_units.to_vec(),
                activation_gate: definition.activation_gate,
                production_authorization_required: definition.production_authorization_required,
            }
        })
        .collect();
    let runtime_surfaces = ALL_RUNTIME_SURFACES
        .iter()
        .copied()
        .map(|surface| RuntimeSurfaceSnapshotV1 {
            surface,
            activation_unit: surface.activation_unit(),
        })
        .collect();
    CapabilityPolicySnapshotV1 {
        activation_units,
        profiles,
        runtime_surfaces,
    }
}

#[cfg(test)]
mod tests {
    use super::snapshot_v1;
    use crate::{ALL_ACTIVATION_UNITS, ALL_PROFILE_IDS, ALL_RUNTIME_SURFACES};

    #[test]
    fn snapshot_is_a_projection_of_the_typed_owner() {
        let snapshot = snapshot_v1();
        assert_eq!(snapshot.activation_units.len(), ALL_ACTIVATION_UNITS.len());
        assert_eq!(snapshot.profiles.len(), ALL_PROFILE_IDS.len());
        assert_eq!(snapshot.runtime_surfaces.len(), ALL_RUNTIME_SURFACES.len());
    }
}
