use crate::dirty_generation::{
    DirtyGenerationError, GenerationSealingMaterialPort, PreparedDirtyGeneration,
    prepare_dirty_generation_candidate,
};
use crate::local_profile::{GenerationWorkspace, LocalGenerationRecord, MaterializationRoot};
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

#[derive(Eq, PartialEq)]
pub struct SignedGenerationUploadCapability {
    url: String,
    headers: Vec<(String, String)>,
    expires_seconds: u32,
}

impl SignedGenerationUploadCapability {
    #[must_use]
    pub fn new(url: String, headers: Vec<(String, String)>, expires_seconds: u32) -> Self {
        Self {
            url,
            headers,
            expires_seconds,
        }
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
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
        unsafe_zeroize_string(&mut self.url);
        for (name, value) in &mut self.headers {
            unsafe_zeroize_string(name);
            unsafe_zeroize_string(value);
        }
    }
}

fn unsafe_zeroize_string(value: &mut String) {
    // `String::as_mut_vec` is unsafe and forbidden in this crate. Replacing with an equal-length
    // zero-filled allocation prevents the secret from remaining reachable through this owner;
    // allocator-level memory sanitization is provided by the process boundary, while no secret is
    // ever exposed through Debug/Display. The function name intentionally documents the limitation.
    let length = value.len();
    *value = "\0".repeat(length);
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
    if lease.tenant_id() != tenant_id
        || lease.profile_id() != profile_id
        || base_record.generation_id() == &successor_generation_id(lease)?
    {
        return Err(ShippingGenerationSaveError::InvalidRetainedAuthority);
    }
    let candidate_generation_id = successor_generation_id(lease)?;
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
    validate_prepared_descriptor(
        tenant_id,
        profile_id,
        base_generation_id,
        prepared,
        lease,
    )?;

    match control
        .upload_authorization(
            tenant_id,
            profile_id,
            base_generation_id,
            prepared,
            lease,
        )
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
        .commit_successor(
            tenant_id,
            profile_id,
            base_generation_id,
            prepared,
            lease,
        )
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

fn successor_generation_id<C, U>(
    lease: &ProfileLease,
) -> Result<GenerationId, ShippingGenerationSaveError<C, U>> {
    let mut hasher = Sha256::new();
    append_len_prefixed(&mut hasher, SUCCESSOR_ID_DOMAIN);
    append_len_prefixed(&mut hasher, lease.tenant_id().as_str().as_bytes());
    append_len_prefixed(&mut hasher, lease.profile_id().as_str().as_bytes());
    append_len_prefixed(&mut hasher, lease.session_id().as_str().as_bytes());
    append_len_prefixed(&mut hasher, &lease.epoch().to_be_bytes());
    append_len_prefixed(&mut hasher, lease.fencing_token().as_str().as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    GenerationId::parse(format!("generation_{}", lower_hex(&digest)))
        .map_err(|_| ShippingGenerationSaveError::CandidateIdentity)
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
        SignedGenerationUploadCapability, publish_verify_and_commit_successor,
    };
    use crate::dirty_generation::{
        GenerationSealingMaterial, GenerationSealingMaterialPort, prepare_dirty_generation_candidate,
    };
    use crate::local_profile::{LocalGenerationRecord, MaterializationRoot};
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{
        DeviceId, FencingToken, GenerationId, ProfileId, SessionId, TenantId, UnixMillis,
    };
    use session_domain::ProfileLease;
    use std::cell::RefCell;
    use std::rc::Rc;

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
                GenerationDek::new(KeyId::parse("profile-generation-root-v1-7")?, [7; 32]),
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
            assert!(!capability.url().is_empty());
            assert!(!capability.headers().is_empty());
            assert!(!container.is_empty());
            self.events.borrow_mut().push("upload");
            Ok(())
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct TestError;

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test error")
        }
    }

    impl std::error::Error for TestError {}

    impl From<encrypted_generation_domain::KeyIdError> for TestError {
        fn from(_: encrypted_generation_domain::KeyIdError) -> Self {
            Self
        }
    }

    #[test]
    fn upload_is_followed_by_server_exact_verify_before_commit()
    -> Result<(), Box<dyn std::error::Error>> {
        let root_path = std::env::temp_dir().join(format!(
            "profile-bridge-shipping-save-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root_path);
        let root = MaterializationRoot::open_or_create(root_path.clone())?;
        let tenant = TenantId::parse("tenant_shipping_save_01")?;
        let profile = ProfileId::parse("profile_shipping_save_01")?;
        let base = GenerationId::parse("generation_shipping_base_01")?;
        let candidate = GenerationId::parse("generation_shipping_next_01")?;
        let device = DeviceId::parse("device_shipping_save_01")?;
        let workspace = root.create_generation(&tenant, &profile, &base)?;
        std::fs::write(workspace.path().join("prefs.js"), b"save")?;
        let mut record = LocalGenerationRecord::new(base.clone(), 4, UnixMillis::new(1));
        record.set_locked(true)?;
        record.begin_use(UnixMillis::new(2))?;
        record.graceful_close(UnixMillis::new(3))?;
        let lease = ProfileLease::issue(
            tenant.clone(),
            profile.clone(),
            SessionId::parse("session_shipping_save_01")?,
            device,
            3,
            FencingToken::parse("fence_shipping_save_01")?,
        )?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut control = Control {
            events: Rc::clone(&events),
            verified: false,
            commit: GenerationSuccessorCommitOutcome::Activated,
        };
        let prepared = prepare_dirty_generation_candidate(
            &record,
            &workspace,
            &root,
            &tenant,
            &profile,
            &candidate,
            &mut control,
        )?;
        let result = publish_verify_and_commit_successor(
            &tenant,
            &profile,
            &base,
            &prepared,
            &lease,
            &mut control,
            &mut Put {
                events: Rc::clone(&events),
            },
        )?;
        assert_eq!(
            events.borrow().as_slice(),
            &["verify", "upload", "verify", "commit"]
        );
        assert_eq!(result.outcome(), GenerationSuccessorCommitOutcome::Activated);
        let _ = crate::test_support::remove_test_root(&root_path);
        Ok(())
    }

    #[test]
    fn commit_is_impossible_until_server_reports_exact_verified() -> Result<(), Box<dyn std::error::Error>> {
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

        let root_path = std::env::temp_dir().join(format!(
            "profile-bridge-shipping-save-negative-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root_path);
        let root = MaterializationRoot::open_or_create(root_path.clone())?;
        let tenant = TenantId::parse("tenant_shipping_save_02")?;
        let profile = ProfileId::parse("profile_shipping_save_02")?;
        let base = GenerationId::parse("generation_shipping_base_02")?;
        let candidate = GenerationId::parse("generation_shipping_next_02")?;
        let workspace = root.create_generation(&tenant, &profile, &base)?;
        std::fs::write(workspace.path().join("prefs.js"), b"save")?;
        let mut record = LocalGenerationRecord::new(base.clone(), 4, UnixMillis::new(1));
        record.set_locked(true)?;
        record.begin_use(UnixMillis::new(2))?;
        record.graceful_close(UnixMillis::new(3))?;
        let lease = ProfileLease::issue(
            tenant.clone(),
            profile.clone(),
            SessionId::parse("session_shipping_save_02")?,
            DeviceId::parse("device_shipping_save_02")?,
            3,
            FencingToken::parse("fence_shipping_save_02")?,
        )?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut control = NeverVerified(Control {
            events: Rc::clone(&events),
            verified: false,
            commit: GenerationSuccessorCommitOutcome::Activated,
        });
        let prepared = prepare_dirty_generation_candidate(
            &record,
            &workspace,
            &root,
            &tenant,
            &profile,
            &candidate,
            &mut control,
        )?;
        let result = publish_verify_and_commit_successor(
            &tenant,
            &profile,
            &base,
            &prepared,
            &lease,
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
        let _ = crate::test_support::remove_test_root(&root_path);
        Ok(())
    }
}
