use crate::OpsctlError;
use crate::repository::canonical_json_document;
use std::path::Path;

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    canonical_json_document(root, "architecture/inventory.json", "inventory")
}
