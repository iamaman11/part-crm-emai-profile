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
    MicrosoftGraph,
}

impl MailboxProvider {
    #[must_use]
    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::GmailApi => "GMAIL_API",
            Self::Imap => "IMAP",
            Self::BrowserFallback => "BROWSER_FALLBACK",
            Self::MicrosoftGraph => "MICROSOFT_GRAPH",
        }
    }

    pub fn parse_storage(value: &str) -> Result<Self, MailboxError> {
        match value {
            "GMAIL_API" => Ok(Self::GmailApi),
            "IMAP" => Ok(Self::Imap),
            "BROWSER_FALLBACK" => Ok(Self::BrowserFallback),
            "MICROSOFT_GRAPH" => Ok(Self::MicrosoftGraph),
            _ => Err(MailboxError::InvalidProvider),
        }
    }

    #[must_use]
    pub const fn runtime_lane(self) -> MailboxRuntimeLane {
        match self {
            Self::GmailApi | Self::Imap | Self::MicrosoftGraph => MailboxRuntimeLane::Cloud,
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
    fn runtime_lanes_preserve_browser_and_phase2e_boundaries() {
        assert_eq!(
            MailboxProvider::GmailApi.runtime_lane(),
            MailboxRuntimeLane::Cloud
        );
        assert_eq!(
            MailboxProvider::Imap.runtime_lane(),
            MailboxRuntimeLane::Cloud
        );
        assert_eq!(
            MailboxProvider::MicrosoftGraph.runtime_lane(),
            MailboxRuntimeLane::Cloud
        );
        assert_eq!(
            MailboxProvider::BrowserFallback.runtime_lane(),
            MailboxRuntimeLane::Browser
        );
        assert!(MailboxProvider::GmailApi.is_phase2e_cloud_supported());
        assert!(MailboxProvider::Imap.is_phase2e_cloud_supported());
        assert!(!MailboxProvider::MicrosoftGraph.is_phase2e_cloud_supported());
        assert!(!MailboxProvider::BrowserFallback.is_phase2e_cloud_supported());
    }

    #[test]
    fn microsoft_graph_has_a_distinct_durable_discriminator() {
        assert_eq!(
            MailboxProvider::MicrosoftGraph.storage_value(),
            "MICROSOFT_GRAPH"
        );
        assert_eq!(
            MailboxProvider::parse_storage("MICROSOFT_GRAPH"),
            Ok(MailboxProvider::MicrosoftGraph)
        );
        assert_ne!(MailboxProvider::MicrosoftGraph, MailboxProvider::Imap);
    }
}
