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
const INTERNAL_NATIVE_IMPLEMENTATION_CONTRACT: &str =
    "{\"mode\":\"native-read-only\",\"child_processes\":0}";

pub(crate) fn run(root: &Path) -> Result<String, OpsctlError> {
    let native_contract: Value = serde_json::from_str(INTERNAL_NATIVE_IMPLEMENTATION_CONTRACT)
        .map_err(|error| OpsctlError::new("doctor", format!("native doctor contract invalid: {error}")))?;
    if native_contract.get("mode").and_then(Value::as_str) != Some("native-read-only")
        || native_contract.get("child_processes").and_then(Value::as_u64) != Some(0)
    {
        return Err(OpsctlError::new(
            "doctor",
            "native doctor implementation contract is invalid",
        ));
    }

    for relative in AUTHORITIES.into_iter().chain(RETAINED_VALIDATORS) {
        require_regular_file(root, relative)?;
    }

    for relative in AUTHORITIES {
        validate_json_authority(root, relative)?;
    }

    // AR-10 removes the implementation-time Python child-process bridge without changing the
    // accepted AR-6 read-only machine-output contract. The implementation detail is proved by
    // Rust/static tests rather than by expanding this public JSON shape.
    Ok("{\"schema_version\":1,\"command\":\"doctor\",\"status\":\"ok\",\"mode\":\"read-only\",\"mutation_executed\":false,\"authorities\":[\"architecture/inventory.json\",\"architecture/python-estate-ar6.json\",\"architecture/credential-authority.json\",\"architecture/credential-lifecycle.json\",\"architecture/profile-security.json\",\"architecture/operator-contract.json\",\"docs/status.json\"]}\n".to_owned())
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
    use serde_json::Value;
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
    fn doctor_is_native_but_preserves_the_accepted_v1_read_only_output()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        let output = run(&root)?;
        let parsed: Value = serde_json::from_str(&output)?;
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["command"], "doctor");
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["mode"], "read-only");
        assert_eq!(parsed["mutation_executed"], false);
        assert!(parsed.get("implementation").is_none());
        assert!(parsed.get("child_processes").is_none());
        assert!(parsed.get("validators_execution").is_none());
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
