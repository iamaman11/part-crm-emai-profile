use crate::MailboxError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxProvider {
    GmailApi,
    Imap,
    BrowserFallback,
}

impl MailboxProvider {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::GmailApi => "GMAIL_API",
            Self::Imap => "IMAP",
            Self::BrowserFallback => "BROWSER_FALLBACK",
        }
    }

    pub fn parse_storage(value: &str) -> Result<Self, MailboxError> {
        match value {
            "GMAIL_API" => Ok(Self::GmailApi),
            "IMAP" => Ok(Self::Imap),
            "BROWSER_FALLBACK" => Ok(Self::BrowserFallback),
            _ => Err(MailboxError::InvalidProvider),
        }
    }
}
