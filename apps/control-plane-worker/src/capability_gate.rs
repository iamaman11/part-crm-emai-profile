use control_plane_contract::RouteClass;
use worker::{Env, Error, Result};

pub const CANONICAL_ENVIRONMENT_VAR: &str = "CANONICAL_ENVIRONMENT";
pub const CAPABILITY_PROFILE_ID_VAR: &str = "CAPABILITY_PROFILE_ID";
pub const CAPABILITY_PROFILE_DIGEST_VAR: &str = "CAPABILITY_PROFILE_DIGEST";
pub const RELEASE_PROFILE_HEADER: &str = "X-Release-Profile";
pub const RELEASE_PROFILE_DIGEST_HEADER: &str = "X-Release-Profile-Digest";
pub const EFFECTIVE_CAPABILITIES_HEADER: &str = "X-Effective-Capabilities";

pub const PRODUCTION_CORE_V1_DIGEST: &str =
    "92ccb88e7b74c89e4f39a5349eb5bf0da6a2d6f9ccc4a89d72ab462cb08e0868";
pub const REHEARSAL_CORE_V1_DIGEST: &str =
    "40ebe3bc1d890757f00433d0ff814720be1ffcd691fff35aea5244a05fc1f45a";
pub const PRODUCTION_MAILBOX_ADMIN_V1_DIGEST: &str =
    "ede6abdcdeb98738855e7fc2309788625ecbce03531b71644413ba69dceaf939";
pub const PRODUCTION_MAILBOX_JOBS_V1_DIGEST: &str =
    "a95ad429c73bc3415d8991ec15d390c4be94daec88a7700f473e2325fa3470ef";
pub const PRODUCTION_OUTBOUND_MAIL_V1_DIGEST: &str =
    "da2a883eba9fa706d6e9adfe87265d3b56a842cd7636877874dce3e86d3bf014";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
}

