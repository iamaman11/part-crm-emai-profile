//! Operator-only S1 observation. No installation, recovery, snapshot, key creation,
//! network operation or browser launch is permitted in this module.
use super::bundle::{InstallBundle, validate_directory_tree};
use crate::browser_execution::load_materialization_binding;
use crate::installed_connection::InstalledConnection;
use crate::local_profile::{DeliveryActivationGuard, LocalProfileError, MaterializationRoot};
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
use std::path::{Path, PathBuf};
use windows_device_key::PersistedP256Key;

const MAX_FILES: u64 = 500_000;

#[derive(Debug, Default, Serialize)]
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
}

#[derive(Debug, Serialize)]
struct S1Observation {
    schema_version: u32,
    kind: &'static str,
    verdict: &'static str,
    native_writer: &'static str,
    existing_key: &'static str,
    signed_delivery: &'static str,
    installed_delivery: &'static str,
    profile_inventory: &'static str,
    generation_authority: &'static str,
    independent_recovery: &'static str,
    inventory: Option<ProfileObservation>,
    reason_codes: Vec<&'static str>,
}

/// Run from the *new* unpacked Bridge executable, with bundled signed release inputs
/// in a scratch directory. Returns a structured observation, never a clearance to install.
/// The installer must independently reacquire its exclusive admission immediately at activation.
pub(super) fn observe(bundle_directory: &Path, installation_root: &Path) -> String {
    let mut result = S1Observation {
        schema_version: 1,
        kind: "PROFILE_BRIDGE_S1_READ_ONLY_OBSERVATION",
        verdict: "UNKNOWN",
        native_writer: "UNKNOWN",
        existing_key: "UNKNOWN",
        signed_delivery: "UNKNOWN",
        installed_delivery: "UNKNOWN",
        profile_inventory: "UNKNOWN",
        generation_authority: "UNKNOWN",
        independent_recovery: "UNKNOWN",
        inventory: None,
        reason_codes: Vec::new(),
    };
    inspect(bundle_directory, installation_root, &mut result);
    serde_json::to_string(&result).unwrap_or_else(|_| {
        String::from("{\"schema_version\":1,\"kind\":\"PROFILE_BRIDGE_S1_READ_ONLY_OBSERVATION\",\"verdict\":\"UNKNOWN\",\"reason_codes\":[\"SERIALIZATION_FAILED\"]}")
    })
}

fn inspect(bundle_directory: &Path, root: &Path, result: &mut S1Observation) {
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
            result.inventory = Some(inventory);
        }
        Err(_) => result.reason_codes.push("PROFILE_INVENTORY_UNVERIFIED"),
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
            result.independent_recovery = "PREVIOUS_DELIVERY_COMPONENTS_VERIFIED";
        } else {
            result
                .reason_codes
                .push("PREVIOUS_DELIVERY_BYTES_UNVERIFIED");
        }
    } else {
        result.independent_recovery = "ACTIVE_DELIVERY_ONLY";
    }

    // Device binding is the only local source for the CNG key name. Opening a
    // missing key must never trigger NCryptCreatePersistedKey.
    match WindowsDeviceApplicationBinding::open(&root.join("device-application-binding.json")) {
        Ok(binding) => {
            let name = format!("part-crm.device.{}", binding.device_id().as_str());
            match PersistedP256Key::open_existing(&name) {
                Ok(key) if key.public_key_spki_der().is_ok() => {
                    result.existing_key = "OPENED_EXISTING_NONEXPORTABLE";
                }
                _ => result.reason_codes.push("EXISTING_CNG_KEY_UNVERIFIED"),
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
    result
        .reason_codes
        .push("INDEPENDENT_PROFILE_BACKUP_NOT_YET_PROVEN");
}

fn inventory(root: &MaterializationRoot) -> Result<ProfileObservation, &'static str> {
    let mut answer = ProfileObservation::default();
    let mut sidecars = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.path().to_path_buf()];
    while let Some(directory) = stack.pop() {
        let children = fs::read_dir(directory).map_err(|_| "READ_DIRECTORY")?;
        for child in children {
            let child = child.map_err(|_| "READ_ENTRY")?;
            let metadata = fs::symlink_metadata(child.path()).map_err(|_| "READ_METADATA")?;
            if metadata.file_type().is_symlink() || is_reparse(&metadata) {
                return Err("LINK_REJECTED");
            }
            let name = child.file_name();
            let name = name.to_str().ok_or("NAME_INVALID")?;
            if metadata.is_dir() {
                answer.directories = answer.directories.checked_add(1).ok_or("COUNT_LIMIT")?;
                if name == "user_data" {
                    answer.firefox_user_data_directories += 1;
                }
                stack.push(child.path());
            } else if metadata.is_file() {
                answer.files = answer.files.checked_add(1).ok_or("COUNT_LIMIT")?;
                answer.bytes = answer
                    .bytes
                    .checked_add(metadata.len())
                    .ok_or("SIZE_LIMIT")?;
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
                return Err("SPECIAL_ENTRY");
            }
            if answer.files + answer.directories > MAX_FILES {
                return Err("COUNT_LIMIT");
            }
        }
    }
    // Full content evidence includes WAL/N+1. Never open a SQLite connection or
    // checkpoint a database. Hash only regular files with bounded streaming reads.
    files.sort();
    let mut manifest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    for path in files {
        let relative = path
            .strip_prefix(root.path())
            .map_err(|_| "RELATIVE_PATH")?;
        let relative = relative.to_str().ok_or("PATH_ENCODING")?;
        let mut file = File::open(&path).map_err(|_| "OPEN_FILE")?;
        let before = file.metadata().map_err(|_| "FILE_METADATA")?;
        manifest.update((relative.len() as u64).to_be_bytes());
        manifest.update(relative.as_bytes());
        manifest.update(before.len().to_be_bytes());
        let mut file_hash = Sha256::new();
        loop {
            let bytes = file.read(&mut buffer).map_err(|_| "READ_FILE")?;
            if bytes == 0 {
                break;
            }
            file_hash.update(&buffer[..bytes]);
        }
        let after = file.metadata().map_err(|_| "FILE_METADATA")?;
        if after.len() != before.len() || after.modified().ok() != before.modified().ok() {
            return Err("CONCURRENT_MODIFICATION");
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
        if validate_sidecar(root, &sidecar).is_ok() {
            answer.materializations_verified += 1;
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

        let result = observe(&installation.join("nonexistent-bundle"), &installation);
        assert!(result.contains("\"verdict\":\"BLOCKED\""), "{result}");
        assert!(result.contains("NATIVE_WRITER_LOCK_BUSY"), "{result}");
        assert_eq!(fs::read(&bridge_marker)?, b"original-bridge-marker");
        // Windows share_mode(0) deliberately prevents a second file open,
        // including this test's own fs::read. Inspect the original bytes only
        // after the simulated native writer releases its exclusive handle.
        drop(handle);
        assert_eq!(fs::read(&native_lock)?, b"original-native-marker");

        let next = observe(&installation.join("nonexistent-bundle"), &installation);
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
        let result = observe(&installation.join("no-bundle"), &installation);
        assert!(result.contains("PROFILE_ROOT_UNVERIFIED"), "{result}");
        assert!(!installation.join("profiles").exists());
        assert!(!installation.join("state").exists());
        fs::remove_dir(installation)?;
        Ok(())
    }

    #[test]
    fn inventory_rejects_unreadable_or_missing_root() {
        let missing =
            std::env::temp_dir().join(format!("s1-unavailable-{}-missing", std::process::id()));
        assert!(MaterializationRoot::open_existing(&missing).is_err());
    }
}
