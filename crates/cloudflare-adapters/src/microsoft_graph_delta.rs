use application_ports::mailboxes::{
    MailboxObservation, MailboxProviderPortError,
};
use mailbox_domain::{
    MailboxBinding, MailboxJob, MailboxProviderFailure, MailboxProviderFailureClass,
};
use profile_platform_primitives::{ActorContext, UnixMillis};
use serde::Deserialize;
use serde_json::Value;
use worker::{Date, Fetch, Headers, Method, Request, RequestInit};
use zeroize::Zeroize;

use crate::cloud_mailbox_secrets::{
    MicrosoftGraphCredential, refresh_microsoft_graph_credential,
};
use crate::microsoft_graph_authorization::D1MicrosoftGraphAuthorization;
use crate::microsoft_graph_delta_cursor::{
    MicrosoftGraphDeltaCursorError, resolve_delta_cursor, store_delta_cursor,
};

const GRAPH_INBOX_DELTA_ENDPOINT: &str =
    "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta?$select=id&$top=100";
const MAX_GRAPH_DELTA_PAGE_SIZE: usize = 100;
const MAX_GRAPH_DELTA_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_GRAPH_ENDPOINT_BYTES: usize = 16 * 1024;
const GRAPH_ORIGIN: &str = "https://graph.microsoft.com/v1.0/";

#[derive(Debug, Deserialize)]
struct GraphDeltaPage {
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    delta_link: Option<String>,
    #[serde(default)]
    value: Vec<Value>,
}

