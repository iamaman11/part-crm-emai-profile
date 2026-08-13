use crate::d1_mailboxes::D1MailboxRepository;
use crate::gmail_outbound_mail::CloudflareGmailOutboundMailProvider;
use crate::smtp_outbound_mail::CloudflareSmtpOutboundMailProvider;
use application_ports::outbound_mail::{
    OutboundMailIntent, OutboundMailProviderOutcome, OutboundMailProviderPort,
    OutboundMailProviderPortError,
};
use mailbox_domain::MailboxProvider;
use profile_platform_primitives::ActorContext;
use worker::d1::D1Database;
use worker::Env;

pub struct CloudflareOutboundMailProvider<'a> {
    repository: D1MailboxRepository,
    gmail: CloudflareGmailOutboundMailProvider<'a>,
    smtp: CloudflareSmtpOutboundMailProvider<'a>,
}

impl<'a> CloudflareOutboundMailProvider<'a> {
    #[must_use]
    pub const fn new(
        env: &'a Env,
        routing_database: D1Database,
        gmail_database: D1Database,
        smtp_database: D1Database,
    ) -> Self {
        Self {
            repository: D1MailboxRepository::new(routing_database),
            gmail: CloudflareGmailOutboundMailProvider::new(env, gmail_database),
            smtp: CloudflareSmtpOutboundMailProvider::new(env, smtp_database),
        }
    }
}

impl OutboundMailProviderPort for CloudflareOutboundMailProvider<'_> {
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
        if binding.binding_id() != intent.binding_id() || !binding.is_executable() {
            return Ok(OutboundMailProviderOutcome::Rejected);
        }
        match binding.provider() {
            MailboxProvider::GmailApi => self.gmail.send(actor, intent).await,
            MailboxProvider::Imap => self.smtp.send(actor, intent).await,
            MailboxProvider::BrowserFallback | MailboxProvider::MicrosoftGraph => {
                Ok(OutboundMailProviderOutcome::Rejected)
            }
        }
    }
}
