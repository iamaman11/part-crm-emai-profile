use core::fmt;
use profile_platform_primitives::{AggregateVersion, ClientId, TenantId};

const MAX_DISPLAY_NAME_LENGTH: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientKind {
    Person,
    Organization,
}

impl ClientKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Organization => "ORGANIZATION",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientStatus {
    Active,
    Archived,
    Merged,
}

impl ClientStatus {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Archived => "ARCHIVED",
            Self::Merged => "MERGED",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientRecord {
    tenant_id: TenantId,
    client_id: ClientId,
    version: AggregateVersion,
    kind: ClientKind,
    display_name: String,
    status: ClientStatus,
}

impl ClientRecord {
    pub fn create(
        tenant_id: TenantId,
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
    ) -> Result<Self, ClientError> {
        Self::restore(
            tenant_id,
            client_id,
            AggregateVersion::INITIAL,
            kind,
            display_name,
            ClientStatus::Active,
        )
    }

    pub fn restore(
        tenant_id: TenantId,
        client_id: ClientId,
        version: AggregateVersion,
        kind: ClientKind,
        display_name: impl Into<String>,
        status: ClientStatus,
    ) -> Result<Self, ClientError> {
        let display_name = normalize_display_name(display_name.into())?;
        Ok(Self {
            tenant_id,
            client_id,
            version,
            kind,
            display_name,
            status,
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn kind(&self) -> ClientKind {
        self.kind
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn status(&self) -> ClientStatus {
        self.status
    }

    pub fn rename(&mut self, display_name: impl Into<String>) -> Result<(), ClientError> {
        if self.status != ClientStatus::Active {
            return Err(ClientError::InvalidStatusTransition);
        }
        let display_name = normalize_display_name(display_name.into())?;
        let next_version = self
            .version
            .next()
            .map_err(|_| ClientError::VersionOverflow)?;
        self.display_name = display_name;
        self.version = next_version;
        Ok(())
    }

    pub fn archive(&mut self) -> Result<(), ClientError> {
        if self.status != ClientStatus::Active {
            return Err(ClientError::InvalidStatusTransition);
        }
        let next_version = self
            .version
            .next()
            .map_err(|_| ClientError::VersionOverflow)?;
        self.status = ClientStatus::Archived;
        self.version = next_version;
        Ok(())
    }

    pub(crate) fn mark_merged(&mut self) -> Result<(), ClientError> {
        if self.status != ClientStatus::Active {
            return Err(ClientError::InvalidStatusTransition);
        }
        let next_version = self
            .version
            .next()
            .map_err(|_| ClientError::VersionOverflow)?;
        self.status = ClientStatus::Merged;
        self.version = next_version;
        Ok(())
    }
}

fn normalize_display_name(value: String) -> Result<String, ClientError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_DISPLAY_NAME_LENGTH {
        return Err(ClientError::InvalidDisplayName);
    }
    Ok(value.to_owned())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientError {
    InvalidDisplayName,
    InvalidStatusTransition,
    VersionOverflow,
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidDisplayName => "client display name is invalid",
            Self::InvalidStatusTransition => "client status transition is invalid",
            Self::VersionOverflow => "client version overflow",
        })
    }
}

impl std::error::Error for ClientError {}

#[cfg(test)]
mod tests {
    use super::{ClientError, ClientKind, ClientRecord, ClientStatus};
    use profile_platform_primitives::{AggregateVersion, ClientId, TenantId};

    fn active_client() -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::create(
            TenantId::parse("tenant_01JCLIENT")?,
            ClientId::parse("client_01JCLIENT")?,
            ClientKind::Person,
            "Synthetic Client",
        )?)
    }

    #[test]
    fn stable_vocabulary_is_explicit() {
        assert_eq!(ClientKind::Person.stable_code(), "PERSON");
        assert_eq!(ClientKind::Organization.stable_code(), "ORGANIZATION");
        assert_eq!(ClientStatus::Active.stable_code(), "ACTIVE");
        assert_eq!(ClientStatus::Archived.stable_code(), "ARCHIVED");
        assert_eq!(ClientStatus::Merged.stable_code(), "MERGED");
    }

    #[test]
    fn creation_normalizes_bounded_display_name() -> Result<(), Box<dyn std::error::Error>> {
        let client = ClientRecord::create(
            TenantId::parse("tenant_01JCLIENT")?,
            ClientId::parse("client_02JCLIENT")?,
            ClientKind::Person,
            "  Synthetic Client  ",
        )?;
        assert_eq!(client.display_name(), "Synthetic Client");
        Ok(())
    }

    #[test]
    fn rename_advances_version_only_after_validation() -> Result<(), Box<dyn std::error::Error>> {
        let mut client = active_client()?;
        assert_eq!(client.rename("  Renamed Client  "), Ok(()));
        assert_eq!(client.display_name(), "Renamed Client");
        assert_eq!(client.version().value(), 2);
        Ok(())
    }

    #[test]
    fn archive_overflow_does_not_partially_mutate_client() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut client = ClientRecord::restore(
            TenantId::parse("tenant_01JCLIENT")?,
            ClientId::parse("client_01JCLIENT")?,
            AggregateVersion::new(u64::MAX)?,
            ClientKind::Person,
            "Synthetic Client",
            ClientStatus::Active,
        )?;
        assert_eq!(client.archive(), Err(ClientError::VersionOverflow));
        assert_eq!(client.status(), ClientStatus::Active);
        assert_eq!(client.version().value(), u64::MAX);
        Ok(())
    }

    #[test]
    fn archived_client_cannot_be_renamed() -> Result<(), Box<dyn std::error::Error>> {
        let mut client = active_client()?;
        client.archive()?;
        assert_eq!(
            client.rename("should not apply"),
            Err(ClientError::InvalidStatusTransition)
        );
        assert_eq!(client.display_name(), "Synthetic Client");
        Ok(())
    }

    #[test]
    fn merged_transition_is_one_way() -> Result<(), Box<dyn std::error::Error>> {
        let mut client = active_client()?;
        client.mark_merged()?;
        assert_eq!(client.status(), ClientStatus::Merged);
        assert_eq!(client.version().value(), 2);
        assert_eq!(
            client.rename("resurrected"),
            Err(ClientError::InvalidStatusTransition)
        );
        assert_eq!(client.archive(), Err(ClientError::InvalidStatusTransition));
        assert_eq!(client.status(), ClientStatus::Merged);
        assert_eq!(client.version().value(), 2);
        Ok(())
    }
}
