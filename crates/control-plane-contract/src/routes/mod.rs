mod clients;
mod foundation;
mod generations;
mod identity;
mod mailboxes;
mod profiles;

use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, path: &str) -> RouteClass {
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

    if is_dynamic_namespace(path) {
        RouteClass::DynamicRouteNotFound
    } else {
        RouteClass::StaticAssets
    }
}

#[must_use]
fn is_bridge_namespace(path: &str) -> bool {
    path == "/bridge" || path.starts_with("/bridge/")
}

#[must_use]
fn is_dynamic_namespace(path: &str) -> bool {
    path == "/api"
        || path.starts_with("/api/")
        || path == "/auth"
        || path.starts_with("/auth/")
        || is_bridge_namespace(path)
}
