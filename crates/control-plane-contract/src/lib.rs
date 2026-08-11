#![forbid(unsafe_code)]

pub mod client_registry_api;
pub mod operator_query_api;
pub mod public_api;
pub mod query_mail_api;
mod routes;

pub const D1_CATALOG_BINDING: &str = "CATALOG_DB";
pub const R2_PROFILES_BINDING: &str = "PROFILE_OBJECTS";
pub const VERIFICATION_QUEUE_BINDING: &str = "GENERATION_VERIFICATION";
pub const PROFILE_COORDINATOR_BINDING: &str = "PROFILE_COORDINATOR";
pub const STATIC_ASSETS_BINDING: &str = "ASSETS";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteClass {
    HealthApi,
    BindingProbeApi,
    AuthenticatedSessionApi,
    OwnerBootstrapApi,
    OwnerTransferApi,
    InvitationCollectionApi,
    InvitationAcceptApi,
    MembershipCollectionApi,
    MembershipStatusApi,
    ClientCollectionApi,
    ClientResourceApi,
    ClientArchiveApi,
    ClientContactApi,
    ClientMergeApi,
    ClientHistoryApi,
    ClientGrantApi,
    ProfileCollectionApi,
    ProfileResourceApi,
    ProfileAssignmentApi,
    ProfileGrantApi,
    ProfileCoordinatorApi,
    ProfileGenerationCollectionApi,
    ProfileGenerationResourceApi,
    ProfileGenerationVerifyApi,
    ProfileGenerationActivateApi,
    ProfileGenerationDeactivateApi,
    ProfileGenerationQuarantineApi,
    MailboxBindingCollectionApi,
    MailboxBindingResourceApi,
    MailboxBindingRevokeApi,
    MailboxBrowserExecutionBindApi,
    MailboxJobCollectionApi,
    MailboxJobResourceApi,
    MailboxJobRunApi,
    DeviceJobClaimableApi,
    DeviceJobClaimApi,
    DeviceJobHeartbeatApi,
    DeviceGenerationUploadCapabilityApi,
    DeviceGenerationCommitApi,
    DeviceJobOutcomeApi,
    NotificationEventCollectionApi,
    NotificationEventAckApi,
    NotificationReplayCollectionApi,
    NotificationOperationsApi,
    DynamicRouteNotFound,
    BridgeDeniedByDefault,
    StaticAssets,
}

#[must_use]
pub fn classify_route(method: &str, path: &str) -> RouteClass {
    routes::classify(method, path)
}

#[must_use]
pub(crate) fn is_dynamic_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/") || path == "/auth" || path.starts_with("/auth/")
}

#[must_use]
pub const fn is_authenticated_api(route: RouteClass) -> bool {
    !matches!(
        route,
        RouteClass::HealthApi
            | RouteClass::BindingProbeApi
            | RouteClass::DynamicRouteNotFound
            | RouteClass::BridgeDeniedByDefault
            | RouteClass::StaticAssets
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HealthPayload {
    pub status: &'static str,
    pub contract_version: &'static str,
}

#[must_use]
pub const fn health_payload() -> HealthPayload {
    HealthPayload {
        status: "ok",
        contract_version: "v1",
    }
}

#[cfg(test)]
mod tests {
    use super::{RouteClass, classify_route, health_payload, is_authenticated_api};

    #[test]
    fn preserves_foundation_routes() {
        assert_eq!(
            classify_route("GET", "/api/v1/health"),
            RouteClass::HealthApi
        );
        assert_eq!(
            classify_route("GET", "/api/v1/bindings"),
            RouteClass::BindingProbeApi
        );
        assert_eq!(
            classify_route("GET", "/bridge/claim/code"),
            RouteClass::BridgeDeniedByDefault
        );
        assert_eq!(classify_route("GET", "/profiles"), RouteClass::StaticAssets);
    }

    #[test]
    fn dynamic_namespaces_fail_closed_for_unknown_versions_routes_and_methods() {
        for (method, path, expected) in [
            ("POST", "/api/v1/health", RouteClass::DynamicRouteNotFound),
            ("GET", "/api/v2/health", RouteClass::DynamicRouteNotFound),
            ("GET", "/api/v1/unknown", RouteClass::DynamicRouteNotFound),
            ("GET", "/api", RouteClass::DynamicRouteNotFound),
            (
                "POST",
                "/auth/v2/callback",
                RouteClass::DynamicRouteNotFound,
            ),
            ("GET", "/auth/unknown", RouteClass::DynamicRouteNotFound),
            ("GET", "/bridge", RouteClass::BridgeDeniedByDefault),
            (
                "POST",
                "/bridge/v2/claim",
                RouteClass::BridgeDeniedByDefault,
            ),
            (
                "DELETE",
                "/bridge/unknown",
                RouteClass::BridgeDeniedByDefault,
            ),
        ] {
            let route = classify_route(method, path);
            assert_eq!(route, expected, "unexpected route for {method} {path}");
            assert!(!is_authenticated_api(route));
            assert_ne!(route, RouteClass::StaticAssets);
        }
    }

    #[test]
    fn identity_client_profile_and_coordinator_routes_are_versioned_and_authenticated() {
        let routes = [
            (
                "GET",
                "/api/v1/session",
                RouteClass::AuthenticatedSessionApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/owner/bootstrap",
                RouteClass::OwnerBootstrapApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/owner/transfer",
                RouteClass::OwnerTransferApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/invitations",
                RouteClass::InvitationCollectionApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/invitations/invitation_01/accept",
                RouteClass::InvitationAcceptApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/members",
                RouteClass::MembershipCollectionApi,
            ),
            (
                "PUT",
                "/api/v1/tenants/tenant_01/members/actor_01/status",
                RouteClass::MembershipStatusApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/clients",
                RouteClass::ClientCollectionApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/clients/client_01",
                RouteClass::ClientResourceApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/profiles",
                RouteClass::ProfileCollectionApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/profiles/profile_01",
                RouteClass::ProfileResourceApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles/profile_01/coordinator",
                RouteClass::ProfileCoordinatorApi,
            ),
        ];
        for (method, path, expected) in routes {
            let route = classify_route(method, path);
            assert_eq!(route, expected, "unexpected route for {method} {path}");
            assert!(is_authenticated_api(route));
        }
    }

    #[test]
    fn health_contract_is_explicitly_versioned() {
        let payload = health_payload();
        assert_eq!(payload.status, "ok");
        assert_eq!(payload.contract_version, "v1");
    }
}
