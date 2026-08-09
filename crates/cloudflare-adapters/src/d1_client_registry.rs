use application_ports::client_registry::{
    ClientRegistryActivityProjection, ClientRegistryAssignmentProjection,
    ClientRegistryContactProjection, ClientRegistryHistoryProjection, ClientRegistryListItem,
    ClientRegistryProjectionError, ClientRegistryProjectionErrorClass, ClientRegistryProjectionPort,
};
use client_domain::{AssignmentStatus, ClientKind, ClientStatus, ContactKind, ContactStatus};
use identity_access_domain::MembershipRole;
use profile_platform_primitives::{
    ActorId, AggregateVersion, AssignmentId, AuditEventId, ClientId, ContactPointId, ProfileId,
    TenantScope, UnixMillis,
};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

pub struct D1ClientRegistryProjectionRepository {
    database: D1Database,
}

impl D1ClientRegistryProjectionRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl ClientRegistryProjectionPort for D1ClientRegistryProjectionRepository {
    async fn list_visible_clients(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
    ) -> Result<Vec<ClientRegistryListItem>, ClientRegistryProjectionError> {
        let owner = owner_flag(role);
        let result = query!(
            &self.database,
            r#"
            SELECT client.client_id, client.kind, client.display_name, client.status, client.version
            FROM clients AS client
            WHERE client.tenant_id = ?
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS membership
                  WHERE membership.tenant_id = client.tenant_id
                    AND membership.actor_id = ?
                    AND membership.status = 'ACTIVE'
                    AND (
                        (? = 1 AND membership.role = 'TENANT_OWNER')
                        OR (
                            ? = 0
                            AND membership.role = 'MEMBER'
                            AND EXISTS (
                                SELECT 1
                                FROM client_grants AS grant_row
                                WHERE grant_row.tenant_id = client.tenant_id
                                  AND grant_row.client_id = client.client_id
                                  AND grant_row.actor_id = membership.actor_id
                            )
                        )
                    )
              )
            ORDER BY client.display_name, client.client_id
            "#,
            scope.tenant_id().as_str(),
            actor_id.as_str(),
            owner,
            owner,
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;
        result
            .results::<ClientListRow>()
            .map_err(dependency_error)?
            .into_iter()
            .map(map_client_list_row)
            .collect()
    }

    async fn load_visible_client_history(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        role: MembershipRole,
        client_id: &ClientId,
    ) -> Result<Option<ClientRegistryHistoryProjection>, ClientRegistryProjectionError> {
        let owner = owner_flag(role);
        let contacts = self
            .visible_contact_rows(scope, actor_id, owner, client_id)
            .await?;
        if contacts.is_none() {
            return Ok(None);
        }
        let assignments = self
            .visible_assignment_rows(scope, actor_id, owner, client_id)
            .await?;
        if assignments.is_none() {
            return Ok(None);
        }
        let activity = self
            .visible_activity_rows(scope, actor_id, owner, client_id)
            .await?;
        if activity.is_none() {
            return Ok(None);
        }
        if !self
            .is_still_visible(scope, actor_id, owner, client_id)
            .await?
        {
            return Ok(None);
        }
        Ok(Some(ClientRegistryHistoryProjection::new(
            contacts.expect("visibility checked"),
            assignments.expect("visibility checked"),
            activity.expect("visibility checked"),
        )))
    }
}

