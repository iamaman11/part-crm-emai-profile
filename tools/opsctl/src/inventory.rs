use crate::OpsctlError;
use crate::repository::{canonical_json_document, compatibility_projection_view};
use std::path::Path;

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    let document = canonical_json_document(root, "architecture/inventory.json", "inventory")?;
    compatibility_projection_view(
        root,
        "inventory",
        "architecture/inventory.json",
        document,
    )
}
