use application_ports::query::{QueryPageSize, QueryPortError, QueryPortErrorClass};
use application_ports::query_clients::{
    ClientContactExactMatchProjection, ClientExactContactQueryPort,
};
use client_domain::{ContactKind, ContactNormalizationVersion, ExactLookupToken};
use profile_platform_primitives::{ActorContext, ClientId, ContactPointId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

pub struct D1ExactContactQueryRepository {
    database: D1Database,
}

impl D1ExactContactQueryRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }
}

impl ClientExactContactQueryPort for D1ExactContactQueryRepository {
    async fn find_visible_clients_by_exact_contact(
        &self,
        actor: &ActorContext,
        kind: ContactKind,
        normalization_version: ContactNormalizationVersion,
        token: &ExactLookupToken,
        limit: QueryPageSize,
    ) -> Result<Vec<ClientContactExactMatchProjection>, QueryPortError> {
        let result = query!(
            &self.database,
            r#"
            SELECT contact.client_id, contact.contact_point_id
            FROM client_contact_points AS contact
            WHERE contact.tenant_id = ?
              AND contact.kind = ?
              AND contact.normalization_version = ?
              AND contact.lookup_key_version = ?
              AND contact.exact_lookup_token = ?
              AND contact.status = 'ACTIVE'
              AND EXISTS (
                  SELECT 1
                  FROM clients AS client
                  WHERE client.tenant_id = contact.tenant_id
                    AND client.client_id = contact.client_id
                    AND client.status = 'ACTIVE'
              )
              AND EXISTS (
                  SELECT 1
                  FROM memberships AS membership
                  WHERE membership.tenant_id = contact.tenant_id
                    AND membership.actor_id = ?
                    AND membership.status = 'ACTIVE'
                    AND (
                        membership.role = 'TENANT_OWNER'
                        OR (
                            membership.role = 'MEMBER'
                            AND EXISTS (
                                SELECT 1
                                FROM client_grants AS grant_row
                                WHERE grant_row.tenant_id = contact.tenant_id
                                  AND grant_row.actor_id = membership.actor_id
                                  AND grant_row.client_id = contact.client_id
                            )
                        )
                    )
              )
            ORDER BY contact.contact_point_id
            LIMIT ?
            "#,
            actor.tenant_scope().tenant_id().as_str(),
            kind.stable_code(),
            i64::from(normalization_version.value()),
            i64::from(token.key_version().value()),
            token.bytes().as_slice(),
            actor.actor_id().as_str(),
            i64::from(limit.value()),
        )
        .map_err(dependency_error)?
        .all()
        .await
        .map_err(dependency_error)?;

        result
            .results::<ExactContactRow>()
            .map_err(dependency_error)?
            .into_iter()
            .map(map_row)
            .collect()
    }
}

#[derive(Deserialize)]
struct ExactContactRow {
    client_id: String,
    contact_point_id: String,
}

fn map_row(row: ExactContactRow) -> Result<ClientContactExactMatchProjection, QueryPortError> {
    Ok(ClientContactExactMatchProjection::new(
        ClientId::parse(row.client_id).map_err(|_| integrity_error())?,
        ContactPointId::parse(row.contact_point_id).map_err(|_| integrity_error())?,
    ))
}

const fn integrity_error() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

fn dependency_error(_error: worker::Error) -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{ExactContactRow, map_row};

    #[test]
    fn exact_contact_rows_restore_typed_ids() -> Result<(), Box<dyn std::error::Error>> {
        let item = map_row(ExactContactRow {
            client_id: "client_01JQUERYCONTACT".to_owned(),
            contact_point_id: "contact_01JQUERYCONTACT".to_owned(),
        })?;
        assert_eq!(item.client_id().as_str(), "client_01JQUERYCONTACT");
        assert_eq!(item.contact_point_id().as_str(), "contact_01JQUERYCONTACT");
        Ok(())
    }
}
