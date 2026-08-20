use crate::OpsctlError;
use crate::repository::{canonical_json_document, compatibility_projection_view};
use std::path::Path;

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    let document = canonical_json_document(root, "docs/status.json", "status")?;
    compatibility_projection_view(root, "status", "docs/status.json", document)
}
