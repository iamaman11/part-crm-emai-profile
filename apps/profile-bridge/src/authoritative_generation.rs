use crate::browser_execution::{
    BrowserLaunchBlocker, browser_writer_locks_absent, load_materialization_binding,
    restore_materialization_binding_after_authoritative_open,
};
use crate::generation_reopen::{
    GenerationObjectDownloadError, GenerationObjectDownloadPort, GenerationReopenControlError,
    GenerationReopenControlPort,
};
use crate::generation_object_spool::VerifiedGenerationObjectFile;
use crate::generation_snapshot::{
    GenerationSnapshotError, StreamedWorkspaceSnapshot,
    materialize_workspace_snapshot_streaming_with_pre_publish,
    materialize_workspace_snapshot_with_pre_publish, verify_workspace_matches_snapshot,
};
use crate::local_profile::{BridgeWorkspaceLock, LocalProfileError, MaterializationRoot};
use encrypted_generation_domain::{
    canonical_generation_object_key, inspect_generation_metadata_prelude, open_generation_expected,
    open_generation_streaming,
};
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};

const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

/// A proven read-only rejection before either workspace creation or a browser writer.
/// It is not proof that the downloaded object was trustworthy or the launch is admissible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthoritativeGenerationPreWriteError {
    DescriptorMismatch,
    Download(GenerationObjectDownloadError),
    MetadataMismatch,
    OpeningControl(GenerationReopenControlError),
    OpeningMaterialMismatch,
    Decryption,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthoritativeGenerationMaterializationError {
    Local,
    IncompleteLocalEvidence,
    InvalidLocalEvidence,
    LocalAccessDenied,
    /// The remote read failed before any local workspace existed or local writer was attempted.
    /// The caller must independently reconfirm absence before a clean coordinator release.
    ControlPlaneBeforeLocalUse(GenerationReopenControlError),
    /// Only emitted when the exact workspace was absent before all read-only operations.
    /// Operator independently rechecks absence before requesting a clean no-use release.
    ReadBeforeLocalUse(AuthoritativeGenerationPreWriteError),
    ControlPlane(GenerationReopenControlError),
    Download(GenerationObjectDownloadError),
    DescriptorMismatch,
    MetadataMismatch,
    OpeningMaterialMismatch,
    Decryption,
    BrowserIdentity,
    Snapshot,
}

impl core::fmt::Display for AuthoritativeGenerationMaterializationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Local => "authoritative local generation lookup failed",
            Self::IncompleteLocalEvidence => {
                "existing local generation is incomplete; retain bytes for recovery"
            }
            Self::InvalidLocalEvidence => "existing local generation evidence is invalid",
            Self::LocalAccessDenied => "local generation evidence cannot be accessed",
            Self::ControlPlaneBeforeLocalUse(_) | Self::ControlPlane(_) => {
                "authoritative generation control-plane request failed"
            }
            Self::Download(_) => "authoritative generation object download failed",
            Self::ReadBeforeLocalUse(reason) => match reason {
                AuthoritativeGenerationPreWriteError::DescriptorMismatch => {
                    "download capability does not name the requested authority"
                }
                AuthoritativeGenerationPreWriteError::Download(_) => {
                    "authoritative generation object download failed"
                }
                AuthoritativeGenerationPreWriteError::MetadataMismatch => {
                    "encrypted generation metadata does not match server authority"
                }
                AuthoritativeGenerationPreWriteError::OpeningControl(_) => {
                    "authoritative generation opening request failed"
                }
                AuthoritativeGenerationPreWriteError::OpeningMaterialMismatch => {
                    "opening material does not match the authenticated encrypted metadata"
                }
                AuthoritativeGenerationPreWriteError::Decryption => {
                    "authoritative generation decryption failed"
                }
            },
            Self::DescriptorMismatch => "download capability does not name the requested authority",
            Self::MetadataMismatch => {
                "encrypted generation metadata does not match server authority"
            }
            Self::OpeningMaterialMismatch => {
                "opening material does not match the authenticated encrypted metadata"
            }
            Self::Decryption => "authoritative generation decryption failed",
            Self::BrowserIdentity => {
                "authoritative generation browser identity materialization failed"
            }
            Self::Snapshot => "authoritative generation snapshot materialization failed",
        })
    }
}

impl std::error::Error for AuthoritativeGenerationMaterializationError {}

impl AuthoritativeGenerationMaterializationError {
    /// All read-only refusals preceding the first local write may prove no materialization use
    /// only when the initial exact workspace was absent. Caller must recheck exact absence.
    #[must_use]
    pub const fn proves_no_local_write_and_absent_workspace(self) -> bool {
        matches!(
            self,
            Self::ControlPlaneBeforeLocalUse(_) | Self::ReadBeforeLocalUse(_)
        )
    }
}

