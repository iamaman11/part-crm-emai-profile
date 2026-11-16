//! Read-only, fail-closed Firefox final SessionStore witness.
//!
//! Playwright's context-close and release of Windows parent.lock also occur
//! after a crash. Neither proves graceful shutdown. Firefox owns all writes;
//! Rust verifies only completed bytes under its existing profile admission.
//! These are pinned Firefox-internal files: CI must retest on runtime upgrades.
#![forbid(unsafe_code)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const CHECKPOINT: &str = "sessionCheckpoints.json";
const FINAL_SESSION: &str = "sessionstore.jsonlz4";
const MOZ_MAGIC: &[u8; 8] = b"mozLz40\0";
const MAX_CHECKPOINT_BYTES: u64 = 16 * 1024;
const MAX_COMPRESSED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloseWitness {
    Clean,
    RecoveryRequired,
    Unconfirmed,
}

fn profile_file(folder: &Path, name: &str) -> PathBuf {
    folder.join("user_data").join(name)
}

fn read_bounded_regular(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > limit
    {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    (bytes.len() as u64 == metadata.len()).then_some(bytes)
}

fn checkpoints(folder: &Path) -> Option<Value> {
    let bytes = read_bounded_regular(&profile_file(folder, CHECKPOINT), MAX_CHECKPOINT_BYTES)?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value.is_object().then_some(value)
}

/// Only call while an observed Firefox writer is active *after SDK Ready*.
/// On each launch Firefox resets the previous run's successful checkpoint.
pub(crate) fn live_run_checkpoint_reset(folder: &Path) -> bool {
    checkpoints(folder).is_some_and(|value| {
        value.get("sessionstore-windows-restored") == Some(&Value::Bool(true))
            && value.get("sessionstore-final-state-write-complete") != Some(&Value::Bool(true))
            && value.get("quit-application") != Some(&Value::Bool(true))
    })
}

fn modified_during_run(path: &Path, started: SystemTime) -> bool {
    fs::symlink_metadata(path).ok().is_some_and(|metadata| {
        !metadata.file_type().is_symlink()
            && metadata.is_file()
            && metadata.modified().is_ok_and(|mtime| mtime >= started)
    })
}

fn valid_final_sessionstore(folder: &Path) -> bool {
    let Some(bytes) =
        read_bounded_regular(&profile_file(folder, FINAL_SESSION), MAX_COMPRESSED_BYTES)
    else {
        return false;
    };
    if bytes.len() < 13 || &bytes[..8] != MOZ_MAGIC {
        return false;
    }
    let mut size_bytes = [0_u8; 4];
    size_bytes.copy_from_slice(&bytes[8..12]);
    let size = u32::from_le_bytes(size_bytes) as usize;
    if !(2..=MAX_DECOMPRESSED_BYTES).contains(&size) {
        return false;
    }
    let Ok(decoded) = lz4_flex::block::decompress(&bytes[12..], size) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&decoded) else {
        return false;
    };
    value.get("version").is_some_and(Value::is_array)
        && value.get("windows").is_some_and(Value::is_array)
}

/// Caller must independently verify SDK termination and *released native
/// Windows writer*. An old valid checkpoint is never a new Clean receipt.
pub(crate) fn verify_final_close(
    folder: &Path,
    started: SystemTime,
    observed_live_reset: bool,
) -> CloseWitness {
    if !observed_live_reset {
        return CloseWitness::Unconfirmed;
    }
    if !modified_during_run(&profile_file(folder, CHECKPOINT), started)
        || !modified_during_run(&profile_file(folder, FINAL_SESSION), started)
    {
        return CloseWitness::RecoveryRequired;
    }
    let Some(value) = checkpoints(folder) else {
        return CloseWitness::RecoveryRequired;
    };
    if value.get("quit-application") != Some(&Value::Bool(true))
        || value.get("sessionstore-final-state-write-complete") != Some(&Value::Bool(true))
        || !valid_final_sessionstore(folder)
    {
        return CloseWitness::RecoveryRequired;
    }
    CloseWitness::Clean
}

