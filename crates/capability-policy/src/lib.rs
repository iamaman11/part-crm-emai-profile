//! Canonical Capability Policy semantic owner.
//!
//! This crate is provider-free and effect-free. Product Runtime and release tooling consume these
//! typed definitions; serialized manifests and environment variables are projections/adapters only.

use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ActivationUnit {
    Foundation,
    Identity,
    Clients,
    BrowserProfiles,
    ProfileRuntime,
    Camoufox,
    Notifications,
    MailboxAdmin,
    MailboxClientBinding,
    MailboxBrowserBinding,
    MailboxRead,
    MailboxJobs,
    OutboundMail,
}

pub const ALL_ACTIVATION_UNITS: [ActivationUnit; 13] = [
    ActivationUnit::Foundation,
    ActivationUnit::Identity,
    ActivationUnit::Clients,
    ActivationUnit::BrowserProfiles,
    ActivationUnit::ProfileRuntime,
    ActivationUnit::Camoufox,
    ActivationUnit::Notifications,
    ActivationUnit::MailboxAdmin,
    ActivationUnit::MailboxClientBinding,
    ActivationUnit::MailboxBrowserBinding,
    ActivationUnit::MailboxRead,
    ActivationUnit::MailboxJobs,
    ActivationUnit::OutboundMail,
];

