use crate::local_profile::{BROWSER_IDENTITY_RECORD_FILE, GenerationWorkspace, LocalProfileError};
use browser_execution_domain::{
    BrowserExecutionError, BrowserIdentityManifest, BrowserWriterDecision,
    BrowserWriterObservation, MaterializationBinding, NetworkIdentityDecision,
    NetworkIdentityObservation, NetworkIdentityPolicy,
};
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MATERIALIZATION_SCHEMA: &str = "profile-platform-materialization-v2";
const EXACT_TEXT_STAGE_ATTEMPTS: usize = 16;
static EXACT_TEXT_STAGE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const BRIDGE_LOCK_FILE: &str = ".profile-platform.lock";
const BRIDGE_LOCK_SCHEMA: &str = "profile-platform-bridge-lock-v1";
const FIREFOX_PARENT_LOCK_FILE: &str = ".parentlock";
const FIREFOX_WINDOWS_PARENT_LOCK_FILE: &str = "parent.lock";
const FIREFOX_LOCK_FILE: &str = "lock";
const FIREFOX_USER_DATA_DIRECTORY: &str = "user_data";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserLaunchBlocker {
    MaterializationStale,
    ProfileBusy,
    RecoveryRequired,
    RetryableNetworkRouteChurn,
    NetworkPolicyMismatch,
    InvalidMaterializationEvidence,
    LocalProfile(LocalProfileError),
}

impl From<LocalProfileError> for BrowserLaunchBlocker {
    fn from(error: LocalProfileError) -> Self {
        Self::LocalProfile(error)
    }
}

impl core::fmt::Display for BrowserLaunchBlocker {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::MaterializationStale => "browser materialization evidence is stale",
            Self::ProfileBusy => "browser profile writer is currently active",
            Self::RecoveryRequired => "browser profile writer state requires explicit recovery",
            Self::RetryableNetworkRouteChurn => "browser network route identity changed retryably",
            Self::NetworkPolicyMismatch => "browser network identity violates policy",
            Self::InvalidMaterializationEvidence => "browser materialization evidence is invalid",
            Self::LocalProfile(_) => "browser local profile operation failed",
        })
    }
}

impl std::error::Error for BrowserLaunchBlocker {}

/// Persist the generation-owned browser identity first, then bind local materialization evidence to
/// its exact SHA-256. The identity record lives inside the generation and therefore travels through
/// the existing encrypted snapshot/save/reopen path; the sidecar remains host-local evidence only.
pub(crate) fn persist_browser_identity(
    workspace: &GenerationWorkspace,
    identity: &BrowserIdentityManifest,
) -> Result<(), BrowserLaunchBlocker> {
    persist_exact_text(
        &browser_identity_record_path(workspace),
        &identity.to_canonical_record(),
    )
}

