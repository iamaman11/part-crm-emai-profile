use crate::cloud_mailbox_secrets::MAILBOX_SECRET_RESOLVER_BINDING;
use application_ports::gmail_oauth_onboarding::{
    GmailOAuthAuthorizationCode, GmailOAuthAuthorizationUrl, GmailOAuthCeremonyId,
    GmailOAuthStartReceipt, GmailOAuthState,
};
use application_ports::gmail_send_authorization::{
    GmailSendAuthorizationCallbackTarget, GmailSendAuthorizationError,
    GmailSendAuthorizationErrorClass, GmailSendAuthorizationPort,
};
use profile_platform_primitives::{
    ActorContext, ActorId, AggregateVersion, MailboxBindingId, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::{Env, Headers, Method, RequestInit};

const START_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/gmail/send/oauth/start";
const INSPECT_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/gmail/send/oauth/inspect";
const COMPLETE_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/gmail/send/oauth/complete";
const DENY_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/gmail/send/oauth/deny";
const GMAIL_SEND_SCOPE: &str = "https://www.googleapis.com/auth/gmail.send";
const MAX_RESPONSE_BYTES: usize = 16 * 1024;

pub struct CloudflareGmailSendAuthorizationPort<'a> {
    env: &'a Env,
}

impl<'a> CloudflareGmailSendAuthorizationPort<'a> {
    #[must_use]
    pub const fn new(env: &'a Env) -> Self {
        Self { env }
    }
}

impl GmailSendAuthorizationPort for CloudflareGmailSendAuthorizationPort<'_> {
    async fn start(
        &self,
        actor: &ActorContext,
        binding_id: &MailboxBindingId,
        expected_version: AggregateVersion,
    ) -> Result<GmailOAuthStartReceipt, GmailSendAuthorizationError> {
        let headers = base_actor_headers(actor)?;
        set_header(&headers, "x-profile-mailbox-binding-id", binding_id.as_str())?;
        set_header(
            &headers,
            "x-profile-mailbox-binding-version",
            &expected_version.value().to_string(),
        )?;
        set_header(&headers, "x-profile-oauth-scope", GMAIL_SEND_SCOPE)?;
        set_header(
            &headers,
            "x-profile-oauth-include-granted-scopes",
            "true",
        )?;
        let response = fetch(self.env, START_ENDPOINT, headers, Operation::Start).await?;
        let document: StartDocument = parse_json(response).await?;
        let ceremony_id =
            GmailOAuthCeremonyId::parse(document.ceremony_id).map_err(|_| integrity_error())?;
        let authorization_url = GmailOAuthAuthorizationUrl::parse(document.authorization_url)
            .map_err(|_| integrity_error())?;
        Ok(GmailOAuthStartReceipt::new(
            ceremony_id,
            authorization_url,
            UnixMillis::new(document.expires_at_ms),
        ))
    }

    async fn inspect(
        &self,
        state: &GmailOAuthState,
    ) -> Result<GmailSendAuthorizationCallbackTarget, GmailSendAuthorizationError> {
        let headers = common_headers()?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        let response = fetch(self.env, INSPECT_ENDPOINT, headers, Operation::Callback).await?;
        let document: InspectDocument = parse_json(response).await?;
        Ok(GmailSendAuthorizationCallbackTarget::new(
            TenantId::parse(document.tenant_id).map_err(|_| integrity_error())?,
            MailboxBindingId::parse(document.binding_id).map_err(|_| integrity_error())?,
            AggregateVersion::new(document.expected_version).map_err(|_| integrity_error())?,
            ActorId::parse(document.starter_actor_id).map_err(|_| integrity_error())?,
            UnixMillis::new(document.expires_at_ms),
        ))
    }

    async fn complete(
        &self,
        actor: &ActorContext,
        state: &GmailOAuthState,
        authorization_code: GmailOAuthAuthorizationCode,
    ) -> Result<(), GmailSendAuthorizationError> {
        let headers = base_actor_headers(actor)?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        set_header(
            &headers,
            "x-profile-oauth-authorization-code",
            authorization_code.as_str(),
        )?;
        fetch(self.env, COMPLETE_ENDPOINT, headers, Operation::Callback)
            .await
            .map(|_| ())
    }

    async fn deny(
        &self,
        actor: &ActorContext,
        state: &GmailOAuthState,
    ) -> Result<(), GmailSendAuthorizationError> {
        let headers = base_actor_headers(actor)?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        fetch(self.env, DENY_ENDPOINT, headers, Operation::Callback)
            .await
            .map(|_| ())
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Start,
    Callback,
}

