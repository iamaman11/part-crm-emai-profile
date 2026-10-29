//! Bounded, non-authoritative, typed operator observations.
//! Not a lifecycle journal: this file never grants a lease, a save, a retry or a profile action.
//! Every stored value is a closed enum or a locally generated non-secret timestamp/identifier.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const DIRECTORY: &str = "diagnostics";
const MAX_RECORD_BYTES: u64 = 1_024;
const MAX_EVENTS: usize = 48;
const MAX_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAX_REPORT_BYTES: usize = 20_480;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static OPERATION_ID: OnceLock<String> = OnceLock::new();

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage {
    InstallPrepare,
    InstallVerified,
    InstallActivate,
    OpenRequested,
    GenerationSelection,
    LocalRecovery,
    CloudMaterialization,
    BrowserPreflight,
    CamouhostLaunch,
    BrowserReady,
    BrowserVisible,
    BrowserClosed,
    LocalUnsynced,
    CloudSaveAttempt,
    CloudSaveConfirmed,
    RecoveryRequired,
    Pairing,
    Delivery,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Started,
    Observed,
    Succeeded,
    Failed,
    Unconfirmed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Cloud {
    Unknown,
    NotConfirmed,
    CommittedExact,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Cause {
    None,
    InstallBlocked,
    DownloadFailed,
    IntegrityFailed,
    LocalStateUncertain,
    BrowserUnavailable,
    CoordinatorUncertain,
    CloudSaveFailed,
    OtherSafeFailure,
    RuntimeDependency,
    RuntimeContract,
    RuntimeTimeout,
    RuntimeIo,
    RuntimeClosed,
    RuntimeUnexpected,
    DownloadDns,
    DownloadConnect,
    DownloadTls,
    DownloadHttp,
    DownloadTimeout,
    DownloadStalled,
    DownloadSize,
    DownloadDisk,
    DownloadRedirect,
    DownloadProcess,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema_version: u8,
    time_unix_ms: u64,
    operation_id: String,
    stage: Stage,
    outcome: Outcome,
    cloud: Cloud,
    cause: Cause,
    duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    open_pages: Option<u16>,
}

fn unix_ms() -> Option<u64> {
    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

fn operation_id() -> &'static str {
    OPERATION_ID.get_or_init(|| {
        format!("op_{:08x}_{:016x}", std::process::id(), unix_ms().unwrap_or(0))
    })
}

/// A best-effort, local-only observation. Failure is never an application failure.
pub fn record_current(stage: Stage, outcome: Outcome, cloud: Cloud, cause: Cause, duration_ms: u64) {
    if let Some(root) = installed_root() {
        record_at(&root, stage, outcome, cloud, cause, duration_ms);
    }
}

/// The installer already owns its trusted installation root; no URI may select this path.
pub fn record_at(root: &Path, stage: Stage, outcome: Outcome, cloud: Cloud, cause: Cause, duration_ms: u64) {
    let _ = try_record(root, stage, outcome, cloud, cause, duration_ms, None);
}

fn installed_root() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let coordinator = crate::windows_delivery_handoff::DeliveryHandoffCoordinator::from_current_executable(executable).ok()?;
    coordinator.installed_root().ok().map(Path::to_path_buf)
}

fn valid_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
}

fn journal_dir(root: &Path, create: bool) -> Option<PathBuf> {
    if !root.is_absolute() || !valid_directory(root) || !valid_directory(&root.join("state")) {
        return None;
    }
    let path = root.join("state").join(DIRECTORY);
    if create {
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&path).ok()?;
            }
            Err(_) => return None,
        }
    }
    valid_directory(&path).then_some(path)
}

