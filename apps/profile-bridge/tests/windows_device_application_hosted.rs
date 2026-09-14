#![cfg(windows)]

use bridge_domain::DeviceKeyPort;
use device_domain::{BridgeRequestProofMethod, bridge_request_proof_message_v1};
use profile_bridge::windows_device_application::{
    WindowsDeviceApplication, WindowsDeviceApplicationBinding,
};
use profile_platform_primitives::{ActorId, CorrelationId, DeviceId, TenantId, UnixMillis};
use sha2::{Digest, Sha256};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use windows_device_key::PersistedP256Key;

#[test]
fn shipping_cng_request_proof_survives_process_restart() -> Result<(), Box<dyn std::error::Error>> {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let tenant_id = TenantId::parse(format!("tenant_v21_{unique}"))?;
    let actor_id = ActorId::parse(format!("actor_v21_{unique}"))?;
    let device_id = DeviceId::parse(format!("device_v21_{unique}"))?;
    let correlation_id = CorrelationId::parse(format!("corr_v21_{unique}"))?;
    let binding_path = std::env::temp_dir().join(format!(
        "part-crm-v21-device-binding-{}-{unique}.json",
        std::process::id()
    ));
    fs::write(
        &binding_path,
        format!(
            r#"{{"tenantId":"{}","actorId":"{}","deviceId":"{}"}}"#,
            tenant_id.as_str(),
            actor_id.as_str(),
            device_id.as_str()
        ),
    )?;

    let cleanup_key_name = format!("part-crm.device.{}", device_id.as_str());
    let proof_result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let binding = WindowsDeviceApplicationBinding::open(&binding_path)?;
        assert_eq!(binding.tenant_id(), &tenant_id);
        assert_eq!(binding.actor_id(), &actor_id);
        assert_eq!(binding.device_id(), &device_id);

        let mut first =
            WindowsDeviceApplication::from_system("https://control.example.invalid", binding)?;
        let first_handle = first.ensure_key_handle(&device_id)?;
        let first_key = PersistedP256Key::open_or_create(&first_handle)?;
        first_key.require_non_exportable()?;
        assert!(first_key.private_key_export_blocked()?);
        let public_key = first_key.public_key_spki_der()?;

        let mut session_digest = [0_u8; 32];
        session_digest.copy_from_slice(&Sha256::digest(b"hosted-windows-application-session"));
        let mut body_digest = [0_u8; 32];
        body_digest.copy_from_slice(&Sha256::digest(br#"{"profileId":"profile_v21"}"#));
        let expires_at = UnixMillis::new(30_000);
        let message = bridge_request_proof_message_v1(
            &tenant_id,
            &actor_id,
            &device_id,
            &session_digest,
            BridgeRequestProofMethod::PostJson,
            "/bridge/api/v1/profile-launch-redemptions",
            &correlation_id,
            &body_digest,
            expires_at,
        )?;
        let signature = first_key.sign_sha256_message(&message)?;
        assert!(first_key.verify_sha256_message(&message, &signature)?);

        let mut changed_body_digest = [0_u8; 32];
        changed_body_digest.copy_from_slice(&Sha256::digest(br#"{"profileId":"other"}"#));
        let changed_message = bridge_request_proof_message_v1(
            &tenant_id,
            &actor_id,
            &device_id,
            &session_digest,
            BridgeRequestProofMethod::PostJson,
            "/bridge/api/v1/profile-launch-redemptions",
            &correlation_id,
            &changed_body_digest,
            expires_at,
        )?;
        assert!(!first_key.verify_sha256_message(&changed_message, &signature)?);
        drop(first_key);
        drop(first);

        let restarted_binding = WindowsDeviceApplicationBinding::open(&binding_path)?;
        let mut restarted = WindowsDeviceApplication::from_system(
            "https://control.example.invalid",
            restarted_binding,
        )?;
        let restarted_handle = restarted.ensure_key_handle(&device_id)?;
        assert_eq!(restarted_handle, first_handle);
        let restarted_key = PersistedP256Key::open_or_create(&restarted_handle)?;
        restarted_key.require_non_exportable()?;
        assert_eq!(restarted_key.public_key_spki_der()?, public_key);
        assert!(restarted_key.verify_sha256_message(&message, &signature)?);
        drop(restarted_key);
        drop(restarted);
        Ok(())
    })();

    let _ = fs::remove_file(&binding_path);
    if let Ok(key) = PersistedP256Key::open_or_create(&cleanup_key_name) {
        let _ = key.delete();
    }
    proof_result
}
