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
use serde_json::{Map, Value};
use worker::Env;
use zeroize::Zeroize;

use crate::resolver_request::{oauth_callback_tenant, signed_resolver_request};

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
        let document = PasswordProvisionDocument::from(configuration);
        let mut payload = serde_json::to_value(&document)
            .map_err(|_| integrity_error())?
            .as_object()
            .cloned()
            .ok_or_else(integrity_error)?;
        drop(document);
        payload.extend(onboarding_payload(actor, onboarding_id, expected_version));
        payload.insert(
            "authenticationMode".to_owned(),
            Value::String("PASSWORD".to_owned()),
        );
        payload.insert(
            "idempotencyKey".to_owned(),
            Value::String(idempotency_key.as_str().to_owned()),
        );
        let response = fetch(
            self.env,
            PASSWORD_PROVISION_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            "standards_password",
            payload,
            Operation::Provision,
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
        let mut payload = onboarding_payload(actor, onboarding_id, expected_version);
        payload.extend([
            string_field("authenticationMode", "MICROSOFT_OAUTH2"),
            string_field("oauthScopes", MICROSOFT_SCOPES),
            string_field("oauthProtocol", "IMAP_SMTP_XOAUTH2"),
        ]);
        let response = fetch(
            self.env,
            MICROSOFT_START_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            "standards_microsoft_oauth",
            payload,
            Operation::Start,
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
        let tenant_id = oauth_callback_tenant(state.as_str()).map_err(|_| integrity_error())?;
        let payload = Map::from_iter([string_field("oauthState", state.as_str())]);
        let response = fetch(
            self.env,
            MICROSOFT_INSPECT_ENDPOINT,
            tenant_id,
            "standards_microsoft_oauth",
            payload,
            Operation::Callback,
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
        require_actor_state_tenant(actor, state.as_str())?;
        let payload = Map::from_iter([
            actor_field(actor),
            string_field("oauthState", state.as_str()),
            string_field("oauthAuthorizationCode", authorization_code.as_str()),
            string_field("oauthScopes", MICROSOFT_SCOPES),
        ]);
        let response = fetch(
            self.env,
            MICROSOFT_COMPLETE_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            "standards_microsoft_oauth",
            payload,
            Operation::Callback,
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
        require_actor_state_tenant(actor, state.as_str())?;
        let payload = Map::from_iter([
            actor_field(actor),
            string_field("oauthState", state.as_str()),
        ]);
        fetch(
            self.env,
            MICROSOFT_DENY_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            "standards_microsoft_oauth",
            payload,
            Operation::Callback,
        )
        .await
        .map(|_| ())
    }

    async fn discard(
        &self,
        actor: &ActorContext,
        secret_handle: &SecretHandle,
    ) -> Result<(), StandardsMailboxProvisioningError> {
        let payload = Map::from_iter([
            actor_field(actor),
            string_field("secretHandle", secret_handle.as_str()),
            string_field("provider", "IMAP"),
        ]);
        fetch(
            self.env,
            DISCARD_ENDPOINT,
            actor.tenant_scope().tenant_id().as_str(),
            "credential_discard",
            payload,
            Operation::Discard,
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
    tenant_id: &str,
    purpose: &str,
    payload: Map<String, Value>,
    operation: Operation,
) -> Result<worker::Response, StandardsMailboxProvisioningError> {
    let resolver = env
        .service(MAILBOX_SECRET_RESOLVER_BINDING)
        .map_err(|_| integrity_error())?;
    let init = signed_resolver_request(env, endpoint, tenant_id, purpose, payload)
        .map_err(|_| integrity_error())?;
    let response = resolver
        .fetch(endpoint, Some(init))
        .await
        .map_err(|_| dependency_error())?;
    map_status(response.status_code(), operation)?;
    Ok(response)
}

fn onboarding_payload(
    actor: &ActorContext,
    onboarding_id: &MailboxOnboardingId,
    expected_version: MailboxOnboardingVersion,
) -> Map<String, Value> {
    Map::from_iter([
        actor_field(actor),
        string_field("mailboxOnboardingId", onboarding_id.as_str()),
        (
            "mailboxOnboardingVersion".to_owned(),
            Value::from(expected_version.value()),
        ),
        string_field("provider", "IMAP"),
    ])
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
) -> Result<(), StandardsMailboxProvisioningError> {
    if oauth_callback_tenant(state).map_err(|_| integrity_error())?
        != actor.tenant_scope().tenant_id().as_str()
    {
        return Err(integrity_error());
    }
    Ok(())
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
