use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "profiles"] if matches!(method, "GET" | "POST") => {
            Some(RouteClass::ProfileCollectionApi)
        }
        ["api", "v1", "tenants", _, "profiles", _, "launch"] if method == "POST" => {
            Some(RouteClass::ProfileResourceApi)
        }
        ["api", "v1", "tenants", _, "profiles", _, "coordinator"]
            if matches!(method, "GET" | "POST") =>
        {
            Some(RouteClass::ProfileCoordinatorApi)
        }
        ["api", "v1", "tenants", _, "profiles", _, "assignment"]
            if matches!(method, "PUT" | "DELETE") =>
        {
            Some(RouteClass::ProfileAssignmentApi)
        }
        ["api", "v1", "tenants", _, "profiles", _, "grants", _]
            if matches!(method, "PUT" | "DELETE") =>
        {
            Some(RouteClass::ProfileGrantApi)
        }
        ["api", "v1", "tenants", _, "profiles", _] if method == "GET" => {
            Some(RouteClass::ProfileResourceApi)
        }
        _ => None,
    }
}
