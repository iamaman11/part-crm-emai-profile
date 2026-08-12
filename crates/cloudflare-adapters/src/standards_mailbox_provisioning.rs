use crate::cloud_mailbox_secrets::MAILBOX_SECRET_RESOLVER_BINDING;
use application_ports::standards_mailbox_onboarding::{
    MicrosoftStandardsOAuthAuthorizationCode, MicrosoftStandardsOAuthAuthorizationUrl,
    MicrosoftStandardsOAuthCallbackTarget, MicrosoftStandardsOAuthCeremonyId,
    MicrosoftStandardsOAuthStartReceipt, MicrosoftStandardsOAuthState,
    StandardsMailboxAuthenticationMode, StandardsMailboxProvisioningError,
    StandardsMailboxProvisioningErrorClass, StandardsMailboxProvisioningPort,
    StandardsMailboxProvisioningReceipt, StandardsPasswordMailboxConfiguration,
    StandardsPasswordProtocolCredential,
};
use mailbox_domain::MailboxOnboardingVersion;
use profile_platform_primitives::{
    ActorContext, ActorId, IdempotencyKey, MailboxOnboardingId, SecretHandle, TenantId, UnixMillis,
};
use serde::{Deserialize, Serialize};
use worker::wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, RequestInit};
use zeroize::Zeroize;

const PASSWORD_PROVISION_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/standards/password/provision";
const MICROSOFT_START_ENDPOINT: &str = "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/standards/microsoft/oauth/start";
const MICROSOFT_INSPECT_ENDPOINT: &str = "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/standards/microsoft/oauth/inspect";
const MICROSOFT_COMPLETE_ENDPOINT: &str = "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/standards/microsoft/oauth/complete";
const MICROSOFT_DENY_ENDPOINT: &str = "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/standards/microsoft/oauth/deny";
const DISCARD_ENDPOINT: &str =
    "https://mailbox-secret-resolver.internal/v1/mailbox-credentials/discard";
const MICROSOFT_SCOPES: &str = "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access";
const MAX_RESPONSE_BYTES: usize = 16 * 1024;

pub struct CloudflareStandardsMailboxProvisioningPort<'a> {
    env: &'a Env,
}

impl<'a> CloudflareStandardsMailboxProvisioningPort<'a> {
    #[must_use]
    pub const fn new(env: &'a Env) -> Self {
        Self { env }
    }
}

