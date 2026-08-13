use crate::d1_mailboxes::D1MailboxRepository;
use crate::gmail_outbound_mail::CloudflareGmailOutboundMailProvider;
use crate::smtp_outbound_mail::CloudflareSmtpOutboundMailProvider;
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
