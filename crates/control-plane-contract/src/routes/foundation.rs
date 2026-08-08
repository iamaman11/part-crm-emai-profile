use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, path: &str) -> Option<RouteClass> {
    match (method, path) {
        ("GET", "/api/v1/health") => Some(RouteClass::HealthApi),
        ("GET", "/api/v1/bindings") => Some(RouteClass::BindingProbeApi),
        ("GET", "/api/v1/session") => Some(RouteClass::AuthenticatedSessionApi),
        _ => None,
    }
}
