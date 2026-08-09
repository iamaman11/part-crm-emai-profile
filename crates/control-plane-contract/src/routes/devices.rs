use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "device-jobs", "claimable"] if method == "GET" => {
            Some(RouteClass::DeviceJobClaimableApi)
        }
        ["api", "v1", "tenants", _, "device-jobs", _, "claim"] if method == "POST" => {
            Some(RouteClass::DeviceJobClaimApi)
        }
        ["api", "v1", "tenants", _, "device-jobs", _, "heartbeat"] if method == "POST" => {
            Some(RouteClass::DeviceJobHeartbeatApi)
        }
        ["api", "v1", "tenants", _, "device-jobs", _, "outcome"] if method == "POST" => {
            Some(RouteClass::DeviceJobOutcomeApi)
        }
        _ => None,
    }
}
