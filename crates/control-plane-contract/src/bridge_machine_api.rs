use profile_platform_primitives::{ProfileId, TenantId};

pub const PROFILE_LAUNCH_REDEMPTION_PATH: &str = "/bridge/v1/profile-launch/redemptions";

#[must_use]
pub fn profile_coordinator_path(tenant_id: &TenantId, profile_id: &ProfileId) -> String {
    format!(
        "/bridge/v1/tenants/{}/profiles/{}/coordinator",
        tenant_id.as_str(),
        profile_id.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::{PROFILE_LAUNCH_REDEMPTION_PATH, profile_coordinator_path};
    use crate::{RouteClass, classify_route};
    use profile_platform_primitives::{ProfileId, TenantId};

    #[test]
    fn canonical_machine_routes_resolve_to_existing_ingress() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            classify_route("POST", PROFILE_LAUNCH_REDEMPTION_PATH),
            RouteClass::ProfileLaunchApi
        );

        let tenant_id = TenantId::parse("tenant_01JBRIDGE")?;
        let profile_id = ProfileId::parse("profile_01JBRIDGE")?;
        let coordinator = profile_coordinator_path(&tenant_id, &profile_id);
        assert_eq!(
            classify_route("POST", &coordinator),
            RouteClass::ProfileCoordinatorApi
        );
        assert_eq!(
            classify_route("GET", &coordinator),
            RouteClass::ProfileCoordinatorApi
        );
        Ok(())
    }

    #[test]
    fn nearby_machine_routes_remain_denied_by_default() {
        for (method, path) in [
            ("GET", PROFILE_LAUNCH_REDEMPTION_PATH),
            ("POST", "/bridge/v2/profile-launch/redemptions"),
            ("POST", "/bridge/v1/profile-launch/redemptions/extra"),
            (
                "POST",
                "/bridge/v2/tenants/tenant_01JBRIDGE/profiles/profile_01JBRIDGE/coordinator",
            ),
        ] {
            assert_eq!(classify_route(method, path), RouteClass::BridgeDeniedByDefault);
        }
    }
}
