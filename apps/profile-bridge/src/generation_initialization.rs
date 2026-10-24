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

pub const INITIAL_OPEN_URI_PREFIX: &str = "profilebridge://open/v1/";
static SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialProfileOpen {
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub profile_id: ProfileId,
    pub expected_version: u64,
}
impl InitialProfileOpen {
    pub fn parse(uri: &str) -> Result<Self, BridgePortError> {
        let rest = uri
            .strip_prefix(INITIAL_OPEN_URI_PREFIX)
            .ok_or(BridgePortError::InvalidResponse)?;
        let parts: Vec<_> = rest.split('/').collect();
        if parts.len() != 4
            || !parts[3].bytes().all(|c| c.is_ascii_digit())
            || parts[3].starts_with('0')
        {
            return Err(BridgePortError::InvalidResponse);
        }
        let expected_version = parts[3]
            .parse::<u64>()
            .map_err(|_| BridgePortError::InvalidResponse)?;
        if expected_version == 0 || expected_version >= i64::MAX as u64 {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(Self {
            tenant_id: TenantId::parse(parts[0]).map_err(|_| BridgePortError::InvalidResponse)?,
            actor_id: ActorId::parse(parts[1]).map_err(|_| BridgePortError::InvalidResponse)?,
            profile_id: ProfileId::parse(parts[2]).map_err(|_| BridgePortError::InvalidResponse)?,
            expected_version,
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
    Verification,
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
    let bundle = bundles
        .select_bundle()
        .map_err(|_| InitialProfileError::RuntimeBundle)?;
    let workspace = root
        .create_generation(&intent.tenant_id, &intent.profile_id, &generation)
        .map_err(|_| InitialProfileError::Snapshot)?;
    // Local initialization owns an unpublished directory, not a fabricated coordinator lease.
    let writer = BridgeWorkspaceLock::acquire(&workspace, device, 1)
        .map_err(|_| InitialProfileError::Snapshot)?;
    let mut committed = false;
    let result = (|| {
        let browser_identity = materialize(&workspace, &bundle)?;
        persist_browser_identity(&workspace, &browser_identity)
            .map_err(|_| InitialProfileError::Snapshot)?;
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
        let digest = PlaintextDigest::calculate(&plaintext).bytes();
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
            GenerationIdentity::new(
                intent.tenant_id.clone(),
                intent.profile_id.clone(),
                generation.clone(),
                None,
            ),
            dek.key_id().clone(),
            nonce_prefix,
            chunk_size,
            &plaintext,
        )
        .map_err(|_| InitialProfileError::Encryption)?;
        let sealed = seal_generation(&metadata, &dek, &plaintext)
            .map_err(|_| InitialProfileError::Encryption)?;
        if PlaintextDigest::calculate(&plaintext).bytes() != digest {
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
        let verification: BridgeGenerationUploadCapabilityResponse =
            decode(post(transport, &path, &upload)?)?;
        if decode_upload_authorization::<core::convert::Infallible>(verification)
            .map_err(|_| InitialProfileError::Verification)?
            != GenerationUploadAuthorization::Verified
        {
            return Err(InitialProfileError::Verification);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
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
            "profilebridge://open/v1/tenant_123/actor_123/profile_123/1",
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
            "profilebridge://open/v1/tenant_initial_retain/actor_initial_retain/profile_initial_retain/1",
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
        )?;
        assert_eq!(
            replay,
            ClaimUri::parse("profilebridge://claim/claim_ready_0123456789abcdef0123456789")?,
        );
        assert!(workspace.path().exists());
        fs::remove_dir_all(&root_path)?;
        Ok(())
    }

    #[test]
    fn first_open_uri_has_only_typed_locators_and_a_canonical_version() {
        assert!(
            InitialProfileOpen::parse("profilebridge://open/v1/tenant_123/actor_123/profile_123/1")
                .is_ok()
        );
        for suffix in [
            "0",
            "01",
            "-1",
            "1?origin=https://evil.invalid",
            "1/extra",
            "9223372036854775807",
        ] {
            assert!(
                InitialProfileOpen::parse(&format!(
                    "profilebridge://open/v1/tenant_123/actor_123/profile_123/{suffix}"
                ))
                .is_err()
            );
        }
    }
}
