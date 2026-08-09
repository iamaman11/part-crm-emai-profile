use application_ports::query::{
    QueryAuthorizationPort, QueryCapability, QueryCursor, QueryPage, QueryPageRequest, QueryPortError,
    QueryPortErrorClass,
};
use application_ports::query_clients::{ClientReadModelPort, ClientReadProjection};
use application_ports::query_mailboxes::{MailboxReadModelPort, MailboxReadProjection};
use application_ports::query_members::{MemberReadModelPort, MemberReadProjection};
use application_ports::query_profiles::{ProfileReadModelPort, ProfileReadProjection};
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

const CLIENT_CURSOR_PREFIX: &str = "clients:";
const PROFILE_CURSOR_PREFIX: &str = "profiles:";
const MEMBER_CURSOR_PREFIX: &str = "members:";
const MAILBOX_CURSOR_PREFIX: &str = "mailboxes:";

pub struct D1QueryRepository {
    database: D1Database,
}

impl D1QueryRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn active_membership_role(
        &self,
        actor: &ActorContext,
    ) -> Result<Option<MembershipRole>, QueryPortError> {
        let role = query!(
            &self.database,
            r#"
            SELECT role
            FROM memberships
            WHERE tenant_id = ?
              AND actor_id = ?
              AND status = 'ACTIVE'
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            actor.actor_id().as_str(),
        )
        .map_err(dependency_error)?
        .first::<String>(Some("role"))
        .await
        .map_err(dependency_error)?;

        role.map(|value| membership_role(&value)).transpose()
    }
}

impl QueryAuthorizationPort for D1QueryRepository {
    async fn is_query_authorized(
        &self,
        actor: &ActorContext,
        capability: QueryCapability,
    ) -> Result<bool, QueryPortError> {
        let Some(role) = self.active_membership_role(actor).await? else {
            return Ok(false);
        };
        Ok(match capability {
            QueryCapability::Clients | QueryCapability::Profiles | QueryCapability::GlobalSearch => {
                true
            }
            QueryCapability::Members | QueryCapability::Mailboxes | QueryCapability::Mail => {
                role == MembershipRole::TenantOwner
            }
        })
    }
}

