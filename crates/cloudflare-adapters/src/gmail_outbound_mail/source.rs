use crate::gmail_send_credential::GmailSendCredential;
use application_ports::outbound_mail::{
    MailAddress, MailRecipients, MailSubject, OutboundMailIntent, OutboundMailOperation,
    ProviderMessageReference,
};
use serde::Deserialize;
use worker::{Fetch, Headers, Method, Request, RequestInit};
use zeroize::Zeroize;

const GMAIL_MESSAGES_ENDPOINT: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const GMAIL_PROFILE_ENDPOINT: &str = "https://gmail.googleapis.com/gmail/v1/users/me/profile";
const GMAIL_REFERENCE_PREFIX: &str = "gmail:";
const MAX_METADATA_RESPONSE_BYTES: usize = 96 * 1024;
const MAX_PROFILE_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;
const MAX_PROVIDER_TOKEN_BYTES: usize = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreparationFailure {
    ReauthRequired,
    RetryableNotSent,
    Rejected,
}

impl core::fmt::Display for PreparationFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Gmail outbound message preparation failed")
    }
}

impl std::error::Error for PreparationFailure {}

pub(super) struct GmailMessageContext {
    pub(super) from: MailAddress,
    pub(super) recipients: MailRecipients,
    pub(super) subject: Option<MailSubject>,
    pub(super) thread_id: Option<String>,
    pub(super) in_reply_to: Option<String>,
    pub(super) references: Option<String>,
}

pub(super) async fn resolve_message_context(
    intent: &OutboundMailIntent,
    credential: &GmailSendCredential,
) -> Result<GmailMessageContext, PreparationFailure> {
    let from = load_profile_address(credential).await?;
    match intent.operation() {
        OutboundMailOperation::New { recipients } => Ok(GmailMessageContext {
            from,
            recipients: recipients.clone(),
            subject: intent.subject().cloned(),
            thread_id: None,
            in_reply_to: None,
            references: None,
        }),
        OutboundMailOperation::Forward { source, recipients } => {
            parse_gmail_reference(source.provider_reference())?;
            Ok(GmailMessageContext {
                from,
                recipients: recipients.clone(),
                subject: intent.subject().cloned(),
                thread_id: None,
                in_reply_to: None,
                references: None,
            })
        }
        OutboundMailOperation::Reply { source } => {
            let metadata = load_source_metadata(source.provider_reference(), credential).await?;
            build_reply_context(from, intent, metadata, false)
        }
        OutboundMailOperation::ReplyAll { source } => {
            let metadata = load_source_metadata(source.provider_reference(), credential).await?;
            build_reply_context(from, intent, metadata, true)
        }
    }
}

fn build_reply_context(
    from: MailAddress,
    intent: &OutboundMailIntent,
    metadata: SourceMetadata,
    reply_all: bool,
) -> Result<GmailMessageContext, PreparationFailure> {
    let source_subject = metadata.header("Subject")?;
    if let Some(requested) = intent.subject() {
        if source_subject.as_deref() != Some(requested.as_str()) {
            return Err(PreparationFailure::Rejected);
        }
    }
    let subject = source_subject
        .map(MailSubject::parse)
        .transpose()
        .map_err(|_| PreparationFailure::Rejected)?;
    let message_id = metadata
        .header("Message-ID")?
        .filter(|value| !value.is_empty())
        .ok_or(PreparationFailure::Rejected)?;
    validate_reply_reference(&message_id)?;

    let primary = metadata
        .header("Reply-To")?
        .filter(|value| !value.is_empty())
        .or(metadata.header("From")?)
        .ok_or(PreparationFailure::Rejected)?;
    let mut to = parse_address_list(&primary)?;
    let mut cc = Vec::new();
    if reply_all {
        if let Some(value) = metadata.header("To")? {
            append_unique(&mut to, parse_address_list(&value)?, Some(&from));
        }
        if let Some(value) = metadata.header("Cc")? {
            append_unique(&mut cc, parse_address_list(&value)?, Some(&from));
        }
    }
    remove_address(&mut to, &from);
    remove_duplicates_against(&mut cc, &to);
    let recipients = MailRecipients::new(to, cc, Vec::new())
        .map_err(|_| PreparationFailure::Rejected)?;

    let references = match metadata.header("References")? {
        Some(existing) if !existing.is_empty() => {
            let combined = format!("{existing} {message_id}");
            validate_reply_reference(&combined)?;
            combined
        }
        _ => message_id.clone(),
    };

    Ok(GmailMessageContext {
        from,
        recipients,
        subject,
        thread_id: Some(metadata.thread_id),
        in_reply_to: Some(message_id),
        references: Some(references),
    })
}

