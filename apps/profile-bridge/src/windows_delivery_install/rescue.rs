//! Exact, independent, immutable local rescue of browser bytes before replacing Bridge.
//! This is an installer-owned safety copy, NOT a cloud CURRENT/WORKING assertion.
//! No new daemon, profile owner or permissions. The caller holds DeliveryActivationGuard.
use super::s1::inventory;
use crate::local_profile::MaterializationRoot;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ROOT_MARKER: &str = ".profile-platform-root";
const RECEIPT: &str = "profile-bytes.sha256";
const SNAPSHOT: &str = "snapshot-v1";
const COPY_BUFFER: usize = 64 * 1024;
const MAX_ENTRIES: u64 = 500_000;

#[derive(Debug)]
pub(super) struct RescueError {
    reason: &'static str,
}
impl RescueError {
    fn new(reason: &'static str) -> Self { Self { reason } }
}
impl std::fmt::Display for RescueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Profile Bridge rescue: {}", self.reason)
    }
}
impl std::error::Error for RescueError {}

fn io<T>(x: std::io::Result<T>, reason: &'static str) -> Result<T, RescueError> {
    x.map_err(|_| RescueError::new(reason))
}

fn regular_dir(path: &Path) -> Result<(), RescueError> {
    super::bundle::validate_directory_tree(path, false)
        .map_err(|_| RescueError::new("unsafe destination or reparse path"))
}
fn regular_file(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.is_file() && meta.file_attributes() & 0x0400 == 0
    }
    #[cfg(not(windows))]
    { meta.is_file() && !meta.file_type().is_symlink() }
}
fn regular_directory(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.is_dir() && meta.file_attributes() & 0x0400 == 0
    }
    #[cfg(not(windows))]
    { meta.is_dir() && !meta.file_type().is_symlink() }
}

/// Only this function is permitted to create the independent copy. It is
/// invoked under the installer owner's exclusive OS-backed materialization
/// lock, after normal signed release preparation, but before activation.
///
/// An existing verified rescue is immutable, including across later updates.
/// An incomplete stage remains for forensic inspection and is NEVER adopted
/// or silently deleted. A subsequent attempt uses a fresh unique stage.
pub(super) fn ensure_rescue(
    installation_root: &Path,
    original: &MaterializationRoot,
) -> Result<&'static str, RescueError> {
    let before = inventory(original).map_err(|_| RescueError::new("original profile inventory unreadable"))?;
    // A pristine first installation has only its durable admission marker.
    if before.files == 1 && before.directories == 0 {
        return Ok("EMPTY_NEW_PROFILE_ROOT");
    }

    let parent = installation_root.parent().ok_or(RescueError::new("missing installation parent"))?;
    regular_dir(parent)?;
    let name = installation_root.file_name().and_then(|s| s.to_str())
        .ok_or(RescueError::new("invalid installation root name"))?;
    let rescue_parent = parent.join(format!("{name}-recovery"));
    match fs::symlink_metadata(&rescue_parent) {
        Ok(meta) if regular_directory(&meta) => regular_dir(&rescue_parent)?,
        Ok(_) => return Err(RescueError::new("existing rescue root not an ordinary directory")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            io(fs::create_dir(&rescue_parent), "could not create independent rescue root")?;
            regular_dir(&rescue_parent)?;
        }
        Err(_) => return Err(RescueError::new("rescue root unreadable")),
    }
    let published = rescue_parent.join(SNAPSHOT);
    match fs::symlink_metadata(&published) {
        Ok(meta) if regular_directory(&meta) => {
            verify_snapshot(&published)?;
            return Ok("EXISTING_RESCUE_BYTES_VERIFIED");
        }
        Ok(_) => return Err(RescueError::new("published rescue is not an ordinary directory")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(RescueError::new("cannot inspect published rescue")),
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|_| RescueError::new("clock unavailable"))?.as_nanos();
    let stage = rescue_parent.join(format!("stage-{}-{nonce}", std::process::id()));
    io(fs::create_dir(&stage), "rescue stage creation failed")?;
    // Unlike the source, the new marker can be written before we acquire a
    // lock on this *new* private directory. The source's locked marker is
    // already verified by MaterializationRoot::open_existing.
    let target = MaterializationRoot::open_or_create(stage.join("profiles"))
        .map_err(|_| RescueError::new("rescue materialization root initialization failed"))?;
    copy_contents(original.path(), target.path())?;
    let after = inventory(original).map_err(|_| RescueError::new("original changed during rescue"))?;
    let copied = inventory(&target).map_err(|_| RescueError::new("copied rescue inventory invalid"))?;
    if before != after || copied != before {
        return Err(RescueError::new("rescue copy differs from original, nothing published"));
    }
    let receipt = stage.join(RECEIPT);
    let mut output = io(
        OpenOptions::new().write(true).create_new(true).open(&receipt),
        "rescue receipt create failed",
    )?;
    io(output.write_all(before.content_manifest_sha256.as_bytes()), "rescue receipt write failed")?;
    io(output.write_all(b"\n"), "rescue receipt terminator failed")?;
    io(output.sync_all(), "rescue receipt sync failed")?;
    verify_snapshot(&stage)?;
    // Rename never overwrites a published directory on Windows. An updater
    // winning this race means the other published copy must be verified.
    if fs::rename(&stage, &published).is_err() {
        verify_snapshot(&published)?;
        return Ok("EXISTING_RESCUE_BYTES_VERIFIED");
    }
    verify_snapshot(&published)?;
    Ok("NEW_RESCUE_BYTES_VERIFIED")
}

