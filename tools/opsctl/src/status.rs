use crate::OpsctlError;
use crate::architecture::evaluate_lifecycle_json;
use std::fs;
use std::path::{Path, PathBuf};

const PROGRAM_SEQUENCE: &str = "architecture/architecture-program-sequence.json";

pub(crate) fn run(
    root: &Path,
    acceptance_evidence_json: &Path,
) -> Result<String, OpsctlError> {
    let sequence = read_regular_file(&root.join(PROGRAM_SEQUENCE), PROGRAM_SEQUENCE)?;
    let evidence_path = resolve_observation_path(root, acceptance_evidence_json);
    let evidence_label = acceptance_evidence_json.display().to_string();
    let evidence = read_regular_file(&evidence_path, &evidence_label)?;
    evaluate_lifecycle_json(&sequence, &evidence)
        .map_err(|error| OpsctlError::new("status", error.to_string()))
}

fn resolve_observation_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn read_regular_file(path: &Path, label: &str) -> Result<String, OpsctlError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        OpsctlError::new(
            "status",
            format!("required lifecycle input is unavailable: {label}: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(OpsctlError::new(
            "status",
            format!("required lifecycle input is not a regular file: {label}"),
        ));
    }
    fs::read_to_string(path).map_err(|error| {
        OpsctlError::new(
            "status",
            format!("cannot read lifecycle input {label}: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::run;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn temp_evidence() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opsctl-pf1-acceptance-evidence-{}-{nonce}.json",
            std::process::id()
        ));
        fs::write(
            &path,
            b"{\"schema_version\":1,\"source_branch\":\"main\",\"acceptance_observations\":[]}\n",
        )?;
        Ok(path)
    }

    #[test]
    fn status_is_derived_from_explicit_evidence() -> Result<(), Box<dyn std::error::Error>> {
        let evidence = temp_evidence()?;
        let output = run(&repository_root(), &evidence)?;
        let parsed: Value = serde_json::from_str(&output)?;
        assert_eq!(parsed["kind"], "DERIVED_LIFECYCLE_STATE");
        assert_eq!(parsed["accepted_checkpoint"], "AR-11");
        assert_eq!(parsed["current_slice"], "AR-12");
        assert_eq!(parsed["production_core_gate"], "BLOCKED");
        fs::remove_file(evidence)?;
        Ok(())
    }
}
