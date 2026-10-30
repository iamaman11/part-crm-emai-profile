//! Operator-only S1 observation. No installation, recovery, snapshot, key creation,
//! network operation or browser launch is permitted in this module.
use super::bundle::{InstallBundle, validate_directory_tree};
use crate::browser_execution::load_materialization_binding;
use crate::installed_connection::InstalledConnection;
use crate::local_profile::{DeliveryActivationGuard, LocalProfileError, MaterializationRoot};
use crate::shipping_save_attempt::{S1PendingInventory, inspect_all_pending_for_s1};
use crate::windows_delivery::verify_delivery_candidate;
use crate::windows_delivery_coordinator::accepted_floor;
use crate::windows_delivery_signature::WindowsCmsSignatureVerifier;
use crate::windows_delivery_staging::{DeliveryStagingRoot, reopen_staged_delivery};
use crate::windows_delivery_store::DeliveryStateStore;
use crate::windows_device_application::WindowsDeviceApplicationBinding;
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_device_key::PersistedP256Key;

const MAX_FILES: u64 = 500_000;

/// One actionable, original-device read-only failure. Keep exact OS errors and
/// original paths for local diagnostics: never turn an unknown failure into READY.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct InventoryFailure {
    code: &'static str,
    path: String,
    detail: Option<String>,
    os_error: Option<i32>,
}

impl InventoryFailure {
    fn at(code: &'static str, path: &Path) -> Self {
        Self {
            code,
            path: path.to_string_lossy().into_owned(),
            detail: None,
            os_error: None,
        }
    }

    fn io(code: &'static str, path: &Path, error: std::io::Error) -> Self {
        Self {
            code,
            path: path.to_string_lossy().into_owned(),
            os_error: error.raw_os_error(),
            detail: Some(error.to_string()),
        }
    }
}

impl std::fmt::Display for InventoryFailure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{} at {}: {:?}", self.code, self.path, self.detail)
    }
}

impl std::error::Error for InventoryFailure {}

fn read_at<T>(
    result: std::io::Result<T>,
    code: &'static str,
    path: &Path,
) -> Result<T, InventoryFailure> {
    result.map_err(|error| InventoryFailure::io(code, path, error))
}

/// File-system identity on Windows is UTF-16, not necessarily valid UTF-8.
/// Hash the original relative Windows name without lossy Unicode conversion.
fn native_path_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[derive(Debug, Default, Eq, PartialEq, Serialize)]
struct ProfileObservation {
    content_manifest_sha256: String,
    files: u64,
    directories: u64,
    bytes: u64,
    firefox_user_data_directories: u64,
    sqlite_wal_files: u64,
    materialization_sidecars: u64,
    materializations_verified: u64,
    pending_named_files: u64,
    sidecar_failures: Vec<InventoryFailure>,
}

#[derive(Debug, Serialize)]
struct S1Observation {
    schema_version: u32,
    kind: &'static str,
    verdict: &'static str,
    native_writer: &'static str,
    existing_key: &'static str,
    existing_key_public_spki_sha256: Option<String>,
    signed_delivery: &'static str,
    installed_delivery: &'static str,
    profile_inventory: &'static str,
    isolated_copy: &'static str,
    pending_evidence: &'static str,
    pending_counts: Option<S1PendingInventory>,
    generation_authority: &'static str,
    previous_delivery_rollback: &'static str,
    independent_recovery: &'static str,
    inventory: Option<ProfileObservation>,
    inventory_failure: Option<InventoryFailure>,
    isolated_copy_failure: Option<InventoryFailure>,
    reason_codes: Vec<&'static str>,
}

