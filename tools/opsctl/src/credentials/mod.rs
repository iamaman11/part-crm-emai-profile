use crate::OpsctlError;
use crate::repository::canonical_json_document;
use std::path::Path;

/// Current accepted AR-8/AR-9 metadata projection behind the legacy
/// `credential-lifecycle` CLI spelling. The future nested `credentials status`
/// spelling is intentionally not activated in AR-9.
pub(crate) fn lifecycle(root: &Path) -> Result<String, OpsctlError> {
    canonical_json_document(
        root,
        "architecture/credential-lifecycle.json",
        "credential-lifecycle",
    )
}

/// Current accepted metadata-only rotation/recovery contract behind the legacy
/// `rotation-plan` CLI spelling. AR-13 owns the future rehearsal-backed
/// `credentials rotation-plan` family.
pub(crate) fn rotation_plan(root: &Path) -> Result<String, OpsctlError> {
    canonical_json_document(root, "architecture/operator-contract.json", "rotation-plan")
}

/// Target command family. This is documentation/compile-time structure only;
/// AR-9 does not expose these values as executable CLI actions.
pub const TARGET_COMMANDS: &[&str] = &["status", "readiness", "rotation-plan"];
pub const ACTIVATION_OWNER: &str = "AR-10/AR-13";
