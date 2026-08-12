use application_ports::mailboxes::MailboxProviderPortError;
use mailbox_domain::{
    MailboxBinding, MailboxProvider, MailboxProviderFailure, MailboxProviderFailureClass,
};
use serde::Deserialize;
use worker::{Env, Headers, Method, RequestInit};
use zeroize::Zeroize;

pub const MAILBOX_SECRET_RESOLVER_BINDING: &str = "MAILBOX_SECRET_RESOLVER";
const MAILBOX_SECRET_RESOLVER_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/resolve";
const MAX_SECRET_DOCUMENT_BYTES: usize = 16 * 1024;
const MAX_IMAP_HOST_LENGTH: usize = 253;
const MAX_IMAP_USERNAME_LENGTH: usize = 512;
const MAX_CREDENTIAL_VALUE_LENGTH: usize = 8 * 1024;

#[derive(Deserialize)]
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

#[derive(Deserialize)]
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImapAuthenticationMode {
    Password,
    Xoauth2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImapCredential {
    host: String,
    port: u16,
    username: String,
    password: Option<String>,
    access_token: Option<String>,
    authentication_mode: Option<ImapAuthenticationMode>,
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
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    #[must_use]
    pub fn access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    #[must_use]
    pub const fn authentication_mode(&self) -> ImapAuthenticationMode {
        match self.authentication_mode {
            Some(value) => value,
            None => ImapAuthenticationMode::Password,
        }
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
            && !contains_imap_line_break(&self.username)
            && match self.authentication_mode() {
                ImapAuthenticationMode::Password => {
                    self.access_token.is_none()
                        && self.password.as_deref().is_some_and(valid_credential_value)
                }
                ImapAuthenticationMode::Xoauth2 => {
                    self.password.is_none()
                        && self
                            .access_token
                            .as_deref()
                            .is_some_and(valid_credential_value)
                }
            }
    }
}

impl Drop for ImapCredential {
    fn drop(&mut self) {
        self.username.zeroize();
        if let Some(password) = self.password.as_mut() {
            password.zeroize();
        }
        if let Some(access_token) = self.access_token.as_mut() {
            access_token.zeroize();
        }
    }
}

pub async fn resolve_mailbox_credential(
    env: &Env,
    binding: &MailboxBinding,
) -> Result<MailboxCredential, MailboxProviderPortError> {
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    let headers = Headers::new();
    headers
        .set("accept", "application/json")
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    headers
        .set("cache-control", "no-store")
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    headers
        .set("x-profile-tenant-id", binding.tenant_id().as_str())
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    headers
        .set(
            "x-profile-mailbox-secret-handle",
            binding.secret_handle().as_str(),
        )
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    headers
        .set(
            "x-profile-mailbox-provider",
            binding.provider().storage_value(),
        )
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(headers);
    let mut response = resolver
        .fetch(MAILBOX_SECRET_RESOLVER_ENDPOINT, Some(init))
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
    map_resolver_status(response.status_code())?;
    if response_content_length_exceeds(&response, MAX_SECRET_DOCUMENT_BYTES)? {
        return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
    }

    let mut document = response
        .bytes()
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
    if document.is_empty() || document.len() > MAX_SECRET_DOCUMENT_BYTES {
        document.zeroize();
        return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy));
    }
    let parsed = serde_json::from_slice::<MailboxCredential>(&document);
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

fn map_resolver_status(status: u16) -> Result<(), MailboxProviderPortError> {
    match status {
        200 => Ok(()),
        401 | 404 | 410 => Err(provider_error(MailboxProviderFailureClass::Authentication)),
        403 | 409 | 422 => Err(provider_error(MailboxProviderFailureClass::ProviderPolicy)),
        408 | 425 | 500..=599 => Err(provider_error(
            MailboxProviderFailureClass::TransientDependency,
        )),
        429 => Err(provider_error(MailboxProviderFailureClass::RateLimited)),
        _ => Err(provider_error(MailboxProviderFailureClass::Permanent)),
    }
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, MailboxProviderPortError> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value
        .parse::<usize>()
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    Ok(length > maximum)
}

pub fn provider_error(class: MailboxProviderFailureClass) -> MailboxProviderPortError {
    match MailboxProviderFailure::new(class, None) {
        Ok(failure) => MailboxProviderPortError::Failure(failure),
        Err(_) => MailboxProviderPortError::IntegrityFailure,
    }
}

fn valid_credential_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CREDENTIAL_VALUE_LENGTH
        && !contains_imap_line_break(value)
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
    use super::{
        ImapAuthenticationMode, ImapTlsMode, MailboxCredential, map_resolver_status,
        valid_imap_host,
    };

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

    #[test]
    fn password_and_xoauth2_documents_are_mutually_exclusive() {
        let password: MailboxCredential = serde_json::from_str(
            r#"{"kind":"imap","host":"imap.example.com","port":993,"username":"user@example.com","password":"secret","access_token":null,"authentication_mode":null,"tls":"implicit"}"#,
        )
        .expect("legacy password document must parse");
        let MailboxCredential::Imap(password) = password else {
            panic!("expected IMAP password credential");
        };
        assert_eq!(
            password.authentication_mode(),
            ImapAuthenticationMode::Password
        );
        assert!(password.validate());

        let xoauth2: MailboxCredential = serde_json::from_str(
            r#"{"kind":"imap","host":"outlook.office365.com","port":993,"username":"user@example.com","password":null,"access_token":"opaque-access-token","authentication_mode":"xoauth2","tls":"implicit"}"#,
        )
        .expect("XOAUTH2 document must parse");
        let MailboxCredential::Imap(xoauth2) = xoauth2 else {
            panic!("expected IMAP XOAUTH2 credential");
        };
        assert_eq!(xoauth2.authentication_mode(), ImapAuthenticationMode::Xoauth2);
        assert!(xoauth2.validate());

        let mixed: MailboxCredential = serde_json::from_str(
            r#"{"kind":"imap","host":"outlook.office365.com","port":993,"username":"user@example.com","password":"forbidden","access_token":"opaque-access-token","authentication_mode":"xoauth2","tls":"implicit"}"#,
        )
        .expect("mixed shape must parse before validation");
        let MailboxCredential::Imap(mixed) = mixed else {
            panic!("expected IMAP mixed credential");
        };
        assert!(!mixed.validate());
    }

    #[test]
    fn secret_resolver_statuses_fail_closed_into_mailbox_taxonomy() {
        assert!(map_resolver_status(200).is_ok());
        for status in [401, 404, 410, 403, 409, 422, 408, 425, 429, 500, 503, 418] {
            assert!(
                map_resolver_status(status).is_err(),
                "accepted status {status}"
            );
        }
    }
}
