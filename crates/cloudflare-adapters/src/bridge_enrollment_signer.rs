use application_ports::bridge_enrollment::{
    BridgeEnrollmentCertificateProfile, BridgeEnrollmentCertificateSignRequest,
    BridgeEnrollmentCertificateSignerError, BridgeEnrollmentCertificateSignerErrorClass,
    BridgeEnrollmentCertificateSignerPort, MAX_BRIDGE_ENROLLMENT_CERTIFICATE_CHAIN_LENGTH,
    MAX_BRIDGE_ENROLLMENT_CERTIFICATE_DER_BYTES, Sha256Hex, SignedBridgeEnrollmentCertificate,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, RequestInit};

pub const BRIDGE_ENROLLMENT_CERTIFICATE_SIGNER_BINDING: &str = "BRIDGE_CERTIFICATE_SIGNER";
const BRIDGE_ENROLLMENT_CERTIFICATE_SIGNER_ENDPOINT: &str =
    "https://bridge-certificate-signer.internal/v1/bridge-enrollment/sign";
const MAX_SIGNER_RESPONSE_BYTES: usize = 512 * 1024;

pub struct CloudflareBridgeEnrollmentCertificateSigner<'a> {
    env: &'a Env,
}

impl<'a> CloudflareBridgeEnrollmentCertificateSigner<'a> {
    #[must_use]
    pub const fn new(env: &'a Env) -> Self {
        Self { env }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SignerRequest<'a> {
    tenant_id: &'a str,
    actor_id: &'a str,
    device_id: &'a str,
    csr_sha256: &'a str,
    csr_der_hex: String,
    profile: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignerResponse {
    csr_sha256: String,
    certificate_sha256: String,
    leaf_certificate_der_hex: String,
    certificate_chain_der_hex: Vec<String>,
}

impl BridgeEnrollmentCertificateSignerPort for CloudflareBridgeEnrollmentCertificateSigner<'_> {
    async fn sign_bridge_enrollment_certificate(
        &self,
        request: &BridgeEnrollmentCertificateSignRequest<'_>,
    ) -> Result<SignedBridgeEnrollmentCertificate, BridgeEnrollmentCertificateSignerError> {
        let recomputed_csr = sha256_hex(request.csr_der())?;
        if &recomputed_csr != request.csr_sha256() {
            return Err(signer_error(
                BridgeEnrollmentCertificateSignerErrorClass::CsrIdentityMismatch,
            ));
        }

        let reservation = request.reservation();
        let payload = SignerRequest {
            tenant_id: reservation.tenant_id().as_str(),
            actor_id: reservation.actor_id().as_str(),
            device_id: reservation.device_id().as_str(),
            csr_sha256: request.csr_sha256().as_str(),
            csr_der_hex: hex_encode(request.csr_der()),
            profile: match request.profile() {
                BridgeEnrollmentCertificateProfile::WindowsRsaSha256ClientAuthV1 => {
                    "windows_rsa_sha256_client_auth_v1"
                }
            },
        };
        let body = serde_json::to_string(&payload).map_err(|_| dependency_unavailable())?;
        let headers = Headers::new();
        headers
            .set("accept", "application/json")
            .map_err(|_| dependency_unavailable())?;
        headers
            .set("content-type", "application/json")
            .map_err(|_| dependency_unavailable())?;
        headers
            .set("cache-control", "no-store")
            .map_err(|_| dependency_unavailable())?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(JsValue::from_str(&body)));

        let signer = self
            .env
            .service(BRIDGE_ENROLLMENT_CERTIFICATE_SIGNER_BINDING)
            .map_err(|_| dependency_unavailable())?;
        let mut response = signer
            .fetch(BRIDGE_ENROLLMENT_CERTIFICATE_SIGNER_ENDPOINT, Some(init))
            .await
            .map_err(|_| dependency_unavailable())?;
        match response.status_code() {
            200 => {}
            400 | 409 | 422 => {
                return Err(signer_error(
                    BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
                ));
            }
            _ => return Err(dependency_unavailable()),
        }
        if response_content_length_exceeds(&response, MAX_SIGNER_RESPONSE_BYTES)? {
            return Err(signer_error(
                BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
            ));
        }
        let document = response
            .bytes()
            .await
            .map_err(|_| dependency_unavailable())?;
        if document.is_empty() || document.len() > MAX_SIGNER_RESPONSE_BYTES {
            return Err(signer_error(
                BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
            ));
        }
        let signed: SignerResponse = serde_json::from_slice(&document).map_err(|_| {
            signer_error(BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch)
        })?;
        let returned_csr = parse_sha256(&signed.csr_sha256)?;
        if returned_csr != *request.csr_sha256() {
            return Err(signer_error(
                BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
            ));
        }
        if signed.certificate_chain_der_hex.len() > MAX_BRIDGE_ENROLLMENT_CERTIFICATE_CHAIN_LENGTH {
            return Err(signer_error(
                BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
            ));
        }
        let leaf_certificate_der = decode_hex_bounded(
            &signed.leaf_certificate_der_hex,
            MAX_BRIDGE_ENROLLMENT_CERTIFICATE_DER_BYTES,
        )?;
        let mut certificate_chain_der = Vec::with_capacity(signed.certificate_chain_der_hex.len());
        for certificate in &signed.certificate_chain_der_hex {
            certificate_chain_der.push(decode_hex_bounded(
                certificate,
                MAX_BRIDGE_ENROLLMENT_CERTIFICATE_DER_BYTES,
            )?);
        }
        let certificate_sha256 = parse_sha256(&signed.certificate_sha256)?;
        if sha256_hex(&leaf_certificate_der)? != certificate_sha256 {
            return Err(signer_error(
                BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
            ));
        }
        SignedBridgeEnrollmentCertificate::for_request(
            request,
            certificate_sha256,
            leaf_certificate_der,
            certificate_chain_der,
        )
    }
}