fn try_record(root: &Path, stage: Stage, outcome: Outcome, cloud: Cloud, cause: Cause, duration_ms: u64, open_pages: Option<u16>) -> Option<()> {
    let directory = journal_dir(root, true)?;
    let now = unix_ms()?;
    let record = Observation {
        schema_version: 1,
        time_unix_ms: now,
        operation_id: operation_id().to_owned(),
        stage,
        outcome,
        cloud,
        cause,
        duration_ms,
        open_pages,
    };
    let bytes = serde_json::to_vec(&record).ok()?;
    if bytes.len() > MAX_RECORD_BYTES as usize {
        return None;
    }
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_path = directory.join(format!("event-{now:016x}-{:08x}-{sequence:016x}.json", std::process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(file_path).ok()?;
    file.write_all(&bytes).and_then(|()| file.sync_all()).ok()?;
    prune(&directory, now);
    Some(())
}

fn is_event_name(value: &str) -> bool {
    value.starts_with("event-")
        && value.ends_with(".json")
        && value.len() <= 90
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.')
}

fn observations(root: &Path) -> Vec<Observation> {
    let Some(directory) = journal_dir(root, false) else { return Vec::new() };
    let Some(now) = unix_ms() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(directory) else { return Vec::new() };
    let mut records = Vec::new();
    // A corrupt, partial, foreign, oversized or link file is ignored, never displayed.
    for item in entries.flatten().take(2_048) {
        let name = item.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_event_name(name) { continue; }
        let Ok(meta) = fs::symlink_metadata(item.path()) else { continue };
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_RECORD_BYTES { continue; }
        let Ok(file) = File::open(item.path()) else { continue };
        let mut bytes = Vec::new();
        if file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes).is_err() || bytes.len() > MAX_RECORD_BYTES as usize { continue; }
        let Ok(record) = serde_json::from_slice::<Observation>(&bytes) else { continue };
        if record.schema_version != 1
            || record.time_unix_ms > now.saturating_add(60_000)
            || now.saturating_sub(record.time_unix_ms) > MAX_AGE_MS
            || record.operation_id.len() != 28
            || !record.operation_id.starts_with("op_")
            || !record.operation_id[3..].bytes().all(|b| b.is_ascii_hexdigit() || b == b'_')
            || record.duration_ms > 86_400_000
            || record.open_pages.is_some_and(|count| count > 512)
        { continue; }
        records.push(record);
    }
    records.sort_by_key(|record| record.time_unix_ms);
    if records.len() > MAX_EVENTS {
        records.drain(..records.len() - MAX_EVENTS);
    }
    records
}

fn prune(directory: &Path, now: u64) {
    let Ok(entries) = fs::read_dir(directory) else { return };
    let mut files = Vec::new();
    for item in entries.flatten().take(2_048) {
        let name = item.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_event_name(name) { continue; }
        let Ok(meta) = fs::symlink_metadata(item.path()) else { continue };
        if !meta.is_file() || meta.file_type().is_symlink() { continue; }
        files.push((name.to_owned(), item.path()));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let excess = files.len().saturating_sub(MAX_EVENTS);
    for (index, (name, path)) in files.into_iter().enumerate() {
        let timestamp = name.split('-').nth(1).and_then(|hex| u64::from_str_radix(hex, 16).ok());
        if index < excess || timestamp.is_some_and(|stamp| now.saturating_sub(stamp) > MAX_AGE_MS) {
            let _ = fs::remove_file(path);
        }
    }
}

/// Observed live Playwright pages (tabs), never inferred restored tab count or window count.
/// There are no titles, addresses or raw browser IPC bytes in this event.
/// Installer-only download evidence. Source and error classification are owned
/// by the verified Windows asset fetcher. No URL, destination or proxy details.
pub fn record_download_failure(root: &Path, cause: Cause) {
    record_at(root, Stage::InstallPrepare, Outcome::Failed, Cloud::Unknown, cause, 0);
}

pub fn record_open_pages(count: u16) {
    if count > 512 { return; }
    if let Some(root) = installed_root() {
        let _ = try_record(&root, Stage::BrowserVisible, Outcome::Observed, Cloud::Unknown, Cause::None, 0, Some(count));
    }
}

fn runtime_identity(root: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        let store = crate::windows_delivery_store::DeliveryStateStore::open(root.join("state")).ok()?;
        let identity = &store.state().active()?.runtime_bundle_release_id;
        let digest = identity.strip_prefix("runtime-bundle-v3-sha256-")?;
        if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')) {
            return Some(identity.clone());
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        None
    }
}

/// A diagnostic recommendation is text only, never an executable retry grant.
fn next_safe_step(record: &Observation) -> &'static str {
    match (record.stage, record.outcome, record.cloud) {
        (Stage::LocalUnsynced, _, _) => "PRESERVE_LOCAL_WAIT_FOR_OWNED_RECOVERY",
        (Stage::CloudSaveConfirmed, Outcome::Succeeded, Cloud::CommittedExact) => {
            "NORMAL_NEXT_OPEN"
        }
        (Stage::CloudSaveAttempt, _, Cloud::NotConfirmed) => {
            "CLOUD_NOT_CONFIRMED_PRESERVE_LOCAL"
        }
        (Stage::InstallPrepare | Stage::InstallVerified | Stage::InstallActivate, Outcome::Failed, _) => {
            "RETRY_INSTALLER_AFTER_CHECK"
        }
        (Stage::BrowserPreflight | Stage::CamouhostLaunch, Outcome::Failed, _) => {
            "RETRY_THROUGH_OWNED_OPEN"
        }
        (Stage::OpenRequested, Outcome::Failed, _) => "RECHECK_OWNED_RECOVERY",
        _ => "NO_RECOMMENDATION",
    }
}

