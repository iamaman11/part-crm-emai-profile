use application_ports::mailboxes::MailboxProviderPortError;
use mailbox_domain::{MailboxProvider, MailboxProviderFailure, MailboxProviderFailureClass};
use profile_platform_primitives::SecretHandle;
use serde::Deserialize;
use worker::Env;
use zeroize::Zeroize;

const MAX_SECRET_DOCUMENT_BYTES: usize = 16 * 1024;
const MAX_IMAP_HOST_LENGTH: usize = 253;
const MAX_IMAP_USERNAME_LENGTH: usize = 512;
const MAX_CREDENTIAL_VALUE_LENGTH: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MailboxCredential {
    GmailApi(GmailApiCredential),
    Imap(ImapCredential),
}

impl MailboxCredential {
    #[must_use]
    pub const fn provider(&self) -> MailboxProvider {
        match self {
            Self::GmailApi(_) => MailboxProvider::GmailApi,
            Self::Imap(_) => MailboxProvider::Imap,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GmailApiCredential {
    access_token: String,
}

impl GmailApiCredential {
    #[must_use]
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    fn validate(&self) -> bool {
        !self.access_token.is_empty() && self.access_token.len() <= MAX_CREDENTIAL_VALUE_LENGTH
    }
}

impl Drop for GmailApiCredential {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImapTlsMode {
    Implicit,
    StartTls,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImapCredential {
    host: String,
    port: u16,
    username: String,
    password: String,
    tls: ImapTlsMode,
}

impl ImapCredential {
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }

    #[must_use]
    pub const fn tls(&self) -> ImapTlsMode {
        self.tls
    }

    fn validate(&self) -> bool {
        valid_imap_host(&self.host)
            && matches!(
                (self.tls, self.port),
                (ImapTlsMode::Implicit, 993) | (ImapTlsMode::StartTls, 143)
            )
            && !self.username.is_empty()
            && self.username.len() <= MAX_IMAP_USERNAME_LENGTH
            && !self.password.is_empty()
            && self.password.len() <= MAX_CREDENTIAL_VALUE_LENGTH
            && !contains_imap_line_break(&self.username)
            && !contains_imap_line_break(&self.password)
    }
}

impl Drop for ImapCredential {
    fn drop(&mut self) {
        self.username.zeroize();
        self.password.zeroize();
    }
}

pub async fn resolve_mailbox_credential(
    env: &Env,
    handle: &SecretHandle,
) -> Result<MailboxCredential, MailboxProviderPortError> {
    let store = env
        .secret_store(handle.as_str())
        .map_err(|_| provider_error(MailboxProviderFailureClass::Authentication))?;
    let mut document = store
        .get()
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?
        .ok_or_else(|| provider_error(MailboxProviderFailureClass::Authentication))?;
    if document.is_empty() || document.len() > MAX_SECRET_DOCUMENT_BYTES {
        document.zeroize();
        return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
    }
    let parsed = serde_json::from_str::<MailboxCredential>(&document);
    document.zeroize();
    let credential =
        parsed.map_err(|_| provider_error(MailboxProviderFailureClass::ProviderPolicy))?;
    let valid = match &credential {
        MailboxCredential::GmailApi(value) => value.validate(),
        MailboxCredential::Imap(value) => value.validate(),
    };
    if !valid {
        return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
    }
    Ok(credential)
}

pub fn provider_error(class: MailboxProviderFailureClass) -> MailboxProviderPortError {
    match MailboxProviderFailure::new(class, None) {
        Ok(failure) => MailboxProviderPortError::Failure(failure),
        Err(_) => MailboxProviderPortError::IntegrityFailure,
    }
}

fn valid_imap_host(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_IMAP_HOST_LENGTH
        || value.eq_ignore_ascii_case("localhost")
        || value.ends_with(".local")
        || value.parse::<std::net::IpAddr>().is_ok()
    {
        return false;
    }
    let mut labels = value.split('.');
    let Some(first) = labels.next() else {
        return false;
    };
    valid_dns_label(first) && labels.all(valid_dns_label)
}

fn valid_dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn contains_imap_line_break(value: &str) -> bool {
    value
        .bytes()
        .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
}

#[cfg(test)]
mod tests {
    use super::{ImapTlsMode, valid_imap_host};

    #[test]
    fn imap_host_validation_rejects_local_and_literal_targets() {
        for forbidden in [
            "",
            "localhost",
            "mail.local",
            "127.0.0.1",
            "::1",
            "-bad.example",
        ] {
            assert!(!valid_imap_host(forbidden), "accepted {forbidden}");
        }
        assert!(valid_imap_host("imap.example.com"));
        assert_eq!(ImapTlsMode::Implicit, ImapTlsMode::Implicit);
    }
}
