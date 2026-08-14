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

#[cfg(test)]
mod tests {
    use super::is_accessible;
    use application_ports::outbound_mail::{
        MailBody, OutboundMailIntent, OutboundMailOperation, OutboundMailSourceReference,
        ProviderMessageReference,
    };
    use application_ports::query::{
        QueryAuthorizationPort, QueryCapability, QueryPage, QueryPortError, QueryPortErrorClass,
    };
    use application_ports::query_mail_provider::{
        ClientMailProviderQueryPort, ClientMailboxEligibilityPort, MailMessageBody,
        MailMessageSummary, MailboxMessageReference, SearchClientMailboxMessagesRequest,
    };
    use profile_platform_primitives::{
        ActorContext, ActorId, ClientId, CorrelationId, MailboxBindingId, TenantId, TenantScope,
    };
    use std::cell::Cell;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    use use_cases_query::QueryApplicationError;

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    struct FakeAuthorization;

    impl QueryAuthorizationPort for FakeAuthorization {
        async fn is_query_authorized(
            &self,
            _actor: &ActorContext,
            capability: QueryCapability,
        ) -> Result<bool, QueryPortError> {
            assert_eq!(capability, QueryCapability::Clients);
            Ok(true)
        }
    }

    struct FakeEligibility {
        client_id: ClientId,
        binding_id: MailboxBindingId,
    }

    impl ClientMailboxEligibilityPort for FakeEligibility {
        async fn is_mailbox_eligible(
            &self,
            _actor: &ActorContext,
            client_id: &ClientId,
            binding_id: &MailboxBindingId,
        ) -> Result<bool, QueryPortError> {
            Ok(client_id == &self.client_id && binding_id == &self.binding_id)
        }
    }

    struct FakeProvider {
        get_calls: Cell<u32>,
        dependency_failure: bool,
    }

    impl ClientMailProviderQueryPort for FakeProvider {
        async fn search_messages(
            &self,
            _scope: &TenantScope,
            _binding_id: &MailboxBindingId,
            _request: &SearchClientMailboxMessagesRequest,
        ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
            Ok(QueryPage::empty())
        }

        async fn get_message(
            &self,
            _scope: &TenantScope,
            _reference: &MailboxMessageReference,
        ) -> Result<Option<MailMessageBody>, QueryPortError> {
            self.get_calls.set(self.get_calls.get() + 1);
            if self.dependency_failure {
                Err(QueryPortError::new(
                    QueryPortErrorClass::DependencyUnavailable,
                ))
            } else {
                Ok(None)
            }
        }
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JC7SOURCE")?),
            ActorId::parse("actor_01JC7SOURCE")?,
            CorrelationId::parse("corr_01JC7SOURCE")?,
        ))
    }

    fn reply_intent(
        client_id: ClientId,
        binding_id: MailboxBindingId,
    ) -> Result<OutboundMailIntent, Box<dyn std::error::Error>> {
        let source = OutboundMailSourceReference::new(
            binding_id.clone(),
            ProviderMessageReference::parse("gmail:source-message-1")?,
        );
        Ok(OutboundMailIntent::new(
            client_id,
            binding_id,
            OutboundMailOperation::Reply { source },
            None,
            MailBody::new(Some("reply body".to_owned()), None)?,
        ))
    }

    #[test]
    fn wrong_client_source_never_reaches_provider_query() -> Result<(), Box<dyn std::error::Error>>
    {
        let allowed_client = ClientId::parse("client_01JC7SOURCE")?;
        let requested_client = ClientId::parse("client_02JC7SOURCE")?;
        let binding_id = MailboxBindingId::parse("binding_01JC7SOURCE")?;
        let eligibility = FakeEligibility {
            client_id: allowed_client,
            binding_id: binding_id.clone(),
        };
        let provider = FakeProvider {
            get_calls: Cell::new(0),
            dependency_failure: false,
        };
        let intent = reply_intent(requested_client.clone(), binding_id)?;
        let accessible = block_on(is_accessible(
            &actor()?,
            &requested_client,
            &FakeAuthorization,
            &eligibility,
            &provider,
            &intent,
        ))?;
        assert!(!accessible);
        assert_eq!(provider.get_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn source_dependency_failure_is_not_concealed_as_not_found()
    -> Result<(), Box<dyn std::error::Error>> {
        let client_id = ClientId::parse("client_01JC7SOURCE")?;
        let binding_id = MailboxBindingId::parse("binding_01JC7SOURCE")?;
        let eligibility = FakeEligibility {
            client_id: client_id.clone(),
            binding_id: binding_id.clone(),
        };
        let provider = FakeProvider {
            get_calls: Cell::new(0),
            dependency_failure: true,
        };
        let intent = reply_intent(client_id.clone(), binding_id)?;
        let result = block_on(is_accessible(
            &actor()?,
            &client_id,
            &FakeAuthorization,
            &eligibility,
            &provider,
            &intent,
        ));
        assert_eq!(result, Err(QueryApplicationError::DependencyUnavailable));
        assert_eq!(provider.get_calls.get(), 1);
        Ok(())
    }
}
