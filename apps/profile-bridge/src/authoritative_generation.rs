use crate::browser_execution::{
    BrowserLaunchBlocker, browser_writer_locks_absent, load_materialization_binding,
    restore_materialization_binding_after_authoritative_open,
};
use crate::generation_object_spool::VerifiedGenerationObjectFile;
use crate::generation_reopen::{
    GenerationObjectDownloadError, GenerationObjectDownloadPort, GenerationReopenControlError,
    GenerationReopenControlPort,
};
use crate::generation_snapshot::{
    GenerationSnapshotError, StreamedWorkspaceSnapshot,
    materialize_profile_current_snapshot_streaming_with_pre_publish,
    materialize_profile_current_snapshot_with_pre_publish,
    materialize_workspace_snapshot_streaming_with_pre_publish,
    materialize_workspace_snapshot_with_pre_publish, verify_workspace_matches_snapshot,
    verify_workspace_matches_streamed_snapshot,
};
use crate::local_profile::{
    BridgeWorkspaceLock, GenerationWorkspace, LocalProfileError, MaterializationRoot,
};
use encrypted_generation_domain::{
    canonical_generation_object_key, inspect_generation_metadata_prelude, open_generation_expected,
    open_generation_streaming,
};
use profile_platform_primitives::{DeviceId, GenerationId, ProfileId, TenantId};
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom};

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
    ensure_authoritative_generation_to(
        root,
        tenant_id,
        profile_id,
        generation_id,
        device_id,
        workspace_epoch,
        control,
        downloader,
        None,
        false,
        None,
    )
}

/// Restore only a Worker-selected, exactly verified CURRENT into the existing
/// stable local destination. This is the same decrypt/AEAD/BPGW/binding owner
/// as historical materialization; no alternate snapshot format or engine.
#[allow(clippy::too_many_arguments)]
pub fn ensure_authoritative_current<C, D>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    profile_version: u64,
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
    if profile_version == 0 {
        return Err(AuthoritativeGenerationMaterializationError::InvalidLocalEvidence);
    }
    ensure_authoritative_generation_to(
        root,
        tenant_id,
        profile_id,
        generation_id,
        device_id,
        workspace_epoch,
        control,
        downloader,
        Some(profile_version),
        false,
        None,
    )
}

/// Reconcile a lost first-DRAFT Commit reply without an extra cloud state
/// or a second snapshot owner. The Worker still selects ONLY VERIFIED CURRENT,
/// the existing R2/AEAD/BPGW reader proves full byte equality under the local
/// OS writer lock, and the original working files are never replaced.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_closed_draft_with_confirmed_current<C, D>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    profile_version: u64,
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
    if profile_version == 0 {
        return Err(AuthoritativeGenerationMaterializationError::InvalidLocalEvidence);
    }
    ensure_authoritative_generation_to(
        root,
        tenant_id,
        profile_id,
        generation_id,
        device_id,
        workspace_epoch,
        control,
        downloader,
        Some(profile_version),
        true,
        None,
    )
}

/// Read-only recovery for an already-committed READY N+1 whose HTTP
/// receipt was lost. The verified D1 CURRENT and decrypted exact BPGW must
/// match the ORIGINAL closed local browser bytes and old confirmed base.
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn reconcile_closed_ready_with_confirmed_current<C, D>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    previous_base: &GenerationId,
    profile_version: u64,
    device_id: &DeviceId,
    workspace_epoch: u64,
    control: &mut C,
    downloader: &mut D,
) -> Result<(), AuthoritativeGenerationMaterializationError>
where
    C: GenerationReopenControlPort,
    C::Error: Into<GenerationReopenControlError>,
    D: GenerationObjectDownloadPort,
    D::Error: Into<GenerationObjectDownloadError>,
{
    if profile_version <= 1 || generation_id == previous_base {
        return Err(AuthoritativeGenerationMaterializationError::InvalidLocalEvidence);
    }
    ensure_authoritative_generation_to(
        root,
        tenant_id,
        profile_id,
        generation_id,
        device_id,
        workspace_epoch,
        control,
        downloader,
        Some(profile_version),
        true,
        Some(previous_base),
    )
}

