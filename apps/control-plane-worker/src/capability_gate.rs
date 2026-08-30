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
pub const TARGET_AUTHORIZATION_OBSERVATION_VAR: &str = "TARGET_AUTHORIZATION_OBSERVATION";
const TARGET_AUTHORIZATION_SCHEMA: &str = "target-v1";
const RELEASE_SET_V3_PREFIX: &str = "release-set-v3-sha256-";

#[derive(Clone, Debug)]
pub struct RuntimeCapabilityContext {
    profile: EffectiveProfile,
}

impl RuntimeCapabilityContext {
    pub fn from_env(env: &Env) -> Result<Self> {
        let environment = env.var(CANONICAL_ENVIRONMENT_VAR)?.to_string();
        let profile_id = env.var(CAPABILITY_PROFILE_ID_VAR)?.to_string();
        let digest = env.var(CAPABILITY_PROFILE_DIGEST_VAR)?.to_string();
        let environment = CanonicalEnvironment::parse(&environment).map_err(policy_error)?;
        let profile_id = ProfileId::parse(&profile_id).map_err(policy_error)?;
        let digest = ProfileDigest::parse_hex(&digest).map_err(policy_error)?;
        let target_authorization = env
            .var(TARGET_AUTHORIZATION_OBSERVATION_VAR)
            .ok()
            .map(|value| value.to_string());
        let request = AdmissionRequest {
            environment,
            profile_id,
            presented_digest: digest,
            authorization: target_authorization_state(
                target_authorization.as_deref(),
                environment,
                profile_id,
                digest,
            )?,
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

fn target_authorization_state(
    observation: Option<&str>,
    environment: CanonicalEnvironment,
    profile_id: ProfileId,
    digest: ProfileDigest,
) -> Result<AuthorizationState> {
    let Some(observation) = observation else {
        return Ok(AuthorizationState::NotAuthorized);
    };
    if environment == CanonicalEnvironment::Production {
        return Err(target_authorization_error());
    }
    let mut fields = observation.split('|');
    let Some(schema) = fields.next() else {
        return Err(target_authorization_error());
    };
    let Some(observed_environment) = fields.next() else {
        return Err(target_authorization_error());
    };
    let Some(observed_profile_id) = fields.next() else {
        return Err(target_authorization_error());
    };
    let Some(observed_digest) = fields.next() else {
        return Err(target_authorization_error());
    };
    let Some(release_set_id) = fields.next() else {
        return Err(target_authorization_error());
    };
    if fields.next().is_some() || schema != TARGET_AUTHORIZATION_SCHEMA {
        return Err(target_authorization_error());
    }
    if CanonicalEnvironment::parse(observed_environment).map_err(|_| target_authorization_error())?
        != environment
        || ProfileId::parse(observed_profile_id).map_err(|_| target_authorization_error())?
            != profile_id
        || ProfileDigest::parse_hex(observed_digest).map_err(|_| target_authorization_error())?
            != digest
        || !valid_release_set_v3_id(release_set_id)
    {
        return Err(target_authorization_error());
    }
    Ok(AuthorizationState::TargetAuthorized)
}

fn valid_release_set_v3_id(value: &str) -> bool {
    let Some(digest) = value.strip_prefix(RELEASE_SET_V3_PREFIX) else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn target_authorization_error() -> Error {
    Error::RustError("target authorization observation failed closed".to_owned())
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
        target_authorization_state,
    };
    use capability_policy::{
        AuthorizationState, CanonicalEnvironment, ProfileId, effective_profile, profile_digest,
    };

    fn release_set_id() -> String {
        format!("release-set-v3-sha256-{}", "a".repeat(64))
    }

    fn target_observation(
        environment: CanonicalEnvironment,
        profile_id: ProfileId,
    ) -> String {
        format!(
            "target-v1|{}|{}|{}|{}",
            environment.id(),
            profile_id.id(),
            profile_digest(profile_id),
            release_set_id()
        )
    }

    #[test]
    fn target_authorization_observation_is_exact_and_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let environment = CanonicalEnvironment::Staging;
        let profile_id = ProfileId::RehearsalCoreV2;
        let digest = profile_digest(profile_id);
        let exact = target_observation(environment, profile_id);
        assert_eq!(
            target_authorization_state(Some(&exact), environment, profile_id, digest)?,
            AuthorizationState::TargetAuthorized
        );
        assert_eq!(
            target_authorization_state(None, environment, profile_id, digest)?,
            AuthorizationState::NotAuthorized
        );

        for malformed in [
            "target-v2|staging|rehearsal-core-v2|22be80b51e794e266a1ac4157f8375e644bec9116335482128774bcf401d46da|release-set-v3-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "target-v1|rehearsal|rehearsal-core-v2|22be80b51e794e266a1ac4157f8375e644bec9116335482128774bcf401d46da|release-set-v3-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "target-v1|staging|production-core-v2|22be80b51e794e266a1ac4157f8375e644bec9116335482128774bcf401d46da|release-set-v3-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "target-v1|staging|rehearsal-core-v2|0000000000000000000000000000000000000000000000000000000000000000|release-set-v3-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "target-v1|staging|rehearsal-core-v2|22be80b51e794e266a1ac4157f8375e644bec9116335482128774bcf401d46da|release-set-v2-sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "target-v1|staging|rehearsal-core-v2|22be80b51e794e266a1ac4157f8375e644bec9116335482128774bcf401d46da|release-set-v3-sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            assert!(
                target_authorization_state(Some(malformed), environment, profile_id, digest)
                    .is_err(),
                "malformed target observation unexpectedly admitted: {malformed}"
            );
        }
        Ok(())
    }

    #[test]
    fn target_authorization_observation_cannot_target_production() {
        let environment = CanonicalEnvironment::Production;
        let profile_id = ProfileId::ProductionCoreV2;
        let observation = target_observation(environment, profile_id);
        assert!(
            target_authorization_state(
                Some(&observation),
                environment,
                profile_id,
                profile_digest(profile_id),
            )
            .is_err()
        );
    }

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
