use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "clients"] if matches!(method, "GET" | "POST") => {
            Some(RouteClass::ClientCollectionApi)
        }
        ["api", "v1", "tenants", _, "clients", _] if matches!(method, "GET" | "PATCH") => {
            Some(RouteClass::ClientResourceApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "archive"] if method == "POST" => {
            Some(RouteClass::ClientArchiveApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "contacts", _]
            if matches!(method, "PUT" | "DELETE") =>
        {
            Some(RouteClass::ClientContactApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "merge"] if method == "POST" => {
            Some(RouteClass::ClientMergeApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "history"] if method == "GET" => {
            Some(RouteClass::ClientHistoryApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "grants", _]
            if matches!(method, "PUT" | "DELETE") =>
        {
            Some(RouteClass::ClientGrantApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "mail", "search"] if method == "POST" => {
            Some(RouteClass::ClientMailSearchApi)
        }
        [
            "api",
            "v1",
            "tenants",
            _,
            "clients",
            _,
            "mail",
            "message" | "send",
        ] if method == "POST" => Some(RouteClass::ClientMailMessageApi),
        _ => None,
    }
}
