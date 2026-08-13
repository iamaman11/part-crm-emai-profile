use crate::cloud_mailbox_secrets::MAILBOX_SECRET_RESOLVER_BINDING;
use mailbox_domain::{MailboxBinding, MailboxProvider};
use serde::Deserialize;
use worker::{Env, Headers, Method, RequestInit};
use zeroize::Zeroize;

const RESOLVE_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/gmail/send/resolve";
const MAX_SECRET_DOCUMENT_BYTES: usize = 16 * 1024;
const MAX_CREDENTIAL_VALUE_LENGTH: usize = 8 * 1024;

pub struct GmailSendCredential {
    access_token: String,
}

impl GmailSendCredential {
    #[must_use]
    pub fn access_token(&self) -> &str {
        &self.access_token
    }
}

impl Drop for GmailSendCredential {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GmailSendCredentialError {
    ReauthRequired,
    RetryableNotSent,
    Rejected,
    IntegrityFailure,
}

pub async fn resolve_gmail_send_credential(
    env: &Env,
    binding: &MailboxBinding,
) -> Result<GmailSendCredential, GmailSendCredentialError> {
    if binding.provider() != MailboxProvider::GmailApi || !binding.is_executable() {
        return Err(GmailSendCredentialError::Rejected);
    }
    let headers = resolver_headers(binding)?;
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| GmailSendCredentialError::IntegrityFailure)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(headers);
    let mut response = resolver
        .fetch(RESOLVE_ENDPOINT, Some(init))
        .await
        .map_err(|_| GmailSendCredentialError::RetryableNotSent)?;
    match response.status_code() {
        200 => {}
        401 | 404 | 410 => return Err(GmailSendCredentialError::ReauthRequired),
        403 | 409 | 422 => return Err(GmailSendCredentialError::Rejected),
        408 | 425 | 429 | 500..=599 => {
            return Err(GmailSendCredentialError::RetryableNotSent);
        }
        _ => return Err(GmailSendCredentialError::Rejected),
    }
    if response_content_length_exceeds(&response, MAX_SECRET_DOCUMENT_BYTES)? {
        return Err(GmailSendCredentialError::Rejected);
    }
    let mut bytes = response
        .bytes()
        .await
        .map_err(|_| GmailSendCredentialError::RetryableNotSent)?;
    if bytes.is_empty() || bytes.len() > MAX_SECRET_DOCUMENT_BYTES {
        bytes.zeroize();
        return Err(GmailSendCredentialError::Rejected);
    }
    let parsed = serde_json::from_slice::<CredentialDocument>(&bytes);
    bytes.zeroize();
    let document = parsed.map_err(|_| GmailSendCredentialError::Rejected)?;
    if !valid_credential_value(&document.access_token) {
        return Err(GmailSendCredentialError::Rejected);
    }
    Ok(GmailSendCredential {
        access_token: document.into_access_token(),
    })
}

fn valid_credential_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CREDENTIAL_VALUE_LENGTH
        && !value.chars().any(char::is_control)
}

fn resolver_headers(binding: &MailboxBinding) -> Result<Headers, GmailSendCredentialError> {
    let headers = Headers::new();
    set_header(&headers, "accept", "application/json")?;
    set_header(&headers, "cache-control", "no-store")?;
    set_header(
        &headers,
        "x-profile-tenant-id",
        binding.tenant_id().as_str(),
    )?;
    set_header(
        &headers,
        "x-profile-mailbox-secret-handle",
        binding.secret_handle().as_str(),
    )?;
    set_header(&headers, "x-profile-mailbox-provider", "GMAIL_API")?;
    set_header(&headers, "x-profile-mailbox-capability", "SEND")?;
    Ok(headers)
}

fn set_header(
    headers: &Headers,
    name: &str,
    value: &str,
) -> Result<(), GmailSendCredentialError> {
    headers
        .set(name, value)
        .map_err(|_| GmailSendCredentialError::IntegrityFailure)
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, GmailSendCredentialError> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| GmailSendCredentialError::IntegrityFailure)?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value
        .parse::<usize>()
        .map_err(|_| GmailSendCredentialError::IntegrityFailure)?;
    Ok(length > maximum)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialDocument {
    access_token: String,
}

impl CredentialDocument {
    fn into_access_token(mut self) -> String {
        core::mem::take(&mut self.access_token)
    }
}

impl Drop for CredentialDocument {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}
