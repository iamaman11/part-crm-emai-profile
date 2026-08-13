mod mime;
mod source;

use crate::d1_mailboxes::D1MailboxRepository;
use crate::smtp_send_credential::{SmtpCredentialError, resolve_smtp_send_credential};
use crate::smtp_session::{SmtpSendFailure, SmtpSession};
use application_ports::outbound_mail::{
    MailAddress, OutboundMailIntent, OutboundMailOperation, OutboundMailProviderOutcome,
    OutboundMailProviderPort, OutboundMailProviderPortError,
};
use mailbox_domain::MailboxProvider;
use mime::{RenderContext, render_mime};
use profile_platform_primitives::ActorContext;
use source::PreparationFailure;
use worker::Env;
use worker::d1::D1Database;

pub struct CloudflareSmtpOutboundMailProvider<'a> {
    env: &'a Env,
    repository: D1MailboxRepository,
}

impl<'a> CloudflareSmtpOutboundMailProvider<'a> {
    #[must_use]
    pub const fn new(env: &'a Env, database: D1Database) -> Self {
        Self {
            env,
            repository: D1MailboxRepository::new(database),
        }
    }
}

impl OutboundMailProviderPort for CloudflareSmtpOutboundMailProvider<'_> {
    async fn send(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
    ) -> Result<OutboundMailProviderOutcome, OutboundMailProviderPortError> {
        let binding = match self
            .repository
            .find_binding(actor.tenant_scope(), intent.binding_id())
            .await
        {
            Ok(Some(binding)) => binding,
            Ok(None) => return Ok(OutboundMailProviderOutcome::Rejected),
            Err(_) => return Ok(OutboundMailProviderOutcome::RetryableNotSent),
        };
        if binding.binding_id() != intent.binding_id()
            || binding.provider() != MailboxProvider::Imap
            || !binding.is_executable()
        {
            return Ok(OutboundMailProviderOutcome::Rejected);
        }

        let credential = match resolve_smtp_send_credential(self.env, &binding).await {
            Ok(credential) => credential,
            Err(error) => return Ok(credential_failure_outcome(error)),
        };
        let sender = match MailAddress::parse(credential.username().to_owned()) {
            Ok(sender) => sender,
            Err(_) => return Ok(OutboundMailProviderOutcome::Rejected),
        };
        let source = match source::resolve_source_context(
            self.env,
            &binding,
            intent,
            sender.as_str(),
        )
        .await
        {
            Ok(source) => source,
            Err(error) => return Ok(preparation_failure_outcome(error)),
        };
        let recipients = match (&source, intent.operation()) {
            (Some(source), _) => &source.recipients,
            (None, OutboundMailOperation::New { recipients })
            | (None, OutboundMailOperation::Forward { recipients, .. }) => recipients,
            (None, OutboundMailOperation::Reply { .. })
            | (None, OutboundMailOperation::ReplyAll { .. }) => {
                return Ok(OutboundMailProviderOutcome::Rejected);
            }
        };
        let context = RenderContext {
            sender: sender.as_str(),
            recipients,
            subject: intent.subject(),
            fallback_subject: source
                .as_ref()
                .and_then(|value| value.fallback_subject.as_deref()),
            in_reply_to: source
                .as_ref()
                .and_then(|value| value.in_reply_to.as_deref()),
            references: source
                .as_ref()
                .and_then(|value| value.references.as_deref()),
        };
        let rendered = match render_mime(&context, intent.body()) {
            Ok(rendered) => rendered,
            Err(()) => return Ok(OutboundMailProviderOutcome::Rejected),
        };
        let mut session = match SmtpSession::connect(&credential).await {
            Ok(session) => session,
            Err(error) => return Ok(transport_failure_outcome(error)),
        };
        match session
            .send_message(
                sender.as_str(),
                &rendered.envelope_recipients,
                &rendered.bytes,
            )
            .await
        {
            Ok(()) => Ok(OutboundMailProviderOutcome::Sent {
                provider_message_reference: None,
            }),
            Err(error) => Ok(transport_failure_outcome(error)),
        }
    }
}

const fn credential_failure_outcome(error: SmtpCredentialError) -> OutboundMailProviderOutcome {
    match error {
        SmtpCredentialError::RetryableNotSent => OutboundMailProviderOutcome::RetryableNotSent,
        SmtpCredentialError::Rejected | SmtpCredentialError::IntegrityFailure => {
            OutboundMailProviderOutcome::Rejected
        }
    }
}

const fn preparation_failure_outcome(error: PreparationFailure) -> OutboundMailProviderOutcome {
    match error {
        PreparationFailure::RetryableNotSent => OutboundMailProviderOutcome::RetryableNotSent,
        PreparationFailure::Rejected => OutboundMailProviderOutcome::Rejected,
    }
}

const fn transport_failure_outcome(error: SmtpSendFailure) -> OutboundMailProviderOutcome {
    match error {
        SmtpSendFailure::RetryableNotSent => OutboundMailProviderOutcome::RetryableNotSent,
        SmtpSendFailure::Rejected | SmtpSendFailure::IntegrityFailure => {
            OutboundMailProviderOutcome::Rejected
        }
        SmtpSendFailure::Ambiguous => OutboundMailProviderOutcome::Ambiguous,
    }
}

#[cfg(test)]
mod tests {
    use super::transport_failure_outcome;
    use crate::smtp_session::SmtpSendFailure;
    use application_ports::outbound_mail::OutboundMailProviderOutcome;

    #[test]
    fn adapter_preserves_c4_outcome_taxonomy() {
        assert_eq!(
            transport_failure_outcome(SmtpSendFailure::RetryableNotSent),
            OutboundMailProviderOutcome::RetryableNotSent
        );
        assert_eq!(
            transport_failure_outcome(SmtpSendFailure::Rejected),
            OutboundMailProviderOutcome::Rejected
        );
        assert_eq!(
            transport_failure_outcome(SmtpSendFailure::Ambiguous),
            OutboundMailProviderOutcome::Ambiguous
        );
    }
}