pub fn persist_materialization_binding(
    workspace: &GenerationWorkspace,
    binding: &MaterializationBinding,
) -> Result<(), BrowserLaunchBlocker> {
    if workspace.materialization_inventory_digest()? != binding.materialized_inventory_digest() {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let identity_record = binding.browser_identity().to_canonical_record();
    persist_exact_text(&browser_identity_record_path(workspace), &identity_record)?;
    let identity_sha256 = sha256_hex(identity_record.as_bytes());
    let path = materialization_sidecar(workspace, binding.generation_id())?;
    let content = render_binding(binding, &identity_sha256);
    persist_exact_text(&path, &content)
}

pub fn load_materialization_binding(
    workspace: &GenerationWorkspace,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> Result<MaterializationBinding, BrowserLaunchBlocker> {
    let identity_record = read_regular_file(&browser_identity_record_path(workspace))?;
    let identity_sha256 = sha256_hex(identity_record.as_bytes());
    let identity = BrowserIdentityManifest::from_canonical_record(&identity_record)
        .map_err(map_execution_error)?;
    let path = materialization_sidecar(workspace, generation_id)?;
    let content = read_regular_file(&path)?;
    parse_binding(
        &content,
        tenant_id,
        profile_id,
        generation_id,
        identity,
        &identity_sha256,
    )
}

/// Rebuild host-local materialization evidence only after an authenticated generation has been
/// opened and materialized. This never creates or changes browser identity; missing identity in the
/// generation fails closed and requires an explicit candidate-generation migration.
pub fn restore_materialization_binding_after_authoritative_open(
    workspace: &GenerationWorkspace,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    source_container_sha256: &str,
) -> Result<MaterializationBinding, BrowserLaunchBlocker> {
    let identity_record = read_regular_file(&browser_identity_record_path(workspace))?;
    let identity = BrowserIdentityManifest::from_canonical_record(&identity_record)
        .map_err(map_execution_error)?;
    let binding = MaterializationBinding::new(
        tenant_id.clone(),
        profile_id.clone(),
        generation_id.clone(),
        source_container_sha256,
        workspace.materialization_inventory_digest()?,
        identity,
    )
    .map_err(map_execution_error)?;
    persist_materialization_binding(workspace, &binding)?;
    Ok(binding)
}

/// Publish host-local readiness for a successor only after the exact sealed object was
/// verified and committed by the existing save owner. The generation-owned identity must
/// already be present and canonical: unlike initial materialization, this path must never
/// synthesize or replace browser data after the authoritative commit.
pub(crate) fn persist_committed_successor_binding(
    workspace: &GenerationWorkspace,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    committed_container_sha256: &str,
) -> Result<(), BrowserLaunchBlocker> {
    if workspace.tenant_id() != tenant_id
        || workspace.profile_id() != profile_id
        || workspace.generation_id() != generation_id
    {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let identity_record = read_regular_file(&browser_identity_record_path(workspace))?;
    let identity = BrowserIdentityManifest::from_canonical_record(&identity_record)
        .map_err(map_execution_error)?;
    if identity.to_canonical_record() != identity_record {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    let binding = MaterializationBinding::new(
        tenant_id.clone(),
        profile_id.clone(),
        generation_id.clone(),
        committed_container_sha256,
        workspace.materialization_inventory_digest()?,
        identity,
    )
    .map_err(map_execution_error)?;
    // Existing create-new/idempotent-exact sidecar owner rejects a conflicting binding.
    // If writing fails, no locally ready outcome may be reported by the save caller.
    persist_materialization_binding(workspace, &binding)?;
    if load_materialization_binding(workspace, tenant_id, profile_id, generation_id)? != binding {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    Ok(())
}

pub fn evaluate_browser_launch(
    workspace: &GenerationWorkspace,
    expected_device_id: &DeviceId,
    expected_workspace_epoch: u64,
    expected: &MaterializationBinding,
    network_policy: &NetworkIdentityPolicy,
    network_observation: Option<&NetworkIdentityObservation>,
    supervised_writer_active: bool,
) -> Result<(), BrowserLaunchBlocker> {
    verify_bridge_lock(workspace, expected_device_id, expected_workspace_epoch)?;

    let actual = load_materialization_binding(
        workspace,
        expected.tenant_id(),
        expected.profile_id(),
        expected.generation_id(),
    )?;
    if &actual != expected {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }

    let (parent_lock, lock) = observed_browser_writer_locks(workspace)?;
    let writer = BrowserWriterObservation::new(false, parent_lock, lock, supervised_writer_active);
    match writer.classify() {
        BrowserWriterDecision::Ready => {}
        BrowserWriterDecision::ProfileBusy => return Err(BrowserLaunchBlocker::ProfileBusy),
        BrowserWriterDecision::RecoveryRequired => {
            return Err(BrowserLaunchBlocker::RecoveryRequired);
        }
    }

    if workspace.materialization_inventory_digest()? != expected.materialized_inventory_digest() {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }

    match network_policy.evaluate(network_observation) {
        NetworkIdentityDecision::Accepted => {
            verify_bridge_lock(workspace, expected_device_id, expected_workspace_epoch)?;
            Ok(())
        }
        NetworkIdentityDecision::RetryableRouteChurn => {
            Err(BrowserLaunchBlocker::RetryableNetworkRouteChurn)
        }
        NetworkIdentityDecision::OperatorRemediationRequired => {
            Err(BrowserLaunchBlocker::NetworkPolicyMismatch)
        }
    }
}

fn verify_bridge_lock(
    workspace: &GenerationWorkspace,
    expected_device_id: &DeviceId,
    expected_workspace_epoch: u64,
) -> Result<(), BrowserLaunchBlocker> {
    if expected_workspace_epoch == 0 {
        return Err(BrowserLaunchBlocker::RecoveryRequired);
    }
    let lock_path = workspace.path().join(BRIDGE_LOCK_FILE);
    let actual =
        read_regular_file(&lock_path).map_err(|_| BrowserLaunchBlocker::RecoveryRequired)?;
    let expected = format!(
        "{BRIDGE_LOCK_SCHEMA}\n{}\n{expected_workspace_epoch}\n",
        expected_device_id.as_str()
    );
    if actual != expected {
        return Err(BrowserLaunchBlocker::RecoveryRequired);
    }
    Ok(())
}

fn materialization_sidecar(
    workspace: &GenerationWorkspace,
    generation_id: &GenerationId,
) -> Result<PathBuf, BrowserLaunchBlocker> {
    let parent = workspace
        .path()
        .parent()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    Ok(parent.join(format!(".{}.materialization", generation_id.as_str())))
}

fn browser_identity_record_path(workspace: &GenerationWorkspace) -> PathBuf {
    workspace.path().join(BROWSER_IDENTITY_RECORD_FILE)
}

fn render_binding(binding: &MaterializationBinding, browser_identity_sha256: &str) -> String {
    format!(
        "schema={MATERIALIZATION_SCHEMA}\ntenant_id={}\nprofile_id={}\ngeneration_id={}\nsource_container_sha256={}\nmaterialized_inventory_digest={}\nbrowser_identity_sha256={}\n",
        binding.tenant_id().as_str(),
        binding.profile_id().as_str(),
        binding.generation_id().as_str(),
        binding.source_container_sha256(),
        binding.materialized_inventory_digest(),
        browser_identity_sha256,
    )
}

fn parse_binding(
    content: &str,
    expected_tenant: &TenantId,
    expected_profile: &ProfileId,
    expected_generation: &GenerationId,
    identity: BrowserIdentityManifest,
    actual_identity_sha256: &str,
) -> Result<MaterializationBinding, BrowserLaunchBlocker> {
    let values = parse_record(content, 7, MATERIALIZATION_SCHEMA)?;
    let tenant = TenantId::parse(required(&values, "tenant_id")?)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let profile = ProfileId::parse(required(&values, "profile_id")?)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let generation = GenerationId::parse(required(&values, "generation_id")?)
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    if &tenant != expected_tenant
        || &profile != expected_profile
        || &generation != expected_generation
    {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    if required(&values, "browser_identity_sha256")? != actual_identity_sha256 {
        return Err(BrowserLaunchBlocker::MaterializationStale);
    }
    let materialized_inventory_digest = required(&values, "materialized_inventory_digest")?
        .parse::<u64>()
        .map_err(|_| BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    MaterializationBinding::new(
        tenant,
        profile,
        generation,
        required(&values, "source_container_sha256")?,
        materialized_inventory_digest,
        identity,
    )
    .map_err(map_execution_error)
}

fn parse_record<'a>(
    content: &'a str,
    expected_fields: usize,
    expected_schema: &str,
) -> Result<BTreeMap<&'a str, &'a str>, BrowserLaunchBlocker> {
    let mut values = BTreeMap::new();
    for line in content.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
        if key.is_empty() || value.is_empty() || values.insert(key, value).is_some() {
            return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
        }
    }
    if values.len() != expected_fields || values.get("schema") != Some(&expected_schema) {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    Ok(values)
}

fn required<'a>(
    values: &'a BTreeMap<&str, &str>,
    key: &str,
) -> Result<&'a str, BrowserLaunchBlocker> {
    values
        .get(key)
        .copied()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)
}

/// Publish immutable local readiness only after the complete record has been synced.
/// Direct create-new on the canonical name could expose a torn sidecar after power loss;
/// a retry would then see AlreadyExists and remain blocked with no usable evidence.
/// A same-directory hard link is no-replace and never overwrites foreign/dirty evidence.
fn persist_exact_text(path: &Path, content: &str) -> Result<(), BrowserLaunchBlocker> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            return if read_regular_file(path)? == content {
                Ok(())
            } else {
                Err(BrowserLaunchBlocker::MaterializationStale)
            };
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(LocalProfileError::from(error).into()),
    }
    let parent = path
        .parent()
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(BrowserLaunchBlocker::InvalidMaterializationEvidence)?;
    for _ in 0..EXACT_TEXT_STAGE_ATTEMPTS {
        let sequence = EXACT_TEXT_STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staging = parent.join(format!(
            ".{file_name}.pending-{}-{sequence}",
            std::process::id()
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(LocalProfileError::from(error).into()),
        };
        let write_result = file
            .write_all(content.as_bytes())
            .and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = write_result {
            // This name belongs to this exact create-new attempt. Never touch the target.
            let _ = fs::remove_file(&staging);
            return Err(LocalProfileError::from(error).into());
        }
        let published = fs::hard_link(&staging, path);
        // The hard link, once created, owns the complete fsynced bytes. Failure to
        // remove this exact staging name is a cleanup failure, not permission to
        // silently declare local readiness and accumulate unbounded staged files.
        fs::remove_file(&staging).map_err(LocalProfileError::from)?;
        match published {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return if read_regular_file(path)? == content {
                    Ok(())
                } else {
                    Err(BrowserLaunchBlocker::MaterializationStale)
                };
            }
            Err(error) => return Err(LocalProfileError::from(error).into()),
        }
    }
    Err(BrowserLaunchBlocker::InvalidMaterializationEvidence)
}

fn sha256_hex(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    let mut encoded = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn read_regular_file(path: &Path) -> Result<String, BrowserLaunchBlocker> {
    let metadata = fs::symlink_metadata(path).map_err(LocalProfileError::from)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BrowserLaunchBlocker::InvalidMaterializationEvidence);
    }
    fs::read_to_string(path)
        .map_err(LocalProfileError::from)
        .map_err(Into::into)
}

