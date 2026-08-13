use mailbox_domain::{MailboxBinding, MailboxProvider};
use serde::Deserialize;
use worker::{Env, Headers, Method, RequestInit};
use zeroize::Zeroize;

const MAILBOX_SECRET_RESOLVER_BINDING: &str = "MAILBOX_SECRET_RESOLVER";
const MAILBOX_SECRET_RESOLVER_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/resolve";
const SMTP_SEND_PURPOSE: &str = "SMTP_SEND";
const MAX_SECRET_DOCUMENT_BYTES: usize = 16 * 1024;
const MAX_HOST_LENGTH: usize = 253;
const MAX_USERNAME_LENGTH: usize = 512;
const MAX_CREDENTIAL_VALUE_LENGTH: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SmtpTlsMode {
    Implicit,
    StartTls,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SmtpAuthenticationMode {
    Password,
    Xoauth2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SmtpCredential {
    host: String,
    port: u16,
    username: String,
    password: Option<String>,
    access_token: Option<String>,
    authentication_mode: SmtpAuthenticationMode,
    tls: SmtpTlsMode,
}

impl SmtpCredential {
    #[must_use]
    pub(crate) fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub(crate) const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    #[must_use]
    pub(crate) fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    #[must_use]
    pub(crate) fn access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    #[must_use]
    pub(crate) const fn authentication_mode(&self) -> SmtpAuthenticationMode {
        self.authentication_mode
    }

    #[must_use]
    pub(crate) const fn tls(&self) -> SmtpTlsMode {
        self.tls
    }

    fn validate(&self) -> bool {
        valid_public_dns_host(&self.host)
            && matches!(
                (self.tls, self.port),
                (SmtpTlsMode::Implicit, 465) | (SmtpTlsMode::StartTls, 587)
            )
            && !self.username.is_empty()
            && self.username.len() <= MAX_USERNAME_LENGTH
            && !contains_protocol_control(&self.username)
            && match self.authentication_mode {
                SmtpAuthenticationMode::Password => {
                    self.access_token.is_none()
                        && self.password.as_deref().is_some_and(valid_credential_value)
                }
                SmtpAuthenticationMode::Xoauth2 => {
                    self.password.is_none()
                        && self
                            .access_token
                            .as_deref()
                            .is_some_and(valid_credential_value)
                }
            }
    }
}

impl Drop for SmtpCredential {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SmtpCredentialError {
    RetryableNotSent,
    Rejected,
    IntegrityFailure,
}

pub(crate) async fn resolve_smtp_send_credential(
    env: &Env,
    binding: &MailboxBinding,
) -> Result<SmtpCredential, SmtpCredentialError> {
    if binding.provider() != MailboxProvider::Imap || !binding.is_executable() {
        return Err(SmtpCredentialError::Rejected);
    }
    let headers = resolver_headers(binding)?;
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(headers);
    let mut response = resolver
        .fetch(MAILBOX_SECRET_RESOLVER_ENDPOINT, Some(init))
        .await
        .map_err(|_| SmtpCredentialError::RetryableNotSent)?;
    classify_resolver_status(response.status_code())?;
    if response_content_length_exceeds(&response, MAX_SECRET_DOCUMENT_BYTES)? {
        return Err(SmtpCredentialError::Rejected);
    }
    let mut document = response
        .bytes()
        .await
        .map_err(|_| SmtpCredentialError::RetryableNotSent)?;
    if document.is_empty() || document.len() > MAX_SECRET_DOCUMENT_BYTES {
        document.zeroize();
        return Err(SmtpCredentialError::Rejected);
    }
    let parsed = serde_json::from_slice::<SmtpCredential>(&document);
    document.zeroize();
    let credential = parsed.map_err(|_| SmtpCredentialError::Rejected)?;
    if !credential.validate() {
        return Err(SmtpCredentialError::Rejected);
    }
    Ok(credential)
}

fn resolver_headers(binding: &MailboxBinding) -> Result<Headers, SmtpCredentialError> {
    let headers = Headers::new();
    headers
        .set("accept", "application/json")
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    headers
        .set("cache-control", "no-store")
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    headers
        .set("x-profile-tenant-id", binding.tenant_id().as_str())
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    headers
        .set(
            "x-profile-mailbox-secret-handle",
            binding.secret_handle().as_str(),
        )
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    headers
        .set(
            "x-profile-mailbox-provider",
            binding.provider().storage_value(),
        )
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    headers
        .set("x-profile-mailbox-credential-purpose", SMTP_SEND_PURPOSE)
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    Ok(headers)
}

const fn classify_resolver_status(status: u16) -> Result<(), SmtpCredentialError> {
    match status {
        200 => Ok(()),
        408 | 425 | 429 | 500..=599 => Err(SmtpCredentialError::RetryableNotSent),
        401 | 403 | 404 | 409 | 410 | 422 => Err(SmtpCredentialError::Rejected),
        _ => Err(SmtpCredentialError::Rejected),
    }
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, SmtpCredentialError> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value
        .parse::<usize>()
        .map_err(|_| SmtpCredentialError::IntegrityFailure)?;
    Ok(length > maximum)
}

fn valid_credential_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CREDENTIAL_VALUE_LENGTH
        && !contains_protocol_control(value)
}

fn valid_public_dns_host(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_HOST_LENGTH
        || value.eq_ignore_ascii_case("localhost")
        || value.ends_with(".local")
        || value.ends_with(".localhost")
        || value.ends_with(".internal")
        || value.parse::<std::net::IpAddr>().is_ok()
    {
        return false;
    }
    let labels: Vec<&str> = value.split('.').collect();
    labels.len() >= 2 && labels.into_iter().all(valid_dns_label)
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

fn contains_protocol_control(value: &str) -> bool {
    value.bytes().any(|byte| byte.is_ascii_control())
}

#[cfg(test)]
mod tests {
    use super::{
        SmtpAuthenticationMode, SmtpCredential, SmtpTlsMode, classify_resolver_status,
        valid_public_dns_host,
    };

    #[test]
    fn smtp_projection_accepts_only_governed_encrypted_endpoints()
    -> Result<(), Box<dyn std::error::Error>> {
        let password = serde_json::from_str::<SmtpCredential>(
            r#"{"host":"smtp.example.com","port":587,"username":"user@example.com","password":"secret","access_token":null,"authentication_mode":"password","tls":"start_tls"}"#,
        )?;
        assert_eq!(
            password.authentication_mode(),
            SmtpAuthenticationMode::Password
        );
        assert_eq!(password.tls(), SmtpTlsMode::StartTls);
        assert!(password.validate());

        let xoauth2 = serde_json::from_str::<SmtpCredential>(
            r#"{"host":"smtp.office365.com","port":587,"username":"user@example.com","password":null,"access_token":"opaque-access-token","authentication_mode":"xoauth2","tls":"start_tls"}"#,
        )?;
        assert!(xoauth2.validate());

        let plaintext = serde_json::from_str::<SmtpCredential>(
            r#"{"host":"smtp.example.com","port":25,"username":"user@example.com","password":"secret","access_token":null,"authentication_mode":"password","tls":"start_tls"}"#,
        )?;
        assert!(!plaintext.validate());
        assert!(valid_public_dns_host("smtp.example.com"));
        assert!(!valid_public_dns_host("127.0.0.1"));
        Ok(())
    }

    #[test]
    fn resolver_statuses_distinguish_safe_retry_from_rejection() {
        assert!(classify_resolver_status(200).is_ok());
        assert_eq!(
            classify_resolver_status(429),
            Err(super::SmtpCredentialError::RetryableNotSent)
        );
        assert_eq!(
            classify_resolver_status(503),
            Err(super::SmtpCredentialError::RetryableNotSent)
        );
        assert_eq!(
            classify_resolver_status(401),
            Err(super::SmtpCredentialError::Rejected)
        );
    }
}