async fn load_profile_address(
    credential: &GmailSendCredential,
) -> Result<MailAddress, PreparationFailure> {
    let bytes = gmail_json_get(
        GMAIL_PROFILE_ENDPOINT,
        credential,
        MAX_PROFILE_RESPONSE_BYTES,
        false,
    )
    .await?
    .ok_or(PreparationFailure::Rejected)?;
    let profile: GmailProfile =
        serde_json::from_slice(&bytes).map_err(|_| PreparationFailure::Rejected)?;
    MailAddress::parse(profile.email_address).map_err(|_| PreparationFailure::Rejected)
}

async fn load_source_metadata(
    reference: &ProviderMessageReference,
    credential: &GmailSendCredential,
) -> Result<SourceMetadata, PreparationFailure> {
    let message_id = parse_gmail_reference(reference)?;
    let mut endpoint = String::with_capacity(GMAIL_MESSAGES_ENDPOINT.len() + message_id.len() + 180);
    endpoint.push_str(GMAIL_MESSAGES_ENDPOINT);
    endpoint.push('/');
    push_percent_encoded(&mut endpoint, message_id);
    endpoint.push_str("?format=metadata");
    for header in [
        "Subject",
        "From",
        "Reply-To",
        "To",
        "Cc",
        "Message-ID",
        "References",
    ] {
        endpoint.push_str("&metadataHeaders=");
        endpoint.push_str(header);
    }
    let bytes = gmail_json_get(&endpoint, credential, MAX_METADATA_RESPONSE_BYTES, true)
        .await?
        .ok_or(PreparationFailure::Rejected)?;
    let metadata: SourceMetadata =
        serde_json::from_slice(&bytes).map_err(|_| PreparationFailure::Rejected)?;
    validate_provider_token(&metadata.id)?;
    validate_provider_token(&metadata.thread_id)?;
    if metadata.id != message_id {
        return Err(PreparationFailure::Rejected);
    }
    Ok(metadata)
}

async fn gmail_json_get(
    endpoint: &str,
    credential: &GmailSendCredential,
    maximum_bytes: usize,
    not_found_is_none: bool,
) -> Result<Option<Vec<u8>>, PreparationFailure> {
    let headers = Headers::new();
    let mut authorization = String::with_capacity(7 + credential.access_token().len());
    authorization.push_str("Bearer ");
    authorization.push_str(credential.access_token());
    let header_result = headers.set("authorization", &authorization);
    authorization.zeroize();
    header_result.map_err(|_| PreparationFailure::Rejected)?;
    headers
        .set("accept", "application/json")
        .map_err(|_| PreparationFailure::Rejected)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request =
        Request::new_with_init(endpoint, &init).map_err(|_| PreparationFailure::Rejected)?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| PreparationFailure::RetryableNotSent)?;
    match response.status_code() {
        200 => {}
        404 if not_found_is_none => return Ok(None),
        401 | 403 => return Err(PreparationFailure::ReauthRequired),
        408 | 425 | 429 | 500..=59I => return Err(PreparationFailure::RetryableNotSent),
        _ => return Err(PreparationFailure::Rejected),
    }
    if response_content_length_exceeds(&response, maximum_bytes)? {
        return Err(PreparationFailure::Rejected);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| PreparationFailure::RetryableNotSent)?;
    if bytes.is_empty() || bytes.len() > maximum_bytes {
        return Err(PreparationFailure::Rejected);
    }
    Ok(Some(bytes))
}

fn parse_gmail_reference(
    reference: &ProviderMessageReference,
) -> Result<&str, PreparationFailure> {
    let token = reference
        .as_str()
        .strip_prefix(GMAIL_REFERENCE_PREFIX)
        .filter(|value| !value.is_empty())
        .ok_or(PreparationFailure::Rejected)?;
    validate_provider_token(token)?;
    Ok(token)
}

fn validate_provider_token(value: &str) -> Result<(), PreparationFailure> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_TOKEN_BYTES
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(PreparationFailure::Rejected);
    }
    Ok(())
}

fn validate_reply_reference(value: &str) -> Result<(), PreparationFailure> {
    if value.is_empty()
        || value.len() > MAX_HEADER_VALUE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(PreparationFailure::Rejected);
    }
    Ok(())
}

