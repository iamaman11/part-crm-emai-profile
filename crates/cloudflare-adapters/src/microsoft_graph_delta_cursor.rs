use application_ports::mailboxes::MailboxProviderPortError;
use mailbox_domain::{MailboxBinding, MailboxProviderFailureClass};
use serde::Deserialize;
use serde_json::{Map, Value};
use worker::Env;

use crate::cloud_mailbox_secrets::{MAILBOX_SECRET_RESOLVER_BINDING, provider_error};
use crate::resolver_request::signed_resolver_request;

const CURSOR_STORE_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/cursors/store";
const CURSOR_RESOLVE_ENDPOINT: &str = "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/cursors/resolve";
const DELTA_CURSOR_PREFIX: &str = "graph-delta:";
const MAX_CURSOR_HANDLE_LENGTH: usize = 192;
const MAX_PROVIDER_CURSOR_LENGTH: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = 24 * 1024;
const GRAPH_ORIGIN: &str = "https://graph.microsoft.com/v1.0/";

pub enum MicrosoftGraphDeltaCursorError {
    Stale,
    Provider(MailboxProviderPortError),
}

pub async fn store_delta_cursor(
    env: &Env,
    binding: &MailboxBinding,
    provider_cursor: &str,
) -> Result<String, MailboxProviderPortError> {
    validate_provider_cursor(provider_cursor)?;
    let response = resolver_post(
        env,
        binding,
        CURSOR_STORE_ENDPOINT,
        cursor_payload(binding, "providerCursor", provider_cursor),
    )
    .await?;
    map_store_status(response.status_code())?;
    let document: StoreCursorDocument = parse_json(response).await?;
    validate_handle(&document.cursor_handle)?;
    Ok(format!("{DELTA_CURSOR_PREFIX}{}", document.cursor_handle))
}

pub async fn resolve_delta_cursor(
    env: &Env,
    binding: &MailboxBinding,
    cursor: &str,
) -> Result<String, MicrosoftGraphDeltaCursorError> {
    let handle = cursor
        .strip_prefix(DELTA_CURSOR_PREFIX)
        .ok_or(MicrosoftGraphDeltaCursorError::Stale)?;
    validate_handle(handle).map_err(MicrosoftGraphDeltaCursorError::Provider)?;
    let response = resolver_post(
        env,
        binding,
        CURSOR_RESOLVE_ENDPOINT,
        cursor_payload(binding, "cursorHandle", handle),
    )
    .await
    .map_err(MicrosoftGraphDeltaCursorError::Provider)?;
    match response.status_code() {
        200 => {}
        400 | 404 | 410 => return Err(MicrosoftGraphDeltaCursorError::Stale),
        status => {
            return Err(MicrosoftGraphDeltaCursorError::Provider(status_error(
                status,
            )));
        }
    }
    let document: ResolveCursorDocument = parse_json(response)
        .await
        .map_err(MicrosoftGraphDeltaCursorError::Provider)?;
    validate_provider_cursor(&document.provider_cursor)
        .map_err(MicrosoftGraphDeltaCursorError::Provider)?;
    Ok(document.provider_cursor)
}

async fn resolver_post(
    env: &Env,
    binding: &MailboxBinding,
    endpoint: &str,
    payload: Map<String, Value>,
) -> Result<worker::Response, MailboxProviderPortError> {
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    let init = signed_resolver_request(
        env,
        endpoint,
        binding.tenant_id().as_str(),
        "microsoft_graph_cursor",
        payload,
    )
    .map_err(|_| MailboxProviderPortError::IntegrityFailure)?;
    resolver
        .fetch(endpoint, Some(init))
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))
}

fn cursor_payload(binding: &MailboxBinding, name: &str, value: &str) -> Map<String, Value> {
    Map::from_iter([
        (
            "mailboxBindingId".to_owned(),
            Value::String(binding.binding_id().as_str().to_owned()),
        ),
        (name.to_owned(), Value::String(value.to_owned())),
    ])
}

fn map_store_status(status: u16) -> Result<(), MailboxProviderPortError> {
    if status == 200 {
        Ok(())
    } else {
        Err(status_error(status))
    }
}

fn status_error(status: u16) -> MailboxProviderPortError {
    match status {
        408 | 425 | 500..=599 => provider_error(MailboxProviderFailureClass::TransientDependency),
        429 => provider_error(MailboxProviderFailureClass::RateLimited),
        401 | 403 | 409 | 422 => provider_error(MailboxProviderFailureClass::ProviderPolicy),
        _ => MailboxProviderPortError::IntegrityFailure,
    }
}

async fn parse_json<T: for<'de> Deserialize<'de>>(
    mut response: worker::Response,
) -> Result<T, MailboxProviderPortError> {
    if response_content_length_exceeds(&response, MAX_RESPONSE_BYTES)? {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| provider_error(MailboxProviderFailureClass::TransientDependency))?;
    if bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    serde_json::from_slice(&bytes).map_err(|_| MailboxProviderPortError::IntegrityFailure)
}

fn validate_handle(handle: &str) -> Result<(), MailboxProviderPortError> {
    if handle.len() < 8
        || handle.len() > MAX_CURSOR_HANDLE_LENGTH
        || !handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(MailboxProviderPortError::IntegrityFailure);
    }
    Ok(())
}

fn validate_provider_cursor(cursor: &str) -> Result<(), MailboxProviderPortError> {
    if cursor.is_empty()
        || cursor.len() > MAX_PROVIDER_CURSOR_LENGTH
        || cursor.chars().any(char::is_control)
        || !cursor.starts_with(GRAPH_ORIGIN)
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreCursorDocument {
    cursor_handle: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolveCursorDocument {
    provider_cursor: String,
}

#[cfg(test)]
mod tests {
    use super::{DELTA_CURSOR_PREFIX, validate_handle, validate_provider_cursor};

    #[test]
    fn delta_cursor_keeps_provider_state_out_of_domain_cursor() {
        assert!(
            validate_provider_cursor(
                "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta?$deltatoken=opaque"
            )
            .is_ok()
        );
        assert!(validate_handle("delta_01JOPAQUE").is_ok());
        let public = format!("{DELTA_CURSOR_PREFIX}delta_01JOPAQUE");
        assert!(!public.contains("graph.microsoft.com"));
        assert!(!public.contains("deltatoken"));
    }
}
