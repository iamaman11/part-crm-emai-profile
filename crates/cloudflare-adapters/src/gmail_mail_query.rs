use crate::cloud_mailbox_secrets::GmailApiCredential;
use application_ports::query::{QueryCursor, QueryPage, QueryPortError, QueryPortErrorClass};
use application_ports::query_mail_provider::{
    MAX_MAIL_BODY_BYTES, MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use mailbox_domain::MailboxBinding;
use profile_platform_primitives::UnixMillis;
use serde::Deserialize;
use worker::{Fetch, Headers, Method, Request, RequestInit};
use zeroize::Zeroize;

const GMAIL_MESSAGES_ENDPOINT: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const GMAIL_CURSOR_PREFIX: &str = "gmail:";
const GMAIL_REFERENCE_PREFIX: &str = "gmail:";
const MAX_GMAIL_QUERY_PAGE_SIZE: u16 = 25;
const MAX_GMAIL_LIST_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_GMAIL_METADATA_RESPONSE_BYTES: usize = 96 * 1024;
const MAX_GMAIL_MESSAGE_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;
const MAX_GMAIL_PARTS: usize = 256;
const MAX_GMAIL_MIME_DEPTH: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessageListResponse {
    #[serde(default)]
    messages: Vec<GmailMessageReference>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GmailMessageReference {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessageResponse {
    id: String,
    internal_date: String,
    payload: GmailMessagePart,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessagePart {
    #[serde(default)]
    mime_type: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    headers: Vec<GmailHeader>,
    #[serde(default)]
    body: GmailMessagePartBody,
    #[serde(default)]
    parts: Vec<GmailMessagePart>,
}

#[derive(Debug, Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct GmailMessagePartBody {
    #[serde(default)]
    size: u64,
    data: Option<String>,
}

pub async fn search_gmail_messages(
    binding: &MailboxBinding,
    request: &SearchClientMailboxMessagesRequest,
    credential: &GmailApiCredential,
) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
    let page_size = request
        .page()
        .limit()
        .value()
        .min(MAX_GMAIL_QUERY_PAGE_SIZE);
    let cursor = request
        .page()
        .cursor()
        .map(parse_gmail_cursor)
        .transpose()?;
    let mut endpoint = String::from(GMAIL_MESSAGES_ENDPOINT);
    endpoint.push_str("?includeSpamTrash=false&maxResults=");
    endpoint.push_str(&page_size.to_string());
    if let Some(term) = request.term() {
        endpoint.push_str("&q=");
        push_percent_encoded(&mut endpoint, term.as_str());
    }
    if let Some(cursor) = cursor {
        endpoint.push_str("&pageToken=");
        push_percent_encoded(&mut endpoint, cursor);
    }

    let bytes = gmail_json_get(
        &endpoint,
        credential,
        MAX_GMAIL_LIST_RESPONSE_BYTES,
        request.page().cursor().is_some(),
    )
    .await?
    .ok_or_else(integrity_failure)?;
    let parsed: GmailMessageListResponse =
        serde_json::from_slice(&bytes).map_err(|_| integrity_failure())?;
    if parsed.messages.len() > usize::from(page_size) {
        return Err(integrity_failure());
    }

    let mut items = Vec::with_capacity(parsed.messages.len());
    for message in parsed.messages {
        validate_gmail_token(&message.id)?;
        let endpoint = message_endpoint(&message.id, "metadata");
        let Some(bytes) = gmail_json_get(
            &endpoint,
            credential,
            MAX_GMAIL_METADATA_RESPONSE_BYTES,
            false,
        )
        .await?
        else {
            continue;
        };
        let message: GmailMessageResponse =
            serde_json::from_slice(&bytes).map_err(|_| integrity_failure())?;
        items.push(summary_from_gmail(binding, &message)?);
    }

    let next_cursor = parsed
        .next_page_token
        .map(|token| gmail_query_cursor(&token))
        .transpose()?;
    Ok(QueryPage::new(items, next_cursor))
}

pub async fn get_gmail_message(
    binding: &MailboxBinding,
    provider_reference: &str,
    credential: &GmailApiCredential,
) -> Result<Option<MailMessageBody>, QueryPortError> {
    let message_id = parse_gmail_reference(provider_reference)?;
    let endpoint = message_endpoint(message_id, "full");
    let Some(bytes) = gmail_json_get(
        &endpoint,
        credential,
        MAX_GMAIL_MESSAGE_RESPONSE_BYTES,
        false,
    )
    .await?
    else {
        return Ok(None);
    };
    let message: GmailMessageResponse =
        serde_json::from_slice(&bytes).map_err(|_| integrity_failure())?;
    if message.id != message_id {
        return Err(integrity_failure());
    }
    let summary = summary_from_gmail(binding, &message)?;
    let (text_body, html_body) = extract_gmail_bodies(&message.payload)?;
    MailMessageBody::new(summary, text_body, html_body)
        .map(Some)
        .map_err(|_| integrity_failure())
}

async fn gmail_json_get(
    endpoint: &str,
    credential: &GmailApiCredential,
    maximum_bytes: usize,
    invalid_cursor_on_bad_request: bool,
) -> Result<Option<Vec<u8>>, QueryPortError> {
    let headers = Headers::new();
    let mut authorization = String::with_capacity(7 + credential.access_token().len());
    authorization.push_str("Bearer ");
    authorization.push_str(credential.access_token());
    let header_result = headers.set("authorization", &authorization);
    authorization.zeroize();
    header_result.map_err(|_| integrity_failure())?;
    headers
        .set("accept", "application/json")
        .map_err(|_| integrity_failure())?;

    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(endpoint, &init).map_err(|_| integrity_failure())?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| dependency_unavailable())?;
    match response.status_code() {
        200 => {}
        400 if invalid_cursor_on_bad_request => return Err(invalid_cursor()),
        404 => return Ok(None),
        401 | 403 | 408 | 425 | 429 | 500..=599 => return Err(dependency_unavailable()),
        _ => return Err(dependency_unavailable()),
    }
    if response_content_length_exceeds(&response, maximum_bytes)? {
        return Err(integrity_failure());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| dependency_unavailable())?;
    if bytes.len() > maximum_bytes {
        return Err(integrity_failure());
    }
    Ok(Some(bytes))
}

fn summary_from_gmail(
    binding: &MailboxBinding,
    message: &GmailMessageResponse,
) -> Result<MailMessageSummary, QueryPortError> {
    validate_gmail_token(&message.id)?;
    let received_at = message
        .internal_date
        .parse::<u64>()
        .map(UnixMillis::new)
        .map_err(|_| integrity_failure())?;
    let subject = bounded_header(&message.payload.headers, "Subject")?;
    let sender = bounded_header(&message.payload.headers, "From")?;
    let reference = MailboxMessageReference::new(
        binding.binding_id().clone(),
        format!("{GMAIL_REFERENCE_PREFIX}{}", message.id),
    )
    .map_err(|_| integrity_failure())?;
    Ok(MailMessageSummary::new(
        reference,
        subject,
        sender,
        received_at,
    ))
}

fn bounded_header(headers: &[GmailHeader], name: &str) -> Result<Option<String>, QueryPortError> {
    let value = headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.trim().to_owned());
    if value.as_ref().is_some_and(|value| {
        value.len() > MAX_HEADER_VALUE_BYTES
            || value
                .chars()
                .any(|character| character.is_control() && character != '\t')
    }) {
        return Err(integrity_failure());
    }
    Ok(value.filter(|value| !value.is_empty()))
}

