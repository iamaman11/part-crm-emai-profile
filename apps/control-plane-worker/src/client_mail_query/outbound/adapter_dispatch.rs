use super::adapters::ClientMailDispatchAdapters;
use application_ports::outbound_mail::{
    OutboundMailIntent, OutboundMailProviderOutcome, OutboundMailProviderPort,
    OutboundMailProviderPortError,
};
use cloudflare_adapters::MailboxProvider;
use profile_platform_primitives::ActorContext;

impl OutboundMailProviderPort for ClientMailDispatchAdapters<'_> {
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
        if !binding.is_executable() {
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
