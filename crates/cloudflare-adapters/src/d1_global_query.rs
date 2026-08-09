use application_ports::query::{QueryPortError, QueryPortErrorClass};
use application_ports::query_clients::ClientReadProjection;
use application_ports::query_global::{
    GlobalSearchKey, GlobalSearchProjection, GlobalSearchReadModelPort,
};
use application_ports::query_mailboxes::MailboxReadProjection;
use application_ports::query_members::MemberReadProjection;
use application_ports::query_profiles::ProfileReadProjection;
use client_domain::{ClientKind, ClientStatus};
use identity_access_domain::{MembershipRole, MembershipStatus};
use mailbox_domain::{MailboxBindingStatus, MailboxProvider};
use profile_domain::ProfileStatus;
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, ClientId, GenerationId, MailboxBindingId, ProfileId,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

pub struct D1GlobalSearchRepository {
    database: D1Database,
}

impl D1GlobalSearchRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn search_client(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
    ) -> Result<Option<ClientReadProjection>, QueryPortError> {
        let row = query!(
            &self.database,
            r#"
            SELECT client.client_id, client.kind, client.display_name, client.status, client.version
            FROM clients AS client
            WHERE client.tenant_id = ?
              AND client.client_id = ?
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS membership
                  WHERE membership.tenant_id = client.tenant_id
                    AND membership.actor_id = ?
                    AND membership.status = 'ACTIVE'
                    AND (
                        membership.role = 'TENANT_OWNER'
                        OR (
                            membership.role = 'MEMBER'
                            AND EXISTS (
                                SELECT 1
                                FROM client_grants AS grant_row
                                WHERE grant_row.tenant_id = client.tenant_id
                                  AND grant_row.actor_id = membership.actor_id
                                  AND grant_row.client_id = client.client_id
                            )
                        )
                    )
              )
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            client_id.as_str(),
            actor.actor_id().as_str(),
        )
        .map_err(dependency_error)?
        .first::<ClientRow>(None)
        .await
        .map_err(dependency_error)?;
        row.map(map_client_row).transpose()
    }

    async fn search_profile(
        &self,
        actor: &ActorContext,
        profile_id: &ProfileId,
    ) -> Result<Option<ProfileReadProjection>, QueryPortError> {
        let row = query!(
            &self.database,
            r#"
            SELECT profile.profile_id, profile.status, profile.version,
                   assignment.client_id AS linked_client_id,
                   profile.active_generation_id
            FROM browser_profiles AS profile
            LEFT JOIN profile_client_assignments AS assignment
              ON assignment.tenant_id = profile.tenant_id
             AND assignment.profile_id = profile.profile_id
             AND assignment.closed_at_ms IS NULL
            WHERE profile.tenant_id = ?
              AND profile.profile_id = ?
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS membership
                  WHERE membership.tenant_id = profile.tenant_id
                    AND membership.actor_id = ?
                    AND membership.status = 'ACTIVE'
                    AND (
                        membership.role = 'TENANT_OWNER'
                        OR (
                            membership.role = 'MEMBER'
                            AND EXISTS (
                                SELECT 1
                                FROM profile_grants AS grant_row
                                WHERE grant_row.tenant_id = profile.tenant_id
                                  AND grant_row.actor_id = membership.actor_id
                                  AND grant_row.profile_id = profile.profile_id
                            )
                        )
                    )
              )
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            profile_id.as_str(),
            actor.actor_id().as_str(),
        )
        .map_err(dependency_error)?
        .first::<ProfileRow>(None)
        .await
        .map_err(dependency_error)?;
        row.map(map_profile_row).transpose()
    }

    async fn search_member(
        &self,
        actor: &ActorContext,
        member_id: &ActorId,
    ) -> Result<Option<MemberReadProjection>, QueryPortError> {
        let row = query!(
            &self.database,
            r#"
            SELECT member.actor_id, member.role, member.status
            FROM memberships AS member
            WHERE member.tenant_id = ?
              AND member.actor_id = ?
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS requester
                  WHERE requester.tenant_id = member.tenant_id
                    AND requester.actor_id = ?
                    AND requester.status = 'ACTIVE'
                    AND (
                        requester.role = 'TENANT_OWNER'
                        OR requester.actor_id = member.actor_id
                    )
              )
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            member_id.as_str(),
            actor.actor_id().as_str(),
        )
        .map_err(dependency_error)?
        .first::<MemberRow>(None)
        .await
        .map_err(dependency_error)?;
        row.map(map_member_row).transpose()
    }

    async fn search_mailbox(
        &self,
        actor: &ActorContext,
        binding_id: &MailboxBindingId,
    ) -> Result<Option<MailboxReadProjection>, QueryPortError> {
        let row = query!(
            &self.database,
            r#"
            SELECT binding.binding_id, binding.provider, binding.status, binding.version
            FROM mailbox_bindings AS binding
            WHERE binding.tenant_id = ?
              AND binding.binding_id = ?
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS requester
                  WHERE requester.tenant_id = binding.tenant_id
                    AND requester.actor_id = ?
                    AND requester.status = 'ACTIVE'
                    AND requester.role = 'TENANT_OWNER'
              )
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            binding_id.as_str(),
            actor.actor_id().as_str(),
        )
        .map_err(dependency_error)?
        .first::<MailboxRow>(None)
        .await
        .map_err(dependency_error)?;
        row.map(map_mailbox_row).transpose()
    }
}

