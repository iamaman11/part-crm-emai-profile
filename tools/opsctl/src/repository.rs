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

/// Resolve the deliberately smaller source tree accepted by `d1 repository`.
///
/// Release builders exercise the typed catalog against ephemeral copies of the
/// two canonical migration directories. Those fixtures are D1 repository
/// inputs, not complete application repositories, so they must not be forced to
/// reproduce unrelated architecture sentinels.
pub(crate) fn resolve_d1_repository_root(explicit: Option<&Path>) -> Result<PathBuf, OpsctlError> {
    let Some(root) = explicit else {
        return resolve_repo_root(None, "d1 repository");
    };
    let canonical = fs::canonicalize(root).map_err(|error| {
        OpsctlError::new(
            "d1 repository",
            format!(
                "cannot resolve D1 repository root {}: {error}",
                root.display()
            ),
        )
    })?;
    if canonical.join("migrations/d1").is_dir() && canonical.join("migrations/resolver-d1").is_dir()
    {
        return Ok(canonical);
    }
    Err(OpsctlError::new(
        "d1 repository",
        "explicit D1 repository root lacks the canonical migration directories",
    ))
}

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path.join("architecture/inventory.json").is_file()
        && path
            .join("architecture/credential-authority.json")
            .is_file()
        && path
            .join("architecture/credential-lifecycle.json")
            .is_file()
        && path.join("architecture/profile-security.json").is_file()
        && path
            .join("scripts/generate-architecture-inventory.py")
            .is_file()
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

#[cfg(test)]
mod tests {
    use super::is_repo_root;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "opsctl-n4-repository-root-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("architecture"))?;
        fs::create_dir_all(root.join("scripts"))?;
        for relative in [
            "Cargo.toml",
            "architecture/inventory.json",
            "architecture/credential-authority.json",
            "architecture/credential-lifecycle.json",
            "architecture/profile-security.json",
            "scripts/generate-architecture-inventory.py",
        ] {
            fs::write(root.join(relative), b"sentinel\n")?;
        }
        Ok(root)
    }

    #[test]
    fn repository_root_does_not_require_retired_authority_sentinels()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        assert!(!root.join("architecture/python-estate-ar6.json").exists());
        assert!(!root.join("scripts/python-estate-ar6.py").exists());
        assert!(!root.join("architecture/operator-contract.json").exists());
        assert!(is_repo_root(&root));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
