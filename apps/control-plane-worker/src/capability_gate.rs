use control_plane_contract::RouteClass;
use worker::{Env, Error, Result};

pub use capability_policy::{ActivationUnit, CapabilityProfile};
use capability_policy::{ProductionAuthorization, RuntimeSurface, admit_profile};

pub const CANONICAL_ENVIRONMENT_VAR: &str = "CANONICAL_ENVIRONMENT";
pub const CAPABILITY_PROFILE_ID_VAR: &str = "CAPABILITY_PROFILE_ID";
pub const CAPABILITY_PROFILE_DIGEST_VAR: &str = "CAPABILITY_PROFILE_DIGEST";

pub fn active_profile(env: &Env) -> Result<CapabilityProfile> {
    let environment = env.var(CANONICAL_ENVIRONMENT_VAR)?.to_string();
    let profile_id = env.var(CAPABILITY_PROFILE_ID_VAR)?.to_string();
    let digest = env.var(CAPABILITY_PROFILE_DIGEST_VAR)?.to_string();
    admit_profile(
        &environment,
        &profile_id,
        &digest,
        ProductionAuthorization::NotAuthorized,
    )
    .map_err(|error| {
        Error::RustError(format!(
            "capability profile selection failed closed: {error}"
        ))
    })
}

pub fn unit_enabled(env: &Env, unit: ActivationUnit) -> Result<bool> {
    Ok(active_profile(env)?.capabilities.enabled(unit))
}

pub fn surface_enabled(env: &Env, surface: RuntimeSurface) -> Result<bool> {
    unit_enabled(env, surface.activation_unit())
}

pub fn route_enabled(env: &Env, route: RouteClass, path: &str) -> Result<bool> {
    let Some(surface) = route_surface(route, path) else {
        return Ok(true);
    };
    surface_enabled(env, surface)
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
}
