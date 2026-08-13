use application_ports::query::{QueryPage, QueryPortError, QueryPortErrorClass};
use application_ports::query_mail_provider::{
    MAX_MAIL_BODY_BYTES, MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use mailbox_domain::MailboxBinding;
use profile_platform_primitives::{ActorContext, ClientId, UnixMillis};
use serde::Deserialize;
use worker::{Fetch, Headers, Method, Request, RequestInit};
use zeroize::Zeroize;

use crate::cloud_mailbox_secrets::{
    MicrosoftGraphCredential, refresh_microsoft_graph_credential,
};
use crate::microsoft_graph_authorization::D1MicrosoftGraphAuthorization;
use crate::microsoft_graph_cursor::{resolve_query_cursor, store_query_cursor};

const GRAPH_MESSAGES_ENDPOINT: &str = "https://graph.microsoft.com/v1.0/me/messages";
const GRAPH_REFERENCE_PREFIX: &str = "graph:";
const MAX_GRAPH_QUERY_PAGE_SIZE: u16 = 25;
const MAX_GRAPH_LIST_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_GRAPH_MESSAGE_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_GRAPH_FIELD_BYTES: usize = 8 * 1024;
const MAX_GRAPH_MESSAGE_ID_BYTES: usize = 480;

#[derive(Debug, Deserialize)]
struct GraphMessageListResponse {
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
    #[serde(default)]
    value: Vec<GraphMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphMessage {
    id: String,
    subject: Option<String>,
    #[serde(rename = "from")]
    from_field: Option<GraphRecipient>,
    received_date_time: String,
    body: Option<GraphItemBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphRecipient {
    email_address: GraphEmailAddress,
}

#[derive(Debug, Deserialize)]
struct GraphEmailAddress {
    name: Option<String>,
    address: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphItemBody {
    content_type: String,
    content: String,
}

pub async fn search_microsoft_graph_messages(
    env: &worker::Env,
    binding: &MailboxBinding,
    request: &SearchClientMailboxMessagesRequest,
    credential: &MicrosoftGraphCredential,
    authorization: &D1MicrosoftGraphAuthorization,
    actor: &ActorContext,
    client_id: &ClientId,
) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
    if !authorization
        .recheck_client_query(actor, client_id, binding.binding_id())
        .await?
    {
        return Ok(QueryPage::empty());
    }

    let page_size = request
        .page()
        .limit()
        .value()
        .min(MAX_GRAPH_QUERY_PAGE_SIZE);
    let invalid_cursor_on_bad_request = request.page().cursor().is_some();
    let endpoint = match request.page().cursor() {
        Some(cursor) => resolve_query_cursor(env, binding, cursor).await?,
        None => initial_search_endpoint(request, page_size),
    };

    let Some(bytes) = graph_json_get(
        env,
        binding,
        &endpoint,
        credential,
        authorization,
        actor,
        client_id,
        MAX_GRAPH_LIST_RESPONSE_BYTES,
        invalid_cursor_on_bad_request,
        false,
    )
    .await?
    else {
        return Ok(QueryPage::empty());
    };
    let parsed: GraphMessageListResponse =
        serde_json::from_slice(&bytes).map_err(|_| integrity_failure())?;
    if parsed.value.len() > usize::from(page_size) {
        return Err(integrity_failure());
    }

    let mut items = Vec::with_capacity(parsed.value.len());
    for message in parsed.value {
        items.push(summary_from_graph(binding, &message)?);
    }
    let next_cursor = match parsed.next_link {
        Some(link) => Some(store_query_cursor(env, binding, &link).await?),
        None => None,
    };
    Ok(QueryPage::new(items, next_cursor))
}

pub async fn get_microsoft_graph_message(
    env: &worker::Env,
    binding: &MailboxBinding,
    provider_reference: &str,
    credential: &MicrosoftGraphCredential,
    authorization: &D1MicrosoftGraphAuthorization,
    actor: &ActorContext,
    client_id: &ClientId,
) -> Result<Option<MailMessageBody>, QueryPortError> {
    if !authorization
        .recheck_client_query(actor, client_id, binding.binding_id())
        .await?
    {
        return Ok(None);
    }
    let message_id = parse_graph_reference(provider_reference)?;
    let endpoint = message_endpoint(message_id);
    let Some(bytes) = graph_json_get(
        env,
        binding,
        &endpoint,
        credential,
        authorization,
        actor,
        client_id,
        MAX_GRAPH_MESSAGE_RESPONSE_BYTES,
        false,
        true,
    )
    .await?
    else {
        return Ok(None);
    };
    let message: GraphMessage = serde_json::from_slice(&bytes).map_err(|_| integrity_failure())?;
    if message.id != message_id {
        return Err(integrity_failure());
    }
    let summary = summary_from_graph(binding, &message)?;
    let (text_body, html_body) = graph_body(message.body)?;
    MailMessageBody::new(summary, text_body, html_body)
        .map(Some)
        .map_err(|_| integrity_failure())
}

fn initial_search_endpoint(request: &SearchClientMailboxMessagesRequest, page_size: u16) -> String {
    let mut endpoint = String::from(GRAPH_MESSAGES_ENDPOINT);
    endpoint.push_str("?$select=id,subject,from,receivedDateTime&$top=");
    endpoint.push_str(&page_size.to_string());
    if let Some(term) = request.term() {
        endpoint.push_str("&$search=");
        let search = graph_search_expression(term.as_str());
        push_percent_encoded(&mut endpoint, &search);
    }
    endpoint
}

fn graph_search_expression(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len() + 2);
    escaped.push('"');
    for character in term.chars() {
        if matches!(character, '\\' | '"') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped.push('"');
    escaped
}

fn message_endpoint(message_id: &str) -> String {
    let mut endpoint = String::from(GRAPH_MESSAGES_ENDPOINT);
    endpoint.push('/');
    push_percent_encoded(&mut endpoint, message_id);
    endpoint.push_str("?$select=id,subject,from,receivedDateTime,body");
    endpoint
}

#[allow(clippy::too_many_arguments)]
async fn graph_json_get(
    env: &worker::Env,
    binding: &MailboxBinding,
    endpoint: &str,
    credential: &MicrosoftGraphCredential,
    authorization: &D1MicrosoftGraphAuthorization,
    actor: &ActorContext,
    client_id: &ClientId,
    maximum_bytes: usize,
    invalid_cursor_on_bad_request: bool,
    prefer_text_body: bool,
) -> Result<Option<Vec<u8>>, QueryPortError> {
    if !authorization
        .recheck_client_query(actor, client_id, binding.binding_id())
        .await?
    {
        return Err(dependency_unavailable());
    }
    let mut response = send_graph_get(endpoint, credential.access_token(), prefer_text_body).await?;
    if response.status_code() == 401 {
        let refreshed = refresh_microsoft_graph_credential(env, binding)
            .await
            .map_err(map_provider_error)?;
        if !authorization
            .recheck_client_query(actor, client_id, binding.binding_id())
            .await?
        {
            return Err(dependency_unavailable());
        }
        response = send_graph_get(endpoint, refreshed.access_token(), prefer_text_body).await?;
    }
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

async fn send_graph_get(
    endpoint: &str,
    access_token: &str,
    prefer_text_body: bool,
) -> Result<worker::Response, QueryPortError> {
    validate_graph_endpoint(endpoint)?;
    let headers = Headers::new();
    let mut authorization = String::with_capacity(7 + access_token.len());
    authorization.push_str("Bearer ");
    authorization.push_str(access_token);
    let header_result = headers.set("authorization", &authorization);
    authorization.zeroize();
    header_result.map_err(|_| integrity_failure())?;
    headers
        .set("accept", "application/json")
        .map_err(|_| integrity_failure())?;
    if prefer_text_body {
        headers
            .set("prefer", "outlook.body-content-type=\"text\"")
            .map_err(|_| integrity_failure())?;
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(endpoint, &init).map_err(|_| integrity_failure())?;
    Fetch::Request(request)
        .send()
        .await
        .map_err(|_| dependency_unavailable())
}

fn validate_graph_endpoint(endpoint: &str) -> Result<(), QueryPortError> {
    if endpoint.len() > 16 * 1024
        || endpoint.chars().any(char::is_control)
        || !endpoint.starts_with("https://graph.microsoft.com/v1.0/")
    {
        return Err(integrity_failure());
    }
    Ok(())
}

fn summary_from_graph(
    binding: &MailboxBinding,
    message: &GraphMessage,
) -> Result<MailMessageSummary, QueryPortError> {
    validate_graph_message_id(&message.id)?;
    let subject = bounded_optional_field(message.subject.as_deref())?;
    let sender = message
        .from_field
        .as_ref()
        .and_then(|from| {
            from.email_address
                .address
                .as_deref()
                .or(from.email_address.name.as_deref())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let sender = bounded_optional_field(sender.as_deref())?;
    let received_at = parse_rfc3339_millis(&message.received_date_time)?;
    let reference = MailboxMessageReference::new(
        binding.binding_id().clone(),
        format!("{GRAPH_REFERENCE_PREFIX}{}", message.id),
    )
    .map_err(|_| integrity_failure())?;
    Ok(MailMessageSummary::new(
        reference,
        subject,
        sender,
        received_at,
    ))
}

fn graph_body(body: Option<GraphItemBody>) -> Result<(Option<String>, Option<String>), QueryPortError> {
    let Some(body) = body else {
        return Ok((None, None));
    };
    if body.content.len() > MAX_MAIL_BODY_BYTES {
        return Err(integrity_failure());
    }
    if body.content_type.eq_ignore_ascii_case("text") {
        Ok(((!body.content.is_empty()).then_some(body.content), None))
    } else if body.content_type.eq_ignore_ascii_case("html") {
        Ok((None, (!body.content.is_empty()).then_some(body.content)))
    } else {
        Err(integrity_failure())
    }
}

fn bounded_optional_field(value: Option<&str>) -> Result<Option<String>, QueryPortError> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    if value.is_some_and(|value| {
        value.len() > MAX_GRAPH_FIELD_BYTES
            || value
                .chars()
                .any(|character| character.is_control() && character != '\t')
    }) {
        return Err(integrity_failure());
    }
    Ok(value.map(str::to_owned))
}

fn validate_graph_message_id(value: &str) -> Result<(), QueryPortError> {
    if value.is_empty()
        || value.len() > MAX_GRAPH_MESSAGE_ID_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(integrity_failure());
    }
    Ok(())
}

fn parse_graph_reference(reference: &str) -> Result<&str, QueryPortError> {
    let value = reference
        .strip_prefix(GRAPH_REFERENCE_PREFIX)
        .ok_or_else(invalid_cursor)?;
    validate_graph_message_id(value).map_err(|_| invalid_cursor())?;
    Ok(value)
}

fn parse_rfc3339_millis(value: &str) -> Result<UnixMillis, QueryPortError> {
    let (date, rest) = value.split_once('T').ok_or_else(integrity_failure)?;
    let mut date_parts = date.split('-');
    let year = parse_i64(date_parts.next())?;
    let month = parse_i64(date_parts.next())?;
    let day = parse_i64(date_parts.next())?;
    if date_parts.next().is_some() || !(1970..=9999).contains(&year) {
        return Err(integrity_failure());
    }

    let (time, offset_seconds) = split_time_offset(rest)?;
    let mut time_parts = time.split(':');
    let hour = parse_i64(time_parts.next())?;
    let minute = parse_i64(time_parts.next())?;
    let second_fraction = time_parts.next().ok_or_else(integrity_failure)?;
    if time_parts.next().is_some() || hour > 23 || minute > 59 {
        return Err(integrity_failure());
    }
    let (second_text, fraction_text) = second_fraction
        .split_once('.')
        .map_or((second_fraction, None), |(second, fraction)| {
            (second, Some(fraction))
        });
    let second = parse_i64(Some(second_text))?;
    if second > 59 {
        return Err(integrity_failure());
    }
    let millis = parse_fraction_millis(fraction_text)?;
    let days = days_from_civil(year, month, day).ok_or_else(integrity_failure)?;
    let seconds = days
        .checked_mul(86_400)
        .and_then(|value| value.checked_add(hour * 3_600 + minute * 60 + second))
        .and_then(|value| value.checked_sub(offset_seconds))
        .ok_or_else(integrity_failure)?;
    if seconds < 0 {
        return Err(integrity_failure());
    }
    let total = u64::try_from(seconds)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000))
        .and_then(|value| value.checked_add(millis))
        .ok_or_else(integrity_failure)?;
    Ok(UnixMillis::new(total))
}

fn split_time_offset(value: &str) -> Result<(&str, i64), QueryPortError> {
    if let Some(time) = value.strip_suffix('Z') {
        return Ok((time, 0));
    }
    let offset_index = value
        .char_indices()
        .rev()
        .find(|(_, character)| matches!(character, '+' | '-'))
        .map(|(index, _)| index)
        .ok_or_else(integrity_failure)?;
    let (time, offset) = value.split_at(offset_index);
    if offset.len() != 6 || offset.as_bytes().get(3) != Some(&b':') {
        return Err(integrity_failure());
    }
    let sign = if offset.starts_with('+') { 1_i64 } else { -1_i64 };
    let hours = offset[1..3]
        .parse::<i64>()
        .map_err(|_| integrity_failure())?;
    let minutes = offset[4..6]
        .parse::<i64>()
        .map_err(|_| integrity_failure())?;
    if hours > 23 || minutes > 59 {
        return Err(integrity_failure());
    }
    Ok((time, sign * (hours * 3_600 + minutes * 60)))
}

fn parse_fraction_millis(value: Option<&str>) -> Result<u64, QueryPortError> {
    let Some(value) = value else {
        return Ok(0);
    };
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(integrity_failure());
    }
    let mut millis = 0_u64;
    for (index, byte) in value.bytes().take(3).enumerate() {
        let digit = u64::from(byte - b'0');
        millis += digit
            * match index {
                0 => 100,
                1 => 10,
                _ => 1,
            };
    }
    Ok(millis)
}

fn parse_i64(value: Option<&str>) -> Result<i64, QueryPortError> {
    value
        .ok_or_else(integrity_failure)?
        .parse::<i64>()
        .map_err(|_| integrity_failure())
}

fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

const fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 0,
    }
}

