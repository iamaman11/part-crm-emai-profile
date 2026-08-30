use crate::dirty_generation::{
    DirtyGenerationError, GenerationSealingMaterialPort, PreparedDirtyGeneration,
    prepare_dirty_generation_candidate,
};
use crate::local_profile::{
    GenerationWorkspace, LocalGenerationRecord, LocalProfileError, MaterializationRoot,
};
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};
use session_domain::ProfileLease;
use sha2::{Digest, Sha256};
use std::fmt;

const SUCCESSOR_ID_DOMAIN: &[u8] = b"profile-generation-successor-id-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationSuccessorCommitOutcome {
    Activated,
    AlreadyActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidSignedGenerationUploadCapability;

impl fmt::Display for InvalidSignedGenerationUploadCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("signed generation upload capability is not canonical UTF-8")
    }
}

impl std::error::Error for InvalidSignedGenerationUploadCapability {}

#[derive(Eq, PartialEq)]
pub struct SignedGenerationUploadCapability {
    url: Vec<u8>,
    headers: Vec<(Vec<u8>, Vec<u8>)>,
    expires_seconds: u32,
}

impl SignedGenerationUploadCapability {
    #[must_use]
    pub fn new(url: String, headers: Vec<(String, String)>, expires_seconds: u32) -> Self {
        Self {
            url: url.into_bytes(),
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.into_bytes(), value.into_bytes()))
                .collect(),
            expires_seconds,
        }
    }

    pub fn url(&self) -> Result<&str, InvalidSignedGenerationUploadCapability> {
        std::str::from_utf8(&self.url).map_err(|_| InvalidSignedGenerationUploadCapability)
    }

    pub fn headers(&self) -> Result<Vec<(&str, &str)>, InvalidSignedGenerationUploadCapability> {
        self.headers
            .iter()
            .map(|(name, value)| {
                Ok((
                    std::str::from_utf8(name)
                        .map_err(|_| InvalidSignedGenerationUploadCapability)?,
                    std::str::from_utf8(value)
                        .map_err(|_| InvalidSignedGenerationUploadCapability)?,
                ))
            })
            .collect()
    }

    #[must_use]
    pub const fn expires_seconds(&self) -> u32 {
        self.expires_seconds
    }
}

impl fmt::Debug for SignedGenerationUploadCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignedGenerationUploadCapability")
            .field("url", &"[REDACTED]")
            .field("headers", &"[REDACTED]")
            .field("expires_seconds", &self.expires_seconds)
            .finish()
    }
}

