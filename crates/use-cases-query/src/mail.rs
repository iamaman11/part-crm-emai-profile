use super::{QueryApplicationError, authorize, map_port_error};
use application_ports::query::{QueryAuthorizationPort, QueryCapability, QueryPage};
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, ClientMailboxEligibilityPort, MailMessageBody, MailMessageSummary,
    MailboxMessageReference, SearchClientMailboxMessagesRequest,
};
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId};

pub async fn search_client_mailbox_messages<A, E, P>(
    actor: &ActorContext,
    authorization: &A,
    eligibility: &E,
    provider: &P,
    client_id: &ClientId,
    binding_id: &MailboxBindingId,
    request: &SearchClientMailboxMessagesRequest,
) -> Result<QueryPage<MailMessageSummary>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    E: ClientMailboxEligibilityPort,
    P: ClientMailProviderQueryPort,
{
    // Client Mail is a resource-scoped Client operation. The coarse Clients gate
    // proves an ACTIVE membership; exact Owner-or-Client-grant, mailbox association,
    // mailbox lifecycle and provider-lane authority remain in the eligibility port.
    if !authorize(actor, authorization, QueryCapability::Clients).await? {
        return Ok(QueryPage::empty());
    }
    if !eligibility
        .is_mailbox_eligible(actor, client_id, binding_id)
        .await
        .map_err(map_port_error)?
    {
        return Ok(QueryPage::empty());
    }
    provider
        .search_messages(actor.tenant_scope(), binding_id, request)
        .await
        .map_err(map_port_error)
}

pub async fn get_client_mailbox_message<A, E, P>(
    actor: &ActorContext,
    authorization: &A,
    eligibility: &E,
    provider: &P,
    client_id: &ClientId,
    reference: &MailboxMessageReference,
) -> Result<Option<MailMessageBody>, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    E: ClientMailboxEligibilityPort,
    P: ClientMailProviderQueryPort,
{
    if !authorize(actor, authorization, QueryCapability::Clients).await? {
        return Ok(None);
    }
    if !eligibility
        .is_mailbox_eligible(actor, client_id, reference.binding_id())
        .await
        .map_err(map_port_error)?
    {
        return Ok(None);
    }
    provider
        .get_message(actor.tenant_scope(), reference)
        .await
        .map_err(map_port_error)
}
