use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "device-pairings", "requests"] if method == "POST" => {
            Some(RouteClass::DevicePairingCollectionApi)
        }
        [
            "api",
            "v1",
            "tenants",
            _,
            "device-pairings",
            "authorizations",
        ] if method == "POST" => Some(RouteClass::DevicePairingAuthorizationApi),
        ["api", "v1", "tenants", _, "device-pairings", "completions"] if method == "POST" => {
            Some(RouteClass::DevicePairingCompletionApi)
        }
        [
            "api",
            "v1",
            "tenants",
            _,
            "devices",
            _,
            "session-challenges",
        ] if method == "POST" => Some(RouteClass::DeviceSessionChallengeApi),
        ["api", "v1", "tenants", _, "devices", _, "sessions"] if method == "POST" => {
            Some(RouteClass::DeviceSessionCollectionApi)
        }
        ["api", "v1", "tenants", _, "device-jobs", "claimable"] if method == "GET" => {
            Some(RouteClass::DeviceJobClaimableApi)
        }
        ["api", "v1", "tenants", _, "device-jobs", _, "claim"] if method == "POST" => {
            Some(RouteClass::DeviceJobClaimApi)
        }
        ["api", "v1", "tenants", _, "device-jobs", _, "heartbeat"] if method == "POST" => {
            Some(RouteClass::DeviceJobHeartbeatApi)
        }
        [
            "api",
            "v1",
            "tenants",
            _,
            "device-jobs",
            _,
            "generation-upload-capability",
        ] if method == "POST" => Some(RouteClass::DeviceGenerationUploadCapabilityApi),
        [
            "api",
            "v1",
            "tenants",
            _,
            "device-jobs",
            _,
            "generation-commit",
        ] if method == "POST" => Some(RouteClass::DeviceGenerationCommitApi),
        ["api", "v1", "tenants", _, "device-jobs", _, "outcome"] if method == "POST" => {
            Some(RouteClass::DeviceJobOutcomeApi)
        }
        _ => None,
    }
}
