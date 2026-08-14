use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const MAX_REQUEST_BYTES: usize = 32 * 1024;
pub const MAX_SECRET_DOCUMENT_BYTES: usize = 16 * 1024;
pub const SIGNATURE_VERSION: &str = "hmac-sha256-v1";
pub const MAX_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
pub const NONCE_BYTES: usize = 16;
pub const AES_GCM_NONCE_BYTES: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolverRoute {
    Discard,
    GmailOAuthComplete,
    GmailOAuthDeny,
    GmailOAuthInspect,
    GmailOAuthStart,
    GmailSendOAuthComplete,
    GmailSendOAuthDeny,
    GmailSendOAuthInspect,
    GmailSendOAuthStart,
    GmailSendResolve,
    MicrosoftGraphCursorResolve,
    MicrosoftGraphCursorStore,
    MicrosoftGraphOAuthComplete,
    MicrosoftGraphOAuthDeny,
    MicrosoftGraphOAuthInspect,
    MicrosoftGraphOAuthStart,
    MicrosoftGraphRefresh,
    Resolve,
    StandardsMicrosoftOAuthComplete,
    StandardsMicrosoftOAuthDeny,
    StandardsMicrosoftOAuthInspect,
    StandardsMicrosoftOAuthStart,
    StandardsPasswordProvision,
}

impl ResolverRoute {
    pub const ALL_PATHS: [&'static str; 23] = [
        "/v1/mailbox-credentials/discard",
        "/v1/mailbox-credentials/gmail/oauth/complete",
        "/v1/mailbox-credentials/gmail/oauth/deny",
        "/v1/mailbox-credentials/gmail/oauth/inspect",
        "/v1/mailbox-credentials/gmail/oauth/start",
        "/v1/mailbox-credentials/gmail/send/oauth/complete",
        "/v1/mailbox-credentials/gmail/send/oauth/deny",
        "/v1/mailbox-credentials/gmail/send/oauth/inspect",
        "/v1/mailbox-credentials/gmail/send/oauth/start",
        "/v1/mailbox-credentials/gmail/send/resolve",
        "/v1/mailbox-credentials/microsoft-graph/cursors/resolve",
        "/v1/mailbox-credentials/microsoft-graph/cursors/store",
        "/v1/mailbox-credentials/microsoft-graph/oauth/complete",
        "/v1/mailbox-credentials/microsoft-graph/oauth/deny",
        "/v1/mailbox-credentials/microsoft-graph/oauth/inspect",
        "/v1/mailbox-credentials/microsoft-graph/oauth/start",
        "/v1/mailbox-credentials/microsoft-graph/refresh",
        "/v1/mailbox-credentials/resolve",
        "/v1/mailbox-credentials/standards/microsoft/oauth/complete",
        "/v1/mailbox-credentials/standards/microsoft/oauth/deny",
        "/v1/mailbox-credentials/standards/microsoft/oauth/inspect",
        "/v1/mailbox-credentials/standards/microsoft/oauth/start",
        "/v1/mailbox-credentials/standards/password/provision",
    ];

