#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteClass {
    HealthApi,
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
    StaticAssets,
}

#[must_use]
pub fn classify_route(method: &str, path: &str) -> RouteClass {
    if method == "GET" && path == "/api/v1/health" {
        return RouteClass::HealthApi;
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
    route.unwrap_or(RouteClass::StaticAssets)
}

#[must_use]
pub const fn is_authenticated_api(route: RouteClass) -> bool {
    !matches!(route, RouteClass::HealthApi | RouteClass::StaticAssets)
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
    fn only_exact_health_route_is_classified_as_health() {
        assert_eq!(
            classify_route("GET", "/api/v1/health"),
            RouteClass::HealthApi
        );
        assert_eq!(
            classify_route("POST", "/api/v1/health"),
            RouteClass::StaticAssets
        );
        assert_eq!(
            classify_route("GET", "/api/v2/health"),
            RouteClass::StaticAssets
        );
    }

    #[test]
    fn owner_member_and_acl_routes_are_versioned_and_authenticated() {
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
        ];

        for (method, path, expected) in routes {
            let actual = classify_route(method, path);
            assert_eq!(actual, expected);
            assert!(is_authenticated_api(actual));
        }
    }

    #[test]
    fn health_payload_is_contract_versioned() {
        let payload = health_payload();
        assert_eq!(payload.status, "ok");
        assert_eq!(payload.contract_version, "v1");
    }
}