impl ClientReadModelPort for D1QueryRepository {
    async fn list_clients(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> Result<QueryPage<ClientReadProjection>, QueryPortError> {
        let after = client_cursor(page)?;
        let fetch_limit = fetch_limit(page);
        let result = query!(
            &self.database,
            r#"
            SELECT client.client_id, client.kind, client.display_name, client.status, client.version
            FROM clients AS client
            WHERE client.tenant_id = ?
              AND client.client_id > ?
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
            ORDER BY client.client_id
            LIMIT ?
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            after.as_str(),
            actor.actor_id().as_str(),
            fetch_limit,
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;

        let rows = result
            .results::<ClientRow>()
            .map_err(dependency_error)?;
        client_page(rows, page)
    }
}

impl ProfileReadModelPort for D1QueryRepository {
    async fn list_profiles(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> Result<QueryPage<ProfileReadProjection>, QueryPortError> {
        let after = profile_cursor(page)?;
        let fetch_limit = fetch_limit(page);
        let result = query!(
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
              AND profile.profile_id > ?
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
            ORDER BY profile.profile_id
            LIMIT ?
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            after.as_str(),
            actor.actor_id().as_str(),
            fetch_limit,
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;

        let rows = result
            .results::<ProfileRow>()
            .map_err(dependency_error)?;
        profile_page(rows, page)
    }
}

impl MemberReadModelPort for D1QueryRepository {
    async fn list_members(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> Result<QueryPage<MemberReadProjection>, QueryPortError> {
        let after = member_cursor(page)?;
        let fetch_limit = fetch_limit(page);
        let result = query!(
            &self.database,
            r#"
            SELECT member.actor_id, member.role, member.status
            FROM memberships AS member
            WHERE member.tenant_id = ?
              AND member.actor_id > ?
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS requester
                  WHERE requester.tenant_id = member.tenant_id
                    AND requester.actor_id = ?
                    AND requester.status = 'ACTIVE'
                    AND requester.role = 'TENANT_OWNER'
              )
            ORDER BY member.actor_id
            LIMIT ?
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            after.as_str(),
            actor.actor_id().as_str(),
            fetch_limit,
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;

        let rows = result
            .results::<MemberRow>()
            .map_err(dependency_error)?;
        member_page(rows, page)
    }
}

impl MailboxReadModelPort for D1QueryRepository {
    async fn list_mailboxes(
        &self,
        actor: &ActorContext,
        page: &QueryPageRequest,
    ) -> Result<QueryPage<MailboxReadProjection>, QueryPortError> {
        let after = mailbox_cursor(page)?;
        let fetch_limit = fetch_limit(page);
        let result = query!(
            &self.database,
            r#"
            SELECT binding.binding_id, binding.provider, binding.status, binding.version
            FROM mailbox_bindings AS binding
            WHERE binding.tenant_id = ?
              AND binding.binding_id > ?
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS requester
                  WHERE requester.tenant_id = binding.tenant_id
                    AND requester.actor_id = ?
                    AND requester.status = 'ACTIVE'
                    AND requester.role = 'TENANT_OWNER'
              )
            ORDER BY binding.binding_id
            LIMIT ?
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            after.as_str(),
            actor.actor_id().as_str(),
            fetch_limit,
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;

        let rows = result
            .results::<MailboxRow>()
            .map_err(dependency_error)?;
        mailbox_page(rows, page)
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

fn fetch_limit(page: &QueryPageRequest) -> i64 {
    i64::from(page.limit().value()) + 1
}

fn client_cursor(page: &QueryPageRequest) -> Result<String, QueryPortError> {
    let value = cursor_value(page, CLIENT_CURSOR_PREFIX)?;
    if value.is_empty() {
        return Ok(value);
    }
    ClientId::parse(&value).map_err(|_| invalid_cursor())?;
    Ok(value)
}

fn profile_cursor(page: &QueryPageRequest) -> Result<String, QueryPortError> {
    let value = cursor_value(page, PROFILE_CURSOR_PREFIX)?;
    if value.is_empty() {
        return Ok(value);
    }
    ProfileId::parse(&value).map_err(|_| invalid_cursor())?;
    Ok(value)
}

fn member_cursor(page: &QueryPageRequest) -> Result<String, QueryPortError> {
    let value = cursor_value(page, MEMBER_CURSOR_PREFIX)?;
    if value.is_empty() {
        return Ok(value);
    }
    ActorId::parse(&value).map_err(|_| invalid_cursor())?;
    Ok(value)
}

fn mailbox_cursor(page: &QueryPageRequest) -> Result<String, QueryPortError> {
    let value = cursor_value(page, MAILBOX_CURSOR_PREFIX)?;
    if value.is_empty() {
        return Ok(value);
    }
    MailboxBindingId::parse(&value).map_err(|_| invalid_cursor())?;
    Ok(value)
}

fn cursor_value(page: &QueryPageRequest, prefix: &str) -> Result<String, QueryPortError> {
    let Some(cursor) = page.cursor() else {
        return Ok(String::new());
    };
    cursor
        .as_str()
        .strip_prefix(prefix)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(invalid_cursor)
}

fn client_page(
    rows: Vec<ClientRow>,
    page: &QueryPageRequest,
) -> Result<QueryPage<ClientReadProjection>, QueryPortError> {
    let limit = usize::from(page.limit().value());
    let has_more = rows.len() > limit;
    let items = rows
        .into_iter()
        .take(limit)
        .map(map_client_row)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| cursor(CLIENT_CURSOR_PREFIX, item.client_id().as_str()))
            .transpose()?
    } else {
        None
    };
    Ok(QueryPage::new(items, next_cursor))
}

fn profile_page(
    rows: Vec<ProfileRow>,
    page: &QueryPageRequest,
) -> Result<QueryPage<ProfileReadProjection>, QueryPortError> {
    let limit = usize::from(page.limit().value());
    let has_more = rows.len() > limit;
    let items = rows
        .into_iter()
        .take(limit)
        .map(map_profile_row)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| cursor(PROFILE_CURSOR_PREFIX, item.profile_id().as_str()))
            .transpose()?
    } else {
        None
    };
    Ok(QueryPage::new(items, next_cursor))
}

fn member_page(
    rows: Vec<MemberRow>,
    page: &QueryPageRequest,
) -> Result<QueryPage<MemberReadProjection>, QueryPortError> {
    let limit = usize::from(page.limit().value());
    let has_more = rows.len() > limit;
    let items = rows
        .into_iter()
        .take(limit)
        .map(map_member_row)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| cursor(MEMBER_CURSOR_PREFIX, item.actor_id().as_str()))
            .transpose()?
    } else {
        None
    };
    Ok(QueryPage::new(items, next_cursor))
}

fn mailbox_page(
    rows: Vec<MailboxRow>,
    page: &QueryPageRequest,
) -> Result<QueryPage<MailboxReadProjection>, QueryPortError> {
    let limit = usize::from(page.limit().value());
    let has_more = rows.len() > limit;
    let items = rows
        .into_iter()
        .take(limit)
        .map(map_mailbox_row)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| cursor(MAILBOX_CURSOR_PREFIX, item.binding_id().as_str()))
            .transpose()?
    } else {
        None
    };
    Ok(QueryPage::new(items, next_cursor))
}

fn cursor(prefix: &str, id: &str) -> Result<QueryCursor, QueryPortError> {
    QueryCursor::parse(format!("{prefix}{id}")).map_err(|_| integrity_error())
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

fn invalid_cursor() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::InvalidCursor)
}

fn integrity_error() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

fn dependency_error(_error: worker::Error) -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{client_cursor, mailbox_cursor};
    use application_ports::query::{
        QueryCursor, QueryPageRequest, QueryPageSize, QueryPortErrorClass,
    };

    #[test]
    fn domain_scoped_cursors_reject_cross_projection_reuse()
    -> Result<(), Box<dyn std::error::Error>> {
        let page = QueryPageRequest::new(
            QueryPageSize::new(25)?,
            Some(QueryCursor::parse("mailboxes:binding_01JQUERY")?),
        );
        let error = client_cursor(&page).expect_err("mailbox cursor must not enter client query");
        assert_eq!(error.class(), QueryPortErrorClass::InvalidCursor);
        assert_eq!(mailbox_cursor(&page)?, "binding_01JQUERY");
        Ok(())
    }
}
