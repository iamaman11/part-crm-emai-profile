use crate::OpsctlError;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn resolve_repo_root(
    explicit: Option<&Path>,
    command: &'static str,
) -> Result<PathBuf, OpsctlError> {
    if let Some(root) = explicit {
        let canonical = fs::canonicalize(root).map_err(|error| {
            OpsctlError::new(
                command,
                format!("cannot resolve repository root {}: {error}", root.display()),
            )
        })?;
        if is_repo_root(&canonical) {
            return Ok(canonical);
        }
        return Err(OpsctlError::new(
            command,
            "explicit path is not the canonical repository root",
        ));
    }

    let current = fs::canonicalize(
        env::current_dir().map_err(|error| OpsctlError::new(command, error.to_string()))?,
    )
    .map_err(|error| OpsctlError::new(command, error.to_string()))?;
    for candidate in current.ancestors() {
        if is_repo_root(candidate) {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(OpsctlError::new(
        command,
        "repository root not found; provide --root PATH",
    ))
}

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("architecture/inventory.json").is_file()
        && path.join("architecture/python-estate-ar6.json").is_file()
        && path
            .join("architecture/credential-authority.json")
            .is_file()
        && path
            .join("architecture/credential-lifecycle.json")
            .is_file()
        && path.join("architecture/profile-security.json").is_file()
        && path.join("architecture/operator-contract.json").is_file()
        && path
            .join("scripts/generate-architecture-inventory.py")
            .is_file()
        && path.join("scripts/python-estate-ar6.py").is_file()
}

pub(crate) fn canonical_json_document(
    root: &Path,
    relative: &str,
    command: &'static str,
) -> Result<String, OpsctlError> {
    let path = root.join(relative);
    let mut contents = fs::read_to_string(&path)
        .map_err(|error| OpsctlError::new(command, format!("cannot read {relative}: {error}")))?;
    let trimmed = contents.trim_start();
    if !trimmed.starts_with('{') || !contents.trim_end().ends_with('}') {
        return Err(OpsctlError::new(
            command,
            format!("canonical JSON authority is malformed: {relative}"),
        ));
    }
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    Ok(contents)
}
