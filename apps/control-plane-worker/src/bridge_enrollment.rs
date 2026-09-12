use crate::access_session::{correlation_hint, neutral_not_found, problem, resolve_active_request_actor};
use crate::command_evidence;
use application_ports::{
    BridgeEnrollmentAuthorityError, BridgeEnrollmentAuthorityErrorClass,
    BridgeEnrollmentAuthorityPort, BridgeEnrollmentCertificateSignRequest,
    BridgeEnrollmentCertificateSignerError, BridgeEnrollmentCertificateSignerErrorClass,
    BridgeEnrollmentCertificateSignerPort, Sha256Hex,
};
use application_ports::bridge_enrollment::MAX_BRIDGE_ENROLLMENT_CSR_DER_BYTES;
use cloudflare_adapters::bridge_enrollment_signer::CloudflareBridgeEnrollmentCertificateSigner;
use cloudflare_adapters::d1_bridge_enrollment::D1BridgeEnrollmentAuthority;
use control_plane_contract::bridge_enrollment_api::{
    BridgeEnrollmentIssueProjection, BridgeEnrollmentIssueRequest,
    BridgeEnrollmentRedemptionProjection, BridgeEnrollmentRedemptionRequest,
};
use control_plane_contract::D1_CATALOG_BINDING;
use profile_platform_primitives::UnixMillis;
use sha2::{Digest, Sha256};
use worker::{Date, Env, Error, Request, Response, Result};

const BRIDGE_ENROLLMENT_DERIVATION_KEY_BINDING: &str = "BRIDGE_ENROLLMENT_DERIVATION_KEY";
const ISSUE_PATH_SUFFIX: &str = "/bridge-enrollment/authorities";
const REDEEM_PATH_SUFFIX: &str = "/bridge-enrollment/redemptions";

pub async fn dispatch(request: &mut Request, env: &Env) -> Result<Response> {
    let path = request.path();
    let tenant_id = path
        .trim_matches('/')
        .split('/')
        .nth(3)
        .unwrap_or_default();
    if path.ends_with(ISSUE_PATH_SUFFIX) {
        issue(request, env, tenant_id).await
    } else if path.ends_with(REDEEM_PATH_SUFFIX) {
        redeem(request, env, tenant_id).await
    } else {
        neutral_not_found(&correlation_hint(request))
    }
}

