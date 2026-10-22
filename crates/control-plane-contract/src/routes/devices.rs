use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "devices", "installer"] if method == "GET" => {
            Some(RouteClass::DeviceInstallerApi)
        }
        ["api", "v1", "tenants", _, "devices", _, "registration"] if method == "GET" => {
            Some(RouteClass::DeviceRegistrationApi)
        }
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

#[cfg(test)]
mod tests {
    use crate::{RouteClass, classify_route, is_authenticated_api};

    #[test]
    fn installer_is_only_an_authenticated_read() {
        let path = "/api/v1/tenants/tenant_01JTEST/devices/installer";
        assert_eq!(classify_route("GET", path), RouteClass::DeviceInstallerApi);
        assert!(is_authenticated_api(RouteClass::DeviceInstallerApi));
        assert_eq!(
            classify_route("POST", path),
            RouteClass::DynamicRouteNotFound
        );
        assert_eq!(
            classify_route("GET", &format!("{path}/latest")),
            RouteClass::DynamicRouteNotFound
        );
    }

    #[test]
    fn registration_is_an_authenticated_read_not_a_machine_enrollment_route() {
        let path = "/api/v1/tenants/tenant_01JTEST/devices/device_01JTEST/registration";
        assert_eq!(
            classify_route("GET", path),
            RouteClass::DeviceRegistrationApi
        );
        assert!(is_authenticated_api(RouteClass::DeviceRegistrationApi));
        for method in ["POST", "PUT", "DELETE"] {
            assert_eq!(
                classify_route(method, path),
                RouteClass::DynamicRouteNotFound
            );
        }
        assert_eq!(
            classify_route("GET", &path.replace("/v1/", "/v2/")),
            RouteClass::DynamicRouteNotFound
        );
    }
}