/// Run from the *new* unpacked Bridge executable, with bundled signed release inputs
/// in a scratch directory. Returns a structured observation, never a clearance to install.
/// The installer must independently reacquire its exclusive admission immediately at activation.
pub(super) fn observe(
    bundle_directory: &Path,
    installation_root: &Path,
    copy_root: Option<&Path>,
) -> String {
    let mut result = S1Observation {
        schema_version: 1,
        kind: "PROFILE_BRIDGE_S1_READ_ONLY_OBSERVATION",
        verdict: "UNKNOWN",
        native_writer: "UNKNOWN",
        existing_key: "UNKNOWN",
        existing_key_public_spki_sha256: None,
        signed_delivery: "UNKNOWN",
        installed_delivery: "UNKNOWN",
        profile_inventory: "UNKNOWN",
        isolated_copy: "NOT_PROVIDED",
        pending_evidence: "UNKNOWN",
        pending_counts: None,
        generation_authority: "UNKNOWN",
        previous_delivery_rollback: "UNKNOWN",
        independent_recovery: "UNKNOWN",
        inventory: None,
        inventory_failure: None,
        isolated_copy_failure: None,
        reason_codes: Vec::new(),
    };
    inspect(bundle_directory, installation_root, copy_root, &mut result);
    serde_json::to_string(&result).unwrap_or_else(|_| {
        String::from("{\"schema_version\":1,\"kind\":\"PROFILE_BRIDGE_S1_READ_ONLY_OBSERVATION\",\"verdict\":\"UNKNOWN\",\"reason_codes\":[\"SERIALIZATION_FAILED\"]}")
    })
}

