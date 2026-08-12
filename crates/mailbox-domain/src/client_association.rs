use core::fmt;
use profile_platform_primitives::{ClientId, MailboxBindingId, TenantId};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MailboxClientAssociationVersion(u64);

impl MailboxClientAssociationVersion {
    pub const NEVER_ASSOCIATED: Self = Self(0);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Result<Self, MailboxClientAssociationError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(MailboxClientAssociationError::VersionOverflow)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxClientAssociationAction {
    Bind,
    Rebind,
    Unbind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxClientAssociationError {
    VersionConflict,
    AlreadyAssociated,
    AlreadyUnassigned,
    VersionOverflow,
}

impl fmt::Display for MailboxClientAssociationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::VersionConflict => "mailbox client association version conflict",
            Self::AlreadyAssociated => "mailbox is already associated with this client",
            Self::AlreadyUnassigned => "mailbox is already unassigned",
            Self::VersionOverflow => "mailbox client association version overflow",
        })
    }
}

impl std::error::Error for MailboxClientAssociationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailboxClientAssociation {
    tenant_id: TenantId,
    binding_id: MailboxBindingId,
    client_id: Option<ClientId>,
    version: MailboxClientAssociationVersion,
}

impl MailboxClientAssociation {
    #[must_use]
    pub const fn unassigned(tenant_id: TenantId, binding_id: MailboxBindingId) -> Self {
        Self {
            tenant_id,
            binding_id,
            client_id: None,
            version: MailboxClientAssociationVersion::NEVER_ASSOCIATED,
        }
    }

    #[must_use]
    pub const fn restore(
        tenant_id: TenantId,
        binding_id: MailboxBindingId,
        client_id: Option<ClientId>,
        version: MailboxClientAssociationVersion,
    ) -> Self {
        Self {
            tenant_id,
            binding_id,
            client_id,
            version,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn client_id(&self) -> Option<&ClientId> {
        self.client_id.as_ref()
    }

    #[must_use]
    pub const fn version(&self) -> MailboxClientAssociationVersion {
        self.version
    }

    pub fn associate(
        &mut self,
        expected_version: MailboxClientAssociationVersion,
        client_id: ClientId,
    ) -> Result<MailboxClientAssociationAction, MailboxClientAssociationError> {
        self.require_version(expected_version)?;
        if self.client_id.as_ref() == Some(&client_id) {
            return Err(MailboxClientAssociationError::AlreadyAssociated);
        }
        let action = if self.client_id.is_some() {
            MailboxClientAssociationAction::Rebind
        } else {
            MailboxClientAssociationAction::Bind
        };
        self.version = self.version.next()?;
        self.client_id = Some(client_id);
        Ok(action)
    }

    pub fn unbind(
        &mut self,
        expected_version: MailboxClientAssociationVersion,
    ) -> Result<MailboxClientAssociationAction, MailboxClientAssociationError> {
        self.require_version(expected_version)?;
        if self.client_id.is_none() {
            return Err(MailboxClientAssociationError::AlreadyUnassigned);
        }
        self.version = self.version.next()?;
        self.client_id = None;
        Ok(MailboxClientAssociationAction::Unbind)
    }

    fn require_version(
        &self,
        expected_version: MailboxClientAssociationVersion,
    ) -> Result<(), MailboxClientAssociationError> {
        if expected_version == self.version {
            Ok(())
        } else {
            Err(MailboxClientAssociationError::VersionConflict)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MailboxClientAssociation, MailboxClientAssociationAction, MailboxClientAssociationError,
        MailboxClientAssociationVersion,
    };
    use profile_platform_primitives::{ClientId, MailboxBindingId, TenantId};

    fn association() -> Result<MailboxClientAssociation, Box<dyn std::error::Error>> {
        Ok(MailboxClientAssociation::unassigned(
            TenantId::parse("tenant_01JMAILCLIENT")?,
            MailboxBindingId::parse("mailbox_01JMAILCLIENT")?,
        ))
    }

    #[test]
    fn bind_rebind_unbind_are_versioned_independently_from_mailbox()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut relationship = association()?;
        assert_eq!(relationship.version().value(), 0);
        assert_eq!(relationship.client_id(), None);

        assert_eq!(
            relationship.associate(
                MailboxClientAssociationVersion::NEVER_ASSOCIATED,
                ClientId::parse("client_01JMAILCLIENTA")?,
            )?,
            MailboxClientAssociationAction::Bind
        );
        assert_eq!(relationship.version().value(), 1);

        assert_eq!(
            relationship.associate(
                MailboxClientAssociationVersion::new(1),
                ClientId::parse("client_01JMAILCLIENTB")?,
            )?,
            MailboxClientAssociationAction::Rebind
        );
        assert_eq!(relationship.version().value(), 2);

        assert_eq!(
            relationship.unbind(MailboxClientAssociationVersion::new(2))?,
            MailboxClientAssociationAction::Unbind
        );
        assert_eq!(relationship.version().value(), 3);
        assert_eq!(relationship.client_id(), None);
        Ok(())
    }

    #[test]
    fn stale_same_target_and_repeat_unbind_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut relationship = association()?;
        let client = ClientId::parse("client_01JMAILCLIENTA")?;
        assert_eq!(
            relationship.associate(MailboxClientAssociationVersion::new(1), client.clone()),
            Err(MailboxClientAssociationError::VersionConflict)
        );
        relationship.associate(MailboxClientAssociationVersion::NEVER_ASSOCIATED, client.clone())?;
        assert_eq!(
            relationship.associate(MailboxClientAssociationVersion::new(1), client),
            Err(MailboxClientAssociationError::AlreadyAssociated)
        );
        relationship.unbind(MailboxClientAssociationVersion::new(1))?;
        assert_eq!(
            relationship.unbind(MailboxClientAssociationVersion::new(2)),
            Err(MailboxClientAssociationError::AlreadyUnassigned)
        );
        Ok(())
    }
}
