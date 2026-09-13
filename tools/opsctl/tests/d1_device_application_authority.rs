use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn expired_pairing_cannot_permanently_lock_the_native_public_key() -> Result<(), Box<dyn Error>> {
    let sql = fs::read_to_string(
        repository_root().join("migrations/d1-successor-v2/0034_device_application_authority.sql"),
    )?;

    assert!(sql.contains(
        "CREATE INDEX device_pairing_transactions_key_lookup\n    ON device_pairing_transactions(public_key_spki_der_hex, expires_at_ms);"
    ));
    assert!(!sql.contains("CREATE UNIQUE INDEX device_pairing_transactions_one_live_key"));
    assert!(!sql.contains("WHERE consumed_at_ms IS NULL;\n\nCREATE TRIGGER device_pairing_transactions_core_immutable"));

    Ok(())
}
