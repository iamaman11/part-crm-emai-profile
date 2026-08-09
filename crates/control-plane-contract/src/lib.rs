#![forbid(unsafe_code)]

pub mod client_registry_api;
pub mod public_api;
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
    MailboxJobCollectionApi,
    MailboxJobResourceApi,
    MailboxJobRunApi,
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
                "PUT",
                "/api/v1/tenants/tenant_01/members/actor_01/status",
                RouteClass::MembershipStatusApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/clients",
                RouteClass::ClientCollectionApi,
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
                "PATCH",
                "/api/v1/tenants/tenant_01/clients/client_01",
                RouteClass::ClientResourceApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/clients/client_01/archive",
                RouteClass::ClientArchiveApi,
            ),
            (
                "PUT",
                "/api/v1/tenants/tenant_01/clients/client_01/contacts/contact_01",
                RouteClass::ClientContactApi,
            ),
            (
                "DELETE",
                "/api/v1/tenants/tenant_01/clients/client_01/contacts/contact_01",
                RouteClass::ClientContactApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/clients/client_01/merge",
                RouteClass::ClientMergeApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/clients/client_01/history",
                RouteClass::ClientHistoryApi,
            ),
            (
                "DELETE",
                "/api/v1/tenants/tenant_01/clients/client_01/grants/actor_01",
                RouteClass::ClientGrantApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles",
                RouteClass::ProfileCollectionApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/profiles/profile_01",
                RouteClass::ProfileResourceApi,
            ),
            (
                "PUT",
                "/api/v1/tenants/tenant_01/profiles/profile_01/assignment",
                RouteClass::ProfileAssignmentApi,
            ),
            (
                "PUT",
                "/api/v1/tenants/tenant_01/profiles/profile_01/grants/actor_01",
                RouteClass::ProfileGrantApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles/profile_01/coordinator",
                RouteClass::ProfileCoordinatorApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/profiles/profile_01/coordinator",
                RouteClass::ProfileCoordinatorApi,
            ),
        ];

        for (method, path, expected) in routes {
            let actual = classify_route(method, path);
            assert_eq!(actual, expected);
            assert!(is_authenticated_api(actual));
        }
    }

    #[test]
    fn generation_routes_are_specific_and_authenticated() {
        let routes = [
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations",
                RouteClass::ProfileGenerationCollectionApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01",
                RouteClass::ProfileGenerationResourceApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/verify",
                RouteClass::ProfileGenerationVerifyApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/activate",
                RouteClass::ProfileGenerationActivateApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/deactivate",
                RouteClass::ProfileGenerationDeactivateApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/quarantine",
                RouteClass::ProfileGenerationQuarantineApi,
            ),
        ];
        for (method, path, expected) in routes {
            let actual = classify_route(method, path);
            assert_eq!(actual, expected);
            assert!(is_authenticated_api(actual));
        }
    }

    #[test]
    fn mailbox_routes_are_specific_and_authenticated() {
        let routes = [
            (
                "POST",
                "/api/v1/tenants/tenant_01/mailboxes",
                RouteClass::MailboxBindingCollectionApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/mailboxes/mailbox_01",
                RouteClass::MailboxBindingResourceApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/revoke",
                RouteClass::MailboxBindingRevokeApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs",
                RouteClass::MailboxJobCollectionApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs/mailjob_01",
                RouteClass::MailboxJobResourceApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs/mailjob_01/run",
                RouteClass::MailboxJobRunApi,
            ),
        ];
        for (method, path, expected) in routes {
            let actual = classify_route(method, path);
            assert_eq!(actual, expected);
            assert!(is_authenticated_api(actual));
        }
    }

    #[test]
    fn notification_routes_are_specific_authenticated_and_fail_closed() {
        let routes = [
            (
                "GET",
                "/api/v1/tenants/tenant_01/notifications/events",
                RouteClass::NotificationEventCollectionApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/notifications/events/ack",
                RouteClass::NotificationEventAckApi,
            ),
            (
                "POST",
                "/api/v1/tenants/tenant_01/notifications/replays",
                RouteClass::NotificationReplayCollectionApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/notifications/operations",
                RouteClass::NotificationOperationsApi,
            ),
        ];
        for (method, path, expected) in routes {
            let actual = classify_route(method, path);
            assert_eq!(actual, expected);
            assert!(is_authenticated_api(actual));
        }
        for (method, path) in [
            ("POST", "/api/v1/tenants/tenant_01/notifications/events"),
            ("GET", "/api/v1/tenants/tenant_01/notifications/replays"),
            ("POST", "/api/v1/tenants/tenant_01/notifications/operations"),
        ] {
            assert_eq!(
                classify_route(method, path),
                RouteClass::DynamicRouteNotFound
            );
        }
    }

    #[test]
    fn versioned_resource_wrong_methods_never_fall_back_to_static_assets() {
        for (method, path) in [
            (
                "GET",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations",
            ),
            (
                "DELETE",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01",
            ),
            (
                "PUT",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/verify",
            ),
            (
                "DELETE",
                "/api/v1/tenants/tenant_01/profiles/profile_01/generations/generation_01/deactivate",
            ),
            ("PUT", "/api/v1/tenants/tenant_01/clients/client_01/archive"),
            ("GET", "/api/v1/tenants/tenant_01/clients/client_01/merge"),
            ("POST", "/api/v1/tenants/tenant_01/clients/client_01/history"),
            ("POST", "/api/v1/tenants/tenant_01/clients/client_01/contacts/contact_01"),
            ("GET", "/api/v1/tenants/tenant_01/mailboxes"),
            ("DELETE", "/api/v1/tenants/tenant_01/mailboxes/mailbox_01"),
            (
                "PUT",
                "/api/v1/tenants/tenant_01/mailboxes/mailbox_01/jobs/mailjob_01/run",
            ),
            ("DELETE", "/api/v1/tenants/tenant_01/notifications/events"),
        ] {
            assert_eq!(
                classify_route(method, path),
                RouteClass::DynamicRouteNotFound
            );
        }
    }

    #[test]
    fn coordinator_wrong_method_never_falls_back_to_static_assets() {
        assert_eq!(
            classify_route(
                "DELETE",
                "/api/v1/tenants/tenant_01/profiles/profile_01/coordinator"
            ),
            RouteClass::DynamicRouteNotFound
        );
    }

    #[test]
    fn health_payload_is_contract_versioned() {
        let payload = health_payload();
        assert_eq!(payload.status, "ok");
        assert_eq!(payload.contract_version, "v1");
    }
}
