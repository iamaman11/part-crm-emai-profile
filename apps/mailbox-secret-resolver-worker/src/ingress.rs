use crate::contract::validate_payload;
use crate::model::{MAX_REQUEST_BYTES, ResolverEnvelope, ResolverRoute};
use crate::protocol::{SignatureError, SignatureInput, verify_versioned};
use control_plane_contract::resolver_service_auth::ServiceAuthKeyring;
use worker::{Env, Method, Request};
use zeroize::Zeroizing;

const CALLER_AUTH_SECRET: &str = "MAILBOX_RESOLVER_CALLER_AUTH_KEY";
const SIGNATURE_VERSION_HEADER: &str = "x-resolver-signature-version";
const KEY_ID_HEADER: &str = "x-resolver-key-id";
const BODY_DIGEST_HEADER: &str = "x-resolver-body-sha256";
const TIMESTAMP_HEADER: &str = "x-resolver-timestamp-ms";
const NONCE_HEADER: &str = "x-resolver-nonce";
const SIGNATURE_HEADER: &str = "x-resolver-signature";

#[derive(Debug)]
pub struct AuthenticatedResolverRequest {
    pub route: ResolverRoute,
    pub path: String,
    pub envelope: ResolverEnvelope,
    pub body_digest: String,
    pub timestamp_ms: u64,
    pub nonce: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngressError {
    MethodNotAllowed,
    RouteNotFound,
    BodyTooLarge,
    InvalidDocument,
    WrongPurpose,
    InvalidPayload,
    CrossTenantState,
    MissingAuthentication,
    InvalidAuthentication,
    StaleAuthentication,
    ConfigurationUnavailable,
}

pub async fn authenticate_request(
    request: &mut Request,
    env: &Env,
    now_ms: u64,
) -> Result<AuthenticatedResolverRequest, IngressError> {
    if request.method() != Method::Post {
        return Err(IngressError::MethodNotAllowed);
    }
    let path = request.path();
    let route = ResolverRoute::parse(&path).ok_or(IngressError::RouteNotFound)?;
    reject_oversized_content_length(request)?;
    let body = Zeroizing::new(
        request
            .bytes()
            .await
            .map_err(|_| IngressError::InvalidDocument)?,
    );
    if body.is_empty() || body.len() > MAX_REQUEST_BYTES {
        return Err(if body.len() > MAX_REQUEST_BYTES {
            IngressError::BodyTooLarge
        } else {
            IngressError::InvalidDocument
        });
    }
    let envelope: ResolverEnvelope =
        serde_json::from_slice(&body).map_err(|_| IngressError::InvalidDocument)?;
    if envelope.purpose != route.purpose() {
        return Err(IngressError::WrongPurpose);
    }
    validate_payload(route, &envelope.payload).map_err(|_| IngressError::InvalidPayload)?;
    if let Some(state) = envelope
        .payload
        .get("oauthState")
        .and_then(|value| value.as_str())
        && state.split_once('.').map(|(tenant, _)| tenant) != Some(envelope.tenant_id.as_str())
    {
        return Err(IngressError::CrossTenantState);
    }

    let version = required_header(request, SIGNATURE_VERSION_HEADER)?;
    let key_id = optional_header(request, KEY_ID_HEADER)?;
    let body_digest = required_header(request, BODY_DIGEST_HEADER)?;
    let timestamp = required_header(request, TIMESTAMP_HEADER)?;
    let timestamp_ms = timestamp
        .parse::<u64>()
        .map_err(|_| IngressError::InvalidAuthentication)?;
    let nonce = required_header(request, NONCE_HEADER)?;
    let signature = required_header(request, SIGNATURE_HEADER)?;
    let caller_secret = Zeroizing::new(
        env.secret(CALLER_AUTH_SECRET)
            .map_err(|_| IngressError::ConfigurationUnavailable)?
            .to_string(),
    );
    let keyring = ServiceAuthKeyring::parse(&caller_secret)
        .map_err(|_| IngressError::ConfigurationUnavailable)?;
    let verification_key = keyring
        .verification_key(&version, key_id.as_deref())
        .map_err(|_| IngressError::InvalidAuthentication)?;
    let signature_input = SignatureInput {
        method: "POST",
        path: &path,
        body: &body,
        tenant_id: &envelope.tenant_id,
        timestamp_ms,
        nonce: &nonce,
    };
    verify_versioned(
        verification_key,
        &version,
        key_id.as_deref(),
        &signature_input,
        &body_digest,
        &signature,
        now_ms,
    )
    .map_err(map_signature_error)?;

    Ok(AuthenticatedResolverRequest {
        route,
        path,
        envelope,
        body_digest,
        timestamp_ms,
        nonce,
    })
}

fn reject_oversized_content_length(request: &Request) -> Result<(), IngressError> {
    let value = request
        .headers()
        .get("content-length")
        .map_err(|_| IngressError::InvalidDocument)?;
    let Some(value) = value else {
        return Ok(());
    };
    let length = value
        .parse::<usize>()
        .map_err(|_| IngressError::InvalidDocument)?;
    if length > MAX_REQUEST_BYTES {
        return Err(IngressError::BodyTooLarge);
    }
    Ok(())
}

fn required_header(request: &Request, name: &str) -> Result<String, IngressError> {
    request
        .headers()
        .get(name)
        .map_err(|_| IngressError::InvalidAuthentication)?
        .filter(|value| !value.is_empty())
        .ok_or(IngressError::MissingAuthentication)
}

fn optional_header(request: &Request, name: &str) -> Result<Option<String>, IngressError> {
    request
        .headers()
        .get(name)
        .map_err(|_| IngressError::InvalidAuthentication)
        .map(|value| value.filter(|item| !item.is_empty()))
}

const fn map_signature_error(error: SignatureError) -> IngressError {
    match error {
        SignatureError::Stale => IngressError::StaleAuthentication,
        SignatureError::InvalidMetadata
        | SignatureError::InvalidDigest
        | SignatureError::InvalidSignature => IngressError::InvalidAuthentication,
    }
}

#[cfg(test)]
mod tests {
    use super::{IngressError, map_signature_error};
    use crate::protocol::SignatureError;

    #[test]
    fn only_stale_signatures_receive_the_stale_classification() {
        assert_eq!(
            map_signature_error(SignatureError::Stale),
            IngressError::StaleAuthentication
        );
        for error in [
            SignatureError::InvalidMetadata,
            SignatureError::InvalidDigest,
            SignatureError::InvalidSignature,
        ] {
            assert_eq!(
                map_signature_error(error),
                IngressError::InvalidAuthentication
            );
        }
    }
}
