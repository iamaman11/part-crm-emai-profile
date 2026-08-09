use crate::d1_profiles::D1ProfileApplicationRepository;
use application_ports::CommandExecutionEvidence;
use application_ports::profile_assignment_context::{
    CurrentProfileAssignmentSnapshot, ProfileAssignmentContext, ProfileAssignmentContextPort,
};
use application_ports::profiles::{
    ProfileApplicationPort, ProfileAssignmentApplicationPort, ProfileAssignmentPortError,
    ProfileAssignmentPortErrorClass, ProfileAssignmentWrite, ProfileCreateWrite,
    ProfileGrantApplicationPort, ProfileGrantPortError, ProfileGrantWrite, ProfilePortError,
    ProfileReadModel, ProfileReplayDecision,
};
use client_domain::{ClientKind, ClientRecord, ClientStatus};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, AssignmentId, ClientId, ProfileId, TenantId,
    TenantScope, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

pub struct D1ProfileApplicationBundle {
    profiles: D1ProfileApplicationRepository,
    assignment_context_database: D1Database,
}

impl D1ProfileApplicationBundle {
    #[must_use]
    pub const fn new(
        governed_database: D1Database,
        idempotency_database: D1Database,
        query_database: D1Database,
        assignment_context_database: D1Database,
    ) -> Self {
        Self {
            profiles: D1ProfileApplicationRepository::new(
                governed_database,
                idempotency_database,
                query_database,
            ),
            assignment_context_database,
        }
    }
}

impl ProfileApplicationPort for D1ProfileApplicationBundle {
    async fn decide_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ProfileReplayDecision, ProfilePortError> {
        self.profiles
            .decide_replay(actor, command_name, evidence)
            .await
    }

    async fn create_profile(
        &self,
        actor: &ActorContext,
        write: &ProfileCreateWrite,
    ) -> Result<(), ProfilePortError> {
        self.profiles.create_profile(actor, write).await
    }

    async fn find_visible_profile(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileReadModel>, ProfilePortError> {
        self.profiles
            .find_visible_profile(scope, actor_id, role, profile_id)
            .await
    }
}

impl ProfileAssignmentApplicationPort for D1ProfileApplicationBundle {
    async fn decide_assignment_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ProfileReplayDecision, ProfileAssignmentPortError> {
        self.profiles
            .decide_assignment_replay(actor, command_name, evidence)
            .await
    }

    async fn assign_profile(
        &self,
        actor: &ActorContext,
        write: &ProfileAssignmentWrite,
    ) -> Result<(), ProfileAssignmentPortError> {
        self.profiles.assign_profile(actor, write).await
    }
}

impl ProfileGrantApplicationPort for D1ProfileApplicationBundle {
    async fn decide_profile_grant_replay(
        &self,
        actor: &ActorContext,
        command_name: &str,
        evidence: &CommandExecutionEvidence,
    ) -> Result<ProfileReplayDecision, ProfileGrantPortError> {
        self.profiles
            .decide_profile_grant_replay(actor, command_name, evidence)
            .await
    }

    async fn grant_profile(
        &self,
        actor: &ActorContext,
        write: &ProfileGrantWrite,
    ) -> Result<(), ProfileGrantPortError> {
        self.profiles.grant_profile(actor, write).await
    }

