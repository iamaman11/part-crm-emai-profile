use crate::model::MAX_CLOCK_SKEW_MS;
use control_plane_contract::resolver_service_auth::canonical_signature_input;
pub use control_plane_contract::resolver_service_auth::{
    KEYED_SIGNATURE_VERSION, LEGACY_SIGNATURE_VERSION,
};
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureInput<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub body: &'a [u8],
    pub tenant_id: &'a str,
    pub timestamp_ms: u64,
    pub nonce: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignatureError {
    InvalidMetadata,
    Stale,
    InvalidDigest,
    InvalidSignature,
}

#[must_use]
pub fn body_digest_hex(body: &[u8]) -> String {
    hex_encode(Sha256::digest(body).as_slice())
}

pub fn sign_hex(secret: &[u8], input: &SignatureInput<'_>) -> Result<String, SignatureError> {
    sign_hex_versioned(secret, LEGACY_SIGNATURE_VERSION, None, input)
}

pub fn sign_hex_versioned(
    secret: &[u8],
    version: &str,
    key_id: Option<&str>,
    input: &SignatureInput<'_>,
) -> Result<String, SignatureError> {
    validate_metadata(input)?;
    let digest = body_digest_hex(input.body);
    let canonical = canonical_signature_input(
        version,
        key_id,
        input.method,
        input.path,
        &digest,
        input.tenant_id,
        input.timestamp_ms,
        input.nonce,
    )
    .map_err(|_| SignatureError::InvalidMetadata)?;
    let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(secret)
        .map_err(|_| SignatureError::InvalidSignature)?;
    mac.update(canonical.as_bytes());
    Ok(hex_encode(mac.finalize().into_bytes().as_slice()))
}

