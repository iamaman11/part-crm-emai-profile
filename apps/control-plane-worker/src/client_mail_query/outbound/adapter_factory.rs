use super::adapters::ClientMailDispatchAdapters;
use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use cloudflare_adapters::gmail_outbound_mail::CloudflareGmailOutboundMailProvider;
use cloudflare_adapters::smtp_outbound_mail::CloudflareSmtpOutboundMailProvider;
use worker::d1::D1Database;
use worker::Env;

impl<'a> ClientMailDispatchAdapters<'a> {
    pub(super) const fn new(
        env: &'a Env,
        routing: D1Database,
        gmail: D1Database,
        smtp: D1Database,
    ) -> Self {
        Self {
            repository: D1MailboxRepository::new(routing),
            gmail: CloudflareGmailOutboundMailProvider::new(env, gmail),
            smtp: CloudflareSmtpOutboundMailProvider::new(env, smtp),
        }
    }
}
