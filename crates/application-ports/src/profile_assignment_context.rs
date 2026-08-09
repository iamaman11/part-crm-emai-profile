use crate::profiles::ProfileAssignmentPortError;
use client_domain::ClientRecord;
use profile_platform_primitives::{
    ActorId, AggregateVersion, AssignmentId, ClientId, ProfileId, TenantScope, UnixMillis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentProfileAssignmentSnapshot {
    assignment_id: AssignmentId,
    client: ClientRecord,
    assigned_by: ActorId,
    assigned_at: UnixMillis,
    reason: String,
}

impl CurrentProfileAssignmentSnapshot {
    #[must_use]
    pub fn new(
        assignment_id: AssignmentId,
        client: ClientRecord,
        assigned_by: ActorId,
        assigned_at: UnixMillis,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            assignment_id,
            client,
            assigned_by,
            assigned_at,
            reason: reason.into(),
        }
    }

    #[must_use]
    pub const fn assignment_id(&self) -> &AssignmentId {
        &self.assignment_id
    }

    #[must_use]
    pub const fn client(&self) -> &ClientRecord {
        &self.client
    }

    #[must_use]
    pub const fn assigned_by(&self) -> &ActorId {
        &self.assigned_by
    }

    #[must_use]
    pub const fn assigned_at(&self) -> UnixMillis {
        self.assigned_at
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileAssignmentContext {
    profile_version: AggregateVersion,
    target_client: ClientRecord,
    current: Option<CurrentProfileAssignmentSnapshot>,
}

impl ProfileAssignmentContext {
    #[must_use]
    pub const fn new(
        profile_version: AggregateVersion,
        target_client: ClientRecord,
        current: Option<CurrentProfileAssignmentSnapshot>,
    ) -> Self {
        Self {
            profile_version,
            target_client,
            current,
        }
    }

    #[must_use]
    pub const fn profile_version(&self) -> AggregateVersion {
        self.profile_version
    }

    #[must_use]
    pub const fn target_client(&self) -> &ClientRecord {
        &self.target_client
    }

    #[must_use]
    pub const fn current(&self) -> Option<&CurrentProfileAssignmentSnapshot> {
        self.current.as_ref()
    }
}

#[allow(async_fn_in_trait)]
pub trait ProfileAssignmentContextPort {
    async fn load_profile_assignment_context(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        target_client_id: &ClientId,
    ) -> Result<Option<ProfileAssignmentContext>, ProfileAssignmentPortError>;
}