fn inspect(
    bundle_directory: &Path,
    root: &Path,
    copy_root: Option<&Path>,
    result: &mut S1Observation,
) {
    if validate_directory_tree(root, false).is_err() {
        result.reason_codes.push("INSTALLATION_ROOT_INVALID");
        return;
    }
    let workspace = match MaterializationRoot::open_existing(root.join("profiles")) {
        Ok(workspace) => workspace,
        Err(_) => {
            result.reason_codes.push("PROFILE_ROOT_UNVERIFIED");
            return;
        }
    };
    // Keep the OS-backed exclusive admission while inspecting: an idle snapshot
    // must never be synthesized from a process-name list or a stale marker file.
    let _guard = match DeliveryActivationGuard::acquire(&workspace) {
        Ok(guard) => {
            result.native_writer = "NO_ACTIVE_WRITER_DURING_OBSERVATION";
            guard
        }
        Err(LocalProfileError::LockBusy) => {
            result.native_writer = "ACTIVE_OR_UNRESOLVED";
            result.verdict = "BLOCKED";
            result.reason_codes.push("NATIVE_WRITER_LOCK_BUSY");
            return;
        }
        Err(_) => {
            result.reason_codes.push("NATIVE_WRITER_LOCK_UNKNOWN");
            return;
        }
    };
    match inventory(&workspace) {
        Ok(inventory) => {
            result.profile_inventory =
                if inventory.materialization_sidecars == inventory.materializations_verified {
                    "STRUCTURE_AND_EXISTING_BINDINGS_VERIFIED"
                } else {
                    "STRUCTURE_ONLY"
                };
            if !inventory.sidecar_failures.is_empty() {
                result
                    .reason_codes
                    .push("MATERIALIZATION_BINDINGS_UNVERIFIED");
            }
            result.inventory = Some(inventory);
        }
        Err(failure) => {
            result.reason_codes.push("PROFILE_INVENTORY_UNVERIFIED");
            result.inventory_failure = Some(failure);
        }
    }
    if let Some(copy_root) = copy_root {
        match result
            .inventory
            .as_ref()
            .map(|original| compare_isolated_copy(root, &workspace, copy_root, original))
        {
            Some(Ok(true)) => {
                result.isolated_copy = "BYTE_EXACT_ON_THIS_HOST_REOPEN_NOT_PROVEN";
                // A byte-identical second directory is a useful independent
                // copy but not a proven browser reopen or CURRENT/LKG authority.
                result.independent_recovery = "BYTE_IDENTICAL_COPY_REOPEN_UNVERIFIED";
            }
            Some(Ok(false)) => {
                result.isolated_copy = "CONTENT_MISMATCH";
                result.reason_codes.push("ISOLATED_COPY_MISMATCH");
            }
            Some(Err(failure)) => {
                result.isolated_copy = "UNVERIFIED";
                result.isolated_copy_failure = Some(failure);
                result.reason_codes.push("ISOLATED_COPY_UNVERIFIED");
            }
            None => {
                result.isolated_copy = "UNVERIFIED";
                result.reason_codes.push("ISOLATED_COPY_UNVERIFIED");
            }
        }
    }
    match inspect_all_pending_for_s1(&workspace) {
        Ok(pending) => {
            result.pending_evidence = "SCOPED_CHECKPOINTS_AND_UNSYNCED_VERIFIED";
            result.pending_counts = Some(pending);
        }
        Err(_) => {
            result.reason_codes.push("PENDING_SAVE_EVIDENCE_UNVERIFIED");
        }
    }

    let state = match DeliveryStateStore::open_read_only(root.join("state")) {
        Ok(state) => {
            if state.state().active().is_some() && state.state().active_health_confirmed() {
                result.installed_delivery = "ACTIVE_HEALTHY_STATE";
            } else {
                result.reason_codes.push("ACTIVE_DELIVERY_NOT_HEALTHY");
                return;
            }
            state
        }
        Err(_) => {
            result
                .reason_codes
                .push("INSTALLED_DELIVERY_STATE_UNVERIFIED");
            return;
        }
    };
    let staging = match DeliveryStagingRoot::open_existing(root.join("releases")) {
        Ok(staging) => staging,
        Err(_) => {
            result.installed_delivery = "STAGED_DELIVERY_UNVERIFIED";
            result.reason_codes.push("RELEASES_DIRECTORY_UNVERIFIED");
            return;
        }
    };
    if state
        .state()
        .active()
        .is_none_or(|active| reopen_staged_delivery(&staging, active).is_err())
    {
        result.installed_delivery = "ACTIVE_COMPONENT_HASHES_UNVERIFIED";
        result.reason_codes.push("ACTIVE_RELEASE_BYTES_UNVERIFIED");
        return;
    }
    result.installed_delivery = "ACTIVE_HEALTHY_COMPONENTS_VERIFIED";
    if let Some(lkg) = state.state().last_known_good() {
        if reopen_staged_delivery(&staging, lkg).is_ok() {
            result.previous_delivery_rollback = "PREVIOUS_DELIVERY_COMPONENTS_VERIFIED";
        } else {
            result
                .reason_codes
                .push("PREVIOUS_DELIVERY_BYTES_UNVERIFIED");
        }
    } else {
        result.previous_delivery_rollback = "ACTIVE_DELIVERY_ONLY";
    }

    // Device binding is the only local source for the CNG key name. Opening a
    // missing key must never trigger NCryptCreatePersistedKey.
    match WindowsDeviceApplicationBinding::open(&root.join("device-application-binding.json")) {
        Ok(binding) => {
            let name = format!("part-crm.device.{}", binding.device_id().as_str());
            match PersistedP256Key::open_existing(&name) {
                Ok(key) => {
                    // A non-exportable key handle alone does not prove that its
                    // private half is still usable. Exercise local P-256 PoP,
                    // but never send a request, mint a key or claim that the
                    // remote registered public key has been independently matched.
                    let challenge = format!(
                        "profile-bridge-s1-existing-device-key-v1:{}:{}",
                        binding.tenant_id().as_str(),
                        binding.device_id().as_str()
                    );
                    let proof = key.sign_sha256_message(challenge.as_bytes());
                    let usable = proof.as_ref().ok().and_then(|signature| {
                        key.verify_sha256_message(challenge.as_bytes(), signature)
                            .ok()
                    }) == Some(true);
                    match (usable, key.public_key_spki_der()) {
                        (true, Ok(spki)) => {
                            result.existing_key = "EXISTING_NONEXPORTABLE_LOCAL_SIGN_VERIFY";
                            result.existing_key_public_spki_sha256 = Some(sha256_lower_hex(&spki));
                        }
                        _ => result.reason_codes.push("EXISTING_CNG_KEY_UNUSABLE"),
                    }
                }
                Err(_) => result.reason_codes.push("EXISTING_CNG_KEY_UNVERIFIED"),
            }
        }
        Err(_) => result.reason_codes.push("DEVICE_BINDING_UNVERIFIED"),
    }

    let bundle = match InstallBundle::open(bundle_directory) {
        Ok(bundle) => bundle,
        Err(_) => {
            result.reason_codes.push("SIGNED_BUNDLE_INPUT_UNVERIFIED");
            return;
        }
    };
    let connection = match InstalledConnection::open(root) {
        Ok(connection) => connection,
        Err(_) => {
            result.reason_codes.push("EXISTING_ORIGIN_UNVERIFIED");
            return;
        }
    };
    if connection.origin() != &bundle.origin {
        result.verdict = "BLOCKED";
        result.reason_codes.push("DELIVERY_ORIGIN_MISMATCH");
        return;
    }

    let mut verifier = WindowsCmsSignatureVerifier;
    match verify_delivery_candidate(
        &bundle.manifest,
        &bundle.signature,
        &bundle.trust,
        accepted_floor(state.state()).as_ref(),
        &mut verifier,
    ) {
        Ok(_) => result.signed_delivery = "CMS_AND_COMPATIBILITY_VERIFIED",
        Err(_) => {
            result.verdict = "BLOCKED";
            result.reason_codes.push("SIGNED_DELIVERY_REJECTED");
            return;
        }
    }

    // File enumeration is not authoritative proof of the CURRENT / WORKING / LKG
    // generation mapping or an independent restorable backup. Never silently
    // promote a structurally readable directory into a verified Save.
    result
        .reason_codes
        .push("GENERATION_AUTHORITY_NOT_YET_PROVEN");
    if result.independent_recovery == "UNKNOWN" {
        result
            .reason_codes
            .push("INDEPENDENT_PROFILE_BACKUP_NOT_YET_PROVEN");
    } else {
        result
            .reason_codes
            .push("ISOLATED_COPY_REOPEN_NOT_YET_PROVEN");
    }
}

