#![forbid(unsafe_code)]

use core::fmt;
use profile_platform_primitives::{
    ActorId, AggregateVersion, ClientId, ProfileId, TenantId, UnixMillis,
};

const MAX_DISPLAY_NAME_LENGTH: usize = 200;
const MAX_REASON_LENGTH: usize = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientKind {
    Person,
    Organization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientStatus {
    Active,
    Archived,
    Merged,
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
        let display_name = display_name.into();
        let display_name = display_name.trim();
        if display_name.is_empty() || display_name.len() > MAX_DISPLAY_NAME_LENGTH {
            return Err(ClientError::InvalidDisplayName);
        }

        Ok(Self {
            tenant_id,
            client_id,
            version: AggregateVersion::INITIAL,
            kind,
            display_name: display_name.to_owned(),
            status: ClientStatus::Active,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignmentStatus {
    Active,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileClientAssignment {
    tenant_id: TenantId,
    profile_id: ProfileId,
    client_id: ClientId,
    status: AssignmentStatus,
    assigned_by: ActorId,
    assigned_at: UnixMillis,
    closed_at: Option<UnixMillis>,
    reason: String,
}

impl ProfileClientAssignment {
    pub fn assign(
        profile_tenant_id: &TenantId,
        profile_id: ProfileId,
        client: &ClientRecord,
        assigned_by: ActorId,
        assigned_at: UnixMillis,
        reason: impl Into<String>,
    ) -> Result<Self, AssignmentError> {
        if profile_tenant_id != client.tenant_id() {
            return Err(AssignmentError::TenantMismatch);
        }
        if client.status() != ClientStatus::Active {
            return Err(AssignmentError::ClientNotActive);
        }

        let reason = reason.into();
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > MAX_REASON_LENGTH {
            return Err(AssignmentError::InvalidReason);
        }

        Ok(Self {
            tenant_id: profile_tenant_id.clone(),
            profile_id,
            client_id: client.client_id().clone(),
            status: AssignmentStatus::Active,
            assigned_by,
            assigned_at,
            closed_at: None,
            reason: reason.to_owned(),
        })
    }

    pub fn close(&mut self, closed_at: UnixMillis) -> Result<(), AssignmentError> {
        if self.status != AssignmentStatus::Active {
            return Err(AssignmentError::AlreadyClosed);
        }
        if closed_at < self.assigned_at {
            return Err(AssignmentError::InvalidCloseTime);
        }
        self.status = AssignmentStatus::Closed;
        self.closed_at = Some(closed_at);
        Ok(())
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn status(&self) -> AssignmentStatus {
        self.status
    }

    #[must_use]
    pub const fn assigned_by(&self) -> &ActorId {
        &self.assigned_by
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignmentError {
    TenantMismatch,
    ClientNotActive,
    InvalidReason,
    AlreadyClosed,
    InvalidCloseTime,
}

impl fmt::Display for AssignmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TenantMismatch => "profile and client tenant differ",
            Self::ClientNotActive => "client is not active",
            Self::InvalidReason => "assignment reason is invalid",
            Self::AlreadyClosed => "assignment is already closed",
            Self::InvalidCloseTime => "assignment close time precedes assignment time",
        })
    }
}

impl std::error::Error for AssignmentError {}

#[cfg(test)]
mod tests {
    use super::{
        AssignmentError, ClientError, ClientKind, ClientRecord, ClientStatus,
        ProfileClientAssignment,
    };
    use profile_platform_primitives::{
        ActorId, AggregateVersion, ClientId, ProfileId, TenantId, UnixMillis,
    };

    fn active_client() -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::create(
            TenantId::parse("tenant_01JCLIENT")?,
            ClientId::parse("client_01JCLIENT")?,
            ClientKind::Person,
            "Synthetic Client",
        )?)
    }

    #[test]
    fn creation_normalizes_bounded_display_name()
    -> Result<(), Box<dyn std::error::Error>> {
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
    fn archive_overflow_does_not_partially_mutate_client()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut client = active_client()?;
        client.version = AggregateVersion::new(u64::MAX)?;
        assert_eq!(client.archive(), Err(ClientError::VersionOverflow));
        assert_eq!(client.status(), ClientStatus::Active);
        assert_eq!(client.version().value(), u64::MAX);
        Ok(())
    }

    #[test]
    fn assignment_requires_same_tenant() -> Result<(), Box<dyn std::error::Error>> {
        let client = active_client()?;
        let result = ProfileClientAssignment::assign(
            &TenantId::parse("tenant_02JCLIENT")?,
            ProfileId::parse("profile_01JCLIENT")?,
            &client,
            ActorId::parse("actor_01JCLIENT")?,
            UnixMillis::new(10),
            "initial assignment",
        );
        assert_eq!(result, Err(AssignmentError::TenantMismatch));
        Ok(())
    }

    #[test]
    fn archived_client_cannot_receive_assignment() -> Result<(), Box<dyn std::error::Error>> {
        let mut client = active_client()?;
        client.archive()?;
        assert_eq!(client.status(), ClientStatus::Archived);

        let result = ProfileClientAssignment::assign(
            client.tenant_id(),
            ProfileId::parse("profile_01JCLIENT")?,
            &client,
            ActorId::parse("actor_01JCLIENT")?,
            UnixMillis::new(10),
            "reassignment",
        );
        assert_eq!(result, Err(AssignmentError::ClientNotActive));
        Ok(())
    }

    #[test]
    fn closing_assignment_preserves_normalized_history()
    -> Result<(), Box<dyn std::error::Error>> {
        let client = active_client()?;
        let mut assignment = ProfileClientAssignment::assign(
            client.tenant_id(),
            ProfileId::parse("profile_01JCLIENT")?,
            &client,
            ActorId::parse("actor_01JCLIENT")?,
            UnixMillis::new(10),
            "  initial assignment  ",
        )?;
        assignment.close(UnixMillis::new(20))?;
        assert_eq!(assignment.reason(), "initial assignment");
        Ok(())
    }
}
