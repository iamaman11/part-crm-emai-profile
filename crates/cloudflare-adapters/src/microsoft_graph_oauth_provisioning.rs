use crate::cloud_mailbox_secrets::MAILBOX_SECRET_RESOLVER_BINDING;
use application_ports::microsoft_graph_oauth_onboarding::{
    MicrosoftGraphOAuthAuthorizationCode, MicrosoftGraphOAuthAuthorizationUrl,
    MicrosoftGraphOAuthCallbackTarget, MicrosoftGraphOAuthCeremonyId,
    MicrosoftGraphOAuthProvisioningError, MicrosoftGraphOAuthProvisioningErrorClass,
    MicrosoftGraphOAuthProvisioningPort, MicrosoftGraphOAuthStartReceipt, MicrosoftGraphOAuthState,
};
use mailbox_domain::MailboxOnboardingVersion;
use profile_platform_primitives::{
    ActorContext, ActorId, MailboxOnboardingId, SecretHandle, TenantId, UnixMillis,
};
use serde::Deserialize;
use worker::{Env, Headers, Method, RequestInit};

const START_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/oauth/start";
const INSPECT_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/oauth/inspect";
const COMPLETE_ENDPOINT: &str = "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/oauth/complete";
const DENY_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/microsoft-graph/oauth/deny";
const DISCARD_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/discard";
const GRAPH_DELEGATED_SCOPES: &str = "openid offline_access https://graph.microsoft.com/Mail.Read";
const PKCE_METHOD: &str = "S256";
const MAX_RESPONSE_BYTES: usize = 16 * 1024;

pub struct CloudflareMicrosoftGraphOAuthProvisioningPort<'a> {
    env: &'a Env,
}

impl<'a> CloudflareMicrosoftGraphOAuthProvisioningPort<'a> {
    #[must_use]
    pub const fn new(env: &'a Env) -> Self {
        Self { env }
    }
}

impl MicrosoftGraphOAuthProvisioningPort for CloudflareMicrosoftGraphOAuthProvisioningPort<'_> {
    async fn start(
        &self,
        actor: &ActorContext,
        onboarding_id: &MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
    ) -> Result<MicrosoftGraphOAuthStartReceipt, MicrosoftGraphOAuthProvisioningError> {
        let headers = base_actor_headers(actor)?;
        set_header(
            &headers,
            "x-profile-mailbox-onboarding-id",
            onboarding_id.as_str(),
        )?;
        set_header(
            &headers,
            "x-profile-mailbox-onboarding-version",
            &expected_version.value().to_string(),
        )?;
        set_header(&headers, "x-profile-oauth-scope", GRAPH_DELEGATED_SCOPES)?;
        set_header(&headers, "x-profile-oauth-pkce-method", PKCE_METHOD)?;
        let response = fetch(self.env, START_ENDPOINT, headers, StartStatus::Start).await?;
        let document: StartDocument = parse_json(response).await?;
        let ceremony_id = MicrosoftGraphOAuthCeremonyId::parse(document.ceremony_id)
            .map_err(|_| integrity_error())?;
        let authorization_url =
            MicrosoftGraphOAuthAuthorizationUrl::parse(document.authorization_url)
                .map_err(|_| integrity_error())?;
        Ok(MicrosoftGraphOAuthStartReceipt::new(
            ceremony_id,
            authorization_url,
            UnixMillis::new(document.expires_at_ms),
        ))
    }

    async fn inspect(
        &self,
        state: &MicrosoftGraphOAuthState,
    ) -> Result<MicrosoftGraphOAuthCallbackTarget, MicrosoftGraphOAuthProvisioningError> {
        let headers = common_headers()?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        let response = fetch(self.env, INSPECT_ENDPOINT, headers, StartStatus::Callback).await?;
        let document: InspectDocument = parse_json(response).await?;
        let tenant_id = TenantId::parse(document.tenant_id).map_err(|_| integrity_error())?;
        let onboarding_id =
            MailboxOnboardingId::parse(document.onboarding_id).map_err(|_| integrity_error())?;
        let starter_actor_id =
            ActorId::parse(document.starter_actor_id).map_err(|_| integrity_error())?;
        Ok(MicrosoftGraphOAuthCallbackTarget::new(
            tenant_id,
            onboarding_id,
            MailboxOnboardingVersion::new(document.expected_version),
            starter_actor_id,
            UnixMillis::new(document.expires_at_ms),
        ))
    }

    async fn complete(
        &self,
        actor: &ActorContext,
        state: &MicrosoftGraphOAuthState,
        authorization_code: MicrosoftGraphOAuthAuthorizationCode,
    ) -> Result<SecretHandle, MicrosoftGraphOAuthProvisioningError> {
        let headers = base_actor_headers(actor)?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        set_header(
            &headers,
            "x-profile-oauth-authorization-code",
            authorization_code.as_str(),
        )?;
        set_header(&headers, "x-profile-oauth-pkce-method", PKCE_METHOD)?;
        let response = fetch(self.env, COMPLETE_ENDPOINT, headers, StartStatus::Callback).await?;
        let document: CompletionDocument = parse_json(response).await?;
        SecretHandle::parse(document.secret_handle).map_err(|_| integrity_error())
    }

    async fn deny(
        &self,
        actor: &ActorContext,
        state: &MicrosoftGraphOAuthState,
    ) -> Result<(), MicrosoftGraphOAuthProvisioningError> {
        let headers = base_actor_headers(actor)?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        fetch(self.env, DENY_ENDPOINT, headers, StartStatus::Callback)
            .await
            .map(|_| ())
    }

    async fn discard(
        &self,
        actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> Result<(), MicrosoftGraphOAuthProvisioningError> {
        let headers = base_actor_headers(actor)?;
        set_header(
            &headers,
            "x-profile-mailbox-secret-handle",
            secret_handle.as_str(),
        )?;
        set_header(&headers, "x-profile-mailbox-provider", "MICROSOFT_GRAPH")?;
        fetch(self.env, DISCARD_ENDPOINT, headers, StartStatus::Discard)
            .await
            .map(|_| ())
    }
}

