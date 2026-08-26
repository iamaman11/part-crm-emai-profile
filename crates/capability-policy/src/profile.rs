use crate::{ActivationUnit, PolicyError, ProfileDigest, identity, validate_policy};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CanonicalEnvironment {
    Rehearsal,
    Staging,
    Production,
}

impl CanonicalEnvironment {
    pub fn parse(value: &str) -> Result<Self, PolicyError> {
        match value {
            "rehearsal" => Ok(Self::Rehearsal),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err(PolicyError::UnknownEnvironment),
        }
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Rehearsal => "rehearsal",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProfileId {
    ProductionCoreV1,
    RehearsalCoreV1,
    ProductionMailboxAdminV1,
    ProductionMailboxJobsV1,
    ProductionOutboundMailV1,
}

pub const ALL_PROFILE_IDS: [ProfileId; 5] = [
    ProfileId::ProductionCoreV1,
    ProfileId::RehearsalCoreV1,
    ProfileId::ProductionMailboxAdminV1,
    ProfileId::ProductionMailboxJobsV1,
    ProfileId::ProductionOutboundMailV1,
];

impl ProfileId {
    pub fn parse(value: &str) -> Result<Self, PolicyError> {
        ALL_PROFILE_IDS
            .iter()
            .copied()
            .find(|profile| profile.id() == value)
            .ok_or(PolicyError::UnknownProfile)
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ProductionCoreV1 => "production-core-v1",
            Self::RehearsalCoreV1 => "rehearsal-core-v1",
            Self::ProductionMailboxAdminV1 => "production-mailbox-admin-v1",
            Self::ProductionMailboxJobsV1 => "production-mailbox-jobs-v1",
            Self::ProductionOutboundMailV1 => "production-outbound-mail-v1",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationGate {
    Ar12OrLaterRehearsal,
    Pc1AfterAr17,
    Pc2,
    Pc3,
    Pc4,
}

impl ActivationGate {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Ar12OrLaterRehearsal => "AR-12_OR_LATER_REHEARSAL",
            Self::Pc1AfterAr17 => "PC-1_AFTER_AR17",
            Self::Pc2 => "PC-2",
            Self::Pc3 => "PC-3",
            Self::Pc4 => "PC-4",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityProfileDefinition {
    pub id: ProfileId,
    pub version: u16,
    pub allowed_environments: &'static [CanonicalEnvironment],
    pub extends: Option<ProfileId>,
    pub enabled_activation_units: &'static [ActivationUnit],
    pub disabled_activation_units: &'static [ActivationUnit],
    pub activation_gate: ActivationGate,
    pub production_authorization_required: bool,
}

const CORE_ENABLED: &[ActivationUnit] = &[
    ActivationUnit::Foundation,
    ActivationUnit::Identity,
    ActivationUnit::Clients,
    ActivationUnit::BrowserProfiles,
    ActivationUnit::ProfileRuntime,
    ActivationUnit::Camoufox,
    ActivationUnit::Notifications,
];
const CORE_DISABLED: &[ActivationUnit] = &[
    ActivationUnit::MailboxAdmin,
    ActivationUnit::MailboxClientBinding,
    ActivationUnit::MailboxBrowserBinding,
    ActivationUnit::MailboxRead,
    ActivationUnit::MailboxJobs,
    ActivationUnit::OutboundMail,
];
const MAILBOX_ADMIN_ENABLED: &[ActivationUnit] = &[
    ActivationUnit::MailboxAdmin,
    ActivationUnit::MailboxClientBinding,
    ActivationUnit::MailboxBrowserBinding,
    ActivationUnit::MailboxRead,
];
const MAILBOX_ADMIN_DISABLED: &[ActivationUnit] =
    &[ActivationUnit::MailboxJobs, ActivationUnit::OutboundMail];
const MAILBOX_JOBS_ENABLED: &[ActivationUnit] = &[ActivationUnit::MailboxJobs];
const OUTBOUND_DISABLED: &[ActivationUnit] = &[ActivationUnit::OutboundMail];
const OUTBOUND_ENABLED: &[ActivationUnit] = &[ActivationUnit::OutboundMail];
const PRODUCTION_ONLY: &[CanonicalEnvironment] = &[CanonicalEnvironment::Production];
const REHEARSAL_ENVIRONMENTS: &[CanonicalEnvironment] = &[
    CanonicalEnvironment::Rehearsal,
    CanonicalEnvironment::Staging,
];

#[must_use]
pub const fn profile_definition(id: ProfileId) -> CapabilityProfileDefinition {
    match id {
        ProfileId::ProductionCoreV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: PRODUCTION_ONLY,
            extends: None,
            enabled_activation_units: CORE_ENABLED,
            disabled_activation_units: CORE_DISABLED,
            activation_gate: ActivationGate::Pc1AfterAr17,
            production_authorization_required: true,
        },
        ProfileId::RehearsalCoreV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: REHEARSAL_ENVIRONMENTS,
            extends: None,
            enabled_activation_units: CORE_ENABLED,
            disabled_activation_units: CORE_DISABLED,
            activation_gate: ActivationGate::Ar12OrLaterRehearsal,
            production_authorization_required: false,
        },
        ProfileId::ProductionMailboxAdminV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: PRODUCTION_ONLY,
            extends: Some(ProfileId::ProductionCoreV1),
            enabled_activation_units: MAILBOX_ADMIN_ENABLED,
            disabled_activation_units: MAILBOX_ADMIN_DISABLED,
            activation_gate: ActivationGate::Pc2,
            production_authorization_required: true,
        },
        ProfileId::ProductionMailboxJobsV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: PRODUCTION_ONLY,
            extends: Some(ProfileId::ProductionMailboxAdminV1),
            enabled_activation_units: MAILBOX_JOBS_ENABLED,
            disabled_activation_units: OUTBOUND_DISABLED,
            activation_gate: ActivationGate::Pc3,
            production_authorization_required: true,
        },
        ProfileId::ProductionOutboundMailV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: PRODUCTION_ONLY,
            extends: Some(ProfileId::ProductionMailboxJobsV1),
            enabled_activation_units: OUTBOUND_ENABLED,
            disabled_activation_units: &[],
            activation_gate: ActivationGate::Pc4,
            production_authorization_required: true,
        },
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveCapabilities(BTreeSet<ActivationUnit>);

impl EffectiveCapabilities {
    #[must_use]
    pub fn enabled(&self, unit: ActivationUnit) -> bool {
        self.0.contains(&unit)
    }

    #[must_use]
    pub fn enabled_units(&self) -> Vec<ActivationUnit> {
        self.0.iter().copied().collect()
    }

    #[must_use]
    pub fn enabled_ids(&self) -> Vec<String> {
        self.0.iter().map(|unit| unit.id().to_owned()).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveProfile {
    pub profile_id: ProfileId,
    pub semantic_digest: ProfileDigest,
    pub capabilities: EffectiveCapabilities,
}

pub fn effective_profile(
    profile_id: ProfileId,
    environment: CanonicalEnvironment,
) -> Result<EffectiveProfile, PolicyError> {
    validate_policy()?;
    effective_profile_validated(profile_id, environment)
}

pub(crate) fn effective_profile_validated(
    profile_id: ProfileId,
    environment: CanonicalEnvironment,
) -> Result<EffectiveProfile, PolicyError> {
    let mut visiting = BTreeSet::new();
    let mut capabilities = BTreeSet::new();
    apply_profile(profile_id, environment, &mut visiting, &mut capabilities)?;
    validate_effective_capabilities(&capabilities)?;
    Ok(EffectiveProfile {
        profile_id,
        semantic_digest: identity::semantic_digest_v1(profile_definition(profile_id)),
        capabilities: EffectiveCapabilities(capabilities),
    })
}

fn apply_profile(
    profile_id: ProfileId,
    environment: CanonicalEnvironment,
    visiting: &mut BTreeSet<ProfileId>,
    capabilities: &mut BTreeSet<ActivationUnit>,
) -> Result<(), PolicyError> {
    if !visiting.insert(profile_id) {
        return Err(PolicyError::ProfileInheritanceCycle);
    }
    let definition = profile_definition(profile_id);
    if !definition.allowed_environments.contains(&environment) {
        visiting.remove(&profile_id);
        return Err(PolicyError::EnvironmentNotAllowed);
    }
    if let Some(parent) = definition.extends {
        apply_profile(parent, environment, visiting, capabilities)?;
    }
    for unit in definition.enabled_activation_units {
        capabilities.insert(*unit);
    }
    for unit in definition.disabled_activation_units {
        capabilities.remove(unit);
    }
    visiting.remove(&profile_id);
    Ok(())
}

fn validate_effective_capabilities(
    capabilities: &BTreeSet<ActivationUnit>,
) -> Result<(), PolicyError> {
    for unit in capabilities {
        for dependency in unit.dependencies() {
            if !capabilities.contains(dependency) {
                return Err(PolicyError::DependencyUnsatisfied {
                    unit: *unit,
                    dependency: *dependency,
                });
            }
        }
        for incompatible in unit.incompatible_with() {
            if capabilities.contains(incompatible) {
                return Err(PolicyError::IncompatibleCapabilities {
                    left: *unit,
                    right: *incompatible,
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_catalog() -> Result<(), PolicyError> {
    let mut ids = BTreeSet::new();
    for profile_id in ALL_PROFILE_IDS {
        if !ids.insert(profile_id.id()) {
            return Err(PolicyError::ProfileInheritanceCycle);
        }
        let definition = profile_definition(profile_id);
        for unit in definition.enabled_activation_units {
            if definition.disabled_activation_units.contains(unit) {
                return Err(PolicyError::ProfileEnableDisableOverlap { unit: *unit });
            }
        }
        let mut visiting = BTreeSet::new();
        validate_inheritance(profile_id, &mut visiting)?;
        for environment in definition.allowed_environments {
            let mut visiting = BTreeSet::new();
            let mut capabilities = BTreeSet::new();
            apply_profile(
                profile_id,
                *environment,
                &mut visiting,
                &mut capabilities,
            )?;
            validate_effective_capabilities(&capabilities)?;
        }
    }
    Ok(())
}

fn validate_inheritance(
    profile_id: ProfileId,
    visiting: &mut BTreeSet<ProfileId>,
) -> Result<(), PolicyError> {
    if !visiting.insert(profile_id) {
        return Err(PolicyError::ProfileInheritanceCycle);
    }
    if let Some(parent) = profile_definition(profile_id).extends {
        validate_inheritance(parent, visiting)?;
    }
    visiting.remove(&profile_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ALL_PROFILE_IDS, CanonicalEnvironment, ProfileId, effective_profile, profile_definition,
        validate_catalog,
    };
    use crate::ActivationUnit;
    use std::collections::BTreeSet;

    #[test]
    fn profile_catalog_is_unique_valid_and_acyclic() {
        assert!(validate_catalog().is_ok());
        let ids: BTreeSet<&str> = ALL_PROFILE_IDS.iter().map(|profile| profile.id()).collect();
        assert_eq!(ids.len(), ALL_PROFILE_IDS.len());
    }

    #[test]
    fn profile_ids_round_trip() {
        for profile in ALL_PROFILE_IDS {
            assert_eq!(ProfileId::parse(profile.id()), Ok(profile));
        }
    }

    #[test]
    fn rehearsal_core_excludes_mail() {
        let result = effective_profile(ProfileId::RehearsalCoreV1, CanonicalEnvironment::Staging);
        assert!(result.is_ok());
        if let Ok(profile) = result {
            assert!(profile.capabilities.enabled(ActivationUnit::Clients));
            assert!(!profile.capabilities.enabled(ActivationUnit::MailboxAdmin));
        }
    }

    #[test]
    fn typed_activation_gate_is_not_digest_state() {
        assert_eq!(
            profile_definition(ProfileId::ProductionCoreV1)
                .activation_gate
                .id(),
            "PC-1_AFTER_AR17"
        );
    }
}
