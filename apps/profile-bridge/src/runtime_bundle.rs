use crate::ProcessControlPort;
use crate::operator_flow::RuntimeBundleSelectionPort;
use bridge_domain::{BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort};
use profile_platform_primitives::{ActorContext, GenerationId, ProfileId, SessionId};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, InventoryError, RuntimeInventory, RuntimeManifest,
    RuntimeManifestError, RuntimePlatform, Sha256Digest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const SHIPPING_RUNTIME_VERSION: &str = "2.0.0";
const SHIPPING_PYTHON_VERSION: &str = "3.12";
const SHIPPING_ENTRYPOINT: &str = "camouhost/real.py";
const SHIPPING_RUNTIME_LOCK: &str = "camouhost/runtime-lock.json";
const SHIPPING_BROWSER_EXECUTABLE: &str = "browser/camoufox.exe";
const SHIPPING_PYTHON_EXECUTABLE: &str = "python/python.exe";
const SHIPPING_RESOLVED_RUNTIME: &str = "camouhost/resolved-runtime.json";
const SHIPPING_COMPONENT_MANIFEST: &str = "runtime-manifest.json";
const SHIPPING_COMPONENT_SCHEMA_VERSION: u32 = 2;
const SHIPPING_COMPONENT_KIND: &str = "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT";
const SHIPPING_COMPONENT_PLATFORM: &str = "windows-x86_64";
const SHIPPING_COMPONENT_RELEASE_PREFIX: &str = "runtime-bundle-v2-sha256-";
const MAX_RUNTIME_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RUNTIME_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RUNTIME_FILES: usize = 500_000;
const MAX_RUNTIME_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SOURCE_PATH_BYTES: usize = 1024;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovedRuntimeBundle {
    manifest: RuntimeManifest,
    inventory: RuntimeInventory,
}

impl ApprovedRuntimeBundle {
    pub fn validate(
        manifest: RuntimeManifest,
        inventory: RuntimeInventory,
        calculated_inventory_sha256: &Sha256Digest,
    ) -> Result<Self, RuntimeBundleApprovalError> {
        manifest
            .validate_inventory_digest(calculated_inventory_sha256)
            .map_err(RuntimeBundleApprovalError::Manifest)?;
        inventory
            .validate_entrypoint(&manifest)
            .map_err(RuntimeBundleApprovalError::Inventory)?;
        Ok(Self {
            manifest,
            inventory,
        })
    }

    #[must_use]
    pub const fn manifest(&self) -> &RuntimeManifest {
        &self.manifest
    }

