mod client_mail;
mod clients;
mod devices;
mod foundation;
mod generations;
mod identity;
mod mailboxes;
mod notifications;
mod profiles;

use crate::{RouteClass, is_dynamic_path};

pub(crate) const BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH: &str =
    "/bridge/v1/profile-launch/redemptions";

#[must_use]
pub(super) fn classify(method: &str, path: &str) -> RouteClass {
    if method == "POST" && path == BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH {
        return RouteClass::ProfileLaunchApi;
    }
    if bridge_profile_coordinator_route(method, path) {
        return RouteClass::ProfileCoordinatorApi;
    }
    if is_bridge_namespace(path) {
        return RouteClass::BridgeDeniedByDefault;
    }

    if let Some(route) = foundation::classify(method, path) {
        return route;
    }

    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let segments = segments.as_slice();

    if let Some(route) = identity::classify(method, segments) {
        return route;
    }
    if let Some(route) = client_mail::classify(method, segments) {
        return route;
    }
    if let Some(route) = clients::classify(method, segments) {
        return route;
    }
    if let Some(route) = generations::classify(method, segments) {
        return route;
    }
    if let Some(route) = profiles::classify(method, segments) {
        return route;
    }
    if let Some(route) = mailboxes::classify(method, segments) {
        return route;
    }
    if let Some(route) = devices::classify(method, segments) {
        return route;
    }
    if let Some(route) = notifications::classify(method, segments) {
        return route;
    }

    if is_dynamic_path(path) {
        RouteClass::DynamicRouteNotFound
    } else {
        RouteClass::StaticAssets
    }
}

#[must_use]
fn bridge_profile_coordinator_route(method: &str, path: &str) -> bool {
    if !matches!(method, "GET" | "POST") {
        return false;
    }
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    matches!(
        segments.as_slice(),
        ["bridge", "v1", "tenants", _, "profiles", _, "coordinator"]
    )
}

#[must_use]
fn is_bridge_namespace(path: &str) -> bool {
    path == "/bridge" || path.starts_with("/bridge/")
}

#[cfg(test)]
mod tests {
    use super::{BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH, classify};
    use crate::RouteClass;

    #[test]
    fn only_exact_post_profile_launch_redemption_escapes_bridge_deny_default() {
        assert_eq!(
            classify("POST", BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH),
            RouteClass::ProfileLaunchApi
        );
        for (method, path) in [
            ("GET", BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH),
            ("PUT", BRIDGE_PROFILE_LAUNCH_REDEMPTION_PATH),
            ("POST", "/bridge/v2/profile-launch/redemptions"),
            ("POST", "/bridge/v1/profile-launch/redemptions/extra"),
            ("POST", "/bridge/v1/profile-launch"),
        ] {
            assert_eq!(classify(method, path), RouteClass::BridgeDeniedByDefault);
        }
    }

    #[test]
    fn only_exact_bridge_profile_coordinator_surface_escapes_deny_default() {
        let path = "/bridge/v1/tenants/tenant_01/profiles/profile_01/coordinator";
        assert_eq!(classify("GET", path), RouteClass::ProfileCoordinatorApi);
        assert_eq!(classify("POST", path), RouteClass::ProfileCoordinatorApi);
        for (method, invalid) in [
            ("DELETE", path),
            ("PUT", path),
            ("GET", "/bridge/v2/tenants/tenant_01/profiles/profile_01/coordinator"),
            ("GET", "/bridge/v1/tenants/tenant_01/profiles/profile_01/coordinator/extra"),
            ("GET", "/bridge/v1/tenants/tenant_01/profiles/coordinator"),
        ] {
            assert_eq!(classify(method, invalid), RouteClass::BridgeDeniedByDefault);
        }
    }
}
