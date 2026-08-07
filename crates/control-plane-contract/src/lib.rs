#![forbid(unsafe_code)]

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
    ClientGrantApi,
    ProfileCollectionApi,
    ProfileResourceApi,
    ProfileAssignmentApi,
    ProfileGrantApi,
    ProfileCoordinatorApi,
    DynamicRouteNotFound,
    BridgeDeniedByDefault,
    StaticAssets,
}

#[must_use]
pub fn classify_route(method: &str, path: &str) -> RouteClass {
    if method == "GET" && path == "/api/v1/health" {
        return RouteClass::HealthApi;
    }
    if method == "GET" && path == "/api/v1/bindings" {
        return RouteClass::BindingProbeApi;
    }
    if path == "/bridge" || path.starts_with("/bridge/") {
        return RouteClass::BridgeDeniedByDefault;
    }
    if method == "GET" && path == "/api/v1/session" {
        return RouteClass::AuthenticatedSessionApi;
    }

    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let route = match segments.as_slice() {
        ["api", "v1", "tenants", _, "owner", "bootstrap"] if method == "POST" => {
            Some(RouteClass::OwnerBootstrapApi)
        }
        ["api", "v1", "tenants", _, "owner", "transfer"] if method == "POST" => {
            Some(RouteClass::OwnerTransferApi)
        }
        ["api", "v1", "tenants", _, "invitations"] if method == "POST" => {
            Some(RouteClass::InvitationCollectionApi)
        }
        ["api", "v1", "tenants", _, "invitations", _, "accept"] if method == "POST" => {
            Some(RouteClass::InvitationAcceptApi)
        }
        ["api", "v1", "tenants", _, "members", _, "status"] if method == "PUT" => {
            Some(RouteClass::MembershipStatusApi)
        }
        ["api", "v1", "tenants", _, "clients"] if method == "POST" => {
            Some(RouteClass::ClientCollectionApi)
        }
        ["api", "v1", "tenants", _, "clients", _] if method == "GET" => {
            Some(RouteClass::ClientResourceApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "grants", _]
            if matches!(method, "PUT" | "DELETE") =>
        {
            Some(RouteClass::ClientGrantApi)
        }
        ["api", "v1", "tenants", _, "profiles"] if method == "POST" => {
            Some(RouteClass::ProfileCollectionApi)
        }
        ["api", "v1", "tenants", _, "profiles", _, "coordinator"]
            if matches!(method, "GET" | "POST") =>
        {
            Some(RouteClass::ProfileCoordinatorApi)
        }
        ["api", "v1", "tenants", _, "profiles", _] if method == "GET" => {
            Some(RouteClass::ProfileResourceApi)
        }
        ["api", "v1", "tenants", _, "profiles", _, "assignment"] if method == "PUT" => {
            Some(RouteClass::ProfileAssignmentApi)
        }
        ["api", "v1", "tenants", _, "profiles", _, "grants", _]
            if matches!(method, "PUT" | "DELETE") =>
        {
            Some(RouteClass::ProfileGrantApi)
        }
        _ => None,
    };
    route.unwrap_or_else(|| {
        if is_dynamic_path(path) {
            RouteClass::DynamicRouteNotFound
        } else {
            RouteClass::StaticAssets
        }
    })
}

#[must_use]
fn is_dynamic_path(path: &str) -> bool {
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
    fn unknown_api_methods_and_versions_fail_closed() {
        for (method, path) in [
            ("POST", "/api/v1/health"),
            ("GET", "/api/v2/health"),
            ("GET", "/api/v1/unknown"),
            ("GET", "/api"),
            ("GET", "/auth/unknown"),
        ] {
            let route = classify_route(method, path);
            assert_eq!(route, RouteClass::DynamicRouteNotFound);
            assert!(!is_authenticated_api(route));
        }
    }

    #[test]
    fn owner_member_acl_and_coordinator_routes_are_versioned_and_authenticated() {
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
                "PUT",
                "/api/v1/tenants/tenant_01/members/actor_01/status",
                RouteClass::MembershipStatusApi,
            ),
            (
                "GET",
                "/api/v1/tenants/tenant_01/clients/client_01",
                RouteClass::ClientResourceApi,
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
