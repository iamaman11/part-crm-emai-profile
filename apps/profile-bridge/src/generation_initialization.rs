//! Automatic first-open preparation. Reuses the snapshot, cipher, immutable upload and launch owners.
use crate::browser_execution::{persist_browser_identity, persist_materialization_binding};
use crate::generation_snapshot::encode_workspace_snapshot;
use crate::local_profile::{BridgeWorkspaceLock, GenerationWorkspace, MaterializationRoot};
use crate::operator_flow::RuntimeBundleSelectionPort;
use crate::runtime_bundle::ApprovedRuntimeBundle;
use crate::shipping_control_plane::{
    MachineHttpMethod, MachineHttpPort, MachineHttpResponse, sealing_material_from_response,
};
use crate::shipping_generation_save::{
    GenerationUploadAuthorization, SignedGenerationObjectPutPort,
};
use crate::shipping_generation_successor_control::decode_upload_authorization;
use bridge_domain::{BridgePortError, ClaimUri};
use browser_execution_domain::BrowserIdentityManifest;
use browser_execution_domain::MaterializationBinding;
use control_plane_contract::generation_initialization_api::{
    InitialGenerationCommitted, InitialGenerationPrepared, InitialGenerationRequest as Request,
};
use control_plane_contract::profile_generation_api::BridgeGenerationUploadCapabilityResponse;
use encrypted_generation_domain::{
    GenerationIdentity, GenerationMetadata, PlaintextDigest, seal_generation,
};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, GenerationId, ProfileId, TenantId,
};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const INITIAL_OPEN_URI_PREFIX: &str = "profilebridge://open/v2/";
static SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialProfileStatus {
    Draft,
    Ready,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialProfileOpen {
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub profile_id: ProfileId,
    pub expected_version: u64,
    pub status: InitialProfileStatus,
}
impl InitialProfileOpen {
    pub fn parse(uri: &str) -> Result<Self, BridgePortError> {
        let rest = uri
            .strip_prefix(INITIAL_OPEN_URI_PREFIX)
            .ok_or(BridgePortError::InvalidResponse)?;
        let parts: Vec<_> = rest.split('/').collect();
        if parts.len() != 5
            || !parts[3].bytes().all(|c| c.is_ascii_digit())
            || parts[3].starts_with('0')
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let expected_version = parts[3]
            .parse::<u64>()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        let status = match parts[4] {
            "DRAFT" => InitialProfileStatus::Draft,
            "READY" => InitialProfileStatus::Ready,
            _ => return Err(BridgePortError::InvalidResponse),
        };
        if expected_version == 0 || expected_version >= i64::MAX as u64 {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(Self {
            tenant_id: TenantId::parse(parts[0]).map_err(|_| BridgePortError::InvalidResponse)?,
            actor_id: ActorId::parse(parts[1]).map_err(|_| BridgePortError::InvalidResponse)?,
            profile_id: ProfileId::parse(parts[2]).map_err(|_| BridgePortError::InvalidResponse)?,
            expected_version,
            status,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialProfileError {
    Authorization,
    RuntimeBundle,
    Runtime,
    Snapshot,
    Encryption,
    Upload,
    Commit,
    SavedRecovery,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn post<T: MachineHttpPort>(
    transport: &mut T,
    path: &str,
    request: &Request,
) -> Result<MachineHttpResponse, InitialProfileError> {
    let body = serde_json::to_vec(request).map_err(|_| InitialProfileError::Authorization)?;
    let id = CorrelationId::parse(format!(
        "corr_initial_{}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
    .map_err(|_| InitialProfileError::Authorization)?;
    transport
        .request(MachineHttpMethod::PostJson, path, &id, Some(&body))
        .map_err(|_| InitialProfileError::Authorization)
}
fn decode<D: serde::de::DeserializeOwned>(
    response: MachineHttpResponse,
) -> Result<D, InitialProfileError> {
    if response.status() != 200 || response.body().len() > 16_384 {
        return Err(InitialProfileError::Authorization);
    }
    serde_json::from_slice(response.body()).map_err(|_| InitialProfileError::Authorization)
}

/// Reuse the existing authenticated server Prepare owner for both DRAFT
/// initialization and a NEW claim for server-selected READY. This stage
/// cannot create local Firefox files or encrypted cloud generations.
fn prepare_initial_or_ready_launch<T: MachineHttpPort>(
    intent: &InitialProfileOpen,
    actor: &ActorContext,
    device: &DeviceId,
    transport: &mut T,
) -> Result<(GenerationId, InitialGenerationPrepared), InitialProfileError> {
    if actor.actor_id() != &intent.actor_id || actor.tenant_scope().tenant_id() != &intent.tenant_id
    {
        return Err(InitialProfileError::Authorization);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| InitialProfileError::Authorization)?
        .as_nanos();
    let identity = format!(
        "{}:{}:{}:{}",
        device.as_str(),
        now,
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let generation = GenerationId::parse(format!(
        "generation_{}",
        hex(&Sha256::digest(identity.as_bytes()))
    ))
    .map_err(|_| InitialProfileError::Authorization)?;
    let path = format!(
        "/bridge/v1/tenants/{}/profiles/{}/generation-initialization",
        intent.tenant_id.as_str(),
        intent.profile_id.as_str()
    );
    let generation_id = generation.as_str().to_owned();
    let expected_profile_version = intent.expected_version;
    let prepared: InitialGenerationPrepared = decode(post(
        transport,
        &path,
        &Request::Prepare {
            generation_id: generation_id.clone(),
            expected_profile_version,
        },
    )?)?;
    Ok((generation, prepared))
}

/// Bounded continuation of the original user-triggered Open after exact
/// N->N+1 recovery. Only an authenticated server READY/Launch verdict
/// can issue a new claim; never enter DRAFT initialization or create N+2.
pub fn reauthorize_current_ready_generation<T: MachineHttpPort>(
    intent: &InitialProfileOpen,
    actor: &ActorContext,
    device: &DeviceId,
    transport: &mut T,
) -> Result<ClaimUri, InitialProfileError> {
    let (_, response) = prepare_initial_or_ready_launch(intent, actor, device, transport)?;
    match response {
        InitialGenerationPrepared::Launch { launch } => {
            ClaimUri::parse(&launch.launch_uri).map_err(|_| InitialProfileError::Authorization)
        }
        InitialGenerationPrepared::Initialize { .. } => Err(InitialProfileError::Authorization),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn initialize<
    T: MachineHttpPort,
    U: SignedGenerationObjectPutPort,
    B: RuntimeBundleSelectionPort,
    F,
>(
    intent: &InitialProfileOpen,
    actor: &ActorContext,
    device: &DeviceId,
    root: &MaterializationRoot,
    materialize: F,
    bundles: &mut B,
    transport: &mut T,
    uploader: &mut U,
) -> Result<ClaimUri, InitialProfileError>
where
    F: FnOnce(
        &GenerationWorkspace,
        &ApprovedRuntimeBundle,
    ) -> Result<BrowserIdentityManifest, InitialProfileError>,
{
    let (generation, prepared) = prepare_initial_or_ready_launch(intent, actor, device, transport)?;
    let generation_id = generation.as_str().to_owned();
    let expected_profile_version = intent.expected_version;
    match prepared {
        InitialGenerationPrepared::Launch { launch } => {
            return ClaimUri::parse(&launch.launch_uri)
                .map_err(|_| InitialProfileError::Authorization);
        }
        InitialGenerationPrepared::Initialize {
            generation_id: observed,
            expected_profile_version: version,
        } if observed == generation_id && version == expected_profile_version => {}
        _ => return Err(InitialProfileError::Authorization),
    }
    #[cfg(windows)]
    let initial_started = std::time::Instant::now();
    let bundle = bundles
        .select_bundle()
        .map_err(|_| InitialProfileError::RuntimeBundle)?;
    #[cfg(windows)]
    eprintln!(
        "bridge_initial_phase name=runtime_integrity elapsed_ms={}",
        initial_started.elapsed().as_millis()
    );
    let workspace = root
        .create_generation(&intent.tenant_id, &intent.profile_id, &generation)
        .map_err(|_| InitialProfileError::Snapshot)?;
    // Local initialization owns an unpublished directory, not a fabricated coordinator lease.
    let writer = BridgeWorkspaceLock::acquire(&workspace, device, 1)
        .map_err(|_| InitialProfileError::Snapshot)?;
    let mut committed = false;
    let result = (|| {
        #[cfg(windows)]
        let identity_started = std::time::Instant::now();
        let browser_identity = materialize(&workspace, &bundle)?;
        #[cfg(windows)]
        eprintln!(
            "bridge_initial_phase name=browser_identity elapsed_ms={}",
            identity_started.elapsed().as_millis()
        );
        persist_browser_identity(&workspace, &browser_identity)
            .map_err(|_| InitialProfileError::Snapshot)?;
        #[cfg(windows)]
        let sealing_started = std::time::Instant::now();
        let inventory = workspace
            .inventory()
            .map_err(|_| InitialProfileError::Snapshot)?;
        let plaintext = zeroize::Zeroizing::new(
            encode_workspace_snapshot(&workspace, &inventory)
                .map_err(|_| InitialProfileError::Snapshot)?,
        );
        if workspace
            .inventory()
            .map_err(|_| InitialProfileError::Snapshot)?
            != inventory
        {
            return Err(InitialProfileError::Snapshot);
        }
        let (container_digest, receipt) = seal_upload_commit_initial(
            &intent.tenant_id,
            &intent.profile_id,
            &generation,
            expected_profile_version,
            &plaintext,
            transport,
            uploader,
        )?;
        committed = true;
        let binding = MaterializationBinding::new(
            intent.tenant_id.clone(),
            intent.profile_id.clone(),
            generation.clone(),
            container_digest,
            workspace
                .materialization_inventory_digest()
                .map_err(|_| InitialProfileError::SavedRecovery)?,
            browser_identity,
        )
        .map_err(|_| InitialProfileError::SavedRecovery)?;
        persist_materialization_binding(&workspace, &binding)
            .map_err(|_| InitialProfileError::SavedRecovery)?;
        ClaimUri::parse(&receipt.launch.launch_uri).map_err(|_| InitialProfileError::Commit)
    })();
    let released = writer.release();
    if committed && (result.is_err() || released.is_err()) {
        // Cloud commit is authoritative, and local cleanup/binding persistence can fail later.
        // Retain local bytes and ambiguous locks for evidenced recovery, never delete them.
        return Err(InitialProfileError::SavedRecovery);
    }
    if released.is_err() {
        return Err(InitialProfileError::Snapshot);
    }
    result
}

/// Retryable first Sync of a CLOSED local DRAFT. This is deliberately not
/// part of browser Open/Close, and uses the existing generation-initialization
/// Prepare -> Seal -> signed R2 PUT -> verified D1 Commit contract unchanged.
///
/// The stable local folder is never copied into another browser generation,
/// deleted or overwritten. A rejected/uncertain server effect leaves dirty
/// evidence intact; no speculative N+2 is generated.
pub fn sync_closed_local_draft<T: MachineHttpPort, U: SignedGenerationObjectPutPort>(
    intent: &InitialProfileOpen,
    actor: &ActorContext,
    device: &DeviceId,
    root: &MaterializationRoot,
    transport: &mut T,
    uploader: &mut U,
) -> Result<(), InitialProfileError> {
    use crate::browser_execution::browser_writer_locks_absent;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    if intent.status != InitialProfileStatus::Draft
        || actor.tenant_scope().tenant_id() != &intent.tenant_id
        || actor.actor_id() != &intent.actor_id
    {
        return Err(InitialProfileError::Authorization);
    }

    let workspace = root
        .open_profile_current(&intent.tenant_id, &intent.profile_id)
        .map_err(|_| InitialProfileError::Snapshot)?;
    if workspace.base_generation_id().is_some()
        || workspace.profile_version() != intent.expected_version
    {
        return Err(InitialProfileError::Authorization);
    }
    let dirty = workspace.path().join(".local-unsynced");
    // This exact marker was created before Firefox was allowed to write.
    // A missing or foreign marker must never trigger a cloud Save.
    if fs::read(&dirty).map_err(|_| InitialProfileError::Snapshot)? != b"local-unsynced-v2" {
        return Err(InitialProfileError::Snapshot);
    }
    let stamp = || -> Result<u64, InitialProfileError> {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| InitialProfileError::Snapshot)?;
        u64::try_from(since_epoch.as_millis()).map_err(|_| InitialProfileError::Snapshot)
    };
    let writer = BridgeWorkspaceLock::acquire(&workspace, device, stamp()?)
        .map_err(|_| InitialProfileError::Snapshot)?;
    // Encode only while the native browser has released its writer lock.
    let captured = (|| {
        if !matches!(browser_writer_locks_absent(&workspace), Ok(true)) {
            return Err(InitialProfileError::Snapshot);
        }
        let inventory = workspace
            .inventory()
            .map_err(|_| InitialProfileError::Snapshot)?;
        let bytes = zeroize::Zeroizing::new(
            encode_workspace_snapshot(&workspace, &inventory)
                .map_err(|_| InitialProfileError::Snapshot)?,
        );
        if workspace
            .inventory()
            .map_err(|_| InitialProfileError::Snapshot)?
            != inventory
        {
            return Err(InitialProfileError::Snapshot);
        }
        Ok((bytes, inventory))
    })();
    let released = writer.release().map_err(|_| InitialProfileError::Snapshot);
    let (plaintext, inventory) = captured?;
    released?;

    let generation = workspace.generation_id().clone();
    let path = format!(
        "/bridge/v1/tenants/{}/profiles/{}/generation-initialization",
        intent.tenant_id.as_str(),
        intent.profile_id.as_str(),
    );
    let prepared: InitialGenerationPrepared = decode(post(
        transport,
        &path,
        &Request::Prepare {
            generation_id: generation.as_str().to_owned(),
            expected_profile_version: intent.expected_version,
        },
    )?)?;
    match prepared {
        InitialGenerationPrepared::Initialize {
            generation_id,
            expected_profile_version,
        } if generation_id == generation.as_str()
            && expected_profile_version == intent.expected_version => {}
        _ => return Err(InitialProfileError::Authorization),
    }

    let (_, receipt) = seal_upload_commit_initial(
        &intent.tenant_id,
        &intent.profile_id,
        &generation,
        intent.expected_version,
        &plaintext,
        transport,
        uploader,
    )?;
    if receipt.generation_id != generation.as_str()
        || receipt.profile_version != intent.expected_version + 1
    {
        return Err(InitialProfileError::Commit);
    }

    // Commit was confirmed, but another browser may already have reopened.
    // A busy writer means recovery of local receipt is pending, never Saved.
    let current = root
        .open_profile_current(&intent.tenant_id, &intent.profile_id)
        .map_err(|_| InitialProfileError::SavedRecovery)?;
    let guard = BridgeWorkspaceLock::acquire(&current, device, stamp()?)
        .map_err(|_| InitialProfileError::SavedRecovery)?;
    let applied = (|| {
        if current.base_generation_id().is_some()
            || current.generation_id() != &generation
            || current.profile_version() != intent.expected_version
            || !matches!(browser_writer_locks_absent(&current), Ok(true))
        {
            return Err(InitialProfileError::SavedRecovery);
        }
        let unchanged = current
            .inventory()
            .map_err(|_| InitialProfileError::SavedRecovery)?
            == inventory;
        root.update_profile_current_sync_base(&current, &generation, receipt.profile_version)
            .map_err(|_| InitialProfileError::SavedRecovery)?;
        // Never clear a dirty marker if user data changed during R2 upload.
        if unchanged {
            let metadata =
                fs::symlink_metadata(&dirty).map_err(|_| InitialProfileError::SavedRecovery)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || fs::read(&dirty).map_err(|_| InitialProfileError::SavedRecovery)?
                    != b"local-unsynced-v2"
            {
                return Err(InitialProfileError::SavedRecovery);
            }
            fs::remove_file(&dirty).map_err(|_| InitialProfileError::SavedRecovery)?;
        }
        Ok(())
    })();
    let unlocked = guard
        .release()
        .map_err(|_| InitialProfileError::SavedRecovery);
    applied?;
    unlocked
}

/// One existing Initial Generation encryption, signed R2 PUT and verified
/// Worker Commit owner. Both the legacy initial adapter and the new closed
/// local DRAFT use it; no second backup format, protocol or cloud writer.
fn seal_upload_commit_initial<T: MachineHttpPort, U: SignedGenerationObjectPutPort>(
    tenant: &TenantId,
    profile: &ProfileId,
    generation: &GenerationId,
    version: u64,
    plaintext: &[u8],
    transport: &mut T,
    uploader: &mut U,
) -> Result<(String, InitialGenerationCommitted), InitialProfileError> {
    let path = format!(
        "/bridge/v1/tenants/{}/profiles/{}/generation-initialization",
        tenant.as_str(),
        profile.as_str()
    );
    let generation_id = generation.as_str().to_owned();
    let expected_profile_version = version;
    #[cfg(windows)]
    let sealing_started = std::time::Instant::now();
    let digest = PlaintextDigest::calculate(plaintext).bytes();
    let material = sealing_material_from_response(post(
        transport,
        &path,
        &Request::SealingMaterial {
            generation_id: generation_id.clone(),
            expected_profile_version,
            plaintext_digest: hex(&digest),
        },
    )?)
    .map_err(|_| InitialProfileError::Encryption)?;
    let (dek, nonce_prefix, chunk_size) = material.into_parts();
    // First generation has no base. Never fabricate a source generation or container digest.
    let metadata = GenerationMetadata::for_plaintext(
        GenerationIdentity::new(tenant.clone(), profile.clone(), generation.clone(), None),
        dek.key_id().clone(),
        nonce_prefix,
        chunk_size,
        plaintext,
    )
    .map_err(|_| InitialProfileError::Encryption)?;
    let sealed =
        seal_generation(&metadata, &dek, plaintext).map_err(|_| InitialProfileError::Encryption)?;
    if PlaintextDigest::calculate(plaintext).bytes() != digest {
        return Err(InitialProfileError::Snapshot);
    }
    let metadata_digest = sealed
        .metadata_digest()
        .bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let container_digest = sealed
        .container_digest()
        .bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let container_bytes =
        u64::try_from(sealed.container().len()).map_err(|_| InitialProfileError::Encryption)?;
    #[cfg(windows)]
    eprintln!(
        "bridge_initial_phase name=snapshot_and_seal elapsed_ms={}",
        sealing_started.elapsed().as_millis()
    );
    #[cfg(windows)]
    let upload_started = std::time::Instant::now();
    let upload = Request::UploadCapability {
        generation_id: generation_id.clone(),
        expected_profile_version,
        metadata_digest: metadata_digest.clone(),
        container_digest: container_digest.clone(),
        container_bytes,
    };
    let authorization: BridgeGenerationUploadCapabilityResponse =
        decode(post(transport, &path, &upload)?)?;
    if let GenerationUploadAuthorization::UploadRequired(capability) =
        decode_upload_authorization::<core::convert::Infallible>(authorization)
            .map_err(|_| InitialProfileError::Upload)?
    {
        uploader
            .put_exact(&capability, sealed.container())
            .map_err(|_| InitialProfileError::Upload)?;
    }
    // The authoritative Commit handler verifies the exact encrypted R2
    // descriptor again *before* persisting READY and issuing a claim.
    // A second UploadCapability call would repeat the same read, add
    // another network round trip and still not replace commit validation.
    // No browser can start unless Commit succeeds with that exact object.
    let receipt: InitialGenerationCommitted = decode(post(
        transport,
        &path,
        &Request::Commit {
            generation_id: generation_id.clone(),
            expected_profile_version,
            metadata_digest,
            container_digest: container_digest.clone(),
            container_bytes,
        },
    )?)
    .map_err(|_| InitialProfileError::Commit)?;
    if receipt.generation_id != generation_id
        || receipt.profile_version != expected_profile_version + 1
    {
        return Err(InitialProfileError::Commit);
    }
    #[cfg(windows)]
    eprintln!(
        "bridge_initial_phase name=upload_verify_commit elapsed_ms={}",
        upload_started.elapsed().as_millis()
    );
    Ok((container_digest, receipt))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_draft_sync_refuses_missing_or_false_dirty_evidence_before_network()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::shipping_control_plane::MachineHttpResponse;
        use crate::shipping_generation_save::SignedGenerationUploadCapability;
        use profile_platform_primitives::TenantScope;

        struct NoTransport;
        impl MachineHttpPort for NoTransport {
            type Error = ();
            fn request(
                &mut self,
                _: MachineHttpMethod,
                _: &str,
                _: &CorrelationId,
                _: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, ()> {
                panic!("DRAFT Sync must not contact the server before local evidence admission")
            }
        }
        struct NoUpload;
        impl SignedGenerationObjectPutPort for NoUpload {
            type Error = ();
            fn put_exact(
                &mut self,
                _: &SignedGenerationUploadCapability,
                _: &[u8],
            ) -> Result<(), ()> {
                panic!("DRAFT Sync must not upload an unverified local folder")
            }
        }

        let intent = InitialProfileOpen::parse(
            "profilebridge://open/v2/tenant_draft_test/actor_draft_test/profile_draft_test/1/DRAFT",
        )?;
        let actor = ActorContext::new(
            TenantScope::new(intent.tenant_id.clone()),
            intent.actor_id.clone(),
            CorrelationId::parse("corr_draft_test")?,
        );
        let device = DeviceId::parse("device_draft_test")?;
        let path = std::env::temp_dir().join(format!(
            "bridge-sync-draft-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let root = MaterializationRoot::open_or_create(&path)?;
        let local_id = GenerationId::parse("generation_draft_test")?;
        let workspace =
            root.create_profile_current(&intent.tenant_id, &intent.profile_id, &local_id, 1)?;
        let sync = |transport: &mut NoTransport, uploader: &mut NoUpload| {
            sync_closed_local_draft(&intent, &actor, &device, &root, transport, uploader)
        };
        assert_eq!(
            sync(&mut NoTransport, &mut NoUpload),
            Err(InitialProfileError::Snapshot)
        );
        std::fs::write(workspace.path().join(".local-unsynced"), b"invalid-marker")?;
        assert_eq!(
            sync(&mut NoTransport, &mut NoUpload),
            Err(InitialProfileError::Snapshot)
        );
        std::fs::remove_dir_all(&path)?;
        Ok(())
    }

    #[test]
    fn closed_draft_sync_rejects_ready_or_foreign_actor_without_network()
    -> Result<(), Box<dyn std::error::Error>> {
        struct NoTransport;
        impl MachineHttpPort for NoTransport {
            type Error = ();
            fn request(
                &mut self,
                _: MachineHttpMethod,
                _: &str,
                _: &CorrelationId,
                _: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, ()> {
                panic!("unauthorized sync contacted control plane")
            }
        }
        struct NoUpload;
        impl SignedGenerationObjectPutPort for NoUpload {
            type Error = ();
            fn put_exact(
                &mut self,
                _: &crate::shipping_generation_save::SignedGenerationUploadCapability,
                _: &[u8],
            ) -> Result<(), ()> {
                panic!("unauthorized sync uploaded a snapshot")
            }
        }
        use profile_platform_primitives::TenantScope;
        let intent = InitialProfileOpen::parse(
            "profilebridge://open/v2/tenant_reject_test/actor_reject_test/profile_reject_test/1/READY",
        )?;
        let actor = ActorContext::new(
            TenantScope::new(intent.tenant_id.clone()),
            intent.actor_id.clone(),
            CorrelationId::parse("corr_reject_test")?,
        );
        let device = DeviceId::parse("device_reject_test")?;
        let path = std::env::temp_dir().join(format!(
            "bridge-sync-reject-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let root = MaterializationRoot::open_or_create(&path)?;
        assert_eq!(
            sync_closed_local_draft(
                &intent,
                &actor,
                &device,
                &root,
                &mut NoTransport,
                &mut NoUpload
            ),
            Err(InitialProfileError::Authorization)
        );
        std::fs::remove_dir_all(&path)?;
        Ok(())
    }

    #[test]
    fn closed_draft_sync_commits_verified_object_then_clears_only_exact_dirty_marker()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::shipping_control_plane::MachineHttpResponse;
        use crate::shipping_generation_save::SignedGenerationUploadCapability;
        use control_plane_contract::generation_key_api::{
            BridgeGenerationSealingMaterialResponse, GENERATION_SEALING_CHUNK_BYTES,
        };
        use control_plane_contract::profile_generation_api::BridgeGenerationUploadCapabilityResponse;
        use profile_platform_primitives::TenantScope;
        struct VerifiedTransport {
            operations: Vec<&'static str>,
        }
        impl MachineHttpPort for VerifiedTransport {
            type Error = ();
            fn request(
                &mut self,
                _: MachineHttpMethod,
                _: &str,
                _: &CorrelationId,
                body: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, ()> {
                let input: Request = serde_json::from_slice(body.ok_or(())?).map_err(|_| ())?;
                let reply = match input {
                    Request::Prepare {
                        generation_id,
                        expected_profile_version,
                    } => {
                        self.operations.push("prepare");
                        serde_json::json!({
                            "state": "initialize",
                            "generationId": generation_id,
                            "expectedProfileVersion": expected_profile_version,
                        })
                    }
                    Request::SealingMaterial { .. } => {
                        self.operations.push("seal");
                        serde_json::to_value(BridgeGenerationSealingMaterialResponse::new(
                            "profile-generation-root-v1-7",
                            "37".repeat(32),
                            "71".repeat(16),
                            GENERATION_SEALING_CHUNK_BYTES,
                        ))
                        .map_err(|_| ())?
                    }
                    Request::UploadCapability { .. } => {
                        self.operations.push("verify-upload");
                        serde_json::to_value(BridgeGenerationUploadCapabilityResponse::verified())
                            .map_err(|_| ())?
                    }
                    Request::Commit {
                        generation_id,
                        expected_profile_version,
                        ..
                    } => {
                        self.operations.push("commit");
                        serde_json::json!({
                            "generationId": generation_id,
                            "profileVersion": expected_profile_version + 1,
                            "replayed": false,
                            "launch": {
                                "launchUri": "profilebridge://claim/claim_ready_0123456789abcdef0123456789",
                                "expiresAtMs": 1000,
                            },
                        })
                    }
                };
                Ok(MachineHttpResponse::new(
                    200,
                    serde_json::to_vec(&reply).map_err(|_| ())?,
                ))
            }
        }
        struct NoUpload;
        impl SignedGenerationObjectPutPort for NoUpload {
            type Error = ();
            fn put_exact(
                &mut self,
                _: &SignedGenerationUploadCapability,
                _: &[u8],
            ) -> Result<(), ()> {
                panic!("server already verified this immutable snapshot")
            }
        }
        let intent = InitialProfileOpen::parse(
            "profilebridge://open/v2/tenant_draft_sync/actor_draft_sync/profile_draft_sync/1/DRAFT",
        )?;
        let actor = ActorContext::new(
            TenantScope::new(intent.tenant_id.clone()),
            intent.actor_id.clone(),
            CorrelationId::parse("corr_draft_sync")?,
        );
        let device = DeviceId::parse("device_draft_sync")?;
        let path = std::env::temp_dir().join(format!(
            "bridge-sync-success-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let root = MaterializationRoot::open_or_create(&path)?;
        let local_id = GenerationId::parse("generation_local_draft_sync")?;
        let workspace =
            root.create_profile_current(&intent.tenant_id, &intent.profile_id, &local_id, 1)?;
        std::fs::create_dir(workspace.path().join("user_data"))?;
        std::fs::write(
            workspace.path().join("user_data/cookies.sqlite"),
            b"persistent-cookie-data",
        )?;
        std::fs::write(workspace.path().join("camoufox-config.json"), b"{}")?;
        std::fs::write(
            workspace.path().join(".local-unsynced"),
            b"local-unsynced-v2",
        )?;
        let mut transport = VerifiedTransport {
            operations: Vec::new(),
        };
        sync_closed_local_draft(
            &intent,
            &actor,
            &device,
            &root,
            &mut transport,
            &mut NoUpload,
        )?;
        assert_eq!(
            transport.operations,
            ["prepare", "seal", "verify-upload", "commit"]
        );
        let confirmed = root.open_profile_current(&intent.tenant_id, &intent.profile_id)?;
        assert_eq!(confirmed.path(), workspace.path());
        assert_eq!(confirmed.base_generation_id(), Some(&local_id));
        assert_eq!(confirmed.profile_version(), 2);
        assert!(!confirmed.path().join(".local-unsynced").exists());
        assert_eq!(
            std::fs::read(confirmed.path().join("user_data/cookies.sqlite"))?,
            b"persistent-cookie-data",
        );
        std::fs::remove_dir_all(path)?;
        Ok(())
    }

    #[test]
    fn existing_profile_goes_directly_to_claim_without_materialization_or_upload()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::runtime_bundle::ApprovedRuntimeBundle;
        use crate::shipping_control_plane::MachineHttpResponse;
        use crate::shipping_generation_save::SignedGenerationUploadCapability;
        use profile_platform_primitives::{ActorId, TenantScope};
        struct Transport;
        impl MachineHttpPort for Transport {
            type Error = ();
            fn request(
                &mut self,
                _: MachineHttpMethod,
                _: &str,
                _: &CorrelationId,
                body: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, ()> {
                assert!(matches!(
                    serde_json::from_slice::<Request>(body.ok_or(())?).map_err(|_| ())?,
                    Request::Prepare { .. }
                ));
                Ok(MachineHttpResponse::new(200, br#"{"state":"launch","launch":{"launchUri":"profilebridge://claim/claim_ready_0123456789abcdef0123456789","expiresAtMs":1000}}"#.to_vec()))
            }
        }
        struct NoBundle;
        impl RuntimeBundleSelectionPort for NoBundle {
            type Error = ();
            fn select_bundle(&mut self) -> Result<ApprovedRuntimeBundle, ()> {
                Err(())
            }
        }
        struct NoUpload;
        impl SignedGenerationObjectPutPort for NoUpload {
            type Error = ();
            fn put_exact(
                &mut self,
                _: &SignedGenerationUploadCapability,
                _: &[u8],
            ) -> Result<(), ()> {
                Err(())
            }
        }
        let intent = InitialProfileOpen::parse(
            "profilebridge://open/v2/tenant_123/actor_123/profile_123/1/DRAFT",
        )?;
        let actor = ActorContext::new(
            TenantScope::new(intent.tenant_id.clone()),
            ActorId::parse("actor_123")?,
            CorrelationId::parse("corr_existing")?,
        );
        let device = DeviceId::parse("device_123")?;
        let path = std::env::temp_dir().join(format!(
            "bridge-existing-initial-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let root = MaterializationRoot::open_or_create(&path)?;
        let result = initialize(
            &intent,
            &actor,
            &device,
            &root,
            |_, _| Err(InitialProfileError::Runtime),
            &mut NoBundle,
            &mut Transport,
            &mut NoUpload,
        );
        assert!(result.is_ok());
        assert!(!path.join(intent.tenant_id.as_str()).exists());
        std::fs::remove_dir_all(&path)?;
        Ok(())
    }
    #[test]
    fn committed_initial_generation_retains_local_bytes_after_binding_failure_and_replays_launch()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::runtime_bundle::ApprovedRuntimeBundle;
        use crate::shipping_control_plane::MachineHttpResponse;
        use crate::shipping_generation_save::SignedGenerationUploadCapability;
        use crate::test_support::browser_identity_fixture;
        use control_plane_contract::generation_key_api::{
            BridgeGenerationSealingMaterialResponse, GENERATION_SEALING_CHUNK_BYTES,
        };
        use control_plane_contract::profile_generation_api::BridgeGenerationUploadCapabilityResponse;
        use profile_platform_primitives::{ActorId, TenantScope};
        use runtime_bundle_domain::{
            BundleRelativePath, InventoryEntry, RuntimeInventory, RuntimeManifest, RuntimePlatform,
            Sha256Digest,
        };
        use std::fs;
        use std::path::PathBuf;

        fn bundle() -> Result<ApprovedRuntimeBundle, Box<dyn std::error::Error>> {
            let a = Sha256Digest::parse("a".repeat(64))?;
            let b = Sha256Digest::parse("b".repeat(64))?;
            let entry = BundleRelativePath::parse("camouhost/main.py")?;
            Ok(ApprovedRuntimeBundle::validate(
                RuntimeManifest::new(
                    "0.1.0",
                    "3.12",
                    RuntimePlatform::WindowsX86_64,
                    entry.clone(),
                    a.clone(),
                )?,
                RuntimeInventory::new([InventoryEntry::new(entry, 10, b)])?,
                &a,
            )?)
        }

        struct Bundle(ApprovedRuntimeBundle);
        impl RuntimeBundleSelectionPort for Bundle {
            type Error = ();
            fn select_bundle(&mut self) -> Result<ApprovedRuntimeBundle, ()> {
                Ok(self.0.clone())
            }
        }

        struct NoUpload;
        impl SignedGenerationObjectPutPort for NoUpload {
            type Error = ();
            fn put_exact(
                &mut self,
                _: &SignedGenerationUploadCapability,
                _: &[u8],
            ) -> Result<(), ()> {
                Err(())
            }
        }

        #[derive(Default)]
        struct CommittingTransport {
            committed_generation: Option<String>,
            commit_count: u64,
            upload_capability_count: u64,
        }
        impl MachineHttpPort for CommittingTransport {
            type Error = ();
            fn request(
                &mut self,
                _: MachineHttpMethod,
                _: &str,
                _: &CorrelationId,
                body: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, ()> {
                let request: Request = serde_json::from_slice(body.ok_or(())?).map_err(|_| ())?;
                let value = match request {
                    Request::Prepare {
                        generation_id,
                        expected_profile_version,
                    } => {
                        self.committed_generation = Some(generation_id.clone());
                        serde_json::json!({
                            "state": "initialize",
                            "generationId": generation_id,
                            "expectedProfileVersion": expected_profile_version,
                        })
                    }
                    Request::SealingMaterial { .. } => {
                        serde_json::to_value(BridgeGenerationSealingMaterialResponse::new(
                            "profile-generation-root-v1-7",
                            "37".repeat(32),
                            "71".repeat(16),
                            GENERATION_SEALING_CHUNK_BYTES,
                        ))
                        .map_err(|_| ())?
                    }
                    Request::UploadCapability { .. } => {
                        self.upload_capability_count += 1;
                        serde_json::to_value(BridgeGenerationUploadCapabilityResponse::verified())
                            .map_err(|_| ())?
                    }
                    Request::Commit {
                        generation_id,
                        expected_profile_version,
                        ..
                    } => {
                        self.commit_count += 1;
                        serde_json::json!({
                            "generationId": generation_id,
                            "profileVersion": expected_profile_version + 1,
                            "replayed": false,
                            "launch": {
                                "launchUri": "profilebridge://claim/claim_ready_0123456789abcdef0123456789",
                                "expiresAtMs": 1000,
                            },
                        })
                    }
                };
                Ok(MachineHttpResponse::new(
                    200,
                    serde_json::to_vec(&value).map_err(|_| ())?,
                ))
            }
        }

        struct ReplayTransport;
        impl MachineHttpPort for ReplayTransport {
            type Error = ();
            fn request(
                &mut self,
                _: MachineHttpMethod,
                _: &str,
                _: &CorrelationId,
                body: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, ()> {
                if !matches!(
                    serde_json::from_slice::<Request>(body.ok_or(())?).map_err(|_| ())?,
                    Request::Prepare { .. },
                ) {
                    return Err(());
                }
                Ok(MachineHttpResponse::new(
                    200,
                    br#"{"state":"launch","launch":{"launchUri":"profilebridge://claim/claim_ready_0123456789abcdef0123456789","expiresAtMs":1000}}"#.to_vec(),
                ))
            }
        }

        let intent = InitialProfileOpen::parse(
            "profilebridge://open/v2/tenant_initial_retain/actor_initial_retain/profile_initial_retain/1/DRAFT",
        )?;
        let actor = ActorContext::new(
            TenantScope::new(intent.tenant_id.clone()),
            ActorId::parse("actor_initial_retain")?,
            CorrelationId::parse("corr_initial_retain")?,
        );
        let device = DeviceId::parse("device_initial_retain")?;
        let root_path: PathBuf = std::env::temp_dir().join(format!(
            "bridge-initial-commit-preservation-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        ));
        let root = MaterializationRoot::open_or_create(&root_path)?;
        let mut bundle_owner = Bundle(bundle()?);
        let mut transport = CommittingTransport::default();
        let result = initialize(
            &intent,
            &actor,
            &device,
            &root,
            |workspace, _bundle| {
                let config = b"{\"postCommitFixture\":true}";
                fs::write(workspace.path().join("camoufox-config.json"), config)
                    .map_err(|_| InitialProfileError::Snapshot)?;
                // Simulate host-local sidecar write conflict after the server commit.
                let sidecar = workspace
                    .path()
                    .parent()
                    .ok_or(InitialProfileError::Snapshot)?
                    .join(format!(
                        ".{}.materialization",
                        workspace.generation_id().as_str()
                    ));
                fs::write(sidecar, b"existing-uncertain-local-evidence")
                    .map_err(|_| InitialProfileError::Snapshot)?;
                let sha: [u8; 32] = Sha256::digest(config).into();
                browser_identity_fixture(
                    "2.0.0",
                    "a".repeat(64),
                    &format!("profile-stability-v1-probe-{}", "b".repeat(64)),
                    hex(&sha),
                )
                .map_err(|_| InitialProfileError::Snapshot)
            },
            &mut bundle_owner,
            &mut transport,
            &mut NoUpload,
        );
        assert_eq!(result, Err(InitialProfileError::SavedRecovery));
        assert_eq!(transport.commit_count, 1);
        assert_eq!(
            transport.upload_capability_count, 1,
            "Commit is the sole final R2 verification; no redundant capability retry"
        );
        let generation = GenerationId::parse(
            transport
                .committed_generation
                .ok_or("missing committed generation")?,
        )?;
        let workspace = root.open_generation(&intent.tenant_id, &intent.profile_id, &generation)?;
        assert_eq!(
            fs::read(workspace.path().join("camoufox-config.json"))?,
            b"{\"postCommitFixture\":true}"
        );
        assert_eq!(
            fs::read(
                workspace
                    .path()
                    .parent()
                    .ok_or("no parent")?
                    .join(format!(".{}.materialization", generation.as_str()))
            )?,
            b"existing-uncertain-local-evidence",
        );

        // A second app request resolves the server's already committed launch; no bundle,
        // new local initial generation or second cloud commit is ever prepared.
        struct NoBundle;
        impl RuntimeBundleSelectionPort for NoBundle {
            type Error = ();
            fn select_bundle(&mut self) -> Result<ApprovedRuntimeBundle, ()> {
                Err(())
            }
        }
        let replay = initialize(
            &intent,
            &actor,
            &device,
            &root,
            |_, _| Err(InitialProfileError::Runtime),
            &mut NoBundle,
            &mut ReplayTransport,
            &mut NoUpload,
        )
        .map_err(|_| std::io::Error::other("replayed committed launch was rejected"))?;
        assert_eq!(
            replay,
            ClaimUri::parse("profilebridge://claim/claim_ready_0123456789abcdef0123456789")?,
        );
        assert!(workspace.path().exists());
        fs::remove_dir_all(&root_path)?;
        Ok(())
    }

    #[test]
    fn ready_reauthorization_requests_one_new_server_claim_and_never_initializes()
    -> Result<(), Box<dyn std::error::Error>> {
        use profile_platform_primitives::TenantScope;
        struct Reply {
            ready: bool,
            calls: usize,
        }
        impl MachineHttpPort for Reply {
            type Error = ();
            fn request(
                &mut self,
                method: MachineHttpMethod,
                path: &str,
                _: &CorrelationId,
                body: Option<&[u8]>,
            ) -> Result<MachineHttpResponse, ()> {
                assert_eq!(method, MachineHttpMethod::PostJson);
                assert!(path.ends_with("/generation-initialization"));
                assert!(matches!(
                    serde_json::from_slice::<Request>(body.ok_or(())?).map_err(|_| ())?,
                    Request::Prepare { .. }
                ));
                self.calls += 1;
                let body = if self.ready {
                    br#"{"state":"launch","launch":{"launchUri":"profilebridge://claim/claim_fresh_0123456789abcdef0123456789","expiresAtMs":1000}}"#.to_vec()
                } else {
                    serde_json::to_vec(&InitialGenerationPrepared::Initialize {
                        generation_id: "generation_draft_not_allowed".to_owned(),
                        expected_profile_version: 1,
                    })
                    .map_err(|_| ())?
                };
                Ok(MachineHttpResponse::new(200, body))
            }
        }
        let intent = InitialProfileOpen::parse(
            "profilebridge://open/v2/tenant_123/actor_123/profile_123/1/READY",
        )?;
        let actor = ActorContext::new(
            TenantScope::new(intent.tenant_id.clone()),
            ActorId::parse("actor_123")?,
            CorrelationId::parse("corr_new_ready_launch")?,
        );
        let device = DeviceId::parse("device_123")?;
        let mut ready = Reply {
            ready: true,
            calls: 0,
        };
        assert_eq!(
            reauthorize_current_ready_generation(&intent, &actor, &device, &mut ready)
                .map_err(|_| std::io::Error::other("fresh ready launch authorization failed"))?,
            ClaimUri::parse("profilebridge://claim/claim_fresh_0123456789abcdef0123456789")?
        );
        assert_eq!(ready.calls, 1);
        let mut draft = Reply {
            ready: false,
            calls: 0,
        };
        assert_eq!(
            reauthorize_current_ready_generation(&intent, &actor, &device, &mut draft),
            Err(InitialProfileError::Authorization)
        );
        assert_eq!(draft.calls, 1);
        Ok(())
    }

    #[test]
    fn first_open_uri_has_typed_locators_version_and_authoritative_status() {
        assert!(
            InitialProfileOpen::parse(
                "profilebridge://open/v2/tenant_123/actor_123/profile_123/1/DRAFT"
            )
            .is_ok()
        );
        assert!(
            InitialProfileOpen::parse(
                "profilebridge://open/v2/tenant_123/actor_123/profile_123/1/READY"
            )
            .is_ok()
        );
        for suffix in [
            "0/DRAFT",
            "01/DRAFT",
            "-1/DRAFT",
            "1?origin=https://evil.invalid/DRAFT",
            "1/extra/DRAFT",
            "9223372036854775807/DRAFT",
            "1/UNKNOWN",
            "1/draft",
            "1/DRAFT/extra",
        ] {
            assert!(
                InitialProfileOpen::parse(&format!(
                    "profilebridge://open/v2/tenant_123/actor_123/profile_123/{suffix}"
                ))
                .is_err()
            );
        }
        assert!(
            InitialProfileOpen::parse("profilebridge://open/v1/tenant_123/actor_123/profile_123/1")
                .is_err()
        );
    }
}
