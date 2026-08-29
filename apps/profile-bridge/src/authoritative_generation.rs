use crate::generation_reopen::{GenerationObjectDownloadPort, GenerationReopenControlPort};
use crate::generation_snapshot::{GenerationSnapshotError, materialize_workspace_snapshot};
use crate::local_profile::{LocalProfileError, MaterializationRoot};
use encrypted_generation_domain::{
    canonical_generation_object_key, inspect_generation_metadata_prelude, open_generation_expected,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthoritativeGenerationMaterializationError {
    Local,
    ControlPlane,
    Download,
    DescriptorMismatch,
    MetadataMismatch,
    OpeningMaterialMismatch,
    Decryption,
    Snapshot,
}

impl core::fmt::Display for AuthoritativeGenerationMaterializationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Local => "authoritative local generation lookup failed",
            Self::ControlPlane => "authoritative generation control-plane request failed",
            Self::Download => "authoritative generation object download failed",
            Self::DescriptorMismatch => "download capability does not name the requested authority",
            Self::MetadataMismatch => "encrypted generation metadata does not match server authority",
            Self::OpeningMaterialMismatch => {
                "opening material does not match the authenticated encrypted metadata"
            }
            Self::Decryption => "authoritative generation decryption failed",
            Self::Snapshot => "authoritative generation snapshot materialization failed",
        })
    }
}

impl std::error::Error for AuthoritativeGenerationMaterializationError {}

/// Ensures that the exact generation already selected by the P2 server-side launch authority is
/// locally materialized. Local absence is never authority to choose another generation: the only
/// recovery path asks the live coordinator for the server-selected active VERIFIED descriptor,
/// downloads that exact immutable object, proves its metadata before requesting the historical
/// DEK, decrypts it with exact identity checks, and delegates BPGW materialization to the one local
/// snapshot/filesystem owner.
pub fn ensure_authoritative_generation<C, D>(
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
    control: &mut C,
    downloader: &mut D,
) -> Result<(), AuthoritativeGenerationMaterializationError>
where
    C: GenerationReopenControlPort,
    D: GenerationObjectDownloadPort,
{
    match root.open_generation(tenant_id, profile_id, generation_id) {
        Ok(_) => return Ok(()),
        Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => {}
        Err(_) => return Err(AuthoritativeGenerationMaterializationError::Local),
    }

    let capability = control
        .download_capability(tenant_id, profile_id)
        .map_err(|_| AuthoritativeGenerationMaterializationError::ControlPlane)?;
    let expected_object_key = canonical_generation_object_key(tenant_id, profile_id, generation_id);
    if capability.generation_id() != generation_id
        || capability.object_key() != expected_object_key
        || capability.container_bytes() == 0
        || capability.expires_seconds() == 0
    {
        return Err(AuthoritativeGenerationMaterializationError::DescriptorMismatch);
    }

    let container = downloader
        .download_generation_object(&capability)
        .map_err(|_| AuthoritativeGenerationMaterializationError::Download)?;
    let inspected = inspect_generation_metadata_prelude(&container)
        .map_err(|_| AuthoritativeGenerationMaterializationError::MetadataMismatch)?;
    let metadata = inspected.metadata();
    if metadata.tenant_id() != tenant_id
        || metadata.profile_id() != profile_id
        || metadata.generation_id() != generation_id
        || metadata.object_key() != expected_object_key
        || inspected.metadata_digest().bytes() != capability.metadata_digest()
    {
        return Err(AuthoritativeGenerationMaterializationError::MetadataMismatch);
    }
    let metadata_prelude = container
        .get(..inspected.prelude_bytes())
        .ok_or(AuthoritativeGenerationMaterializationError::MetadataMismatch)?;
    let opening_key = control
        .opening_material(tenant_id, profile_id, metadata_prelude)
        .map_err(|_| AuthoritativeGenerationMaterializationError::ControlPlane)?;
    if opening_key.key_id() != metadata.key_id() {
        return Err(AuthoritativeGenerationMaterializationError::OpeningMaterialMismatch);
    }

    let opened = open_generation_expected(
        &container,
        &opening_key,
        tenant_id,
        profile_id,
        generation_id,
    )
    .map_err(|_| AuthoritativeGenerationMaterializationError::Decryption)?;
    materialize_workspace_snapshot(root, tenant_id, profile_id, generation_id, opened.plaintext())
        .map_err(|error| match error {
            GenerationSnapshotError::Local(_) => {
                AuthoritativeGenerationMaterializationError::Local
            }
            _ => AuthoritativeGenerationMaterializationError::Snapshot,
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AuthoritativeGenerationMaterializationError, ensure_authoritative_generation,
    };
    use crate::generation_reopen::{
        GenerationDownloadCapability, GenerationObjectDownloadPort, GenerationReopenControlPort,
    };
    use crate::generation_snapshot::encode_workspace_snapshot;
    use crate::local_profile::MaterializationRoot;
    use encrypted_generation_domain::{
        GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
        canonical_generation_object_key, seal_generation,
    };
    use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
    use sha2::{Digest, Sha256};
    use std::fs;
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
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
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
    }

    impl GenerationReopenControlPort for FakeControl {
        type Error = ();

        fn download_capability(
            &mut self,
            tenant_id: &TenantId,
            profile_id: &ProfileId,
        ) -> Result<GenerationDownloadCapability, Self::Error> {
            if tenant_id != &self.tenant_id || profile_id != &self.profile_id {
                return Err(());
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
            if tenant_id != &self.tenant_id || profile_id != &self.profile_id {
                return Err(());
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
        type Error = ();

        fn download_generation_object(
            &mut self,
            capability: &GenerationDownloadCapability,
        ) -> Result<Vec<u8>, Self::Error> {
            let digest: [u8; 32] = Sha256::digest(&self.container).into();
            if digest != capability.container_digest()
                || u64::try_from(self.container.len()).ok() != Some(capability.container_bytes())
            {
                return Err(());
            }
            self.calls += 1;
            Ok(self.container.clone())
        }
    }

    #[test]
    fn missing_local_authority_is_exactly_rematerialized_before_launch()
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
            &mut control,
            &mut downloader,
        )?;
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
                &mut control,
                &mut downloader,
            ),
            Err(AuthoritativeGenerationMaterializationError::MetadataMismatch)
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
                &mut control,
                &mut downloader,
            ),
            Err(AuthoritativeGenerationMaterializationError::DescriptorMismatch)
        );
        assert_eq!(downloader.calls, 0);
        assert_eq!(control.opening_calls, 0);
        fixture.cleanup();
        Ok(())
    }
}
