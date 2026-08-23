use crate::OpsctlError;
use crate::repository::canonical_json_document;
use std::path::Path;

/// Accepted metadata-only credential lifecycle projection behind
/// `opsctl credentials status`.
pub(crate) fn lifecycle(root: &Path) -> Result<String, OpsctlError> {
    canonical_json_document(
        root,
        "architecture/credential-lifecycle.json",
        "credential-lifecycle",
    )
}

/// Metadata-only rotation/recovery view behind `opsctl credentials rotation-plan`.
///
/// The bounded credential lifecycle contract owns these semantics. This command does
/// not rotate credentials, execute a provider call, or own a second operator policy.
pub(crate) fn rotation_plan(root: &Path) -> Result<String, OpsctlError> {
    canonical_json_document(
        root,
        "architecture/credential-lifecycle.json",
        "rotation-plan",
    )
}

/// AR-10 activates only metadata reads in the modular credentials namespace.
pub const ACTIVE_METADATA_COMMANDS: &[&str] = &["status", "rotation-plan"];

/// AR-13 owns the first rehearsal-backed readiness/rotation operational semantics.
pub const DEFERRED_OPERATIONAL_COMMANDS: &[&str] = &["readiness"];
pub const DEFERRED_OPERATIONAL_OWNER: &str = "AR-13";
