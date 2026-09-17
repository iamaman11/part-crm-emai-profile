use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, path: &str) -> Option<RouteClass> {
    match (method, path) {
        ("GET", "/api/v1/health") => Some(RouteClass::HealthApi),
        ("GET", "/api/v1/bindings") => Some(RouteClass::BindingProbeApi),
        ("GET", "/api/v1/session") => Some(RouteClass::AuthenticatedSessionApi),
        ("GET", "/api/v1/session/tenants") => Some(RouteClass::AuthenticatedTenantContextsApi),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::classify;
    use crate::RouteClass;

    #[test]
    fn tenant_context_bootstrap_is_exact_authenticated_session_surface() {
        assert_eq!(
            classify("GET", "/api/v1/session/tenants"),
            Some(RouteClass::AuthenticatedTenantContextsApi)
        );
        for (method, path) in [
            ("POST", "/api/v1/session/tenants"),
            ("GET", "/api/v1/session/tenants/extra"),
            ("GET", "/api/v1/session/tenant"),
        ] {
            assert_eq!(classify(method, path), None);
        }
    }
}
