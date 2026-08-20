use crate::OpsctlError;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const LIFECYCLE_POLICY: &str = "architecture/lifecycle-projection-policy.json";
const ACCEPTANCE_POLICY: &str = "architecture/architecture-acceptance-policy.json";
const PROGRAM_SEQUENCE: &str = "architecture/architecture-program-sequence.json";
const ACCEPTANCE_DERIVER: &str = ".github/scripts/architecture-acceptance.mjs derive";

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
        && path.join(ACCEPTANCE_POLICY).is_file()
        && path.join(PROGRAM_SEQUENCE).is_file()
        && path.join(LIFECYCLE_POLICY).is_file()
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

pub(crate) fn canonical_json_value(
    root: &Path,
    relative: &str,
    command: &'static str,
) -> Result<Value, OpsctlError> {
    let document = canonical_json_document(root, relative, command)?;
    serde_json::from_str(&document).map_err(|error| {
        OpsctlError::new(
            command,
            format!("canonical JSON authority cannot be parsed: {relative}: {error}"),
        )
    })
}

pub(crate) fn compatibility_projection_view(
    root: &Path,
    command: &'static str,
    relative: &'static str,
    document: String,
) -> Result<String, OpsctlError> {
    let projection: Value = serde_json::from_str(&document).map_err(|error| {
        OpsctlError::new(
            command,
            format!("compatibility projection cannot be parsed: {relative}: {error}"),
        )
    })?;
    let lifecycle = canonical_json_value(root, LIFECYCLE_POLICY, command)?;
    validate_lifecycle_policy_identity(&lifecycle, command)?;
    let snapshots = lifecycle
        .get("tracked_compatibility_snapshots")
        .and_then(Value::as_array)
        .ok_or_else(|| OpsctlError::new(command, "lifecycle projection policy lost snapshot registry"))?;
    let registered = snapshots.iter().any(|entry| {
        entry.get("path").and_then(Value::as_str) == Some(relative)
            && entry.get("classification").and_then(Value::as_str)
                == Some("TRANSITION_PROVENANCE_ONLY")
    });
    if !registered {
        return Err(OpsctlError::new(
            command,
            format!("operator compatibility projection is not registered: {relative}"),
        ));
    }

    let output = json!({
        "schema_version": 1,
        "command": command,
        "mode": "read-only",
        "mutation_executed": false,
        "live_architecture_state": {
            "source": "GIT_DERIVED_AT_READ_TIME",
            "acceptance_policy": ACCEPTANCE_POLICY,
            "program_sequence": PROGRAM_SEQUENCE,
            "deriver": ACCEPTANCE_DERIVER,
            "tracked_mutable_lifecycle_state": false
        },
        "compatibility_projection": {
            "path": relative,
            "classification": "TRANSITION_PROVENANCE_ONLY",
            "authoritative": false,
            "document": projection
        }
    });
    serde_json::to_string_pretty(&output)
        .map(|mut value| {
            value.push('\n');
            value
        })
        .map_err(|error| OpsctlError::new(command, format!("cannot render operator view: {error}")))
}

pub(crate) fn validate_lifecycle_policy_identity(
    lifecycle: &Value,
    command: &'static str,
) -> Result<(), OpsctlError> {
    if lifecycle.get("schema_version").and_then(Value::as_u64) != Some(1)
        || lifecycle.get("kind").and_then(Value::as_str) != Some("LIFECYCLE_PROJECTION_POLICY")
        || lifecycle.get("status").and_then(Value::as_str) != Some("current")
    {
        return Err(OpsctlError::new(
            command,
            "lifecycle projection policy identity/status drifted",
        ));
    }
    let authority = lifecycle.get("live_state_authority").ok_or_else(|| {
        OpsctlError::new(command, "lifecycle projection policy lost live_state_authority")
    })?;
    if authority.get("acceptance_policy").and_then(Value::as_str) != Some(ACCEPTANCE_POLICY)
        || authority.get("program_sequence").and_then(Value::as_str) != Some(PROGRAM_SEQUENCE)
        || authority.get("deriver").and_then(Value::as_str) != Some(ACCEPTANCE_DERIVER)
        || authority
            .get("tracked_mutable_lifecycle_state")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(OpsctlError::new(
            command,
            "lifecycle projection policy no longer delegates live state exclusively to Git-derived acceptance",
        ));
    }
    Ok(())
}
