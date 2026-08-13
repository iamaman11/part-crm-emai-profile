use application_ports::mailboxes::MailboxProviderPortError;
use application_ports::query::{QueryPortError, QueryPortErrorClass};
use mailbox_domain::MailboxProviderFailureClass;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::query;

use crate::cloud_mailbox_secrets::provider_error;

pub struct D1MicrosoftGraphAuthorization {
    database: D1Database,
}

impl D1MicrosoftGraphAuthorization {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn recheck_client_query(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
        binding_id: &MailboxBindingId,
    ) -> Result<bool, QueryPortError> {
        self.load_authorized_client(actor, binding_id, Some(client_id))
            .await
            .map(|client| client.is_some())
            .map_err(|_| QueryPortError::new(QueryPortErrorClass::DependencyUnavailable))
    }

    pub async fn recheck_job(
        &self,
        actor: &ActorContext,
        binding_id: &MailboxBindingId,
    ) -> Result<ClientId, MailboxProviderPortError> {
        let client = self
            .load_authorized_client(actor, binding_id, None)
            .await
            .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?
            .ok_or_else(|| provider_error(MailboxProviderFailureClass::ProviderPolicy))?;
        ClientId::parse(client.client_id).map_err(|_| MailboxProviderPortError::IntegrityFailure)
    }

    async fn load_authorized_client(
        &self,
        actor: &ActorContext,
        binding_id: &MailboxBindingId,
        expected_client: Option<&ClientId>,
    ) -> Result<Option<AuthorizedClientRow>, worker::Error> {
        let expected_client = expected_client.map(ClientId::as_str);
        query!(
            &self.database,
            r#"
            SELECT association.client_id AS client_id
            FROM mailbox_bindings AS binding
            JOIN mailbox_client_association_state AS association
              ON association.tenant_id = binding.tenant_id
             AND association.binding_id = binding.binding_id
            JOIN clients AS client
              ON client.tenant_id = association.tenant_id
             AND client.client_id = association.client_id
            JOIN memberships AS requester
              ON requester.tenant_id = binding.tenant_id
             AND requester.actor_id = ?
            WHERE binding.tenant_id = ?
              AND binding.binding_id = ?
              AND binding.provider = 'MICROSOFT_GRAPH'
              AND binding.status = 'ACTIVE'
              AND binding.execution_status = 'ACTIVE'
              AND association.client_id IS NOT NULL
              AND client.status = 'ACTIVE'
              AND requester.status = 'ACTIVE'
              AND requester.role IN ('TENANT_OWNER', 'MEMBER')
              AND (? IS NULL OR client.client_id = ?)
              AND EXISTS (
                  SELECT 1
                  FROM client_grants AS grant_row
                  WHERE grant_row.tenant_id = client.tenant_id
                    AND grant_row.client_id = client.client_id
                    AND grant_row.actor_id = requester.actor_id
              )
            LIMIT 1
            "#,
            actor.actor_id().as_str(),
            actor.tenant_scope().tenant_id().as_str(),
            binding_id.as_str(),
            expected_client,
            expected_client,
        )?
        .first::<AuthorizedClientRow>(None)
        .await
    }
}

#[derive(Deserialize)]
struct AuthorizedClientRow {
    client_id: String,
}