/// One lock interpretation for preflight and evidenced pre-writer cancellation.
/// Firefox uses the user_data profile directory, including Windows parent.lock.
/// Retain support for legacy workspace-root locks; neither path is ever deleted.
fn observed_browser_writer_locks(
    workspace: &GenerationWorkspace,
) -> Result<(bool, bool), BrowserLaunchBlocker> {
    let root = workspace.path();
    let parent_lock = path_present(&root.join(FIREFOX_PARENT_LOCK_FILE))?;
    let lock = path_present(&root.join(FIREFOX_LOCK_FILE))?;
    // Windows is the production shipping OS: its user_data/parent.lock is a real
    // OS file-handle lock. Other platforms retain the prior root-level protocol;
    // Unix .parentlock symlinks need separate process-liveness proof.
    #[cfg(windows)]
    {
        let browser_data = root.join(FIREFOX_USER_DATA_DIRECTORY);
        let parent_lock = parent_lock
            || path_present(&browser_data.join(FIREFOX_PARENT_LOCK_FILE))?
            || windows_parent_lock_active(&browser_data.join(FIREFOX_WINDOWS_PARENT_LOCK_FILE))?;
        let lock = lock || path_present(&browser_data.join(FIREFOX_LOCK_FILE))?;
        Ok((parent_lock, lock))
    }
    #[cfg(not(windows))]
    {
        Ok((parent_lock, lock))
    }
}