fn extract_gmail_bodies(
    root: &GmailMessagePart,
) -> Result<(Option<String>, Option<String>), QueryPortError> {
    let mut text = String::new();
    let mut html = String::new();
    let mut stack = vec![(root, 0_usize)];
    let mut visited = 0_usize;
    while let Some((part, depth)) = stack.pop() {
        visited = visited.checked_add(1).ok_or_else(integrity_failure)?;
        if visited > MAX_GMAIL_PARTS || depth > MAX_GMAIL_MIME_DEPTH {
            return Err(integrity_failure());
        }
        if part.parts.len() > MAX_GMAIL_PARTS.saturating_sub(visited) {
            return Err(integrity_failure());
        }
        for child in part.parts.iter().rev() {
            stack.push((child, depth + 1));
        }
        if !part.filename.is_empty() {
            continue;
        }
        let target = if part.mime_type.eq_ignore_ascii_case("text/plain") {
            Some(&mut text)
        } else if part.mime_type.eq_ignore_ascii_case("text/html") {
            Some(&mut html)
        } else {
            None
        };
        let Some(target) = target else {
            continue;
        };
        if part.body.size > MAX_MAIL_BODY_BYTES as u64 {
            return Err(integrity_failure());
        }
        let Some(data) = part.body.data.as_deref() else {
            continue;
        };
        let decoded = decode_base64url(data, MAX_MAIL_BODY_BYTES)?;
        append_utf8_body(target, &decoded)?;
        if text.len().saturating_add(html.len()) > MAX_MAIL_BODY_BYTES {
            return Err(integrity_failure());
        }
    }
    Ok((
        (!text.is_empty()).then_some(text),
        (!html.is_empty()).then_some(html),
    ))
}

