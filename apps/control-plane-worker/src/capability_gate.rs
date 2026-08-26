use control_plane_contract::RouteClass;
use worker::{Env, Error, Result};

pub use capability_policy::ActivationUnit;
use capability_policy::{
    AdmissionRequest, AuthorizationState, CanonicalEnvironment, EffectiveProfile, ProfileDigest,
    ProfileId, RuntimeSurface, admit,
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
        RouteClass::HealthApi => Some(RuntimeSurface::HttpHealth),
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
    use super::{ActivationUnit, RouteClass, RuntimeSurface, route_surface};

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
    fn mailbox_job_route_maps_to_mailbox_jobs_capability() {
        let surface = route_surface(RouteClass::MailboxJobRunApi, "/api/mailboxes/jobs/run");
        assert_eq!(surface, Some(RuntimeSurface::HttpMailboxJobs));
        assert_eq!(
            surface.map(RuntimeSurface::activation_unit),
            Some(ActivationUnit::MailboxJobs)
        );
    }
}
