use crate::query::{QueryPage, QueryPageRequest, QueryPortError};
use core::{fmt, future::Future};
use profile_platform_primitives::{
    ActorContext, ClientId, MailboxBindingId, TenantScope, UnixMillis,
};

const MAX_PROVIDER_MESSAGE_REFERENCE_LENGTH: usize = 512;
const MAX_MAIL_SEARCH_TERM_LENGTH: usize = 200;
pub const MAX_MAIL_BODY_BYTES: usize = 1_048_576;

#[derive(Clone, Eq, PartialEq)]
pub struct MailSearchTerm(String);

impl MailSearchTerm {
    pub fn parse(value: impl Into<String>) -> Result<Self, MailQueryInputError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty()
            || value.len() > MAX_MAIL_SEARCH_TERM_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(MailQueryInputError::InvalidSearchTerm);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MailboxMessageReference {
    binding_id: MailboxBindingId,
    provider_reference: String,
}

impl MailboxMessageReference {
    pub fn new(
        binding_id: MailboxBindingId,
        provider_reference: impl Into<String>,
    ) -> Result<Self, MailQueryInputError> {
        let provider_reference = provider_reference.into();
        if provider_reference.is_empty()
            || provider_reference.len() > MAX_PROVIDER_MESSAGE_REFERENCE_LENGTH
            || provider_reference.chars().any(char::is_control)
        {
            return Err(MailQueryInputError::InvalidMessageReference);
        }
        Ok(Self {
            binding_id,
            provider_reference,
        })
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub fn provider_reference(&self) -> &str {
        &self.provider_reference
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MailMessageSummary {
    reference: MailboxMessageReference,
    subject: Option<String>,
    sender: Option<String>,
    received_at: UnixMillis,
}

impl MailMessageSummary {
    #[must_use]
    pub const fn new(
        reference: MailboxMessageReference,
        subject: Option<String>,
        sender: Option<String>,
        received_at: UnixMillis,
    ) -> Self {
        Self {
            reference,
            subject,
            sender,
            received_at,
        }
    }

    #[must_use]
    pub const fn reference(&self) -> &MailboxMessageReference {
        &self.reference
    }

    #[must_use]
    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    #[must_use]
    pub fn sender(&self) -> Option<&str> {
        self.sender.as_deref()
    }

    #[must_use]
    pub const fn received_at(&self) -> UnixMillis {
        self.received_at
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MailMessageBody {
    summary: MailMessageSummary,
    text_body: Option<String>,
    html_body: Option<String>,
}

impl MailMessageBody {
    pub fn new(
        summary: MailMessageSummary,
        text_body: Option<String>,
        html_body: Option<String>,
    ) -> Result<Self, MailQueryInputError> {
        let total = text_body.as_ref().map_or(0, String::len)
            + html_body.as_ref().map_or(0, String::len);
        if total > MAX_MAIL_BODY_BYTES {
            return Err(MailQueryInputError::MessageBodyTooLarge);
        }
        Ok(Self {
            summary,
            text_body,
            html_body,
        })
    }

    #[must_use]
    pub const fn summary(&self) -> &MailMessageSummary {
        &self.summary
    }

    #[must_use]
    pub fn text_body(&self) -> Option<&str> {
        self.text_body.as_deref()
    }

    #[must_use]
    pub fn html_body(&self) -> Option<&str> {
        self.html_body.as_deref()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct SearchClientMailboxMessagesRequest {
    term: Option<MailSearchTerm>,
    page: QueryPageRequest,
}

impl SearchClientMailboxMessagesRequest {
    #[must_use]
    pub const fn new(term: Option<MailSearchTerm>, page: QueryPageRequest) -> Self {
        Self { term, page }
    }

    #[must_use]
    pub const fn term(&self) -> Option<&MailSearchTerm> {
        self.term.as_ref()
    }

    #[must_use]
    pub const fn page(&self) -> &QueryPageRequest {
        &self.page
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailQueryInputError {
    InvalidSearchTerm,
    InvalidMessageReference,
    MessageBodyTooLarge,
}

impl fmt::Display for MailQueryInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSearchTerm => "mail search term is invalid",
            Self::InvalidMessageReference => "mail message reference is invalid",
            Self::MessageBodyTooLarge => "mail message body exceeds the accepted bound",
        })
    }
}

impl std::error::Error for MailQueryInputError {}

pub trait ClientMailboxEligibilityPort {
    fn is_mailbox_eligible(
        &self,
        actor: &ActorContext,
        client_id: &ClientId,
        binding_id: &MailboxBindingId,
    ) -> impl Future<Output = Result<bool, QueryPortError>>;
}

pub trait ClientMailProviderQueryPort {
    fn search_messages(
        &self,
        scope: &TenantScope,
        binding_id: &MailboxBindingId,
        request: &SearchClientMailboxMessagesRequest,
    ) -> impl Future<Output = Result<QueryPage<MailMessageSummary>, QueryPortError>>;

    fn get_message(
        &self,
        scope: &TenantScope,
        reference: &MailboxMessageReference,
    ) -> impl Future<Output = Result<Option<MailMessageBody>, QueryPortError>>;
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_MAIL_BODY_BYTES, MailMessageBody, MailMessageSummary, MailQueryInputError,
        MailSearchTerm, MailboxMessageReference,
    };
    use profile_platform_primitives::{MailboxBindingId, UnixMillis};

    #[test]
    fn transient_mail_inputs_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(
            MailSearchTerm::parse("bad\nterm"),
            Err(MailQueryInputError::InvalidSearchTerm)
        ));
        let reference = MailboxMessageReference::new(
            MailboxBindingId::parse("binding_01JMAILQUERY")?,
            "provider-message-1",
        )?;
        let summary = MailMessageSummary::new(reference, None, None, UnixMillis::new(1));
        assert!(matches!(
            MailMessageBody::new(summary, Some("x".repeat(MAX_MAIL_BODY_BYTES + 1)), None),
            Err(MailQueryInputError::MessageBodyTooLarge)
        ));
        Ok(())
    }
}
