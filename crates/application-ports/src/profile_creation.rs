use crate::profiles::{ProfileCreateWrite, ProfileGrantRole};

const PROFILE_CREATOR_GRANT_REASON: &str = "profile creator access";

/// Server-owned ACL semantics attached to every accepted Profile creation write.
///
/// The browser cannot select this grant. `ProfileApplicationPort::create_profile`
/// implementations must persist the Profile, this creator grant and command evidence
/// as one atomic application outcome.
pub trait ProfileCreateGrantSpec {
    fn creator_grant_role(&self) -> ProfileGrantRole;
    fn creator_grant_reason(&self) -> &'static str;
}

impl ProfileCreateGrantSpec for ProfileCreateWrite {
    fn creator_grant_role(&self) -> ProfileGrantRole {
        ProfileGrantRole::Operator
    }

    fn creator_grant_reason(&self) -> &'static str {
        PROFILE_CREATOR_GRANT_REASON
    }
}

#[cfg(test)]
mod tests {
    use super::ProfileCreateGrantSpec;
    use crate::CommandExecutionEvidence;
    use crate::profiles::{ProfileCreateWrite, ProfileGrantRole};
    use profile_domain::BrowserProfile;
    use profile_platform_primitives::{
        AuditEventId, IdempotencyKey, OutboxEventId, ProfileId, TenantId, UnixMillis,
    };

    #[test]
    fn profile_create_contract_owns_operator_creator_grant()
    -> Result<(), Box<dyn std::error::Error>> {
        let write = ProfileCreateWrite::new(
            BrowserProfile::create(
                TenantId::parse("tenant_01JPROFILECREATOR")?,
                ProfileId::parse("profile_01JPROFILECREATOR")?,
            ),
            CommandExecutionEvidence::new(
                IdempotencyKey::parse("idem_01JPROFILECREATOR")?,
                "digest_01JPROFILECREATOR",
                AuditEventId::parse("audit_01JPROFILECREATOR")?,
                OutboxEventId::parse("outbox_01JPROFILECREATOR")?,
                UnixMillis::new(10),
                UnixMillis::new(20),
            ),
            "{}",
        );

        assert_eq!(write.creator_grant_role(), ProfileGrantRole::Operator);
        assert_eq!(write.creator_grant_reason(), "profile creator access");
        Ok(())
    }
}