/// Ensures that the exact generation already selected by the P2 server-side launch authority is
/// locally materialized. Local absence is never authority to choose another generation: the only
/// recovery path asks the live coordinator for the server-selected active VERIFIED descriptor,
/// downloads that exact immutable object, proves its metadata before requesting the historical
/// DEK, decrypts it with exact identity checks, and delegates BPGW materialization to the one local
/// snapshot/filesystem owner. Generation-owned browser identity is parsed from the authenticated
/// snapshot and its host-local materialization evidence is prepared before the canonical generation
/// path is published, so a successful authoritative reopen never exposes a generation without the
/// identity evidence required by shipping preflight.
// The exact tenant/profile/generation and current device/epoch are independent ownership
// witnesses; bundling them into an unvalidated request would obscure the trust boundary.
#[allow(clippy::too_many_arguments)]
pub fn ensure_authoritative_generation<C, D>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    device_id: &DeviceId,
    workspace_epoch: u64,
    control: &mut C,
    downloader: &mut D,
) -> Result<(), AuthoritativeGenerationMaterializationError>
where
    C: GenerationReopenControlPort,
    <C as GenerationReopenControlPort>::Error: Into<GenerationReopenControlError>,
    D: GenerationObjectDownloadPort,
    D::Error: Into<GenerationObjectDownloadError>,
{
    let incomplete_local = match root.open_generation(tenant_id, profile_id, generation_id) {
        Ok(workspace) => {
            match load_materialization_binding(&workspace, tenant_id, profile_id, generation_id) {
                Ok(binding) => {
                    if workspace
                        .materialization_inventory_digest()
                        .map_err(|_| AuthoritativeGenerationMaterializationError::Local)?
                        != binding.materialized_inventory_digest()
                    {
                        return Err(
                            AuthoritativeGenerationMaterializationError::InvalidLocalEvidence,
                        );
                    }
                    return Ok(());
                }
                Err(BrowserLaunchBlocker::LocalProfile(LocalProfileError::Io(
                    std::io::ErrorKind::NotFound,
                ))) => Some(workspace),
                Err(BrowserLaunchBlocker::LocalProfile(LocalProfileError::Io(
                    std::io::ErrorKind::PermissionDenied,
                ))) => {
                    return Err(AuthoritativeGenerationMaterializationError::LocalAccessDenied);
                }
                Err(BrowserLaunchBlocker::LocalProfile(_)) => {
                    return Err(AuthoritativeGenerationMaterializationError::Local);
                }
                Err(_) => {
                    return Err(AuthoritativeGenerationMaterializationError::InvalidLocalEvidence);
                }
            }
        }
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => None,
        Err(_) => return Err(AuthoritativeGenerationMaterializationError::Local),
    };

    let capability = control
        .download_capability(tenant_id, profile_id)
        .map_err(|error| {
            let reason = error.into();
            if incomplete_local.is_none() {
                // No workspace, lock, materialization or runtime path has been entered.
                // A second exact local lookup at the caller is still required for Clean.
                AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(reason)
            } else {
                // Existing incomplete/dirty bytes disallow a clean cancellation.
                AuthoritativeGenerationMaterializationError::ControlPlane(reason)
            }
        })?;
    // The exact selected workspace was observed missing before any remote read. No local
    // mutation takes place until materialize_workspace_snapshot_with_pre_publish below.
    // Existing incomplete/dirty materialization never receives a clean no-use witness.
    let prewrite_refusal = |reason: AuthoritativeGenerationPreWriteError| {
        if incomplete_local.is_none() {
            AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(reason)
        } else {
            match reason {
                AuthoritativeGenerationPreWriteError::DescriptorMismatch => {
                    AuthoritativeGenerationMaterializationError::DescriptorMismatch
                }
                AuthoritativeGenerationPreWriteError::Download(reason) => {
                    AuthoritativeGenerationMaterializationError::Download(reason)
                }
                AuthoritativeGenerationPreWriteError::MetadataMismatch => {
                    AuthoritativeGenerationMaterializationError::MetadataMismatch
                }
                AuthoritativeGenerationPreWriteError::OpeningControl(reason) => {
                    AuthoritativeGenerationMaterializationError::ControlPlane(reason)
                }
                AuthoritativeGenerationPreWriteError::OpeningMaterialMismatch => {
                    AuthoritativeGenerationMaterializationError::OpeningMaterialMismatch
                }
                AuthoritativeGenerationPreWriteError::Decryption => {
                    AuthoritativeGenerationMaterializationError::Decryption
                }
            }
        }
    };
    let expected_object_key = canonical_generation_object_key(tenant_id, profile_id, generation_id);
    if capability.generation_id() != generation_id
        || capability.object_key() != expected_object_key
        || capability.container_bytes() == 0
        || capability.expires_seconds() == 0
    {
        return Err(prewrite_refusal(
            AuthoritativeGenerationPreWriteError::DescriptorMismatch,
        ));
    }
    let source_container_sha256 = encode_sha256(capability.container_digest());

    // The sole authoritative reopen owner selects the encrypted spool path.
    // No caller controls the generation, object, digest, DEK or workspace.
    // Same descriptor yields one deterministic no-replace pending file;
    // interrupted/foreign evidence cannot silently be overwritten.
    let staged_ciphertext_path = root.path().join(format!(
        ".pending-r2-{}",
        encode_sha256(capability.container_digest())
    ));
    let stream = if incomplete_local.is_none() {
        downloader.download_generation_object_to_new_pending_file(
            &capability,
            &staged_ciphertext_path,
        )
    } else {
        Ok(None)
    }
    .map_err(|error| {
        prewrite_refusal(AuthoritativeGenerationPreWriteError::Download(error.into()))
    })?;
    if let Some(ciphertext) = stream {
        return materialize_streamed_authoritative_generation(
            root,
            tenant_id,
            profile_id,
            generation_id,
            control,
            &capability,
            ciphertext,
            &staged_ciphertext_path,
        );
    }

    let container = downloader
        .download_generation_object(&capability)
        .map_err(|error| {
            prewrite_refusal(AuthoritativeGenerationPreWriteError::Download(error.into()))
        })?;
    let inspected = inspect_generation_metadata_prelude(&container)
        .map_err(|_| prewrite_refusal(AuthoritativeGenerationPreWriteError::MetadataMismatch))?;
    let metadata = inspected.metadata();
    if metadata.tenant_id() != tenant_id
        || metadata.profile_id() != profile_id
        || metadata.generation_id() != generation_id
        || metadata.object_key() != expected_object_key
        || inspected.metadata_digest().bytes() != capability.metadata_digest()
    {
        return Err(prewrite_refusal(
            AuthoritativeGenerationPreWriteError::MetadataMismatch,
        ));
    }
    let metadata_prelude = container
        .get(..inspected.prelude_bytes())
        .ok_or_else(|| prewrite_refusal(AuthoritativeGenerationPreWriteError::MetadataMismatch))?;
    let opening_key = control
        .opening_material(tenant_id, profile_id, metadata_prelude)
        .map_err(|error| {
            prewrite_refusal(AuthoritativeGenerationPreWriteError::OpeningControl(
                error.into(),
            ))
        })?;
    if opening_key.key_id() != metadata.key_id() {
        return Err(prewrite_refusal(
            AuthoritativeGenerationPreWriteError::OpeningMaterialMismatch,
        ));
    }

    let opened = open_generation_expected(
        &container,
        &opening_key,
        tenant_id,
        profile_id,
        generation_id,
    )
    .map_err(|_| prewrite_refusal(AuthoritativeGenerationPreWriteError::Decryption))?;
    if let Some(workspace) = incomplete_local {
        // The caller holds the precise coordinator lease. Take this device's local lock
        // and compare ALL workspace bytes, including mutable user_data, with cloud truth.
        let lock = BridgeWorkspaceLock::acquire(&workspace, device_id, workspace_epoch)
            .map_err(|_| AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence)?;
        let restored = (|| {
            if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
                return Err(AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence);
            }
            verify_workspace_matches_snapshot(&workspace, opened.plaintext()).map_err(|_| {
                AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence
            })?;
            restore_materialization_binding_after_authoritative_open(
                &workspace,
                tenant_id,
                profile_id,
                generation_id,
                &source_container_sha256,
            )
            .map_err(|_| AuthoritativeGenerationMaterializationError::InvalidLocalEvidence)?;
            Ok(())
        })();
        let released = lock
            .release()
            .map_err(|_| AuthoritativeGenerationMaterializationError::InvalidLocalEvidence);
        restored?;
        released?;
        return Ok(());
    }
    materialize_workspace_snapshot_with_pre_publish(
        root,
        tenant_id,
        profile_id,
        generation_id,
        opened.plaintext(),
        |workspace| {
            restore_materialization_binding_after_authoritative_open(
                workspace,
                tenant_id,
                profile_id,
                generation_id,
                &source_container_sha256,
            )
            .map(|_| ())
            .map_err(|_| GenerationSnapshotError::PrePublish)
        },
    )
    .map_err(|error| match error {
        GenerationSnapshotError::Local(_) => AuthoritativeGenerationMaterializationError::Local,
        GenerationSnapshotError::PrePublish => {
            AuthoritativeGenerationMaterializationError::BrowserIdentity
        }
        _ => AuthoritativeGenerationMaterializationError::Snapshot,
    })?;
    Ok(())
}

