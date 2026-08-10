#![forbid(unsafe_code)]

mod container;
mod lifecycle;

pub use container::{
    ContainerDigest, GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, MetadataDigest,
    NoncePrefix, OpenedGeneration, PlaintextDigest, SealedGeneration, open_generation,
    open_generation_expected, seal_generation,
};
pub use lifecycle::{
    CloudGenerationRecord, CloudGenerationRepository, CloudGenerationStatus, OrphanPlan,
    PointerSnapshot, PublishResult, RestoreResult, SupportSummary,
};

use core::fmt;

pub const CONTAINER_VERSION: u16 = 1;
pub const ALGORITHM_SUITE: &str = "XCHACHA20-POLY1305-SHA256-V1";

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
