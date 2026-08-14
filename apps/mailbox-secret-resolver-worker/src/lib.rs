#![forbid(unsafe_code)]

mod contract;
mod crypto;
mod ingress;
mod model;
mod operations;
mod protocol;
mod provider;
mod replay;
mod storage;

pub use contract::{ContractError, validate_payload};
pub use crypto::{
    AuthenticatedContext, CryptoError, EncryptedValue, EncryptionKeyring, ResolverCrypto,
    WorkerNonceSource,
};
pub use ingress::{AuthenticatedResolverRequest, IngressError, authenticate_request};
pub use model::{
    ErrorDocument, MAX_REQUEST_BYTES, MAX_SECRET_DOCUMENT_BYTES, NONCE_BYTES, ResolverEnvelope,
    ResolverRoute,
};
pub use operations::{OperationError, dispatch_operation, reconcile_encryption_keys};
pub use protocol::{SignatureError, SignatureInput, body_digest_hex, sign_hex, verify};
pub use provider::{
    OAuthProvider, ProviderError, ProviderTokenSet, authorization_url, exchange_authorization_code,
    refresh_access_token,
};
pub use replay::{ReplayClaimError, claim_request_nonce};
pub use storage::{
    EncryptedRecordStore, ReconciliationResult, RecordIdentity, RecordStoreError, StoredSecret,
};

use worker::{
    Context, Date, Env, Request, Response, Result, ScheduleContext, ScheduledEvent, event,
};

#[event(fetch, respond_with_errors)]
pub async fn main(mut request: Request, env: Env, _context: Context) -> Result<Response> {
    let now_ms = Date::now().as_millis();
    let authenticated = match authenticate_request(&mut request, &env, now_ms).await {
        Ok(value) => value,
        Err(error) => return error_response(ingress_status(error), ingress_code(error)),
    };
    let database = match env.d1("RESOLVER_DB") {
        Ok(value) => value,
        Err(_) => return error_response(503, "resolver_configuration_unavailable"),
    };
    if let Err(error) = claim_request_nonce(
        &database,
        &authenticated.envelope.tenant_id,
        &authenticated.nonce,
        &authenticated.path,
        &authenticated.body_digest,
        now_ms,
    )
    .await
    {
        return match error {
            ReplayClaimError::ReplayRejected => error_response(409, "resolver_replay_rejected"),
            ReplayClaimError::StorageUnavailable => {
                error_response(503, "resolver_dependency_unavailable")
            }
        };
    }
    match dispatch_operation(authenticated, &env, now_ms).await {
        Ok(response) => Ok(response),
        Err(error) => error_response(operation_status(error), operation_code(error)),
    }
}

#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _context: ScheduleContext) {
    assert!(
        reconcile_encryption_keys(&env, Date::now().as_millis())
            .await
            .is_ok(),
        "resolver key reconciliation failed"
    );
}

const fn ingress_status(error: IngressError) -> u16 {
    match error {
        IngressError::MethodNotAllowed => 405,
        IngressError::RouteNotFound => 404,
        IngressError::BodyTooLarge => 413,
        IngressError::CrossTenantState => 403,
        IngressError::MissingAuthentication
        | IngressError::InvalidAuthentication
        | IngressError::StaleAuthentication => 401,
        IngressError::ConfigurationUnavailable => 503,
        IngressError::InvalidDocument
        | IngressError::WrongPurpose
        | IngressError::InvalidPayload => 422,
    }
}

const fn ingress_code(error: IngressError) -> &'static str {
    match error {
        IngressError::MethodNotAllowed => "resolver_method_not_allowed",
        IngressError::RouteNotFound => "resolver_route_not_found",
        IngressError::BodyTooLarge => "resolver_request_too_large",
        IngressError::InvalidDocument => "resolver_invalid_document",
        IngressError::WrongPurpose => "resolver_wrong_purpose",
        IngressError::InvalidPayload => "resolver_invalid_payload",
        IngressError::CrossTenantState => "resolver_cross_tenant_rejected",
        IngressError::MissingAuthentication => "resolver_authentication_required",
        IngressError::InvalidAuthentication => "resolver_authentication_invalid",
        IngressError::StaleAuthentication => "resolver_authentication_stale",
        IngressError::ConfigurationUnavailable => "resolver_configuration_unavailable",
    }
}

const fn operation_status(error: OperationError) -> u16 {
    match error {
        OperationError::InvalidRequest => 422,
        OperationError::NotFound => 404,
        OperationError::Expired => 410,
        OperationError::ReplayRejected => 409,
        OperationError::ProviderRejected => 400,
        OperationError::DependencyUnavailable | OperationError::ConfigurationUnavailable => 503,
        OperationError::InternalFailure => 500,
    }
}

const fn operation_code(error: OperationError) -> &'static str {
    match error {
        OperationError::InvalidRequest => "resolver_invalid_request",
        OperationError::NotFound => "resolver_record_not_found",
        OperationError::Expired => "resolver_record_expired",
        OperationError::ReplayRejected => "resolver_replay_rejected",
        OperationError::ProviderRejected => "resolver_provider_rejected",
        OperationError::DependencyUnavailable => "resolver_dependency_unavailable",
        OperationError::ConfigurationUnavailable => "resolver_configuration_unavailable",
        OperationError::InternalFailure => "resolver_internal_failure",
    }
}

fn error_response(status: u16, code: &'static str) -> Result<Response> {
    let response = Response::from_json(&ErrorDocument { code })?.with_status(status);
    response.headers().set("cache-control", "no-store")?;
    Ok(response)
}