/// Negative browser-writer evidence for a locally held Bridge lock before runtime spawn.
pub(crate) fn browser_writer_locks_absent(
    workspace: &GenerationWorkspace,
) -> Result<bool, BrowserLaunchBlocker> {
    let (parent_lock, lock) = observed_browser_writer_locks(workspace)?;
    if parent_lock || lock {
        return Ok(false);
    }
    #[cfg(not(windows))]
    {
        // Camouhost alone can distinguish stale Unix fcntl marker files from
        // active writers before launch. The stricter coordinator Clean proof
        // cannot borrow that future check: ANY unknown marker forbids Clean.
        let data = workspace.path().join(FIREFOX_USER_DATA_DIRECTORY);
        for marker in [
            FIREFOX_PARENT_LOCK_FILE,
            FIREFOX_WINDOWS_PARENT_LOCK_FILE,
            FIREFOX_LOCK_FILE,
        ] {
            if path_present(&data.join(marker))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Windows Firefox may retain parent.lock after the owning process exits.
/// An exclusive read-only open proves no active Windows file-handle ownership.
/// Unreadable/locked evidence never authorizes Clean.
#[cfg(windows)]
fn windows_parent_lock_active(path: &Path) -> Result<bool, BrowserLaunchBlocker> {
    use std::os::windows::fs::OpenOptionsExt;
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Ok(true);
            }
            match OpenOptions::new().read(true).share_mode(0).open(path) {
                Ok(handle) => {
                    drop(handle);
                    Ok(false)
                }
                Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) => Ok(true),
                Err(error) => Err(LocalProfileError::from(error).into()),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(LocalProfileError::from(error).into()),
    }
}

fn path_present(path: &Path) -> Result<bool, BrowserLaunchBlocker> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(LocalProfileError::from(error).into()),
    }
}