impl StandardsMailboxProvisioningPort for CloudflareStandardsMailboxProvisioningPort<'_> {
    async fn provision_password(
        &self,
        actor: &ActorContext,
        onboarding_id: &MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
        idempotency_key: &IdempotencyKey,
        configuration: StandardsPasswordMailboxConfiguration,
    ) -> Result<StandardsMailboxProvisioningReceipt, StandardsMailboxProvisioningError> {
        let headers = base_onboarding_headers(actor, onboarding_id, expected_version)?;
        set_header(&headers, "content-type", "application/json")?;
        set_header(
            &headers,
            "x-profile-mailbox-authentication-mode",
            "PASSWORD",
        )?;
        set_header(
            &headers,
            "x-profile-idempotency-key",
            idempotency_key.as_str(),
        )?;
        let document = PasswordProvisionDocument::from(configuration);
        let mut body = serde_json::to_string(&document).map_err(|_| integrity_error())?;
        drop(document);
        let js_body = JsValue::from_str(&body);
        body.zeroize();
        let response = fetch(
            self.env,
            PASSWORD_PROVISION_ENDPOINT,
            headers,
            Operation::Provision,
            Some(js_body),
        )
        .await?;
        parse_provisioning_receipt(response, StandardsMailboxAuthenticationMode::Password).await
    }

    async fn start_microsoft_oauth(
        &self,
        actor: &ActorContext,
        onboarding_id: &MailboxOnboardingId,
        expected_version: MailboxOnboardingVersion,
    ) -> Result<MicrosoftStandardsOAuthStartReceipt, StandardsMailboxProvisioningError> {
        let headers = base_onboarding_headers(actor, onboarding_id, expected_version)?;
        set_header(
            &headers,
            "x-profile-mailbox-authentication-mode",
            "MICROSOFT_OAUTH2",
        )?;
        set_header(&headers, "x-profile-oauth-scopes", MICROSOFT_SCOPES)?;
        set_header(&headers, "x-profile-oauth-protocol", "IMAP_SMTP_XOAUTH2")?;
        let response = fetch(
            self.env,
            MICROSOFT_START_ENDPOINT,
            headers,
            Operation::Start,
            None,
        )
        .await?;
        let document: StartDocument = parse_json(response).await?;
        let ceremony_id = MicrosoftStandardsOAuthCeremonyId::parse(document.ceremony_id)
            .map_err(|_| integrity_error())?;
        let authorization_url =
            MicrosoftStandardsOAuthAuthorizationUrl::parse(document.authorization_url)
                .map_err(|_| integrity_error())?;
        Ok(MicrosoftStandardsOAuthStartReceipt::new(
            ceremony_id,
            authorization_url,
            UnixMillis::new(document.expires_at_ms),
        ))
    }

    async fn inspect_microsoft_oauth(
        &self,
        state: &MicrosoftStandardsOAuthState,
    ) -> Result<MicrosoftStandardsOAuthCallbackTarget, StandardsMailboxProvisioningError> {
        let headers = common_headers()?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        let response = fetch(
            self.env,
            MICROSOFT_INSPECT_ENDPOINT,
            headers,
            Operation::Callback,
            None,
        )
        .await?;
        let document: InspectDocument = parse_json(response).await?;
        let tenant_id = TenantId::parse(document.tenant_id).map_err(|_| integrity_error())?;
        let onboarding_id =
            MailboxOnboardingId::parse(document.onboarding_id).map_err(|_| integrity_error())?;
        let starter_actor_id =
            ActorId::parse(document.starter_actor_id).map_err(|_| integrity_error())?;
        Ok(MicrosoftStandardsOAuthCallbackTarget::new(
            tenant_id,
            onboarding_id,
            MailboxOnboardingVersion::new(document.expected_version),
            starter_actor_id,
            UnixMillis::new(document.expires_at_ms),
        ))
    }

    async fn complete_microsoft_oauth(
        &self,
        actor: &ActorContext,
        state: &MicrosoftStandardsOAuthState,
        authorization_code: MicrosoftStandardsOAuthAuthorizationCode,
    ) -> Result<StandardsMailboxProvisioningReceipt, StandardsMailboxProvisioningError> {
        let headers = base_actor_headers(actor)?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        set_header(
            &headers,
            "x-profile-oauth-authorization-code",
            authorization_code.as_str(),
        )?;
        set_header(&headers, "x-profile-oauth-scopes", MICROSOFT_SCOPES)?;
        let response = fetch(
            self.env,
            MICROSOFT_COMPLETE_ENDPOINT,
            headers,
            Operation::Callback,
            None,
        )
        .await?;
        parse_provisioning_receipt(
            response,
            StandardsMailboxAuthenticationMode::MicrosoftOAuth2,
        )
        .await
    }

    async fn deny_microsoft_oauth(
        &self,
        actor: &ActorContext,
        state: &MicrosoftStandardsOAuthState,
    ) -> Result<(), StandardsMailboxProvisioningError> {
        let headers = base_actor_headers(actor)?;
        set_header(&headers, "x-profile-oauth-state", state.as_str())?;
        fetch(
            self.env,
            MICROSOFT_DENY_ENDPOINT,
            headers,
            Operation::Callback,
            None,
        )
        .await
        .map(|_| ())
    }

    async fn discard(
        &self,
        actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> Result<(), StandardsMailboxProvisioningError> {
        let headers = base_actor_headers(actor)?;
        set_header(
            &headers,
            "x-profile-mailbox-secret-handle",
            secret_handle.as_str(),
        )?;
        set_header(&headers, "x-profile-mailbox-provider", "IMAP")?;
        fetch(
            self.env,
            DISCARD_ENDPOINT,
            headers,
            Operation::Discard,
            None,
        )
        .await
        .map(|_| ())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasswordProvisionDocument {
    imap: PasswordProtocolDocument,
    smtp: PasswordProtocolDocument,
}

impl From<StandardsPasswordMailboxConfiguration> for PasswordProvisionDocument {
    fn from(value: StandardsPasswordMailboxConfiguration) -> Self {
        let (imap, smtp) = value.into_parts();
        Self {
            imap: PasswordProtocolDocument::from(imap),
            smtp: PasswordProtocolDocument::from(smtp),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasswordProtocolDocument {
    host: String,
    port: u16,
    transport_security: &'static str,
    username: String,
    password: String,
}

impl From<StandardsPasswordProtocolCredential> for PasswordProtocolDocument {
    fn from(value: StandardsPasswordProtocolCredential) -> Self {
        let (endpoint, username, password) = value.into_parts();
        let transport_security = endpoint.transport_security().resolver_value();
        Self {
            host: endpoint.host().to_owned(),
            port: endpoint.port(),
            transport_security,
            username: username.into_inner(),
            password: password.into_inner(),
        }
    }
}

impl Drop for PasswordProtocolDocument {
    fn drop(&mut self) {
        self.username.zeroize();
        self.password.zeroize();
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Provision,
    Start,
    Callback,
    Discard,
}

async fn fetch(
    env: &Env,
    endpoint: &str,
    headers: Headers,
    operation: Operation,
    body: Option<JsValue>,
) -> Result<worker::Response, StandardsMailboxProvisioningError> {
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| integrity_error())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(headers);
    if body.is_some() {
        init.with_body(body);
    }
    let response = resolver
        .fetch(endpoint, Some(init))
        .await
        .map_err(|_| dependency_error())?;
    map_status(response.status_code(), operation)?;
    Ok(response)
}

fn common_headers() -> Result<Headers, StandardsMailboxProvisioningError> {
    let headers = Headers::new();
    set_header(&headers, "accept", "application/json")?;
    set_header(&headers, "cache-control", "no-store")?;
    Ok(headers)
}

fn base_actor_headers(actor: &ActorContext) -> Result<Headers, StandardsMailboxProvisioningError> {
    let headers = common_headers()?;
    set_header(
        &headers,
        "x-profile-tenant-id",
        actor.tenant_scope().tenant_id().as_str(),
    )?;
    set_header(&headers, "x-profile-actor-id", actor.actor_id().as_str())?;
    Ok(headers)
}

fn base_onboarding_headers(
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
) -> Result<Headers, StandardsMailboxProvisioningError> {
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
    set_header(&headers, "x-profile-mailbox-provider", "IMAP")?;
    Ok(headers)
}

fn set_header(
    headers: &Headers,
    name: &str,
    value: &str,
) -> Result<(), StandardsMailboxProvisioningError> {
    headers.set(name, value).map_err(|_| integrity_error())
}

fn map_status(status: u16, operation: Operation) -> Result<(), StandardsMailboxProvisioningError> {
    match status {
        200 | 204 => Ok(()),
        404 | 401 | 403 => Err(not_found_error()),
        410 => Err(StandardsMailboxProvisioningError::new(
            StandardsMailboxProvisioningErrorClass::Expired,
        )),
        409 | 412 if matches!(operation, Operation::Callback) => {
            Err(StandardsMailboxProvisioningError::new(
                StandardsMailboxProvisioningErrorClass::ReplayRejected,
            ))
        }
        409 | 412 => Err(StandardsMailboxProvisioningError::new(
            StandardsMailboxProvisioningErrorClass::Conflict,
        )),
        408 | 425 | 429 | 500..=599 => Err(dependency_error()),
        400 | 422 => Err(integrity_error()),
        _ => Err(StandardsMailboxProvisioningError::new(
            StandardsMailboxProvisioningErrorClass::InternalFailure,
        )),
    }
}

async fn parse_json<T: for<'de> Deserialize<'de>>(
    mut response: worker::Response,
) -> Result<T, StandardsMailboxProvisioningError> {
    if response_content_length_exceeds(&response, MAX_RESPONSE_BYTES)? {
        return Err(integrity_error());
    }
    let bytes = response.bytes().await.map_err(|_| dependency_error())?;
    if bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES {
        return Err(integrity_error());
    }
    serde_json::from_slice(&bytes).map_err(|_| integrity_error())
}

async fn parse_provisioning_receipt(
    response: worker::Response,
    expected_mode: StandardsMailboxAuthenticationMode,
) -> Result<StandardsMailboxProvisioningReceipt, StandardsMailboxProvisioningError> {
    let document: ProvisioningDocument = parse_json(response).await?;
    let mode = match document.authentication_mode.as_str() {
        "PASSWORD" => StandardsMailboxAuthenticationMode::Password,
        "MICROSOFT_OAUTH2" => StandardsMailboxAuthenticationMode::MicrosoftOAuth2,
        _ => return Err(integrity_error()),
    };
    if mode != expected_mode || !document.imap_read_search_ready || !document.smtp_send_ready {
        return Err(integrity_error());
    }
    let secret_handle =
        SecretHandle::parse(document.secret_handle).map_err(|_| integrity_error())?;
    Ok(StandardsMailboxProvisioningReceipt::new(
        secret_handle,
        mode,
        document.imap_read_search_ready,
        document.smtp_send_ready,
    ))
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, StandardsMailboxProvisioningError> {
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
struct ProvisioningDocument {
    secret_handle: String,
    authentication_mode: String,
    imap_read_search_ready: bool,
    smtp_send_ready: bool,
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

const fn integrity_error() -> StandardsMailboxProvisioningError {
    StandardsMailboxProvisioningError::new(StandardsMailboxProvisioningErrorClass::IntegrityFailure)
}

const fn dependency_error() -> StandardsMailboxProvisioningError {
    StandardsMailboxProvisioningError::new(
        StandardsMailboxProvisioningErrorClass::DependencyUnavailable,
    )
}

const fn not_found_error() -> StandardsMailboxProvisioningError {
    StandardsMailboxProvisioningError::new(StandardsMailboxProvisioningErrorClass::NotFound)
}

#[cfg(test)]
mod tests {
    use super::{MICROSOFT_SCOPES, Operation, map_status};
    use application_ports::standards_mailbox_onboarding::StandardsMailboxProvisioningErrorClass;

    #[test]
    fn microsoft_scopes_are_protocol_only_and_graph_free() {
        assert!(MICROSOFT_SCOPES.contains("IMAP.AccessAsUser.All"));
        assert!(MICROSOFT_SCOPES.contains("SMTP.Send"));
        assert!(MICROSOFT_SCOPES.contains("offline_access"));
        assert!(!MICROSOFT_SCOPES.contains("graph.microsoft.com"));
        assert!(!MICROSOFT_SCOPES.contains("Mail.Read"));
        assert!(!MICROSOFT_SCOPES.contains("Mail.Send"));
    }

    #[test]
    fn callback_replay_and_expiry_fail_closed() {
        assert!(map_status(200, Operation::Callback).is_ok());
        assert_eq!(
            map_status(409, Operation::Callback).map_err(|error| error.class()),
            Err(StandardsMailboxProvisioningErrorClass::ReplayRejected)
        );
        assert_eq!(
            map_status(410, Operation::Callback).map_err(|error| error.class()),
            Err(StandardsMailboxProvisioningErrorClass::Expired)
        );
    }
}