fn local_confirmation(record: &Observation) -> &'static str {
    match (record.stage, record.outcome, record.cloud) {
        (Stage::LocalUnsynced, Outcome::Unconfirmed, Cloud::NotConfirmed) => {
            "LOCAL_UNSYNCED_RETAINED"
        }
        (Stage::CloudSaveConfirmed, Outcome::Succeeded, Cloud::CommittedExact) => {
            "EXACT_SAVED_BY_OWNER"
        }
        _ => "NOT_OBSERVED",
    }
}

fn installed_sequence(root: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        let store = crate::windows_delivery_store::DeliveryStateStore::open(root.join("state")).ok()?;
        store.state().active().map(|identity| identity.sequence)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        None
    }
}

/// Read-only, strictly reconstructed from admitted typed observations. No profile data is read.
#[must_use]
pub fn report_current() -> String {
    installed_root().map_or_else(
        || report_at(Path::new("")),
        |root| report_at(&root),
    )
}

#[must_use]
pub fn report_at(root: &Path) -> String {
    let runtime = runtime_identity(root).unwrap_or_else(|| "NOT_OBSERVED".to_owned());
    let release_sequence = installed_sequence(root).map_or_else(|| "NOT_OBSERVED".to_owned(), |n| n.to_string());
    let mut report = format!(
        "PROFILE_BRIDGE_DIAGNOSTICS_V1\nBRIDGE_VERSION={}\nINSTALLED_RELEASE_SEQUENCE={}\nRUNTIME_IDENTITY={}\nREPORT_AUTHORITY=LOCAL_OBSERVATIONS_ONLY\n",
        env!("CARGO_PKG_VERSION"), release_sequence, runtime,
    );
    for record in observations(root) {
        let line = format!(
            "TIME_MS={};OP={};STAGE={:?};OUTCOME={:?};CLOUD={:?};CAUSE={:?};DURATION_MS={};LOCAL={};NEXT_SAFE_STEP={}\n",
            record.time_unix_ms, record.operation_id, record.stage, record.outcome, record.cloud, record.cause, record.duration_ms,
            local_confirmation(&record), next_safe_step(&record)
        );
        if report.len() + line.len() > MAX_REPORT_BYTES { break; }
        report.push_str(&line);
        if let Some(pages) = record.open_pages {
            let count_line = format!("OBSERVED_OPEN_PAGES={pages};RESTORED_COUNT=UNPROVEN;WINDOW_COUNT=UNKNOWN\n");
            if report.len() + count_line.len() <= MAX_REPORT_BYTES { report.push_str(&count_line); }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn test_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("bridge-report-{}-{}", std::process::id(), TEST_ID.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(root.join("state")).expect("test root");
        root
    }

    #[test]
    fn typed_report_survives_restart_and_does_not_replay_owner_state() {
        let root = test_root();
        record_at(&root, Stage::LocalUnsynced, Outcome::Unconfirmed, Cloud::NotConfirmed, Cause::None, 7);
        record_at(&root, Stage::CloudSaveConfirmed, Outcome::Succeeded, Cloud::CommittedExact, Cause::None, 9);
        let text = report_at(&root);
        assert!(text.contains("LocalUnsynced"));
        assert!(text.contains("CloudSaveConfirmed"));
        assert!(text.contains("LOCAL_OBSERVATIONS_ONLY"));
        assert!(!text.contains(&root.to_string_lossy().to_string()));
        assert_eq!(text, report_at(&root));
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn corrupted_and_untrusted_files_are_never_echoed() {
        let root = test_root();
        record_at(&root, Stage::OpenRequested, Outcome::Started, Cloud::Unknown, Cause::None, 0);
        let directory = root.join("state").join(DIRECTORY);
        fs::write(directory.join("event-0000000000000001-fake.json"), b"https://secret.invalid/?token=top_secret profile_bytes cookie").expect("fixture");
        let output = report_at(&root);
        for leak in ["secret.invalid", "top_secret", "profile_bytes", "cookie", root.to_str().expect("path")] {
            assert!(!output.contains(leak));
        }
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn untrusted_records_never_admit_unknown_fields_or_secret_urls() {
        let root = test_root();
        let directory = root.join("state").join(DIRECTORY);
        // Even a plausible event file cannot smuggle arbitrary fields into a
        // typed diagnostic. Production never deserializes it into owner state.
        let malicious = serde_json::json!({
            "schema_version": 1,
            "time_unix_ms": unix_ms().expect("clock"),
            "operation_id": operation_id(),
            "stage": "CLOUD_SAVE_CONFIRMED",
            "outcome": "SUCCEEDED",
            "cloud": "COMMITTED_EXACT",
            "cause": "NONE",
            "duration_ms": 100,
            "private_url": "https://private.invalid/?token=secret",
        });
        fs::write(directory.join("event-0000000000000001-forged.json"), serde_json::to_vec(&malicious).expect("fixture")).expect("fixture");
        let report = report_at(&root);
        assert!(!report.contains("private.invalid"));
        assert!(!report.contains("CLOUD_SAVE_CONFIRMED"));
        assert!(report.contains("REPORT_AUTHORITY=LOCAL_OBSERVATIONS_ONLY"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn read_only_report_does_not_create_or_repair_any_state() {
        let root = std::env::temp_dir().join(format!(
            "bridge-report-no-state-{}-{}",
            std::process::id(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("fixture");
        let report = report_at(&root);
        assert!(report.contains("RUNTIME_IDENTITY=NOT_OBSERVED"));
        assert!(!root.join("state").exists());
        assert!(!root.join(DIRECTORY).exists());
        fs::remove_dir(root).expect("cleanup");
    }

    #[test]
    fn reporting_pending_local_never_claims_cloud_commit_or_authorized_retry() {
        let root = test_root();
        record_at(&root, Stage::LocalUnsynced, Outcome::Unconfirmed, Cloud::NotConfirmed, Cause::None, 42);
        let report = report_at(&root);
        assert!(report.contains("LOCAL=LOCAL_UNSYNCED_RETAINED"));
        assert!(report.contains("PRESERVE_LOCAL_WAIT_FOR_OWNED_RECOVERY"));
        assert!(!report.contains("CLOUD=CommittedExact"));
        assert!(!report.contains("AUTHORIZATION_GRANTED"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_is_bounded_and_ignores_symlink_or_missing_state() {
        let root = test_root();
        for _ in 0..100 { record_at(&root, Stage::BrowserPreflight, Outcome::Observed, Cloud::Unknown, Cause::None, 1); }
        let text = report_at(&root);
        assert!(text.len() <= MAX_REPORT_BYTES);
        assert!(text.lines().count() <= MAX_EVENTS + 5);
        fs::remove_dir_all(root).expect("cleanup fixture");
    }
}
