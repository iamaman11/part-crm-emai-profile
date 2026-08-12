use crate::clients::{ClientCreateWrite, ClientGrantRole};

const CLIENT_CREATOR_GRANT_REASON: &str = "client creator access";

/// Server-owned ACL semantics attached to every accepted Client creation write.
///
/// The browser cannot select this grant. `ClientApplicationPort::create_client`
/// implementations must persist the Client, this creator grant and command evidence
/// as one atomic application outcome.
pub trait ClientCreateGrantSpec {
    fn creator_grant_role(&self) -> ClientGrantRole;
    fn creator_grant_reason(&self) -> &'static str;
}

impl ClientCreateGrantSpec for ClientCreateWrite {
    fn creator_grant_role(&self) -> ClientGrantRole {
        ClientGrantRole::Editor
    }

    fn creator_grant_reason(&self) -> &'static str {
        CLIENT_CREATOR_GRANT_REASON
    }
}

#[cfg(test)]
mod tests {
    use super::ClientCreateGrantSpec;
    use crate::CommandExecutionEvidence;
    use crate::clients::{ClientCreateWrite, ClientGrantRole};
    use client_domain::{ClientKind, ClientRecord};
    use profile_platform_primitives::{
        AuditEventId, ClientId, IdempotencyKey, OutboxEventId, TenantId, UnixMillis,
    };

    #[test]
    fn client_create_contract_owns_editor_creator_grant() -> Result<(), Box<dyn std::error::Error>> {
        let write = ClientCreateWrite::new(
            ClientRecord::create(
                TenantId::parse("tenant_01JCREATORGRANT")?,
                ClientId::parse("client_01JCREATORGRANT")?,
                ClientKind::Person,
                "Creator Grant Client",
            )?,
            "Creator Grant Client",
            CommandExecutionEvidence::new(
                IdempotencyKey::parse("idem_01JCREATORGRANT")?,
                "digest_01JCREATORGRANT",
                AuditEventId::parse("audit_01JCREATORGRANT")?,
                OutboxEventId::parse("outbox_01JCREATORGRANT")?,
                UnixMillis::new(10),
                UnixMillis::new(20),
            ),
            "{}",
        );

        assert_eq!(write.creator_grant_role(), ClientGrantRole::Editor);
        assert_eq!(write.creator_grant_reason(), "client creator access");
        Ok(())
    }
}