fn response_content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, BridgeEnrollmentCertificateSignerError> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| dependency_unavailable())?;
    let Some(value) = value else {
        return Ok(false);
    };
    let length = value
        .parse::<usize>()
        .map_err(|_| dependency_unavailable())?;
    Ok(length > maximum)
}

fn parse_sha256(value: &str) -> Result<Sha256Hex, BridgeEnrollmentCertificateSignerError> {
    Sha256Hex::parse(value.to_owned()).map_err(|_| {
        signer_error(BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch)
    })
}

fn sha256_hex(value: &[u8]) -> Result<Sha256Hex, BridgeEnrollmentCertificateSignerError> {
    parse_sha256(&hex_encode(Sha256::digest(value).as_slice()))
}

fn decode_hex_bounded(
    value: &str,
    maximum: usize,
) -> Result<Vec<u8>, BridgeEnrollmentCertificateSignerError> {
    if value.is_empty()
        || value.len() % 2 != 0
        || value.len() / 2 > maximum
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(signer_error(
            BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch,
        ));
    }
    let mut decoded = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();
    for chunk in bytes.chunks_exact(2) {
        let high = hex_nibble(chunk[0]).ok_or_else(|| {
            signer_error(BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch)
        })?;
        let low = hex_nibble(chunk[1]).ok_or_else(|| {
            signer_error(BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch)
        })?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
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

fn signer_error(
    class: BridgeEnrollmentCertificateSignerErrorClass,
) -> BridgeEnrollmentCertificateSignerError {
    BridgeEnrollmentCertificateSignerError::new(class)
}

fn dependency_unavailable() -> BridgeEnrollmentCertificateSignerError {
    signer_error(BridgeEnrollmentCertificateSignerErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{decode_hex_bounded, hex_encode, sha256_hex};

    #[test]
    fn public_certificate_hex_is_exact_lowercase_and_bounded()
    -> Result<(), Box<dyn std::error::Error>> {
        let der = [0x30, 0x03, 0x02, 0x01, 0x00];
        let encoded = hex_encode(&der);
        assert_eq!(encoded, "3003020100");
        assert_eq!(decode_hex_bounded(&encoded, 16)?, der);
        assert!(decode_hex_bounded("30FF", 16).is_err());
        assert!(decode_hex_bounded("300", 16).is_err());
        assert!(decode_hex_bounded("3000", 1).is_err());
        assert_eq!(sha256_hex(&der)?.as_str().len(), 64);
        Ok(())
    }
}
