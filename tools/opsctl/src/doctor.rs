use crate::OpsctlError;
use crate::repository::canonical_json_document;
use serde_json::Value;
use std::fs;
use std::path::Path;

const AUTHORITIES: [&str; 7] = [
    "architecture/inventory.json",
    "architecture/python-estate-ar6.json",
    "architecture/credential-authority.json",
    "architecture/credential-lifecycle.json",
    "architecture/profile-security.json",
    "architecture/operator-contract.json",
    "docs/status.json",
];

const RETAINED_VALIDATORS: [&str; 2] = [
    "scripts/generate-architecture-inventory.py",
    "scripts/python-estate-ar6.py",
];

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    for relative in AUTHORITIES.into_iter().chain(RETAINED_VALIDATORS) {
        require_regular_file(root, relative)?;
    }

    for relative in AUTHORITIES {
        validate_json_authority(root, relative)?;
    }

    Ok("{\"schema_version\":2,\"command\":\"doctor\",\"status\":\"ok\",\"mode\":\"native-read-only\",\"mutation_executed\":false,\"child_processes\":0,\"validators_execution\":\"independent-ci\",\"authorities\":[\"architecture/inventory.json\",\"architecture/python-estate-ar6.json\",\"architecture/credential-authority.json\",\"architecture/credential-lifecycle.json\",\"architecture/profile-security.json\",\"architecture/operator-contract.json\",\"docs/status.json\"]}\n".to_owned())
}

fn require_regular_file(root: &Path, relative: &str) -> Result<(), OpsctlError> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        OpsctlError::new(
            "doctor",
            format!("required canonical file is missing: {relative}: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(OpsctlError::new(
            "doctor",
            format!("required canonical file is not a regular file: {relative}"),
        ));
    }
    Ok(())
}

fn validate_json_authority(root: &Path, relative: &str) -> Result<(), OpsctlError> {
    let document = canonical_json_document(root, relative, "doctor")?;
    let value: Value = serde_json::from_str(&document).map_err(|error| {
        OpsctlError::new(
            "doctor",
            format!("canonical JSON authority cannot be parsed: {relative}: {error}"),
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        OpsctlError::new(
            "doctor",
            format!("canonical JSON authority is not an object: {relative}"),
        )
    })?;
    let schema_version = object
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            OpsctlError::new(
                "doctor",
                format!("canonical JSON authority lacks numeric schema_version: {relative}"),
            )
        })?;
    if schema_version == 0 {
        return Err(OpsctlError::new(
            "doctor",
            format!("canonical JSON authority has invalid schema_version: {relative}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root =
            std::env::temp_dir().join(format!("opsctl-ar10-doctor-{}-{nonce}", std::process::id()));
        for directory in ["architecture", "docs", "scripts"] {
            fs::create_dir_all(root.join(directory))?;
        }
        for relative in [
            "architecture/inventory.json",
            "architecture/python-estate-ar6.json",
            "architecture/credential-authority.json",
            "architecture/credential-lifecycle.json",
            "architecture/profile-security.json",
            "architecture/operator-contract.json",
            "docs/status.json",
        ] {
            fs::write(root.join(relative), b"{\"schema_version\":1}\n")?;
        }
        fs::write(
            root.join("scripts/generate-architecture-inventory.py"),
            b"# retained\n",
        )?;
        fs::write(root.join("scripts/python-estate-ar6.py"), b"# retained\n")?;
        Ok(root)
    }

    #[test]
    fn doctor_is_native_and_does_not_require_validator_execution()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let output = run(&root)?;
        assert!(output.contains("\"child_processes\":0"));
        assert!(output.contains("\"validators_execution\":\"independent-ci\""));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn malformed_authority_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        fs::write(root.join("architecture/inventory.json"), b"{not-json}\n")?;
        assert!(run(&root).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
