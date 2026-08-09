use crate::MailboxError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxRuntimeLane {
    Cloud,
    Browser,
}

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

    #[must_use]
    pub const fn runtime_lane(self) -> MailboxRuntimeLane {
        match self {
            Self::GmailApi | Self::Imap => MailboxRuntimeLane::Cloud,
            Self::BrowserFallback => MailboxRuntimeLane::Browser,
        }
    }

    #[must_use]
    pub const fn is_phase2e_cloud_supported(self) -> bool {
        matches!(self, Self::GmailApi | Self::Imap)
    }
}

#[cfg(test)]
mod tests {
    use super::{MailboxProvider, MailboxRuntimeLane};

    #[test]
    fn only_approved_phase2e_providers_are_cloud_lane() {
        assert_eq!(MailboxProvider::GmailApi.runtime_lane(), MailboxRuntimeLane::Cloud);
        assert_eq!(MailboxProvider::Imap.runtime_lane(), MailboxRuntimeLane::Cloud);
        assert_eq!(
            MailboxProvider::BrowserFallback.runtime_lane(),
            MailboxRuntimeLane::Browser
        );
        assert!(MailboxProvider::GmailApi.is_phase2e_cloud_supported());
        assert!(MailboxProvider::Imap.is_phase2e_cloud_supported());
        assert!(!MailboxProvider::BrowserFallback.is_phase2e_cloud_supported());
    }
}
