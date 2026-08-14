use crate::cloud_mailbox_secrets::MAILBOX_SECRET_RESOLVER_BINDING;
use crate::resolver_request::signed_resolver_request;
use mailbox_domain::{MailboxBinding, MailboxProvider};
use serde::Deserialize;
use serde_json::{Map, Value};
use worker::Env;
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
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| GmailSendCredentialError::IntegrityFailure)?;
    let payload = Map::from_iter([
        (
            "mailboxBindingId".to_owned(),
            Value::String(binding.binding_id().as_str().to_owned()),
        ),
        (
            "secretHandle".to_owned(),
            Value::String(binding.secret_handle().as_str().to_owned()),
        ),
        ("provider".to_owned(), Value::String("GMAIL_API".to_owned())),
        ("capability".to_owned(), Value::String("SEND".to_owned())),
    ]);
    let init = signed_resolver_request(
        env,
        RESOLVE_ENDPOINT,
        binding.tenant_id().as_str(),
        "gmail_send",
        payload,
    )
    .map_err(|_| GmailSendCredentialError::IntegrityFailure)?;
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