fn push_percent_encoded(target: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            target.push(char::from(byte));
        } else {
            target.push('%');
            target.push(char::from(HEX[usize::from(byte >> 4)]));
            target.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
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

fn map_provider_error(error: application_ports::mailboxes::MailboxProviderPortError) -> QueryPortError {
    match error {
        application_ports::mailboxes::MailboxProviderPortError::IntegrityFailure => {
            integrity_failure()
        }
        application_ports::mailboxes::MailboxProviderPortError::Failure(_) => {
            dependency_unavailable()
        }
    }
}

const fn invalid_cursor() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::InvalidCursor)
}

const fn integrity_failure() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

const fn dependency_unavailable() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        graph_search_expression, parse_rfc3339_millis, push_percent_encoded,
        validate_graph_endpoint,
    };

    #[test]
    fn graph_endpoint_validation_preserves_full_provider_url_without_cross_origin_follow() {
        assert!(
            validate_graph_endpoint(
                "https://graph.microsoft.com/v1.0/me/messages?$skiptoken=opaque-state"
            )
            .is_ok()
        );
        assert!(
            validate_graph_endpoint(
                "https://graph.microsoft.com.evil.example/v1.0/me/messages?$skiptoken=x"
            )
            .is_err()
        );
    }

    #[test]
    fn search_expression_is_quoted_and_percent_encoding_is_deterministic() {
        assert_eq!(graph_search_expression("subject:test"), "\"subject:test\"");
        assert_eq!(
            graph_search_expression("a\"b"),
            "\"a\\\"b\""
        );
        let mut encoded = String::new();
        push_percent_encoded(&mut encoded, "\"subject:test\"");
        assert_eq!(encoded, "%22subject%3Atest%22");
    }

    #[test]
    fn graph_datetime_parser_handles_utc_fraction_and_offset() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            parse_rfc3339_millis("1970-01-01T00:00:00Z")?.value(),
            0
        );
        assert_eq!(
            parse_rfc3339_millis("1970-01-01T00:00:00.123Z")?.value(),
            123
        );
        assert_eq!(
            parse_rfc3339_millis("1970-01-01T01:00:00+01:00")?.value(),
            0
        );
        assert!(parse_rfc3339_millis("2026-02-30T00:00:00Z").is_err());
        Ok(())
    }
}
