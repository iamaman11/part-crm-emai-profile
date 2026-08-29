use control_plane_contract::RouteClass;
use worker::{Env, Error, Result};

pub use capability_policy::{ActivationUnit, RuntimeSurface};
use capability_policy::{
    AdmissionRequest, AuthorizationState, CanonicalEnvironment, EffectiveProfile, ProfileDigest,
    ProfileId, admit,
};

pub const CANONICAL_ENVIRONMENT_VAR: &str = "CANONICAL_ENVIRONMENT";
pub const CAPABILITY_PROFILE_ID_VAR: &str = "CAPABILITY_PROFILE_ID";
pub const CAPABILITY_PROFILE_DIGEST_VAR: &str = "CAPABILITY_PROFILE_DIGEST";

#[derive(Clone, Debug)]
pub struct RuntimeCapabilityContext {
    profile: EffectiveProfile,
}

impl RuntimeCapabilityContext {
    pub fn from_env(env: &Env) -> Result<Self> {
        let environment = env.var(CANONICAL_ENVIRONMENT_VAR)?.to_string();
        let profile_id = env.var(CAPABILITY_PROFILE_ID_VAR)?.to_string();
        let digest = env.var(CAPABILITY_PROFILE_DIGEST_VAR)?.to_string();
        let request = AdmissionRequest {
            environment: CanonicalEnvironment::parse(&environment).map_err(policy_error)?,
            profile_id: ProfileId::parse(&profile_id).map_err(policy_error)?,
            presented_digest: ProfileDigest::parse_hex(&digest).map_err(policy_error)?,
            authorization: AuthorizationState::NotAuthorized,
        };
        let profile = admit(request).map_err(policy_error)?;
        Ok(Self { profile })
    }

    #[must_use]
    pub const fn profile(&self) -> &EffectiveProfile {
        &self.profile
    }

    #[must_use]
    pub fn unit_enabled(&self, unit: ActivationUnit) -> bool {
        self.profile.capabilities.enabled(unit)
    }

    #[must_use]
    pub fn surface_enabled(&self, surface: RuntimeSurface) -> bool {
        self.unit_enabled(surface.activation_unit())
    }

    #[must_use]
    pub fn route_enabled(&self, route: RouteClass, path: &str) -> bool {
        route_surface(route, path).is_none_or(|surface| self.surface_enabled(surface))
    }
}

fn policy_error(error: capability_policy::PolicyError) -> Error {
    Error::RustError(format!(
        "capability profile selection failed closed: {error}"
    ))
}

