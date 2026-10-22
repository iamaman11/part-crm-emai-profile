use super::{NativeInstallerError, bundle::InstallBundle};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
static SEQUENCE: AtomicU64 = AtomicU64::new(1);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Result<Self, std::io::Error> {
        let path = std::env::temp_dir().join(format!(
            "bridge-install-bundle-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn bootstrap() -> Vec<u8> {
    format!(r#"{{"schema_version":1,"kind":"WINDOWS_PROFILE_BRIDGE_INSTALLATION","control_plane_origin":"https://control.example.com","signer_key_id":"publisher","signer_certificate_sha256":"{}"}}"#, "a".repeat(64)).into_bytes()
}
fn fixture() -> Result<Directory, std::io::Error> {
    let directory = Directory::new()?;
    fs::write(directory.0.join("installation.json"), bootstrap())?;
    fs::write(
        directory.0.join("windows-delivery-manifest.json"),
        b"unsigned-manifest",
    )?;
    fs::write(
        directory.0.join("windows-delivery-signature.json"),
        b"unsigned-signature",
    )?;
    Ok(directory)
}
#[test]
fn bootstrap_only_decodes_inputs_and_never_admits_an_unsigned_delivery() -> TestResult {
    let directory = fixture()?;
    let bundle = InstallBundle::open(&directory.0)?;
    assert_eq!(bundle.origin.as_str(), "https://control.example.com");
    struct NoSignature;
    impl crate::windows_delivery::DetachedSignatureVerifier for NoSignature {
        type Error = ();
        fn verify_cms(&mut self, _: &[u8], _: &[u8], _: &str) -> Result<bool, ()> {
            Err(())
        }
    }
    assert!(
        crate::windows_delivery::verify_delivery_candidate(
            &bundle.manifest,
            &bundle.signature,
            &bundle.trust,
            None,
            &mut NoSignature
        )
        .is_err()
    );
    assert!(!directory.0.join("state").exists());
    Ok(())
}
#[test]
fn bootstrap_contract_is_closed_versioned_and_duplicate_safe() -> TestResult {
    let directory = fixture()?;
    let valid = String::from_utf8(bootstrap())?;
    for invalid in [
        valid.replace("\"schema_version\":1", "\"schema_version\":2"),
        valid.replace(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
        ),
        valid.replace("INSTALLATION", "UNKNOWN"),
        valid.replace("https://control", "http://control"),
        valid.replace("\"signer_key_id\":", "\"extra\":true,\"signer_key_id\":"),
        valid.replace(&"a".repeat(64), "not-a-pin"),
        " ".repeat(4097),
    ] {
        fs::write(directory.0.join("installation.json"), invalid)?;
        assert!(matches!(
            InstallBundle::open(&directory.0),
            Err(NativeInstallerError::InvalidBundle)
        ));
    }
    Ok(())
}
#[test]
fn missing_and_oversized_signature_inputs_never_become_defaults() -> TestResult {
    let directory = fixture()?;
    let signature = directory.0.join("windows-delivery-signature.json");
    fs::remove_file(&signature)?;
    assert!(matches!(
        InstallBundle::open(&directory.0),
        Err(NativeInstallerError::InputIo)
    ));
    fs::write(&signature, vec![0; 2 * 1024 * 1024 + 4097])?;
    assert!(matches!(
        InstallBundle::open(&directory.0),
        Err(NativeInstallerError::InvalidBundle)
    ));
    assert!(!directory.0.join("connection.json").exists());
    Ok(())
}
#[cfg(unix)]
#[test]
fn bootstrap_symlink_and_symlinked_ancestor_are_rejected() -> TestResult {
    use std::os::unix::fs::symlink;
    let directory = fixture()?;
    let external = Directory::new()?;
    let linked_root = external.0.join("linked");
    symlink(&directory.0, &linked_root)?;
    assert!(InstallBundle::open(&linked_root).is_err());
    fs::remove_file(linked_root)?;
    fs::remove_file(directory.0.join("installation.json"))?;
    fs::write(external.0.join("real.json"), bootstrap())?;
    symlink(
        external.0.join("real.json"),
        directory.0.join("installation.json"),
    )?;
    assert!(matches!(
        InstallBundle::open(&directory.0),
        Err(NativeInstallerError::InvalidBundle)
    ));
    Ok(())
}