impl D1ClientRegistryProjectionRepository {
    async fn visible_contact_rows(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        owner: i32,
        client_id: &ClientId,
    ) -> Result<Option<Vec<ClientRegistryContactProjection>>, ClientRegistryProjectionError> {
        let result = query!(
            &self.database,
            r#"
            WITH visible_client AS (
                SELECT client.client_id
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
                            (? = 1 AND membership.role = 'TENANT_OWNER')
                            OR (
                                ? = 0
                                AND membership.role = 'MEMBER'
                                AND EXISTS (
                                    SELECT 1 FROM client_grants AS grant_row
                                    WHERE grant_row.tenant_id = client.tenant_id
                                      AND grant_row.client_id = client.client_id
                                      AND grant_row.actor_id = membership.actor_id
                                )
                            )
                        )
                  )
            )
            SELECT visible_client.client_id AS visible_client_id,
                   contact.contact_point_id, contact.kind, contact.status
            FROM visible_client
            LEFT JOIN client_contact_points AS contact
              ON contact.tenant_id = ?
             AND contact.client_id = visible_client.client_id
            ORDER BY contact.contact_point_id
            "#,
            scope.tenant_id().as_str(),
            client_id.as_str(),
            actor_id.as_str(),
            owner,
            owner,
            scope.tenant_id().as_str(),
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;
        let rows = result
            .results::<ContactRow>()
            .map_err(dependency_error)?;
        marker_map(rows, map_contact_row)
    }

    async fn visible_assignment_rows(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        owner: i32,
        client_id: &ClientId,
    ) -> Result<Option<Vec<ClientRegistryAssignmentProjection>>, ClientRegistryProjectionError> {
        let result = query!(
            &self.database,
            r#"
            WITH visible_client AS (
                SELECT client.client_id
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
                            (? = 1 AND membership.role = 'TENANT_OWNER')
                            OR (
                                ? = 0
                                AND membership.role = 'MEMBER'
                                AND EXISTS (
                                    SELECT 1 FROM client_grants AS grant_row
                                    WHERE grant_row.tenant_id = client.tenant_id
                                      AND grant_row.client_id = client.client_id
                                      AND grant_row.actor_id = membership.actor_id
                                )
                            )
                        )
                  )
            )
            SELECT visible_client.client_id AS visible_client_id,
                   assignment.assignment_id,
                   assignment.profile_id,
                   assignment.assigned_at_ms,
                   assignment.closed_at_ms,
                   assignment.reason
            FROM visible_client
            LEFT JOIN profile_client_assignments AS assignment
              ON assignment.tenant_id = ?
             AND assignment.client_id = visible_client.client_id
             AND (
                 ? = 1
                 OR EXISTS (
                     SELECT 1
                     FROM profile_grants AS profile_grant
                     WHERE profile_grant.tenant_id = assignment.tenant_id
                       AND profile_grant.profile_id = assignment.profile_id
                       AND profile_grant.actor_id = ?
                 )
             )
            ORDER BY assignment.assigned_at_ms DESC, assignment.assignment_id
            "#,
            scope.tenant_id().as_str(),
            client_id.as_str(),
            actor_id.as_str(),
            owner,
            owner,
            scope.tenant_id().as_str(),
            owner,
            actor_id.as_str(),
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;
        let rows = result
            .results::<AssignmentRow>()
            .map_err(dependency_error)?;
        marker_map(rows, map_assignment_row)
    }

    async fn visible_activity_rows(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        owner: i32,
        client_id: &ClientId,
    ) -> Result<Option<Vec<ClientRegistryActivityProjection>>, ClientRegistryProjectionError> {
        let result = query!(
            &self.database,
            r#"
            WITH visible_client AS (
                SELECT client.client_id
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
                            (? = 1 AND membership.role = 'TENANT_OWNER')
                            OR (
                                ? = 0
                                AND membership.role = 'MEMBER'
                                AND EXISTS (
                                    SELECT 1 FROM client_grants AS grant_row
                                    WHERE grant_row.tenant_id = client.tenant_id
                                      AND grant_row.client_id = client.client_id
                                      AND grant_row.actor_id = membership.actor_id
                                )
                            )
                        )
                  )
            ), bounded_activity AS (
                SELECT audit_event_id, action, resource_type, resource_id, result_code, occurred_at_ms
                FROM audit_events
                WHERE tenant_id = ?
                  AND (
                      (resource_type = 'client' AND resource_id = ?)
                      OR (
                          resource_type = 'client_contact'
                          AND EXISTS (
                              SELECT 1
                              FROM client_contact_points AS contact
                              WHERE contact.tenant_id = audit_events.tenant_id
                                AND contact.client_id = ?
                                AND contact.contact_point_id = audit_events.resource_id
                          )
                      )
                  )
                ORDER BY occurred_at_ms DESC, audit_event_id DESC
                LIMIT 100
            )
            SELECT visible_client.client_id AS visible_client_id,
                   activity.audit_event_id, activity.action, activity.resource_type,
                   activity.resource_id, activity.result_code, activity.occurred_at_ms
            FROM visible_client
            LEFT JOIN bounded_activity AS activity ON 1 = 1
            ORDER BY activity.occurred_at_ms DESC, activity.audit_event_id DESC
            "#,
            scope.tenant_id().as_str(),
            client_id.as_str(),
            actor_id.as_str(),
            owner,
            owner,
            scope.tenant_id().as_str(),
            client_id.as_str(),
            client_id.as_str(),
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;
        let rows = result
            .results::<ActivityRow>()
            .map_err(dependency_error)?;
        marker_map(rows, map_activity_row)
    }

    async fn is_still_visible(
        &self,
        scope: &TenantScope,
        actor_id: &ActorId,
        owner: i32,
        client_id: &ClientId,
    ) -> Result<bool, ClientRegistryProjectionError> {
        let row = query!(
            &self.database,
            r#"
            SELECT client.client_id
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
                        (? = 1 AND membership.role = 'TENANT_OWNER')
                        OR (
                            ? = 0
                            AND membership.role = 'MEMBER'
                            AND EXISTS (
                                SELECT 1 FROM client_grants AS grant_row
                                WHERE grant_row.tenant_id = client.tenant_id
                                  AND grant_row.client_id = client.client_id
                                  AND grant_row.actor_id = membership.actor_id
                            )
                        )
                    )
              )
            "#,
            scope.tenant_id().as_str(),
            client_id.as_str(),
            actor_id.as_str(),
            owner,
            owner,
        )
        .map_err(dependency_error)?
        .first::<String>(Some("client_id"))
        .await
        .map_err(dependency_error)?;
        Ok(row.is_some())
    }
}

