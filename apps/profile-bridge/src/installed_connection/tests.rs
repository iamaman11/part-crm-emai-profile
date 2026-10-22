use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Result<Self, std::io::Error> {
        let directory = std::env::temp_dir().join(format!(
            "bridge-connection-test-{}-{}",
            std::process::id(),
            WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory)?;
        Ok(Self(directory))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        // The test created this exact unique directory under the OS temporary directory.
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn origin() -> Result<ControlPlaneOrigin, BridgePortError> {
    ControlPlaneOrigin::parse("https://control.example.com")
}

#[test]
fn installation_origin_cannot_carry_credentials_paths_or_handoff_parameters() -> TestResult {
    assert_eq!(origin()?.as_str(), "https://control.example.com");
    for invalid in [
        "",
        "http://control.example.com",
        "https://",
        "https://user@control.example.com",
        "https://control.example.com/",
        "https://control.example.com?x=1",
        "https://control.example.com#claim",
        "https://control.example.com\\path",
        "https://control.example.com\n",
    ] {
        assert!(ControlPlaneOrigin::parse(invalid).is_err(), "{invalid:?}");
    }
    Ok(())
}

#[test]
fn setup_record_is_complete_and_exact_retry_is_read_only() -> TestResult {
    let directory = Directory::new()?;
    let prepared = InstalledConnection::prepare_at(&directory.0, origin()?)?;
    assert!(!directory.0.join(SETTINGS_FILE).exists());
    prepared.persist()?;
    let installed = InstalledConnection::open(&directory.0)?;
    assert_eq!(installed, prepared);
    assert_eq!(installed.binding_path(), directory.0.join(BINDING_FILE));
    let path = directory.0.join(SETTINGS_FILE);
    let bytes = fs::read(&path)?;
    let modified = fs::metadata(&path)?.modified()?;
    prepared.persist()?;
    assert_eq!(fs::read(&path)?, bytes);
    assert_eq!(fs::metadata(&path)?.modified()?, modified);
    assert_eq!(fs::read_dir(&directory.0)?.count(), 1);
    Ok(())
}

#[test]
fn existing_settings_and_device_binding_cannot_be_silently_rebound() -> TestResult {
    let directory = Directory::new()?;
    let first = InstalledConnection::prepare_at(&directory.0, origin()?)?;
    first.persist()?;
    fs::write(directory.0.join(BINDING_FILE), b"existing-device-binding")?;
    let path = directory.0.join(SETTINGS_FILE);
    let bytes = fs::read(&path)?;
    assert_eq!(
        InstalledConnection::prepare_at(
            &directory.0,
            ControlPlaneOrigin::parse("https://other.example.com")?
        ),
        Err(InstalledConnectionError::Conflict)
    );
    first.persist()?;
    assert_eq!(fs::read(&path)?, bytes);
    fs::remove_file(&path)?;
    assert_eq!(
        InstalledConnection::prepare_at(&directory.0, origin()?),
        Err(InstalledConnectionError::Conflict)
    );
    assert!(!path.exists());
    assert_eq!(
        fs::read(directory.0.join(BINDING_FILE))?,
        b"existing-device-binding"
    );
    Ok(())
}

#[test]
fn conflicting_record_created_after_preparation_is_never_replaced() -> TestResult {
    let directory = Directory::new()?;
    let first = InstalledConnection::prepare_at(&directory.0, origin()?)?;
    let competing = InstalledConnection::prepare_at(
        &directory.0,
        ControlPlaneOrigin::parse("https://other.example.com")?,
    )?;
    competing.persist()?;
    let bytes = fs::read(directory.0.join(SETTINGS_FILE))?;
    assert_eq!(first.persist(), Err(InstalledConnectionError::Conflict));
    assert_eq!(fs::read(directory.0.join(SETTINGS_FILE))?, bytes);
    Ok(())
}

#[test]
fn missing_corrupt_extended_or_oversized_records_never_become_defaults() -> TestResult {
    let directory = Directory::new()?;
    let path = directory.0.join(SETTINGS_FILE);
    assert!(InstalledConnection::open(&directory.0).is_err());
    for invalid in [
        "{}".to_owned(), "{".to_owned(),
        r#"{"schema_version":2,"control_plane_origin":"https://control.example.com"}"#.to_owned(),
        r#"{"schema_version":1,"control_plane_origin":"http://control.example.com"}"#.to_owned(),
        r#"{"schema_version":1,"control_plane_origin":"https://control.example.com","device_id":"device_spoof"}"#.to_owned(),
        " ".repeat(MAX_SETTINGS_BYTES as usize + 1),
    ] {
        fs::write(&path, &invalid)?;
        assert_eq!(InstalledConnection::open(&directory.0), Err(InstalledConnectionError::InvalidRecord));
        assert_eq!(InstalledConnection::prepare_at(&directory.0, origin()?), Err(InstalledConnectionError::InvalidRecord));
        assert_eq!(fs::read(&path)?, invalid.as_bytes());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_not_installed_connection_records() -> TestResult {
    use std::os::unix::fs::symlink;
    let directory = Directory::new()?;
    let external = Directory::new()?;
    let settings = InstalledConnection::prepare_at(&external.0, origin()?)?;
    settings.persist()?;
    symlink(
        external.0.join(SETTINGS_FILE),
        directory.0.join(SETTINGS_FILE),
    )?;
    assert_eq!(
        InstalledConnection::open(&directory.0),
        Err(InstalledConnectionError::InvalidRecord)
    );
    fs::remove_file(directory.0.join(SETTINGS_FILE))?;
    let linked = directory.0.join("linked-root");
    symlink(&external.0, &linked)?;
    assert_eq!(
        InstalledConnection::open(&linked),
        Err(InstalledConnectionError::InvalidRecord)
    );
    fs::remove_file(linked)?;
    Ok(())
}