async fn fetch(
    env: &Env,
    endpoint: &str,
    headers: Headers,
    operation: Operation,
) -> Result<worker::Response, GmailSendAuthorizationError> {
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| integrity_error())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(headers);
    let response = resolver
        .fetch(endpoint, Some(init))
        .await
        .map_err(|_| dependency_error())?;
    map_status(response.status_code(), operation)?;
    Ok(response)
}

fn common_headers() -> Result<Headers, GmailSendAuthorizationError> {
    let headers = Headers::new();
    set_header(&headers, "accept", "application/json")?;
    set_header(&headers, "cache-control", "no-store")?;
    Ok(headers)
}

fn base_actor_headers(actor: &ActorContext) -> Result<Headers, GmailSendAuthorizationError> {
    let headers = common_headers()?;
    set_header(
        &headers,
        "x-profile-tenant-id",
        actor.tenant_scope().tenant_id().as_str(),
    )?;
    set_header(&headers, "x-profile-actor-id", actor.actor_id().as_str())?;
    Ok(headers)
}

fn set_header(
    headers: &Headers,
    name: &str,
    value: &str,
) -> Result<(), GmailSendAuthorizationError> {
    headers.set(name, value).map_err(|_| integrity_error())
}

fn map_status(status: u16, operation: Operation) -> Result<(), GmailSendAuthorizationError> {
    match status {
        200 | 204 => Ok(()),
        401 | 403 | 404 => Err(not_found_error()),
        410 => Err(GmailSendAuthorizationError::new(
            GmailSendAuthorizationErrorClass::Expired,
        )),
        409 | 412 if matches!(operation, Operation::Callback) => Err(
            GmailSendAuthorizationError::new(GmailSendAuthorizationErrorClass::ReplayRejected),
        ),
        409 | 412 => Err(GmailSendAuthorizationError::new(
            GmailSendAuthorizationErrorClass::Conflict,
        )),
        408 | 425 | 429 | 500..=599 => Err(dependency_error()),
        400 => Err(GmailSendAuthorizationError::new(
            GmailSendAuthorizationErrorClass::ProviderDenied,
        )),
        422 => Err(integrity_error()),
        _ => Err(GmailSendAuthorizationError::new(
            GmailSendAuthorizationErrorClass::InternalFailure,
        )),
    }
}

async fn parse_json<T: for<'de> Deserialize<'de>>(
    mut response: worker::Response,
) -> Result<T, GmailSendAuthorizationError> {
    if response_content_length_exceeds(&response, MAX_RESPONSE_BYTES)? {
        return Err(integrity_error());
    }
    let bytes = response.bytes().await.map_err(|_| dependency_error())?;
    if bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES {
        return Err(integrity_error());
    }
    serde_json::from_slice(&bytes).map_err(|_| integrity_error())
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, GmailSendAuthorizationError> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| integrity_error())?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value.parse::<usize>().map_err(|_| integrity_error())?;
    Ok(length > maximum)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartDocument {
    ceremony_id: String,
    authorization_url: String,
    expires_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InspectDocument {
    tenant_id: String,
    binding_id: String,
    expected_version: u64,
    starter_actor_id: String,
    expires_at_ms: u64,
}

const fn integrity_error() -> GmailSendAuthorizationError {
    GmailSendAuthorizationError::new(GmailSendAuthorizationErrorClass::IntegrityFailure)
}

const fn dependency_error() -> GmailSendAuthorizationError {
    GmailSendAuthorizationError::new(GmailSendAuthorizationErrorClass::DependencyUnavailable)
}

const fn not_found_error() -> GmailSendAuthorizationError {
    GmailSendAuthorizationError::new(GmailSendAuthorizationErrorClass::NotFound)
}

#[cfg(test)]
mod tests {
    use super::{Operation, map_status};
    use application_ports::gmail_send_authorization::GmailSendAuthorizationErrorClass;

    #[test]
    fn callback_replay_and_provider_denial_are_distinct() {
        assert_eq!(
            map_status(409, Operation::Callback)
                .expect_err("replay must fail")
                .class(),
            GmailSendAuthorizationErrorClass::ReplayRejected
        );
        assert_eq!(
            map_status(400, Operation::Callback)
                .expect_err("provider denial must fail")
                .class(),
            GmailSendAuthorizationErrorClass::ProviderDenied
        );
    }
}