/// Compare the source against an already-existing isolated copy. This routine
/// only reads both trees; copying/launching the backup is a separate operator
/// action outside this command and never touches the original installation.
fn compare_isolated_copy(
    installed_root: &Path,
    original: &MaterializationRoot,
    copy_root: &Path,
    expected: &ProfileObservation,
) -> Result<bool, InventoryFailure> {
    let installed = read_at(
        fs::canonicalize(installed_root),
        "ORIGINAL_ROOT",
        installed_root,
    )?;
    // Check every path component before canonicalization; a reparse or
    // symlink alias must never be treated as an independent recovery copy.
    let copy = MaterializationRoot::open_existing(copy_root.to_path_buf()).map_err(|error| {
        InventoryFailure {
            code: "BACKUP_INVALID",
            path: copy_root.to_string_lossy().into_owned(),
            detail: Some(error.to_string()),
            os_error: None,
        }
    })?;
    let backup = copy.path();
    if backup.starts_with(&installed)
        || installed.starts_with(&backup)
        || original.path().starts_with(&backup)
        || backup.starts_with(original.path())
    {
        return Err(InventoryFailure::at("BACKUP_MUST_BE_ISOLATED", copy_root));
    }
    let observed = inventory(&copy)?;
    Ok(&observed == expected)
}