const ALL_ACTIVATION_UNITS: [ActivationUnit; 13] = [
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
enum ProfileAuthorization {
    NonProductionCandidate,
    ProductionBlocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveCapabilities(u16);

impl EffectiveCapabilities {
    const FOUNDATION: u16 = 1 << 0;
    const IDENTITY: u16 = 1 << 1;
    const CLIENTS: u16 = 1 << 2;
    const BROWSER_PROFILES: u16 = 1 << 3;
    const PROFILE_RUNTIME: u16 = 1 << 4;
    const CAMOUFOX: u16 = 1 << 5;
    const NOTIFICATIONS: u16 = 1 << 6;
    const MAILBOX_ADMIN: u16 = 1 << 7;
    const MAILBOX_CLIENT_BINDING: u16 = 1 << 8;
    const MAILBOX_BROWSER_BINDING: u16 = 1 << 9;
    const MAILBOX_READ: u16 = 1 << 10;
    const MAILBOX_JOBS: u16 = 1 << 11;
    const OUTBOUND_MAIL: u16 = 1 << 12;

    const CORE: u16 = Self::FOUNDATION
        | Self::IDENTITY
        | Self::CLIENTS
        | Self::BROWSER_PROFILES
        | Self::PROFILE_RUNTIME
        | Self::CAMOUFOX
        | Self::NOTIFICATIONS;

    #[must_use]
    pub const fn enabled(self, unit: ActivationUnit) -> bool {
        let bit = match unit {
            ActivationUnit::Foundation => Self::FOUNDATION,
            ActivationUnit::Identity => Self::IDENTITY,
            ActivationUnit::Clients => Self::CLIENTS,
            ActivationUnit::BrowserProfiles => Self::BROWSER_PROFILES,
            ActivationUnit::ProfileRuntime => Self::PROFILE_RUNTIME,
            ActivationUnit::Camoufox => Self::CAMOUFOX,
            ActivationUnit::Notifications => Self::NOTIFICATIONS,
            ActivationUnit::MailboxAdmin => Self::MAILBOX_ADMIN,
            ActivationUnit::MailboxClientBinding => Self::MAILBOX_CLIENT_BINDING,
            ActivationUnit::MailboxBrowserBinding => Self::MAILBOX_BROWSER_BINDING,
            ActivationUnit::MailboxRead => Self::MAILBOX_READ,
            ActivationUnit::MailboxJobs => Self::MAILBOX_JOBS,
            ActivationUnit::OutboundMail => Self::OUTBOUND_MAIL,
        };
        self.0 & bit != 0
    }

    #[must_use]
    pub fn enabled_ids(self) -> String {
        ALL_ACTIVATION_UNITS
            .iter()
            .copied()
            .filter(|unit| self.enabled(*unit))
            .map(ActivationUnit::id)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityProfile {
    pub id: &'static str,
    pub digest: &'static str,
    authorization: ProfileAuthorization,
    pub capabilities: EffectiveCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSelectionError {
    UnknownEnvironment,
    UnknownProfile,
    DigestMismatch,
    EnvironmentNotAllowed,
    ProductionNotAuthorized,
}

fn profile_definition(
    profile_id: &str,
) -> std::result::Result<CapabilityProfile, ProfileSelectionError> {
    match profile_id {
        "production-core-v1" => Ok(CapabilityProfile {
            id: "production-core-v1",
            digest: PRODUCTION_CORE_V1_DIGEST,
            authorization: ProfileAuthorization::ProductionBlocked,
            capabilities: EffectiveCapabilities(EffectiveCapabilities::CORE),
        }),
        "rehearsal-core-v1" => Ok(CapabilityProfile {
            id: "rehearsal-core-v1",
            digest: REHEARSAL_CORE_V1_DIGEST,
            authorization: ProfileAuthorization::NonProductionCandidate,
            capabilities: EffectiveCapabilities(EffectiveCapabilities::CORE),
        }),
        "production-mailbox-admin-v1" => Ok(CapabilityProfile {
            id: "production-mailbox-admin-v1",
            digest: PRODUCTION_MAILBOX_ADMIN_V1_DIGEST,
            authorization: ProfileAuthorization::ProductionBlocked,
            capabilities: EffectiveCapabilities(
                EffectiveCapabilities::CORE
                    | EffectiveCapabilities::MAILBOX_ADMIN
                    | EffectiveCapabilities::MAILBOX_CLIENT_BINDING
                    | EffectiveCapabilities::MAILBOX_BROWSER_BINDING
                    | EffectiveCapabilities::MAILBOX_READ,
            ),
        }),
        "production-mailbox-jobs-v1" => Ok(CapabilityProfile {
            id: "production-mailbox-jobs-v1",
            digest: PRODUCTION_MAILBOX_JOBS_V1_DIGEST,
            authorization: ProfileAuthorization::ProductionBlocked,
            capabilities: EffectiveCapabilities(
                EffectiveCapabilities::CORE
                    | EffectiveCapabilities::MAILBOX_ADMIN
                    | EffectiveCapabilities::MAILBOX_CLIENT_BINDING
                    | EffectiveCapabilities::MAILBOX_BROWSER_BINDING
                    | EffectiveCapabilities::MAILBOX_READ
                    | EffectiveCapabilities::MAILBOX_JOBS,
            ),
        }),
        "production-outbound-mail-v1" => Ok(CapabilityProfile {
            id: "production-outbound-mail-v1",
            digest: PRODUCTION_OUTBOUND_MAIL_V1_DIGEST,
            authorization: ProfileAuthorization::ProductionBlocked,
            capabilities: EffectiveCapabilities(
                EffectiveCapabilities::CORE
                    | EffectiveCapabilities::MAILBOX_ADMIN
                    | EffectiveCapabilities::MAILBOX_CLIENT_BINDING
                    | EffectiveCapabilities::MAILBOX_BROWSER_BINDING
                    | EffectiveCapabilities::MAILBOX_READ
                    | EffectiveCapabilities::MAILBOX_JOBS
                    | EffectiveCapabilities::OUTBOUND_MAIL,
            ),
        }),
        _ => Err(ProfileSelectionError::UnknownProfile),
    }
}

pub fn active_profile(env: &Env) -> Result<CapabilityProfile> {
    let environment = env.var(CANONICAL_ENVIRONMENT_VAR)?.to_string();
    let profile_id = env.var(CAPABILITY_PROFILE_ID_VAR)?.to_string();
    let digest = env.var(CAPABILITY_PROFILE_DIGEST_VAR)?.to_string();
    select_profile(&environment, &profile_id, &digest).map_err(|error| {
        Error::RustError(format!(
            "capability profile selection failed closed: {error:?}"
        ))
    })
}

pub fn unit_enabled(env: &Env, unit: ActivationUnit) -> Result<bool> {
    Ok(active_profile(env)?.capabilities.enabled(unit))
}

pub fn route_enabled(env: &Env, route: RouteClass, path: &str) -> Result<bool> {
    let Some(unit) = route_activation_unit(route, path) else {
        return Ok(true);
    };
    unit_enabled(env, unit)
}

#[must_use]
pub fn select_profile(
    environment: &str,
    profile_id: &str,
    profile_digest: &str,
) -> std::result::Result<CapabilityProfile, ProfileSelectionError> {
    if !matches!(environment, "rehearsal" | "staging" | "production") {
        return Err(ProfileSelectionError::UnknownEnvironment);
    }

    let profile = profile_definition(profile_id)?;

    if profile.digest != profile_digest {
        return Err(ProfileSelectionError::DigestMismatch);
    }

    let environment_allowed = match profile.id {
        "rehearsal-core-v1" => matches!(environment, "rehearsal" | "staging"),
        _ => environment == "production",
    };
    if !environment_allowed {
        return Err(ProfileSelectionError::EnvironmentNotAllowed);
    }

    if environment == "production"
        && profile.authorization == ProfileAuthorization::ProductionBlocked
    {
        return Err(ProfileSelectionError::ProductionNotAuthorized);
    }

    Ok(profile)
}

#[must_use]
pub fn route_activation_unit(route: RouteClass, path: &str) -> Option<ActivationUnit> {
    match route {
        RouteClass::HealthApi
        | RouteClass::BindingProbeApi
        | RouteClass::AuthenticatedSessionApi => Some(ActivationUnit::Foundation),
        RouteClass::OwnerBootstrapApi
        | RouteClass::OwnerTransferApi
        | RouteClass::InvitationCollectionApi
        | RouteClass::InvitationAcceptApi
        | RouteClass::MembershipCollectionApi
        | RouteClass::MembershipStatusApi => Some(ActivationUnit::Identity),
        RouteClass::ClientCollectionApi
        | RouteClass::ClientResourceApi
        | RouteClass::ClientArchiveApi
        | RouteClass::ClientContactApi
        | RouteClass::ClientMergeApi
        | RouteClass::ClientHistoryApi
        | RouteClass::ClientGrantApi => Some(ActivationUnit::Clients),
        RouteClass::ClientMailSearchApi | RouteClass::ClientMailMessageApi => {
            Some(ActivationUnit::MailboxRead)
        }
        RouteClass::ClientMailSendApi => Some(ActivationUnit::OutboundMail),
        RouteClass::ProfileCollectionApi
        | RouteClass::ProfileResourceApi
        | RouteClass::ProfileAssignmentApi
        | RouteClass::ProfileGrantApi
        | RouteClass::ProfileCoordinatorApi
        | RouteClass::ProfileGenerationCollectionApi
        | RouteClass::ProfileGenerationResourceApi
        | RouteClass::ProfileGenerationVerifyApi
        | RouteClass::ProfileGenerationActivateApi
        | RouteClass::ProfileGenerationDeactivateApi
        | RouteClass::ProfileGenerationQuarantineApi => Some(ActivationUnit::BrowserProfiles),
        RouteClass::MailboxBindingResourceApi if path.contains("/client-association") => {
            Some(ActivationUnit::MailboxClientBinding)
        }
        RouteClass::MailboxBindingCollectionApi
        | RouteClass::MailboxBindingResourceApi
        | RouteClass::MailboxBindingRevokeApi => Some(ActivationUnit::MailboxAdmin),
        RouteClass::MailboxBrowserExecutionBindApi => Some(ActivationUnit::MailboxBrowserBinding),
        RouteClass::MailboxJobCollectionApi
        | RouteClass::MailboxJobResourceApi
        | RouteClass::MailboxJobRunApi => Some(ActivationUnit::MailboxJobs),
        RouteClass::DeviceJobClaimableApi
        | RouteClass::DeviceJobClaimApi
        | RouteClass::DeviceJobHeartbeatApi
        | RouteClass::DeviceGenerationUploadCapabilityApi
        | RouteClass::DeviceGenerationCommitApi
        | RouteClass::DeviceJobOutcomeApi => Some(ActivationUnit::ProfileRuntime),
        RouteClass::NotificationEventCollectionApi
        | RouteClass::NotificationEventAckApi
        | RouteClass::NotificationReplayCollectionApi
        | RouteClass::NotificationOperationsApi => Some(ActivationUnit::Notifications),
        RouteClass::DynamicRouteNotFound
        | RouteClass::BridgeDeniedByDefault
        | RouteClass::StaticAssets => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        ActivationUnit, PRODUCTION_CORE_V1_DIGEST, ProfileSelectionError, REHEARSAL_CORE_V1_DIGEST,
        profile_definition, route_activation_unit, select_profile,
    };
    use control_plane_contract::RouteClass;
    use serde_json::Value;

    fn profile_row<'a>(rows: &'a [Value], profile_id: &str) -> &'a Value {
        rows.iter()
            .find(|row| row.get("profile_id").and_then(Value::as_str) == Some(profile_id))
            .unwrap_or_else(|| panic!("missing architecture release profile {profile_id}"))
    }

    fn string_set(value: Option<&Value>, field: &str, profile_id: &str) -> BTreeSet<String> {
        let Some(array) = value.and_then(Value::as_array) else {
            panic!("profile {profile_id} field {field} must be an array");
        };
        array
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .unwrap_or_else(|| {
                        panic!("profile {profile_id} field {field} contains a non-string")
                    })
                    .to_owned()
            })
            .collect()
    }

    fn architecture_effective_units(
        rows: &[Value],
        profile_id: &str,
        visiting: &mut BTreeSet<String>,
    ) -> BTreeSet<String> {
        assert!(
            visiting.insert(profile_id.to_owned()),
            "capability profile inheritance cycle at {profile_id}"
        );
        let row = profile_row(rows, profile_id);
        let mut result = if let Some(parent) = row.get("extends").and_then(Value::as_str) {
            architecture_effective_units(rows, parent, visiting)
        } else {
            BTreeSet::new()
        };
        result.extend(string_set(
            row.get("enabled_activation_units"),
            "enabled_activation_units",
            profile_id,
        ));
        for disabled in string_set(
            row.get("disabled_activation_units"),
            "disabled_activation_units",
            profile_id,
        ) {
            result.remove(&disabled);
        }
        visiting.remove(profile_id);
        result
    }

    fn runtime_units(profile_id: &str) -> BTreeSet<String> {
        profile_definition(profile_id)
            .unwrap_or_else(|error| panic!("runtime profile {profile_id} missing: {error:?}"))
            .capabilities
            .enabled_ids()
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn production_profile_is_blocked_before_ar17_pc1_authorization() {
        assert_eq!(
            select_profile(
                "production",
                "production-core-v1",
                PRODUCTION_CORE_V1_DIGEST
            ),
            Err(ProfileSelectionError::ProductionNotAuthorized)
        );
    }

    #[test]
    fn staging_core_excludes_mail() -> Result<(), ProfileSelectionError> {
        let profile = select_profile("staging", "rehearsal-core-v1", REHEARSAL_CORE_V1_DIGEST)?;
        assert!(profile.capabilities.enabled(ActivationUnit::Clients));
        assert!(profile.capabilities.enabled(ActivationUnit::ProfileRuntime));
        assert!(profile.capabilities.enabled(ActivationUnit::Camoufox));
        assert!(!profile.capabilities.enabled(ActivationUnit::MailboxAdmin));
        assert!(!profile.capabilities.enabled(ActivationUnit::MailboxJobs));
        assert!(!profile.capabilities.enabled(ActivationUnit::OutboundMail));
        assert!(
            profile
                .capabilities
                .enabled_ids()
                .contains("browser_profiles")
        );
        Ok(())
    }

    #[test]
    fn wrong_digest_fails_closed() {
        assert_eq!(
            select_profile("staging", "rehearsal-core-v1", PRODUCTION_CORE_V1_DIGEST),
            Err(ProfileSelectionError::DigestMismatch)
        );
    }

    #[test]
    fn route_activation_is_finer_than_application_owner() {
        assert_eq!(
            route_activation_unit(
                RouteClass::ClientMailSearchApi,
                "/api/v1/tenants/t/clients/c/mail/search"
            ),
            Some(ActivationUnit::MailboxRead)
        );
        assert_eq!(
            route_activation_unit(
                RouteClass::ClientMailSendApi,
                "/api/v1/tenants/t/clients/c/mail/send"
            ),
            Some(ActivationUnit::OutboundMail)
        );
        assert_eq!(
            route_activation_unit(
                RouteClass::MailboxBindingResourceApi,
                "/api/v1/tenants/t/mailboxes/m/client-association"
            ),
            Some(ActivationUnit::MailboxClientBinding)
        );
    }

    #[test]
    fn runtime_profile_authority_matches_architecture_and_wrangler_projection() {
        let architecture: Value = serde_json::from_str(include_str!(
            "../../../architecture/release-architecture-ar11.json"
        ))
        .expect("release architecture JSON must parse");
        let rows = architecture
            .get("release_profiles")
            .and_then(Value::as_array)
            .expect("release_profiles must be an array");
        let expected_ids = BTreeSet::from([
            "production-core-v1",
            "production-mailbox-admin-v1",
            "production-mailbox-jobs-v1",
            "production-outbound-mail-v1",
            "rehearsal-core-v1",
        ]);
        let architecture_ids = rows
            .iter()
            .map(|row| {
                row.get("profile_id")
                    .and_then(Value::as_str)
                    .expect("every release profile requires profile_id")
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(architecture_ids, expected_ids);

        for profile_id in expected_ids {
            assert_eq!(
                architecture_effective_units(rows, profile_id, &mut BTreeSet::new()),
                runtime_units(profile_id),
                "architecture/runtime capability composition drifted for {profile_id}"
            );
            let allowed = string_set(
                profile_row(rows, profile_id).get("allowed_environments"),
                "allowed_environments",
                profile_id,
            );
            let expected_allowed = if profile_id == "rehearsal-core-v1" {
                BTreeSet::from(["rehearsal".to_owned(), "staging".to_owned()])
            } else {
                BTreeSet::from(["production".to_owned()])
            };
            assert_eq!(
                allowed, expected_allowed,
                "environment authority drifted for {profile_id}"
            );
        }

        let wrangler: Value =
            serde_json::from_str(include_str!("../../../deploy/cloudflare/wrangler.jsonc"))
                .expect("canonical Wrangler JSON must parse");
        for (environment, profile_id) in [
            ("staging", "rehearsal-core-v1"),
            ("production", "production-core-v1"),
        ] {
            let vars = &wrangler["env"][environment]["vars"];
            let runtime = profile_definition(profile_id)
                .unwrap_or_else(|error| panic!("runtime profile {profile_id} missing: {error:?}"));
            assert_eq!(
                vars.get("CANONICAL_ENVIRONMENT").and_then(Value::as_str),
                Some(environment)
            );
            assert_eq!(
                vars.get("CAPABILITY_PROFILE_ID").and_then(Value::as_str),
                Some(runtime.id)
            );
            assert_eq!(
                vars.get("CAPABILITY_PROFILE_DIGEST")
                    .and_then(Value::as_str),
                Some(runtime.digest)
            );
        }
    }
}
