use application_ports::query::{QueryCursor, QueryPortError, QueryPortErrorClass};
use mailbox_domain::MailboxBinding;
use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, RequestInit};
use zeroize::Zeroize;

use crate::cloud_mailbox_secrets::MAILBOX_SECRET_RESOLVER_BINDING;

const CURSOR_STORE_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/cursors/store";
const CURSOR_RESOLVE_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/cursors/resolve";
const QUERY_CURSOR_PREFIX: &str = "graph-page:";
const MAX_CURSOR_HANDLE_LENGTH: usize = 192;
const MAX_PROVIDER_CURSOR_LENGTH: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = 24 * 1024;
const GRAPH_ORIGIN: &str = "https://graph.microsoft.com/v1.0/";

pub async fn store_query_cursor(
    env: &Env,
    binding: &MailboxBinding,
    provider_cursor: &str,
) -> Result<QueryCursor, QueryPortError> {
    validate_provider_cursor(provider_cursor)?;
    let response = resolver_post(
        env,
        binding,
        CURSOR_STORE_ENDPOINT,
        &serde_json::json!({"providerCursor": provider_cursor}).to_string(),
        false,
    )
    .await?;
    let document: StoreCursorDocument = parse_json(response).await?;
    validate_handle(&document.cursor_handle)?;
    QueryCursor::parse(format!("{QUERY_CURSOR_PREFIX}{}", document.cursor_handle))
        .map_err(|_| integrity_failure())
}

pub async fn resolve_query_cursor(
    env: &Env,
    binding: &MailboxBinding,
    cursor: &QueryCursor,
) -> Result<String, QueryPortError> {
    let handle = cursor
        .as_str()
        .strip_prefix(QUERY_CURSOR_PREFIX)
        .ok_or_else(invalid_cursor)?;
    validate_handle(handle)?;
    let response = resolver_post(
        env,
        binding,
        CURSOR_RESOLVE_ENDPOINT,
        &serde_json::json!({"cursorHandle": handle}).to_string(),
        true,
    )
    .await?;
    let document: ResolveCursorDocument = parse_json(response).await?;
    validate_provider_cursor(&document.provider_cursor)?;
    Ok(document.provider_cursor)
}

async fn resolver_post(
    env: &Env,
    binding: &MailboxBinding,
    endpoint: &str,
    body: &str,
    invalid_cursor_on_gone: bool,
) -> Result<worker::Response, QueryPortError> {
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| integrity_failure())?;
    let headers = Headers::new();
    headers
        .set("accept", "application/json")
        .map_err(|_| integrity_failure())?;
    headers
        .set("content-type", "application/json")
        .map_err(|_| integrity_failure())?;
    headers
        .set("cache-control", "no-store")
        .map_err(|_| integrity_failure())?;
    headers
        .set("x-profile-tenant-id", binding.tenant_id().as_str())
        .map_err(|_| integrity_failure())?;
    headers
        .set("x-profile-mailbox-binding-id", binding.binding_id().as_str())
        .map_err(|_| integrity_failure())?;

    let mut body = body.to_owned();
    let js_body = JsValue::from_str(&body);
    body.zeroize();
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(js_body));
    let response = resolver
        .fetch(endpoint, Some(init))
        .await
        .map_err(|_| dependency_unavailable())?;
    match response.status_code() {
        200 => Ok(response),
        400 | 404 | 410 if invalid_cursor_on_gone => Err(invalid_cursor()),
        408 | 425 | 429 | 500..=599 => Err(dependency_unavailable()),
        401 | 403 | 409 | 422 => Err(integrity_failure()),
        _ => Err(integrity_failure()),
    }
}

async fn parse_json<T: for<'de> Deserialize<'de>>(
    mut response: worker::Response,
) -> Result<T, QueryPortError> {
    if response_content_length_exceeds(&response, MAX_RESPONSE_BYTES)? {
        return Err(integrity_failure());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| dependency_unavailable())?;
    if bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES {
        return Err(integrity_failure());
    }
    serde_json::from_slice(&bytes).map_err(|_| integrity_failure())
}

fn validate_handle(handle: &str) -> Result<(), QueryPortError> {
    if handle.len() < 8
        || handle.len() > MAX_CURSOR_HANDLE_LENGTH
        || !handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(invalid_cursor());
    }
    Ok(())
}

fn validate_provider_cursor(cursor: &str) -> Result<(), QueryPortError> {
    if cursor.is_empty()
        || cursor.len() > MAX_PROVIDER_CURSOR_LENGTH
        || cursor.chars().any(char::is_control)
        || !cursor.starts_with(GRAPH_ORIGIN)
    {
        return Err(integrity_failure());
    }
    Ok(())
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
    use super::{validate_handle, validate_provider_cursor};

    #[test]
    fn provider_cursor_is_full_graph_url_but_public_handle_is_short_and_opaque() {
        assert!(
            validate_provider_cursor(
                "https://graph.microsoft.com/v1.0/me/messages?$skiptoken=opaque-provider-state"
            )
            .is_ok()
        );
        assert!(
            validate_provider_cursor(
                "https://evil.example/v1.0/me/messages?$skiptoken=opaque-provider-state"
            )
            .is_err()
        );
        assert!(validate_handle("cursor_01JOPAQUE").is_ok());
        assert!(validate_handle("raw?skiptoken=forbidden").is_err());
    }
}