    #[must_use]
    pub fn parse(path: &str) -> Option<Self> {
        Some(match path {
            "/v1/mailbox-credentials/discard" => Self::Discard,
            "/v1/mailbox-credentials/gmail/oauth/complete" => Self::GmailOAuthComplete,
            "/v1/mailbox-credentials/gmail/oauth/deny" => Self::GmailOAuthDeny,
            "/v1/mailbox-credentials/gmail/oauth/inspect" => Self::GmailOAuthInspect,
            "/v1/mailbox-credentials/gmail/oauth/start" => Self::GmailOAuthStart,
            "/v1/mailbox-credentials/gmail/send/oauth/complete" => Self::GmailSendOAuthComplete,
            "/v1/mailbox-credentials/gmail/send/oauth/deny" => Self::GmailSendOAuthDeny,
            "/v1/mailbox-credentials/gmail/send/oauth/inspect" => Self::GmailSendOAuthInspect,
            "/v1/mailbox-credentials/gmail/send/oauth/start" => Self::GmailSendOAuthStart,
            "/v1/mailbox-credentials/gmail/send/resolve" => Self::GmailSendResolve,
            "/v1/mailbox-credentials/microsoft-graph/cursors/resolve" => {
                Self::MicrosoftGraphCursorResolve
            }
            "/v1/mailbox-credentials/microsoft-graph/cursors/store" => {
                Self::MicrosoftGraphCursorStore
            }
            "/v1/mailbox-credentials/microsoft-graph/oauth/complete" => {
                Self::MicrosoftGraphOAuthComplete
            }
            "/v1/mailbox-credentials/microsoft-graph/oauth/deny" => Self::MicrosoftGraphOAuthDeny,
            "/v1/mailbox-credentials/microsoft-graph/oauth/inspect" => {
                Self::MicrosoftGraphOAuthInspect
            }
            "/v1/mailbox-credentials/microsoft-graph/oauth/start" => Self::MicrosoftGraphOAuthStart,
            "/v1/mailbox-credentials/microsoft-graph/refresh" => Self::MicrosoftGraphRefresh,
            "/v1/mailbox-credentials/resolve" => Self::Resolve,
            "/v1/mailbox-credentials/standards/microsoft/oauth/complete" => {
                Self::StandardsMicrosoftOAuthComplete
            }
            "/v1/mailbox-credentials/standards/microsoft/oauth/deny" => {
                Self::StandardsMicrosoftOAuthDeny
            }
            "/v1/mailbox-credentials/standards/microsoft/oauth/inspect" => {
                Self::StandardsMicrosoftOAuthInspect
            }
            "/v1/mailbox-credentials/standards/microsoft/oauth/start" => {
                Self::StandardsMicrosoftOAuthStart
            }
            "/v1/mailbox-credentials/standards/password/provision" => {
                Self::StandardsPasswordProvision
            }
            _ => return None,
        })
    }

    #[must_use]
    pub const fn purpose(self) -> &'static str {
        match self {
            Self::Discard => "credential_discard",
            Self::GmailOAuthComplete
            | Self::GmailOAuthDeny
            | Self::GmailOAuthInspect
            | Self::GmailOAuthStart => "gmail_read_oauth",
            Self::GmailSendOAuthComplete
            | Self::GmailSendOAuthDeny
            | Self::GmailSendOAuthInspect
            | Self::GmailSendOAuthStart
            | Self::GmailSendResolve => "gmail_send",
            Self::MicrosoftGraphCursorResolve | Self::MicrosoftGraphCursorStore => {
                "microsoft_graph_cursor"
            }
            Self::MicrosoftGraphOAuthComplete
            | Self::MicrosoftGraphOAuthDeny
            | Self::MicrosoftGraphOAuthInspect
            | Self::MicrosoftGraphOAuthStart
            | Self::MicrosoftGraphRefresh => "microsoft_graph_mail",
            Self::Resolve => "mailbox_execute",
            Self::StandardsMicrosoftOAuthComplete
            | Self::StandardsMicrosoftOAuthDeny
            | Self::StandardsMicrosoftOAuthInspect
            | Self::StandardsMicrosoftOAuthStart => "standards_microsoft_oauth",
            Self::StandardsPasswordProvision => "standards_password",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolverEnvelope {
    pub tenant_id: String,
    pub purpose: String,
    #[serde(flatten)]
    pub payload: serde_json::Map<String, serde_json::Value>,
}

impl Drop for ResolverEnvelope {
    fn drop(&mut self) {
        self.tenant_id.zeroize();
        self.purpose.zeroize();
        zeroize_map(&mut self.payload);
    }
}

fn zeroize_map(map: &mut serde_json::Map<String, serde_json::Value>) {
    for value in map.values_mut() {
        zeroize_value(value);
    }
}

fn zeroize_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(secret) => secret.zeroize(),
        serde_json::Value::Array(values) => {
            for nested in values {
                zeroize_value(nested);
            }
        }
        serde_json::Value::Object(map) => zeroize_map(map),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDocument<'a> {
    pub code: &'a str,
}

#[cfg(test)]
mod tests {
    use super::ResolverRoute;

    #[test]
    fn exact_route_inventory_is_unique_and_parseable() {
        let mut paths = ResolverRoute::ALL_PATHS.to_vec();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), 23);
        assert!(
            paths
                .into_iter()
                .all(|path| ResolverRoute::parse(path).is_some())
        );
        assert!(ResolverRoute::parse("/v1/mailbox-credentials/unknown").is_none());
    }
}