/// Cold materialization only. A complete descriptor-verified ciphertext
/// exists, but neither a metadata prelude nor an AEAD chunk alone can publish
/// user_data. The decrypted BPGW file is an unpublished bounded scratch input;
/// the ONE existing snapshot owner performs full-file checks and no-replace
/// publication with generation-bound browser identity in pre-publish.
#[allow(clippy::too_many_arguments)]
fn materialize_streamed_authoritative_generation<C>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    control: &mut C,
    capability: &crate::generation_reopen::GenerationDownloadCapability,
    mut ciphertext: VerifiedGenerationObjectFile,
    ciphertext_path: &std::path::Path,
) -> Result<(), AuthoritativeGenerationMaterializationError>
where
    C: GenerationReopenControlPort,
    C::Error: Into<GenerationReopenControlError>,
{
    use AuthoritativeGenerationMaterializationError as Error;
    use AuthoritativeGenerationPreWriteError as PreWrite;
    let prelude = ciphertext
        .read_descriptor_verified_metadata_prelude()
        .map_err(|_| Error::ReadBeforeLocalUse(PreWrite::MetadataMismatch))?;
    let inspected = inspect_generation_metadata_prelude(&prelude)
        .map_err(|_| Error::ReadBeforeLocalUse(PreWrite::MetadataMismatch))?;
    let metadata = inspected.metadata();
    if metadata.tenant_id() != tenant_id
        || metadata.profile_id() != profile_id
        || metadata.generation_id() != generation_id
        || metadata.object_key()
            != canonical_generation_object_key(tenant_id, profile_id, generation_id)
        || inspected.metadata_digest().bytes() != capability.metadata_digest()
    {
        return Err(Error::ReadBeforeLocalUse(PreWrite::MetadataMismatch));
    }
    let key = control
        .opening_material(tenant_id, profile_id, &prelude)
        .map_err(|reason| Error::ReadBeforeLocalUse(PreWrite::OpeningControl(reason.into())))?;
    if key.key_id() != metadata.key_id() {
        return Err(Error::ReadBeforeLocalUse(PreWrite::OpeningMaterialMismatch));
    }
    let plaintext_path = root.path().join(format!(
        ".pending-bpgw-{}",
        encode_sha256(capability.container_digest())
    ));
    let mut plaintext = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&plaintext_path)
        .map_err(|_| Error::Local)?;
    let report = open_generation_streaming(ciphertext.reader(), &key, &mut plaintext)
        .map_err(|_| Error::ReadBeforeLocalUse(PreWrite::Decryption))?;
    if report.metadata() != metadata
        || report.metadata_digest().bytes() != capability.metadata_digest()
        || report.container_digest().bytes() != capability.container_digest()
        || report.container_bytes() != capability.container_bytes()
    {
        return Err(Error::ReadBeforeLocalUse(PreWrite::Decryption));
    }
    ciphertext
        .recheck_and_rewind()
        .map_err(|_| Error::ReadBeforeLocalUse(PreWrite::Decryption))?;
    plaintext.sync_all().map_err(|_| Error::Local)?;
    if plaintext
        .metadata()
        .map_err(|_| Error::Local)?
        .len()
        != metadata.plaintext_bytes()
    {
        return Err(Error::ReadBeforeLocalUse(PreWrite::Decryption));
    }
    plaintext.seek(SeekFrom::Start(0)).map_err(|_| Error::Local)?;
    let digest = encode_sha256(capability.container_digest());
    let outcome = materialize_workspace_snapshot_streaming_with_pre_publish(
        root,
        tenant_id,
        profile_id,
        generation_id,
        &mut plaintext,
        StreamedWorkspaceSnapshot {
            plaintext_bytes: metadata.plaintext_bytes(),
            plaintext_digest: metadata.plaintext_digest().bytes(),
        },
        |workspace| {
            restore_materialization_binding_after_authoritative_open(
                workspace,
                tenant_id,
                profile_id,
                generation_id,
                &digest,
            )
            .map(|_| ())
            .map_err(|_| GenerationSnapshotError::PrePublish)
        },
    )
    .map_err(|error| match error {
        GenerationSnapshotError::Local(_) => Error::Local,
        GenerationSnapshotError::PrePublish => Error::BrowserIdentity,
        _ => Error::Snapshot,
    });
    outcome?;
    // Only after the full AEAD tag + BPGW + browser binding are verified and
    // the canonical workspace is published may the two owned scratch files
    // be retired. Failure to remove one prevents a false Clean terminal.
    drop(plaintext);
    drop(ciphertext);
    fs::remove_file(&plaintext_path).map_err(|_| Error::Local)?;
    fs::remove_file(ciphertext_path).map_err(|_| Error::Local)?;
    Ok(())
}

