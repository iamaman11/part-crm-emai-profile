use std::fmt;

mod filesystem;
mod lifecycle;

pub use filesystem::{
    BridgeWorkspaceLock, GenerationInventory, GenerationWorkspace, InventoryEntry,
    MaterializationRoot, RecoveryClone,
};
pub use lifecycle::{
    ForgottenWindowAction, ForgottenWindowPolicy, LocalGenerationRecord, LocalGenerationState,
    QuotaPlan, QuotaPolicy, SupportBundleSummary,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalProfileError {
    RootMustBeAbsolute,
    RootIsSymlink,
    RootIsNotDirectory,
    RootUnmarkedAndNotEmpty,
    RootMarkerMismatch,
    UnsafeRelativePath,
    TargetAlreadyExists,
    GenerationMarkerMissing,
    GenerationMarkerMismatch,
    SymbolicLinkRejected,
    SpecialFileRejected,
    InventoryLimitExceeded,
    InventorySizeOverflow,
    LockBusy,
    LockOwnershipMismatch,
    InvalidPolicy,
    InvalidTransition,
    ClockRegression,
    TimeOverflow,
    SourceChanged,
    CloneChanged,
    Io(std::io::ErrorKind),
}

impl fmt::Display for LocalProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RootMustBeAbsolute => "local profile root must be absolute",
            Self::RootIsSymlink => "local profile root cannot be a symbolic link",
            Self::RootIsNotDirectory => "local profile root must be a directory",
            Self::RootUnmarkedAndNotEmpty => {
                "an existing non-empty local profile root must already be marked"
            }
            Self::RootMarkerMismatch => "local profile root marker does not match",
            Self::UnsafeRelativePath => "inventory contains an unsafe relative path",
            Self::TargetAlreadyExists => "local generation target already exists",
            Self::GenerationMarkerMissing => "local generation marker is missing",
            Self::GenerationMarkerMismatch => "local generation marker does not match",
            Self::SymbolicLinkRejected => "symbolic links are rejected in local generations",
            Self::SpecialFileRejected => "special filesystem entries are rejected",
            Self::InventoryLimitExceeded => "local generation inventory file limit exceeded",
            Self::InventorySizeOverflow => "local generation inventory size overflow",
            Self::LockBusy => "local generation already has a Bridge writer lock",
            Self::LockOwnershipMismatch => "Bridge lock ownership does not match",
            Self::InvalidPolicy => "local lifecycle policy is invalid",
            Self::InvalidTransition => "local generation state transition is invalid",
            Self::ClockRegression => "observed local lifecycle time moved backwards",
            Self::TimeOverflow => "local lifecycle time overflow",
            Self::SourceChanged => "source generation changed during clone creation",
            Self::CloneChanged => "recovery clone no longer matches its accepted inventory",
            Self::Io(_) => "local filesystem operation failed",
        })
    }
}

impl std::error::Error for LocalProfileError {}

impl From<std::io::Error> for LocalProfileError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[cfg(test)]
mod browser_lock_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
