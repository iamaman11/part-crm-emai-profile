#![forbid(unsafe_code)]

mod container;
mod key_derivation;
mod lifecycle;

pub use container::{
    ContainerDigest, GenerationDek, GenerationIdentity, GenerationMetadata,
    InspectedGenerationMetadataPrelude, KeyId, MAX_GENERATION_METADATA_PRELUDE_BYTES,
    MetadataDigest, NoncePrefix, OpenedGeneration, PlaintextDigest, SealedGeneration,
    inspect_generation_metadata_prelude, open_generation, open_generation_expected,
    seal_generation,
};
pub use key_derivation::{
    DerivedGenerationMaterial, GenerationKeyDerivationContext, GenerationKeyDerivationError,
    GenerationRootKey, GenerationRootKeyVersion, derive_generation_material,
};
pub use lifecycle::{
    CloudGenerationRecord, CloudGenerationRepository, CloudGenerationStatus, OrphanPlan,
    PointerSnapshot, PublishResult, RestoreResult, SupportSummary,
};

use core::fmt;
use profile_platform_primitives::{GenerationId, ProfileId, TenantId};

pub const CONTAINER_VERSION: u16 = 1;
pub const ALGORITHM_SUITE: &str = "XCHACHA20-POLY1305-SHA256-V1";
pub const MAX_GENERATION_CONTAINER_BYTES: usize = 83_886_080;

/// Canonical immutable object identity for an encrypted profile generation.
///
/// R2 adapters, Worker validation, and the Bridge reopen path must reuse this owner rather than
/// reconstructing object keys independently.
#[must_use]
pub fn canonical_generation_object_key(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    generation_id: &GenerationId,
) -> String {
    format!(
        "tenants/{}/profiles/{}/generations/{}.bpgc",
        tenant_id.as_str(),
        profile_id.as_str(),
        generation_id.as_str()
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncryptedGenerationError {
    InvalidKeyId,
    InvalidChunkSize,
    PlaintextTooLarge,
    MetadataMismatch,
    InvalidContainer,
    UnsupportedVersion,
    AuthenticationFailed,
    DigestMismatch,
    IdentityMismatch,
    NonceReuse,
    ImmutableConflict,
    MissingObject,
    MissingGeneration,
    GenerationQuarantined,
    GenerationNotVerified,
    StalePointer,
    InvalidRollback,
    VersionOverflow,
    TimeRegression,
}

impl fmt::Display for EncryptedGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidKeyId => "key ID must be an opaque ASCII identifier",
            Self::InvalidChunkSize => "encrypted generation chunk size is outside policy",
            Self::PlaintextTooLarge => "encrypted generation plaintext exceeds policy",
            Self::MetadataMismatch => "generation metadata does not match plaintext or key",
            Self::InvalidContainer => "encrypted generation container is malformed",
            Self::UnsupportedVersion => "encrypted generation version or algorithm is unsupported",
            Self::AuthenticationFailed => "encrypted generation authentication failed",
            Self::DigestMismatch => "encrypted generation plaintext digest does not match metadata",
            Self::IdentityMismatch => "encrypted generation identity does not match the request",
            Self::NonceReuse => "nonce prefix has already been used with this generation key",
            Self::ImmutableConflict => "immutable object already exists with different bytes",
            Self::MissingObject => "encrypted generation object is missing",
            Self::MissingGeneration => "encrypted generation catalog record is missing",
            Self::GenerationQuarantined => "encrypted generation is quarantined",
            Self::GenerationNotVerified => "encrypted generation is not verified",
            Self::StalePointer => "generation pointer compare-and-swap version is stale",
            Self::InvalidRollback => "requested generation is not the retained rollback target",
            Self::VersionOverflow => "generation pointer version overflow",
            Self::TimeRegression => "generation lifecycle time moved backwards",
        })
    }
}

impl std::error::Error for EncryptedGenerationError {}

#[cfg(test)]
mod tests;
