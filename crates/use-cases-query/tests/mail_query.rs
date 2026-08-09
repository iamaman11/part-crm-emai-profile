use application_ports::query::{
    QueryAuthorizationPort, QueryCapability, QueryPage, QueryPageRequest, QueryPageSize,
    QueryPortError,
};
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, ClientMailboxEligibilityPort, MailMessageBody, MailMessageSummary,
    MailboxMessageReference, SearchClientMailboxMessagesRequest,
};
use profile_platform_primitives::{
    ActorContext, ActorId, ClientId, CorrelationId, MailboxBindingId, TenantId, TenantScope,
    UnixMillis,
};
use std::cell::Cell;
use std::future::Future;
use std::task::{Context, Poll, Waker};
use use_cases_query::mail::{get_client_mailbox_message, search_client_mailbox_messages};

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

struct FakeAuthorization {
    allowed: bool,
    calls: Cell<u32>,
}

impl QueryAuthorizationPort for FakeAuthorization {
    async fn is_query_authorized(
        &self,
        _actor: &ActorContext,
        capability: QueryCapability,
    ) -> Result<bool, QueryPortError> {
        self.calls.set(self.calls.get() + 1);
        assert_eq!(capability, QueryCapability::Mail);
        Ok(self.allowed)
    }
}

struct FakeEligibility {
    allowed_binding: MailboxBindingId,
    calls: Cell<u32>,
}

impl ClientMailboxEligibilityPort for FakeEligibility {
    async fn is_mailbox_eligible(
        &self,
        _actor: &ActorContext,
        _client_id: &ClientId,
        binding_id: &MailboxBindingId,
    ) -> Result<bool, QueryPortError> {
        self.calls.set(self.calls.get() + 1);
        Ok(binding_id == &self.allowed_binding)
    }
}

struct FakeProvider {
    search_calls: Cell<u32>,
    get_calls: Cell<u32>,
}

impl ClientMailProviderQueryPort for FakeProvider {
    async fn search_messages(
        &self,
        _scope: &TenantScope,
        binding_id: &MailboxBindingId,
        _request: &SearchClientMailboxMessagesRequest,
    ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
        self.search_calls.set(self.search_calls.get() + 1);
        let reference = MailboxMessageReference::new(binding_id.clone(), "provider-message-1")
            .map_err(|_| {
                application_ports::query::QueryPortError::new(
                    application_ports::query::QueryPortErrorClass::IntegrityFailure,
                )
            })?;
        Ok(QueryPage::new(
            vec![MailMessageSummary::new(
                reference,
                Some("Synthetic subject".to_owned()),
                Some("synthetic@example.invalid".to_owned()),
                UnixMillis::new(10),
            )],
            None,
        ))
    }

    async fn get_message(
        &self,
        _scope: &TenantScope,
        reference: &MailboxMessageReference,
    ) -> Result<Option<MailMessageBody>, QueryPortError> {
        self.get_calls.set(self.get_calls.get() + 1);
        let summary = MailMessageSummary::new(
            reference.clone(),
            Some("Synthetic subject".to_owned()),
            Some("synthetic@example.invalid".to_owned()),
            UnixMillis::new(10),
        );
        MailMessageBody::new(
            summary,
            Some("Synthetic confidential body".to_owned()),
            Some("<p>Synthetic confidential body</p>".to_owned()),
        )
        .map(Some)
        .map_err(|_| {
            application_ports::query::QueryPortError::new(
                application_ports::query::QueryPortErrorClass::IntegrityFailure,
            )
        })
    }
}

fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
    Ok(ActorContext::new(
        TenantScope::new(TenantId::parse("tenant_01JMAILQUERY")?),
        ActorId::parse("actor_01JMAILQUERY")?,
        CorrelationId::parse("corr_01JMAILQUERY")?,
    ))
}

fn client() -> Result<ClientId, Box<dyn std::error::Error>> {
    Ok(ClientId::parse("client_01JMAILQUERY")?)
}

fn binding(value: &str) -> Result<MailboxBindingId, Box<dyn std::error::Error>> {
    Ok(MailboxBindingId::parse(value)?)
}