fn encode_sha256(digest: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{
        AuthoritativeGenerationMaterializationError, AuthoritativeGenerationPreWriteError,
        encode_sha256, ensure_authoritative_generation,
    };
    use crate::browser_execution::{load_materialization_binding, persist_materialization_binding};
    use crate::generation_reopen::{
        GenerationDownloadCapability, GenerationObjectDownloadError, GenerationObjectDownloadPort,
        GenerationReopenControlError, GenerationReopenControlPort,
    };
    use crate::generation_snapshot::encode_workspace_snapshot;
    use crate::local_profile::{BROWSER_IDENTITY_RECORD_FILE, MaterializationRoot};
    use crate::test_support::browser_identity_fixture;
    use browser_execution_domain::MaterializationBinding;
    use encrypted_generation_domain::{
        GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
        canonical_generation_object_key, seal_generation,
    };
    use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct Fixture {
        root_path: PathBuf,
        root: MaterializationRoot,
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        key_id: KeyId,
        key_bytes: [u8; 32],
        container: Vec<u8>,
        metadata_digest: [u8; 32],
        container_digest: [u8; 32],
        browser_identity_record: Vec<u8>,
        config_bytes: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            Self::build(true)
        }

        fn without_browser_identity() -> Result<Self, Box<dyn std::error::Error>> {
            Self::build(false)
        }

        fn build(include_browser_identity: bool) -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-authoritative-generation-{}-{counter}",
                std::process::id()
            ));
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant_id = TenantId::parse(format!("tenant_authgen_{counter}"))?;
            let profile_id = ProfileId::parse(format!("profile_authgen_{counter}"))?;
            let generation_id = GenerationId::parse(format!("generation_authgen_{counter}"))?;
            let source_id = GenerationId::parse(format!("generation_authgen_source_{counter}"))?;
            let source = root.create_generation(&tenant_id, &profile_id, &source_id)?;
            fs::create_dir_all(source.path().join("user_data/storage"))?;
            fs::write(source.path().join("prefs.js"), b"authoritative-prefs")?;
            fs::write(
                source.path().join("user_data/storage/state.bin"),
                b"authoritative-state",
            )?;
            let config_bytes = b"{\"fixture\":\"canonical-camoufox-config\"}\n".to_vec();
            fs::write(source.path().join("camoufox-config.json"), &config_bytes)?;

            let browser_identity_record = if include_browser_identity {
                let config_digest: [u8; 32] = Sha256::digest(&config_bytes).into();
                let identity = browser_identity_fixture(
                    "2.0.0",
                    "a".repeat(64),
                    &format!("profile-stability-v1-probe-{}", "b".repeat(64)),
                    encode_sha256(config_digest),
                )?;
                let binding = MaterializationBinding::new(
                    tenant_id.clone(),
                    profile_id.clone(),
                    source_id.clone(),
                    "f".repeat(64),
                    source.materialization_inventory_digest()?,
                    identity,
                )?;
                persist_materialization_binding(&source, &binding)?;
                fs::read(source.path().join(BROWSER_IDENTITY_RECORD_FILE))?
            } else {
                Vec::new()
            };

            let inventory = source.inventory()?;
            let snapshot = encode_workspace_snapshot(&source, &inventory)?;
            let key_id = KeyId::parse("profile-generation-root-v1-7")?;
            let key_bytes = [0x37; 32];
            let key = GenerationDek::new(key_id.clone(), key_bytes);
            let metadata = GenerationMetadata::for_plaintext(
                GenerationIdentity::new(
                    tenant_id.clone(),
                    profile_id.clone(),
                    generation_id.clone(),
                    Some(source_id),
                ),
                key_id.clone(),
                NoncePrefix::new([0x71; 16]),
                4096,
                &snapshot,
            )?;
            let sealed = seal_generation(&metadata, &key, &snapshot)?;
            let metadata_digest = sealed.metadata_digest().bytes();
            let container_digest = sealed.container_digest().bytes();
            let container = sealed.into_container();
            Ok(Self {
                root_path,
                root,
                tenant_id,
                profile_id,
                generation_id,
                key_id,
                key_bytes,
                container,
                metadata_digest,
                container_digest,
                browser_identity_record,
                config_bytes,
            })
        }

        fn control(&self) -> FakeControl {
            FakeControl {
                tenant_id: self.tenant_id.clone(),
                profile_id: self.profile_id.clone(),
                generation_id: self.generation_id.clone(),
                object_key: canonical_generation_object_key(
                    &self.tenant_id,
                    &self.profile_id,
                    &self.generation_id,
                ),
                metadata_digest: self.metadata_digest,
                container_digest: self.container_digest,
                container_bytes: u64::try_from(self.container.len()).unwrap_or(0),
                key_id: self.key_id.clone(),
                key_bytes: self.key_bytes,
                opening_calls: 0,
                capability_error: None,
                opening_error: None,
            }
        }

        fn cleanup(&self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    struct FakeControl {
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        object_key: String,
        metadata_digest: [u8; 32],
        container_digest: [u8; 32],
        container_bytes: u64,
        key_id: KeyId,
        key_bytes: [u8; 32],
        opening_calls: u64,
        capability_error: Option<GenerationReopenControlError>,
        opening_error: Option<GenerationReopenControlError>,
    }

    impl GenerationReopenControlPort for FakeControl {
        type Error = GenerationReopenControlError;

        fn download_capability(
            &mut self,
            tenant_id: &TenantId,
            profile_id: &ProfileId,
        ) -> Result<GenerationDownloadCapability, Self::Error> {
            if let Some(reason) = self.capability_error {
                return Err(reason);
            }
            if tenant_id != &self.tenant_id || profile_id != &self.profile_id {
                return Err(GenerationReopenControlError::InvalidResponse);
            }
            Ok(GenerationDownloadCapability::new(
                self.generation_id.clone(),
                self.object_key.clone(),
                self.metadata_digest,
                self.container_digest,
                self.container_bytes,
                "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/bucket/object?X-Amz-Signature=TEST".to_owned(),
                300,
            ))
        }

        fn opening_material(
            &mut self,
            tenant_id: &TenantId,
            profile_id: &ProfileId,
            _metadata_prelude: &[u8],
        ) -> Result<GenerationDek, Self::Error> {
            if let Some(reason) = self.opening_error {
                return Err(reason);
            }
            if tenant_id != &self.tenant_id || profile_id != &self.profile_id {
                return Err(GenerationReopenControlError::InvalidResponse);
            }
            self.opening_calls += 1;
            Ok(GenerationDek::new(self.key_id.clone(), self.key_bytes))
        }
    }

    struct FakeDownloader {
        container: Vec<u8>,
        calls: u64,
    }

    impl GenerationObjectDownloadPort for FakeDownloader {
        type Error = GenerationObjectDownloadError;

        fn download_generation_object(
            &mut self,
            capability: &GenerationDownloadCapability,
        ) -> Result<Vec<u8>, Self::Error> {
            let digest: [u8; 32] = Sha256::digest(&self.container).into();
            if digest != capability.container_digest()
                || u64::try_from(self.container.len()).ok() != Some(capability.container_bytes())
            {
                return Err(GenerationObjectDownloadError::DigestMismatch);
            }
            self.calls += 1;
            Ok(self.container.clone())
        }
    }

    /// Exercise the real owner through the additive file path, never the
    /// legacy Vec download. The source is a previously sealed BPGC container.
    struct FakeStreamDownloader {
        container: Vec<u8>,
        calls: u64,
    }

    impl GenerationObjectDownloadPort for FakeStreamDownloader {
        type Error = GenerationObjectDownloadError;

        fn download_generation_object(
            &mut self,
            _capability: &GenerationDownloadCapability,
        ) -> Result<Vec<u8>, Self::Error> {
            Err(GenerationObjectDownloadError::InvalidCapability)
        }

        fn download_generation_object_to_new_pending_file(
            &mut self,
            capability: &GenerationDownloadCapability,
            destination: &std::path::Path,
        ) -> Result<Option<crate::generation_object_spool::VerifiedGenerationObjectFile>, Self::Error>
        {
            if u64::try_from(self.container.len()).ok() != Some(capability.container_bytes())
                || <[u8; 32]>::from(Sha256::digest(&self.container))
                    != capability.container_digest()
            {
                return Err(GenerationObjectDownloadError::DigestMismatch);
            }
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .map_err(|_| GenerationObjectDownloadError::Transport)?;
            output
                .write_all(&self.container)
                .map_err(|_| GenerationObjectDownloadError::Transport)?;
            output
                .sync_all()
                .map_err(|_| GenerationObjectDownloadError::Transport)?;
            drop(output);
            self.calls += 1;
            let verified = crate::generation_object_spool::VerifiedGenerationObjectFile::open(
                destination,
                capability.container_bytes(),
                capability.container_digest(),
            )
            .map_err(|_| GenerationObjectDownloadError::DigestMismatch)?;
            Ok(Some(verified))
        }
    }

    #[test]
    fn cold_streamed_reopen_authenticates_before_materialization_and_reuses_binding()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        let mut downloader = FakeStreamDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        let device = DeviceId::parse("device_authgen_streamed")?;
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &device,
            3,
            &mut control,
            &mut downloader,
        )?;
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &device,
            4,
            &mut control,
            &mut downloader,
        )?;
        assert_eq!(downloader.calls, 1);
        assert_eq!(control.opening_calls, 1);
        let materialized = fixture.root.open_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        assert_eq!(
            fs::read(materialized.path().join("user_data/storage/state.bin"))?,
            b"authoritative-state"
        );
        assert_eq!(
            fs::read(materialized.path().join(BROWSER_IDENTITY_RECORD_FILE))?,
            fixture.browser_identity_record
        );
        assert!(load_materialization_binding(
            &materialized,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id
        ).is_ok());
        let digest = encode_sha256(fixture.container_digest);
        assert!(!fixture.root.path().join(format!(".pending-r2-{digest}")).exists());
        assert!(!fixture.root.path().join(format!(".pending-bpgw-{digest}")).exists());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn cold_streamed_reopen_rejects_tampered_aead_without_publishing_browser_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut body = fixture.container.clone();
        let last = body.len() - 1;
        body[last] ^= 0x41;
        let mut control = fixture.control();
        control.container_digest = Sha256::digest(&body).into();
        let mut downloader = FakeStreamDownloader {
            container: body,
            calls: 0,
        };
        let outcome = ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &DeviceId::parse("device_authgen_streamed_reject")?,
            3,
            &mut control,
            &mut downloader,
        );
        assert_eq!(
            outcome,
            Err(AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                AuthoritativeGenerationPreWriteError::Decryption
            ))
        );
        assert_eq!(control.opening_calls, 1);
        assert_eq!(downloader.calls, 1);
        assert!(matches!(
            fixture.root.open_generation(
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id
            ),
            Err(crate::local_profile::LocalProfileError::Io(std::io::ErrorKind::NotFound))
        ));
        assert!(fixture.root.path().join(format!(
            ".pending-r2-{}",
            encode_sha256(control.container_digest)
        )).exists());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn object_get_failure_before_any_workspace_keeps_typed_reason_and_no_use()
    -> Result<(), Box<dyn std::error::Error>> {
        struct DisconnectedObjectGet(GenerationObjectDownloadError);
        impl GenerationObjectDownloadPort for DisconnectedObjectGet {
            type Error = GenerationObjectDownloadError;
            fn download_generation_object(
                &mut self,
                _: &GenerationDownloadCapability,
            ) -> Result<Vec<u8>, Self::Error> {
                Err(self.0)
            }
        }
        for reason in [
            GenerationObjectDownloadError::Transport,
            GenerationObjectDownloadError::HttpStatus,
            GenerationObjectDownloadError::SizeMismatch,
            GenerationObjectDownloadError::DigestMismatch,
            GenerationObjectDownloadError::InvalidCapability,
        ] {
            let fixture = Fixture::new()?;
            let mut control = fixture.control();
            let error = ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_read_only_get")?,
                1,
                &mut control,
                &mut DisconnectedObjectGet(reason),
            )
            .err()
            .ok_or_else(|| std::io::Error::other("object GET failed before local write"))?;
            assert_eq!(
                error,
                AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    AuthoritativeGenerationPreWriteError::Download(reason)
                )
            );
            assert!(error.proves_no_local_write_and_absent_workspace());
            assert!(matches!(
                fixture.root.open_generation(
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &fixture.generation_id,
                ),
                Err(crate::local_profile::LocalProfileError::Io(
                    std::io::ErrorKind::NotFound
                ))
            ));
            assert_eq!(control.opening_calls, 0);
            fixture.cleanup();
        }
        Ok(())
    }

    #[test]
    fn object_get_failure_with_existing_workspace_never_claims_clean_no_use()
    -> Result<(), Box<dyn std::error::Error>> {
        struct DisconnectedObjectGet;
        impl GenerationObjectDownloadPort for DisconnectedObjectGet {
            type Error = GenerationObjectDownloadError;
            fn download_generation_object(
                &mut self,
                _: &GenerationDownloadCapability,
            ) -> Result<Vec<u8>, Self::Error> {
                Err(GenerationObjectDownloadError::Transport)
            }
        }
        let fixture = Fixture::new()?;
        let _incomplete = fixture.root.create_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let mut control = fixture.control();
        let error = ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &DeviceId::parse("device_incomplete_object_get")?,
            1,
            &mut control,
            &mut DisconnectedObjectGet,
        )
        .err()
        .ok_or_else(|| std::io::Error::other("incomplete workspace retained"))?;
        assert_eq!(
            error,
            AuthoritativeGenerationMaterializationError::Download(
                GenerationObjectDownloadError::Transport
            )
        );
        assert!(!error.proves_no_local_write_and_absent_workspace());
        assert_eq!(control.opening_calls, 0);
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn all_read_only_verification_failures_keep_exact_no_use_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        // These are distinct failure paths before the first workspace or writer exists.
        // A clean coordinator cancellation must never admit any of the invalid objects.
        for (name, expected) in [
            (
                "wrong_key_id",
                AuthoritativeGenerationPreWriteError::OpeningMaterialMismatch,
            ),
            (
                "bad_decryption_key",
                AuthoritativeGenerationPreWriteError::Decryption,
            ),
            (
                "opening_http_refusal",
                AuthoritativeGenerationPreWriteError::OpeningControl(
                    GenerationReopenControlError::HttpStatus,
                ),
            ),
        ] {
            let fixture = Fixture::new()?;
            let mut control = fixture.control();
            match name {
                "wrong_key_id" => {
                    control.key_id = KeyId::parse("profile-generation-root-v1-8")?;
                }
                "bad_decryption_key" => control.key_bytes[0] ^= 0x01,
                "opening_http_refusal" => {
                    control.opening_error = Some(GenerationReopenControlError::HttpStatus);
                }
                _ => unreachable!("closed test fixture name"),
            }
            let mut downloader = FakeDownloader {
                container: fixture.container.clone(),
                calls: 0,
            };
            let refusal = ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_prewrite_verify_failure")?,
                1,
                &mut control,
                &mut downloader,
            )
            .err()
            .ok_or_else(|| std::io::Error::other("invalid generation cannot publish"))?;
            assert_eq!(
                refusal,
                AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(expected),
            );
            assert!(refusal.proves_no_local_write_and_absent_workspace());
            assert!(matches!(
                fixture.root.open_generation(
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &fixture.generation_id,
                ),
                Err(crate::local_profile::LocalProfileError::Io(
                    std::io::ErrorKind::NotFound
                ))
            ));
            assert_eq!(downloader.calls, 1);
            fixture.cleanup();
        }
        Ok(())
    }

    #[test]
    fn malformed_object_bytes_before_workspace_do_not_publish_or_request_key()
    -> Result<(), Box<dyn std::error::Error>> {
        // The verifier is an independent narrow owner. A malformed object in a
        // unit-injected raw port is never mistaken for locally dirty data or accepted.
        struct MalformedObjectGet;
        impl GenerationObjectDownloadPort for MalformedObjectGet {
            type Error = GenerationObjectDownloadError;
            fn download_generation_object(
                &mut self,
                _: &GenerationDownloadCapability,
            ) -> Result<Vec<u8>, Self::Error> {
                Ok(vec![0; 24])
            }
        }
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        let refusal = ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &DeviceId::parse("device_malformed_object_read")?,
            1,
            &mut control,
            &mut MalformedObjectGet,
        )
        .err()
        .ok_or_else(|| {
            std::io::Error::other("malformed encrypted object is rejected before publication")
        })?;
        assert_eq!(
            refusal,
            AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                AuthoritativeGenerationPreWriteError::MetadataMismatch,
            )
        );
        assert_eq!(control.opening_calls, 0);
        assert!(refusal.proves_no_local_write_and_absent_workspace());
        assert!(matches!(
            fixture.root.open_generation(
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
            ),
            Err(crate::local_profile::LocalProfileError::Io(
                std::io::ErrorKind::NotFound
            ))
        ));
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn remote_read_refusal_distinguishes_absent_and_incomplete_local_evidence()
    -> Result<(), Box<dyn std::error::Error>> {
        for reason in [
            GenerationReopenControlError::Transport,
            GenerationReopenControlError::HttpStatus,
            GenerationReopenControlError::InvalidResponse,
        ] {
            let fixture = Fixture::new()?;
            let mut control = fixture.control();
            control.capability_error = Some(reason);
            let mut downloader = FakeDownloader {
                container: fixture.container.clone(),
                calls: 0,
            };
            let refusal = ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_remote_read_failure")?,
                1,
                &mut control,
                &mut downloader,
            );
            assert_eq!(
                refusal,
                Err(
                    AuthoritativeGenerationMaterializationError::ControlPlaneBeforeLocalUse(reason)
                )
            );
            assert!(matches!(
                fixture.root.open_generation(
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &fixture.generation_id,
                ),
                Err(crate::local_profile::LocalProfileError::Io(
                    std::io::ErrorKind::NotFound
                ))
            ));
            assert_eq!(downloader.calls, 0);
            assert_eq!(control.opening_calls, 0);
            fixture.cleanup();
        }

        let fixture = Fixture::new()?;
        let workspace = fixture.root.create_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let mut control = fixture.control();
        control.capability_error = Some(GenerationReopenControlError::HttpStatus);
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_local_evidence_failure")?,
                1,
                &mut control,
                &mut downloader,
            ),
            Err(AuthoritativeGenerationMaterializationError::ControlPlane(
                GenerationReopenControlError::HttpStatus
            ))
        );
        assert!(workspace.path().exists());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn opening_material_invalid_response_is_typed_and_preserves_no_use_without_workspace()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        control.opening_error = Some(GenerationReopenControlError::InvalidResponse);
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_opening_reject")?,
                1,
                &mut control,
                &mut downloader,
            ),
            Err(
                AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    AuthoritativeGenerationPreWriteError::OpeningControl(
                        GenerationReopenControlError::InvalidResponse
                    )
                )
            )
        );
        assert_eq!(downloader.calls, 1);
        assert_eq!(control.opening_calls, 0);
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn directory_without_binding_is_not_reusable_and_is_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let workspace = fixture.root.create_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        fs::write(
            workspace.path().join(BROWSER_IDENTITY_RECORD_FILE),
            &fixture.browser_identity_record,
        )?;
        fs::write(
            workspace.path().join("camoufox-config.json"),
            &fixture.config_bytes,
        )?;
        fs::create_dir_all(workspace.path().join("user_data/storage"))?;
        fs::write(
            workspace.path().join("user_data/storage/state.bin"),
            b"retain-me",
        )?;
        let mut control = fixture.control();
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_authgen_test")?,
                1,
                &mut control,
                &mut downloader,
            ),
            Err(AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence)
        );
        assert_eq!(downloader.calls, 1);
        assert_eq!(control.opening_calls, 1);
        assert_eq!(
            fs::read(workspace.path().join("user_data/storage/state.bin"))?,
            b"retain-me"
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn exact_verified_cloud_bytes_recover_missing_sidecar_then_reuse_cache()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let workspace = fixture.root.create_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        fs::write(
            workspace.path().join(BROWSER_IDENTITY_RECORD_FILE),
            &fixture.browser_identity_record,
        )?;
        fs::write(
            workspace.path().join("camoufox-config.json"),
            &fixture.config_bytes,
        )?;
        fs::write(workspace.path().join("prefs.js"), b"authoritative-prefs")?;
        fs::create_dir_all(workspace.path().join("user_data/storage"))?;
        fs::write(
            workspace.path().join("user_data/storage/state.bin"),
            b"authoritative-state",
        )?;
        let mut control = fixture.control();
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        for _ in 0..2 {
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_authgen_test")?,
                1,
                &mut control,
                &mut downloader,
            )?;
        }
        assert_eq!(downloader.calls, 1);
        assert_eq!(control.opening_calls, 1);
        assert_eq!(
            load_materialization_binding(
                &workspace,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
            )?
            .source_container_sha256(),
            encode_sha256(fixture.container_digest)
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn corrupt_existing_binding_cannot_enable_reuse_or_delete_data()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let workspace = fixture.root.create_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        fs::write(
            workspace.path().join(BROWSER_IDENTITY_RECORD_FILE),
            &fixture.browser_identity_record,
        )?;
        let sidecar = workspace
            .path()
            .parent()
            .ok_or("missing parent")?
            .join(format!(
                ".{}.materialization",
                fixture.generation_id.as_str()
            ));
        fs::write(&sidecar, b"invalid-evidence")?;
        let mut control = fixture.control();
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_authgen_test")?,
                1,
                &mut control,
                &mut downloader,
            ),
            Err(AuthoritativeGenerationMaterializationError::InvalidLocalEvidence)
        );
        assert_eq!(downloader.calls, 0);
        assert!(workspace.path().exists());
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn missing_local_authority_restores_same_browser_identity_before_publish()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &DeviceId::parse("device_authgen_test")?,
            1,
            &mut control,
            &mut downloader,
        )?;
        // The next open selects this same server generation and must use its local materialization.
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &DeviceId::parse("device_authgen_test")?,
            1,
            &mut control,
            &mut downloader,
        )?;
        assert_eq!(downloader.calls, 1);
        assert_eq!(control.opening_calls, 1);
        let workspace = fixture.root.open_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        assert_eq!(
            fs::read(workspace.path().join("prefs.js"))?,
            b"authoritative-prefs"
        );
        assert_eq!(
            fs::read(workspace.path().join("user_data/storage/state.bin"))?,
            b"authoritative-state"
        );
        let restored_config = fs::read(workspace.path().join("camoufox-config.json"))?;
        assert_eq!(restored_config.as_slice(), fixture.config_bytes.as_slice());
        let restored_identity = fs::read(workspace.path().join(BROWSER_IDENTITY_RECORD_FILE))?;
        assert_eq!(
            restored_identity.as_slice(),
            fixture.browser_identity_record.as_slice()
        );
        let binding = load_materialization_binding(
            &workspace,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        assert_eq!(
            binding.source_container_sha256(),
            encode_sha256(fixture.container_digest)
        );
        let binding_identity = binding.browser_identity().to_canonical_record();
        assert_eq!(
            binding_identity.as_bytes(),
            fixture.browser_identity_record.as_slice()
        );
        assert_eq!(control.opening_calls, 1);
        assert_eq!(downloader.calls, 1);
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn missing_portable_browser_identity_never_publishes_authoritative_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::without_browser_identity()?;
        let mut control = fixture.control();
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_authgen_test")?,
                1,
                &mut control,
                &mut downloader,
            ),
            Err(AuthoritativeGenerationMaterializationError::BrowserIdentity)
        );
        assert!(
            fixture
                .root
                .open_generation(
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &fixture.generation_id,
                )
                .is_err()
        );
        assert_eq!(control.opening_calls, 1);
        assert_eq!(downloader.calls, 1);
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn metadata_mismatch_fails_before_opening_material_is_issued()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        control.metadata_digest = [0xaa; 32];
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_authgen_test")?,
                1,
                &mut control,
                &mut downloader,
            ),
            Err(
                AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    AuthoritativeGenerationPreWriteError::MetadataMismatch,
                )
            )
        );
        assert_eq!(control.opening_calls, 0);
        assert!(
            fixture
                .root
                .open_generation(
                    &fixture.tenant_id,
                    &fixture.profile_id,
                    &fixture.generation_id,
                )
                .is_err()
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn stale_server_descriptor_never_falls_back_or_requests_a_key()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        control.generation_id = GenerationId::parse("generation_stale_authority")?;
        let mut downloader = FakeDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &DeviceId::parse("device_authgen_test")?,
                1,
                &mut control,
                &mut downloader,
            ),
            Err(
                AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    AuthoritativeGenerationPreWriteError::DescriptorMismatch,
                )
            )
        );
        assert_eq!(downloader.calls, 0);
        assert_eq!(control.opening_calls, 0);
        fixture.cleanup();
        Ok(())
    }
}
