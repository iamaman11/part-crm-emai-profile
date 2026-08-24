use crate::OpsctlError;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorCheckId {
    WorkspaceManifest,
    CatalogMigrations,
    ResolverMigrations,
}

impl DoctorCheckId {
    const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceManifest => "workspace_manifest",
            Self::CatalogMigrations => "catalog_migrations",
            Self::ResolverMigrations => "resolver_migrations",
        }
    }
}

const CHECKS: [DoctorCheckId; 3] = [
    DoctorCheckId::WorkspaceManifest,
    DoctorCheckId::CatalogMigrations,
    DoctorCheckId::ResolverMigrations,
];

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    require_regular_file(root, "Cargo.toml")?;
    require_directory(root, "migrations/d1")?;
    require_directory(root, "migrations/resolver-d1")?;

    let checks = CHECKS
        .iter()
        .map(|check| format!(r#"{{\"id\":\"{}\",\"status\":\"pass\"}}"#, check.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        r#"{{\"schema_version\":2,\"command\":\"doctor\",\"status\":\"ok\",\"mode\":\"read-only\",\"mutation_executed\":false,\"checks\":[{checks}]}}
"#
    ))
}

fn require_regular_file(root: &Path, relative: &str) -> Result<(), OpsctlError> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        OpsctlError::new(
            "doctor",
            format!("required local file is missing: {relative}: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(OpsctlError::new(
            "doctor",
            format!("required local file is not a regular file: {relative}"),
        ));
    }
    Ok(())
}

fn require_directory(root: &Path, relative: &str) -> Result<(), OpsctlError> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        OpsctlError::new(
            "doctor",
            format!("required local directory is missing: {relative}: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(OpsctlError::new(
            "doctor",
            format!("required local directory is not a directory: {relative}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;
    use serde_json::{Value, json};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("opsctl-pf3-doctor-{nonce}"));
        fs::create_dir_all(root.join("migrations/d1"))?;
        fs::create_dir_all(root.join("migrations/resolver-d1"))?;
        fs::write(root.join("Cargo.toml"), b"[workspace]\n")?;
        Ok(root)
    }

    #[test]
    fn doctor_has_no_semantic_authority_catalog() -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let output = run(&root)?;
        let parsed: Value = serde_json::from_str(&output)?;
        assert_eq!(parsed["schema_version"], 2);
        assert_eq!(parsed["command"], "doctor");
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["mode"], "read-only");
        assert_eq!(parsed["mutation_executed"], false);
        assert_eq!(
            parsed["checks"],
            json!([
                {"id": "workspace_manifest", "status": "pass"},
                {"id": "catalog_migrations", "status": "pass"},
                {"id": "resolver_migrations", "status": "pass"}
            ])
        );
        for forbidden in [
            "authorities",
            "architecture/inventory.json",
            "implementation",
            "child_processes",
            "validators_execution",
        ] {
            assert!(!output.contains(forbidden));
        }
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn missing_required_local_structure_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        fs::remove_dir_all(root.join("migrations/resolver-d1"))?;
        assert!(run(&root).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
