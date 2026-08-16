use application_ports::outbound_mail::{
    OutboundMailIntent, OutboundMailProviderOutcome, OutboundMailProviderPort,
    OutboundMailProviderPortError,
};
use cloudflare_adapters::MailboxProvider;
use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use cloudflare_adapters::d1_outbound_mail_intents::D1OutboundMailIntentRepository;
use cloudflare_adapters::gmail_outbound_mail::CloudflareGmailOutboundMailProvider;
use cloudflare_adapters::smtp_outbound_mail::CloudflareSmtpOutboundMailProvider;
use control_plane_contract::D1_CATALOG_BINDING;
use profile_platform_primitives::{ActorContext, MailboxBindingId};
use worker::{Env, Result};

pub enum ClientMailOutboundProvider<'a> {
    Gmail(CloudflareGmailOutboundMailProvider<'a>),
    Smtp(CloudflareSmtpOutboundMailProvider<'a>),
    Unsupported,
}

impl OutboundMailProviderPort for ClientMailOutboundProvider<'_> {
    async fn send(
        &self,
        actor: &ActorContext,
        intent: &OutboundMailIntent,
    ) -> std::result::Result<OutboundMailProviderOutcome, OutboundMailProviderPortError> {
        match self {
            Self::Gmail(provider) => provider.send(actor, intent).await,
            Self::Smtp(provider) => provider.send(actor, intent).await,
            Self::Unsupported => Ok(OutboundMailProviderOutcome::Rejected),
        }
    }
}

pub fn outbound_mail_intent_repository(env: &Env) -> Result<D1OutboundMailIntentRepository> {
    Ok(D1OutboundMailIntentRepository::new(
        env.d1(D1_CATALOG_BINDING)?,
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

pub async fn client_mail_outbound_provider<'a>(
    env: &'a Env,
    actor: &ActorContext,
    binding_id: &MailboxBindingId,
) -> Result<Option<ClientMailOutboundProvider<'a>>> {
    let repository = D1MailboxRepository::new(env.d1(D1_CATALOG_BINDING)?);
    let Some(binding) = repository
        .find_binding(actor.tenant_scope(), binding_id)
        .await?
    else {
        return Ok(None);
    };

    let provider = match binding.provider() {
        MailboxProvider::GmailApi => ClientMailOutboundProvider::Gmail(
            CloudflareGmailOutboundMailProvider::new(env, env.d1(D1_CATALOG_BINDING)?),
        ),
        MailboxProvider::Imap => ClientMailOutboundProvider::Smtp(
            CloudflareSmtpOutboundMailProvider::new(env, env.d1(D1_CATALOG_BINDING)?),
        ),
        MailboxProvider::BrowserFallback | MailboxProvider::MicrosoftGraph => {
            ClientMailOutboundProvider::Unsupported
        }
    };

    Ok(Some(provider))
}