fn map_execution_error(_error: BrowserExecutionError) -> BrowserLaunchBlocker {
    BrowserLaunchBlocker::InvalidMaterializationEvidence
}

#[cfg(test)]
mod tests {
    use super::{
        BROWSER_IDENTITY_RECORD_FILE, BrowserLaunchBlocker, evaluate_browser_launch,
        load_materialization_binding, persist_browser_identity,
        persist_committed_successor_binding, persist_materialization_binding,
    };
    use crate::local_profile::{BridgeWorkspaceLock, MaterializationRoot};
    use crate::test_support::remove_test_root;
    use browser_execution_domain::{
        BrowserIdentityManifest, BrowserOsIdentity, DisplayIdentity, FontIdentity,
        GraphicsIdentity, HardwareCapabilityIdentity, LocaleIdentity, MaterializationBinding,
        NetworkClass, NetworkIdentityObservation, NetworkIdentityPolicy, OriginDeterminismMode,
        OriginDeterministicIdentity, ProfileStableIdentity,
    };
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root_path(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "profile-bridge-phase2f-{label}-{}-{nonce}",
            std::process::id()
        )))
    }

    fn identity() -> Result<BrowserIdentityManifest, Box<dyn std::error::Error>> {
        let digest = |character: char| character.to_string().repeat(64);
        let stable = ProfileStableIdentity::new(
            1,
            BrowserOsIdentity::new(
                "Mozilla/5.0 Firefox/152.0",
                152,
                "Win32",
                "Windows NT 10.0; Win64; x64",
            )?,
            HardwareCapabilityIdentity::new(8, 8, 0)?,
            DisplayIdentity::new(1920, 1080, 1920, 1040, 0, 0, 24, 24, 1000)?,
            GraphicsIdentity::new(
                "Google Inc. (NVIDIA)",
                "ANGLE (NVIDIA GeForce)",
                digest('1'),
                digest('2'),
                digest('3'),
                digest('4'),
                digest('5'),
            )?,
            FontIdentity::new(digest('6'), digest('7'))?,
            OriginDeterministicIdentity::new(
                OriginDeterminismMode::ProfileGenerationSeed,
                digest('8'),
                digest('9'),
            )?,
            LocaleIdentity::new("en-US", digest('a'), Some(digest('b')))?,
        )?;
        Ok(BrowserIdentityManifest::new(
            2,
            "profile-stability-v1",
            "1.2.3",
            digest('c'),
            "profile-stability-v1-probe-test",
            digest('d'),
            stable,
        )?)
    }

    fn policy(route: &str) -> Result<NetworkIdentityPolicy, Box<dyn std::error::Error>> {
        Ok(NetworkIdentityPolicy::new(
            Some("PL".to_owned()),
            Some("Mazowieckie".to_owned()),
            Some("Europe/Warsaw".to_owned()),
            [NetworkClass::Mobile],
            [5617],
            Some(route.to_owned()),
        )?)
    }

    fn observation(route: &str) -> Result<NetworkIdentityObservation, Box<dyn std::error::Error>> {
        Ok(NetworkIdentityObservation::new(
            "PL",
            "Mazowieckie",
            "Europe/Warsaw",
            NetworkClass::Mobile,
            5617,
            route,
        )?)
    }

    #[test]
    fn atomic_exact_text_publication_never_exposes_partial_canonical_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("atomic-exact-text")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JATOMICTEXT")?;
        let profile = ProfileId::parse("profile_01JATOMICTEXT")?;
        let generation = GenerationId::parse("generation_01JATOMICTEXT")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let canonical = workspace.path().join(BROWSER_IDENTITY_RECORD_FILE);
        let partial = workspace.path().join(".interrupted-pending");
        fs::write(&partial, b"truncated-record")?;
        // Staging garbage is not allowed to impersonate the canonical record.
        assert!(!canonical.exists());
        persist_browser_identity(&workspace, &identity()?)?;
        assert_eq!(
            fs::read_to_string(&canonical)?,
            identity()?.to_canonical_record()
        );
        persist_browser_identity(&workspace, &identity()?)?;
        let canonical_before = fs::read(&canonical)?;
        fs::write(&canonical, b"corrupt-existing-evidence")?;
        assert_eq!(
            persist_browser_identity(&workspace, &identity()?),
            Err(BrowserLaunchBlocker::MaterializationStale)
        );
        assert_eq!(fs::read(&canonical)?, b"corrupt-existing-evidence");
        assert_ne!(canonical_before, fs::read(&canonical)?);
        // No remaining .pending from a successful owned publish. A foreign
        // abandoned attempt is not deleted without an ownership witness.
        let files = fs::read_dir(workspace.path())?
            .map(|item| item.map(|item| item.file_name().to_string_lossy().to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        assert!(
            !files
                .iter()
                .any(|name| name.starts_with("..profile-browser-identity-v1.pending-"))
        );
        assert!(partial.exists());
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn committed_successor_binding_requires_original_identity_and_rejects_conflicting_rewrite()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("committed-binding-fail-closed")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JCOMMITTEDBIND")?;
        let profile = ProfileId::parse("profile_01JCOMMITTEDBIND")?;
        let generation = GenerationId::parse("generation_01JCOMMITTEDBIND")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let prefs = workspace.path().join("prefs.js");
        fs::write(&prefs, b"browser-retained-bytes")?;

        // A copied directory without the actual generation-owned identity is not ready.
        assert!(
            persist_committed_successor_binding(
                &workspace,
                &tenant,
                &profile,
                &generation,
                &"a".repeat(64),
            )
            .is_err()
        );
        assert_eq!(fs::read(&prefs)?, b"browser-retained-bytes");
        assert!(load_materialization_binding(&workspace, &tenant, &profile, &generation).is_err());

        persist_browser_identity(&workspace, &identity()?)?;
        persist_committed_successor_binding(
            &workspace,
            &tenant,
            &profile,
            &generation,
            &"a".repeat(64),
        )?;
        let original = load_materialization_binding(&workspace, &tenant, &profile, &generation)?;
        assert_eq!(original.source_container_sha256(), "a".repeat(64));
        // Exact idempotent replay is safe; different immutable object identity cannot rewrite it.
        persist_committed_successor_binding(
            &workspace,
            &tenant,
            &profile,
            &generation,
            &"a".repeat(64),
        )?;
        assert_eq!(
            persist_committed_successor_binding(
                &workspace,
                &tenant,
                &profile,
                &generation,
                &"b".repeat(64),
            ),
            Err(BrowserLaunchBlocker::MaterializationStale)
        );
        assert_eq!(
            load_materialization_binding(&workspace, &tenant, &profile, &generation)?,
            original
        );
        assert_eq!(fs::read(&prefs)?, b"browser-retained-bytes");
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn launch_requires_exact_materialized_inventory_and_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("freshness")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JLAUNCH")?;
        let profile = ProfileId::parse("profile_01JLAUNCH")?;
        let generation = GenerationId::parse("generation_01JLAUNCH")?;
        let device = DeviceId::parse("device_01JLAUNCH")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        fs::write(workspace.path().join("prefs.js"), b"accepted")?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        assert!(
            workspace
                .path()
                .join(BROWSER_IDENTITY_RECORD_FILE)
                .is_file()
        );
        assert_eq!(
            load_materialization_binding(
                &workspace,
                binding.tenant_id(),
                binding.profile_id(),
                binding.generation_id(),
            )?,
            binding
        );
        evaluate_browser_launch(
            &workspace,
            &device,
            1,
            &binding,
            &policy("route-a")?,
            Some(&observation("route-a")?),
            false,
        )?;

        fs::create_dir(workspace.path().join("user_data"))?;
        fs::write(
            workspace.path().join("user_data/storage-state.bin"),
            b"mutable-browser-state",
        )?;
        evaluate_browser_launch(
            &workspace,
            &device,
            1,
            &binding,
            &policy("route-a")?,
            Some(&observation("route-a")?),
            false,
        )?;

        fs::write(workspace.path().join("prefs.js"), b"mutated")?;
        assert_eq!(
            evaluate_browser_launch(
                &workspace,
                &device,
                1,
                &binding,
                &policy("route-a")?,
                Some(&observation("route-a")?),
                false,
            ),
            Err(BrowserLaunchBlocker::MaterializationStale)
        );
        bridge_lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn tampered_generation_owned_identity_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("identity-tamper")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JIDENTITY")?;
        let profile = ProfileId::parse("profile_01JIDENTITY")?;
        let generation = GenerationId::parse("generation_01JIDENTITY")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let binding = MaterializationBinding::new(
            tenant.clone(),
            profile.clone(),
            generation.clone(),
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        let identity_path = workspace.path().join(BROWSER_IDENTITY_RECORD_FILE);
        let original = fs::read_to_string(&identity_path)?;
        fs::write(
            &identity_path,
            original.replace("display_width=1920", "display_width=1600"),
        )?;
        assert_eq!(
            load_materialization_binding(&workspace, &tenant, &profile, &generation),
            Err(BrowserLaunchBlocker::InvalidMaterializationEvidence)
        );
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn direct_connection_keeps_writer_and_identity_checks() -> Result<(), Box<dyn std::error::Error>>
    {
        let root_path = root_path("direct-network")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JNETWORK")?;
        let profile = ProfileId::parse("profile_01JNETWORK")?;
        let generation = GenerationId::parse("generation_01JNETWORK")?;
        let device = DeviceId::parse("device_01JNETWORK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        let direct = NetworkIdentityPolicy::direct();
        evaluate_browser_launch(&workspace, &device, 1, &binding, &direct, None, false)?;
        assert_eq!(
            evaluate_browser_launch(
                &workspace,
                &DeviceId::parse("device_FOREIGN")?,
                1,
                &binding,
                &direct,
                None,
                false,
            ),
            Err(BrowserLaunchBlocker::RecoveryRequired)
        );
        assert_eq!(
            evaluate_browser_launch(&workspace, &device, 1, &binding, &direct, None, true),
            Err(BrowserLaunchBlocker::ProfileBusy)
        );
        fs::write(
            workspace.path().join("prefs.js"),
            b"changed immutable input",
        )?;
        assert_eq!(
            evaluate_browser_launch(&workspace, &device, 1, &binding, &direct, None, false),
            Err(BrowserLaunchBlocker::MaterializationStale)
        );
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn deleting_a_bound_policy_cannot_downgrade_to_direct() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::shipping_network::{GenerationNetworkEvidence, PROFILE_NETWORK_POLICY_FILE};
        let root_path = root_path("network-policy-removal")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JNETWORK")?;
        let profile = ProfileId::parse("profile_01JNETWORK")?;
        let generation = GenerationId::parse("generation_01JNETWORK")?;
        let device = DeviceId::parse("device_01JNETWORK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let policy_path = workspace.path().join(PROFILE_NETWORK_POLICY_FILE);
        fs::write(&policy_path, br#"{"schema":"profile-bridge-network-policy-v1","country":"PL","region":null,"timezone":null,"allowedNetworkClasses":["mobile"],"allowedAsns":[5617],"requiredRouteIdentity":null}"#)?;
        let lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        let restricted = GenerationNetworkEvidence::open(&workspace)?.policy();
        assert_eq!(
            evaluate_browser_launch(&workspace, &device, 1, &binding, &restricted, None, false),
            Err(BrowserLaunchBlocker::NetworkPolicyMismatch)
        );
        fs::remove_file(policy_path)?;
        let direct = GenerationNetworkEvidence::open(&workspace)?.policy();
        assert!(!direct.requires_observation());
        assert_eq!(
            evaluate_browser_launch(&workspace, &device, 1, &binding, &direct, None, false),
            Err(BrowserLaunchBlocker::MaterializationStale)
        );
        lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_unknown_firefox_marker_never_authorizes_clean_release()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("unix-marker-clean");
        let root_path = root_path?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JUNIXCLEAN")?;
        let profile = ProfileId::parse("profile_01JUNIXCLEAN")?;
        let generation = GenerationId::parse("generation_01JUNIXCLEAN")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        fs::create_dir_all(workspace.path().join("user_data"))?;
        fs::write(workspace.path().join("user_data/.parentlock"), b"leftover")?;
        assert!(!super::browser_writer_locks_absent(&workspace)?);
        assert_eq!(
            fs::read(workspace.path().join("user_data/.parentlock"))?,
            b"leftover"
        );
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_parent_lock_requires_live_exclusive_file_handle_to_block()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::windows::fs::OpenOptionsExt;
        let root_path = root_path("windows-parent-lock")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JWINDOWSLOCK")?;
        let profile = ProfileId::parse("profile_01JWINDOWSLOCK")?;
        let generation = GenerationId::parse("generation_01JWINDOWSLOCK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        fs::create_dir_all(workspace.path().join("user_data"))?;
        let path = workspace.path().join("user_data/parent.lock");
        fs::write(&path, b"leftover-windows-lock-file")?;
        assert!(super::browser_writer_locks_absent(&workspace)?);
        {
            let _active_handle = fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&path)?;
            assert!(!super::browser_writer_locks_absent(&workspace)?);
        }
        assert!(super::browser_writer_locks_absent(&workspace)?);
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn browser_lock_is_never_deleted_and_requires_recovery()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("browser-lock")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JLOCK")?;
        let profile = ProfileId::parse("profile_01JLOCK")?;
        let generation = GenerationId::parse("generation_01JLOCK")?;
        let device = DeviceId::parse("device_01JLOCK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        fs::write(workspace.path().join(".parentlock"), b"browser-owned")?;
        assert_eq!(
            evaluate_browser_launch(
                &workspace,
                &device,
                1,
                &binding,
                &policy("route-a")?,
                Some(&observation("route-a")?),
                false,
            ),
            Err(BrowserLaunchBlocker::RecoveryRequired)
        );
        assert!(workspace.path().join(".parentlock").exists());
        bridge_lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn network_route_churn_is_retryable_before_launch() -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("network")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JNETWORK")?;
        let profile = ProfileId::parse("profile_01JNETWORK")?;
        let generation = GenerationId::parse("generation_01JNETWORK")?;
        let device = DeviceId::parse("device_01JNETWORK")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 1)?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;
        assert_eq!(
            evaluate_browser_launch(
                &workspace,
                &device,
                1,
                &binding,
                &policy("route-a")?,
                Some(&observation("route-b")?),
                false,
            ),
            Err(BrowserLaunchBlocker::RetryableNetworkRouteChurn)
        );
        bridge_lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }

    #[test]
    fn stale_workspace_epoch_or_foreign_device_fails_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root_path("stale-ownership")?;
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let tenant = TenantId::parse("tenant_01JSTALE")?;
        let profile = ProfileId::parse("profile_01JSTALE")?;
        let generation = GenerationId::parse("generation_01JSTALE")?;
        let device = DeviceId::parse("device_01JSTALE")?;
        let foreign_device = DeviceId::parse("device_02JSTALE")?;
        let workspace = root.create_generation(&tenant, &profile, &generation)?;
        let bridge_lock = BridgeWorkspaceLock::acquire(&workspace, &device, 7)?;
        let binding = MaterializationBinding::new(
            tenant,
            profile,
            generation,
            "c".repeat(64),
            workspace.materialization_inventory_digest()?,
            identity()?,
        )?;
        persist_materialization_binding(&workspace, &binding)?;

        assert_eq!(
            evaluate_browser_launch(
                &workspace,
                &device,
                8,
                &binding,
                &policy("route-a")?,
                Some(&observation("route-a")?),
                false,
            ),
            Err(BrowserLaunchBlocker::RecoveryRequired)
        );
        assert_eq!(
            evaluate_browser_launch(
                &workspace,
                &foreign_device,
                7,
                &binding,
                &policy("route-a")?,
                Some(&observation("route-a")?),
                false,
            ),
            Err(BrowserLaunchBlocker::RecoveryRequired)
        );
        assert!(workspace.path().join(super::BRIDGE_LOCK_FILE).exists());
        bridge_lock.release()?;
        remove_test_root(&root_path)?;
        Ok(())
    }
}
