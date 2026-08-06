use crate::{BrowserProfile, GenerationVerification, ProfileGeneration};
use core::fmt;
use profile_platform_primitives::{AggregateVersion, GenerationId, ProfileId, TenantId};

const MIN_OBJECT_KEY_LENGTH: usize = 16;
const MAX_OBJECT_KEY_LENGTH: usize = 512;
const SHA256_HEX_LENGTH: usize = 64;
const MIN_VERIFICATION_REFERENCE_LENGTH: usize = 8;
const MAX_VERIFICATION_REFERENCE_LENGTH: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationObjectKey(String);

impl GenerationObjectKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, GenerationRegistryError> {
        let value = value.into();
        let valid_length =
            (MIN_OBJECT_KEY_LENGTH..=MAX_OBJECT_KEY_LENGTH).contains(&value.len());
        let valid_chars = value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':')
        });
        if !valid_length
            || value.starts_with('/')
            || value.contains("..")
            || value.contains('\\')
            || !valid_chars
        {
            return Err(GenerationRegistryError::InvalidObjectKey);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationDigest(String);

impl GenerationDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, GenerationRegistryError> {
        let value = value.into();
        if value.len() != SHA256_HEX_LENGTH
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(GenerationRegistryError::InvalidDigest);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationReference(String);

impl VerificationReference {
    pub fn parse(value: impl Into<String>) -> Result<Self, GenerationRegistryError> {
        let value = value.into();
        let valid_length = (MIN_VERIFICATION_REFERENCE_LENGTH
            ..=MAX_VERIFICATION_REFERENCE_LENGTH)
            .contains(&value.len());
        let valid_chars = value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':')
        });
        if !valid_length || !valid_chars {
            return Err(GenerationRegistryError::InvalidVerificationReference);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationRegistryStatus {
    Registered,
    Verified,
    Quarantined,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationRegistryRecord {
    tenant_id: TenantId,
    profile_id: ProfileId,
    generation_id: GenerationId,
    object_key: GenerationObjectKey,
    metadata_digest: GenerationDigest,
    container_digest: GenerationDigest,
    status: GenerationRegistryStatus,
    version: AggregateVersion,
    verification_reference: Option<VerificationReference>,
}

impl GenerationRegistryRecord {
    #[must_use]
    pub const fn register(
        tenant_id: TenantId,
        profile_id: ProfileId,
        generation_id: GenerationId,
        object_key: GenerationObjectKey,
        metadata_digest: GenerationDigest,
        container_digest: GenerationDigest,
    ) -> Self {
        Self {
            tenant_id,
            profile_id,
            generation_id,
            object_key,
            metadata_digest,
            container_digest,
            status: GenerationRegistryStatus::Registered,
            version: AggregateVersion::INITIAL,
            verification_reference: None,
        }
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &ProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    #[must_use]
    pub const fn object_key(&self) -> &GenerationObjectKey {
        &self.object_key
    }

    #[must_use]
    pub const fn metadata_digest(&self) -> &GenerationDigest {
        &self.metadata_digest
    }

    #[must_use]
    pub const fn container_digest(&self) -> &GenerationDigest {
        &self.container_digest
    }

    #[must_use]
    pub const fn status(&self) -> GenerationRegistryStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    #[must_use]
    pub const fn verification_reference(&self) -> Option<&VerificationReference> {
        self.verification_reference.as_ref()
    }

    pub fn verify(
        &mut self,
        reference: VerificationReference,
    ) -> Result<(), GenerationRegistryError> {
        if self.status != GenerationRegistryStatus::Registered {
            return Err(GenerationRegistryError::InvalidStatusTransition);
        }
        let next_version = self
            .version
            .next()
            .map_err(|_| GenerationRegistryError::VersionOverflow)?;
        self.status = GenerationRegistryStatus::Verified;
        self.version = next_version;
        self.verification_reference = Some(reference);
        Ok(())
    }

    pub fn quarantine_for_profile(
        &mut self,
        profile: &BrowserProfile,
    ) -> Result<(), GenerationRegistryError> {
        if profile.tenant_id() != &self.tenant_id {
            return Err(GenerationRegistryError::TenantMismatch);
        }
        if profile.profile_id() != &self.profile_id {
            return Err(GenerationRegistryError::ProfileMismatch);
        }
        if profile.active_generation_id() == Some(&self.generation_id) {
            return Err(GenerationRegistryError::GenerationActive);
        }
        if self.status == GenerationRegistryStatus::Quarantined {
            return Err(GenerationRegistryError::InvalidStatusTransition);
        }
        let next_version = self
            .version
            .next()
            .map_err(|_| GenerationRegistryError::VersionOverflow)?;
        self.status = GenerationRegistryStatus::Quarantined;
        self.version = next_version;
        Ok(())
    }

    #[must_use]
    pub fn profile_generation(&self) -> ProfileGeneration {
        let verification = match self.status {
            GenerationRegistryStatus::Registered => GenerationVerification::Unverified,
            GenerationRegistryStatus::Verified => GenerationVerification::Verified,
            GenerationRegistryStatus::Quarantined => GenerationVerification::Corrupt,
        };
        ProfileGeneration::new(
            self.tenant_id.clone(),
            self.profile_id.clone(),
            self.generation_id.clone(),
            verification,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationRegistryError {
    InvalidObjectKey,
    InvalidDigest,
    InvalidVerificationReference,
    InvalidStatusTransition,
    TenantMismatch,
    ProfileMismatch,
    GenerationActive,
    VersionOverflow,
}

impl fmt::Display for GenerationRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidObjectKey => "generation object key is invalid",
            Self::InvalidDigest => "generation digest must be lowercase SHA-256 hex",
            Self::InvalidVerificationReference => "verification reference is invalid",
            Self::InvalidStatusTransition => "generation registry transition is invalid",
            Self::TenantMismatch => "generation registry tenant differs from profile tenant",
            Self::ProfileMismatch => "generation registry profile differs from profile",
            Self::GenerationActive => "active profile generation cannot be quarantined",
            Self::VersionOverflow => "generation registry version overflow",
        })
    }
}

impl std::error::Error for GenerationRegistryError {}

#[cfg(test)]
mod tests {
    use super::{
        GenerationDigest, GenerationObjectKey, GenerationRegistryError, GenerationRegistryRecord,
        GenerationRegistryStatus, VerificationReference,
    };
    use crate::{BrowserProfile, ProfileError, ProfileStatus};
    use profile_platform_primitives::{AggregateVersion, GenerationId, ProfileId, TenantId};

    const DIGEST_A: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST_B: &str =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn profile() -> Result<BrowserProfile, Box<dyn std::error::Error>> {
        Ok(BrowserProfile::create(
            TenantId::parse("tenant_01JREGISTRY")?,
            ProfileId::parse("profile_01JREGISTRY")?,
        ))
    }

    fn record(
        profile: &BrowserProfile,
    ) -> Result<GenerationRegistryRecord, Box<dyn std::error::Error>> {
        Ok(GenerationRegistryRecord::register(
            profile.tenant_id().clone(),
            profile.profile_id().clone(),
            GenerationId::parse("generation_01JREGISTRY")?,
            GenerationObjectKey::parse("profiles/v1/object_01JREGISTRY")?,
            GenerationDigest::parse(DIGEST_A)?,
            GenerationDigest::parse(DIGEST_B)?,
        ))
    }

    #[test]
    fn metadata_boundaries_reject_paths_and_noncanonical_digests() {
        assert!(GenerationObjectKey::parse("../profile/object").is_err());
        assert!(GenerationObjectKey::parse("/absolute/profile/object").is_err());
        assert!(GenerationObjectKey::parse("profile\\object_bad").is_err());
        assert!(GenerationDigest::parse(DIGEST_A.to_uppercase()).is_err());
        assert!(GenerationDigest::parse("abc").is_err());
        assert!(VerificationReference::parse("review reference").is_err());
    }

    #[test]
    fn only_verified_registry_record_activates_profile()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = profile()?;
        let mut record = record(&profile)?;
        assert_eq!(
            profile.activate_generation(&record.profile_generation()),
            Err(ProfileError::GenerationNotVerified)
        );
        record.verify(VerificationReference::parse("review:01JREGISTRY")?)?;
        profile.activate_generation(&record.profile_generation())?;
        assert_eq!(profile.status(), ProfileStatus::Ready);
        assert_eq!(
            profile.active_generation_id(),
            Some(record.generation_id())
        );
        Ok(())
    }

    #[test]
    fn active_generation_cannot_be_quarantined() -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = profile()?;
        let mut record = record(&profile)?;
        record.verify(VerificationReference::parse("review:01JREGISTRY")?)?;
        profile.activate_generation(&record.profile_generation())?;
        assert_eq!(
            record.quarantine_for_profile(&profile),
            Err(GenerationRegistryError::GenerationActive)
        );
        assert_eq!(record.status(), GenerationRegistryStatus::Verified);
        Ok(())
    }

    #[test]
    fn verification_overflow_is_atomic() -> Result<(), Box<dyn std::error::Error>> {
        let profile = profile()?;
        let mut record = record(&profile)?;
        record.version = AggregateVersion::new(u64::MAX)?;
        assert_eq!(
            record.verify(VerificationReference::parse("review:01JREGISTRY")?),
            Err(GenerationRegistryError::VersionOverflow)
        );
        assert_eq!(record.status(), GenerationRegistryStatus::Registered);
        assert!(record.verification_reference().is_none());
        Ok(())
    }

    #[test]
    fn quarantine_overflow_is_atomic() -> Result<(), Box<dyn std::error::Error>> {
        let profile = profile()?;
        let mut record = record(&profile)?;
        record.version = AggregateVersion::new(u64::MAX)?;
        assert_eq!(
            record.quarantine_for_profile(&profile),
            Err(GenerationRegistryError::VersionOverflow)
        );
        assert_eq!(record.status(), GenerationRegistryStatus::Registered);
        Ok(())
    }
}
