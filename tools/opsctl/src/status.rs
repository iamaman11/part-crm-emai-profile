use crate::OpsctlError;
use crate::repository::canonical_json_document;
use std::path::Path;

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    canonical_json_document(root, "docs/status.json", "status")
}
