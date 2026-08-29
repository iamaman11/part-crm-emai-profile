use crate::ProcessControlPort;
use crate::operator_flow::RuntimeBundleSelectionPort;
use bridge_domain::{BridgePortError, CAMOUHOST_IPC_VERSION, CamouhostMessage, CamouhostPort};
use profile_platform_primitives::{ActorContext, GenerationId, ProfileId, SessionId};
use runtime_bundle_domain::{
    BundleRelativePath, InventoryEntry, InventoryError, RuntimeInventory, RuntimeManifest,
    RuntimeManifestError, RuntimePlatform, Sha256Digest,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const SHIPPING_RUNTIME_VERSION: &str = "2.0.0";
const SHIPPING_PYTHON_VERSION: &str = "3.12";
const SHIPPING_ENTRYPOINT: &str = "camouhost/real.py";
const SHIPPING_RUNTIME_LOCK: &str = "camouhost/runtime-lock.json";
const MAX_RUNTIME_FILE_BYTES: u64 = 32 * 1024 * 1024;
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
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RuntimeBundleSelectionError::InvalidRoot);
        }
        let runtime_root = fs::canonicalize(runtime_root)
            .map_err(|_| RuntimeBundleSelectionError::InvalidRoot)?;
        Ok(Self { runtime_root })
    }

    #[must_use]
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    fn load_bundle(&self) -> Result<ApprovedRuntimeBundle, RuntimeBundleSelectionError> {
        let entrypoint = read_runtime_file(&self.runtime_root, SHIPPING_ENTRYPOINT)?;
        let runtime_lock = read_runtime_file(&self.runtime_root, SHIPPING_RUNTIME_LOCK)?;
        let entries = [entrypoint, runtime_lock];
        let calculated_inventory_sha256 = inventory_digest(&entries)?;
        let manifest = RuntimeManifest::new(
            SHIPPING_RUNTIME_VERSION,
            SHIPPING_PYTHON_VERSION,
            RuntimePlatform::WindowsX86_64,
            BundleRelativePath::parse(SHIPPING_ENTRYPOINT)
                .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?,
            calculated_inventory_sha256.clone(),
        )
        .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
        let inventory = RuntimeInventory::new(entries.into_iter().map(RuntimeFile::into_entry))
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

fn read_runtime_file(
    runtime_root: &Path,
    relative: &str,
) -> Result<RuntimeFile, RuntimeBundleSelectionError> {
    let relative = BundleRelativePath::parse(relative)
        .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?;
    let path = runtime_root.join(relative.as_str());
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_RUNTIME_FILE_BYTES
    {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    let bytes = fs::read(path).map_err(|_| RuntimeBundleSelectionError::MissingRuntimeFile)?;
    if u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
        return Err(RuntimeBundleSelectionError::InvalidRuntime);
    }
    Ok(RuntimeFile {
        path: relative,
        length: metadata.len(),
        sha256: Sha256Digest::parse(sha256_hex(&bytes))
            .map_err(|_| RuntimeBundleSelectionError::InvalidRuntime)?,
    })
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
        ApprovedRuntimeBundle, FilesystemRuntimeBundleSelection, RuntimeBundleApprovalError,
        RuntimeBundleSelectionError, RuntimeLaunchError, RuntimeSessionOrchestrator,
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

    #[test]
    fn filesystem_selector_binds_exact_real_runtime_file_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = runtime_root("exact")?;
        fs::create_dir_all(root_path.join("camouhost"))?;
        fs::write(root_path.join("camouhost/real.py"), b"print('real')\n")?;
        fs::write(
            root_path.join("camouhost/runtime-lock.json"),
            b"{\"runtime_role\":\"real_camoufox\"}\n",
        )?;
        let mut selector = FilesystemRuntimeBundleSelection::open(&root_path)?;
        let first = select(&mut selector)?;
        assert_eq!(first.manifest().runtime_version(), "2.0.0");
        assert_eq!(first.manifest().entrypoint().as_str(), "camouhost/real.py");
        let first_digest = first.manifest().inventory_sha256().clone();

        fs::write(
            root_path.join("camouhost/runtime-lock.json"),
            b"{\"runtime_role\":\"real_camoufox\",\"changed\":true}\n",
        )?;
        let second = select(&mut selector)?;
        assert_ne!(second.manifest().inventory_sha256(), &first_digest);
        fs::remove_dir_all(root_path)?;
        Ok(())
    }

    #[test]
    fn filesystem_selector_rejects_missing_or_relative_runtime_roots()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            FilesystemRuntimeBundleSelection::open(PathBuf::from("runtime/camouhost")),
            Err(RuntimeBundleSelectionError::InvalidRoot)
        );
        let root_path = runtime_root("missing")?;
        fs::create_dir_all(root_path.join("camouhost"))?;
        fs::write(root_path.join("camouhost/runtime-lock.json"), b"{}\n")?;
        let mut selector = FilesystemRuntimeBundleSelection::open(&root_path)?;
        assert_eq!(
            select(&mut selector),
            Err(RuntimeBundleSelectionError::MissingRuntimeFile)
        );
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