impl Drop for SignedGenerationUploadCapability {
    fn drop(&mut self) {
        self.url.fill(0);
        for (name, value) in &mut self.headers {
            name.fill(0);
            value.fill(0);
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum GenerationUploadAuthorization {
    Verified,
    UploadRequired(SignedGenerationUploadCapability),
}

pub trait GenerationSuccessorControlPort: GenerationSealingMaterialPort {
    fn upload_authorization(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        prepared: &PreparedDirtyGeneration,
        lease: &ProfileLease,
    ) -> Result<GenerationUploadAuthorization, Self::Error>;

    fn commit_successor(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        prepared: &PreparedDirtyGeneration,
        lease: &ProfileLease,
    ) -> Result<GenerationSuccessorCommitOutcome, Self::Error>;
}

pub trait SignedGenerationObjectPutPort {
    type Error;

    fn put_exact(
        &mut self,
        capability: &SignedGenerationUploadCapability,
        container: &[u8],
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedGenerationSuccessor {
    generation_id: GenerationId,
    object_key: String,
    metadata_digest: String,
    container_digest: String,
    container_bytes: u64,
    outcome: GenerationSuccessorCommitOutcome,
}

impl CommittedGenerationSuccessor {
    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }

    #[must_use]
    pub fn container_digest(&self) -> &str {
        &self.container_digest
    }

    #[must_use]
    pub const fn container_bytes(&self) -> u64 {
        self.container_bytes
    }

    #[must_use]
    pub const fn outcome(&self) -> GenerationSuccessorCommitOutcome {
        self.outcome
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ShippingGenerationSaveError<C, U> {
    InvalidRetainedAuthority,
    CandidateIdentity,
    Prepare(DirtyGenerationError),
    Control(C),
    Upload(U),
    VerificationNotProven,
    DescriptorMismatch,
}

impl<C: fmt::Display, U: fmt::Display> fmt::Display for ShippingGenerationSaveError<C, U> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRetainedAuthority => {
                formatter.write_str("retained dirty save authority is invalid")
            }
            Self::CandidateIdentity => formatter.write_str("successor generation identity failed"),
            Self::Prepare(error) => write!(formatter, "successor preparation failed: {error}"),
            Self::Control(error) => write!(formatter, "successor control-plane failed: {error}"),
            Self::Upload(error) => write!(formatter, "successor signed upload failed: {error}"),
            Self::VerificationNotProven => {
                formatter.write_str("successor upload was not exactly verified by the server")
            }
            Self::DescriptorMismatch => {
                formatter.write_str("committed successor descriptor does not match prepared bytes")
            }
        }
    }
}

impl<C: std::error::Error, U: std::error::Error> std::error::Error
    for ShippingGenerationSaveError<C, U>
{
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_retained_generation_successor<C>(
    base_record: &LocalGenerationRecord,
    source_workspace: &GenerationWorkspace,
    root: &MaterializationRoot,
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    lease: &ProfileLease,
    control: &mut C,
) -> Result<PreparedDirtyGeneration, ShippingGenerationSaveError<C::Error, core::convert::Infallible>>
where
    C: GenerationSuccessorControlPort,
{
    if lease.tenant_id() != tenant_id || lease.profile_id() != profile_id {
        return Err(ShippingGenerationSaveError::InvalidRetainedAuthority);
    }
    let candidate_generation_id = successor_generation_id_for_lease(lease)
        .map_err(|_| ShippingGenerationSaveError::CandidateIdentity)?;
    if base_record.generation_id() == &candidate_generation_id {
        return Err(ShippingGenerationSaveError::InvalidRetainedAuthority);
    }

    match root.reject_generation_for_rematerialization(
        tenant_id,
        profile_id,
        &candidate_generation_id,
    ) {
        Ok(()) | Err(LocalProfileError::Io(std::io::ErrorKind::NotFound)) => {}
        Err(error) => {
            return Err(ShippingGenerationSaveError::Prepare(
                DirtyGenerationError::Local(error),
            ));
        }
    }

    prepare_dirty_generation_candidate(
        base_record,
        source_workspace,
        root,
        tenant_id,
        profile_id,
        &candidate_generation_id,
        control,
    )
    .map_err(ShippingGenerationSaveError::Prepare)
}

#[allow(clippy::too_many_arguments)]
pub fn publish_verify_and_commit_successor<C, U>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    base_generation_id: &GenerationId,
    prepared: &PreparedDirtyGeneration,
    lease: &ProfileLease,
    control: &mut C,
    upload: &mut U,
) -> Result<CommittedGenerationSuccessor, ShippingGenerationSaveError<C::Error, U::Error>>
where
    C: GenerationSuccessorControlPort,
    U: SignedGenerationObjectPutPort,
{
    validate_prepared_descriptor(tenant_id, profile_id, base_generation_id, prepared, lease)?;

    match control
        .upload_authorization(tenant_id, profile_id, base_generation_id, prepared, lease)
        .map_err(ShippingGenerationSaveError::Control)?
    {
        GenerationUploadAuthorization::Verified => {}
        GenerationUploadAuthorization::UploadRequired(capability) => {
            if capability.expires_seconds() == 0 {
                return Err(ShippingGenerationSaveError::VerificationNotProven);
            }
            upload
                .put_exact(&capability, prepared.sealed().container())
                .map_err(ShippingGenerationSaveError::Upload)?;
            if !matches!(
                control
                    .upload_authorization(
                        tenant_id,
                        profile_id,
                        base_generation_id,
                        prepared,
                        lease,
                    )
                    .map_err(ShippingGenerationSaveError::Control)?,
                GenerationUploadAuthorization::Verified
            ) {
                return Err(ShippingGenerationSaveError::VerificationNotProven);
            }
        }
    }

    let outcome = control
        .commit_successor(tenant_id, profile_id, base_generation_id, prepared, lease)
        .map_err(ShippingGenerationSaveError::Control)?;
    let container_bytes = u64::try_from(prepared.sealed().container().len())
        .map_err(|_| ShippingGenerationSaveError::DescriptorMismatch)?;
    Ok(CommittedGenerationSuccessor {
        generation_id: prepared.sealed().metadata().generation_id().clone(),
        object_key: prepared.object_key(),
        metadata_digest: prepared.metadata_digest().to_owned(),
        container_digest: prepared.container_digest().to_owned(),
        container_bytes,
        outcome,
    })
}

fn validate_prepared_descriptor<C, U>(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    base_generation_id: &GenerationId,
    prepared: &PreparedDirtyGeneration,
    lease: &ProfileLease,
) -> Result<(), ShippingGenerationSaveError<C, U>> {
    let metadata = prepared.sealed().metadata();
    if lease.tenant_id() != tenant_id
        || lease.profile_id() != profile_id
        || metadata.tenant_id() != tenant_id
        || metadata.profile_id() != profile_id
        || metadata.base_generation_id() != Some(base_generation_id)
        || lease.epoch() == 0
        || prepared.sealed().container().is_empty()
    {
        return Err(ShippingGenerationSaveError::DescriptorMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CandidateGenerationIdError;

fn successor_generation_id_for_lease(
    lease: &ProfileLease,
) -> Result<GenerationId, CandidateGenerationIdError> {
    let mut hasher = Sha256::new();
    append_len_prefixed(&mut hasher, SUCCESSOR_ID_DOMAIN);
    append_len_prefixed(&mut hasher, lease.tenant_id().as_str().as_bytes());
    append_len_prefixed(&mut hasher, lease.profile_id().as_str().as_bytes());
    append_len_prefixed(&mut hasher, lease.session_id().as_str().as_bytes());
    append_len_prefixed(&mut hasher, &lease.epoch().to_be_bytes());
    append_len_prefixed(&mut hasher, lease.fencing_token().as_str().as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    GenerationId::parse(format!("generation_{}", lower_hex(&digest)))
        .map_err(|_| CandidateGenerationIdError)
}

fn append_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        GenerationSuccessorCommitOutcome, GenerationSuccessorControlPort,
        GenerationUploadAuthorization, ShippingGenerationSaveError, SignedGenerationObjectPutPort,
        SignedGenerationUploadCapability, prepare_retained_generation_successor,
        publish_verify_and_commit_successor,
    };
    use crate::dirty_generation::{
        DirtyGenerationError, GenerationSealingMaterial, GenerationSealingMaterialPort,
        prepare_dirty_generation_candidate,
    };
    use crate::local_profile::{
        BridgeWorkspaceLock, LocalGenerationRecord, LocalProfileError, MaterializationRoot,
    };
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, ProfileId, SessionId, TenantId, UnixMillis,
    };
    use session_domain::ProfileLease;
    use std::cell::RefCell;
    use std::fmt;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct TestError;

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test error")
        }
    }

    impl std::error::Error for TestError {}

    struct Control {
        events: Rc<RefCell<Vec<&'static str>>>,
        verified: bool,
        commit: GenerationSuccessorCommitOutcome,
    }

    impl GenerationSealingMaterialPort for Control {
        type Error = TestError;

        fn material_for(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            _base_generation_id: &GenerationId,
            _generation_id: &GenerationId,
            _plaintext_digest: [u8; 32],
        ) -> Result<GenerationSealingMaterial, Self::Error> {
            Ok(GenerationSealingMaterial::new(
                GenerationDek::new(
                    KeyId::parse("profile-generation-root-v1-7").map_err(|_| TestError)?,
                    [7; 32],
                ),
                NoncePrefix::new([8; 16]),
                4096,
            ))
        }
    }

    impl GenerationSuccessorControlPort for Control {
        fn upload_authorization(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            _base_generation_id: &GenerationId,
            _prepared: &crate::dirty_generation::PreparedDirtyGeneration,
            _lease: &ProfileLease,
        ) -> Result<GenerationUploadAuthorization, Self::Error> {
            self.events.borrow_mut().push("verify");
            if self.verified {
                Ok(GenerationUploadAuthorization::Verified)
            } else {
                self.verified = true;
                Ok(GenerationUploadAuthorization::UploadRequired(
                    SignedGenerationUploadCapability::new(
                        "https://example.invalid/object?signature=redacted".to_owned(),
                        vec![("x-test".to_owned(), "secret".to_owned())],
                        300,
                    ),
                ))
            }
        }

        fn commit_successor(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            _base_generation_id: &GenerationId,
            _prepared: &crate::dirty_generation::PreparedDirtyGeneration,
            _lease: &ProfileLease,
        ) -> Result<GenerationSuccessorCommitOutcome, Self::Error> {
            self.events.borrow_mut().push("commit");
            Ok(self.commit)
        }
    }

    struct Put {
        events: Rc<RefCell<Vec<&'static str>>>,
    }

    impl SignedGenerationObjectPutPort for Put {
        type Error = TestError;

        fn put_exact(
            &mut self,
            capability: &SignedGenerationUploadCapability,
            container: &[u8],
        ) -> Result<(), Self::Error> {
            assert!(!capability.url().map_err(|_| TestError)?.is_empty());
            assert!(!capability.headers().map_err(|_| TestError)?.is_empty());
            assert!(!container.is_empty());
            self.events.borrow_mut().push("upload");
            Ok(())
        }
    }

    struct Fixture {
        root_path: std::path::PathBuf,
        root: MaterializationRoot,
        tenant: TenantId,
        profile: ProfileId,
        base: GenerationId,
        candidate: GenerationId,
        record: LocalGenerationRecord,
        lease: ProfileLease,
    }

    impl Fixture {
        fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-shipping-save-{label}-{}-{counter}",
                std::process::id()
            ));
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant = TenantId::parse(format!("tenant_shipping_save_{counter}"))?;
            let profile = ProfileId::parse(format!("profile_shipping_save_{counter}"))?;
            let base = GenerationId::parse(format!("generation_shipping_base_{counter}"))?;
            let candidate = GenerationId::parse(format!("generation_shipping_next_{counter}"))?;
            let workspace = root.create_generation(&tenant, &profile, &base)?;
            std::fs::write(workspace.path().join("prefs.js"), b"save")?;
            let mut record = LocalGenerationRecord::new(base.clone(), 4, UnixMillis::new(1));
            record.set_locked(true)?;
            record.begin_use(UnixMillis::new(2))?;
            record.graceful_close(UnixMillis::new(3))?;
            let lease = ProfileLease::issue(
                tenant.clone(),
                profile.clone(),
                SessionId::parse(format!("session_shipping_save_{counter}"))?,
                DeviceId::parse(format!("device_shipping_save_{counter}"))?,
                3,
                FencingToken::parse(format!("fence_shipping_save_{counter}"))?,
            )?;
            Ok(Self {
                root_path,
                root,
                tenant,
                profile,
                base,
                candidate,
                record,
                lease,
            })
        }