#[derive(Clone, Copy)]
enum StartStatus {
    Start,
    Callback,
    Discard,
}

async fn fetch(
    env: &Env,
    endpoint: &str,
    headers: Headers,
    operation: StartStatus,
) -> Result<worker::Response, MicrosoftGraphOAuthProvisioningError> {
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

fn common_headers() -> Result<Headers, MicrosoftGraphOAuthProvisioningError> {
    let headers = Headers::new();
    set_header(&headers, "accept", "application/json")?;
    set_header(&headers, "cache-control", "no-store")?;
    Ok(headers)
}

fn base_actor_headers(
    actor: &ActorContext,
) -> Result<Headers, MicrosoftGraphOAuthProvisioningError> {
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
) -> Result<(), MicrosoftGraphOAuthProvisioningError> {
    headers.set(name, value).map_err(|_| integrity_error())
}

fn map_status(
    status: u16,
    operation: StartStatus,
) -> Result<(), MicrosoftGraphOAuthProvisioningError> {
    match status {
        200 | 204 => Ok(()),
        404 | 401 | 403 => Err(not_found_error()),
        410 => Err(MicrosoftGraphOAuthProvisioningError::new(
            MicrosoftGraphOAuthProvisioningErrorClass::Expired,
        )),
        409 | 412 if matches!(operation, StartStatus::Callback) => {
            Err(MicrosoftGraphOAuthProvisioningError::new(
                MicrosoftGraphOAuthProvisioningErrorClass::ReplayRejected,
            ))
        }
        409 | 412 => Err(MicrosoftGraphOAuthProvisioningError::new(
            MicrosoftGraphOAuthProvisioningErrorClass::Conflict,
        )),
        408 | 425 | 429 | 500..=599 => Err(dependency_error()),
        400 | 422 => Err(integrity_error()),
        _ => Err(MicrosoftGraphOAuthProvisioningError::new(
            MicrosoftGraphOAuthProvisioningErrorClass::InternalFailure,
        )),
    }
}

async fn parse_json<T: for<'de> Deserialize<'de>>(
    mut response: worker::Response,
) -> Result<T, MicrosoftGraphOAuthProvisioningError> {
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
) -> Result<bool, MicrosoftGraphOAuthProvisioningError> {
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
    onboarding_id: String,
    expected_version: u64,
    starter_actor_id: String,
    expires_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompletionDocument {
    secret_handle: String,
}

const fn integrity_error() -> MicrosoftGraphOAuthProvisioningError {
    MicrosoftGraphOAuthProvisioningError::new(
        MicrosoftGraphOAuthProvisioningErrorClass::IntegrityFailure,
    )
}

const fn dependency_error() -> MicrosoftGraphOAuthProvisioningError {
    MicrosoftGraphOAuthProvisioningError::new(
        MicrosoftGraphOAuthProvisioningErrorClass::DependencyUnavailable,
    )
}

const fn not_found_error() -> MicrosoftGraphOAuthProvisioningError {
    MicrosoftGraphOAuthProvisioningError::new(MicrosoftGraphOAuthProvisioningErrorClass::NotFound)
}

#[cfg(test)]
mod tests {
    use super::{GRAPH_DELEGATED_SCOPES, PKCE_METHOD, StartStatus, map_status};
    use application_ports::microsoft_graph_oauth_onboarding::MicrosoftGraphOAuthProvisioningErrorClass;

    #[test]
    fn graph_oauth_scope_is_read_only_and_pkce_is_s256() {
        let scopes: Vec<&str> = GRAPH_DELEGATED_SCOPES.split_ascii_whitespace().collect();
        assert!(scopes.contains(&"openid"));
        assert!(scopes.contains(&"offline_access"));
        assert!(scopes.contains(&"https://graph.microsoft.com/Mail.Read"));
        assert!(!GRAPH_DELEGATED_SCOPES.contains("Mail.Send"));
        assert_eq!(PKCE_METHOD, "S256");
    }

    #[test]
    fn callback_replay_and_expiry_fail_closed() {
        assert!(map_status(200, StartStatus::Callback).is_ok());
        assert_eq!(
            map_status(409, StartStatus::Callback)
                .expect_err("callback replay must fail")
                .class(),
            MicrosoftGraphOAuthProvisioningErrorClass::ReplayRejected
        );
        assert_eq!(
            map_status(410, StartStatus::Callback)
                .expect_err("expired callback must fail")
                .class(),
            MicrosoftGraphOAuthProvisioningErrorClass::Expired
        );
    }

    #[test]
    fn start_conflict_is_not_misreported_as_callback_replay() {
        assert_eq!(
            map_status(409, StartStatus::Start)
                .expect_err("start conflict must fail")
                .class(),
            MicrosoftGraphOAuthProvisioningErrorClass::Conflict
        );
    }
}