pub fn verify(
    secret: &[u8],
    input: &SignatureInput<'_>,
    supplied_body_digest: &str,
    supplied_signature: &str,
    now_ms: u64,
) -> Result<(), SignatureError> {
    verify_versioned(
        secret,
        LEGACY_SIGNATURE_VERSION,
        None,
        input,
        supplied_body_digest,
        supplied_signature,
        now_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn verify_versioned(
    secret: &[u8],
    version: &str,
    key_id: Option<&str>,
    input: &SignatureInput<'_>,
    supplied_body_digest: &str,
    supplied_signature: &str,
    now_ms: u64,
) -> Result<(), SignatureError> {
    validate_metadata(input)?;
    if input.timestamp_ms.abs_diff(now_ms) > MAX_CLOCK_SKEW_MS {
        return Err(SignatureError::Stale);
    }
    let digest = body_digest_hex(input.body);
    if !constant_time_hex_eq(&digest, supplied_body_digest) {
        return Err(SignatureError::InvalidDigest);
    }
    let expected = sign_hex_versioned(secret, version, key_id, input)?;
    if !constant_time_hex_eq(&expected, supplied_signature) {
        return Err(SignatureError::InvalidSignature);
    }
    Ok(())
}

fn validate_metadata(input: &SignatureInput<'_>) -> Result<(), SignatureError> {
    let valid_method = input.method == "POST";
    let valid_path = input.path.starts_with("/v1/mailbox-credentials/")
        && input.path.len() <= 160
        && !input.path.contains('?');
    let valid_tenant = valid_identifier(input.tenant_id, 128);
    let valid_nonce =
        input.nonce.len() == 32 && input.nonce.bytes().all(|value| value.is_ascii_hexdigit());
    if valid_method && valid_path && valid_tenant && valid_nonce && input.timestamp_ms > 0 {
        Ok(())
    } else {
        Err(SignatureError::InvalidMetadata)
    }
}

fn valid_identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

fn constant_time_hex_eq(expected: &str, supplied: &str) -> bool {
    if expected.len() != supplied.len() {
        return false;
    }
    expected
        .bytes()
        .zip(supplied.bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[must_use]
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        KEYED_SIGNATURE_VERSION, LEGACY_SIGNATURE_VERSION, SignatureError, SignatureInput,
        body_digest_hex, sign_hex, sign_hex_versioned, verify, verify_versioned,
    };

    fn input<'a>(body: &'a [u8]) -> SignatureInput<'a> {
        SignatureInput {
            method: "POST",
            path: "/v1/mailbox-credentials/resolve",
            body,
            tenant_id: "tenant_01",
            timestamp_ms: 1_000_000,
            nonce: "00112233445566778899aabbccddeeff",
        }
    }

    #[test]
    fn legacy_v1_wire_contract_is_preserved() -> Result<(), SignatureError> {
        let request = input(br#"{"tenantId":"tenant_01"}"#);
        let digest = body_digest_hex(request.body);
        let signature = sign_hex(b"caller-auth-key", &request)?;
        assert!(verify(b"caller-auth-key", &request, &digest, &signature, 1_000_001).is_ok());
        assert_eq!(
            sign_hex_versioned(b"caller-auth-key", LEGACY_SIGNATURE_VERSION, None, &request)?,
            signature
        );
        Ok(())
    }

    #[test]
    fn keyed_v2_authenticates_exact_key_id() -> Result<(), SignatureError> {
        let request = input(br#"{"tenantId":"tenant_01"}"#);
        let digest = body_digest_hex(request.body);
        let signature = sign_hex_versioned(
            b"new-caller-auth-key",
            KEYED_SIGNATURE_VERSION,
            Some("key-2026-08"),
            &request,
        )?;
        verify_versioned(
            b"new-caller-auth-key",
            KEYED_SIGNATURE_VERSION,
            Some("key-2026-08"),
            &request,
            &digest,
            &signature,
            1_000_001,
        )?;
        assert_eq!(
            verify_versioned(
                b"new-caller-auth-key",
                KEYED_SIGNATURE_VERSION,
                Some("other-key"),
                &request,
                &digest,
                &signature,
                1_000_001,
            ),
            Err(SignatureError::InvalidSignature)
        );
        Ok(())
    }

    #[test]
    fn version_and_key_id_shapes_are_fail_closed() {
        let request = input(b"{}");
        assert_eq!(
            sign_hex_versioned(b"key", KEYED_SIGNATURE_VERSION, None, &request),
            Err(SignatureError::InvalidMetadata)
        );
        assert_eq!(
            sign_hex_versioned(
                b"key",
                LEGACY_SIGNATURE_VERSION,
                Some("legacy-v1"),
                &request
            ),
            Err(SignatureError::InvalidMetadata)
        );
        assert_eq!(
            sign_hex_versioned(b"key", "hmac-sha256-v99", None, &request),
            Err(SignatureError::InvalidMetadata)
        );
    }

    #[test]
    fn method_path_body_tenant_time_and_nonce_are_authenticated() -> Result<(), SignatureError> {
        let request = input(br#"{"tenantId":"tenant_01"}"#);
        let digest = body_digest_hex(request.body);
        let signature = sign_hex(b"caller-auth-key", &request)?;
        let mut cross_tenant = input(request.body);
        cross_tenant.tenant_id = "tenant_02";
        assert_eq!(
            verify(
                b"caller-auth-key",
                &cross_tenant,
                &digest,
                &signature,
                1_000_001
            ),
            Err(SignatureError::InvalidSignature)
        );
        Ok(())
    }

    #[test]
    fn stale_and_tampered_requests_fail_closed() -> Result<(), SignatureError> {
        let request = input(br#"{"tenantId":"tenant_01"}"#);
        let digest = body_digest_hex(request.body);
        let signature = sign_hex(b"caller-auth-key", &request)?;
        assert_eq!(
            verify(b"caller-auth-key", &request, &digest, &signature, 1_400_001),
            Err(SignatureError::Stale)
        );
        assert_eq!(
            verify(
                b"caller-auth-key",
                &input(br#"{"tenantId":"tenant_01","extra":true}"#),
                &digest,
                &signature,
                1_000_001
            ),
            Err(SignatureError::InvalidDigest)
        );
        Ok(())
    }
}
