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
use serde_json::{Map, Value};
use worker::Env;

use crate::resolver_request::{oauth_callback_tenant, signed_resolver_request};

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
        let payload = Map::from_iter([
            actor_field(actor),
            string_field("mailboxBindingId", binding_id.as_str()),
            (
                "mailboxBindingVersion".to_owned(),
                Value::from(expected_version.value()),
            ),
            string_field("oauthScope", GMAIL_SEND_SCOPE),
            ("oauthIncludeGrantedScopes".to_owned(), Value::Bool(true)),
        ]);
        let response = fetch(
            self.env,
            START_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            payload,
            Operation::Start,
        )
        .await?;
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
        let tenant_id = oauth_callback_tenant(state.as_str()).map_err(|_| integrity_error())?;
        let payload = Map::from_iter([string_field("oauthState", state.as_str())]);
        let response = fetch(
            self.env,
            INSPECT_ENDPOINT,
            tenant_id,
            payload,
            Operation::Callback,
        )
        .await?;
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
        require_actor_state_tenant(actor, state.as_str())?;
        let payload = Map::from_iter([
            actor_field(actor),
            string_field("oauthState", state.as_str()),
            string_field("oauthAuthorizationCode", authorization_code.as_str()),
        ]);
        fetch(
            self.env,
            COMPLETE_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            payload,
            Operation::Callback,
        )
        .await
        .map(|_| ())
    }

    async fn deny(
        &self,
        actor: &ActorContext,
        state: &GmailOAuthState,
    ) -> Result<(), GmailSendAuthorizationError> {
        require_actor_state_tenant(actor, state.as_str())?;
        let payload = Map::from_iter([
            actor_field(actor),
            string_field("oauthState", state.as_str()),
        ]);
        fetch(
            self.env,
            DENY_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            payload,
            Operation::Callback,
        )
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
    tenant_id: &str,
    payload: Map<String, Value>,
    operation: Operation,
) -> Result<worker::Response, GmailSendAuthorizationError> {
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| integrity_error())?;
    let init = signed_resolver_request(env, endpoint, tenant_id, "gmail_send", payload)
        .map_err(|_| integrity_error())?;
    let response = resolver
        .fetch(endpoint, Some(init))
        .await
        .map_err(|_| dependency_error())?;
    map_status(response.status_code(), operation)?;
    Ok(response)
}

fn actor_field(actor: &ActorContext) -> (String, Value) {
    string_field("actorId", actor.actor_id().as_str())
}

fn string_field(name: &str, value: &str) -> (String, Value) {
    (name.to_owned(), Value::String(value.to_owned()))
}

fn require_actor_state_tenant(
    actor: &ActorContext,
    state: &str,
) -> Result<(), GmailSendAuthorizationError> {
    if oauth_callback_tenant(state).map_err(|_| integrity_error())?
        != actor.tenant_scope().tenant_id().as_str()
    {
        return Err(integrity_error());
    }
    Ok(())
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
            map_status(409, Operation::Callback).map_err(|error| error.class()),
            Err(GmailSendAuthorizationErrorClass::ReplayRejected)
        );
        assert_eq!(
            map_status(400, Operation::Callback).map_err(|error| error.class()),
            Err(GmailSendAuthorizationErrorClass::ProviderDenied)
        );
    }
}
