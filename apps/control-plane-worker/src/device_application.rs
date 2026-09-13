use crate::access_session::{
    correlation_hint, neutral_not_found, problem, resolve_active_request_actor,
};
use cloudflare_adapters::d1_device_application_authority::D1DeviceApplicationAuthority;
use cloudflare_adapters::device_webcrypto::verify_p256_sha256;
use control_plane_contract::device_application_api::{
    DeviceApplicationSessionProjection, DevicePairingAuthorizeRequest,
    DevicePairingCompleteRequest, DevicePairingCreateProjection, DevicePairingCreateRequest,
    DeviceProofChallengeProjection, DeviceSessionChallengeRequest, DeviceSessionRenewRequest,
    OPAQUE_TOKEN_HEX_LENGTH, P256_SIGNATURE_P1363_HEX_LENGTH, P256_SPKI_DER_HEX_LENGTH,
};
use control_plane_contract::{D1_CATALOG_BINDING, RouteClass};
use device_domain::{DevicePublicKey, device_proof_message_v1};
use profile_platform_primitives::{
    AggregateVersion, CorrelationId, DeviceId, TenantId, UnixMillis,
};
use sha2::{Digest, Sha256};
use worker::wasm_bindgen::JsCast;
use worker::web_sys::WorkerGlobalScope;
use worker::{Date, Env, Error, Request, Response, Result};

const PAIRING_TTL_MS: u64 = 300_000;
const CHALLENGE_TTL_MS: u64 = 60_000;
const SESSION_TTL_MS: u64 = 600_000;
const TOKEN_BYTES: usize = OPAQUE_TOKEN_HEX_LENGTH / 2;
const P256_SIGNATURE_BYTES: usize = P256_SIGNATURE_P1363_HEX_LENGTH / 2;
const P256_SPKI_DER_BYTES: usize = P256_SPKI_DER_HEX_LENGTH / 2;

pub async fn dispatch(route: RouteClass, request: &mut Request, env: &Env) -> Result<Response> {
    match route {
        RouteClass::DevicePairingCollectionApi => create_pairing(request, env).await,
        RouteClass::DevicePairingAuthorizationApi => authorize_pairing(request, env).await,
        RouteClass::DevicePairingCompletionApi => complete_pairing(request, env).await,
        RouteClass::DeviceSessionChallengeApi => create_session_challenge(request, env).await,
        RouteClass::DeviceSessionCollectionApi => renew_session(request, env).await,
        _ => neutral_not_found(&correlation_hint(request)),
    }
}

async fn create_pairing(request: &mut Request, env: &Env) -> Result<Response> {
    let correlation_id = match request_correlation_id(request) {
        Some(value) => value,
        None => return invalid_request(&correlation_hint(request)),
    };
    let tenant_id = match path_tenant_id(request) {
        Some(value) => value,
        None => return invalid_request(correlation_id.as_str()),
    };
    let body = match request.json::<DevicePairingCreateRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id.as_str()),
    };
    let device_id = match DeviceId::parse(body.device_id().to_owned()) {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id.as_str()),
    };
    let public_key = match decode_public_key(body.public_key_spki_der_hex()) {
        Ok(value) => value,
        Err(()) => return invalid_request(correlation_id.as_str()),
    };
    let issued_at = now();
    let expires_at = match add_ttl(issued_at, PAIRING_TTL_MS) {
        Some(value) => value,
        None => return integrity_failure(correlation_id.as_str()),
    };
    let pairing_token = random_token()?;
    let pairing_digest = token_digest(&pairing_token);
    let authority = application_authority(env)?;
    if authority
        .create_pairing(
            &tenant_id,
            &pairing_digest,
            &device_id,
            &public_key,
            issued_at,
            expires_at,
        )
        .await
        .is_err()
    {
        return dependency_unavailable(correlation_id.as_str());
    }
    Response::from_json(&DevicePairingCreateProjection {
        pairing_token,
        expires_at_ms: expires_at.value(),
    })
    .map(|response| response.with_status(201))
}

