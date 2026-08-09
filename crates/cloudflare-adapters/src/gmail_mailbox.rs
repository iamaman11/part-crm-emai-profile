use crate::cloud_mailbox_secrets::{GmailApiCredential, provider_error};
use application_ports::mailboxes::MailboxProviderPortError;
use mailbox_domain::{MailboxBinding, MailboxJob, MailboxObservation, MailboxProviderFailureClass};
use serde::Deserialize;
use worker::{Fetch, Headers, Method, Request, RequestInit};

const GMAIL_MESSAGES_ENDPOINT: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const MAX_GMAIL_RESPONSE_BYTES: usize = 256 * 1024;
const GMAIL_PAGE_SIZE: u32 = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessageListResponse {
    #[serde(default)]
    messages: Vec<GmailMessageReference>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessageReference {
    id: String,
}

pub async fn check_gmail_mailbox(
    binding: &MailboxBinding,
    job: &MailboxJob,
    credential: &GmailApiCredential,
) -> Result<MailboxObservation, MailboxProviderPortError> {
    let mut endpoint = String::from(GMAIL_MESSAGES_ENDPOINT);
    endpoint.push_str("?maxResults=");
    endpoint.push_str(&GMAIL_PAGE_SIZE.to_string());
    endpoint.push_str("&includeSpamTrash=false");
    if let Some(cursor) = job.cursor() {
        endpoint.push_str("&pageToken=");
        push_percent_encoded(&mut endpoint, cursor);
    }

    let headers = Headers::new();
    let mut authorization = String::with_capacity(7 + credential.access_token().len());
    authorization.push_str("Bearer ");
    authorization.push_str(credential.access_token());
    headers
        .set("authorization", &authorization)
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    authorization.clear();
    headers
        .set("accept", "application/json")
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(&endpoint, &init)
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;

    match response.status_code() {
        200 => {}
        401 => return Err(provider_error(MailboxProviderFailureClass::Authentication)),
        403 => return Err(provider_error(MailboxProviderFailureClass::ProviderPolicy)),
        408 | 425 | 500..=599 => {
            return Err(provider_error(MailboxProviderFailureClass::TransientDependency));
        }
        429 => return Err(provider_error(MailboxProviderFailureClass::RateLimited)),
        _ => return Err(provider_error(MailboxProviderFailureClass::Permanent)),
    }

    if response_content_length_exceeds(&response, MAX_GMAIL_RESPONSE_BYTES)? {
        return Err(provider_error(MailboxProviderFailureClass::Permanent));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
    if bytes.len() > MAX_GMAIL_RESPONSE_BYTES {
        return Err(provider_error(MailboxProviderFailureClass::Permanent));
    }
    let parsed: GmailMessageListResponse = serde_json::from_slice(&bytes)
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
    if parsed.messages.iter().any(|message| message.id.is_empty()) {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    let bounded_item_count = u32::try_from(parsed.messages.len())
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    MailboxObservation::new(
        binding.binding_id().clone(),
        "GMAIL_API_OK",
        bounded_item_count,
        parsed.next_page_token,
    )
    .map_err(|_| MailboxProviderPortError::IntegrityFailure)
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

fn push_percent_encoded(output: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::push_percent_encoded;

    #[test]
    fn query_component_encoding_is_rfc3986_safe() {
        let mut output = String::new();
        push_percent_encoded(&mut output, "a b+c/=");
        assert_eq!(output, "a%20b%2Bc%2F%3D");
    }
}
