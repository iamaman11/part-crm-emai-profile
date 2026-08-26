use crate::PolicyError;
use std::collections::BTreeSet;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityDefinition {
    pub unit: ActivationUnit,
    pub dependencies: &'static [ActivationUnit],
    pub incompatible_with: &'static [ActivationUnit],
    pub requires_windows_profile_bridge: bool,
}

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

    pub fn parse(value: &str) -> Result<Self, PolicyError> {
        ALL_ACTIVATION_UNITS
            .iter()
            .copied()
            .find(|unit| unit.id() == value)
            .ok_or(PolicyError::UnknownActivationUnit)
    }

    #[must_use]
    pub const fn definition(self) -> CapabilityDefinition {
        CapabilityDefinition {
            unit: self,
            dependencies: self.dependencies(),
            incompatible_with: self.incompatible_with(),
            requires_windows_profile_bridge: self.requires_windows_profile_bridge(),
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

pub(crate) fn validate_catalog() -> Result<(), PolicyError> {
    let mut ids = BTreeSet::new();
    for unit in ALL_ACTIVATION_UNITS {
        if !ids.insert(unit.id()) {
            return Err(PolicyError::ActivationDependencyCycle);
        }
        if unit.dependencies().contains(&unit) {
            return Err(PolicyError::ActivationSelfDependency { unit });
        }
        if unit.incompatible_with().contains(&unit) {
            return Err(PolicyError::ActivationSelfIncompatibility { unit });
        }
        for incompatible in unit.incompatible_with() {
            if !incompatible.incompatible_with().contains(&unit) {
                return Err(PolicyError::AsymmetricIncompatibility {
                    left: unit,
                    right: *incompatible,
                });
            }
        }
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for unit in ALL_ACTIVATION_UNITS {
        visit(unit, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit(
    unit: ActivationUnit,
    visiting: &mut BTreeSet<ActivationUnit>,
    visited: &mut BTreeSet<ActivationUnit>,
) -> Result<(), PolicyError> {
    if visited.contains(&unit) {
        return Ok(());
    }
    if !visiting.insert(unit) {
        return Err(PolicyError::ActivationDependencyCycle);
    }
    for dependency in unit.dependencies() {
        visit(*dependency, visiting, visited)?;
    }
    visiting.remove(&unit);
    visited.insert(unit);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ALL_ACTIVATION_UNITS, ActivationUnit, validate_catalog};
    use std::collections::BTreeSet;

    #[test]
    fn catalog_is_complete_unique_and_acyclic() {
        assert!(validate_catalog().is_ok());
        let ids: BTreeSet<&str> = ALL_ACTIVATION_UNITS.iter().map(|unit| unit.id()).collect();
        assert_eq!(ids.len(), ALL_ACTIVATION_UNITS.len());
    }

    #[test]
    fn string_ids_round_trip() {
        for unit in ALL_ACTIVATION_UNITS {
            assert_eq!(ActivationUnit::parse(unit.id()), Ok(unit));
        }
    }
}