async fn authorize_pairing(request: &mut Request, env: &Env) -> Result<Response> {
    let tenant_id = match path_tenant_id(request) {
        Some(value) => value,
        None => return neutral_not_found(&correlation_hint(request)),
    };
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id.as_str())).await?
    else {
        return neutral_not_found(&correlation_hint(request));
    };
    let correlation_id = actor.actor().correlation_id().as_str();
    let body = match request.json::<DevicePairingAuthorizeRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let pairing_digest = match checked_token_digest(body.pairing_token()) {
        Some(value) => value,
        None => return invalid_request(correlation_id),
    };
    let challenge_token = random_token()?;
    let challenge_digest = token_digest(&challenge_token);
    let nonce = random_bytes::<32>()?;
    let issued_at = now();
    let expires_at = match add_ttl(issued_at, CHALLENGE_TTL_MS) {
        Some(value) => value,
        None => return integrity_failure(correlation_id),
    };
    let authority = application_authority(env)?;
    if authority
        .authorize_pairing_with_challenge(
            actor.actor(),
            &pairing_digest,
            &challenge_digest,
            &nonce,
            issued_at,
            expires_at,
        )
        .await
        .is_err()
    {
        return neutral_not_found(correlation_id);
    }
    let proof = match authority
        .load_pairing_proof(&tenant_id, &pairing_digest, &challenge_digest, issued_at)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return integrity_failure(correlation_id),
        Err(_) => return dependency_unavailable(correlation_id),
    };
    if proof.actor_id() != actor.actor().actor_id()
        || proof.nonce() != &nonce
        || proof.expires_at() != expires_at
    {
        return integrity_failure(correlation_id);
    }
    Response::from_json(&DeviceProofChallengeProjection {
        challenge_token,
        device_id: proof.device_id().as_str().to_owned(),
        nonce_hex: hex_encode(&nonce),
        expires_at_ms: expires_at.value(),
    })
    .map(|response| response.with_status(200))
}

async fn complete_pairing(request: &mut Request, env: &Env) -> Result<Response> {
    let correlation_id = match request_correlation_id(request) {
        Some(value) => value,
        None => return invalid_request(&correlation_hint(request)),
    };
    let tenant_id = match path_tenant_id(request) {
        Some(value) => value,
        None => return neutral_not_found(correlation_id.as_str()),
    };
    let body = match request.json::<DevicePairingCompleteRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id.as_str()),
    };
    let pairing_digest = match checked_token_digest(body.pairing_token()) {
        Some(value) => value,
        None => return invalid_request(correlation_id.as_str()),
    };
    let challenge_digest = match checked_token_digest(body.challenge_token()) {
        Some(value) => value,
        None => return invalid_request(correlation_id.as_str()),
    };
    let signature = match decode_exact_hex::<P256_SIGNATURE_BYTES>(body.signature_p1363_hex()) {
        Some(value) => value,
        None => return invalid_request(correlation_id.as_str()),
    };
    let completed_at = now();
    let authority = application_authority(env)?;
    let proof = match authority
        .load_pairing_proof(&tenant_id, &pairing_digest, &challenge_digest, completed_at)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return neutral_not_found(correlation_id.as_str()),
        Err(_) => return dependency_unavailable(correlation_id.as_str()),
    };
    let message = device_proof_message_v1(
        &tenant_id,
        proof.actor_id(),
        proof.device_id(),
        proof.nonce(),
        proof.expires_at(),
    )
    .map_err(|error| Error::RustError(error.to_string()))?;
    if !verify_p256_sha256(proof.public_key(), &signature, &message).await? {
        return neutral_not_found(correlation_id.as_str());
    }
    let session_token = random_token()?;
    let session_digest = token_digest(&session_token);
    let session_expires_at = match add_ttl(completed_at, SESSION_TTL_MS) {
        Some(value) => value,
        None => return integrity_failure(correlation_id.as_str()),
    };
    let next_binding_version = match proof.latest_binding_version() {
        Some(version) => match version.next() {
            Ok(value) => value,
            Err(_) => return integrity_failure(correlation_id.as_str()),
        },
        None => AggregateVersion::INITIAL,
    };
    if authority
        .complete_verified_pairing(
            &tenant_id,
            &pairing_digest,
            &challenge_digest,
            &session_digest,
            proof.latest_binding_version(),
            next_binding_version,
            proof.auth_epoch(),
            completed_at,
            session_expires_at,
        )
        .await
        .is_err()
    {
        return neutral_not_found(correlation_id.as_str());
    }
    Response::from_json(&DeviceApplicationSessionProjection {
        session_token,
        device_id: proof.device_id().as_str().to_owned(),
        expires_at_ms: session_expires_at.value(),
    })
    .map(|response| response.with_status(201))
}

