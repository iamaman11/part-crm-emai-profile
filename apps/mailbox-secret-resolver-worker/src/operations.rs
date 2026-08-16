use crate::crypto::{EncryptionKeyring, ResolverCrypto, WorkerNonceSource};
use crate::ingress::AuthenticatedResolverRequest;
use crate::model::ResolverRoute;
use crate::refresh_fencing::CredentialLifecycleState;
use crate::provider::{
    OAuthProvider, ProviderError, authorization_url, exchange_authorization_code,
    refresh_access_token,
};
use crate::storage::{
    EncryptedRecordStore, ReconciliationResult, RecordIdentity, RecordStoreError,
    RefreshAcquireError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use worker::{Env, Response};
use zeroize::{Zeroize, Zeroizing};

const RESOLVER_DB_BINDING: &str = "RESOLVER_DB";
const ENCRYPTION_KEYRING_SECRET: &str = "MAILBOX_RESOLVER_ENCRYPTION_KEYRING";
const HANDLE_HMAC_SECRET: &str = "MAILBOX_RESOLVER_HANDLE_HMAC_KEY";
const OAUTH_CEREMONY_LIFETIME_MS: u64 = 10 * 60 * 1000;
const ACCESS_TOKEN_REFRESH_SKEW_MS: u64 = 60_000;
const COMPETING_REFRESH_REUSE_MIN_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationError {
    InvalidRequest,
    NotFound,
    Expired,
    ReplayRejected,
    ProviderRejected,
    ReauthRequired,
    DependencyUnavailable,
    ConfigurationUnavailable,
    InternalFailure,
}

pub async fn dispatch_operation(
    request: AuthenticatedResolverRequest,
    env: &Env,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let store = encrypted_store(env)?;
    let tenant_id = request.envelope.tenant_id.as_str();
    let payload = &request.envelope.payload;
    match request.route {
        ResolverRoute::Discard => discard(&store, tenant_id, payload, now_ms).await,
        ResolverRoute::MicrosoftGraphCursorStore => {
            store_cursor(&store, tenant_id, payload, now_ms).await
        }
        ResolverRoute::MicrosoftGraphCursorResolve => {
            resolve_cursor(&store, tenant_id, payload, now_ms).await
        }
        ResolverRoute::Resolve => resolve_credential(&store, env, tenant_id, payload, now_ms).await,
        ResolverRoute::GmailSendResolve => {
            resolve_gmail_send(&store, env, tenant_id, payload, now_ms).await
        }
        ResolverRoute::MicrosoftGraphRefresh => {
            refresh_graph(&store, env, tenant_id, payload, now_ms).await
        }
        ResolverRoute::StandardsPasswordProvision => {
            provision_password(&store, tenant_id, payload, &request.body_digest, now_ms).await
        }
        ResolverRoute::GmailOAuthStart
        | ResolverRoute::GmailSendOAuthStart
        | ResolverRoute::MicrosoftGraphOAuthStart
        | ResolverRoute::StandardsMicrosoftOAuthStart => {
            start_oauth(&store, env, request.route, tenant_id, payload, now_ms).await
        }
        ResolverRoute::GmailOAuthInspect
        | ResolverRoute::GmailSendOAuthInspect
        | ResolverRoute::MicrosoftGraphOAuthInspect
        | ResolverRoute::StandardsMicrosoftOAuthInspect => {
            inspect_oauth(&store, request.route, tenant_id, payload, now_ms).await
        }
        ResolverRoute::GmailOAuthDeny
        | ResolverRoute::GmailSendOAuthDeny
        | ResolverRoute::MicrosoftGraphOAuthDeny
        | ResolverRoute::StandardsMicrosoftOAuthDeny => {
            deny_oauth(&store, request.route, tenant_id, payload, now_ms).await
        }
        ResolverRoute::GmailOAuthComplete
        | ResolverRoute::GmailSendOAuthComplete
        | ResolverRoute::MicrosoftGraphOAuthComplete
        | ResolverRoute::StandardsMicrosoftOAuthComplete => {
            complete_oauth(&store, env, request.route, tenant_id, payload, now_ms).await
        }
    }
}

pub async fn reconcile_encryption_keys(
    env: &Env,
    now_ms: u64,
) -> Result<ReconciliationResult, OperationError> {
    encrypted_store(env)?
        .reconcile_key_rotation(now_ms, 100)
        .await
        .map_err(map_store_error)
}

fn encrypted_store(env: &Env) -> Result<EncryptedRecordStore<WorkerNonceSource>, OperationError> {
    let database = env
        .d1(RESOLVER_DB_BINDING)
        .map_err(|_| OperationError::ConfigurationUnavailable)?;
    let keyring_secret = Zeroizing::new(
        env.secret(ENCRYPTION_KEYRING_SECRET)
            .map_err(|_| OperationError::ConfigurationUnavailable)?
            .to_string(),
    );
    let keyring = EncryptionKeyring::parse(&keyring_secret)
        .map_err(|_| OperationError::ConfigurationUnavailable)?;
    let handle_key = Zeroizing::new(
        env.secret(HANDLE_HMAC_SECRET)
            .map_err(|_| OperationError::ConfigurationUnavailable)?
            .to_string(),
    );
    let crypto = ResolverCrypto::new(keyring, handle_key.as_bytes().to_vec(), WorkerNonceSource)
        .map_err(|_| OperationError::ConfigurationUnavailable)?;
    Ok(EncryptedRecordStore::new(database, crypto))
}

async fn store_cursor(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let handle = store.random_handle("cursor_").map_err(map_store_error)?;
    let binding_id = string(payload, "mailboxBindingId")?;
    let provider_cursor = string(payload, "providerCursor")?;
    let document = serde_json::to_vec(&json!({
        "mailboxBindingId": binding_id,
        "providerCursor": provider_cursor,
    }))
    .map_err(|_| OperationError::InternalFailure)?;
    store
        .store(
            &RecordIdentity {
                tenant_id,
                raw_handle: &handle,
                provider: "MICROSOFT_GRAPH",
                record_kind: "cursor",
            },
            &document,
            now_ms,
            None,
        )
        .await
        .map_err(map_store_error)?;
    json_response(json!({"cursorHandle": handle}))
}

async fn resolve_cursor(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let handle = string(payload, "cursorHandle")?;
    let binding_id = string(payload, "mailboxBindingId")?;
    let stored = store
        .load(
            &RecordIdentity {
                tenant_id,
                raw_handle: handle,
                provider: "MICROSOFT_GRAPH",
                record_kind: "cursor",
            },
            now_ms,
        )
        .await
        .map_err(map_store_error)?;
    let document: CursorDocument =
        serde_json::from_slice(&stored.document).map_err(|_| OperationError::InternalFailure)?;
    if document.mailbox_binding_id != binding_id {
        return Err(OperationError::NotFound);
    }
    json_response(json!({"providerCursor": document.provider_cursor}))
}

async fn discard(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    store
        .discard(
            tenant_id,
            string(payload, "secretHandle")?,
            "credential",
            now_ms,
        )
        .await
        .map_err(map_store_error)?;
    empty_response(204)
}

async fn provision_password(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    payload: &Map<String, Value>,
    request_digest: &str,
    now_ms: u64,
) -> Result<Response, OperationError> {
    claim_idempotency(
        store,
        tenant_id,
        string(payload, "idempotencyKey")?,
        request_digest,
        now_ms,
    )
    .await?;
    let handle = store
        .deterministic_handle(tenant_id, "secret_", string(payload, "idempotencyKey")?)
        .map_err(map_store_error)?;
    let credential = StoredCredential {
        provider: "IMAP".to_owned(),
        access_token: None,
        refresh_token: None,
        expires_at_ms: None,
        scope: None,
        imap: Some(parse_protocol(payload, "imap")?),
        smtp: Some(parse_protocol(payload, "smtp")?),
    };
    store_credential(store, tenant_id, &handle, &credential, now_ms).await?;
    json_response(json!({
        "secretHandle": handle,
        "authenticationMode": "PASSWORD",
        "imapReadSearchReady": true,
        "smtpSendReady": true,
    }))
}

async fn claim_idempotency(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    idempotency_key: &str,
    request_digest: &str,
    now_ms: u64,
) -> Result<(), OperationError> {
    store
        .claim_idempotency(
            tenant_id,
            idempotency_key,
            "standards_password_provision",
            request_digest,
            now_ms,
        )
        .await
        .map_err(map_store_error)
}

async fn resolve_credential(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    env: &Env,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let provider = string(payload, "provider")?;
    let handle = string(payload, "secretHandle")?;
    let loaded = load_credential(store, tenant_id, handle, provider, "credential", now_ms).await?;
    let credential = refresh_credential(
        store,
        env,
        &RefreshTarget {
            tenant_id,
            handle,
            provider,
            record_kind: "credential",
        },
        loaded,
        RefreshMode::IfNeeded,
        now_ms,
    )
    .await?;
    match provider {
        "GMAIL_API" => json_response(json!({
            "kind": "gmail_api",
            "access_token": credential.access_token.as_deref().ok_or(OperationError::InternalFailure)?,
        })),
        "MICROSOFT_GRAPH" => json_response(json!({
            "kind": "microsoft_graph",
            "access_token": credential.access_token.as_deref().ok_or(OperationError::InternalFailure)?,
        })),
        "IMAP" => protocol_response(&credential, payload.get("credentialPurpose")),
        _ => Err(OperationError::InvalidRequest),
    }
}

async fn resolve_gmail_send(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    env: &Env,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let handle = string(payload, "mailboxBindingId")?;
    let loaded = load_credential(
        store,
        tenant_id,
        handle,
        "GMAIL_API",
        "gmail_send_capability",
        now_ms,
    )
    .await?;
    let credential = refresh_credential(
        store,
        env,
        &RefreshTarget {
            tenant_id,
            handle,
            provider: "GMAIL_API",
            record_kind: "gmail_send_capability",
        },
        loaded,
        RefreshMode::IfNeeded,
        now_ms,
    )
    .await?;
    json_response(json!({
        "access_token": credential.access_token.as_deref().ok_or(OperationError::InternalFailure)?,
    }))
}

async fn refresh_graph(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    env: &Env,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let handle = string(payload, "secretHandle")?;
    let loaded = load_credential(
        store,
        tenant_id,
        handle,
        "MICROSOFT_GRAPH",
        "credential",
        now_ms,
    )
    .await?;
    let credential = refresh_credential(
        store,
        env,
        &RefreshTarget {
            tenant_id,
            handle,
            provider: "MICROSOFT_GRAPH",
            record_kind: "credential",
        },
        loaded,
        RefreshMode::Force,
        now_ms,
    )
    .await?;
    json_response(json!({
        "access_token": credential.access_token.as_deref().ok_or(OperationError::InternalFailure)?,
    }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefreshMode {
    IfNeeded,
    Force,
}

struct LoadedCredential {
    credential: StoredCredential,
    mutation_generation: u64,
}

struct RefreshTarget<'a> {
    tenant_id: &'a str,
    handle: &'a str,
    provider: &'a str,
    record_kind: &'a str,
}

async fn refresh_credential(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    env: &Env,
    target: &RefreshTarget<'_>,
    mut loaded: LoadedCredential,
    mode: RefreshMode,
    now_ms: u64,
) -> Result<StoredCredential, OperationError> {
    if !refresh_required(&loaded.credential, mode, now_ms) {
        return Ok(loaded.credential);
    }
    let provider = oauth_provider(&loaded.credential)?;
    let identity = RecordIdentity {
        tenant_id: target.tenant_id,
        raw_handle: target.handle,
        provider: target.provider,
        record_kind: target.record_kind,
    };
    let lease = match store
        .acquire_refresh_lease(&identity, loaded.mutation_generation, now_ms)
        .await
    {
        Ok(lease) => lease,
        Err(RefreshAcquireError::Busy)
        | Err(RefreshAcquireError::Store(RecordStoreError::ConcurrentMutation)) => {
            return reload_after_refresh_competition(
                store,
                target,
                loaded.mutation_generation,
                mode,
                now_ms,
            )
            .await;
        }
        Err(RefreshAcquireError::ReauthRequired) => return Err(OperationError::ReauthRequired),
        Err(RefreshAcquireError::Store(error)) => return Err(map_store_error(error)),
    };

    let Some(refresh_token) = loaded.credential.refresh_token.as_deref() else {
        return release_then_error(
            store,
            &identity,
            &lease,
            now_ms,
            OperationError::InternalFailure,
        )
        .await;
    };

    let tokens = refresh_access_token(
        env,
        provider,
        refresh_token,
        loaded.credential.scope.as_deref(),
    )
    .await;

    match tokens {
        Ok(mut tokens) => {
            loaded.credential.access_token = Some(core::mem::take(&mut tokens.access_token));
            if let Some(replacement) = tokens.refresh_token.as_mut() {
                loaded.credential.refresh_token = Some(core::mem::take(replacement));
            }
            loaded.credential.expires_at_ms =
                Some(now_ms.saturating_add(tokens.expires_in.saturating_mul(1000)));
            let mut document = serde_json::to_vec(&loaded.credential)
                .map_err(|_| OperationError::InternalFailure)?;
            let committed = store.commit_refresh(&identity, &lease, &document, now_ms).await;
            document.zeroize();
            match committed {
                Ok(_) => Ok(loaded.credential),
                Err(RecordStoreError::ConcurrentMutation) => {
                    reload_after_refresh_competition(
                        store,
                        target,
                        loaded.mutation_generation,
                        mode,
                        now_ms,
                    )
                    .await
                }
                Err(error) => Err(map_store_error(error)),
            }
        }
        Err(ProviderError::CredentialRejected) => {
            match store.mark_reauth_required(&identity, &lease, now_ms).await {
                Ok(_) => Err(OperationError::ReauthRequired),
                Err(RecordStoreError::ConcurrentMutation) => {
                    reload_after_refresh_competition(
                        store,
                        target,
                        loaded.mutation_generation,
                        mode,
                        now_ms,
                    )
                    .await
                }
                Err(error) => Err(map_store_error(error)),
            }
        }
        Err(error) => {
            match store.release_refresh_lease(&identity, &lease, now_ms).await {
                Ok(()) => Err(map_provider_error(error)),
                Err(RecordStoreError::ConcurrentMutation) => {
                    reload_after_refresh_competition(
                        store,
                        target,
                        loaded.mutation_generation,
                        mode,
                        now_ms,
                    )
                    .await
                }
                Err(store_error) => Err(map_store_error(store_error)),
            }
        }
    }
}

async fn release_then_error(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    identity: &RecordIdentity<'_>,
    lease: &crate::storage::RefreshLease,
    now_ms: u64,
    error: OperationError,
) -> Result<StoredCredential, OperationError> {
    match store.release_refresh_lease(identity, lease, now_ms).await {
        Ok(()) | Err(RecordStoreError::ConcurrentMutation) => Err(error),
        Err(store_error) => Err(map_store_error(store_error)),
    }
}

async fn reload_after_refresh_competition(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    target: &RefreshTarget<'_>,
    previous_generation: u64,
    mode: RefreshMode,
    now_ms: u64,
) -> Result<StoredCredential, OperationError> {
    let reloaded = load_credential(
        store,
        target.tenant_id,
        target.handle,
        target.provider,
        target.record_kind,
        now_ms,
    )
    .await?;
    if reloaded.mutation_generation > previous_generation {
        return Ok(reloaded.credential);
    }
    if mode == RefreshMode::IfNeeded
        && access_token_usable_during_refresh(&reloaded.credential, now_ms)
    {
        return Ok(reloaded.credential);
    }
    Err(OperationError::DependencyUnavailable)
}

fn refresh_required(credential: &StoredCredential, mode: RefreshMode, now_ms: u64) -> bool {
    match mode {
        RefreshMode::Force => true,
        RefreshMode::IfNeeded => credential.expires_at_ms.is_some_and(|expires_at| {
            expires_at <= now_ms.saturating_add(ACCESS_TOKEN_REFRESH_SKEW_MS)
        }),
    }
}

fn access_token_usable_during_refresh(credential: &StoredCredential, now_ms: u64) -> bool {
    credential.access_token.is_some()
        && credential.expires_at_ms.is_some_and(|expires_at| {
            expires_at > now_ms.saturating_add(COMPETING_REFRESH_REUSE_MIN_MS)
        })
}

fn oauth_provider(credential: &StoredCredential) -> Result<OAuthProvider, OperationError> {
    match credential.provider.as_str() {
        "GMAIL_API" => Ok(OAuthProvider::Google),
        "MICROSOFT_GRAPH" | "IMAP" if credential.refresh_token.is_some() => {
            Ok(OAuthProvider::Microsoft)
        }
        "IMAP" => Err(OperationError::InvalidRequest),
        _ => Err(OperationError::InvalidRequest),
    }
}

fn protocol_response(
    credential: &StoredCredential,
    credential_purpose: Option<&Value>,
) -> Result<Response, OperationError> {
    let smtp = credential_purpose.and_then(Value::as_str) == Some("SMTP_SEND");
    let protocol = if smtp {
        credential.smtp.as_ref()
    } else {
        credential.imap.as_ref()
    }
    .ok_or(OperationError::InternalFailure)?;
    let mode = if credential.access_token.is_some() {
        "xoauth2"
    } else {
        "password"
    };
    let mut document = json!({
        "host": protocol.host,
        "port": protocol.port,
        "username": protocol.username,
        "authentication_mode": mode,
        "tls": protocol.tls,
    });
    let object = document
        .as_object_mut()
        .ok_or(OperationError::InternalFailure)?;
    if let Some(token) = credential.access_token.as_ref() {
        object.insert("access_token".to_owned(), Value::String(token.clone()));
    } else if let Some(password) = protocol.password.as_ref() {
        object.insert("password".to_owned(), Value::String(password.clone()));
    } else {
        return Err(OperationError::InternalFailure);
    }
    if smtp {
        object.remove("kind");
    } else {
        object.insert("kind".to_owned(), Value::String("imap".to_owned()));
    }
    json_response(document)
}

async fn store_credential(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    handle: &str,
    credential: &StoredCredential,
    now_ms: u64,
) -> Result<(), OperationError> {
    let mut document =
        serde_json::to_vec(credential).map_err(|_| OperationError::InternalFailure)?;
    let result = store
        .store(
            &RecordIdentity {
                tenant_id,
                raw_handle: handle,
                provider: &credential.provider,
                record_kind: if credential.provider == "GMAIL_SEND" {
                    "gmail_send_capability"
                } else {
                    "credential"
                },
            },
            &document,
            now_ms,
            None,
        )
        .await
        .map_err(map_store_error);
    document.zeroize();
    result
}

async fn load_credential(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    handle: &str,
    provider: &str,
    record_kind: &str,
    now_ms: u64,
) -> Result<LoadedCredential, OperationError> {
    let stored = store
        .load(
            &RecordIdentity {
                tenant_id,
                raw_handle: handle,
                provider,
                record_kind,
            },
            now_ms,
        )
        .await
        .map_err(map_store_error)?;
    if stored.lifecycle == CredentialLifecycleState::ReauthRequired {
        return Err(OperationError::ReauthRequired);
    }
    let credential: StoredCredential =
        serde_json::from_slice(&stored.document).map_err(|_| OperationError::InternalFailure)?;
    if credential.provider != provider {
        return Err(OperationError::InternalFailure);
    }
    Ok(LoadedCredential {
        credential,
        mutation_generation: stored.mutation_generation,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CursorDocument {
    mailbox_binding_id: String,
    provider_cursor: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredCredential {
    provider: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at_ms: Option<u64>,
    scope: Option<String>,
    imap: Option<StoredProtocol>,
    smtp: Option<StoredProtocol>,
}

impl Drop for StoredCredential {
    fn drop(&mut self) {
        if let Some(value) = self.access_token.as_mut() {
            value.zeroize();
        }
        if let Some(value) = self.refresh_token.as_mut() {
            value.zeroize();
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredProtocol {
    host: String,
    port: u64,
    tls: String,
    username: String,
    password: Option<String>,
}

impl Drop for StoredProtocol {
    fn drop(&mut self) {
        self.username.zeroize();
        if let Some(value) = self.password.as_mut() {
            value.zeroize();
        }
    }
}

fn parse_protocol(
    payload: &Map<String, Value>,
    name: &str,
) -> Result<StoredProtocol, OperationError> {
    let value = payload.get(name).ok_or(OperationError::InvalidRequest)?;
    let protocol = value.as_object().ok_or(OperationError::InvalidRequest)?;
    Ok(StoredProtocol {
        host: string(protocol, "host")?.to_owned(),
        port: unsigned(protocol, "port")?,
        tls: match string(protocol, "transportSecurity")? {
            "IMPLICIT_TLS" => "implicit".to_owned(),
            "STARTTLS" => "start_tls".to_owned(),
            _ => return Err(OperationError::InvalidRequest),
        },
        username: string(protocol, "username")?.to_owned(),
        password: Some(string(protocol, "password")?.to_owned()),
    })
}

fn string<'a>(payload: &'a Map<String, Value>, name: &str) -> Result<&'a str, OperationError> {
    payload
        .get(name)
        .and_then(Value::as_str)
        .ok_or(OperationError::InvalidRequest)
}

fn unsigned(payload: &Map<String, Value>, name: &str) -> Result<u64, OperationError> {
    payload
        .get(name)
        .and_then(Value::as_u64)
        .ok_or(OperationError::InvalidRequest)
}

fn json_response(mut value: Value) -> Result<Response, OperationError> {
    let serialized = Response::from_json(&value).map_err(|_| OperationError::InternalFailure);
    zeroize_json(&mut value);
    let response = serialized?;
    no_store(response)
}

fn zeroize_json(value: &mut Value) {
    match value {
        Value::String(secret) => secret.zeroize(),
        Value::Array(values) => {
            for nested in values {
                zeroize_json(nested);
            }
        }
        Value::Object(values) => {
            for nested in values.values_mut() {
                zeroize_json(nested);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn empty_response(status: u16) -> Result<Response, OperationError> {
    let response = Response::empty()
        .map_err(|_| OperationError::InternalFailure)?
        .with_status(status);
    no_store(response)
}

fn no_store(response: Response) -> Result<Response, OperationError> {
    response
        .headers()
        .set("cache-control", "no-store")
        .map_err(|_| OperationError::InternalFailure)?;
    Ok(response)
}

const fn map_store_error(error: RecordStoreError) -> OperationError {
    match error {
        RecordStoreError::NotFound | RecordStoreError::Discarded => OperationError::NotFound,
        RecordStoreError::Expired => OperationError::Expired,
        RecordStoreError::ReplayRejected => OperationError::ReplayRejected,
        RecordStoreError::StorageUnavailable => OperationError::DependencyUnavailable,
        RecordStoreError::InvalidInput => OperationError::InvalidRequest,
        RecordStoreError::ConcurrentMutation => OperationError::DependencyUnavailable,
        RecordStoreError::Crypto => OperationError::InternalFailure,
    }
}

const fn map_provider_error(error: ProviderError) -> OperationError {
    match error {
        ProviderError::CredentialRejected => OperationError::ReauthRequired,
        ProviderError::ProviderRejected => OperationError::ProviderRejected,
        ProviderError::DependencyUnavailable => OperationError::DependencyUnavailable,
        ProviderError::InvalidConfiguration => OperationError::ConfigurationUnavailable,
        ProviderError::InvalidRequest => OperationError::InvalidRequest,
        ProviderError::ResponseTooLarge | ProviderError::InvalidResponse => {
            OperationError::InternalFailure
        }
    }
}

async fn start_oauth(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    env: &Env,
    route: ResolverRoute,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let (mode, provider, target_id, expected_version, scopes, include_granted) = match route {
        ResolverRoute::GmailOAuthStart => (
            CeremonyMode::GmailRead,
            OAuthProvider::Google,
            string(payload, "mailboxOnboardingId")?,
            unsigned(payload, "mailboxOnboardingVersion")?,
            string(payload, "oauthScope")?,
            false,
        ),
        ResolverRoute::GmailSendOAuthStart => (
            CeremonyMode::GmailSend,
            OAuthProvider::Google,
            string(payload, "mailboxBindingId")?,
            unsigned(payload, "mailboxBindingVersion")?,
            string(payload, "oauthScope")?,
            true,
        ),
        ResolverRoute::MicrosoftGraphOAuthStart => (
            CeremonyMode::MicrosoftGraph,
            OAuthProvider::Microsoft,
            string(payload, "mailboxOnboardingId")?,
            unsigned(payload, "mailboxOnboardingVersion")?,
            string(payload, "oauthScope")?,
            false,
        ),
        ResolverRoute::StandardsMicrosoftOAuthStart => (
            CeremonyMode::StandardsMicrosoft,
            OAuthProvider::Microsoft,
            string(payload, "mailboxOnboardingId")?,
            unsigned(payload, "mailboxOnboardingVersion")?,
            string(payload, "oauthScopes")?,
            false,
        ),
        _ => return Err(OperationError::InvalidRequest),
    };
    let opaque_state = store.random_handle("state_").map_err(map_store_error)?;
    let state = format!("{tenant_id}.{opaque_state}");
    let ceremony_id = store.random_handle("ceremony_").map_err(map_store_error)?;
    let pkce_verifier = store.random_handle("pkce_").map_err(map_store_error)?;
    let expires_at_ms = now_ms.saturating_add(OAUTH_CEREMONY_LIFETIME_MS);
    let ceremony = OAuthCeremony {
        mode,
        ceremony_id: ceremony_id.clone(),
        actor_id: string(payload, "actorId")?.to_owned(),
        target_id: target_id.to_owned(),
        expected_version,
        scopes: scopes.to_owned(),
        pkce_verifier: pkce_verifier.clone(),
        expires_at_ms,
    };
    let authorization_url = authorization_url(
        env,
        provider,
        &state,
        scopes,
        Some(&pkce_verifier),
        include_granted,
    )
    .map_err(map_provider_error)?;
    store_ceremony(
        store,
        tenant_id,
        &state,
        provider_name(provider),
        &ceremony,
        now_ms,
    )
    .await?;
    json_response(json!({
        "ceremonyId": ceremony_id,
        "authorizationUrl": authorization_url,
        "expiresAtMs": expires_at_ms,
    }))
}

async fn inspect_oauth(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    route: ResolverRoute,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let expected_mode = mode_for_route(route)?;
    let provider = provider_for_mode(expected_mode);
    let ceremony = load_ceremony(
        store,
        tenant_id,
        string(payload, "oauthState")?,
        provider_name(provider),
        now_ms,
        false,
    )
    .await?;
    if ceremony.mode != expected_mode {
        return Err(OperationError::NotFound);
    }
    let mut document = json!({
        "tenantId": tenant_id,
        "expectedVersion": ceremony.expected_version,
        "starterActorId": ceremony.actor_id,
        "expiresAtMs": ceremony.expires_at_ms,
    });
    let object = document
        .as_object_mut()
        .ok_or(OperationError::InternalFailure)?;
    object.insert(
        if expected_mode == CeremonyMode::GmailSend {
            "bindingId"
        } else {
            "onboardingId"
        }
        .to_owned(),
        Value::String(ceremony.target_id.clone()),
    );
    json_response(document)
}

async fn deny_oauth(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    route: ResolverRoute,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let expected_mode = mode_for_route(route)?;
    verified_consume_ceremony(
        store,
        tenant_id,
        string(payload, "oauthState")?,
        provider_name(provider_for_mode(expected_mode)),
        expected_mode,
        string(payload, "actorId")?,
        now_ms,
    )
    .await?;
    empty_response(204)
}

async fn complete_oauth(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    env: &Env,
    route: ResolverRoute,
    tenant_id: &str,
    payload: &Map<String, Value>,
    now_ms: u64,
) -> Result<Response, OperationError> {
    let expected_mode = mode_for_route(route)?;
    let provider = provider_for_mode(expected_mode);
    let ceremony = verified_consume_ceremony(
        store,
        tenant_id,
        string(payload, "oauthState")?,
        provider_name(provider),
        expected_mode,
        string(payload, "actorId")?,
        now_ms,
    )
    .await?;
    let mut tokens = exchange_authorization_code(
        env,
        provider,
        string(payload, "oauthAuthorizationCode")?,
        Some(&ceremony.pkce_verifier),
    )
    .await
    .map_err(map_provider_error)?;
    let expires_at_ms = now_ms.saturating_add(tokens.expires_in * 1000);
    let access_token = core::mem::take(&mut tokens.access_token);
    let refresh_token = tokens.refresh_token.as_mut().map(core::mem::take);
    let scope = tokens
        .scope
        .clone()
        .or_else(|| Some(ceremony.scopes.clone()));

    match expected_mode {
        CeremonyMode::GmailRead | CeremonyMode::MicrosoftGraph => {
            let handle = store.random_handle("secret_").map_err(map_store_error)?;
            let credential = StoredCredential {
                provider: if expected_mode == CeremonyMode::GmailRead {
                    "GMAIL_API"
                } else {
                    "MICROSOFT_GRAPH"
                }
                .to_owned(),
                access_token: Some(access_token),
                refresh_token,
                expires_at_ms: Some(expires_at_ms),
                scope,
                imap: None,
                smtp: None,
            };
            store_credential(store, tenant_id, &handle, &credential, now_ms).await?;
            json_response(json!({"secretHandle": handle}))
        }
        CeremonyMode::GmailSend => {
            let credential = StoredCredential {
                provider: "GMAIL_API".to_owned(),
                access_token: Some(access_token),
                refresh_token,
                expires_at_ms: Some(expires_at_ms),
                scope,
                imap: None,
                smtp: None,
            };
            store_credential_kind(
                store,
                tenant_id,
                &ceremony.target_id,
                "gmail_send_capability",
                &credential,
                now_ms,
            )
            .await?;
            empty_response(204)
        }
        CeremonyMode::StandardsMicrosoft => {
            let username = tokens
                .id_token
                .as_deref()
                .and_then(id_token_username)
                .ok_or(OperationError::ProviderRejected)?;
            let handle = store.random_handle("secret_").map_err(map_store_error)?;
            let credential = StoredCredential {
                provider: "IMAP".to_owned(),
                access_token: Some(access_token),
                refresh_token,
                expires_at_ms: Some(expires_at_ms),
                scope,
                imap: Some(StoredProtocol {
                    host: "outlook.office365.com".to_owned(),
                    port: 993,
                    tls: "implicit".to_owned(),
                    username: username.clone(),
                    password: None,
                }),
                smtp: Some(StoredProtocol {
                    host: "smtp.office365.com".to_owned(),
                    port: 587,
                    tls: "start_tls".to_owned(),
                    username,
                    password: None,
                }),
            };
            store_credential(store, tenant_id, &handle, &credential, now_ms).await?;
            json_response(json!({
                "secretHandle": handle,
                "authenticationMode": "MICROSOFT_OAUTH2",
                "imapReadSearchReady": true,
                "smtpSendReady": true,
            }))
        }
    }
}

async fn store_ceremony(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    state: &str,
    provider: &str,
    ceremony: &OAuthCeremony,
    now_ms: u64,
) -> Result<(), OperationError> {
    let mut document = serde_json::to_vec(ceremony).map_err(|_| OperationError::InternalFailure)?;
    let result = store
        .store(
            &RecordIdentity {
                tenant_id,
                raw_handle: state,
                provider,
                record_kind: "oauth_ceremony",
            },
            &document,
            now_ms,
            Some(ceremony.expires_at_ms),
        )
        .await
        .map_err(map_store_error);
    document.zeroize();
    result
}

async fn load_ceremony(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    state: &str,
    provider: &str,
    now_ms: u64,
    consume: bool,
) -> Result<OAuthCeremony, OperationError> {
    let identity = RecordIdentity {
        tenant_id,
        raw_handle: state,
        provider,
        record_kind: "oauth_ceremony",
    };
    let stored = if consume {
        store.consume(&identity, now_ms).await
    } else {
        store.load(&identity, now_ms).await
    }
    .map_err(map_store_error)?;
    serde_json::from_slice(&stored.document).map_err(|_| OperationError::InternalFailure)
}

async fn verified_consume_ceremony(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    state: &str,
    provider: &str,
    expected_mode: CeremonyMode,
    actor_id: &str,
    now_ms: u64,
) -> Result<OAuthCeremony, OperationError> {
    let inspected = load_ceremony(store, tenant_id, state, provider, now_ms, false).await?;
    if inspected.mode != expected_mode || inspected.actor_id != actor_id {
        return Err(OperationError::NotFound);
    }
    let consumed = load_ceremony(store, tenant_id, state, provider, now_ms, true).await?;
    if consumed.ceremony_id != inspected.ceremony_id
        || consumed.mode != inspected.mode
        || consumed.actor_id != inspected.actor_id
    {
        return Err(OperationError::InternalFailure);
    }
    Ok(consumed)
}

async fn store_credential_kind(
    store: &EncryptedRecordStore<WorkerNonceSource>,
    tenant_id: &str,
    handle: &str,
    record_kind: &str,
    credential: &StoredCredential,
    now_ms: u64,
) -> Result<(), OperationError> {
    let mut document =
        serde_json::to_vec(credential).map_err(|_| OperationError::InternalFailure)?;
    let result = store
        .store(
            &RecordIdentity {
                tenant_id,
                raw_handle: handle,
                provider: &credential.provider,
                record_kind,
            },
            &document,
            now_ms,
            None,
        )
        .await
        .map_err(map_store_error);
    document.zeroize();
    result
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CeremonyMode {
    GmailRead,
    GmailSend,
    MicrosoftGraph,
    StandardsMicrosoft,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OAuthCeremony {
    mode: CeremonyMode,
    ceremony_id: String,
    actor_id: String,
    target_id: String,
    expected_version: u64,
    scopes: String,
    pkce_verifier: String,
    expires_at_ms: u64,
}

impl Drop for OAuthCeremony {
    fn drop(&mut self) {
        self.pkce_verifier.zeroize();
    }
}

const fn mode_for_route(route: ResolverRoute) -> Result<CeremonyMode, OperationError> {
    match route {
        ResolverRoute::GmailOAuthInspect
        | ResolverRoute::GmailOAuthComplete
        | ResolverRoute::GmailOAuthDeny => Ok(CeremonyMode::GmailRead),
        ResolverRoute::GmailSendOAuthInspect
        | ResolverRoute::GmailSendOAuthComplete
        | ResolverRoute::GmailSendOAuthDeny => Ok(CeremonyMode::GmailSend),
        ResolverRoute::MicrosoftGraphOAuthInspect
        | ResolverRoute::MicrosoftGraphOAuthComplete
        | ResolverRoute::MicrosoftGraphOAuthDeny => Ok(CeremonyMode::MicrosoftGraph),
        ResolverRoute::StandardsMicrosoftOAuthInspect
        | ResolverRoute::StandardsMicrosoftOAuthComplete
        | ResolverRoute::StandardsMicrosoftOAuthDeny => Ok(CeremonyMode::StandardsMicrosoft),
        _ => Err(OperationError::InvalidRequest),
    }
}

const fn provider_for_mode(mode: CeremonyMode) -> OAuthProvider {
    match mode {
        CeremonyMode::GmailRead | CeremonyMode::GmailSend => OAuthProvider::Google,
        CeremonyMode::MicrosoftGraph | CeremonyMode::StandardsMicrosoft => OAuthProvider::Microsoft,
    }
}

const fn provider_name(provider: OAuthProvider) -> &'static str {
    match provider {
        OAuthProvider::Google => "GOOGLE",
        OAuthProvider::Microsoft => "MICROSOFT",
    }
}

fn id_token_username(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let mut decoded = base64url_decode(payload)?;
    let claims = serde_json::from_slice::<IdTokenClaims>(&decoded).ok();
    decoded.zeroize();
    let claims = claims?;
    claims.preferred_username.or(claims.email)
}

#[derive(Deserialize)]
struct IdTokenClaims {
    preferred_username: Option<String>,
    email: Option<String>,
}

fn base64url_decode(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes() {
        let decoded = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(decoded);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(u8::try_from((accumulator >> bits) & 0xff).ok()?);
        }
    }
    if bits >= 6 || (accumulator & ((1_u32 << bits) - 1)) != 0 {
        return None;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::{
        RefreshMode, StoredCredential, access_token_usable_during_refresh, base64url_decode,
        id_token_username, refresh_required,
    };

    #[test]
    fn microsoft_id_token_username_is_bounded_to_the_payload_claim() {
        let token = "e30.eyJwcmVmZXJyZWRfdXNlcm5hbWUiOiJ1c2VyQGV4YW1wbGUuY29tIn0.signature";
        assert_eq!(
            id_token_username(token).as_deref(),
            Some("user@example.com")
        );
        assert!(base64url_decode("not valid!").is_none());
    }

    #[test]
    fn password_credentials_do_not_enter_oauth_refresh() {
        let credential = StoredCredential {
            provider: "IMAP".to_owned(),
            access_token: None,
            refresh_token: None,
            expires_at_ms: None,
            scope: None,
            imap: None,
            smtp: None,
        };
        assert!(!refresh_required(
            &credential,
            RefreshMode::IfNeeded,
            1_000
        ));
    }

    #[test]
    fn expiring_oauth_credentials_refresh_and_live_tokens_can_bridge_single_flight() {
        let credential = StoredCredential {
            provider: "MICROSOFT_GRAPH".to_owned(),
            access_token: Some("opaque-access-token".to_owned()),
            refresh_token: Some("opaque-refresh-token".to_owned()),
            expires_at_ms: Some(70_000),
            scope: Some("offline_access Mail.Read".to_owned()),
            imap: None,
            smtp: None,
        };
        assert!(refresh_required(
            &credential,
            RefreshMode::IfNeeded,
            20_000
        ));
        assert!(access_token_usable_during_refresh(&credential, 20_000));
        assert!(!access_token_usable_during_refresh(&credential, 66_000));
        assert!(refresh_required(&credential, RefreshMode::Force, 1));
    }
}