fn verify_snapshot(published: &Path) -> Result<(), RescueError> {
    regular_dir(published)?;
    let receipt = published.join(RECEIPT);
    let meta = io(fs::symlink_metadata(&receipt), "rescue receipt missing")?;
    if !regular_file(&meta) || meta.len() != 65 {
        return Err(RescueError::new("invalid rescue receipt"));
    }
    let expected = io(fs::read_to_string(receipt), "rescue receipt unreadable")?;
    let expected = expected.strip_suffix('\n').ok_or(RescueError::new("rescue receipt format"))?;
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(RescueError::new("rescue receipt digest malformed"));
    }
    let root = MaterializationRoot::open_existing(published.join("profiles"))
        .map_err(|_| RescueError::new("rescue root marker invalid"))?;
    let current = inventory(&root).map_err(|_| RescueError::new("rescue files corrupt or unreadable"))?;
    if current.content_manifest_sha256 != expected {
        return Err(RescueError::new("rescue bytes do not match verified receipt"));
    }
    Ok(())
}

fn copy_contents(source: &Path, target: &Path) -> Result<(), RescueError> {
    let mut work: Vec<(PathBuf, PathBuf)> = vec![(source.to_owned(), target.to_owned())];
    let mut count = 0u64;
    while let Some((from, to)) = work.pop() {
        regular_dir(&from)?;
        regular_dir(&to)?;
        for entry in io(fs::read_dir(&from), "source directory inaccessible")? {
            let entry = io(entry, "source directory changed")?;
            let filename = entry.file_name();
            if from == source && filename == ROOT_MARKER {
                continue; // separately validated source and generated target markers
            }
            count += 1;
            if count > MAX_ENTRIES {
                return Err(RescueError::new("profile copy count limit exceeded"));
            }
            let original = entry.path();
            let metadata = io(fs::symlink_metadata(&original), "source metadata missing")?;
            let destination = to.join(&filename);
            if regular_directory(&metadata) {
                io(fs::create_dir(&destination), "rescue directory creation failed")?;
                work.push((original, destination));
            } else if regular_file(&metadata) {
                let mut input = io(File::open(&original), "source file cannot be opened")?;
                let mut output = io(
                    OpenOptions::new().create_new(true).write(true).open(&destination),
                    "destination file cannot be created",
                )?;
                let mut buffer = [0u8; COPY_BUFFER];
                let mut copied = 0u64;
                loop {
                    let n = io(input.read(&mut buffer), "source file read failed")?;
                    if n == 0 { break }
                    io(output.write_all(&buffer[..n]), "rescue file write failed")?;
                    copied = copied.checked_add(n as u64)
                        .ok_or(RescueError::new("rescue file size overflow"))?;
                }
                io(output.sync_all(), "rescue file flush failed")?;
                let final_meta = io(input.metadata(), "source metadata changed")?;
                if copied != metadata.len() || final_meta.len() != metadata.len()
                    || final_meta.modified().ok() != metadata.modified().ok()
                {
                    return Err(RescueError::new("source modified during rescue"));
                }
            } else {
                return Err(RescueError::new("symlink/reparse or special source entry forbidden"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_profile::DeliveryActivationGuard;

    fn temp(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rescue-{label}-{}-{}",
            std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
    }

    #[test]
    fn rescue_copies_all_wal_and_immutable_backup_survives_next_update()
    -> Result<(), Box<dyn std::error::Error>> {
        let parent = temp("pass");
        fs::create_dir_all(&parent)?;
        let installed = parent.join("ProfileBridge");
        let original = MaterializationRoot::open_or_create(installed.join("profiles"))?;
        let data = original.path().join("user_data");
        fs::create_dir(&data)?;
        fs::write(data.join("cookies.sqlite-wal"), b"old dirty bytes")?;
        fs::write(original.path().join(".pending-local"), b"unsynced evidence")?;
        let before = inventory(&original)?;
        let guard = DeliveryActivationGuard::acquire(&original)?;
        assert_eq!(ensure_rescue(&installed, &original)?, "NEW_RESCUE_BYTES_VERIFIED");
        let backup = parent.join("ProfileBridge-recovery").join(SNAPSHOT);
        assert_eq!(inventory(&MaterializationRoot::open_existing(backup.join("profiles"))?)?, before);
        let original_bytes = fs::read(data.join("cookies.sqlite-wal"))?;
        assert_eq!(original_bytes, b"old dirty bytes");
        fs::write(data.join("cookies.sqlite-wal"), b"new working data")?;
        assert_eq!(ensure_rescue(&installed, &original)?, "EXISTING_RESCUE_BYTES_VERIFIED");
        assert_eq!(fs::read(backup.join("profiles/user_data/cookies.sqlite-wal"))?, original_bytes);
        drop(guard);
        fs::remove_dir_all(parent)?;
        Ok(())
    }

    #[test]
    fn corrupt_rescue_never_gets_adopted_or_overwritten()
    -> Result<(), Box<dyn std::error::Error>> {
        let parent = temp("corrupt");
        fs::create_dir_all(&parent)?;
        let installed = parent.join("ProfileBridge");
        let root = MaterializationRoot::open_or_create(installed.join("profiles"))?;
        fs::write(root.path().join("dirty"), b"original")?;
        let guard = DeliveryActivationGuard::acquire(&root)?;
        ensure_rescue(&installed, &root)?;
        let backup = parent.join("ProfileBridge-recovery").join(SNAPSHOT);
        fs::write(backup.join("profiles/dirty"), b"tampered")?;
        assert!(ensure_rescue(&installed, &root).is_err());
        assert_eq!(fs::read(root.path().join("dirty"))?, b"original");
        assert_eq!(fs::read(backup.join("profiles/dirty"))?, b"tampered");
        drop(guard);
        fs::remove_dir_all(parent)?;
        Ok(())
    }

    #[test]
    fn symlink_source_never_becomes_published_backup()
    -> Result<(), Box<dyn std::error::Error>> {
        let parent = temp("symlink");
        fs::create_dir_all(&parent)?;
        let installed = parent.join("ProfileBridge");
        let root = MaterializationRoot::open_or_create(installed.join("profiles"))?;
        let guard = DeliveryActivationGuard::acquire(&root)?;
        // Validation of invalid source paths precedes any publication.
        let external = parent.join("external");
        fs::write(&external, b"external")?;
        // Creation of reparse points is privilege-restricted on some Windows
        // CI runners; test the negative path whenever the OS permits it.
        if std::os::windows::fs::symlink_file(&external, root.path().join("link")).is_ok() {
            assert!(ensure_rescue(&installed, &root).is_err());
        }
        assert!(!parent.join("ProfileBridge-recovery").join(SNAPSHOT).exists());
        assert_eq!(fs::read(&external)?, b"external");
        drop(guard);
        fs::remove_dir_all(parent)?;
        Ok(())
    }

    #[test]
    fn empty_new_install_has_no_external_copy()
    -> Result<(), Box<dyn std::error::Error>> {
        let parent = temp("new");
        fs::create_dir_all(&parent)?;
        let installed = parent.join("ProfileBridge");
        let root = MaterializationRoot::open_or_create(installed.join("profiles"))?;
        let guard = DeliveryActivationGuard::acquire(&root)?;
        assert_eq!(ensure_rescue(&installed, &root)?, "EMPTY_NEW_PROFILE_ROOT");
        assert!(!parent.join("ProfileBridge-recovery").exists());
        drop(guard);
        fs::remove_dir_all(parent)?;
        Ok(())
    }
}
