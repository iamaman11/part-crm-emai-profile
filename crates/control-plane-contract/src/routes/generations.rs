use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "profiles", _, "generations"] if method == "POST" => {
            Some(RouteClass::ProfileGenerationCollectionApi)
        }
        [
            "api",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generations",
            _,
            "verify",
        ] if method == "POST" => Some(RouteClass::ProfileGenerationVerifyApi),
        [
            "api",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generations",
            _,
            "activate",
        ] if method == "POST" => Some(RouteClass::ProfileGenerationActivateApi),
        [
            "api",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generations",
            _,
            "deactivate",
        ] if method == "POST" => Some(RouteClass::ProfileGenerationDeactivateApi),
        [
            "api",
            "v1",
            "tenants",
            _,
            "profiles",
            _,
            "generations",
            _,
            "quarantine",
        ] if method == "POST" => Some(RouteClass::ProfileGenerationQuarantineApi),
        ["api", "v1", "tenants", _, "profiles", _, "generations", _] if method == "GET" => {
            Some(RouteClass::ProfileGenerationResourceApi)
        }
        _ => None,
    }
}