fn append_utf8_body(target: &mut String, decoded: &[u8]) -> Result<(), QueryPortError> {
    let decoded = core::str::from_utf8(decoded).map_err(|_| integrity_failure())?;
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(decoded);
    Ok(())
}

fn decode_base64url(value: &str, maximum_bytes: usize) -> Result<Vec<u8>, QueryPortError> {
    if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(integrity_failure());
    }
    let padding = value
        .len()
        .saturating_sub(value.trim_end_matches('=').len());
    if padding > 2 {
        return Err(integrity_failure());
    }
    let unpadded = value.trim_end_matches('=');
    if unpadded.len() % 4 == 1 || unpadded.contains('=') {
        return Err(integrity_failure());
    }
    if padding > 0 && value.len() % 4 != 0 {
        return Err(integrity_failure());
    }
    let maximum_decoded = unpadded
        .len()
        .checked_add(3)
        .and_then(|value| value.checked_div(4))
        .and_then(|value| value.checked_mul(3))
        .ok_or_else(integrity_failure)?;
    if maximum_decoded > maximum_bytes.saturating_add(2) {
        return Err(integrity_failure());
    }
    let mut output = Vec::with_capacity(maximum_decoded.min(maximum_bytes));
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in unpadded.bytes() {
        let value = base64url_value(byte).ok_or_else(integrity_failure)?;
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            if output.len() == maximum_bytes {
                return Err(integrity_failure());
            }
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
        if bits == 0 {
            accumulator = 0;
        } else {
            accumulator &= (1_u32 << bits) - 1;
        }
    }
    if bits > 0 && accumulator != 0 {
        return Err(integrity_failure());
    }
    Ok(output)
}

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' | b'+' => Some(62),
        b'_' | b'/' => Some(63),
        _ => None,
    }
}

fn parse_gmail_cursor(cursor: &QueryCursor) -> Result<&str, QueryPortError> {
    cursor
        .as_str()
        .strip_prefix(GMAIL_CURSOR_PREFIX)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid_cursor)
}

fn gmail_query_cursor(token: &str) -> Result<QueryCursor, QueryPortError> {
    validate_gmail_token(token)?;
    QueryCursor::parse(format!("{GMAIL_CURSOR_PREFIX}{token}")).map_err(|_| integrity_failure())
}

fn parse_gmail_reference(reference: &str) -> Result<&str, QueryPortError> {
    let token = reference
        .strip_prefix(GMAIL_REFERENCE_PREFIX)
        .filter(|value| !value.is_empty())
        .ok_or_else(integrity_failure)?;
    validate_gmail_token(token)?;
    Ok(token)
}

fn validate_gmail_token(value: &str) -> Result<(), QueryPortError> {
    if value.is_empty()
        || value.len() > 500
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(integrity_failure());
    }
    Ok(())
}

fn message_endpoint(message_id: &str, format: &str) -> String {
    let mut endpoint = String::with_capacity(GMAIL_MESSAGES_ENDPOINT.len() + message_id.len() + 80);
    endpoint.push_str(GMAIL_MESSAGES_ENDPOINT);
    endpoint.push('/');
    push_percent_encoded(&mut endpoint, message_id);
    endpoint.push_str("?format=");
    endpoint.push_str(format);
    if format == "metadata" {
        endpoint.push_str("&metadataHeaders=Subject&metadataHeaders=From");
    }
    endpoint
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, QueryPortError> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| integrity_failure())?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value.parse::<usize>().map_err(|_| integrity_failure())?;
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

fn invalid_cursor() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::InvalidCursor)
}

fn integrity_failure() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

fn dependency_unavailable() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{decode_base64url, gmail_query_cursor, parse_gmail_cursor, push_percent_encoded};

    #[test]
    fn gmail_cursor_is_provider_scoped() -> Result<(), Box<dyn std::error::Error>> {
        let cursor = gmail_query_cursor("token-123")?;
        assert_eq!(parse_gmail_cursor(&cursor)?, "token-123");
        Ok(())
    }

    #[test]
    fn gmail_base64url_decoder_is_bounded_and_canonical() -> Result<(), Box<dyn std::error::Error>>
    {
        assert_eq!(decode_base64url("SGVsbG8td29ybGQ", 64)?, b"Hello-world");
        assert_eq!(decode_base64url("SGVsbG8=", 64)?, b"Hello");
        assert!(decode_base64url("A", 64).is_err());
        assert!(decode_base64url("SGVsbG8=", 4).is_err());
        assert!(decode_base64url("SGVsbG8===", 64).is_err());
        Ok(())
    }

    #[test]
    fn gmail_query_encoding_is_rfc3986_safe() {
        let mut output = String::new();
        push_percent_encoded(&mut output, "a b+c/=");
        assert_eq!(output, "a%20b%2Bc%2F%3D");
    }
}