        fn prepared(
            &self,
            control: &mut impl GenerationSealingMaterialPort<Error = TestError>,
        ) -> Result<crate::dirty_generation::PreparedDirtyGeneration, Box<dyn std::error::Error>>
        {
            let workspace = self
                .root
                .open_generation(&self.tenant, &self.profile, &self.base)?;
            Ok(prepare_dirty_generation_candidate(
                &self.record,
                &workspace,
                &self.root,
                &self.tenant,
                &self.profile,
                &self.candidate,
                control,
            )?)
        }

        fn source_workspace(
            &self,
        ) -> Result<crate::local_profile::GenerationWorkspace, Box<dyn std::error::Error>> {
            Ok(self
                .root
                .open_generation(&self.tenant, &self.profile, &self.base)?)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    #[test]
    fn signed_upload_capability_redacts_debug_and_preserves_canonical_utf8()
    -> Result<(), Box<dyn std::error::Error>> {
        let capability = SignedGenerationUploadCapability::new(
            "https://example.invalid/object?secret=1".to_owned(),
            vec![("x-secret".to_owned(), "value".to_owned())],
            300,
        );
        assert!(capability.url()?.contains("secret=1"));
        assert_eq!(capability.headers()?, vec![("x-secret", "value")]);
        let debug = format!("{capability:?}");
        assert!(!debug.contains("secret=1"));
        assert!(!debug.contains("value"));
        Ok(())
    }

    #[test]
    fn retained_retry_rebuilds_only_the_exact_deterministic_candidate()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new("retry")?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut control = Control {
            events,
            verified: false,
            commit: GenerationSuccessorCommitOutcome::Activated,
        };
        let source = fixture.source_workspace()?;
        let first = prepare_retained_generation_successor(
            &fixture.record,
            &source,
            &fixture.root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.lease,
            &mut control,
        )?;
        let candidate_generation_id = first.sealed().metadata().generation_id().clone();
        std::fs::write(
            first.candidate_workspace().path().join("retry-residue"),
            b"stale-precommit-candidate",
        )?;

        let second = prepare_retained_generation_successor(
            &fixture.record,
            &source,
            &fixture.root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.lease,
            &mut control,
        )?;

        assert_eq!(
            second.sealed().metadata().generation_id(),
            &candidate_generation_id
        );
        assert!(!second.candidate_workspace().path().join("retry-residue").exists());
        assert_eq!(
            second.candidate_workspace().inventory()?,
            *second.candidate_inventory()
        );
        Ok(())
    }

