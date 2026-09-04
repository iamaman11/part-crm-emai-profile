use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "owner", "bootstrap"] if method == "POST" => {
            Some(RouteClass::OwnerBootstrapApi)
        }
        ["api", "v1", "tenants", _, "owner", "transfer"] if method == "POST" => {
            Some(RouteClass::OwnerTransferApi)
        }
        ["api", "v1", "tenants", _, "invitations"] if method == "POST" => {
            Some(RouteClass::InvitationCollectionApi)
        }
        ["api", "v1", "tenants", _, "invitations", _, "accept"] if method == "POST" => {
            Some(RouteClass::InvitationAcceptApi)
        }
        ["api", "v1", "tenants", _, "members"] if method == "GET" => {
            Some(RouteClass::MembershipCollectionApi)
        }
        ["api", "v1", "tenants", _, "members", _, "status"] if method == "PUT" => {
            Some(RouteClass::MembershipStatusApi)
        }
        ["api", "v1", "tenants", _, "members", _, "device-binding"] if method == "PUT" => {
            Some(RouteClass::DeviceBindingResourceApi)
        }
        ["api", "v1", "tenants", _, "members", _, "device-binding"] if method == "DELETE" => {
            Some(RouteClass::DeviceBindingRevokeApi)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::classify;
    use crate::RouteClass;

    #[test]
    fn device_binding_governance_is_exact_and_fail_closed() {
        let segments = [
            "api",
            "v1",
            "tenants",
            "tenant_01",
            "members",
            "actor_01",
            "device-binding",
        ];
        assert_eq!(
            classify("PUT", &segments),
            Some(RouteClass::DeviceBindingResourceApi)
        );
        assert_eq!(
            classify("DELETE", &segments),
            Some(RouteClass::DeviceBindingRevokeApi)
        );
        for method in ["GET", "POST", "PATCH"] {
            assert_eq!(classify(method, &segments), None);
        }
    }
}