#[allow(clippy::too_many_arguments)]
fn ensure_authoritative_generation_to<C, D>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    device_id: &DeviceId,
    workspace_epoch: u64,
    control: &mut C,
    downloader: &mut D,
    profile_version: Option<u64>,
    reconcile_existing: bool,
    expected_previous_base: Option<&GenerationId>,
) -> Result<(), AuthoritativeGenerationMaterializationError>
where
    C: GenerationReopenControlPort,
    <C as GenerationReopenControlPort>::Error: Into<GenerationReopenControlError>,
    D: GenerationObjectDownloadPort,
    D::Error: Into<GenerationObjectDownloadError>,
{
    #[cfg(windows)]
    let local_evidence_started = std::time::Instant::now();
    // Restore must never interpret an existing local CURRENT as an old cloud
    // generation or repair/replace potentially dirty native Firefox files.
    let incomplete_local = if let Some(version) = profile_version {
        match root.open_profile_current(tenant_id, profile_id) {
            Ok(workspace)
                if reconcile_existing
                    && workspace.base_generation_id() == expected_previous_base
                    && (match expected_previous_base {
                        Some(base) => workspace.generation_id() == base && generation_id != base,
                        None => workspace.generation_id() == generation_id,
                    })
                    && workspace.profile_version().checked_add(1) == Some(version) =>
            {
                Some(workspace)
            }
            Ok(_) => return Err(AuthoritativeGenerationMaterializationError::InvalidLocalEvidence),
            Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) if !reconcile_existing => None,
            Err(_) => return Err(AuthoritativeGenerationMaterializationError::Local),
        }
    } else {
        match root.open_generation(tenant_id, profile_id, generation_id) {
            Ok(workspace) => {
                match load_materialization_binding(&workspace, tenant_id, profile_id, generation_id)
                {
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
                        #[cfg(windows)]
                        eprintln!(
                            "bridge_open_subphase name=ready_local_reuse elapsed_ms={}",
                            local_evidence_started.elapsed().as_millis()
                        );
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
                        return Err(
                            AuthoritativeGenerationMaterializationError::InvalidLocalEvidence,
                        );
                    }
                }
            }
            Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => None,
            Err(_) => return Err(AuthoritativeGenerationMaterializationError::Local),
        }
    };

    #[cfg(windows)]
    eprintln!(
        "bridge_open_subphase name=cloud_materialization_needed elapsed_ms={}",
        local_evidence_started.elapsed().as_millis()
    );
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
    // Stream the exact ciphertext also when the existing local workspace lacks
    // only its host-local binding. Legacy Vec is reserved for non-stream ports.
    let stream = downloader
        .download_generation_object_to_new_pending_file(&capability, &staged_ciphertext_path)
        .map_err(|error| {
            prewrite_refusal(AuthoritativeGenerationPreWriteError::Download(error.into()))
        })?;
    if let Some(ciphertext) = stream {
        return materialize_streamed_authoritative_generation(
            root,
            tenant_id,
            profile_id,
            generation_id,
            device_id,
            workspace_epoch,
            control,
            &capability,
            ciphertext,
            &staged_ciphertext_path,
            incomplete_local,
            profile_version,
            expected_previous_base,
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
        || expected_previous_base.is_some_and(|old| metadata.base_generation_id() != Some(old))
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
        // The caller supplies the current same-device admission epoch. Take this
        // device's local lock and compare ALL workspace bytes, including mutable
        // user_data, with cloud truth.
        let lock = BridgeWorkspaceLock::acquire_or_recover_interrupted(
            &workspace,
            device_id,
            workspace_epoch,
        )
        .map_err(|_| AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence)?;
        let restored = (|| {
            if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
                return Err(AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence);
            }
            verify_workspace_matches_snapshot(&workspace, opened.plaintext()).map_err(|_| {
                AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence
            })?;
            if let Some(version) = profile_version {
                confirm_closed_current_matches_cloud(
                    root,
                    &workspace,
                    generation_id,
                    version,
                    expected_previous_base,
                )
                .map_err(|_| AuthoritativeGenerationMaterializationError::InvalidLocalEvidence)?;
            } else {
                restore_materialization_binding_after_authoritative_open(
                    &workspace,
                    tenant_id,
                    profile_id,
                    generation_id,
                    &source_container_sha256,
                )
                .map_err(|_| AuthoritativeGenerationMaterializationError::InvalidLocalEvidence)?;
            }
            Ok(())
        })();
        let released = lock
            .release()
            .map_err(|_| AuthoritativeGenerationMaterializationError::InvalidLocalEvidence);
        restored?;
        released?;
        return Ok(());
    }
    let bind = |workspace: &GenerationWorkspace| {
        if profile_version.is_some() {
            return require_native_camoufox_config(workspace);
        }
        restore_materialization_binding_after_authoritative_open(
            workspace,
            tenant_id,
            profile_id,
            generation_id,
            &source_container_sha256,
        )
        .map(|_| ())
        .map_err(|_| GenerationSnapshotError::PrePublish)
    };
    let published = match profile_version {
        Some(version) => materialize_profile_current_snapshot_with_pre_publish(
            root,
            tenant_id,
            profile_id,
            generation_id,
            version,
            opened.plaintext(),
            bind,
        ),
        None => materialize_workspace_snapshot_with_pre_publish(
            root,
            tenant_id,
            profile_id,
            generation_id,
            opened.plaintext(),
            bind,
        ),
    };
    published.map_err(|error| match error {
        GenerationSnapshotError::Local(_) => AuthoritativeGenerationMaterializationError::Local,
        GenerationSnapshotError::PrePublish => {
            AuthoritativeGenerationMaterializationError::BrowserIdentity
        }
        _ => AuthoritativeGenerationMaterializationError::Snapshot,
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn materialize_streamed_authoritative_generation<C>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    device_id: &DeviceId,
    workspace_epoch: u64,
    control: &mut C,
    capability: &crate::generation_reopen::GenerationDownloadCapability,
    mut ciphertext: VerifiedGenerationObjectFile,
    ciphertext_path: &std::path::Path,
    incomplete_local: Option<GenerationWorkspace>,
    profile_version: Option<u64>,
    expected_previous_base: Option<&GenerationId>,
) -> Result<(), AuthoritativeGenerationMaterializationError>
where
    C: GenerationReopenControlPort,
    C::Error: Into<GenerationReopenControlError>,
{
    use AuthoritativeGenerationMaterializationError as Error;
    use AuthoritativeGenerationPreWriteError as PreWrite;
    // Do not return an "absent local generation" clean-release witness when
    // Firefox files already exist but the host-only materialization sidecar
    // is missing. The ordinary Vec fallback already enforces this boundary.
    let prewrite = |reason: PreWrite| {
        if incomplete_local.is_none() {
            Error::ReadBeforeLocalUse(reason)
        } else {
            match reason {
                PreWrite::DescriptorMismatch => Error::DescriptorMismatch,
                PreWrite::Download(error) => Error::Download(error),
                PreWrite::MetadataMismatch => Error::MetadataMismatch,
                PreWrite::OpeningControl(error) => Error::ControlPlane(error),
                PreWrite::OpeningMaterialMismatch => Error::OpeningMaterialMismatch,
                PreWrite::Decryption => Error::Decryption,
            }
        }
    };
    let prelude = ciphertext
        .read_descriptor_verified_metadata_prelude()
        .map_err(|_| prewrite(PreWrite::MetadataMismatch))?;
    let inspected = inspect_generation_metadata_prelude(&prelude)
        .map_err(|_| prewrite(PreWrite::MetadataMismatch))?;
    let metadata = inspected.metadata();
    if metadata.tenant_id() != tenant_id
        || metadata.profile_id() != profile_id
        || metadata.generation_id() != generation_id
        || metadata.object_key()
            != canonical_generation_object_key(tenant_id, profile_id, generation_id)
        || inspected.metadata_digest().bytes() != capability.metadata_digest()
        || expected_previous_base.is_some_and(|old| metadata.base_generation_id() != Some(old))
    {
        return Err(prewrite(PreWrite::MetadataMismatch));
    }
    let key = control
        .opening_material(tenant_id, profile_id, &prelude)
        .map_err(|reason| prewrite(PreWrite::OpeningControl(reason.into())))?;
    if key.key_id() != metadata.key_id() {
        return Err(prewrite(PreWrite::OpeningMaterialMismatch));
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
        .map_err(|_| prewrite(PreWrite::Decryption))?;
    if report.metadata() != metadata
        || report.metadata_digest().bytes() != capability.metadata_digest()
        || report.container_digest().bytes() != capability.container_digest()
        || report.container_bytes() != capability.container_bytes()
    {
        return Err(prewrite(PreWrite::Decryption));
    }
    ciphertext
        .recheck_and_rewind()
        .map_err(|_| prewrite(PreWrite::Decryption))?;
    plaintext.sync_all().map_err(|_| Error::Local)?;
    if plaintext.metadata().map_err(|_| Error::Local)?.len() != metadata.plaintext_bytes() {
        return Err(prewrite(PreWrite::Decryption));
    }
    plaintext
        .seek(SeekFrom::Start(0))
        .map_err(|_| Error::Local)?;
    let digest = encode_sha256(capability.container_digest());
    if let Some(workspace) = incomplete_local {
        // No original native bytes may be replaced. Recover just the sidecar
        // after exact decrypted BPGW equality under the existing OS writer lock.
        let lock = BridgeWorkspaceLock::acquire_or_recover_interrupted(
            &workspace,
            device_id,
            workspace_epoch,
        )
        .map_err(|_| Error::IncompleteLocalEvidence)?;
        let verified = (|| {
            if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
                return Err(Error::IncompleteLocalEvidence);
            }
            verify_workspace_matches_streamed_snapshot(
                &workspace,
                &mut plaintext,
                StreamedWorkspaceSnapshot {
                    plaintext_bytes: metadata.plaintext_bytes(),
                    plaintext_digest: metadata.plaintext_digest().bytes(),
                },
            )
            .map_err(|_| Error::IncompleteLocalEvidence)?;
            if let Some(version) = profile_version {
                confirm_closed_current_matches_cloud(
                    root,
                    &workspace,
                    generation_id,
                    version,
                    expected_previous_base,
                )
                .map_err(|_| Error::InvalidLocalEvidence)?;
            } else {
                restore_materialization_binding_after_authoritative_open(
                    &workspace,
                    tenant_id,
                    profile_id,
                    generation_id,
                    &digest,
                )
                .map_err(|_| Error::InvalidLocalEvidence)?;
            }
            Ok(())
        })();
        let released = lock.release().map_err(|_| Error::InvalidLocalEvidence);
        if let Err(reason) = verified {
            if profile_version.is_some() {
                // These two create-new temporary files belong to this exact
                // read-only equality attempt. A changed local profile is an
                // ordinary conflict, not a reason to block every future
                // retry with an abandoned ciphertext spool.
                drop(plaintext);
                drop(ciphertext);
                fs::remove_file(&plaintext_path).map_err(|_| Error::Local)?;
                fs::remove_file(ciphertext_path).map_err(|_| Error::Local)?;
            }
            return Err(reason);
        }
        released?;
    } else {
        let expected = StreamedWorkspaceSnapshot {
            plaintext_bytes: metadata.plaintext_bytes(),
            plaintext_digest: metadata.plaintext_digest().bytes(),
        };
        let bind = |workspace: &GenerationWorkspace| {
            if profile_version.is_some() {
                return require_native_camoufox_config(workspace);
            }
            restore_materialization_binding_after_authoritative_open(
                workspace,
                tenant_id,
                profile_id,
                generation_id,
                &digest,
            )
            .map(|_| ())
            .map_err(|_| GenerationSnapshotError::PrePublish)
        };
        let published = match profile_version {
            Some(version) => materialize_profile_current_snapshot_streaming_with_pre_publish(
                root,
                tenant_id,
                profile_id,
                generation_id,
                version,
                &mut plaintext,
                expected,
                bind,
            ),
            None => materialize_workspace_snapshot_streaming_with_pre_publish(
                root,
                tenant_id,
                profile_id,
                generation_id,
                &mut plaintext,
                expected,
                bind,
            ),
        };
        published.map_err(|error| match error {
            GenerationSnapshotError::Local(_) => Error::Local,
            GenerationSnapshotError::PrePublish => Error::BrowserIdentity,
            _ => Error::Snapshot,
        })?;
    }
    // Only after the full AEAD tag + BPGW + browser binding are verified and
    // the canonical workspace is published may the two owned scratch files
    // be retired. Failure to remove one prevents a false Clean terminal.
    drop(plaintext);
    drop(ciphertext);
    fs::remove_file(&plaintext_path).map_err(|_| Error::Local)?;
    fs::remove_file(ciphertext_path).map_err(|_| Error::Local)?;
    Ok(())
}

/// Called exclusively after full BPGW equality with the Worker-confirmed
/// CURRENT inside the already-held OS writer lock. This is also the only
/// automatic local receipt reconciliation for a lost DRAFT Commit reply.
fn confirm_closed_current_matches_cloud(
    root: &MaterializationRoot,
    workspace: &GenerationWorkspace,
    confirmed_id: &GenerationId,
    profile_version: u64,
    previous_base: Option<&GenerationId>,
) -> Result<(), ()> {
    if workspace.base_generation_id() != previous_base
        || workspace.profile_version().checked_add(1) != Some(profile_version)
        || match previous_base {
            Some(base) => workspace.generation_id() != base || confirmed_id == base,
            None => workspace.generation_id() != confirmed_id,
        }
    {
        return Err(());
    }
    let path = workspace.path().join(".local-unsynced");
    let meta = fs::symlink_metadata(&path).map_err(|_| ())?;
    if meta.file_type().is_symlink()
        || !meta.is_file()
        || fs::read(&path).map_err(|_| ())? != b"local-unsynced-v2"
    {
        return Err(());
    }
    root.update_profile_current_sync_base(workspace, confirmed_id, profile_version)
        .map_err(|_| ())?;
    fs::remove_file(path).map_err(|_| ())?;
    Ok(())
}

/// Local-first native Camoufox owns fingerprint configuration. Do not require
/// an obsolete managed-browser identity sidecar in an authenticated CURRENT
/// snapshot. The official runtime validates the canonical JSON on actual Open.
fn require_native_camoufox_config(
    workspace: &GenerationWorkspace,
) -> Result<(), GenerationSnapshotError> {
    let path = workspace.path().join("camoufox-config.json");
    let metadata = fs::symlink_metadata(&path).map_err(|_| GenerationSnapshotError::PrePublish)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        return Err(GenerationSnapshotError::PrePublish);
    }
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
    fn streamed_binding_repair_preserves_exact_native_files_without_vec()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        let mut download = FakeStreamDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        let device = DeviceId::parse("device_binding_repair_stream")?;
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &device,
            1,
            &mut control,
            &mut download,
        )?;
        let workspace = fixture.root.open_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let sidecar = workspace
            .path()
            .parent()
            .ok_or_else(|| std::io::Error::other("missing generation parent"))?
            .join(format!(
                ".{}.materialization",
                fixture.generation_id.as_str()
            ));
        fs::remove_file(&sidecar)?;
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &device,
            2,
            &mut control,
            &mut download,
        )?;
        assert_eq!(download.calls, 2);
        assert!(
            load_materialization_binding(
                &workspace,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id
            )
            .is_ok()
        );
        assert_eq!(
            fs::read(workspace.path().join("user_data/storage/state.bin"))?,
            b"authoritative-state"
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn streamed_binding_repair_refusal_never_claims_local_workspace_absence()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        let mut download = FakeStreamDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        let device = DeviceId::parse("device_binding_repair_control_refusal")?;
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &device,
            1,
            &mut control,
            &mut download,
        )?;
        let workspace = fixture.root.open_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let sidecar = workspace
            .path()
            .parent()
            .ok_or_else(|| std::io::Error::other("missing generation parent"))?
            .join(format!(
                ".{}.materialization",
                fixture.generation_id.as_str()
            ));
        fs::remove_file(&sidecar)?;
        control.opening_error = Some(GenerationReopenControlError::Transport);
        let outcome = ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &device,
            2,
            &mut control,
            &mut download,
        );
        assert_eq!(
            outcome,
            Err(AuthoritativeGenerationMaterializationError::ControlPlane(
                GenerationReopenControlError::Transport
            ))
        );
        assert!(matches!(
            outcome,
            Err(reason) if !reason.proves_no_local_write_and_absent_workspace()
        ));
        assert!(!sidecar.exists());
        assert_eq!(
            fs::read(workspace.path().join("user_data/storage/state.bin"))?,
            b"authoritative-state"
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn streamed_binding_repair_rejects_changed_native_bytes_without_sidecar()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut control = fixture.control();
        let mut download = FakeStreamDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        let device = DeviceId::parse("device_binding_repair_negative")?;
        ensure_authoritative_generation(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            &device,
            1,
            &mut control,
            &mut download,
        )?;
        let workspace = fixture.root.open_generation(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
        )?;
        let sidecar = workspace
            .path()
            .parent()
            .ok_or_else(|| std::io::Error::other("missing generation parent"))?
            .join(format!(
                ".{}.materialization",
                fixture.generation_id.as_str()
            ));
        fs::remove_file(&sidecar)?;
        fs::write(
            workspace.path().join("user_data/storage/state.bin"),
            b"changed",
        )?;
        assert_eq!(
            ensure_authoritative_generation(
                &fixture.root,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id,
                &device,
                2,
                &mut control,
                &mut download,
            ),
            Err(AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence)
        );
        assert!(!sidecar.exists());
        assert_eq!(
            fs::read(workspace.path().join("user_data/storage/state.bin"))?,
            b"changed"
        );
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn lost_draft_commit_reconciles_only_exact_cloud_current_without_second_browser()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::without_browser_identity()?;
        let workspace = fixture.root.create_profile_current(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            1,
        )?;
        fs::create_dir_all(workspace.path().join("user_data/storage"))?;
        fs::write(workspace.path().join("prefs.js"), b"authoritative-prefs")?;
        fs::write(
            workspace.path().join("user_data/storage/state.bin"),
            b"authoritative-state",
        )?;
        fs::write(
            workspace.path().join("camoufox-config.json"),
            &fixture.config_bytes,
        )?;
        fs::write(
            workspace.path().join(".local-unsynced"),
            b"local-unsynced-v2",
        )?;
        let mut control = fixture.control();
        let mut downloader = FakeStreamDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        super::reconcile_closed_draft_with_confirmed_current(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            2,
            &DeviceId::parse("device_draft_confirmed")?,
            11,
            &mut control,
            &mut downloader,
        )?;
        let confirmed = fixture
            .root
            .open_profile_current(&fixture.tenant_id, &fixture.profile_id)?;
        assert_eq!(confirmed.path(), workspace.path());
        assert_eq!(confirmed.base_generation_id(), Some(&fixture.generation_id));
        assert_eq!(confirmed.profile_version(), 2);
        assert!(!confirmed.path().join(".local-unsynced").exists());
        assert_eq!(
            fs::read(confirmed.path().join("user_data/storage/state.bin"))?,
            b"authoritative-state",
        );
        assert_eq!(downloader.calls, 1);
        fixture.cleanup();
        Ok(())
    }

    #[test]
    fn lost_draft_commit_rejects_different_native_bytes_and_preserves_dirty()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::without_browser_identity()?;
        let workspace = fixture.root.create_profile_current(
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            1,
        )?;
        fs::create_dir_all(workspace.path().join("user_data/storage"))?;
        fs::write(workspace.path().join("prefs.js"), b"authoritative-prefs")?;
        fs::write(
            workspace.path().join("user_data/storage/state.bin"),
            b"NEW LOCAL USER DATA",
        )?;
        fs::write(
            workspace.path().join("camoufox-config.json"),
            &fixture.config_bytes,
        )?;
        fs::write(
            workspace.path().join(".local-unsynced"),
            b"local-unsynced-v2",
        )?;
        let mut control = fixture.control();
        let mut downloader = FakeStreamDownloader {
            container: fixture.container.clone(),
            calls: 0,
        };
        let outcome = super::reconcile_closed_draft_with_confirmed_current(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            2,
            &DeviceId::parse("device_draft_diverged")?,
            12,
            &mut control,
            &mut downloader,
        );
        assert_eq!(
            outcome,
            Err(AuthoritativeGenerationMaterializationError::IncompleteLocalEvidence),
        );
        let still_local = fixture
            .root
            .open_profile_current(&fixture.tenant_id, &fixture.profile_id)?;
        assert_eq!(still_local.base_generation_id(), None);
        assert_eq!(still_local.profile_version(), 1);
        assert_eq!(
            fs::read(still_local.path().join("user_data/storage/state.bin"))?,
            b"NEW LOCAL USER DATA",
        );
        assert_eq!(
            fs::read(still_local.path().join(".local-unsynced"))?,
            b"local-unsynced-v2",
        );
        let digest = encode_sha256(fixture.container_digest);
        assert!(
            !fixture
                .root
                .path()
                .join(format!(".pending-r2-{digest}"))
                .exists()
        );
        assert!(
            !fixture
                .root
                .path()
                .join(format!(".pending-bpgw-{digest}"))
                .exists()
        );
        // A subsequent exact retry can reconcile once the local files equal
        // the immutable cloud snapshot, with no second generation or reset.
        fs::write(
            still_local.path().join("user_data/storage/state.bin"),
            b"authoritative-state",
        )?;
        super::reconcile_closed_draft_with_confirmed_current(
            &fixture.root,
            &fixture.tenant_id,
            &fixture.profile_id,
            &fixture.generation_id,
            2,
            &DeviceId::parse("device_draft_diverged")?,
            13,
            &mut control,
            &mut downloader,
        )?;
        let confirmed = fixture
            .root
            .open_profile_current(&fixture.tenant_id, &fixture.profile_id)?;
        assert_eq!(confirmed.profile_version(), 2);
        assert_eq!(confirmed.base_generation_id(), Some(&fixture.generation_id));
        fixture.cleanup();
        Ok(())
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
        assert!(
            load_materialization_binding(
                &materialized,
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id
            )
            .is_ok()
        );
        let digest = encode_sha256(fixture.container_digest);
        assert!(
            !fixture
                .root
                .path()
                .join(format!(".pending-r2-{digest}"))
                .exists()
        );
        assert!(
            !fixture
                .root
                .path()
                .join(format!(".pending-bpgw-{digest}"))
                .exists()
        );
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
            Err(
                AuthoritativeGenerationMaterializationError::ReadBeforeLocalUse(
                    AuthoritativeGenerationPreWriteError::Decryption
                )
            )
        );
        assert_eq!(control.opening_calls, 1);
        assert_eq!(downloader.calls, 1);
        assert!(matches!(
            fixture.root.open_generation(
                &fixture.tenant_id,
                &fixture.profile_id,
                &fixture.generation_id
            ),
            Err(crate::local_profile::LocalProfileError::Io(
                std::io::ErrorKind::NotFound
            ))
        ));
        assert!(
            fixture
                .root
                .path()
                .join(format!(
                    ".pending-r2-{}",
                    encode_sha256(control.container_digest)
                ))
                .exists()
        );
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
