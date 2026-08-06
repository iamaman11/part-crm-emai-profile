use std::path::Path;

pub fn unsafe_repair(workspace: &Path) {
    let _ = std::fs::remove_file(workspace.join(".parentlock"));
}