async fn issue(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let correlation_id = actor.actor().correlation_id().as_str();
    let body = match request.json::<BridgeEnrollmentIssueRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let evidence = match command_evidence::from_request(request, actor.actor(), &body) {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let authority = match enrollment_authority(env) {
        Ok(value) => value,
        Err(()) => return dependency_unavailable(correlation_id),
    };
    match authority
        .issue_bridge_enrollment_authority(actor.actor(), &evidence)
        .await
    {
        Ok(issued) => {
            let status = if issued.replayed() { 200 } else { 201 };
            Response::from_json(&BridgeEnrollmentIssueProjection {
                claim_code: issued.claim_code().to_owned(),
                expires_at_ms: issued.expires_at().value(),
                device_id: issued.device_id().as_str().to_owned(),
            })
            .map(|response| response.with_status(status))
        }
        Err(error) => authority_failure(correlation_id, error),
    }
}

async fn redeem(request: &mut Request, env: &Env, tenant_id: &str) -> Result<Response> {
    let Some(actor) = resolve_active_request_actor(request, env, Some(tenant_id)).await? else {
        return neutral_not_found(&correlation_hint(request));
    };
    let correlation_id = actor.actor().correlation_id().as_str();
    let body = match request.json::<BridgeEnrollmentRedemptionRequest>().await {
        Ok(value) => value,
        Err(_) => return invalid_request(correlation_id),
    };
    let csr_der = match decode_exact_csr_der(body.csr_der_hex()) {
        Ok(value) => value,
        Err(()) => return invalid_request(correlation_id),
    };
    let csr_sha256 = machine_sha256(&csr_der)?;
    let authority = match enrollment_authority(env) {
        Ok(value) => value,
        Err(()) => return dependency_unavailable(correlation_id),
    };
    let reservation = match authority
        .reserve_bridge_enrollment_csr(
            body.claim_code(),
            &csr_sha256,
            UnixMillis::new(Date::now().as_millis()),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return authority_failure(correlation_id, error),
    };
    if reservation.tenant_id() != actor.actor().tenant_scope().tenant_id()
        || reservation.actor_id() != actor.actor().actor_id()
    {
        return neutral_not_found(correlation_id);
    }

    let sign_request = match BridgeEnrollmentCertificateSignRequest::new(
        &reservation,
        &csr_der,
        csr_sha256.clone(),
    ) {
        Ok(value) => value,
        Err(error) => return signer_failure(correlation_id, error),
    };
    let signer = CloudflareBridgeEnrollmentCertificateSigner::new(env);
    let signed = match signer.sign_bridge_enrollment_certificate(&sign_request).await {
        Ok(value) => value,
        Err(error) => return signer_failure(correlation_id, error),
    };
    if let Err(error) = signed.validate_for_request(&sign_request) {
        return signer_failure(correlation_id, error);
    }
    let exact_leaf_sha256 = machine_sha256(signed.leaf_certificate_der())?;
    if &exact_leaf_sha256 != signed.certificate_sha256() {
        return integrity_failure(correlation_id);
    }

    let completion = match authority
        .finalize_bridge_enrollment_certificate(
            body.claim_code(),
            &csr_sha256,
            &exact_leaf_sha256,
            UnixMillis::new(Date::now().as_millis()),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return authority_failure(correlation_id, error),
    };
    if completion.reservation().tenant_id() != reservation.tenant_id()
        || completion.reservation().actor_id() != reservation.actor_id()
        || completion.reservation().device_id() != reservation.device_id()
        || completion.reservation().csr_sha256() != reservation.csr_sha256()
        || completion.certificate_sha256() != signed.certificate_sha256()
    {
        return integrity_failure(correlation_id);
    }

    Response::from_json(&BridgeEnrollmentRedemptionProjection {
        device_id: reservation.device_id().as_str().to_owned(),
        certificate_sha256: exact_leaf_sha256.as_str().to_owned(),
        leaf_certificate_der_hex: hex_encode(signed.leaf_certificate_der()),
        certificate_chain_der_hex: signed
            .certificate_chain_der()
            .iter()
            .map(|certificate| hex_encode(certificate))
            .collect(),
    })
    .map(|response| response.with_status(200))
}

fn enrollment_authority(env: &Env) -> std::result::Result<D1BridgeEnrollmentAuthority, ()> {
    let derivation_key = env
        .secret(BRIDGE_ENROLLMENT_DERIVATION_KEY_BINDING)
        .map_err(|_| ())?
        .to_string();
    let database = env.d1(D1_CATALOG_BINDING).map_err(|_| ())?;
    D1BridgeEnrollmentAuthority::new(database, derivation_key).map_err(|_| ())
}

fn decode_exact_csr_der(value: &str) -> std::result::Result<Vec<u8>, ()> {
    if value.is_empty()
        || value.len() % 2 != 0
        || value.len() / 2 > MAX_BRIDGE_ENROLLMENT_CSR_DER_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(());
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let high = hex_nibble(chunk[0]).ok_or(())?;
        let low = hex_nibble(chunk[1]).ok_or(())?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn machine_sha256(value: &[u8]) -> Result<Sha256Hex> {
    Sha256Hex::parse(hex_encode(Sha256::digest(value).as_slice()))
        .map_err(|_| Error::RustError("machine SHA-256 identity failure".to_owned()))
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

fn authority_failure(correlation_id: &str, error: BridgeEnrollmentAuthorityError) -> Result<Response> {
    match error.class() {
        BridgeEnrollmentAuthorityErrorClass::NotFound => neutral_not_found(correlation_id),
        BridgeEnrollmentAuthorityErrorClass::ReplayRejected => {
            problem(correlation_id, 409, "replay_rejected", "Replay Rejected")
        }
        BridgeEnrollmentAuthorityErrorClass::Conflict => {
            problem(correlation_id, 409, "conflict", "Conflict")
        }
        BridgeEnrollmentAuthorityErrorClass::IntegrityFailure => integrity_failure(correlation_id),
        BridgeEnrollmentAuthorityErrorClass::DependencyUnavailable => dependency_unavailable(correlation_id),
    }
}

fn signer_failure(
    correlation_id: &str,
    error: BridgeEnrollmentCertificateSignerError,
) -> Result<Response> {
    match error.class() {
        BridgeEnrollmentCertificateSignerErrorClass::MalformedCsr
        | BridgeEnrollmentCertificateSignerErrorClass::UnsupportedKeyProfile
        | BridgeEnrollmentCertificateSignerErrorClass::ProofOfPossessionRejected
        | BridgeEnrollmentCertificateSignerErrorClass::CsrIdentityMismatch => {
            invalid_request(correlation_id)
        }
        BridgeEnrollmentCertificateSignerErrorClass::CertificateIdentityMismatch => {
            integrity_failure(correlation_id)
        }
        BridgeEnrollmentCertificateSignerErrorClass::DependencyUnavailable => {
            dependency_unavailable(correlation_id)
        }
    }
}

fn invalid_request(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 400, "invalid_request", "Invalid Request")
}

fn integrity_failure(correlation_id: &str) -> Result<Response> {
    problem(correlation_id, 500, "integrity_failure", "Integrity Failure")
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
    use super::{decode_exact_csr_der, hex_encode, machine_sha256};

    #[test]
    fn csr_transport_is_single_exact_lowercase_der_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let der = [0x30, 0x03, 0x02, 0x01, 0x00];
        let encoded = hex_encode(&der);
        let decoded = decode_exact_csr_der(&encoded)
            .map_err(|()| std::io::Error::other("decode failed"))?;
        assert_eq!(decoded.as_slice(), der.as_slice());
        assert!(decode_exact_csr_der("30FF").is_err());
        assert!(decode_exact_csr_der("300").is_err());
        assert_eq!(machine_sha256(&der)?.as_str().len(), 64);
        Ok(())
    }
}
