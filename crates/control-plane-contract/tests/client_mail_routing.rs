use control_plane_contract::{RouteClass, classify_route, is_authenticated_api};

#[test]
fn canonical_client_mail_routes_are_authenticated() {
    for (path, expected) in [
        (
            "/api/v1/tenants/tenant_01/clients/client_01/mail/search",
            RouteClass::ClientMailSearchApi,
        ),
        (
            "/api/v1/tenants/tenant_01/clients/client_01/mail/message",
            RouteClass::ClientMailMessageApi,
        ),
    ] {
        let route = classify_route("POST", path);
        assert_eq!(route, expected);
        assert!(is_authenticated_api(route));
    }
}

#[test]
fn malformed_client_mail_routes_fail_closed_and_never_become_static_assets() {
    for (method, path) in [
        (
            "GET",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/search",
        ),
        (
            "PUT",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/message",
        ),
        (
            "POST",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/unknown",
        ),
        (
            "POST",
            "/api/v2/tenants/tenant_01/clients/client_01/mail/search",
        ),
        (
            "POST",
            "/api/v1/tenants/tenant_01/clients/client_01/mail/search/extra",
        ),
    ] {
        let route = classify_route(method, path);
        assert_eq!(route, RouteClass::DynamicRouteNotFound, "{method} {path}");
        assert!(!is_authenticated_api(route));
        assert_ne!(route, RouteClass::StaticAssets);
    }
}
