#![forbid(unsafe_code)]

pub const D1_CATALOG_BINDING: &str = "CATALOG_DB";
pub const R2_PROFILES_BINDING: &str = "PROFILE_OBJECTS";
pub const VERIFICATION_QUEUE_BINDING: &str = "GENERATION_VERIFICATION";
pub const PROFILE_COORDINATOR_BINDING: &str = "PROFILE_COORDINATOR";
pub const STATIC_ASSETS_BINDING: &str = "ASSETS";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteClass {
    ApiHealth,
    ApiBindingProbe,
    BrowserAsset,
    BridgeDeniedByDefault,
}

#[must_use]
pub fn classify_route(path: &str) -> RouteClass {
    match path {
        "/api/v1/health" => RouteClass::ApiHealth,
        "/api/v1/bindings" => RouteClass::ApiBindingProbe,
        path if path.starts_with("/bridge/") => RouteClass::BridgeDeniedByDefault,
        _ => RouteClass::BrowserAsset,
    }
}

#[cfg(test)]
mod tests {
    use super::{RouteClass, classify_route};

    #[test]
    fn classifies_worker_first_api_routes() {
        assert_eq!(classify_route("/api/v1/health"), RouteClass::ApiHealth);
        assert_eq!(
            classify_route("/api/v1/bindings"),
            RouteClass::ApiBindingProbe
        );
    }

    #[test]
    fn bridge_routes_fail_closed_before_device_protocol_exists() {
        assert_eq!(
            classify_route("/bridge/claim/code"),
            RouteClass::BridgeDeniedByDefault
        );
    }

    #[test]
    fn all_other_routes_are_static_asset_candidates() {
        assert_eq!(classify_route("/profiles"), RouteClass::BrowserAsset);
    }
}