#[must_use]
pub fn route_surface(route: RouteClass, path: &str) -> Option<RuntimeSurface> {
    match route {
        RouteClass::HealthApi => None,
        RouteClass::BindingProbeApi => Some(RuntimeSurface::HttpBindings),
        RouteClass::AuthenticatedSessionApi => Some(RuntimeSurface::HttpSession),
        RouteClass::OwnerBootstrapApi
        | RouteClass::OwnerTransferApi
        | RouteClass::InvitationCollectionApi
        | RouteClass::InvitationAcceptApi
        | RouteClass::MembershipCollectionApi
        | RouteClass::MembershipStatusApi => Some(RuntimeSurface::HttpIdentity),
        RouteClass::ClientCollectionApi
        | RouteClass::ClientResourceApi
        | RouteClass::ClientArchiveApi
        | RouteClass::ClientContactApi
        | RouteClass::ClientMergeApi
        | RouteClass::ClientHistoryApi
        | RouteClass::ClientGrantApi => Some(RuntimeSurface::HttpClients),
        RouteClass::ClientMailSearchApi | RouteClass::ClientMailMessageApi => {
            Some(RuntimeSurface::HttpClientMailRead)
        }
        RouteClass::ClientMailSendApi => Some(RuntimeSurface::HttpOutboundMail),
        RouteClass::ProfileLaunchApi => Some(RuntimeSurface::HttpProfileRuntimeLaunch),
        RouteClass::ProfileCoordinatorApi if path.starts_with("/bridge/") => {
            Some(RuntimeSurface::HttpProfileRuntimeLaunch)
        }
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
        | RouteClass::ProfileGenerationQuarantineApi => Some(RuntimeSurface::HttpBrowserProfiles),
        RouteClass::MailboxBindingResourceApi if path.contains("/client-association") => {
            Some(RuntimeSurface::HttpMailboxClientBinding)
        }
        RouteClass::MailboxBindingCollectionApi
        | RouteClass::MailboxBindingResourceApi
        | RouteClass::MailboxBindingRevokeApi => Some(RuntimeSurface::HttpMailboxAdmin),
        RouteClass::MailboxBrowserExecutionBindApi => {
            Some(RuntimeSurface::HttpMailboxBrowserBinding)
        }
        RouteClass::MailboxJobCollectionApi
        | RouteClass::MailboxJobResourceApi
        | RouteClass::MailboxJobRunApi => Some(RuntimeSurface::HttpMailboxJobs),
        RouteClass::DeviceJobClaimableApi
        | RouteClass::DeviceJobClaimApi
        | RouteClass::DeviceJobHeartbeatApi
        | RouteClass::DeviceGenerationUploadCapabilityApi
        | RouteClass::DeviceGenerationCommitApi
        | RouteClass::DeviceJobOutcomeApi => Some(RuntimeSurface::HttpProfileRuntimeDeviceJobs),
        RouteClass::NotificationEventCollectionApi
        | RouteClass::NotificationEventAckApi
        | RouteClass::NotificationReplayCollectionApi
        | RouteClass::NotificationOperationsApi => Some(RuntimeSurface::HttpNotifications),
        RouteClass::DynamicRouteNotFound
        | RouteClass::BridgeDeniedByDefault
        | RouteClass::StaticAssets => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActivationUnit, RouteClass, RuntimeCapabilityContext, RuntimeSurface, route_surface,
    };
    use capability_policy::{CanonicalEnvironment, ProfileId, effective_profile};

    #[test]
    fn route_adapter_delegates_semantics_to_capability_policy() {
        let surface = route_surface(RouteClass::ClientMailSendApi, "/api/mail/send");
        assert_eq!(surface, Some(RuntimeSurface::HttpOutboundMail));
        assert_eq!(
            surface.map(RuntimeSurface::activation_unit),
            Some(ActivationUnit::OutboundMail)
        );
    }

    #[test]
    fn profile_launch_route_maps_to_profile_runtime_not_browser_profiles() {
        let launch = route_surface(
            RouteClass::ProfileLaunchApi,
            "/api/v1/tenants/tenant_01/profiles/profile_01/launch",
        );
        assert_eq!(launch, Some(RuntimeSurface::HttpProfileRuntimeLaunch));
        assert_eq!(
            launch.map(RuntimeSurface::activation_unit),
            Some(ActivationUnit::ProfileRuntime)
        );
        assert_eq!(
            route_surface(
                RouteClass::ProfileResourceApi,
                "/api/v1/tenants/tenant_01/profiles/profile_01",
            ),
            Some(RuntimeSurface::HttpBrowserProfiles)
        );
    }

    #[test]
    fn bridge_coordinator_route_is_governed_by_profile_runtime_capability() {
        let surface = route_surface(
            RouteClass::ProfileCoordinatorApi,
            "/bridge/v1/tenants/tenant_01/profiles/profile_01/coordinator",
        );
        assert_eq!(surface, Some(RuntimeSurface::HttpProfileRuntimeLaunch));
        assert_eq!(
            surface.map(RuntimeSurface::activation_unit),
            Some(ActivationUnit::ProfileRuntime)
        );
        assert_eq!(
            route_surface(
                RouteClass::ProfileCoordinatorApi,
                "/api/v1/tenants/tenant_01/profiles/profile_01/coordinator",
            ),
            Some(RuntimeSurface::HttpBrowserProfiles)
        );
    }

    #[test]
    fn mailbox_job_route_maps_to_mailbox_jobs_capability() {
        let surface = route_surface(RouteClass::MailboxJobRunApi, "/api/mailboxes/jobs/run");
        assert_eq!(surface, Some(RuntimeSurface::HttpMailboxJobs));
        assert_eq!(
            surface.map(RuntimeSurface::activation_unit),
            Some(ActivationUnit::MailboxJobs)
        );
    }

    #[test]
    fn health_is_explicitly_outside_capability_profile_admission() {
        assert_eq!(route_surface(RouteClass::HealthApi, "/health"), None);
    }

    #[test]
    fn surface_admission_uses_effective_profile_for_queue_and_schedule()
    -> Result<(), Box<dyn std::error::Error>> {
        let context = RuntimeCapabilityContext {
            profile: effective_profile(
                ProfileId::RehearsalCoreV2,
                CanonicalEnvironment::Rehearsal,
            )?,
        };

        assert!(!context.surface_enabled(RuntimeSurface::QueueIntegrationEvents));
        assert!(!context.surface_enabled(RuntimeSurface::ScheduleIntegrationEvents));
        assert!(!context.surface_enabled(RuntimeSurface::HttpNotifications));
        assert!(!context.surface_enabled(RuntimeSurface::QueueMailboxJobs));
        assert!(!context.surface_enabled(RuntimeSurface::ScheduleMailboxJobs));
        assert!(context.surface_enabled(RuntimeSurface::HttpClients));
        Ok(())
    }
}