    #[must_use]
    pub const fn inventory(&self) -> &RuntimeInventory {
        &self.inventory
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeBundleApprovalError {
    Manifest(RuntimeManifestError),
    Inventory(InventoryError),
}

impl fmt::Display for RuntimeBundleApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => error.fmt(formatter),
            Self::Inventory(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RuntimeBundleApprovalError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PackagedFileIdentity {
    path: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PackagedFileSet {
    files: Vec<PackagedFileIdentity>,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct PackagedEntrypoints {
    browser: String,
    camouhost: String,
    python: String,
    runtime_lock: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct PackagedRuntimeManifest {
    schema_version: u32,
    kind: String,
    platform: String,
    source_commit_sha: String,
    source_inputs: PackagedFileSet,
    files: PackagedFileSet,
    entrypoints: PackagedEntrypoints,
    release_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemRuntimeBundleSelection {
    runtime_root: PathBuf,
}

impl FilesystemRuntimeBundleSelection {
    pub fn open(runtime_root: impl Into<PathBuf>) -> Result<Self, RuntimeBundleSelectionError> {
        let runtime_root = runtime_root.into();
        if !runtime_root.is_absolute() {
            return Err(RuntimeBundleSelectionError::InvalidRoot);
        }
        let metadata = fs::symlink_metadata(&runtime_root)
            .map_err(|_| RuntimeBundleSelectionError::InvalidRoot)?;
        if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
            return Err(RuntimeBundleSelectionError::InvalidRoot);
        }
        let runtime_root =
            fs::canonicalize(runtime_root).map_err(|_| RuntimeBundleSelectionError::InvalidRoot)?;
        Ok(Self { runtime_root })
    }

    #[must_use]
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    fn load_bundle(&self) -> Result<ApprovedRuntimeBundle, RuntimeBundleSelectionError> {
        let packaged = read_packaged_manifest(&self.runtime_root)?;
        validate_packaged_manifest(&packaged)?;

        let mut expected_files = HashSet::new();
        expected_files.insert(SHIPPING_COMPONENT_MANIFEST.to_owned());
        let mut expected_directories = HashSet::new();
        let mut runtime_files = Vec::with_capacity(packaged.files.files.len());
        let mut total_bytes = 0_u64;
        let mut previous_path: Option<&str> = None;
        for expected in &packaged.files.files {
            if previous_path.is_some_and(|previous| previous >= expected.path.as_str()) {
                return Err(RuntimeBundleSelectionError::InvalidRuntime);
            }
            previous_path = Some(&expected.path);
            validate_runtime_file_identity(expected)?;
            total_bytes = total_bytes
                .checked_add(expected.size_bytes)
                .ok_or(RuntimeBundleSelectionError::InvalidRuntime)?;
            if total_bytes > MAX_RUNTIME_BYTES {
                return Err(RuntimeBundleSelectionError::InvalidRuntime);
            }
            let runtime_file = read_expected_runtime_file(&self.runtime_root, expected)?;
            add_expected_directories(&expected.path, &mut expected_directories)?;
            if !expected_files.insert(expected.path.clone()) {
                return Err(RuntimeBundleSelectionError::InvalidRuntime);
            }
            runtime_files.push(runtime_file);
        }
        if runtime_files.is_empty() || runtime_files.len() > MAX_RUNTIME_FILES {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        }
        for required in [
            SHIPPING_ENTRYPOINT,
            SHIPPING_RUNTIME_LOCK,
            SHIPPING_BROWSER_EXECUTABLE,
            SHIPPING_PYTHON_EXECUTABLE,
            SHIPPING_RESOLVED_RUNTIME,
        ] {
            if !expected_files.contains(required) {
                return Err(RuntimeBundleSelectionError::InvalidRuntime);
            }
        }
        validate_runtime_tree(
            &self.runtime_root,
            &expected_files,
            &expected_directories,
        )?;

        let calculated_inventory_sha256 = inventory_digest(&runtime_files)?;
        let manifest = RuntimeManifest::new(
            SHIPPING_RUNTIME_VERSION,
            SHIPPING_PYTHON_VERSION,
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse(SHIPPING_ENTRYPOINT)
                .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?,
            calculated_inventory_sha256.clone(),
        )
        .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
        let inventory = RuntimeInventory::new(runtime_files.into_iter().map(RuntimeFile::into_entry))
            .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
        ApprovedRuntimeBundle::validate(manifest, inventory, &calculated_inventory_sha256)
            .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)
    }
}

impl RuntimeBundleSelectionPort for FilesystemRuntimeBundleSelection {
    type Error = RuntimeBundleSelectionError;

    fn select_bundle(
        &mut self,
        _actor: &ActorContext,
        _profile_id: &ProfileId,
        _generation_id: &GenerationId,
    ) -> Result<ApprovedRuntimeBundle, Self::Error> {
        self.load_bundle()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeBundleSelectionError {
    InvalidRoot,
    MissingRuntimeFile,
    InvalidRuntime,
}

impl fmt::Display for RuntimeBundleSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRoot => "shipping runtime root is invalid",
            Self::MissingRuntimeFile => "shipping runtime file is unavailable",
            Self::InvalidRuntime => "shipping runtime bundle is invalid",
        })
    }
}

impl std::error::Error for RuntimeBundleSelectionError {}

struct RuntimeFile {
    path: BundleRelativePath,
    length: u64,
    sha256: Sha256Digest,
}

impl RuntimeFile {
    fn into_entry(self) -> InventoryEntry {
        InventoryEntry::new(self.path, self.length, self.sha256)
    }
}

fn read_packaged_manifest(
    runtime_root: &Path,
) -> Result<PackagedRuntimeManifest, RuntimeBundleSelectionError> {
    let path = runtime_root.join(SHIPPING_COMPONENT_MANIFEST);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    if metadata_is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_RUNTIME_MANIFEST_BYTES
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    let bytes = fs::read(&path).map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    let manifest: PackagedRuntimeManifest =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    let mut identity: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    let release_id = identity
        .as_object_mut()
        .and_then(|value| value.remove("release_id"))
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(RuntimeBundleSelectionError::InvalidRuntime)?;
    let identity_bytes =
        serde_json::to_vec(&identity).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    let expected_release_id = format!(
        "{SHIPPING_COMPONENT_RELEASE_PREFIX}{}",
        sha256_hex(&identity_bytes)
    );
    if release_id != manifest.release_id || release_id != expected_release_id {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(manifest)
}

fn validate_packaged_manifest(
    manifest: &PackagedRuntimeManifest,
) -> Result<(), RuntimeBundleSelectionError> {
    if manifest.schema_version != SHIPPING_COMPONENT_SCHEMA_VERSION
        || manifest.kind != SHIPPING_COMPONENT_KIND
        || manifest.platform != SHIPPING_COMPONENT_PLATFORM
        || !is_lower_hex(&manifest.source_commit_sha, 40)
        || !manifest
            .release_id
            .strip_prefix(SHIPPING_COMPONENT_RELEASE_PREFIX)
            .is_some_and(|digest| is_lower_hex(digest, 64))
        || manifest.entrypoints.browser != SHIPPING_BROWSER_EXECUTABLE
        || manifest.entrypoints.camouhost != SHIPPING_ENTRYPOINT
        || manifest.entrypoints.python != SHIPPING_PYTHON_EXECUTABLE
        || manifest.entrypoints.runtime_lock != SHIPPING_RUNTIME_LOCK
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    validate_source_identity_set(&manifest.source_inputs)?;
    validate_runtime_identity_set(&manifest.files)
}

fn validate_source_identity_set(identity: &PackagedFileSet) -> Result<(), RuntimeBundleSelectionError> {
    if identity.files.is_empty()
        || identity.files.len() > MAX_RUNTIME_FILES
        || !is_lower_hex(&identity.sha256, 64)
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    let mut previous: Option<&str> = None;
    for file in &identity.files {
        validate_source_file_identity(file)?;
        if previous.is_some_and(|value| value >= file.path.as_str()) {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        }
        previous = Some(&file.path);
    }
    validate_file_set_digest(identity)
}

fn validate_runtime_identity_set(identity: &PackagedFileSet) -> Result<(), RuntimeBundleSelectionError> {
    if identity.files.is_empty()
        || identity.files.len() > MAX_RUNTIME_FILES
        || !is_lower_hex(&identity.sha256, 64)
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    let mut previous: Option<&str> = None;
    for file in &identity.files {
        validate_runtime_file_identity(file)?;
        if previous.is_some_and(|value| value >= file.path.as_str()) {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        }
        previous = Some(&file.path);
    }
    validate_file_set_digest(identity)
}

fn validate_file_set_digest(identity: &PackagedFileSet) -> Result<(), RuntimeBundleSelectionError> {
    let canonical =
        serde_json::to_vec(&identity.files).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    if sha256_hex(&canonical) != identity.sha256 {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(())
}

fn validate_source_file_identity(file: &PackagedFileIdentity) -> Result<(), RuntimeBundleSelectionError> {
    let path = file.path.as_str();
    if path.is_empty()
        || path.len() > MAX_SOURCE_PATH_BYTES
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.contains("//")
        || path.ends_with('/')
        || path.split('/').any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        || !is_lower_hex(&file.sha256, 64)
        || file.size_bytes > MAX_RUNTIME_FILE_BYTES
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(())
}

fn validate_runtime_file_identity(file: &PackagedFileIdentity) -> Result<(), RuntimeBundleSelectionError> {
    BundleRelativePath::parse(&file.path).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    if !is_lower_hex(&file.sha256, 64) || file.size_bytes > MAX_RUNTIME_FILE_BYTES {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(())
}

fn read_expected_runtime_file(
    runtime_root: &Path,
    expected: &PackagedFileIdentity,
) -> Result<RuntimeFile, RuntimeBundleSelectionError> {
    let relative = BundleRelativePath::parse(&expected.path)
        .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    let path = runtime_root.join(path_from_bundle_relative(&relative));
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    if metadata_is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() != expected.size_bytes
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    if sha256_regular_file(&path, expected.size_bytes)? != expected.sha256 {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(RuntimeFile {
        path: relative,
        length: expected.size_bytes,
        sha256: Sha256Digest::parse(expected.sha256.clone())
            .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?,
    })
}

fn sha256_regular_file(path: &Path, expected_size: u64) -> Result<String, RuntimeBundleSelectionError> {
    let mut file = fs::File::open(path).map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    let mut observed = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
        if read == 0 {
            break;
        }
        observed = observed
            .checked_add(u64::try_from(read).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?)
            .ok_or(RuntimeBundleSelectionError::InvalidRuntime)?;
        if observed > expected_size {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        }
        digest.update(&buffer[..read]);
    }
    if observed != expected_size {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    let mut encoded = String::with_capacity(64);
    for byte in digest.finalize() {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    Ok(encoded)
}

fn path_from_bundle_relative(relative: &BundleRelativePath) -> PathBuf {
    let mut path = PathBuf::new();
    for segment in relative.as_str().split('/') {
        path.push(segment);
    }
    path
}

fn add_expected_directories(
    relative: &str,
    expected: &mut HashSet<String>,
) -> Result<(), RuntimeBundleSelectionError> {
    let parts: Vec<&str> = relative.split('/').collect();
    if parts.is_empty() {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    let mut current = String::new();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);
        expected.insert(current.clone());
    }
    Ok(())
}

fn validate_runtime_tree(
    root: &Path,
    expected_files: &HashSet<String>,
    expected_directories: &HashSet<String>,
) -> Result<(), RuntimeBundleSelectionError> {
    let mut observed_files = HashSet::new();
    let mut observed_directories = HashSet::new();
    collect_runtime_tree(
        root,
        root,
        &mut observed_files,
        &mut observed_directories,
    )?;
    if &observed_files != expected_files || &observed_directories != expected_directories {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(())
}

fn collect_runtime_tree(
    root: &Path,
    current: &Path,
    files: &mut HashSet<String>,
    directories: &mut HashSet<String>,
) -> Result<(), RuntimeBundleSelectionError> {
    for entry in fs::read_dir(current).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)? {
        let entry = entry.map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        }
        let relative = normalized_descendant(root, &path)?;
        if metadata.is_dir() {
            directories.insert(relative);
            collect_runtime_tree(root, &path, files, directories)?;
        } else if metadata.is_file() {
            files.insert(relative);
        } else {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        }
    }
    Ok(())
}

fn normalized_descendant(
    root: &Path,
    path: &Path,
) -> Result<String, RuntimeBundleSelectionError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(RuntimeBundleSelectionError::InvalidRuntime);
        };
        parts.push(
            value
                .to_str()
                .ok_or(RuntimeBundleSelectionError::InvalidRuntime)?,
        );
    }
    if parts.is_empty() {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(parts.join("/"))
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = FILE_ATTRIBUTE_REPARSE_POINT;
        false
    }
}

fn inventory_digest(entries: &[RuntimeFile]) -> Result<Sha256Digest, RuntimeBundleSelectionError> {
    let mut canonical = String::from("[");
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            canonical.push(',');
        }
        canonical.push_str(&format!(
            "{{\"length\":{},\"path\":\"{}\",\"sha256\":\"{}\"}}",
            entry.length,
            entry.path.as_str(),
            entry.sha256.as_str()
        ));
    }
    canonical.push_str("]\n");
    Sha256Digest::parse(sha256_hex(canonical.as_bytes()))
        .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub struct RuntimeSessionOrchestrator;

impl RuntimeSessionOrchestrator {
    pub fn launch<P, C>(
        _bundle: &ApprovedRuntimeBundle,
        session_id: &SessionId,
        process: &mut P,
        camouhost: &mut C,
    ) -> Result<(), RuntimeLaunchError>
    where
        P: ProcessControlPort,
        C: CamouhostPort,
    {
        process
            .spawn(session_id)
            .map_err(RuntimeLaunchError::Process)?;

        let hello = camouhost
            .exchange(&CamouhostMessage::Hello {
                version: CAMOUHOST_IPC_VERSION,
            })
            .map_err(|error| rollback_camouhost(process, session_id, error))?;
        if hello
            != (CamouhostMessage::HelloAck {
                version: CAMOUHOST_IPC_VERSION,
            })
        {
            return Err(rollback_camouhost(
                process,
                session_id,
                BridgePortError::InvalidResponse,
            ));
        }

        let ready = camouhost
            .exchange(&CamouhostMessage::Launch {
                session_id: session_id.clone(),
            })
            .map_err(|error| rollback_camouhost(process, session_id, error))?;
        if ready
            != (CamouhostMessage::Ready {
                session_id: session_id.clone(),
            })
        {
            return Err(rollback_camouhost(
                process,
                session_id,
                BridgePortError::InvalidResponse,
            ));
        }
        Ok(())
    }

    pub fn close<P, C>(
        _bundle: &ApprovedRuntimeBundle,
        session_id: &SessionId,
        process: &mut P,
        camouhost: &mut C,
    ) -> Result<(), RuntimeLaunchError>
    where
        P: ProcessControlPort,
        C: CamouhostPort,
    {
        process
            .request_graceful_close(session_id)
            .map_err(RuntimeLaunchError::Process)?;
        let closed = camouhost
            .exchange(&CamouhostMessage::Close {
                session_id: session_id.clone(),
            })
            .map_err(|error| rollback_camouhost(process, session_id, error))?;
        if closed
            != (CamouhostMessage::Closed {
                session_id: session_id.clone(),
                clean: true,
            })
        {
            return Err(rollback_camouhost(
                process,
                session_id,
                BridgePortError::InvalidResponse,
            ));
        }
        process
            .confirm_stopped(session_id)
            .map_err(|error| rollback_process_failure(process, session_id, error))?;
        Ok(())
    }
}

fn rollback_camouhost<P: ProcessControlPort>(
    process: &mut P,
    session_id: &SessionId,
    source: BridgePortError,
) -> RuntimeLaunchError {
    match process.force_terminate(session_id) {
        Ok(()) => RuntimeLaunchError::Camouhost(source),
        Err(rollback) => RuntimeLaunchError::Rollback { source, rollback },
    }
}

fn rollback_process_failure<P: ProcessControlPort>(
    process: &mut P,
    session_id: &SessionId,
    source: BridgePortError,
) -> RuntimeLaunchError {
    match process.force_terminate(session_id) {
        Ok(()) => RuntimeLaunchError::Process(source),
        Err(rollback) => RuntimeLaunchError::Rollback { source, rollback },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLaunchError {
    Process(BridgePortError),
    Camouhost(BridgePortError),
    Rollback {
        source: BridgePortError,
        rollback: BridgePortError,
    },
}

impl fmt::Display for RuntimeLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Process(error) => write!(formatter, "runtime process error: {error}"),
            Self::Camouhost(error) => write!(formatter, "Camouhost protocol error: {error}"),
            Self::Rollback { source, rollback } => write!(
                formatter,
                "runtime session error: {source}; process rollback failed: {rollback}"
            ),
        }
    }
}

impl std::error::Error for RuntimeLaunchError {}

#[cfg(test)]
mod tests {
    use super::{
        ApprovedRuntimeBundle, FilesystemRuntimeBundleSelection, PackagedFileIdentity,
        RuntimeBundleApprovalError, RuntimeBundleSelectionError, RuntimeLaunchError,
        RuntimeSessionOrchestrator, SHIPPING_BROWSER_EXECUTABLE, SHIPPING_COMPONENT_KIND,
        SHIPPING_COMPONENT_PLATFORM, SHIPPING_COMPONENT_RELEASE_PREFIX,
        SHIPPING_COMPONENT_SCHEMA_VERSION, SHIPPING_ENTRYPOINT, SHIPPING_PYTHON_EXECUTABLE,
        SHIPPING_RESOLVED_RUNTIME, SHIPPING_RUNTIME_LOCK, sha256_hex,
    };
    use crate::operator_flow::RuntimeBundleSelectionPort;
    use crate::{FakeCamouhost, FakeProcessControl, ProcessAction};
    use bridge_domain::{BridgePortError, CamouhostMessage, CamouhostPort};
    use profile_platform_primitives::{
        ActorContext, ActorId, CorrelationId, GenerationId, ProfileId, SessionId, TenantId,
        TenantScope,
    };
    use runtime_bundle_domain::{
        BundleRelativePath, InventoryEntry, InventoryError, RuntimeInventory, RuntimeManifest,
        RuntimeManifestError, RuntimePlatform, Sha256Digest,
    };
    use serde_json::{Value, json};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct CloseFailCamouhost {
        inner: FakeCamouhost,
    }

