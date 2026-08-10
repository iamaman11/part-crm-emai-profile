use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "notifications", "events"] if method == "GET" => {
            Some(RouteClass::NotificationEventCollectionApi)
        }
        ["api", "v1", "tenants", _, "notifications", "realtime"] if method == "GET" => {
            // Realtime remains part of the notification collection ingress family. The owning
            // notification dispatcher distinguishes the WebSocket upgrade path; wrong methods
            // still fall through to DynamicRouteNotFound.
            Some(RouteClass::NotificationEventCollectionApi)
        }
        ["api", "v1", "tenants", _, "notifications", "events", "ack"] if method == "POST" => {
            Some(RouteClass::NotificationEventAckApi)
        }
        ["api", "v1", "tenants", _, "notifications", "replays"] if method == "POST" => {
            Some(RouteClass::NotificationReplayCollectionApi)
        }
        ["api", "v1", "tenants", _, "notifications", "operations"] if method == "GET" => {
            Some(RouteClass::NotificationOperationsApi)
        }
        _ => None,
    }
}