    async fn revoke_profile_grant(
        &self,
        actor: &ActorContext,
        write: &ProfileGrantWrite,
    ) -> Result<(), ProfileGrantPortError> {
        self.profiles.revoke_profile_grant(actor, write).await
    }
}

impl ProfileAssignmentContextPort for D1ProfileApplicationBundle {
    async fn load_profile_assignment_context(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        target_client_id: &ClientId,
    ) -> Result<Option<ProfileAssignmentContext>, ProfileAssignmentPortError> {
        let row = query!(
            &self.assignment_context_database,
            r#"
            SELECT
                profile.version AS profile_version,
                target.client_id AS target_client_id,
                target.kind AS target_client_kind,
                target.display_name AS target_client_display_name,
                target.status AS target_client_status,
                target.version AS target_client_version,
                assignment.assignment_id AS current_assignment_id,
                current_client.client_id AS current_client_id,
                current_client.kind AS current_client_kind,
                current_client.display_name AS current_client_display_name,
                current_client.status AS current_client_status,
                current_client.version AS current_client_version,
                assignment.assigned_by_actor_id AS current_assigned_by_actor_id,
                assignment.assigned_at_ms AS current_assigned_at_ms,
                assignment.reason AS current_reason
            FROM browser_profiles AS profile
            JOIN clients AS target
              ON target.tenant_id = profile.tenant_id
             AND target.client_id = ?
            LEFT JOIN profile_client_assignments AS assignment
              ON assignment.tenant_id = profile.tenant_id
             AND assignment.profile_id = profile.profile_id
             AND assignment.closed_at_ms IS NULL
            LEFT JOIN clients AS current_client
              ON current_client.tenant_id = assignment.tenant_id
             AND current_client.client_id = assignment.client_id
            WHERE profile.tenant_id = ?
              AND profile.profile_id = ?
            "#,
            target_client_id.as_str(),
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )
        .map_err(|_| dependency_error())?
        .first::<ProfileAssignmentContextRow>(None)
        .await
        .map_err(|_| dependency_error())?;

        row.map(|row| map_context(scope.tenant_id(), row))
            .transpose()
    }
}

#[derive(Deserialize)]
struct ProfileAssignmentContextRow {
    profile_version: i64,
    target_client_id: String,
    target_client_kind: String,
    target_client_display_name: String,
    target_client_status: String,
    target_client_version: i64,
    current_assignment_id: Option<String>,
    current_client_id: Option<String>,
    current_client_kind: Option<String>,
    current_client_display_name: Option<String>,
    current_client_status: Option<String>,
    current_client_version: Option<i64>,
    current_assigned_by_actor_id: Option<String>,
    current_assigned_at_ms: Option<i64>,
    current_reason: Option<String>,
}

fn map_context(
    tenant_id: &TenantId,
    row: ProfileAssignmentContextRow,
) -> Result<ProfileAssignmentContext, ProfileAssignmentPortError> {
    let profile_version = aggregate_version(row.profile_version)?;
    let target_client = client_record(
        tenant_id,
        row.target_client_id,
        row.target_client_kind,
        row.target_client_display_name,
        row.target_client_status,
        row.target_client_version,
    )?;

    let current = match row.current_assignment_id {
        None => {
            if row.current_client_id.is_some()
                || row.current_client_kind.is_some()
                || row.current_client_display_name.is_some()
                || row.current_client_status.is_some()
                || row.current_client_version.is_some()
                || row.current_assigned_by_actor_id.is_some()
                || row.current_assigned_at_ms.is_some()
                || row.current_reason.is_some()
            {
                return Err(integrity_error());
            }
            None
        }
        Some(assignment_id) => {
            let current_client = client_record(
                tenant_id,
                required(row.current_client_id)?,
                required(row.current_client_kind)?,
                required(row.current_client_display_name)?,
                required(row.current_client_status)?,
                required(row.current_client_version)?,
            )?;
            let assigned_by = ActorId::parse(required(row.current_assigned_by_actor_id)?)
                .map_err(|_| integrity_error())?;
            let assigned_at = unix_millis(required(row.current_assigned_at_ms)?)?;
            Some(CurrentProfileAssignmentSnapshot::new(
                AssignmentId::parse(assignment_id).map_err(|_| integrity_error())?,
                current_client,
                assigned_by,
                assigned_at,
                required(row.current_reason)?,
            ))
        }
    };

    Ok(ProfileAssignmentContext::new(
        profile_version,
        target_client,
        current,
    ))
}

fn client_record(
    tenant_id: &TenantId,
    client_id: String,
    kind: String,
    display_name: String,
    status: String,
    version: i64,
) -> Result<ClientRecord, ProfileAssignmentPortError> {
    ClientRecord::restore(
        tenant_id.clone(),
        ClientId::parse(client_id).map_err(|_| integrity_error())?,
        aggregate_version(version)?,
        match kind.as_str() {
            "PERSON" => ClientKind::Person,
            "ORGANIZATION" => ClientKind::Organization,
            _ => return Err(integrity_error()),
        },
        display_name,
        match status.as_str() {
            "ACTIVE" => ClientStatus::Active,
            "ARCHIVED" => ClientStatus::Archived,
            "MERGED" => ClientStatus::Merged,
            _ => return Err(integrity_error()),
        },
    )
    .map_err(|_| integrity_error())
}

fn aggregate_version(value: i64) -> Result<AggregateVersion, ProfileAssignmentPortError> {
    let value = u64::try_from(value).map_err(|_| integrity_error())?;
    AggregateVersion::new(value).map_err(|_| integrity_error())
}

fn unix_millis(value: i64) -> Result<UnixMillis, ProfileAssignmentPortError> {
    let value = u64::try_from(value).map_err(|_| integrity_error())?;
    Ok(UnixMillis::new(value))
}

fn required<T>(value: Option<T>) -> Result<T, ProfileAssignmentPortError> {
    value.ok_or_else(integrity_error)
}

const fn dependency_error() -> ProfileAssignmentPortError {
    ProfileAssignmentPortError::new(ProfileAssignmentPortErrorClass::DependencyUnavailable)
}

const fn integrity_error() -> ProfileAssignmentPortError {
    ProfileAssignmentPortError::new(ProfileAssignmentPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::{aggregate_version, unix_millis};
    use application_ports::profiles::ProfileAssignmentPortErrorClass;

    #[test]
    fn persisted_versions_and_times_reject_invalid_sqlite_values() {
        assert_eq!(
            aggregate_version(0)
                .expect_err("zero version must fail")
                .class(),
            ProfileAssignmentPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            aggregate_version(-1)
                .expect_err("negative version must fail")
                .class(),
            ProfileAssignmentPortErrorClass::IntegrityFailure
        );
        assert_eq!(
            unix_millis(-1)
                .expect_err("negative timestamp must fail")
                .class(),
            ProfileAssignmentPortErrorClass::IntegrityFailure
        );
    }
}