    #[test]
    fn retained_retry_never_replaces_a_writer_owned_candidate()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new("retry-writer")?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut control = Control {
            events,
            verified: false,
            commit: GenerationSuccessorCommitOutcome::Activated,
        };
        let source = fixture.source_workspace()?;
        let first = prepare_retained_generation_successor(
            &fixture.record,
            &source,
            &fixture.root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.lease,
            &mut control,
        )?;
        let lock = BridgeWorkspaceLock::acquire(
            first.candidate_workspace(),
            fixture.lease.device_id(),
            fixture.lease.epoch(),
        )?;

        let retry = prepare_retained_generation_successor(
            &fixture.record,
            &source,
            &fixture.root,
            &fixture.tenant,
            &fixture.profile,
            &fixture.lease,
            &mut control,
        );
        assert!(matches!(
            retry,
            Err(ShippingGenerationSaveError::Prepare(
                DirtyGenerationError::Local(LocalProfileError::LockBusy)
            ))
        ));

        lock.release()?;
        Ok(())
    }

    #[test]
    fn upload_is_followed_by_server_exact_verify_before_commit()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new("positive")?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut control = Control {
            events: Rc::clone(&events),
            verified: false,
            commit: GenerationSuccessorCommitOutcome::Activated,
        };
        let prepared = fixture.prepared(&mut control)?;
        let result = publish_verify_and_commit_successor(
            &fixture.tenant,
            &fixture.profile,
            &fixture.base,
            &prepared,
            &fixture.lease,
            &mut control,
            &mut Put {
                events: Rc::clone(&events),
            },
        )?;
        assert_eq!(
            events.borrow().as_slice(),
            &["verify", "upload", "verify", "commit"]
        );
        assert_eq!(
            result.outcome(),
            GenerationSuccessorCommitOutcome::Activated
        );
        Ok(())
    }

    #[test]
    fn commit_is_impossible_until_server_reports_exact_verified()
    -> Result<(), Box<dyn std::error::Error>> {
        struct NeverVerified(Control);

        impl GenerationSealingMaterialPort for NeverVerified {
            type Error = TestError;

            fn material_for(
                &mut self,
                tenant_id: &TenantId,
                profile_id: &ProfileId,
                base_generation_id: &GenerationId,
                generation_id: &GenerationId,
                plaintext_digest: [u8; 32],
            ) -> Result<GenerationSealingMaterial, Self::Error> {
                self.0.material_for(
                    tenant_id,
                    profile_id,
                    base_generation_id,
                    generation_id,
                    plaintext_digest,
                )
            }
        }

        impl GenerationSuccessorControlPort for NeverVerified {
            fn upload_authorization(
                &mut self,
                _tenant_id: &TenantId,
                _profile_id: &ProfileId,
                _base_generation_id: &GenerationId,
                _prepared: &crate::dirty_generation::PreparedDirtyGeneration,
                _lease: &ProfileLease,
            ) -> Result<GenerationUploadAuthorization, Self::Error> {
                self.0.events.borrow_mut().push("verify");
                Ok(GenerationUploadAuthorization::UploadRequired(
                    SignedGenerationUploadCapability::new(
                        "https://example.invalid/object?signature=redacted".to_owned(),
                        vec![("x-test".to_owned(), "secret".to_owned())],
                        300,
                    ),
                ))
            }

            fn commit_successor(
                &mut self,
                _tenant_id: &TenantId,
                _profile_id: &ProfileId,
                _base_generation_id: &GenerationId,
                _prepared: &crate::dirty_generation::PreparedDirtyGeneration,
                _lease: &ProfileLease,
            ) -> Result<GenerationSuccessorCommitOutcome, Self::Error> {
                self.0.events.borrow_mut().push("commit");
                Ok(self.0.commit)
            }
        }

        let fixture = Fixture::new("negative")?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut control = NeverVerified(Control {
            events: Rc::clone(&events),
            verified: false,
            commit: GenerationSuccessorCommitOutcome::Activated,
        });
        let prepared = fixture.prepared(&mut control)?;
        let result = publish_verify_and_commit_successor(
            &fixture.tenant,
            &fixture.profile,
            &fixture.base,
            &prepared,
            &fixture.lease,
            &mut control,
            &mut Put {
                events: Rc::clone(&events),
            },
        );
        assert!(matches!(
            result,
            Err(ShippingGenerationSaveError::VerificationNotProven)
        ));
        assert_eq!(events.borrow().as_slice(), &["verify", "upload", "verify"]);
        Ok(())
    }
}
