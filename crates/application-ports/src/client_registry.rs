use client_domain::{AssignmentStatus, ClientKind, ClientStatus, ContactKind, ContactStatus};
use core::fmt;
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorId, AggregateVersion, AssignmentId, AuditEventId, ClientId, ContactPointId, ProfileId,
    TenantScope, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientRegistryListItem {
    client_id: ClientId,
    kind: ClientKind,
    display_name: String,
    status: ClientStatus,
    version: AggregateVersion,
}

impl ClientRegistryListItem {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        kind: ClientKind,
        display_name: impl Into<String>,
        status: ClientStatus,
        version: AggregateVersion,
    ) -> Self {
        Self {
            client_id,
            kind,
            display_name: display_name.into(),
            status,
            version,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
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

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientRegistryContactProjection {
    contact_point_id: ContactPointId,
    kind: ContactKind,
    status: ContactStatus,
}

impl ClientRegistryContactProjection {
    #[must_use]
    pub const fn new(
        contact_point_id: ContactPointId,
        kind: ContactKind,
        status: ContactStatus,
    ) -> Self {
        Self {
            contact_point_id,
            kind,
            status,
        }
    }

    #[must_use]
    pub const fn contact_point_id(&self) -> &ContactPointId {
        &self.contact_point_id
    }

    #[must_use]
    pub const fn kind(&self) -> ContactKind {
        self.kind
    }

    #[must_use]
    pub const fn status(&self) -> ContactStatus {
        self.status
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientRegistryAssignmentProjection {
    assignment_id: AssignmentId,
    profile_id: ProfileId,
    status: AssignmentStatus,
    assigned_at: UnixMillis,
    closed_at: Option<UnixMillis>,
    reason: String,
}

impl ClientRegistryAssignmentProjection {
    #[must_use]
    pub fn new(
        assignment_id: AssignmentId,
        profile_id: ProfileId,
        status: AssignmentStatus,
        assigned_at: UnixMillis,
        closed_at: Option<UnixMillis>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            assignment_id,
            profile_id,
            status,
            assigned_at,
            closed_at,
            reason: reason.into(),
        }
    }

    #[must_use]
    pub const fn assignment_id(&self) -> &AssignmentId {
        &self.assignment_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn status(&self) -> AssignmentStatus {
        self.status
    }

    #[must_use]
    pub const fn assigned_at(&self) -> UnixMillis {
        self.assigned_at
    }

    #[must_use]
    pub const fn closed_at(&self) -> Option<UnixMillis> {
        self.closed_at
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientRegistryActivityProjection {
    audit_event_id: AuditEventId,
    action: String,
    resource_type: String,
    resource_id: String,
    result_code: String,
    occurred_at: UnixMillis,
}

impl ClientRegistryActivityProjection {
    #[must_use]
    pub fn new(
        audit_event_id: AuditEventId,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        result_code: impl Into<String>,
        occurred_at: UnixMillis,
    ) -> Self {
        Self {
            audit_event_id,
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            result_code: result_code.into(),
            occurred_at,
        }
    }

    #[must_use]
    pub const fn audit_event_id(&self) -> &AuditEventId {
        &self.audit_event_id
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub const fn occurred_at(&self) -> UnixMillis {
        self.occurred_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientRegistryHistoryProjection {
    contacts: Vec<ClientRegistryContactProjection>,
    assignments: Vec<ClientRegistryAssignmentProjection>,
    activity: Vec<ClientRegistryActivityProjection>,
}

impl ClientRegistryHistoryProjection {
    #[must_use]
    pub const fn new(
        contacts: Vec<ClientRegistryContactProjection>,
        assignments: Vec<ClientRegistryAssignmentProjection>,
        activity: Vec<ClientRegistryActivityProjection>,
    ) -> Self {
        Self {
            contacts,
            assignments,
            activity,
        }
    }

    #[must_use]
    pub fn contacts(&self) -> &[ClientRegistryContactProjection] {
        &self.contacts
    }

    #[must_use]
    pub fn assignments(&self) -> &[ClientRegistryAssignmentProjection] {
        &self.assignments
    }

    #[must_use]
    pub fn activity(&self) -> &[ClientRegistryActivityProjection] {
        &self.activity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientRegistryProjectionErrorClass {
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientRegistryProjectionError {
    class: ClientRegistryProjectionErrorClass,
}

impl ClientRegistryProjectionError {
    #[must_use]
    pub const fn new(class: ClientRegistryProjectionErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> ClientRegistryProjectionErrorClass {
        self.class
    }
}

impl fmt::Display for ClientRegistryProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            ClientRegistryProjectionErrorClass::IntegrityFailure => {
                "client registry projection integrity failure"
            }
            ClientRegistryProjectionErrorClass::DependencyUnavailable => {
                "client registry projection dependency unavailable"
            }
        })
    }
}

impl std::error::Error for ClientRegistryProjectionError {}

#[allow(async_fn_in_trait)]
pub trait ClientRegistryProjectionPort {
    async fn list_visible_clients(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
    ) -> Result<Vec<ClientRegistryListItem>, ClientRegistryProjectionError>;

    async fn load_visible_client_history(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        client_id: &ClientId,
    ) -> Result<Option<ClientRegistryHistoryProjection>, ClientRegistryProjectionError>;
}
