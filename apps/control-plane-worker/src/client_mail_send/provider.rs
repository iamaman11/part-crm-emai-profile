use application_ports::outbound_mail::{
    OutboundMailIntent, OutboundMailProviderOutcome, OutboundMailProviderPort,
    OutboundMailProviderPortError,
};
use cloudflare_adapters::gmail_outbound_mail::CloudflareGmailOutboundMailProvider;
use cloudflare_adapters::smtp_outbound_mail::CloudflareSmtpOutboundMailProvider;
use profile_platform_primitives::ActorContext;

pub(super) enum ClientMailProvider<'a> {
    Gmail(CloudflareGmailOutboundMailProvider<'a>),
    Smtp(CloudflareSmtpOutboundMailProvider<'a>),
    Unsupported,
}

impl OutboundMailProviderPort for ClientMailProvider<'_> {
    async fn send(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
    ) -> Result<OutboundMailProviderOutcome, OutboundMailProviderPortError> {
        match self {
            Self::Gmail(provider) => provider.send(actor, intent).await,
            Self::Smtp(provider) => provider.send(actor, intent).await,
            Self::Unsupported => Ok(OutboundMailProviderOutcome::Rejected),
        }
    }
}