fn sha256_lower_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn inventory(root: &MaterializationRoot) -> Result<ProfileObservation, InventoryFailure> {
    let mut answer = ProfileObservation::default();
    let mut sidecars = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut directories: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.path().to_path_buf()];
    while let Some(directory) = stack.pop() {
        let children = read_at(fs::read_dir(&directory), "READ_DIRECTORY", &directory)?;
        for child in children {
            let child = read_at(child, "READ_ENTRY", &directory)?;
            let metadata = read_at(
                fs::symlink_metadata(child.path()),
                "READ_METADATA",
                &child.path(),
            )?;
            if metadata.file_type().is_symlink() || is_reparse(&metadata) {
                return Err(InventoryFailure::at("LINK_REJECTED", &child.path()));
            }
            let name = child.file_name();
            let name = name.to_string_lossy();
            if metadata.is_dir() {
                answer.directories = answer
                    .directories
                    .checked_add(1)
                    .ok_or_else(|| InventoryFailure::at("COUNT_LIMIT", &child.path()))?;
                if name == "user_data" {
                    answer.firefox_user_data_directories += 1;
                }
                directories.push(child.path());
                stack.push(child.path());
            } else if metadata.is_file() {
                answer.files = answer
                    .files
                    .checked_add(1)
                    .ok_or_else(|| InventoryFailure::at("COUNT_LIMIT", &child.path()))?;
                answer.bytes = answer
                    .bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| InventoryFailure::at("SIZE_LIMIT", &child.path()))?;
                files.push(child.path());
                if name.ends_with("-wal") {
                    answer.sqlite_wal_files += 1;
                }
                if name.ends_with(".materialization") {
                    answer.materialization_sidecars += 1;
                    sidecars.push(child.path());
                }
                if name.contains("unsynced") || name.starts_with(".pending-") {
                    answer.pending_named_files += 1;
                }
            } else {
                return Err(InventoryFailure::at("SPECIAL_ENTRY", &child.path()));
            }
            if answer.files + answer.directories > MAX_FILES {
                return Err(InventoryFailure::at("COUNT_LIMIT", &child.path()));
            }
        }
    }
    // Full content evidence includes WAL/N+1. Never open a SQLite connection or
    // checkpoint a database. Hash only regular files with bounded streaming reads.
    files.sort();
    directories.sort();
    let mut manifest = Sha256::new();
    for directory in directories {
        let relative = directory
            .strip_prefix(root.path())
            .map_err(|_| InventoryFailure::at("RELATIVE_DIR", &directory))?;
        let bytes = native_path_bytes(relative);
        manifest.update(b"directory:");
        manifest.update((bytes.len() as u64).to_be_bytes());
        manifest.update(&bytes);
    }
    let mut buffer = [0u8; 64 * 1024];
    for path in files {
        let relative = path
            .strip_prefix(root.path())
            .map_err(|_| InventoryFailure::at("RELATIVE_PATH", &path))?;
        let relative_bytes = native_path_bytes(relative);
        manifest.update(b"file:");
        let mut file = read_at(File::open(&path), "OPEN_FILE", &path)?;
        let before = read_at(file.metadata(), "FILE_METADATA", &path)?;
        manifest.update((relative_bytes.len() as u64).to_be_bytes());
        manifest.update(&relative_bytes);
        manifest.update(before.len().to_be_bytes());
        let mut file_hash = Sha256::new();
        loop {
            let bytes = read_at(file.read(&mut buffer), "READ_FILE", &path)?;
            if bytes == 0 {
                break;
            }
            file_hash.update(&buffer[..bytes]);
        }
        let after = read_at(file.metadata(), "FILE_METADATA", &path)?;
        if after.len() != before.len() || after.modified().ok() != before.modified().ok() {
            return Err(InventoryFailure::at("CONCURRENT_MODIFICATION", &path));
        }
        manifest.update(file_hash.finalize());
    }
    answer.content_manifest_sha256 = manifest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    for sidecar in sidecars {
        // A sidecar is not a WORKING certificate. Validate its canonical
        // generation binding and original inventory without repairing it.
        match validate_sidecar(root, &sidecar) {
            Ok(()) => answer.materializations_verified += 1,
            Err(code) => answer
                .sidecar_failures
                .push(InventoryFailure::at(code, &sidecar)),
        }
    }
    Ok(answer)
}

fn validate_sidecar(root: &MaterializationRoot, path: &Path) -> Result<(), &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "SIDECAR_UNREADABLE")?;
    if !metadata.is_file() || metadata.len() > 4096 || metadata.len() == 0 {
        return Err("SIDECAR_INVALID");
    }
    let content = fs::read_to_string(path).map_err(|_| "SIDECAR_INVALID")?;
    let value = |key: &str| -> Result<&str, &'static str> {
        let prefix = format!("{key}=");
        content
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .ok_or("SIDECAR_MISSING_ID")
    };
    let tenant = TenantId::parse(value("tenant_id")?).map_err(|_| "SIDECAR_TENANT")?;
    let profile = ProfileId::parse(value("profile_id")?).map_err(|_| "SIDECAR_PROFILE")?;
    let generation =
        GenerationId::parse(value("generation_id")?).map_err(|_| "SIDECAR_GENERATION")?;
    if path.file_name().and_then(|v| v.to_str())
        != Some(format!(".{}.materialization", generation.as_str()).as_str())
    {
        return Err("SIDECAR_NAME_MISMATCH");
    }
    let workspace = root
        .open_generation(&tenant, &profile, &generation)
        .map_err(|_| "SIDECAR_WORKSPACE")?;
    let binding = load_materialization_binding(&workspace, &tenant, &profile, &generation)
        .map_err(|_| "SIDECAR_BINDING")?;
    if workspace
        .materialization_inventory_digest()
        .map_err(|_| "SIDECAR_DIGEST")?
        != binding.materialized_inventory_digest()
    {
        return Err("SIDECAR_DIGEST_MISMATCH");
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0400 != 0
}

#[cfg(test)]
mod tests {
    use super::inventory;
    use crate::local_profile::MaterializationRoot;
    use std::fs;