    impl CamouhostPort for CloseFailCamouhost {
        fn exchange(
            &mut self,
            message: &CamouhostMessage,
        ) -> Result<CamouhostMessage, BridgePortError> {
            if matches!(message, CamouhostMessage::Close { .. }) {
                return Err(BridgePortError::Unavailable);
            }
            self.inner.exchange(message)
        }
    }

    fn digest(character: char) -> Result<Sha256Digest, Box<dyn std::error::Error>> {
        Ok(Sha256Digest::parse(character.to_string().repeat(64))?)
    }

    fn approved_bundle() -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
        let calculated = digest('a')?;
        let entrypoint = BundleRelativePath::parse("camouhost/main.py")?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(entrypoint, 10, digest('b')?)])?;
        Ok(ApprovedRuntimeBundle::validate(
            manifest,
            inventory,
            &calculated,
        )?)
    }

    fn runtime_root(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "profile-bridge-runtime-selection-{label}-{}-{nonce}",
            std::process::id()
        )))
    }

    fn actor() -> Result<ActorContext, Box<dyn std::error::Error>> {
        Ok(ActorContext::new(
            TenantScope::new(TenantId::parse("tenant_01JRUNTIMESELECT")?),
            ActorId::parse("actor_01JRUNTIMESELECT")?,
            CorrelationId::parse("corr_01JRUNTIMESELECT")?,
        ))
    }

    fn select(
        selector: &mut FilesystemRuntimeBundleSelection,
    ) -> Result<ApprovedRuntimeBundle, RuntimeBundleSelectionError> {
        selector.select_bundle(
            &actor().map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?,
            &ProfileId::parse("profile_01JRUNTIMESELECT")
                .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?,
            &GenerationId::parse("generation_01JRUNTIMESELECT")
                .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?,
        )
    }

    fn write_packaged_runtime(root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let fixtures = [
            (SHIPPING_BROWSER_EXECUTABLE, b"browser".as_slice()),
            (SHIPPING_ENTRYPOINT, b"print('real')\n".as_slice()),
            (SHIPPING_RESOLVED_RUNTIME, b"{}\n".as_slice()),
            (SHIPPING_RUNTIME_LOCK, b"{\"runtime_role\":\"real_camoufox\"}\n".as_slice()),
            (SHIPPING_PYTHON_EXECUTABLE, b"python".as_slice()),
        ];
        let mut files = Vec::new();
        for (relative, content) in fixtures {
            let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, content)?;
            files.push(PackagedFileIdentity {
                path: relative.to_owned(),
                sha256: sha256_hex(content),
                size_bytes: u64::try_from(content.len())?,
            });
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        let files_sha = sha256_hex(&serde_json::to_vec(&files)?);
        let source_files = vec![PackagedFileIdentity {
            path: "runtime/camouhost/runtime-lock.json".to_owned(),
            sha256: "a".repeat(64),
            size_bytes: 1,
        }];
        let source_sha = sha256_hex(&serde_json::to_vec(&source_files)?);
        let mut payload = json!({
            "schema_version": SHIPPING_COMPONENT_SCHEMA_VERSION,
            "kind": SHIPPING_COMPONENT_KIND,
            "platform": SHIPPING_COMPONENT_PLATFORM,
            "source_commit_sha": "1".repeat(40),
            "source_inputs": {"files": source_files, "sha256": source_sha},
            "files": {"files": files, "sha256": files_sha},
            "entrypoints": {
                "browser": SHIPPING_BROWSER_EXECUTABLE,
                "camouhost": SHIPPING_ENTRYPOINT,
                "python": SHIPPING_PYTHON_EXECUTABLE,
                "runtime_lock": SHIPPING_RUNTIME_LOCK,
            },
        });
        let release_id = format!(
            "{SHIPPING_COMPONENT_RELEASE_PREFIX}{}",
            sha256_hex(&serde_json::to_vec(&payload)?)
        );
        let Value::Object(ref mut object) = payload else {
            return Err("manifest payload is not an object".into());
        };
        object.insert("release_id".to_owned(), Value::String(release_id));
        let mut bytes = serde_json::to_vec_pretty(&payload)?;
        bytes.push(b'\n');
        fs::write(root.join("runtime-manifest.json"), bytes)?;
        Ok(())
    }

    #[test]
    fn filesystem_selector_binds_entire_packaged_runtime_and_rejects_tamper()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = runtime_root("exact")?;
        fs::create_dir_all(&root_path)?;
        write_packaged_runtime(&root_path)?;
        let mut selector = FilesystemRuntimeBundleSelection::open(&root_path)?;
        let first = select(&mut selector)?;
        assert_eq!(first.manifest().runtime_version(), "2.0.0");
        assert_eq!(first.manifest().entrypoint().as_str(), SHIPPING_ENTRYPOINT);
        assert!(first.inventory().entries().len() >= 5);

        fs::write(
            root_path.join("camouhost/runtime-lock.json"),
            b"{\"runtime_role\":\"real_camoufox\",\"changed\":true}\n",
        )?;
        assert_eq!(select(&mut selector), Err(RuntimeBundleSelectionError::InvalidRuntime));
        fs::remove_dir_all(root_path)?;
        Ok(())
    }

    #[test]
    fn filesystem_selector_rejects_extra_missing_or_relative_runtime_state()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            FilesystemRuntimeBundleSelection::open(PathBuf::from("runtime/camouhost")),
            Err(RuntimeBundleSelectionError::InvalidRoot)
        );
        let root_path = runtime_root("missing")?;
        fs::create_dir_all(&root_path)?;
        assert_eq!(
            FilesystemRuntimeBundleSelection::open(&root_path)?.load_bundle(),
            Err(RuntimeBundleSelectionError::MissingRuntimeFile)
        );
        write_packaged_runtime(&root_path)?;
        fs::write(root_path.join("unexpected.txt"), b"unexpected")?;
        let mut selector = FilesystemRuntimeBundleSelection::open(&root_path)?;
        assert_eq!(select(&mut selector), Err(RuntimeBundleSelectionError::InvalidRuntime));
        fs::remove_dir_all(root_path)?;
        Ok(())
    }

    #[test]
    fn digest_mismatch_is_rejected_before_process_spawn() -> Result<(), Box<dyn std::error::Error>>
    {
        let expected = digest('a')?;
        let entrypoint = BundleRelativePath::parse("camouhost/main.py")?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            entrypoint.clone(),
            expected,
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(entrypoint, 10, digest('b')?)])?;
        let result = ApprovedRuntimeBundle::validate(manifest, inventory, &digest('c')?);
        assert_eq!(
            result,
            Err(RuntimeBundleApprovalError::Manifest(
                RuntimeManifestError::InventoryDigestMismatch
            ))
        );
        let process = FakeProcessControl::default();
        assert!(process.actions().is_empty());
        Ok(())
    }

    #[test]
    fn missing_entrypoint_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let calculated = digest('a')?;
        let manifest = RuntimeManifest::new(
            "0.1.0",
            "3.12",
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse("camouhost/main.py")?,
            calculated.clone(),
        )?;
        let inventory = RuntimeInventory::new([InventoryEntry::new(
            BundleRelativePath::parse("camouhost/other.py")?,
            10,
            digest('b')?,
        )])?;
        assert_eq!(
            ApprovedRuntimeBundle::validate(manifest, inventory, &calculated),
            Err(RuntimeBundleApprovalError::Inventory(
                InventoryError::EntrypointMissing
            ))
        );
        Ok(())
    }

    #[test]
    fn approved_bundle_launches_and_closes_exact_session() -> Result<(), Box<dyn std::error::Error>>
    {
        let bundle = approved_bundle()?;
        let session_id = SessionId::parse("session_01JSTEP7RUNTIME")?;
        let mut process = FakeProcessControl::default();
        let mut camouhost = FakeCamouhost::default();
        RuntimeSessionOrchestrator::launch(&bundle, &session_id, &mut process, &mut camouhost)?;
        RuntimeSessionOrchestrator::close(&bundle, &session_id, &mut process, &mut camouhost)?;
        assert_eq!(
            process.actions(),
            [
                ProcessAction::Spawn(session_id.clone()),
                ProcessAction::GracefulClose(session_id.clone()),
                ProcessAction::ConfirmStopped(session_id),
            ]
        );
        Ok(())
    }

    #[test]
    fn ambiguous_close_forces_process_termination() -> Result<(), Box<dyn std::error::Error>> {
        let bundle = approved_bundle()?;
        let session_id = SessionId::parse("session_01JSTEP7CLOSEFAIL")?;
        let mut process = FakeProcessControl::default();
        let mut camouhost = CloseFailCamouhost::default();
        RuntimeSessionOrchestrator::launch(&bundle, &session_id, &mut process, &mut camouhost)?;
        assert_eq!(
            RuntimeSessionOrchestrator::close(&bundle, &session_id, &mut process, &mut camouhost),
            Err(RuntimeLaunchError::Camouhost(BridgePortError::Unavailable))
        );
        assert_eq!(
            process.actions(),
            [
                ProcessAction::Spawn(session_id.clone()),
                ProcessAction::GracefulClose(session_id.clone()),
                ProcessAction::ForceTerminate(session_id),
            ]
        );
        Ok(())
    }
}
