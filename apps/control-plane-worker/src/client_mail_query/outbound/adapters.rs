use cloudflare_adapters::d1_mailboxes::D1MailboxRepository;
use cloudflare_adapters::gmail_outbound_mail::CloudflareGmailOutboundMailProvider;
use cloudflare_adapters::smtp_outbound_mail::CloudflareSmtpOutboundMailProvider;
use worker::d1::D1Database;
use worker::Env;

pub(super) struct ClientMailDispatchAdapters<'a> {
    pub(super) repository: D1MailboxRepository,
    pub(super) gmail: CloudflareGmailOutboundMailProvider<'a>,
    pub(super) smtp: CloudflareSmtpOutboundMailProvider<'a>,
}