    #[test]
    fn scan_never_changes_wal_or_pending_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(format!(
            "s1-profile-fixture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let root = MaterializationRoot::open_or_create(directory.join("profiles"))?;
        let data = root.path().join("user_data");
        fs::create_dir(&data)?;
        let wal = data.join("cookies.sqlite-wal");
        let pending = root.path().join(".pending-bpgw-test");
        fs::write(&wal, b"uncheckpointed-data")?;
        fs::write(&pending, b"pending-local-data")?;
        let before = fs::read(&wal)?;
        let observation = inventory(&root)?;
        assert_eq!(observation.content_manifest_sha256.len(), 64);
        assert_eq!(
            observation.content_manifest_sha256,
            inventory(&root)?.content_manifest_sha256
        );
        assert_eq!(observation.firefox_user_data_directories, 1);
        assert_eq!(observation.sqlite_wal_files, 1);
        assert_eq!(observation.pending_named_files, 1);
        assert_eq!(fs::read(&wal)?, before);
        assert_eq!(fs::read(&pending)?, b"pending-local-data");
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn opened_exclusive_file_reports_exact_inventory_failure_without_change()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::observe;
        use std::os::windows::fs::OpenOptionsExt;
        let directory = std::env::temp_dir().join(format!(
            "s1-inventory-locked-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let root = MaterializationRoot::open_or_create(directory.join("profiles"))?;
        let data = root.path().join("user_data");
        fs::create_dir(&data)?;
        let wal = data.join("cookies.sqlite-wal");
        let original = b"original WAL must survive";
        fs::write(&wal, original)?;
        let lock = fs::OpenOptions::new().read(true).share_mode(0).open(&wal)?;
        let error = super::inventory(&root).expect_err("locked original must fail");
        assert_eq!(error.code, "OPEN_FILE");
        assert_eq!(error.path, wal.to_string_lossy());
        assert!(
            error
                .detail
                .as_ref()
                .is_some_and(|detail| !detail.is_empty())
        );
        assert!(error.os_error.is_some());

        let observed = observe(&directory.join("missing-bundle"), &directory, None);
        let report: serde_json::Value = serde_json::from_str(&observed)?;
        assert_eq!(report["verdict"], "UNKNOWN");
        assert_eq!(report["reason_codes"][0], "PROFILE_INVENTORY_UNVERIFIED");
        assert_eq!(report["inventory_failure"]["code"], "OPEN_FILE");
        assert_eq!(
            report["inventory_failure"]["path"],
            wal.to_string_lossy().as_ref()
        );
        assert!(report["inventory_failure"]["detail"].as_str().is_some());
        assert!(report["inventory_failure"]["os_error"].as_i64().is_some());
        assert!(report["inventory"].is_null());
        drop(lock);
        assert_eq!(fs::read(&wal)?, original);
        assert!(super::inventory(&root).is_ok());
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn malformed_materialization_sidecar_exposes_path_and_reason_without_repair()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(format!(
            "s1-sidecar-diagnostic-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let root = MaterializationRoot::open_or_create(directory.join("profiles"))?;
        let sidecar = root.path().join(".generation_test.materialization");
        fs::write(&sidecar, b"not a valid sidecar")?;
        let observed = inventory(&root)?;
        assert_eq!(observed.materialization_sidecars, 1);
        assert_eq!(observed.materializations_verified, 0);
        assert_eq!(observed.sidecar_failures.len(), 1);
        assert_eq!(observed.sidecar_failures[0].code, "SIDECAR_MISSING_ID");
        assert_eq!(observed.sidecar_failures[0].path, sidecar.to_string_lossy());
        assert_eq!(fs::read(&sidecar)?, b"not a valid sidecar");
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn active_native_firefox_writer_blocks_s1_without_cleaning_markers()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::observe;
        use crate::local_profile::MaterializationRoot;
        use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
        use std::os::windows::fs::OpenOptionsExt;

        let installation = std::env::temp_dir().join(format!(
            "s1-live-writer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let profiles = MaterializationRoot::open_or_create(installation.join("profiles"))?;
        let generation = profiles.create_generation(
            &TenantId::parse("tenant_s1_native")?,
            &ProfileId::parse("profile_s1_native")?,
            &GenerationId::parse("generation_s1_native")?,
        )?;
        let user_data = generation.path().join("user_data");
        fs::create_dir(&user_data)?;
        let native_lock = user_data.join("parent.lock");
        fs::write(&native_lock, b"original-native-marker")?;
        let bridge_marker = generation.path().join(".profile-platform.lock");
        fs::write(&bridge_marker, b"original-bridge-marker")?;
        let handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&native_lock)?;

        let result = observe(
            &installation.join("nonexistent-bundle"),
            &installation,
            None,
        );
        assert!(result.contains("\"verdict\":\"BLOCKED\""), "{result}");
        assert!(result.contains("NATIVE_WRITER_LOCK_BUSY"), "{result}");
        assert_eq!(fs::read(&bridge_marker)?, b"original-bridge-marker");
        // Windows share_mode(0) deliberately prevents a second file open,
        // including this test's own fs::read. Inspect the original bytes only
        // after the simulated native writer releases its exclusive handle.
        drop(handle);
        assert_eq!(fs::read(&native_lock)?, b"original-native-marker");

        let next = observe(
            &installation.join("nonexistent-bundle"),
            &installation,
            None,
        );
        assert!(
            next.contains("NO_ACTIVE_WRITER_DURING_OBSERVATION"),
            "{next}"
        );
        assert!(next.contains("\"verdict\":\"UNKNOWN\""), "{next}");
        assert_eq!(fs::read(&bridge_marker)?, b"original-bridge-marker");
        fs::remove_dir_all(installation)?;
        Ok(())
    }

    #[test]
    fn missing_profile_root_does_not_get_created() -> Result<(), Box<dyn std::error::Error>> {
        use super::observe;
        let installation = std::env::temp_dir().join(format!(
            "s1-missing-profile-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir(&installation)?;
        let result = observe(&installation.join("no-bundle"), &installation, None);
        assert!(result.contains("PROFILE_ROOT_UNVERIFIED"), "{result}");
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        assert_eq!(parsed["previous_delivery_rollback"], "UNKNOWN");
        assert_eq!(parsed["independent_recovery"], "UNKNOWN");
        assert!(parsed["existing_key_public_spki_sha256"].is_null());
        assert!(!installation.join("profiles").exists());
        assert!(!installation.join("state").exists());
        fs::remove_dir(installation)?;
        Ok(())
    }

    #[test]
    fn isolated_backup_must_match_original_wal_and_tree_without_writing()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::compare_isolated_copy;
        let outer = std::env::temp_dir().join(format!(
            "s1-copy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let installed = outer.join("installed");
        let source = MaterializationRoot::open_or_create(installed.join("profiles"))?;
        let original_data = source.path().join("user_data");
        fs::create_dir(&original_data)?;
        let original_file = original_data.join("cookies.sqlite-wal");
        fs::write(&original_file, b"uncheckpointed original bytes")?;
        let expected = inventory(&source)?;

        let backup = MaterializationRoot::open_or_create(outer.join("isolated-copy"))?;
        let copy_data = backup.path().join("user_data");
        fs::create_dir(&copy_data)?;
        let backup_file = copy_data.join("cookies.sqlite-wal");
        fs::write(&backup_file, b"uncheckpointed original bytes")?;
        assert!(compare_isolated_copy(
            &installed,
            &source,
            backup.path(),
            &expected
        )?);
        assert!(compare_isolated_copy(&installed, &source, source.path(), &expected).is_err());
        assert!(compare_isolated_copy(&installed, &source, &outer, &expected).is_err());
        let missing = outer.join("missing-copy");
        assert!(compare_isolated_copy(&installed, &source, &missing, &expected).is_err());
        assert!(!missing.exists());
        assert_eq!(fs::read(&original_file)?, b"uncheckpointed original bytes");

        // A copy's mutated WAL cannot be promoted into recovery evidence.
        fs::write(&backup_file, b"mutated original checkpoint bytes")?;
        assert!(!compare_isolated_copy(
            &installed,
            &source,
            backup.path(),
            &expected
        )?);
        fs::write(&backup_file, b"uncheckpointed original bytes")?;
        fs::create_dir(copy_data.join("unmatched-empty-dir"))?;
        assert!(!compare_isolated_copy(
            &installed,
            &source,
            backup.path(),
            &expected
        )?);
        assert_eq!(fs::read(&original_file)?, b"uncheckpointed original bytes");
        fs::remove_dir_all(outer)?;
        Ok(())
    }

    #[test]
    fn inventory_rejects_unreadable_or_missing_root() {
        let missing =
            std::env::temp_dir().join(format!("s1-unavailable-{}-missing", std::process::id()));
        assert!(MaterializationRoot::open_existing(&missing).is_err());
    }
}
