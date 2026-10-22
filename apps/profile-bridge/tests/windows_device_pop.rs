#![cfg(windows)]
#![forbid(unsafe_code)]

use device_domain::{DEVICE_PROOF_NONCE_BYTES, device_proof_message_v1};
use profile_platform_primitives::{ActorId, DeviceId, TenantId, UnixMillis};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_device_key::{
    KeyDisposition, P256_SPKI_DER_BYTES, P1363_SIGNATURE_BYTES, PersistedP256Key,
};

#[test]
fn native_windows_cng_p256_key_is_reused_non_exportable_and_signs_canonical_pop()
-> Result<(), Box<dyn std::error::Error>> {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let key_name = format!("part-crm-v2-1-proof-{}-{unique}", std::process::id());

    let first = PersistedP256Key::open_or_create(&key_name)?;
    assert_eq!(first.disposition(), KeyDisposition::Created);
    assert!(first.private_key_export_blocked()?);
    let public_key = first.public_key_spki_der()?;
    assert_eq!(public_key.len(), P256_SPKI_DER_BYTES);
    assert_eq!(&public_key[..2], &[0x30, 0x59]);

    let tenant_id = TenantId::parse("tenant_01JV2CNG")?;
    let actor_id = ActorId::parse("actor_01JV2CNG")?;
    let device_id = DeviceId::parse("device_01JV2CNG")?;
    let nonce = [0xA5_u8; DEVICE_PROOF_NONCE_BYTES];
    let expires_at = UnixMillis::new(1_900_000_000_000);
    let message = device_proof_message_v1(&tenant_id, &actor_id, &device_id, &nonce, expires_at)?;
    let signature = first.sign_sha256_message(&message)?;
    assert_eq!(signature.len(), P1363_SIGNATURE_BYTES);
    assert!(first.verify_sha256_message(&message, &signature)?);
    drop(first);

    let reopened = PersistedP256Key::open_or_create(&key_name)?;
    assert_eq!(reopened.disposition(), KeyDisposition::OpenedExisting);
    assert_eq!(reopened.public_key_spki_der()?, public_key);
    assert!(reopened.private_key_export_blocked()?);
    assert!(reopened.verify_sha256_message(&message, &signature)?);

    let wrong_nonce = [0x5A_u8; DEVICE_PROOF_NONCE_BYTES];
    let wrong_message =
        device_proof_message_v1(&tenant_id, &actor_id, &device_id, &wrong_nonce, expires_at)?;
    assert!(!reopened.verify_sha256_message(&wrong_message, &signature)?);

    reopened.delete()?;
    Ok(())
}
