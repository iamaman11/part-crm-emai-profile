//! Canonical Capability Policy semantic owner.
//!
//! This crate is provider-free and effect-free. Runtime and release tooling consume typed policy;
//! serialized manifests and environment variables are projections/adapters only.

mod activation;
mod admission;
mod identity;
mod profile;
mod snapshot;
mod surface;

pub use activation::{ALL_ACTIVATION_UNITS, ActivationUnit, CapabilityDefinition};
pub use admission::{AdmissionRequest, AuthorizationState, admit};
pub use identity::{ProfileDigest, profile_digest};
pub use profile::{
    ALL_PROFILE_IDS, ActivationGate, CanonicalEnvironment, CapabilityProfileDefinition,
    EffectiveCapabilities, EffectiveProfile, ProfileId, effective_profile, profile_definition,
};
pub use snapshot::{
    ActivationUnitSnapshotV1, CapabilityPolicySnapshotV1, ProfileSnapshotV1,
    RuntimeSurfaceSnapshotV1, snapshot_v1,
};
pub use surface::{ALL_RUNTIME_SURFACES, RuntimeSurface};

use std::fmt::{Display, Formatter};

/// Transitional adapter for the existing runtime callers on this Draft branch.
/// Removed in the consumer cutover commit; semantic evaluation stays in `admit`.
pub type ProductionAuthorization = AuthorizationState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityProfile {
    pub id: &'static str,
    pub digest: String,
    pub capabilities: EffectiveCapabilities,
}

pub fn admit_profile(
    environment: &str,
    profile_id: &str,
    profile_digest: &str,
    production_authorization: ProductionAuthorization,
) -> Result<CapabilityProfile, PolicyError> {
    let request = AdmissionRequest {
        environment: CanonicalEnvironment::parse(environment)?,
        profile_id: ProfileId::parse(profile_id)?,
        presented_digest: ProfileDigest::parse_hex(profile_digest)?,
        authorization: production_authorization,
    };
    let profile = admit(request)?;
    Ok(CapabilityProfile {
        id: profile.profile_id.id(),
        digest: profile.semantic_digest.to_hex(),
        capabilities: profile.capabilities,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyError {
    UnknownActivationUnit,
    UnknownEnvironment,
    UnknownProfile,
    InvalidDigest,
    DigestMismatch,
    EnvironmentNotAllowed,
    ProductionNotAuthorized,
    ActivationDependencyCycle,
    ActivationSelfDependency { unit: ActivationUnit },
    ActivationSelfIncompatibility { unit: ActivationUnit },
    AsymmetricIncompatibility {
        left: ActivationUnit,
        right: ActivationUnit,
    },
    ProfileInheritanceCycle,
    ProfileEnableDisableOverlap { unit: ActivationUnit },
    DependencyUnsatisfied {
        unit: ActivationUnit,
        dependency: ActivationUnit,
    },
    IncompatibleCapabilities {
        left: ActivationUnit,
        right: ActivationUnit,
    },
}

impl Display for PolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownActivationUnit => formatter.write_str("unknown activation unit"),
            Self::UnknownEnvironment => formatter.write_str("unknown canonical environment"),
            Self::UnknownProfile => formatter.write_str("unknown capability profile"),
            Self::InvalidDigest => formatter.write_str("invalid capability profile digest"),
            Self::DigestMismatch => formatter.write_str("capability profile digest mismatch"),
            Self::EnvironmentNotAllowed => {
                formatter.write_str("capability profile environment not allowed")
            }
            Self::ProductionNotAuthorized => {
                formatter.write_str("production capability profile not authorized")
            }
            Self::ActivationDependencyCycle => {
                formatter.write_str("activation dependency graph contains a cycle")
            }
            Self::ActivationSelfDependency { unit } => {
                write!(formatter, "activation unit {} depends on itself", unit.id())
            }
            Self::ActivationSelfIncompatibility { unit } => {
                write!(formatter, "activation unit {} conflicts with itself", unit.id())
            }
            Self::AsymmetricIncompatibility { left, right } => write!(
                formatter,
                "activation incompatibility is asymmetric: {} -> {}",
                left.id(),
                right.id()
            ),
            Self::ProfileInheritanceCycle => {
                formatter.write_str("capability profile inheritance cycle")
            }
            Self::ProfileEnableDisableOverlap { unit } => write!(
                formatter,
                "capability profile both enables and disables {}",
                unit.id()
            ),
            Self::DependencyUnsatisfied { unit, dependency } => write!(
                formatter,
                "capability dependency unsatisfied: {} requires {}",
                unit.id(),
                dependency.id()
            ),
            Self::IncompatibleCapabilities { left, right } => write!(
                formatter,
                "incompatible capabilities enabled: {} and {}",
                left.id(),
                right.id()
            ),
        }
    }
}

impl std::error::Error for PolicyError {}

pub fn validate_policy() -> Result<(), PolicyError> {
    activation::validate_catalog()?;
    profile::validate_catalog()?;
    surface::validate_catalog();
    Ok(())
}