#[cfg(test)]
mod tests {
    use super::{CloseWitness, live_run_checkpoint_reset, verify_final_close};
    use std::fs;
    use std::time::SystemTime;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("user_data")).unwrap();
        dir
    }

    fn write_final_file(folder: &std::path::Path) {
        let json = br#"{"version":["sessionrestore",1],"windows":[{"tabs":[]}]}"#;
        let mut raw = b"mozLz40\0".to_vec();
        raw.extend_from_slice(&(json.len() as u32).to_le_bytes());
        raw.extend_from_slice(&lz4_flex::block::compress(json));
        fs::write(folder.join("user_data/sessionstore.jsonlz4"), raw).unwrap();
    }

    #[test]
    fn normal_close_requires_live_reset_and_new_valid_final_file() {
        let dir = fixture();
        // Test-only: establish the invocation before the fixture file writes.
        // Windows metadata timestamp granularity can round an immediate write
        // below SystemTime::now(); production's strict timestamp gate is unchanged.
        let started = SystemTime::now() - std::time::Duration::from_secs(2);
        let checkpoint = dir.path().join("user_data/sessionCheckpoints.json");
        fs::write(&checkpoint, br#"{"sessionstore-windows-restored":true}"#).unwrap();
        assert!(live_run_checkpoint_reset(dir.path()));
        write_final_file(dir.path());
        fs::write(
            &checkpoint,
            br#"{"quit-application":true,"sessionstore-final-state-write-complete":true}"#,
        )
        .unwrap();
        assert_eq!(
            verify_final_close(dir.path(), started, true),
            CloseWitness::Clean
        );
        assert_eq!(
            verify_final_close(dir.path(), started, false),
            CloseWitness::Unconfirmed
        );
    }

    #[test]
    fn crash_without_final_state_requires_recovery() {
        let dir = fixture();
        let started = SystemTime::now();
        let checkpoint = dir.path().join("user_data/sessionCheckpoints.json");
        fs::write(&checkpoint, br#"{"sessionstore-windows-restored":true}"#).unwrap();
        assert!(live_run_checkpoint_reset(dir.path()));
        assert_eq!(
            verify_final_close(dir.path(), started, true),
            CloseWitness::RecoveryRequired
        );
    }

    #[test]
    fn previous_good_checkpoint_is_not_a_new_run_reset() {
        let dir = fixture();
        fs::write(
            dir.path().join("user_data/sessionCheckpoints.json"),
            br#"{"quit-application":true,"sessionstore-final-state-write-complete":true}"#,
        )
        .unwrap();
        assert!(!live_run_checkpoint_reset(dir.path()));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires two disposable Windows Firefox GUI test roots"]
    fn recorded_real_windows_gui_close_and_crash_differ() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::path::Path;
        use std::time::{Duration, UNIX_EPOCH};

        fn check(root: &Path) -> Result<CloseWitness, Box<dyn std::error::Error>> {
            let meta: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("process.json"))?)?;
            let start = meta
                .get("start_unix")
                .or_else(|| meta.get("start"))
                .and_then(serde_json::Value::as_f64)
                .ok_or("test process start missing")?;
            let started = UNIX_EPOCH + Duration::from_secs_f64(start);
            Ok(verify_final_close(&root.join("profile"), started, true))
        }

        let clean = std::env::var_os("PROFILE_BRIDGE_TEST_CLEAN_ROOT")
            .ok_or("PROFILE_BRIDGE_TEST_CLEAN_ROOT missing")?;
        let crash = std::env::var_os("PROFILE_BRIDGE_TEST_CRASH_ROOT")
            .ok_or("PROFILE_BRIDGE_TEST_CRASH_ROOT missing")?;
        assert_eq!(check(Path::new(&clean))?, CloseWitness::Clean);
        assert_eq!(check(Path::new(&crash))?, CloseWitness::RecoveryRequired);
        Ok(())
    }
    #[test]
    fn corrupt_and_absent_final_files_fail_closed() {
        let dir = fixture();
        let started = SystemTime::now();
        fs::write(
            dir.path().join("user_data/sessionCheckpoints.json"),
            br#"{"quit-application":true,"sessionstore-final-state-write-complete":true}"#,
        )
        .unwrap();
        assert_eq!(
            verify_final_close(dir.path(), started, true),
            CloseWitness::RecoveryRequired
        );
        write_final_file(dir.path());
        fs::write(
            dir.path().join("user_data/sessionstore.jsonlz4"),
            b"mozLz40\x00broken",
        )
        .unwrap();
        assert_eq!(
            verify_final_close(dir.path(), started, true),
            CloseWitness::RecoveryRequired
        );
    }
}