pub async fn check_microsoft_graph_mailbox(
    env: &worker::Env,
    binding: &MailboxBinding,
    job: &MailboxJob,
    credential: &MicrosoftGraphCredential,
    authorization: &D1MicrosoftGraphAuthorization,
    actor: &ActorContext,
) -> Result<MailboxObservation, MailboxProviderPortError> {
    let mut reseeded = false;
    let (endpoint, started_from_cursor) = match job.cursor() {
        Some(cursor) => match resolve_delta_cursor(env, binding, cursor).await {
            Ok(endpoint) => (endpoint, true),
            Err(MicrosoftGraphDeltaCursorError::Stale) => {
                reseeded = true;
                (GRAPH_INBOX_DELTA_ENDPOINT.to_owned(), false)
            }
            Err(MicrosoftGraphDeltaCursorError::Provider(error)) => return Err(error),
        },
        None => (GRAPH_INBOX_DELTA_ENDPOINT.to_owned(), false),
    };

    let now = UnixMillis::new(Date::now().as_millis());
    let mut refreshed = None;
    let mut response = authorized_graph_get(
        env,
        binding,
        &endpoint,
        credential,
        &mut refreshed,
        authorization,
        actor,
    )
    .await?;

    if started_from_cursor && matches!(response.status_code(), 400 | 410) {
        reseeded = true;
        response = authorized_graph_get(
            env,
            binding,
            GRAPH_INBOX_DELTA_ENDPOINT,
            credential,
            &mut refreshed,
            authorization,
            actor,
        )
        .await?;
    }

    match response.status_code() {
        200 => {}
        404 => {
            return MailboxObservation::new(
                binding.binding_id().clone(),
                "GRAPH_NOT_FOUND",
                0,
                job.cursor().map(str::to_owned),
            )
            .map_err(|_| MailboxProviderPortError::IntegrityFailure);
        }
        401 => return Err(provider_failure(MailboxProviderFailureClass::Authentication, None)),
        403 => return Err(provider_failure(MailboxProviderFailureClass::ProviderPolicy, None)),
        429 => {
            let retry_at = retry_after_hint(&response, now);
            return Err(provider_failure(
                MailboxProviderFailureClass::RateLimited,
                retry_at,
            ));
        }
        408 | 425 | 500..=599 => {
            return Err(provider_failure(
                MailboxProviderFailureClass::TransientDependency,
                None,
            ));
        }
        410 => {
            return Err(provider_failure(
                MailboxProviderFailureClass::TransientDependency,
                None,
            ));
        }
        400 => return Err(MailboxProviderPortError::IntegrityFailure),
        _ => return Err(provider_failure(MailboxProviderFailureClass::Permanent, None)),
    }

    if response_content_length_exceeds(&response, MAX_GRAPH_DELTA_RESPONSE_BYTES)? {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| provider_failure(MailboxProviderFailureClass::TransientDependency, None))?;
    if bytes.len() > MAX_GRAPH_DELTA_RESPONSE_BYTES {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    let page: GraphDeltaPage =
        serde_json::from_slice(&bytes).map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    if page.value.len() > MAX_GRAPH_DELTA_PAGE_SIZE {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    let provider_cursor = match (page.next_link, page.delta_link) {
        (Some(next), None) => next,
        (None, Some(delta)) => delta,
        _ => return Err(MailboxProviderPortError::IntegrityFailure),
    };
    let next_cursor = store_delta_cursor(env, binding, &provider_cursor).await?;
    let item_count =
        u32::try_from(page.value.len()).map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    MailboxObservation::new(
        binding.binding_id().clone(),
        if reseeded {
            "GRAPH_DELTA_RESEEDED"
        } else {
            "GRAPH_DELTA_OK"
        },
        item_count,
        Some(next_cursor),
    )
    .map_err(|_| MailboxProviderPortError::IntegrityFailure)
}

async fn authorized_graph_get(
    env: &worker::Env,
    binding: &MailboxBinding,
    endpoint: &str,
    initial_credential: &MicrosoftGraphCredential,
    refreshed: &mut Option<MicrosoftGraphCredential>,
    authorization: &D1MicrosoftGraphAuthorization,
    actor: &ActorContext,
) -> Result<worker::Response, MailboxProviderPortError> {
    authorization.recheck_job(actor, binding.binding_id()).await?;
    let credential = refreshed.as_ref().unwrap_or(initial_credential);
    let mut response = send_graph_get(endpoint, credential.access_token()).await?;
    if response.status_code() != 401 || refreshed.is_some() {
        return Ok(response);
    }

    let replacement = refresh_microsoft_graph_credential(env, binding)
        .await
        .map_err(map_refresh_failure)?;
    *refreshed = Some(replacement);
    authorization.recheck_job(actor, binding.binding_id()).await?;
    let credential = refreshed
        .as_ref()
        .ok_or(MailboxProviderPortError::IntegrityFailure)?;
    response = send_graph_get(endpoint, credential.access_token()).await?;
    Ok(response)
}

async fn send_graph_get(
    endpoint: &str,
    access_token: &str,
) -> Result<worker::Response, MailboxProviderPortError> {
    validate_graph_endpoint(endpoint)?;
    let headers = Headers::new();
    let mut authorization = String::with_capacity(7 + access_token.len());
    authorization.push_str("Bearer ");
    authorization.push_str(access_token);
    let header_result = headers.set("authorization", &authorization);
    authorization.zeroize();
    header_result.map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    headers
        .set("accept", "application/json")
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(endpoint, &init)
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    Fetch::Request(request)
        .send()
        .await
        .map_err(|_| provider_failure(MailboxProviderFailureClass::TransientDependency, None))
}

fn map_refresh_failure(error: MailboxProviderPortError) -> MailboxProviderPortError {
    match error {
        MailboxProviderPortError::IntegrityFailure => MailboxProviderPortError::IntegrityFailure,
        MailboxProviderPortError::Failure(_) => {
            provider_failure(MailboxProviderFailureClass::Authentication, None)
        }
    }
}

fn retry_after_hint(response: &worker::Response, now: UnixMillis) -> Option<UnixMillis> {
    let value = response.headers().get("retry-after").ok().flatten()?;
    parse_retry_after(value.trim(), now)
}

fn parse_retry_after(value: &str, now: UnixMillis) -> Option<UnixMillis> {
    if let Ok(seconds) = value.parse::<u64>() {
        return now
            .value()
            .checked_add(seconds.checked_mul(1_000)?)
            .map(UnixMillis::new);
    }
    parse_imf_fixdate(value)
}

fn parse_imf_fixdate(value: &str) -> Option<UnixMillis> {
    let parts: Vec<&str> = value.split_ascii_whitespace().collect();
    if parts.len() != 6 || !parts[0].ends_with(',') || parts[5] != "GMT" {
        return None;
    }
    let day = parts[1].parse::<i64>().ok()?;
    let month = match parts[2] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year = parts[3].parse::<i64>().ok()?;
    let mut time = parts[4].split(':');
    let hour = time.next()?.parse::<i64>().ok()?;
    let minute = time.next()?.parse::<i64>().ok()?;
    let second = time.next()?.parse::<i64>().ok()?;
    if time.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(hour.checked_mul(3_600)?)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)?;
    if seconds < 0 {
        return None;
    }
    u64::try_from(seconds)
        .ok()?
        .checked_mul(1_000)
        .map(UnixMillis::new)
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

fn validate_graph_endpoint(endpoint: &str) -> Result<(), MailboxProviderPortError> {
    if endpoint.is_empty()
        || endpoint.len() > MAX_GRAPH_ENDPOINT_BYTES
        || endpoint.chars().any(char::is_control)
        || !endpoint.starts_with(GRAPH_ORIGIN)
    {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    Ok(())
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

fn provider_failure(
    class: MailboxProviderFailureClass,
    retry_at: Option<UnixMillis>,
) -> MailboxProviderPortError {
    MailboxProviderFailure::new(class, retry_at).map_or(
        MailboxProviderPortError::IntegrityFailure,
        MailboxProviderPortError::Failure,
    )
}

#[cfg(test)]
mod tests {
    use super::{GRAPH_INBOX_DELTA_ENDPOINT, parse_retry_after, validate_graph_endpoint};
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn delta_initial_sync_is_per_folder_and_bounded() {
        assert!(GRAPH_INBOX_DELTA_ENDPOINT.contains("/mailFolders/inbox/messages/delta"));
        assert!(GRAPH_INBOX_DELTA_ENDPOINT.contains("$select=id"));
        assert!(GRAPH_INBOX_DELTA_ENDPOINT.contains("$top=100"));
        assert!(!GRAPH_INBOX_DELTA_ENDPOINT.contains("/me/messages/delta"));
    }

    #[test]
    fn retry_after_supports_seconds_and_imf_fixdate() {
        assert_eq!(
            parse_retry_after("30", UnixMillis::new(1_000)),
            Some(UnixMillis::new(31_000))
        );
        assert_eq!(
            parse_retry_after("Thu, 01 Jan 1970 00:01:00 GMT", UnixMillis::new(0)),
            Some(UnixMillis::new(60_000))
        );
        assert_eq!(parse_retry_after("invalid", UnixMillis::new(0)), None);
    }

    #[test]
    fn delta_provider_cursor_cannot_escape_graph_origin() {
        assert!(validate_graph_endpoint(GRAPH_INBOX_DELTA_ENDPOINT).is_ok());
        assert!(
            validate_graph_endpoint(
                "https://graph.microsoft.com.evil.example/v1.0/me/mailFolders/inbox/messages/delta"
            )
            .is_err()
        );
    }
}