fn request() -> Result<SearchClientMailboxMessagesRequest, Box<dyn std::error::Error>> {
    Ok(SearchClientMailboxMessagesRequest::new(
        None,
        QueryPageRequest::new(QueryPageSize::new(25)?, None),
    ))
}

#[test]
fn authorization_denial_never_checks_eligibility_or_provider()
-> Result<(), Box<dyn std::error::Error>> {
    let allowed = binding("binding_01JMAILQUERY")?;
    let authorization = FakeAuthorization {
        allowed: false,
        calls: Cell::new(0),
    };
    let eligibility = FakeEligibility {
        allowed_binding: allowed.clone(),
        calls: Cell::new(0),
    };
    let provider = FakeProvider {
        search_calls: Cell::new(0),
        get_calls: Cell::new(0),
    };
    let result = block_on(search_client_mailbox_messages(
        &actor()?,
        &authorization,
        &eligibility,
        &provider,
        &client()?,
        &allowed,
        &request()?,
    ))?;
    assert!(result.items().is_empty());
    assert_eq!(authorization.calls.get(), 1);
    assert_eq!(eligibility.calls.get(), 0);
    assert_eq!(provider.search_calls.get(), 0);
    Ok(())
}

#[test]
fn ineligible_mailbox_never_reaches_provider() -> Result<(), Box<dyn std::error::Error>> {
    let allowed = binding("binding_01JMAILQUERY")?;
    let foreign = binding("binding_02JMAILQUERY")?;
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let eligibility = FakeEligibility {
        allowed_binding: allowed,
        calls: Cell::new(0),
    };
    let provider = FakeProvider {
        search_calls: Cell::new(0),
        get_calls: Cell::new(0),
    };
    let result = block_on(search_client_mailbox_messages(
        &actor()?,
        &authorization,
        &eligibility,
        &provider,
        &client()?,
        &foreign,
        &request()?,
    ))?;
    assert!(result.items().is_empty());
    assert_eq!(eligibility.calls.get(), 1);
    assert_eq!(provider.search_calls.get(), 0);
    Ok(())
}

#[test]
fn eligible_search_and_body_fetch_are_provider_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let allowed = binding("binding_01JMAILQUERY")?;
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let eligibility = FakeEligibility {
        allowed_binding: allowed.clone(),
        calls: Cell::new(0),
    };
    let provider = FakeProvider {
        search_calls: Cell::new(0),
        get_calls: Cell::new(0),
    };
    let page = block_on(search_client_mailbox_messages(
        &actor()?,
        &authorization,
        &eligibility,
        &provider,
        &client()?,
        &allowed,
        &request()?,
    ))?;
    assert_eq!(page.items().len(), 1);
    assert_eq!(provider.search_calls.get(), 1);

    let reference = page.items()[0].reference().clone();
    let body = block_on(get_client_mailbox_message(
        &actor()?,
        &authorization,
        &eligibility,
        &provider,
        &client()?,
        &reference,
    ))?
    .ok_or("synthetic body missing")?;
    assert_eq!(body.text_body(), Some("Synthetic confidential body"));
    assert_eq!(provider.get_calls.get(), 1);
    Ok(())
}

#[test]
fn foreign_message_reference_cannot_bypass_client_mailbox_eligibility()
-> Result<(), Box<dyn std::error::Error>> {
    let allowed = binding("binding_01JMAILQUERY")?;
    let foreign = binding("binding_02JMAILQUERY")?;
    let authorization = FakeAuthorization {
        allowed: true,
        calls: Cell::new(0),
    };
    let eligibility = FakeEligibility {
        allowed_binding: allowed,
        calls: Cell::new(0),
    };
    let provider = FakeProvider {
        search_calls: Cell::new(0),
        get_calls: Cell::new(0),
    };
    let reference = MailboxMessageReference::new(foreign, "provider-message-foreign")?;
    let body = block_on(get_client_mailbox_message(
        &actor()?,
        &authorization,
        &eligibility,
        &provider,
        &client()?,
        &reference,
    ))?;
    assert!(body.is_none());
    assert_eq!(eligibility.calls.get(), 1);
    assert_eq!(provider.get_calls.get(), 0);
    Ok(())
}
