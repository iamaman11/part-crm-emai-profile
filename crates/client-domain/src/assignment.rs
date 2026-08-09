use crate::{ClientRecord, ClientStatus};
use core::fmt;
use profile_platform_primitives::{
    ActorId, AssignmentId, ClientId, ProfileId, TenantId, UnixMillis,
};

const MAX_REASON_LENGTH: usize = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignmentRole {
    Primary,
}

impl AssignmentRole {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Primary => "PRIMARY",
        }
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
    assignment_id: AssignmentId,
    profile_id: ProfileId,
    client_id: ClientId,
    role: AssignmentRole,
    status: AssignmentStatus,
    assigned_by: ActorId,
    assigned_at: UnixMillis,
    closed_at: Option<UnixMillis>,
    reason: String,
}

impl ProfileClientAssignment {
    pub fn assign(
        profile_tenant_id: &TenantId,
        assignment_id: AssignmentId,
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
            assignment_id,
            profile_id,
            client_id: client.client_id().clone(),
            role: AssignmentRole::Primary,
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
    pub const fn assignment_id(&self) -> &AssignmentId {
        &self.assignment_id
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
    pub const fn role(&self) -> AssignmentRole {
        self.role
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
pub struct PrimaryAssignmentTransition {
    closed_previous: Option<ProfileClientAssignment>,
    next: ProfileClientAssignment,
}

impl PrimaryAssignmentTransition {
    #[must_use]
    pub const fn closed_previous(&self) -> Option<&ProfileClientAssignment> {
        self.closed_previous.as_ref()
    }

    #[must_use]
    pub const fn next(&self) -> &ProfileClientAssignment {
        &self.next
    }
}

pub fn plan_primary_reassignment(
    profile_tenant_id: &TenantId,
    profile_id: &ProfileId,
    current: Option<&ProfileClientAssignment>,
    next_assignment_id: AssignmentId,
    next_client: &ClientRecord,
    assigned_by: ActorId,
    assigned_at: UnixMillis,
    reason: impl Into<String>,
) -> Result<PrimaryAssignmentTransition, AssignmentError> {
    let next = ProfileClientAssignment::assign(
        profile_tenant_id,
        next_assignment_id,
        profile_id.clone(),
        next_client,
        assigned_by,
        assigned_at,
        reason,
    )?;

    let closed_previous = match current {
        None => None,
        Some(previous) => {
            if previous.tenant_id() != profile_tenant_id || previous.profile_id() != profile_id {
                return Err(AssignmentError::CurrentScopeMismatch);
            }
            if previous.role() != AssignmentRole::Primary
                || previous.status() != AssignmentStatus::Active
            {
                return Err(AssignmentError::CurrentNotActivePrimary);
            }
            if previous.client_id() == next.client_id() {
                return Err(AssignmentError::AlreadyPrimaryClient);
            }

            let mut closed = previous.clone();
            closed.close(assigned_at)?;
            Some(closed)
        }
    };

    Ok(PrimaryAssignmentTransition {
        closed_previous,
        next,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignmentError {
    TenantMismatch,
    ClientNotActive,
    InvalidReason,
    AlreadyClosed,
    InvalidCloseTime,
    CurrentScopeMismatch,
    CurrentNotActivePrimary,
    AlreadyPrimaryClient,
}

impl fmt::Display for AssignmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TenantMismatch => "profile and client tenant differ",
            Self::ClientNotActive => "client is not active",
            Self::InvalidReason => "assignment reason is invalid",
            Self::AlreadyClosed => "assignment is already closed",
            Self::InvalidCloseTime => "assignment close time precedes assignment time",
            Self::CurrentScopeMismatch => "current assignment belongs to another profile scope",
            Self::CurrentNotActivePrimary => {
                "current assignment is not an active primary assignment"
            }
            Self::AlreadyPrimaryClient => {
                "profile is already assigned to the requested primary client"
            }
        })
    }
}

impl std::error::Error for AssignmentError {}

#[cfg(test)]
mod tests {
    use super::{
        AssignmentError, AssignmentRole, AssignmentStatus, ProfileClientAssignment,
        plan_primary_reassignment,
    };
    use crate::{ClientKind, ClientRecord, ClientStatus};
    use profile_platform_primitives::{
        ActorId, AssignmentId, ClientId, ProfileId, TenantId, UnixMillis,
    };

    fn active_client(client_id: &str) -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::create(
            TenantId::parse("tenant_01JCLIENT")?,
            ClientId::parse(client_id)?,
            ClientKind::Person,
            client_id,
        )?)
    }

    fn initial_assignment(
        client: &ClientRecord,
    ) -> Result<ProfileClientAssignment, Box<dyn std::error::Error>> {
        Ok(ProfileClientAssignment::assign(
            client.tenant_id(),
            AssignmentId::parse("assignment_01JCLIENT")?,
            ProfileId::parse("profile_01JCLIENT")?,
            client,
            ActorId::parse("actor_01JCLIENT")?,
            UnixMillis::new(10),
            "initial assignment",
        )?)
    }

    #[test]
    fn assignment_requires_same_tenant() -> Result<(), Box<dyn std::error::Error>> {
        let client = active_client("client_01JCLIENT")?;
        let result = ProfileClientAssignment::assign(
            &TenantId::parse("tenant_02JCLIENT")?,
            AssignmentId::parse("assignment_01JCLIENT")?,
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
        let mut client = active_client("client_01JCLIENT")?;
        client.archive()?;
        assert_eq!(client.status(), ClientStatus::Archived);
        let result = ProfileClientAssignment::assign(
            client.tenant_id(),
            AssignmentId::parse("assignment_01JCLIENT")?,
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
    fn closing_assignment_preserves_normalized_history() -> Result<(), Box<dyn std::error::Error>> {
        let client = active_client("client_01JCLIENT")?;
        let mut assignment = ProfileClientAssignment::assign(
            client.tenant_id(),
            AssignmentId::parse("assignment_01JCLIENT")?,
            ProfileId::parse("profile_01JCLIENT")?,
            &client,
            ActorId::parse("actor_01JCLIENT")?,
            UnixMillis::new(10),
            "  initial assignment  ",
        )?;
        assignment.close(UnixMillis::new(20))?;
        assert_eq!(assignment.assignment_id().as_str(), "assignment_01JCLIENT");
        assert_eq!(assignment.role(), AssignmentRole::Primary);
        assert_eq!(assignment.status(), AssignmentStatus::Closed);
        assert_eq!(assignment.closed_at(), Some(UnixMillis::new(20)));
        assert_eq!(assignment.reason(), "initial assignment");
        Ok(())
    }

    #[test]
    fn reassignment_plan_closes_previous_before_exposing_next_active_assignment()
    -> Result<(), Box<dyn std::error::Error>> {
        let old_client = active_client("client_01JCLIENT")?;
        let new_client = active_client("client_02JCLIENT")?;
        let current = initial_assignment(&old_client)?;
        let profile_id = current.profile_id().clone();

        let transition = plan_primary_reassignment(
            current.tenant_id(),
            &profile_id,
            Some(&current),
            AssignmentId::parse("assignment_02JCLIENT")?,
            &new_client,
            ActorId::parse("actor_02JCLIENT")?,
            UnixMillis::new(20),
            "reassigned by operator",
        )?;

        let closed = transition
            .closed_previous()
            .ok_or("missing closed history")?;
        assert_eq!(closed.status(), AssignmentStatus::Closed);
        assert_eq!(closed.closed_at(), Some(UnixMillis::new(20)));
        assert_eq!(closed.client_id(), old_client.client_id());
        assert_eq!(transition.next().status(), AssignmentStatus::Active);
        assert_eq!(transition.next().role(), AssignmentRole::Primary);
        assert_eq!(transition.next().assignment_id().as_str(), "assignment_02JCLIENT");
        assert_eq!(transition.next().client_id(), new_client.client_id());
        assert_eq!(transition.next().profile_id(), &profile_id);
        assert_eq!(current.status(), AssignmentStatus::Active);
        assert_eq!(current.closed_at(), None);
        Ok(())
    }

    #[test]
    fn reassignment_rejects_same_client_or_wrong_current_scope_without_mutating_history()
    -> Result<(), Box<dyn std::error::Error>> {
        let old_client = active_client("client_01JCLIENT")?;
        let new_client = active_client("client_02JCLIENT")?;
        let current = initial_assignment(&old_client)?;
        let profile_id = current.profile_id().clone();

        assert_eq!(
            plan_primary_reassignment(
                current.tenant_id(),
                &profile_id,
                Some(&current),
                AssignmentId::parse("assignment_02JCLIENT")?,
                &old_client,
                ActorId::parse("actor_02JCLIENT")?,
                UnixMillis::new(20),
                "same client",
            ),
            Err(AssignmentError::AlreadyPrimaryClient)
        );
        assert_eq!(current.status(), AssignmentStatus::Active);
        assert_eq!(current.closed_at(), None);

        assert_eq!(
            plan_primary_reassignment(
                current.tenant_id(),
                &ProfileId::parse("profile_02JCLIENT")?,
                Some(&current),
                AssignmentId::parse("assignment_03JCLIENT")?,
                &new_client,
                ActorId::parse("actor_02JCLIENT")?,
                UnixMillis::new(20),
                "wrong scope",
            ),
            Err(AssignmentError::CurrentScopeMismatch)
        );
        assert_eq!(current.status(), AssignmentStatus::Active);
        Ok(())
    }

    #[test]
    fn closed_current_assignment_cannot_be_reused_as_active_primary()
    -> Result<(), Box<dyn std::error::Error>> {
        let old_client = active_client("client_01JCLIENT")?;
        let new_client = active_client("client_02JCLIENT")?;
        let mut current = initial_assignment(&old_client)?;
        current.close(UnixMillis::new(15))?;
        let profile_id = current.profile_id().clone();
        assert_eq!(
            plan_primary_reassignment(
                current.tenant_id(),
                &profile_id,
                Some(&current),
                AssignmentId::parse("assignment_02JCLIENT")?,
                &new_client,
                ActorId::parse("actor_02JCLIENT")?,
                UnixMillis::new(20),
                "reassign from closed",
            ),
            Err(AssignmentError::CurrentNotActivePrimary)
        );
        Ok(())
    }

    #[test]
    fn one_client_can_be_primary_for_multiple_profiles() -> Result<(), Box<dyn std::error::Error>> {
        let client = active_client("client_01JCLIENT")?;
        let first = ProfileClientAssignment::assign(
            client.tenant_id(),
            AssignmentId::parse("assignment_01JCLIENT")?,
            ProfileId::parse("profile_01JCLIENT")?,
            &client,
            ActorId::parse("actor_01JCLIENT")?,
            UnixMillis::new(10),
            "first profile",
        )?;
        let second = ProfileClientAssignment::assign(
            client.tenant_id(),
            AssignmentId::parse("assignment_02JCLIENT")?,
            ProfileId::parse("profile_02JCLIENT")?,
            &client,
            ActorId::parse("actor_01JCLIENT")?,
            UnixMillis::new(11),
            "second profile",
        )?;
        assert_eq!(first.client_id(), second.client_id());
        assert_ne!(first.profile_id(), second.profile_id());
        assert_eq!(first.status(), AssignmentStatus::Active);
        assert_eq!(second.status(), AssignmentStatus::Active);
        Ok(())
    }
}