fn parse_address_list(value: &str) -> Result<Vec<MailAddress>, PreparationFailure> {
    if value.is_empty() || value.len() > MAX_HEADER_VALUE_BYTES || value.chars().any(char::is_control)
    {
        return Err(PreparationFailure::Rejected);
    }
    let mut items = Vec::new();
    let mut start = 0_usize;
    let mut quoted = false;
    let mut angle_depth = 0_u8;
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'"' if angle_depth == 0 => quoted = !quoted,
            b'<' if !quoted => {
                angle_depth = angle_depth
                    .checked_add(1)
                    .ok_or(PreparationFailure::Rejected)?;
                if angle_depth > 1 {
                    return Err(PreparationFailure::Rejected);
                }
            }
            b'>' if !quoted => {
                if angle_depth != 1 {
                    return Err(PreparationFailure::Rejected);
                }
                angle_depth = 0;
            }
            b',' if !quoted && angle_depth == 0 => {
                push_address_token(&mut items, &value[start..index])?;
                start = index + 1;
            }
            _ => {}
        }
    }
    if quoted || angle_depth != 0 {
        return Err(PreparationFailure::Rejected);
    }
    push_address_token(&mut items, &value[start..])?;
    if items.is_empty() {
        return Err(PreparationFailure::Rejected);
    }
    Ok(items)
}

fn push_address_token(
    output: &mut Vec<MailAddress>,
    token: &str,
) -> Result<(), PreparationFailure> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(());
    }
    let mailbox = match (token.rfind('<'), token.rfind('>')) {
        (Some(open), Some(close)) if open < close && close == token.len() - 1 => {
            token[open + 1..close].trim()
        }
        (None, None) => token,
        _ => return Err(PreparationFailure::Rejected),
    };
    let address = MailAddress::parse(mailbox.to_owned()).map_err(|_| PreparationFailure::Rejected)?;
    if !output
        .iter()
        .any(|existing| existing.as_str().eq_ignore_ascii_case(address.as_str()))
    {
        output.push(address);
    }
    Ok(())
}

fn append_unique(
    target: &mut Vec<MailAddress>,
    incoming: Vec<MailAddress>,
    excluded: Option<&MailAddress>,
) {
    for address in incoming {
        if excluded.is_some_and(|value| value.as_str().eq_ignore_ascii_case(address.as_str())) {
            continue;
        }
        if !target
            .iter()
            .any(|existing| existing.as_str().eq_ignore_ascii_case(address.as_str()))
        {
            target.push(address);
        }
    }
}

fn remove_address(values: &mut Vec<MailAddress>, address: &MailAddress) {
    values.retain(|value| !value.as_str().eq_ignore_ascii_case(address.as_str()));
}

fn remove_duplicates_against(values: &mut Vec<MailAddress>, primary: &[MailAddress]) {
    values.retain(|value| {
        !primary
            .iter()
            .any(|existing| existing.as_str().eq_ignore_ascii_case(value.as_str()))
    });
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, PreparationFailure> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| PreparationFailure::Rejected)?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value
        .parse::<usize>()
        .map_err(|_| PreparationFailure::Rejected)?;
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailProfile {
    email_address: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceMetadata {
    id: String,
    thread_id: String,
    payload: SourcePayload,
}

impl SourceMetadata {
    fn header(&self, name: &str) -> Result<Option<String>, PreparationFailure> {
        let value = self
            .payload
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.trim().to_owned());
        if value.as_ref().is_some_and(|value| {
            value.len() > MAX_HEADER_VALUE_BYTES || value.chars().any(char::is_control)
        }) {
            return Err(PreparationFailure::Rejected);
        }
        Ok(value.filter(|value| !value.is_empty()))
    }
}

#[derive(Deserialize)]
struct SourcePayload {
    #[serde(default)]
    headers: Vec<SourceHeader>,
}

#[derive(Deserialize)]
struct SourceHeader {
    name: String,
    value: String,
}

#[cfg(test)]
mod tests {
    use super::{PreparationFailure, parse_address_list, parse_gmail_reference};
    use application_ports::outbound_mail::ProviderMessageReference;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn provider_reference_is_exact_and_bounded() -> TestResult {
        let reference = ProviderMessageReference::parse("gmail:abc123")?;
        assert_eq!(parse_gmail_reference(&reference), Ok("abc123"));
        let foreign = ProviderMessageReference::parse("graph:abc123")?;
        assert_eq!(
            parse_gmail_reference(&foreign),
            Err(PreparationFailure::Rejected)
        );
        Ok(())
    }

    #[test]
    fn address_parser_accepts_common_display_names_and_rejects_controls() -> TestResult {
        let parsed = parse_address_list("Alice <alice@example.com>, bob@example.com")?;
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].as_str(), "alice@example.com");
        assert!(parse_address_list("alice@example.com\r\nx@example.com").is_err());
        Ok(())
    }
}
