use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
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
        _ => None,
    }
}
