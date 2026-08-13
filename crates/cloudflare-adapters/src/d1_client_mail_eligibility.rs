use application_ports::client_mail_access::{
    ClientMailboxAccessPort, ClientMailboxAccessPortError, ClientMailboxAccessPortErrorClass,
};
use application_ports::query::{QueryPortError, QueryPortErrorClass};
use application_ports::query_mail_provider::ClientMailboxEligibilityPort;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

pub struct D1ClientMailboxEligibilityRepository {
    database: D1Database,
}

impl D1ClientMailboxEligibilityRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    async fn is_accessible(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
        binding_id: &MailboxBindingId,
    ) -> Result<bool, worker::Error> {
        query!(
            &self.database,
            r#"
SELECT 1 AS eligible
FROM clients AS client
JOIN mailbox_bindings AS binding
  ON binding.tenant_id = client.tenant_id
 AND binding.binding_id = ?
JOIN mailbox_client_association_state AS association
  ON association.tenant_id = binding.tenant_id
 AND association.binding_id = binding.binding_id
 AND association.client_id = client.client_id
WHERE client.tenant_id = ?
  AND client.client_id = ?
  AND client.status = 'ACTIVE'
  AND binding.status = 'ACTIVE'
  AND binding.execution_status = 'ACTIVE'
  AND (
      binding.provider IN ('GMAIL_API', 'IMAP')
      OR binding.provider = 'MICROSOFT_GRAPH'
  )
  AND EXISTS (
      SELECT 1
      FROM memberships AS requester
      WHERE requester.tenant_id = client.tenant_id
        AND requester.actor_id = ?
        AND requester.status = 'ACTIVE'
        AND (
            requester.role = 'TENANT_OWNER'
            OR (
                requester.role = 'MEMBER'
                AND EXISTS (
                    SELECT 1
                    FROM client_grants AS grant_row
                    WHERE grant_row.tenant_id = client.tenant_id
                      AND grant_row.actor_id = requester.actor_id
                      AND grant_row.client_id = client.client_id
                )
            )
        )
  )
LIMIT 1
"#,
            binding_id.as_str(),
            actor.tenant_scope().tenant_id().as_str(),
            client_id.as_str(),
            actor.actor_id().as_str(),
        )?
        .first::<EligibilityRow>(None)
        .await
        .map(|row| row.is_some())
    }
}

impl ClientMailboxEligibilityPort for D1ClientMailboxEligibilityRepository {
    async fn is_mailbox_eligible(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
        binding_id: &MailboxBindingId,
    ) -> Result<bool, QueryPortError> {
        self.is_accessible(actor, client_id, binding_id)
            .await
            .map_err(query_dependency_error)
    }
}

impl ClientMailboxAccessPort for D1ClientMailboxEligibilityRepository {
    async fn is_mailbox_accessible(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
        binding_id: &MailboxBindingId,
    ) -> Result<bool, ClientMailboxAccessPortError> {
        self.is_accessible(actor, client_id, binding_id)
            .await
            .map_err(access_dependency_error)
    }
}

#[derive(Deserialize)]
struct EligibilityRow {
    #[allow(dead_code)]
    eligible: i64,
}

fn query_dependency_error(_error: worker::Error) -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}

fn access_dependency_error(_error: worker::Error) -> ClientMailboxAccessPortError {
    ClientMailboxAccessPortError::new(ClientMailboxAccessPortErrorClass::DependencyUnavailable)
}