#[derive(Deserialize)]
struct ClientListRow {
    client_id: String,
    kind: String,
    display_name: String,
    status: String,
    version: i64,
}

#[derive(Deserialize)]
struct ContactRow {
    visible_client_id: String,
    contact_point_id: Option<String>,
    kind: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
struct AssignmentRow {
    visible_client_id: String,
    assignment_id: Option<String>,
    profile_id: Option<String>,
    assigned_at_ms: Option<i64>,
    closed_at_ms: Option<i64>,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct ActivityRow {
    visible_client_id: String,
    audit_event_id: Option<String>,
    action: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    result_code: Option<String>,
    occurred_at_ms: Option<i64>,
}

trait VisibilityMarker {
    fn visible_client_id(&self) -> &str;
}

impl VisibilityMarker for ContactRow {
    fn visible_client_id(&self) -> &str {
        &self.visible_client_id
    }
}

impl VisibilityMarker for AssignmentRow {
    fn visible_client_id(&self) -> &str {
        &self.visible_client_id
    }
}

impl VisibilityMarker for ActivityRow {
    fn visible_client_id(&self) -> &str {
        &self.visible_client_id
    }
}

fn marker_map<R, T>(
    rows: Vec<R>,
    mapper: impl Fn(R) -> Result<Option<T>, ClientRegistryProjectionError>,
) -> Result<Option<Vec<T>>, ClientRegistryProjectionError>
where
    R: VisibilityMarker,
{
    if rows.is_empty() {
        return Ok(None);
    }
    let marker = rows[0].visible_client_id().to_owned();
    if marker.is_empty() || rows.iter().any(|row| row.visible_client_id() != marker) {
        return Err(integrity_error());
    }
    rows.into_iter()
        .map(mapper)
        .filter_map(|result| match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn map_client_list_row(
    row: ClientListRow,
) -> Result<ClientRegistryListItem, ClientRegistryProjectionError> {
    Ok(ClientRegistryListItem::new(
        ClientId::parse(row.client_id).map_err(|_| integrity_error())?,
        client_kind(&row.kind)?,
        row.display_name,
        client_status(&row.status)?,
        aggregate_version(row.version)?,
    ))
}

fn map_contact_row(
    row: ContactRow,
) -> Result<Option<ClientRegistryContactProjection>, ClientRegistryProjectionError> {
    match (row.contact_point_id, row.kind, row.status) {
        (None, None, None) => Ok(None),
        (Some(id), Some(kind), Some(status)) => Ok(Some(ClientRegistryContactProjection::new(
            ContactPointId::parse(id).map_err(|_| integrity_error())?,
            contact_kind(&kind)?,
            contact_status(&status)?,
        ))),
        _ => Err(integrity_error()),
    }
}

fn map_assignment_row(
    row: AssignmentRow,
) -> Result<Option<ClientRegistryAssignmentProjection>, ClientRegistryProjectionError> {
    match (
        row.assignment_id,
        row.profile_id,
        row.assigned_at_ms,
        row.reason,
    ) {
        (None, None, None, None) if row.closed_at_ms.is_none() => Ok(None),
        (Some(assignment_id), Some(profile_id), Some(assigned_at_ms), Some(reason)) => {
            let closed_at = row.closed_at_ms.map(unix_millis).transpose()?;
            Ok(Some(ClientRegistryAssignmentProjection::new(
                AssignmentId::parse(assignment_id).map_err(|_| integrity_error())?,
                ProfileId::parse(profile_id).map_err(|_| integrity_error())?,
                if closed_at.is_some() {
                    AssignmentStatus::Closed
                } else {
                    AssignmentStatus::Active
                },
                unix_millis(assigned_at_ms)?,
                closed_at,
                reason,
            )))
        }
        _ => Err(integrity_error()),
    }
}

fn map_activity_row(
    row: ActivityRow,
) -> Result<Option<ClientRegistryActivityProjection>, ClientRegistryProjectionError> {
    match (
        row.audit_event_id,
        row.action,
        row.resource_type,
        row.resource_id,
        row.result_code,
        row.occurred_at_ms,
    ) {
        (None, None, None, None, None, None) => Ok(None),
        (
            Some(audit_event_id),
            Some(action),
            Some(resource_type),
            Some(resource_id),
            Some(result_code),
            Some(occurred_at_ms),
        ) => Ok(Some(ClientRegistryActivityProjection::new(
            AuditEventId::parse(audit_event_id).map_err(|_| integrity_error())?,
            action,
            resource_type,
            resource_id,
            result_code,
            unix_millis(occurred_at_ms)?,
        ))),
        _ => Err(integrity_error()),
    }
}

const fn owner_flag(role: MembershipRole) -> i32 {
    match role {
        MembershipRole::TenantOwner => 1,
        MembershipRole::Member => 0,
    }
}

fn client_kind(value: &str) -> Result<ClientKind, ClientRegistryProjectionError> {
    match value {
        "PERSON" => Ok(ClientKind::Person),
        "ORGANIZATION" => Ok(ClientKind::Organization),
        _ => Err(integrity_error()),
    }
}

fn client_status(value: &str) -> Result<ClientStatus, ClientRegistryProjectionError> {
    match value {
        "ACTIVE" => Ok(ClientStatus::Active),
        "ARCHIVED" => Ok(ClientStatus::Archived),
        "MERGED" => Ok(ClientStatus::Merged),
        _ => Err(integrity_error()),
    }
}

fn contact_kind(value: &str) -> Result<ContactKind, ClientRegistryProjectionError> {
    match value {
        "EMAIL" => Ok(ContactKind::Email),
        "PHONE" => Ok(ContactKind::Phone),
        "URL" => Ok(ContactKind::Url),
        _ => Err(integrity_error()),
    }
}

fn contact_status(value: &str) -> Result<ContactStatus, ClientRegistryProjectionError> {
    match value {
        "ACTIVE" => Ok(ContactStatus::Active),
        "ARCHIVED" => Ok(ContactStatus::Archived),
        _ => Err(integrity_error()),
    }
}

fn aggregate_version(value: i64) -> Result<AggregateVersion, ClientRegistryProjectionError> {
    let value = u64::try_from(value).map_err(|_| integrity_error())?;
    AggregateVersion::new(value).map_err(|_| integrity_error())
}

fn unix_millis(value: i64) -> Result<UnixMillis, ClientRegistryProjectionError> {
    Ok(UnixMillis::new(
        u64::try_from(value).map_err(|_| integrity_error())?,
    ))
}

fn dependency_error(_error: worker::Error) -> ClientRegistryProjectionError {
    ClientRegistryProjectionError::new(ClientRegistryProjectionErrorClass::DependencyUnavailable)
}

const fn integrity_error() -> ClientRegistryProjectionError {
    ClientRegistryProjectionError::new(ClientRegistryProjectionErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::{ActivityRow, AssignmentRow, ContactRow, map_activity_row, map_assignment_row, map_contact_row};

    #[test]
    fn left_join_marker_rows_map_to_empty_projection_items() {
        assert!(
            map_contact_row(ContactRow {
                visible_client_id: "client_01JREGISTRY".to_owned(),
                contact_point_id: None,
                kind: None,
                status: None,
            })
            .expect("valid empty contact marker")
            .is_none()
        );
        assert!(
            map_assignment_row(AssignmentRow {
                visible_client_id: "client_01JREGISTRY".to_owned(),
                assignment_id: None,
                profile_id: None,
                assigned_at_ms: None,
                closed_at_ms: None,
                reason: None,
            })
            .expect("valid empty assignment marker")
            .is_none()
        );
        assert!(
            map_activity_row(ActivityRow {
                visible_client_id: "client_01JREGISTRY".to_owned(),
                audit_event_id: None,
                action: None,
                resource_type: None,
                resource_id: None,
                result_code: None,
                occurred_at_ms: None,
            })
            .expect("valid empty activity marker")
            .is_none()
        );
    }
}