async fn create_session_challenge(request: &mut Request, env: &Env) -> Result<Response> {
    let correlation_id = match request_correlation_id(request) {
        Some(value) => value,
        None => return invalid_request(&correlation_hint(request)),
    };
    let tenant_id = match path_tenant_id(request) {
        Some(value) => value,
        None => return neutral_not_found(correlation_id.as_str()),
    };
    let device_id = match path_device_id(request) {
        Some(value) => value,
        None => return neutral_not_found(correlation_id.as_str()),
    };
    if request
        .json::<DeviceSessionChallengeRequest>()
        .await
        .is_err()
    {
        return invalid_request(correlation_id.as_str());
    }
    let authority = application_authority(env)?;
    let device = match authority
        .resolve_active_device(&tenant_id, &device_id)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return neutral_not_found(correlation_id.as_str()),
        Err(_) => return dependency_unavailable(correlation_id.as_str()),
    };
    let challenge_token = random_token()?;
    let challenge_digest = token_digest(&challenge_token);
    let nonce = random_bytes::<32>()?;
    let issued_at = now();
    let expires_at = match add_ttl(issued_at, CHALLENGE_TTL_MS) {
        Some(value) => value,
        None => return integrity_failure(correlation_id.as_str()),
    };
    if authority
        .create_session_challenge(
            &tenant_id,
            &challenge_digest,
            &device,
            &nonce,
            issued_at,
            expires_at,
        )
        .await
        .is_err()
    {
        return dependency_unavailable(correlation_id.as_str());
    }
    Response::from_json(&DeviceProofChallengeProjection {
        challenge_token,
        device_id: device_id.as_str().to_owned(),
        nonce_hex: hex_encode(&nonce),
        expires_at_ms: expires_at.value(),
    })
    .map(|response| response.with_status(201))
}

async fn renew_session(request: &mut Request, env: &Env) -> Result<Response> {
    let correlation_id = match request_correlation_id(request) {
        Some(value) => value,
        None => return invalid_request(&correlation_hint(request)),
    };
    let tenant_id = match path_tenant_id(request) {
        Some(value) => value,
        None => return neutral_not_found(correlation_id.as_str()),
    };
    let device_id = match path_device_id(request) {
        Some(value) => value,
        None => return neutral_not_found(correlation_id.as_str()),
    };
    let body = match request.json::<DeviceSessionRenewRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id.as_str()),
    };
    let challenge_digest = match checked_token_digest(body.challenge_token()) {
        Some(value) => value,
        None => return invalid_request(correlation_id.as_str()),
    };
    let signature = match decode_exact_hex::<P256_SIGNATURE_BYTES>(body.signature_p1363_hex()) {
        Some(value) => value,
        None => return invalid_request(correlation_id.as_str()),
    };
    let completed_at = now();
    let authority = application_authority(env)?;
    let proof = match authority
        .load_session_proof(&tenant_id, &challenge_digest, completed_at)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return neutral_not_found(correlation_id.as_str()),
        Err(_) => return dependency_unavailable(correlation_id.as_str()),
    };
    if proof.device_id() != &device_id {
        return neutral_not_found(correlation_id.as_str());
    }
    let message = device_proof_message_v1(
        &tenant_id,
        proof.actor_id(),
        proof.device_id(),
        proof.nonce(),
        proof.expires_at(),
    )
    .map_err(|error| Error::RustError(error.to_string()))?;
    if !verify_p256_sha256(proof.public_key(), &signature, &message).await? {
        return neutral_not_found(correlation_id.as_str());
    }
    let session_token = random_token()?;
    let session_digest = token_digest(&session_token);
    let session_expires_at = match add_ttl(completed_at, SESSION_TTL_MS) {
        Some(value) => value,
        None => return integrity_failure(correlation_id.as_str()),
    };
    if authority
        .complete_verified_session_renewal(
            &tenant_id,
            &challenge_digest,
            &session_digest,
            proof.auth_epoch(),
            proof.binding_version(),
            completed_at,
            session_expires_at,
        )
        .await
        .is_err()
    {
        return neutral_not_found(correlation_id.as_str());
    }
    Response::from_json(&DeviceApplicationSessionProjection {
        session_token,
        device_id: device_id.as_str().to_owned(),
        expires_at_ms: session_expires_at.value(),
    })
    .map(|response| response.with_status(201))
}