impl ActivationUnit {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Foundation => "foundation",
            Self::Identity => "identity",
            Self::Clients => "clients",
            Self::BrowserProfiles => "browser_profiles",
            Self::ProfileRuntime => "profile_runtime",
            Self::Camoufox => "camoufox",
            Self::Notifications => "notifications",
            Self::MailboxAdmin => "mailbox_admin",
            Self::MailboxClientBinding => "mailbox_client_binding",
            Self::MailboxBrowserBinding => "mailbox_browser_binding",
            Self::MailboxRead => "mailbox_read",
            Self::MailboxJobs => "mailbox_jobs",
            Self::OutboundMail => "outbound_mail",
        }
    }

    #[must_use]
    pub const fn dependencies(self) -> &'static [ActivationUnit] {
        match self {
            Self::Foundation => &[],
            Self::Identity => &[Self::Foundation],
            Self::Clients => &[Self::Foundation, Self::Identity],
            Self::BrowserProfiles => &[Self::Foundation, Self::Identity, Self::Clients],
            Self::ProfileRuntime => &[Self::Foundation, Self::Identity, Self::BrowserProfiles],
            Self::Camoufox => &[Self::ProfileRuntime],
            Self::Notifications => &[Self::Foundation, Self::Identity],
            Self::MailboxAdmin => &[Self::Foundation, Self::Identity],
            Self::MailboxClientBinding => &[Self::MailboxAdmin, Self::Clients],
            Self::MailboxBrowserBinding => &[
                Self::MailboxAdmin,
                Self::BrowserProfiles,
                Self::ProfileRuntime,
            ],
            Self::MailboxRead => &[Self::MailboxAdmin, Self::Clients],
            Self::MailboxJobs => &[Self::MailboxAdmin],
            Self::OutboundMail => &[
                Self::MailboxAdmin,
                Self::MailboxClientBinding,
                Self::Clients,
            ],
        }
    }

    #[must_use]
    pub const fn incompatible_with(self) -> &'static [ActivationUnit] {
        &[]
    }

    #[must_use]
    pub const fn requires_windows_profile_bridge(self) -> bool {
        matches!(
            self,
            Self::ProfileRuntime | Self::Camoufox | Self::MailboxBrowserBinding
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductionAuthorization {
    NotAuthorized,
    Authorized,
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
        match value {
            "production-core-v1" => Ok(Self::ProductionCoreV1),
            "rehearsal-core-v1" => Ok(Self::RehearsalCoreV1),
            "production-mailbox-admin-v1" => Ok(Self::ProductionMailboxAdminV1),
            "production-mailbox-jobs-v1" => Ok(Self::ProductionMailboxJobsV1),
            "production-outbound-mail-v1" => Ok(Self::ProductionOutboundMailV1),
            _ => Err(PolicyError::UnknownProfile),
        }
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
pub struct CapabilityProfileDefinition {
    pub id: ProfileId,
    pub version: u16,
    pub allowed_environments: &'static [CanonicalEnvironment],
    pub extends: Option<ProfileId>,
    pub enabled_activation_units: &'static [ActivationUnit],
    pub disabled_activation_units: &'static [ActivationUnit],
    pub activation_gate: &'static str,
    pub production_authorization_required: bool,
    pub digest: &'static str,
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
            activation_gate: "PC-1_AFTER_AR17",
            production_authorization_required: true,
            digest: "92ccb88e7b74c89e4f39a5349eb5bf0da6a2d6f9ccc4a89d72ab462cb08e0868",
        },
        ProfileId::RehearsalCoreV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: REHEARSAL_ENVIRONMENTS,
            extends: None,
            enabled_activation_units: CORE_ENABLED,
            disabled_activation_units: CORE_DISABLED,
            activation_gate: "AR-12_OR_LATER_REHEARSAL",
            production_authorization_required: false,
            digest: "40ebe3bc1d890757f00433d0ff814720be1ffcd691fff35aea5244a05fc1f45a",
        },
        ProfileId::ProductionMailboxAdminV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: PRODUCTION_ONLY,
            extends: Some(ProfileId::ProductionCoreV1),
            enabled_activation_units: MAILBOX_ADMIN_ENABLED,
            disabled_activation_units: MAILBOX_ADMIN_DISABLED,
            activation_gate: "PC-2",
            production_authorization_required: true,
            digest: "ede6abdcdeb98738855e7fc2309788625ecbce03531b71644413ba69dceaf939",
        },
        ProfileId::ProductionMailboxJobsV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: PRODUCTION_ONLY,
            extends: Some(ProfileId::ProductionMailboxAdminV1),
            enabled_activation_units: MAILBOX_JOBS_ENABLED,
            disabled_activation_units: OUTBOUND_DISABLED,
            activation_gate: "PC-3",
            production_authorization_required: true,
            digest: "a95ad429c73bc3415d8991ec15d390c4be94daec88a7700f473e2325fa3470ef",
        },
        ProfileId::ProductionOutboundMailV1 => CapabilityProfileDefinition {
            id,
            version: 1,
            allowed_environments: PRODUCTION_ONLY,
            extends: Some(ProfileId::ProductionMailboxJobsV1),
            enabled_activation_units: OUTBOUND_ENABLED,
            disabled_activation_units: &[],
            activation_gate: "PC-4",
            production_authorization_required: true,
            digest: "da2a883eba9fa706d6e9adfe87265d3b56a842cd7636877874dce3e86d3bf014",
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveCapabilities(u16);

impl EffectiveCapabilities {
    #[must_use]
    pub const fn enabled(self, unit: ActivationUnit) -> bool {
        self.0 & (1_u16 << (unit as u16)) != 0
    }

    #[must_use]
    pub fn enabled_ids(self) -> Vec<String> {
        ALL_ACTIVATION_UNITS
            .iter()
            .copied()
            .filter(|unit| self.enabled(*unit))
            .map(ActivationUnit::id)
            .map(str::to_owned)
            .collect()
    }

    fn set(&mut self, unit: ActivationUnit, enabled: bool) {
        let bit = 1_u16 << (unit as u16);
        if enabled {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityProfile {
    pub id: &'static str,
    pub digest: &'static str,
    pub capabilities: EffectiveCapabilities,
}

pub fn effective_profile(
    profile_id: ProfileId,
    environment: CanonicalEnvironment,
) -> Result<CapabilityProfile, PolicyError> {
    let mut visiting = BTreeSet::new();
    let mut capabilities = EffectiveCapabilities(0);
    apply_profile(profile_id, environment, &mut visiting, &mut capabilities)?;
    validate_effective_dependencies(capabilities)?;
    let definition = profile_definition(profile_id);
    Ok(CapabilityProfile {
        id: definition.id.id(),
        digest: definition.digest,
        capabilities,
    })
}

fn apply_profile(
    profile_id: ProfileId,
    environment: CanonicalEnvironment,
    visiting: &mut BTreeSet<ProfileId>,
    capabilities: &mut EffectiveCapabilities,
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
        capabilities.set(*unit, true);
    }
    for unit in definition.disabled_activation_units {
        capabilities.set(*unit, false);
    }
    visiting.remove(&profile_id);
    Ok(())
}

fn validate_effective_dependencies(capabilities: EffectiveCapabilities) -> Result<(), PolicyError> {
    for unit in ALL_ACTIVATION_UNITS {
        if !capabilities.enabled(unit) {
            continue;
        }
        for dependency in unit.dependencies() {
            if !capabilities.enabled(*dependency) {
                return Err(PolicyError::DependencyUnsatisfied);
            }
        }
        for incompatible in unit.incompatible_with() {
            if capabilities.enabled(*incompatible) {
                return Err(PolicyError::IncompatibleCapabilities);
            }
        }
    }
    Ok(())
}

pub fn admit_profile(
    environment: &str,
    profile_id: &str,
    profile_digest: &str,
    production_authorization: ProductionAuthorization,
) -> Result<CapabilityProfile, PolicyError> {
    let environment = CanonicalEnvironment::parse(environment)?;
    let profile_id = ProfileId::parse(profile_id)?;
    let definition = profile_definition(profile_id);
    if definition.digest != profile_digest {
        return Err(PolicyError::DigestMismatch);
    }
    let profile = effective_profile(profile_id, environment)?;
    if environment == CanonicalEnvironment::Production
        && definition.production_authorization_required
        && production_authorization != ProductionAuthorization::Authorized
    {
        return Err(PolicyError::ProductionNotAuthorized);
    }
    Ok(profile)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RuntimeSurface {
    HttpHealth,
    HttpBindings,
    HttpSession,
    HttpIdentity,
    HttpClients,
    HttpClientMailRead,
    HttpOutboundMail,
    HttpBrowserProfiles,
    HttpMailboxAdmin,
    HttpMailboxClientBinding,
    HttpMailboxBrowserBinding,
    HttpMailboxJobs,
    HttpProfileRuntimeDeviceJobs,
    HttpNotifications,
    QueueIntegrationEvents,
    QueueMailboxJobs,
    ScheduleIntegrationEvents,
    ScheduleMailboxJobs,
    ResolverIngress,
    ResolverReconciliation,
    BridgeProfileRuntimeCommands,
    BridgeCamoufoxLaunch,
}

pub const ALL_RUNTIME_SURFACES: [RuntimeSurface; 22] = [
    RuntimeSurface::HttpHealth,
    RuntimeSurface::HttpBindings,
    RuntimeSurface::HttpSession,
    RuntimeSurface::HttpIdentity,
    RuntimeSurface::HttpClients,
    RuntimeSurface::HttpClientMailRead,
    RuntimeSurface::HttpOutboundMail,
    RuntimeSurface::HttpBrowserProfiles,
    RuntimeSurface::HttpMailboxAdmin,
    RuntimeSurface::HttpMailboxClientBinding,
    RuntimeSurface::HttpMailboxBrowserBinding,
    RuntimeSurface::HttpMailboxJobs,
    RuntimeSurface::HttpProfileRuntimeDeviceJobs,
    RuntimeSurface::HttpNotifications,
    RuntimeSurface::QueueIntegrationEvents,
    RuntimeSurface::QueueMailboxJobs,
    RuntimeSurface::ScheduleIntegrationEvents,
    RuntimeSurface::ScheduleMailboxJobs,
    RuntimeSurface::ResolverIngress,
    RuntimeSurface::ResolverReconciliation,
    RuntimeSurface::BridgeProfileRuntimeCommands,
    RuntimeSurface::BridgeCamoufoxLaunch,
];

impl RuntimeSurface {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::HttpHealth => "http.health",
            Self::HttpBindings => "http.bindings",
            Self::HttpSession => "http.session",
            Self::HttpIdentity => "http.identity",
            Self::HttpClients => "http.clients",
            Self::HttpClientMailRead => "http.client_mail_read",
            Self::HttpOutboundMail => "http.outbound_mail",
            Self::HttpBrowserProfiles => "http.browser_profiles",
            Self::HttpMailboxAdmin => "http.mailbox_admin",
            Self::HttpMailboxClientBinding => "http.mailbox_client_binding",
            Self::HttpMailboxBrowserBinding => "http.mailbox_browser_binding",
            Self::HttpMailboxJobs => "http.mailbox_jobs",
            Self::HttpProfileRuntimeDeviceJobs => "http.profile_runtime_device_jobs",
            Self::HttpNotifications => "http.notifications",
            Self::QueueIntegrationEvents => "queue.integration_events.consumer",
            Self::QueueMailboxJobs => "queue.mailbox_jobs.consumer",
            Self::ScheduleIntegrationEvents => "schedule.integration_events.dispatcher",
            Self::ScheduleMailboxJobs => "schedule.mailbox_jobs.dispatcher",
            Self::ResolverIngress => "service.mailbox_secret_resolver.ingress",
            Self::ResolverReconciliation => "schedule.mailbox_secret_resolver.reconciliation",
            Self::BridgeProfileRuntimeCommands => "bridge.profile_runtime.commands",
            Self::BridgeCamoufoxLaunch => "bridge.camoufox.launch",
        }
    }

    #[must_use]
    pub const fn activation_unit(self) -> ActivationUnit {
        match self {
            Self::HttpHealth | Self::HttpBindings | Self::HttpSession => ActivationUnit::Foundation,
            Self::HttpIdentity => ActivationUnit::Identity,
            Self::HttpClients => ActivationUnit::Clients,
            Self::HttpClientMailRead => ActivationUnit::MailboxRead,
            Self::HttpOutboundMail => ActivationUnit::OutboundMail,
            Self::HttpBrowserProfiles => ActivationUnit::BrowserProfiles,
            Self::HttpMailboxAdmin | Self::ResolverIngress | Self::ResolverReconciliation => {
                ActivationUnit::MailboxAdmin
            }
            Self::HttpMailboxClientBinding => ActivationUnit::MailboxClientBinding,
            Self::HttpMailboxBrowserBinding => ActivationUnit::MailboxBrowserBinding,
            Self::HttpMailboxJobs | Self::QueueMailboxJobs | Self::ScheduleMailboxJobs => {
                ActivationUnit::MailboxJobs
            }
            Self::HttpProfileRuntimeDeviceJobs | Self::BridgeProfileRuntimeCommands => {
                ActivationUnit::ProfileRuntime
            }
            Self::HttpNotifications
            | Self::QueueIntegrationEvents
            | Self::ScheduleIntegrationEvents => ActivationUnit::Notifications,
            Self::BridgeCamoufoxLaunch => ActivationUnit::Camoufox,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyError {
    UnknownEnvironment,
    UnknownProfile,
    DigestMismatch,
    EnvironmentNotAllowed,
    ProductionNotAuthorized,
    ProfileInheritanceCycle,
    DependencyUnsatisfied,
    IncompatibleCapabilities,
}

impl Display for PolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::UnknownEnvironment => "unknown canonical environment",
            Self::UnknownProfile => "unknown capability profile",
            Self::DigestMismatch => "capability profile digest mismatch",
            Self::EnvironmentNotAllowed => "capability profile environment not allowed",
            Self::ProductionNotAuthorized => "production capability profile not authorized",
            Self::ProfileInheritanceCycle => "capability profile inheritance cycle",
            Self::DependencyUnsatisfied => "capability dependency unsatisfied",
            Self::IncompatibleCapabilities => "incompatible capabilities enabled",
        })
    }
}

impl std::error::Error for PolicyError {}

#[cfg(test)]
mod tests {
    use super::{
        ActivationUnit, CanonicalEnvironment, PolicyError, ProductionAuthorization, ProfileId,
        RuntimeSurface, admit_profile, effective_profile, profile_definition,
    };

    #[test]
    fn current_profile_digests_are_frozen_in_the_single_owner() {
        assert_eq!(
            profile_definition(ProfileId::ProductionCoreV1).digest,
            "92ccb88e7b74c89e4f39a5349eb5bf0da6a2d6f9ccc4a89d72ab462cb08e0868"
        );
        assert_eq!(
            profile_definition(ProfileId::RehearsalCoreV1).digest,
            "40ebe3bc1d890757f00433d0ff814720be1ffcd691fff35aea5244a05fc1f45a"
        );
    }

    #[test]
    fn rehearsal_core_is_valid_and_excludes_mail() -> Result<(), PolicyError> {
        let profile = admit_profile(
            "staging",
            "rehearsal-core-v1",
            profile_definition(ProfileId::RehearsalCoreV1).digest,
            ProductionAuthorization::NotAuthorized,
        )?;
        assert!(profile.capabilities.enabled(ActivationUnit::Clients));
        assert!(!profile.capabilities.enabled(ActivationUnit::MailboxAdmin));
        Ok(())
    }

    #[test]
    fn production_fails_closed_without_authorization() {
        assert_eq!(
            admit_profile(
                "production",
                "production-core-v1",
                profile_definition(ProfileId::ProductionCoreV1).digest,
                ProductionAuthorization::NotAuthorized,
            ),
            Err(PolicyError::ProductionNotAuthorized)
        );
    }

    #[test]
    fn wrong_digest_and_wrong_environment_fail_closed() {
        assert_eq!(
            admit_profile(
                "staging",
                "rehearsal-core-v1",
                "bad",
                ProductionAuthorization::NotAuthorized,
            ),
            Err(PolicyError::DigestMismatch)
        );
        assert_eq!(
            effective_profile(ProfileId::RehearsalCoreV1, CanonicalEnvironment::Production),
            Err(PolicyError::EnvironmentNotAllowed)
        );
    }

    #[test]
    fn resolver_and_runtime_surfaces_have_canonical_units() {
        assert_eq!(
            RuntimeSurface::ResolverIngress.activation_unit(),
            ActivationUnit::MailboxAdmin
        );
        assert_eq!(
            RuntimeSurface::BridgeCamoufoxLaunch.activation_unit(),
            ActivationUnit::Camoufox
        );
    }
}
