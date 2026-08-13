use crate::CommandExecutionEvidence;
use core::fmt;
use profile_platform_primitives::{ActorContext, ClientId, MailboxBindingId, OutboxEventId};

const MAX_MAIL_ADDRESS_BYTES: usize = 320;
const MAX_MAIL_RECIPIENTS: usize = 100;
const MAX_MAIL_SUBJECT_BYTES: usize = 998;
pub const MAX_OUTBOUND_MAIL_BODY_BYTES: usize = 1_048_576;
const MAX_PROVIDER_MESSAGE_REFERENCE_BYTES: usize = 512;

#[derive(Clone, Eq, PartialEq)]
pub struct MailAddress(String);

impl MailAddress {
    pub fn parse(value: impl Into<String>) -> Result<Self, OutboundMailInputError> {
        let value = value.into();
        let value = value.trim();
        let valid_bounds = (3..=MAX_MAIL_ADDRESS_BYTES).contains(&value.len());
        let valid_chars = !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace());
        let valid_shape = value
            .rsplit_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && !domain.is_empty());

        if !valid_bounds || !valid_chars || !valid_shape {
            return Err(OutboundMailInputError::InvalidAddress);
        }

        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MailRecipients {
    to: Vec<MailAddress>,
    cc: Vec<MailAddress>,
    bcc: Vec<MailAddress>,
}

impl MailRecipients {
    pub fn new(
        to: Vec<MailAddress>,
        cc: Vec<MailAddress>,
        bcc: Vec<MailAddress>,
    ) -> Result<Self, OutboundMailInputError> {
        let total = to.len().saturating_add(cc.len()).saturating_add(bcc.len());
        if total == 0 {
            return Err(OutboundMailInputError::RecipientsRequired);
        }
        if total > MAX_MAIL_RECIPIENTS {
            return Err(OutboundMailInputError::RecipientLimitExceeded);
        }
        Ok(Self { to, cc, bcc })
    }

    #[must_use]
    pub fn to(&self) -> &[MailAddress] {
        &self.to
    }

    #[must_use]
    pub fn cc(&self) -> &[MailAddress] {
        &self.cc
    }