fn application_authority(env: &Env) -> Result<D1DeviceApplicationAuthority> {
    Ok(D1DeviceApplicationAuthority::new(
        env.d1(D1_CATALOG_BINDING)?,
    ))
}

fn request_correlation_id(request: &Request) -> Option<CorrelationId> {
    let value = request.headers().get("X-Correlation-Id").ok().flatten()?;
    CorrelationId::parse(value).ok()
}

fn path_tenant_id(request: &Request) -> Option<TenantId> {
    let path = request.path();
    let value = path.trim_matches('/').split('/').nth(3)?;
    TenantId::parse(value.to_owned()).ok()
}

fn path_device_id(request: &Request) -> Option<DeviceId> {
    let path = request.path();
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if segments.get(4).copied() != Some("devices") {
        return None;
    }
    DeviceId::parse((*segments.get(5)?).to_owned()).ok()
}

fn now() -> UnixMillis {
    UnixMillis::new(Date::now().as_millis())
}

fn add_ttl(now: UnixMillis, ttl_ms: u64) -> Option<UnixMillis> {
    now.value().checked_add(ttl_ms).map(UnixMillis::new)
}

fn random_token() -> Result<String> {
    random_bytes::<TOKEN_BYTES>().map(|bytes| hex_encode(&bytes))
}

fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    let global: WorkerGlobalScope = worker::js_sys::global().unchecked_into();
    global
        .crypto()?
        .get_random_values_with_u8_array(&mut bytes)
        .map_err(|_| Error::RustError("secure random generation failed".to_owned()))?;
    Ok(bytes)
}

fn checked_token_digest(token: &str) -> Option<String> {
    if token.len() != OPAQUE_TOKEN_HEX_LENGTH || !is_lower_hex(token) {
        return None;
    }
    Some(token_digest(token))
}

fn token_digest(token: &str) -> String {
    hex_encode(Sha256::digest(token.as_bytes()).as_slice())
}

fn decode_public_key(value: &str) -> std::result::Result<DevicePublicKey, ()> {
    let bytes = decode_exact_hex::<P256_SPKI_DER_BYTES>(value).ok_or(())?;
    DevicePublicKey::p256_spki_der(bytes.to_vec()).map_err(|_| ())
}

fn decode_exact_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !is_lower_hex(value) {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn integrity_failure(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        500,
        "integrity_failure",
        "Integrity Failure",
    )
}

fn dependency_unavailable(correlation_id: &str) -> Result<Response> {
    problem(
        correlation_id,
        503,
        "dependency_unavailable",
        "Dependency Unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CHALLENGE_TTL_MS, PAIRING_TTL_MS, SESSION_TTL_MS, add_ttl, checked_token_digest,
        decode_exact_hex, hex_encode, token_digest,
    };
    use profile_platform_primitives::UnixMillis;

    #[test]
    fn ttl_topology_stays_within_d1_contract_limits() {
        assert!(PAIRING_TTL_MS <= 600_000);
        assert!(CHALLENGE_TTL_MS <= 120_000);
        assert!(SESSION_TTL_MS <= 900_000);
        assert!(CHALLENGE_TTL_MS < SESSION_TTL_MS);
        assert!(SESSION_TTL_MS < PAIRING_TTL_MS * 3);
        assert_eq!(
            add_ttl(UnixMillis::new(10), CHALLENGE_TTL_MS)
                .expect("bounded ttl")
                .value(),
            10 + CHALLENGE_TTL_MS
        );
    }

    #[test]
    fn opaque_tokens_are_canonical_before_digesting() {
        let token = "ab".repeat(32);
        assert_eq!(checked_token_digest(&token), Some(token_digest(&token)));
        assert!(checked_token_digest(&"AB".repeat(32)).is_none());
        assert!(checked_token_digest(&"a".repeat(63)).is_none());
        assert_eq!(token_digest(&token).len(), 64);
    }

    #[test]
    fn proof_signature_transport_is_exact_p1363() {
        let bytes = [0xabu8; 64];
        let encoded = hex_encode(&bytes);
        assert_eq!(decode_exact_hex::<64>(&encoded), Some(bytes));
        assert!(decode_exact_hex::<64>(&encoded.to_ascii_uppercase()).is_none());
        assert!(decode_exact_hex::<64>(&encoded[..126]).is_none());
    }
}
