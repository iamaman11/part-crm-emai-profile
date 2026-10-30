//! Operator-only S1 observation. No installation, recovery, snapshot, key creation,
 //! network operation or browser launch is permitted in this module.
use super::bundle::{InstallBundle, validate_directory_tree};
use crate::installed_connection::InstalledConnection;
use crate::local_profile::{DeliveryActivationGuard, LocalProfileError, MaterializationRoot};
use crate::windows_delivery::verify_delivery_candidate;
use crate::windows_delivery_coordinator::accepted_floor;
use crate::windows_delivery_signature::WindowsCmsSignatureVerifier;
use crate::windows_delivery_store::DeliveryStateStore;
use crate::windows_device_application::WindowsDeviceApplicationBinding;
use serde::Serialize;
use std::fs;
use std::path::Path;
use windows_device_key::PersistedP256Key;

const MAX_FILES: u64 = 500_000;

#[derive(Debug, Default, Serialize)]
struct ProfileObservation {
    files: u64,
    directories: u64,
    bytes: u64,
    firefox_user_data_directories: u64,
    sqlite_wal_files: u64,
    materialization_sidecars: u64,
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
    match inventory(workspace.path()) {
        Ok(inventory) => {
            result.profile_inventory = "STRUCTURE_OBSERVED";
            result.inventory = Some(inventory);
        }
        Err(_) => result.reason_codes.push("PROFILE_INVENTORY_UNVERIFIED"),
    }

    let state = match DeliveryStateStore::open(root.join("state")) {
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
            result.reason_codes.push("INSTALLED_DELIVERY_STATE_UNVERIFIED");
            return;
        }
    };
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
    result.reason_codes.push("GENERATION_AUTHORITY_NOT_YET_PROVEN");
    result.reason_codes.push("INDEPENDENT_RECOVERY_NOT_YET_PROVEN");
}

fn inventory(root: &Path) -> Result<ProfileObservation, &'static str> {
    let mut answer = ProfileObservation::default();
    let mut stack = vec![root.to_path_buf()];
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
                answer.bytes = answer.bytes.checked_add(metadata.len()).ok_or("SIZE_LIMIT")?;
                if name.ends_with("-wal") {
                    answer.sqlite_wal_files += 1;
                }
                if name.ends_with(".materialization") {
                    answer.materialization_sidecars += 1;
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
    Ok(answer)
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0400 != 0
}

#[cfg(test)]
mod tests {
    use super::inventory;
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
        fs::create_dir(&directory)?;
        let data = directory.join("user_data");
        fs::create_dir(&data)?;
        fs::write(data.join("cookies.sqlite-wal"), b"uncheckpointed-data")?;
        fs::write(directory.join(".pending-bpgw-test"), b"pending-local-data")?;
        let before = fs::read(data.join("cookies.sqlite-wal"))?;
        let observation = inventory(&directory)?;
        assert_eq!(observation.firefox_user_data_directories, 1);
        assert_eq!(observation.sqlite_wal_files, 1);
        assert_eq!(observation.pending_named_files, 1);
        assert_eq!(fs::read(data.join("cookies.sqlite-wal"))?, before);
        assert_eq!(fs::read(directory.join(".pending-bpgw-test"))?, b"pending-local-data");
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn inventory_rejects_unreadable_or_missing_root() {
        let missing = std::env::temp_dir().join(format!(
            "s1-unavailable-{}-missing", std::process::id()
        ));
        assert!(inventory(&missing).is_err());
    }
}