    #[must_use]
    pub fn bcc(&self) -> &[MailAddress] {
        &self.bcc
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MailSubject(String);

impl MailSubject {
    pub fn parse(value: impl Into<String>) -> Result<Self, OutboundMailInputError> {
        let value = value.into();
        if value.len() > MAX_MAIL_SUBJECT_BYTES || value.chars().any(char::is_control) {
            return Err(OutboundMailInputError::InvalidSubject);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MailBody {
    text: Option<String>,
    html: Option<String>,
}

impl MailBody {
    pub fn new(text: Option<String>, html: Option<String>) -> Result<Self, OutboundMailInputError> {
        let has_text = text.as_ref().is_some_and(|value| !value.is_empty());
        let has_html = html.as_ref().is_some_and(|value| !value.is_empty());
        if !has_text && !has_html {
            return Err(OutboundMailInputError::BodyRequired);
        }

        let text_len = text.as_ref().map_or(0, String::len);
        let html_len = html.as_ref().map_or(0, String::len);
        let total = text_len.saturating_add(html_len);
        if total > MAX_OUTBOUND_MAIL_BODY_BYTES {
            return Err(OutboundMailInputError::BodyTooLarge);
        }

        Ok(Self { text, html })
    }

    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    #[must_use]
    pub fn html(&self) -> Option<&str> {
        self.html.as_deref()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProviderMessageReference(String);

impl ProviderMessageReference {
    pub fn parse(value: impl Into<String>) -> Result<Self, OutboundMailInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_PROVIDER_MESSAGE_REFERENCE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(OutboundMailInputError::InvalidMessageReference);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct OutboundMailSourceReference {
    binding_id: MailboxBindingId,
    provider_reference: ProviderMessageReference,
}

impl OutboundMailSourceReference {
    #[must_use]
    pub const fn new(
        binding_id: MailboxBindingId,
        provider_reference: ProviderMessageReference,
    ) -> Self {
        Self {
            binding_id,
            provider_reference,
        }
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn provider_reference(&self) -> &ProviderMessageReference {
        &self.provider_reference
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum OutboundMailOperation {
    New {
        recipients: MailRecipients,
    },
    Reply {
        source: OutboundMailSourceReference,
    },
    ReplyAll {
        source: OutboundMailSourceReference,
    },
    Forward {
        source: OutboundMailSourceReference,
        recipients: MailRecipients,
    },
}

impl OutboundMailOperation {
    #[must_use]
    pub const fn kind_code(&self) -> &'static str {
        match self {
            Self::New { .. } => "NEW",
            Self::Reply { .. } => "REPLY",
            Self::ReplyAll { .. } => "REPLY_ALL",
            Self::Forward { .. } => "FORWARD",
        }
    }

    #[must_use]
    pub const fn source(&self) -> Option<&OutboundMailSourceReference> {
        match self {
            Self::New { .. } => None,
            Self::Reply { source } | Self::ReplyAll { source } | Self::Forward { source, .. } => {
                Some(source)
            }
        }
    }

    #[must_use]
    pub const fn recipients(&self) -> Option<&MailRecipients> {
        match self {
            Self::New { recipients } | Self::Forward { recipients, .. } => Some(recipients),
            Self::Reply { .. } | Self::ReplyAll { .. } => None,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct OutboundMailIntent {
    client_id: ClientId,
    binding_id: MailboxBindingId,
    operation: OutboundMailOperation,
    subject: Option<MailSubject>,
    body: MailBody,
}

impl OutboundMailIntent {
    #[must_use]
    pub const fn new(
        client_id: ClientId,
        binding_id: MailboxBindingId,
        operation: OutboundMailOperation,
        subject: Option<MailSubject>,
        body: MailBody,
    ) -> Self {
        Self {
            client_id,
            binding_id,
            operation,
            subject,
            body,
        }
    }

    #[must_use]
    pub const fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    #[must_use]
    pub const fn binding_id(&self) -> &MailboxBindingId {
        &self.binding_id
    }

    #[must_use]
    pub const fn operation(&self) -> &OutboundMailOperation {
        &self.operation
    }

    #[must_use]
    pub const fn subject(&self) -> Option<&MailSubject> {
        self.subject.as_ref()
    }

    #[must_use]
    pub const fn body(&self) -> &MailBody {
        &self.body
    }

    pub fn validate_source_binding(&self) -> Result<(), OutboundMailInputError> {
        if self
            .operation
            .source()
            .is_some_and(|source| source.binding_id() != &self.binding_id)
        {
            Err(OutboundMailInputError::SourceBindingMismatch)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundMailInputError {
    InvalidAddress,
    RecipientsRequired,
    RecipientLimitExceeded,
    InvalidSubject,
    BodyRequired,
    BodyTooLarge,
    InvalidMessageReference,
    SourceBindingMismatch,
}

impl fmt::Display for OutboundMailInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAddress => "mail address is invalid",
            Self::RecipientsRequired => "at least one mail recipient is required",
            Self::RecipientLimitExceeded => "mail recipient limit exceeded",
            Self::InvalidSubject => "mail subject is invalid",
            Self::BodyRequired => "mail body is required",
            Self::BodyTooLarge => "mail body exceeds the accepted bound",
            Self::InvalidMessageReference => "mail message reference is invalid",
            Self::SourceBindingMismatch => {
                "source message reference belongs to a different mailbox binding"
            }
        })
    }
}

impl std::error::Error for OutboundMailInputError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundMailIntentState {
    Pending,
    Dispatching,
    Retryable,
    Sent,
    Ambiguous,
    Rejected,
}

#[derive(Clone, Eq, PartialEq)]
pub struct OutboundMailIntentReceipt {
    intent_id: OutboxEventId,
    state: OutboundMailIntentState,
    attempt_count: u8,
    provider_message_reference: Option<ProviderMessageReference>,
}

impl OutboundMailIntentReceipt {
    #[must_use]
    pub const fn new(
        intent_id: OutboxEventId,
        state: OutboundMailIntentState,
        attempt_count: u8,
        provider_message_reference: Option<ProviderMessageReference>,
    ) -> Self {
        Self {
            intent_id,
            state,
            attempt_count,
            provider_message_reference,
        }
    }

    #[must_use]
    pub const fn intent_id(&self) -> &OutboxEventId {
        &self.intent_id
    }

    #[must_use]
    pub const fn state(&self) -> OutboundMailIntentState {
        self.state
    }

    #[must_use]
    pub const fn attempt_count(&self) -> u8 {
        self.attempt_count
    }

    #[must_use]
    pub const fn provider_message_reference(&self) -> Option<&ProviderMessageReference> {
        self.provider_message_reference.as_ref()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum OutboundMailReserveDecision {
    Reserved,
    Existing(OutboundMailIntentReceipt),
    Conflict,
}

#[derive(Clone, Eq, PartialEq)]
pub enum OutboundMailClaimDecision {
    Claimed { attempt: u8 },
    Existing(OutboundMailIntentReceipt),
}

#[derive(Clone, Eq, PartialEq)]
pub enum OutboundMailProviderOutcome {
    Sent {
        provider_message_reference: Option<ProviderMessageReference>,
    },
    RetryableNotSent,
    Rejected,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundMailIntentPortErrorClass {
    NotFound,
    Conflict,
    IntegrityFailure,
    InternalFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutboundMailIntentPortError {
    class: OutboundMailIntentPortErrorClass,
}

impl OutboundMailIntentPortError {
    #[must_use]
    pub const fn new(class: OutboundMailIntentPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> OutboundMailIntentPortErrorClass {
        self.class
    }
}

impl fmt::Display for OutboundMailIntentPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("outbound mail intent persistence failure")
    }
}

impl std::error::Error for OutboundMailIntentPortError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundMailProviderPortErrorClass {
    DependencyUnavailable,
    InternalFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutboundMailProviderPortError {
    class: OutboundMailProviderPortErrorClass,
}

impl OutboundMailProviderPortError {
    #[must_use]
    pub const fn new(class: OutboundMailProviderPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> OutboundMailProviderPortErrorClass {
        self.class
    }
}

impl fmt::Display for OutboundMailProviderPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("outbound mail provider failure")
    }
}

impl std::error::Error for OutboundMailProviderPortError {}

#[allow(async_fn_in_trait)]
pub trait OutboundMailIntentApplicationPort {
    async fn reserve_intent(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailReserveDecision, OutboundMailIntentPortError>;

    async fn claim_dispatch(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
        max_attempts: u8,
    ) -> Result<OutboundMailClaimDecision, OutboundMailIntentPortError>;

    async fn complete_dispatch(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
        outcome: &OutboundMailProviderOutcome,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError>;

    async fn mark_ambiguous(
        &self,
        actor: &ActorContext,
        evidence: &CommandExecutionEvidence,
    ) -> Result<OutboundMailIntentReceipt, OutboundMailIntentPortError>;
}

#[allow(async_fn_in_trait)]
pub trait OutboundMailProviderPort {
    /// Adapters must return `Ambiguous` rather than an ordinary error once provider
    /// acceptance can no longer be ruled out.
    async fn send(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
    ) -> Result<OutboundMailProviderOutcome, OutboundMailProviderPortError>;
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_OUTBOUND_MAIL_BODY_BYTES, MailAddress, MailBody, MailRecipients,
        OutboundMailInputError, OutboundMailIntent, OutboundMailOperation,
        OutboundMailSourceReference, ProviderMessageReference,
    };
    use profile_platform_primitives::{ClientId, MailboxBindingId};

    #[test]
    fn content_inputs_are_bounded_without_debug_surfaces() -> Result<(), Box<dyn std::error::Error>>
    {
        assert!(matches!(
            MailAddress::parse("bad address@example.com"),
            Err(OutboundMailInputError::InvalidAddress)
        ));
        assert!(matches!(
            MailBody::new(Some("x".repeat(MAX_OUTBOUND_MAIL_BODY_BYTES + 1)), None),
            Err(OutboundMailInputError::BodyTooLarge)
        ));
        let recipients = MailRecipients::new(
            vec![MailAddress::parse("client@example.com")?],
            Vec::new(),
            Vec::new(),
        )?;
        assert_eq!(recipients.to()[0].as_str(), "client@example.com");
        Ok(())
    }

    #[test]
    fn source_reference_must_match_selected_mailbox() -> Result<(), Box<dyn std::error::Error>> {
        let selected = MailboxBindingId::parse("binding_selected")?;
        let source = OutboundMailSourceReference::new(
            MailboxBindingId::parse("binding_other")?,
            ProviderMessageReference::parse("provider-message-1")?,
        );
        let intent = OutboundMailIntent::new(
            ClientId::parse("client_01JOUTBOUND")?,
            selected,
            OutboundMailOperation::Reply { source },
            None,
            MailBody::new(Some("reply".to_owned()), None)?,
        );
        assert!(matches!(
            intent.validate_source_binding(),
            Err(OutboundMailInputError::SourceBindingMismatch)
        ));
        Ok(())
    }
}
