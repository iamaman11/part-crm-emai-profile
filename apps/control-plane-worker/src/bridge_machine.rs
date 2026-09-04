use crate::access_session::verify_access_assertion;
use application_ports::DeviceJobPortErrorClass;
use application_ports::profile_launch::ProfileLaunchMachineBinding;
use cloudflare_adapters::d1_authenticated_device::D1AuthenticatedDevice;
use control_plane_contract::D1_CATALOG_BINDING;
use profile_platform_primitives::{CorrelationId, MachineCertificateFingerprint};
use worker::{Env, Error, Request, Result};

pub const BRIDGE_ACCESS_AUDIENCE_VAR: &str = "BRIDGE_ACCESS_AUDIENCE";

/// Authenticate the dedicated Bridge perimeter and resolve the actual machine through the existing
/// device-principal owner. Claim or coordinator payloads must not be parsed before this succeeds.
pub async fn resolve_bridge_machine(
    request: &Request,
    env: &Env,
    correlation_id: &CorrelationId,
) -> Result<Option<ProfileLaunchMachineBinding>> {
    let Some(_access_identity) =
        verify_access_assertion(request, env, BRIDGE_ACCESS_AUDIENCE_VAR).await?
    else {
        return Ok(None);
    };
    let Some(machine_fingerprint) = verified_mtls_fingerprint(request) else {
        return Ok(None);
    };

    let devices = D1AuthenticatedDevice::new(env.d1(D1_CATALOG_BINDING)?);
    match devices
        .resolve_machine_certificate_fingerprint(&machine_fingerprint)
        .await
    {
        Ok(value) => Ok(value),
        Err(error) => match error.class() {
            DeviceJobPortErrorClass::AuthenticationFailed => Ok(None),
            DeviceJobPortErrorClass::IntegrityFailure => Err(Error::RustError(format!(
                "Bridge machine identity integrity failure ({})",
                correlation_id.as_str()
            ))),
            DeviceJobPortErrorClass::DependencyUnavailable => Err(Error::RustError(format!(
                "Bridge machine identity dependency unavailable ({})",
                correlation_id.as_str()
            ))),
        },
    }
}

fn verified_mtls_fingerprint(request: &Request) -> Option<MachineCertificateFingerprint> {
    let tls = request.cf()?.tls_client_auth()?;
    if tls.cert_presented() != "1" || tls.cert_verified() != "SUCCESS" {
        return None;
    }
    MachineCertificateFingerprint::parse(tls.cert_fingerprint_sha256()).ok()
}

#[cfg(test)]
mod tests {
    use super::BRIDGE_ACCESS_AUDIENCE_VAR;
    use profile_platform_primitives::MachineCertificateFingerprint;

    #[test]
    fn bridge_machine_auth_uses_a_dedicated_access_audience() {
        assert_eq!(BRIDGE_ACCESS_AUDIENCE_VAR, "BRIDGE_ACCESS_AUDIENCE");
        assert_ne!(BRIDGE_ACCESS_AUDIENCE_VAR, "ACCESS_AUDIENCE");
    }

    #[test]
    fn bridge_device_identity_accepts_only_exact_sha256_certificate_fingerprint()
    -> Result<(), Box<dyn std::error::Error>> {
        let lower = "a1".repeat(32);
        let upper = lower.to_ascii_uppercase();
        assert_eq!(
            MachineCertificateFingerprint::parse(lower.clone())?.as_str(),
            lower
        );
        assert_eq!(MachineCertificateFingerprint::parse(upper)?.as_str(), lower);
        assert!(MachineCertificateFingerprint::parse("a".repeat(63)).is_err());
        assert!(MachineCertificateFingerprint::parse("g".repeat(64)).is_err());
        assert!(MachineCertificateFingerprint::parse(format!("{}:", "a".repeat(63))).is_err());
        Ok(())
    }
}