impl GlobalSearchReadModelPort for D1GlobalSearchRepository {
    async fn search_exact(
        &self,
        actor: &ActorContext,
        key: &GlobalSearchKey,
    ) -> Result<Option<GlobalSearchProjection>, QueryPortError> {
        match key {
            GlobalSearchKey::Client(client_id) => self
                .search_client(actor, client_id)
                .await
                .map(|projection| projection.map(GlobalSearchProjection::Client)),
            GlobalSearchKey::Profile(profile_id) => self
                .search_profile(actor, profile_id)
                .await
                .map(|projection| projection.map(GlobalSearchProjection::Profile)),
            GlobalSearchKey::Member(member_id) => self
                .search_member(actor, member_id)
                .await
                .map(|projection| projection.map(GlobalSearchProjection::Member)),
            GlobalSearchKey::Mailbox(binding_id) => self
                .search_mailbox(actor, binding_id)
                .await
                .map(|projection| projection.map(GlobalSearchProjection::Mailbox)),
        }
    }
}

#[derive(Deserialize)]
struct ClientRow {
    client_id: String,
    kind: String,
    display_name: String,
    status: String,
    version: i64,
}

#[derive(Deserialize)]
struct ProfileRow {
    profile_id: String,
    status: String,
    version: i64,
    linked_client_id: Option<String>,
    active_generation_id: Option<String>,
}

#[derive(Deserialize)]
struct MemberRow {
    actor_id: String,
    role: String,
    status: String,
}

#[derive(Deserialize)]
struct MailboxRow {
    binding_id: String,
    provider: String,
    status: String,
    version: i64,
}

fn map_client_row(row: ClientRow) -> Result<ClientReadProjection, QueryPortError> {
    Ok(ClientReadProjection::new(
        ClientId::parse(row.client_id).map_err(|_| integrity_error())?,
        client_kind(&row.kind)?,
        row.display_name,
        client_status(&row.status)?,
        aggregate_version(row.version)?,
    ))
}

fn map_profile_row(row: ProfileRow) -> Result<ProfileReadProjection, QueryPortError> {
    Ok(ProfileReadProjection::new(
        ProfileId::parse(row.profile_id).map_err(|_| integrity_error())?,
        profile_status(&row.status)?,
        aggregate_version(row.version)?,
        row.linked_client_id
            .map(ClientId::parse)
            .transpose()
            .map_err(|_| integrity_error())?,
        row.active_generation_id
            .map(GenerationId::parse)
            .transpose()
            .map_err(|_| integrity_error())?,
    ))
}

fn map_member_row(row: MemberRow) -> Result<MemberReadProjection, QueryPortError> {
    Ok(MemberReadProjection::new(
        ActorId::parse(row.actor_id).map_err(|_| integrity_error())?,
        membership_role(&row.role)?,
        membership_status(&row.status)?,
    ))
}

fn map_mailbox_row(row: MailboxRow) -> Result<MailboxReadProjection, QueryPortError> {
    Ok(MailboxReadProjection::new(
        MailboxBindingId::parse(row.binding_id).map_err(|_| integrity_error())?,
        mailbox_provider(&row.provider)?,
        mailbox_status(&row.status)?,
        aggregate_version(row.version)?,
    ))
}

fn aggregate_version(value: i64) -> Result<AggregateVersion, QueryPortError> {
    let value = u64::try_from(value).map_err(|_| integrity_error())?;
    AggregateVersion::new(value).map_err(|_| integrity_error())
}

fn client_kind(value: &str) -> Result<ClientKind, QueryPortError> {
    match value {
        "PERSON" => Ok(ClientKind::Person),
        "ORGANIZATION" => Ok(ClientKind::Organization),
        _ => Err(integrity_error()),
    }
}

fn client_status(value: &str) -> Result<ClientStatus, QueryPortError> {
    match value {
        "ACTIVE" => Ok(ClientStatus::Active),
        "ARCHIVED" => Ok(ClientStatus::Archived),
        "MERGED" => Ok(ClientStatus::Merged),
        _ => Err(integrity_error()),
    }
}

fn profile_status(value: &str) -> Result<ProfileStatus, QueryPortError> {
    match value {
        "DRAFT" => Ok(ProfileStatus::Draft),
        "QUARANTINED" => Ok(ProfileStatus::Quarantined),
        "READY" => Ok(ProfileStatus::Ready),
        "IN_USE" => Ok(ProfileStatus::InUse),
        "DIRTY_LOCAL" => Ok(ProfileStatus::DirtyLocal),
        "SYNCING" => Ok(ProfileStatus::Syncing),
        "SUSPENDED" => Ok(ProfileStatus::Suspended),
        "DELETING" => Ok(ProfileStatus::Deleting),
        "DELETED" => Ok(ProfileStatus::Deleted),
        _ => Err(integrity_error()),
    }
}

fn membership_role(value: &str) -> Result<MembershipRole, QueryPortError> {
    match value {
        "TENANT_OWNER" => Ok(MembershipRole::TenantOwner),
        "MEMBER" => Ok(MembershipRole::Member),
        _ => Err(integrity_error()),
    }
}

fn membership_status(value: &str) -> Result<MembershipStatus, QueryPortError> {
    match value {
        "ACTIVE" => Ok(MembershipStatus::Active),
        "SUSPENDED" => Ok(MembershipStatus::Suspended),
        "REVOKED" => Ok(MembershipStatus::Revoked),
        _ => Err(integrity_error()),
    }
}

fn mailbox_provider(value: &str) -> Result<MailboxProvider, QueryPortError> {
    MailboxProvider::parse_storage(value).map_err(|_| integrity_error())
}

fn mailbox_status(value: &str) -> Result<MailboxBindingStatus, QueryPortError> {
    match value {
        "ACTIVE" => Ok(MailboxBindingStatus::Active),
        "REVOKED" => Ok(MailboxBindingStatus::Revoked),
        _ => Err(integrity_error()),
    }
}

fn integrity_error() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

fn dependency_error(_error: worker::Error) -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}
