use application_ports::outbound_mail::OutboundMailIntent;
use application_ports::query::QueryAuthorizationPort;
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, ClientMailboxEligibilityPort, MailboxMessageReference,
};
use profile_platform_primitives::{ActorContext, ClientId};
use use_cases_query::{QueryApplicationError, get_client_mailbox_message};

pub(super) async fn is_accessible<A, E, P>(
    actor: &ActorContext,
    client_id: &ClientId,
    authorization: &A,
    eligibility: &E,
    provider: &P,
    intent: &OutboundMailIntent,
) -> Result<bool, QueryApplicationError>
where
    A: QueryAuthorizationPort,
    E: ClientMailboxEligibilityPort,
    P: ClientMailProviderQueryPort,
{
    let Some(source) = intent.operation().source() else {
        return Ok(true);
    };
    let reference = MailboxMessageReference::new(
        intent.binding_id().clone(),
        source.provider_reference().as_str().to_owned(),
    )
    .map_err(|_| QueryApplicationError::InvalidInput)?;
    match get_client_mailbox_message(
        actor,
        authorization,
        eligibility,
        provider,
        client_id,
        &reference,
    )
    .await
    {
        Ok(Some(_)) => Ok(true),
        Ok(None) => Ok(false),
        Err(error) => Err(error),
    }
}
