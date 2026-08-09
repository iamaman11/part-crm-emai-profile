use application_ports::query::{QueryPage, QueryPortError, QueryPortErrorClass};
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use profile_platform_primitives::{MailboxBindingId, TenantScope, UnixMillis};

const CLOUD_MESSAGE_REFERENCE: &str = "synthetic-cloud-message-1";
const CLOUD_BODY: &str = "Synthetic confidential cloud message body";

#[derive(Clone, Copy, Default)]
pub struct DeterministicFakeCloudMailQueryAdapter;

impl ClientMailProviderQueryPort for DeterministicFakeCloudMailQueryAdapter {
    async fn search_messages(
        &self,
        _scope: &TenantScope,
        binding_id: &MailboxBindingId,
        _request: &SearchClientMailboxMessagesRequest,
    ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
        let reference = MailboxMessageReference::new(binding_id.clone(), CLOUD_MESSAGE_REFERENCE)
            .map_err(|_| integrity_error())?;
        Ok(QueryPage::new(
            vec![MailMessageSummary::new(
                reference,
                Some("Synthetic cloud subject".to_owned()),
                Some("synthetic-cloud@example.invalid".to_owned()),
                UnixMillis::new(100),
            )],
            None,
        ))
    }

    async fn get_message(
        &self,
        _scope: &TenantScope,
        reference: &MailboxMessageReference,
    ) -> Result<Option<MailMessageBody>, QueryPortError> {
        if reference.provider_reference() != CLOUD_MESSAGE_REFERENCE {
            return Ok(None);
        }
        let summary = MailMessageSummary::new(
            reference.clone(),
            Some("Synthetic cloud subject".to_owned()),
            Some("synthetic-cloud@example.invalid".to_owned()),
            UnixMillis::new(100),
        );
        MailMessageBody::new(
            summary,
            Some(CLOUD_BODY.to_owned()),
            Some(format!("<p>{CLOUD_BODY}</p>")),
        )
        .map(Some)
        .map_err(|_| integrity_error())
    }
}

const fn integrity_error() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

#[cfg(test)]
mod tests {
    use super::{CLOUD_BODY, DeterministicFakeCloudMailQueryAdapter};
    use application_ports::query::{QueryPageRequest, QueryPageSize};
    use application_ports::query_mail_provider::{
        ClientMailProviderQueryPort, SearchClientMailboxMessagesRequest,
    };
    use profile_platform_primitives::{MailboxBindingId, TenantId, TenantScope};
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

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

    #[test]
    fn fake_cloud_adapter_returns_full_synthetic_body_without_persistence()
    -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DeterministicFakeCloudMailQueryAdapter;
        let scope = TenantScope::new(TenantId::parse("tenant_01JFAKECLOUD")?);
        let binding = MailboxBindingId::parse("binding_01JFAKECLOUD")?;
        let request = SearchClientMailboxMessagesRequest::new(
            None,
            QueryPageRequest::new(QueryPageSize::new(10)?, None),
        );
        let page = block_on(adapter.search_messages(&scope, &binding, &request))?;
        assert_eq!(page.items().len(), 1);
        let body = block_on(adapter.get_message(&scope, page.items()[0].reference()))?
            .ok_or("synthetic cloud body missing")?;
        assert_eq!(body.text_body(), Some(CLOUD_BODY));
        assert!(body.html_body().is_some());
        Ok(())
    }
}
