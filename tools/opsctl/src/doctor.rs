use crate::OpsctlError;
use crate::repository::{
    canonical_json_document, canonical_json_value, validate_lifecycle_policy_identity,
};
use serde_json::Value;
use std::fs;
use std::path::Path;

const ACCEPTANCE_POLICY: &str = "architecture/architecture-acceptance-policy.json";
const PROGRAM_SEQUENCE: &str = "architecture/architecture-program-sequence.json";
const LIFECYCLE_POLICY: &str = "architecture/lifecycle-projection-policy.json";
const TRANSITION: &str = "architecture/architecture-rebaseline-v3-transition.json";

const AUTHORITIES: [&str; 11] = [
    "architecture/inventory.json",
    "architecture/python-estate-ar6.json",
    ACCEPTANCE_POLICY,
    PROGRAM_SEQUENCE,
    LIFECYCLE_POLICY,
    TRANSITION,
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
        .map_err(|error| {
            OpsctlError::new("doctor", format!("native doctor contract invalid: {error}"))
        })?;
    if native_contract.get("mode").and_then(Value::as_str) != Some("native-read-only")
        || native_contract
            .get("child_processes")
            .and_then(Value::as_u64)
            != Some(0)
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
    validate_lifecycle_projection_boundary(root)?;

    // Keep the accepted v1 doctor output stable. Native semantic checks may become stronger without
    // granting child-process, provider, credential or mutation authority to opsctl.
    Ok("{\"schema_version\":1,\"command\":\"doctor\",\"status\":\"ok\",\"mode\":\"read-only\",\"mutation_executed\":false,\"authorities\":[\"architecture/inventory.json\",\"architecture/python-estate-ar6.json\",\"architecture/architecture-acceptance-policy.json\",\"architecture/architecture-program-sequence.json\",\"architecture/lifecycle-projection-policy.json\",\"architecture/architecture-rebaseline-v3-transition.json\",\"architecture/credential-authority.json\",\"architecture/credential-lifecycle.json\",\"architecture/profile-security.json\",\"architecture/operator-contract.json\",\"docs/status.json\"]}\n".to_owned())
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

fn validate_lifecycle_projection_boundary(root: &Path) -> Result<(), OpsctlError> {
    let lifecycle = canonical_json_value(root, LIFECYCLE_POLICY, "doctor")?;
    validate_lifecycle_policy_identity(&lifecycle, "doctor")?;

    let consumer = lifecycle.get("consumer_policy").ok_or_else(|| {
        OpsctlError::new("doctor", "lifecycle projection policy lost consumer_policy")
    })?;
    for field in [
        "tracked_snapshot_may_decide_accepted_or_current_slice",
        "tracked_snapshot_may_authorize_production",
        "tracked_snapshot_may_drive_ar12_through_ar17_closeout",
    ] {
        if consumer.get(field).and_then(Value::as_bool) != Some(false) {
            return Err(OpsctlError::new(
                "doctor",
                format!("lifecycle projection policy must keep {field}=false"),
            ));
        }
    }
    for field in [
        "operator_surface_must_label_snapshot_non_authoritative",
        "stable_inventory_generation_must_preserve_snapshot_without_advancing_it",
    ] {
        if consumer.get(field).and_then(Value::as_bool) != Some(true) {
            return Err(OpsctlError::new(
                "doctor",
                format!("lifecycle projection policy must keep {field}=true"),
            ));
        }
    }
    if consumer
        .get("future_acceptance_requires_source_projection_commit")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(OpsctlError::new(
            "doctor",
            "future acceptance must not require a source projection commit",
        ));
    }

    let acceptance = canonical_json_value(root, ACCEPTANCE_POLICY, "doctor")?;
    let projection = acceptance.get("projection_policy").ok_or_else(|| {
        OpsctlError::new("doctor", "architecture acceptance policy lost projection_policy")
    })?;
    if projection.get("lifecycle_policy").and_then(Value::as_str) != Some(LIFECYCLE_POLICY)
        || projection
            .get("tracked_mutable_lifecycle_state_forbidden_as_authority")
            .and_then(Value::as_bool)
            != Some(true)
        || projection
            .get("future_acceptance_requires_source_projection_commit")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(OpsctlError::new(
            "doctor",
            "architecture acceptance policy no longer binds tracked lifecycle projections to Git-derived authority",
        ));
    }

    let status = canonical_json_value(root, "docs/status.json", "doctor")?;
    require_bool(&status, &["production_ready"], false, "docs/status.json production_ready")?;
    require_bool(
        &status,
        &["current", "architecture_complete"],
        false,
        "docs/status.json architecture_complete",
    )?;
    require_text(
        &status,
        &["current", "production_core_gate"],
        "BLOCKED",
        "docs/status.json production_core_gate",
    )?;

    let inventory = canonical_json_value(root, "architecture/inventory.json", "doctor")?;
    let inventory_invariants = ["current_delivery_map", "invariants"];
    require_bool_path_suffix(
        &inventory,
        &inventory_invariants,
        "architecture_complete",
        false,
        "inventory architecture_complete",
    )?;
    require_text_path_suffix(
        &inventory,
        &inventory_invariants,
        "production_core_gate",
        "BLOCKED",
        "inventory production_core_gate",
    )?;
    require_bool_path_suffix(
        &inventory,
        &inventory_invariants,
        "production_ready",
        false,
        "inventory production_ready",
    )?;
    require_bool_path_suffix(
        &inventory,
        &inventory_invariants,
        "production_mutation",
        false,
        "inventory production_mutation",
    )?;

    let transition = canonical_json_value(root, TRANSITION, "doctor")?;
    require_bool(
        &transition,
        &["state_model", "architecture_complete"],
        false,
        "transition architecture_complete",
    )?;
    require_text(
        &transition,
        &["state_model", "production_core_gate"],
        "BLOCKED",
        "transition production_core_gate",
    )?;
    require_bool(
        &transition,
        &["state_model", "production_ready"],
        false,
        "transition production_ready",
    )?;
    Ok(())
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |current, key| current.get(*key))
}

fn require_bool(
    value: &Value,
    path: &[&str],
    expected: bool,
    label: &str,
) -> Result<(), OpsctlError> {
    if value_at(value, path).and_then(Value::as_bool) != Some(expected) {
        return Err(OpsctlError::new(
            "doctor",
            format!("{label} must remain {expected}"),
        ));
    }
    Ok(())
}

fn require_text(
    value: &Value,
    path: &[&str],
    expected: &str,
    label: &str,
) -> Result<(), OpsctlError> {
    if value_at(value, path).and_then(Value::as_str) != Some(expected) {
        return Err(OpsctlError::new(
            "doctor",
            format!("{label} must remain {expected}"),
        ));
    }
    Ok(())
}

fn require_bool_path_suffix(
    value: &Value,
    prefix: &[&str],
    suffix: &str,
    expected: bool,
    label: &str,
) -> Result<(), OpsctlError> {
    let parent = value_at(value, prefix).ok_or_else(|| {
        OpsctlError::new("doctor", format!("{label} parent path is missing"))
    })?;
    require_bool(parent, &[suffix], expected, label)
}

fn require_text_path_suffix(
    value: &Value,
    prefix: &[&str],
    suffix: &str,
    expected: &str,
    label: &str,
) -> Result<(), OpsctlError> {
    let parent = value_at(value, prefix).ok_or_else(|| {
        OpsctlError::new("doctor", format!("{label} parent path is missing"))
    })?;
    require_text(parent, &[suffix], expected, label)
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
            std::env::temp_dir().join(format!("opsctl-post-ar11-doctor-{}-{nonce}", std::process::id()));
        for directory in ["architecture", "docs", "scripts", ".github/scripts"] {
            fs::create_dir_all(root.join(directory))?;
        }

        fs::write(
            root.join("architecture/inventory.json"),
            br#"{"schema_version":3,"current_delivery_map":{"invariants":{"architecture_complete":false,"production_core_gate":"BLOCKED","production_ready":false,"production_mutation":false}}}
"#,
        )?;
        fs::write(
            root.join("docs/status.json"),
            br#"{"schema_version":6,"production_ready":false,"current":{"architecture_complete":false,"production_core_gate":"BLOCKED"}}
"#,
        )?;
        fs::write(
            root.join("architecture/architecture-rebaseline-v3-transition.json"),
            br#"{"schema_version":13,"state_model":{"architecture_complete":false,"production_core_gate":"BLOCKED","production_ready":false}}
"#,
        )?;
        fs::write(
            root.join("architecture/architecture-acceptance-policy.json"),
            br#"{"schema_version":1,"projection_policy":{"lifecycle_policy":"architecture/lifecycle-projection-policy.json","tracked_mutable_lifecycle_state_forbidden_as_authority":true,"future_acceptance_requires_source_projection_commit":false}}
"#,
        )?;
        fs::write(
            root.join("architecture/architecture-program-sequence.json"),
            b"{\"schema_version\":1}\n",
        )?;
        fs::write(
            root.join("architecture/lifecycle-projection-policy.json"),
            br#"{"schema_version":1,"kind":"LIFECYCLE_PROJECTION_POLICY","status":"current","live_state_authority":{"acceptance_policy":"architecture/architecture-acceptance-policy.json","program_sequence":"architecture/architecture-program-sequence.json","deriver":".github/scripts/architecture-acceptance.mjs derive","tracked_mutable_lifecycle_state":false},"tracked_compatibility_snapshots":[],"consumer_policy":{"tracked_snapshot_may_decide_accepted_or_current_slice":false,"tracked_snapshot_may_authorize_production":false,"tracked_snapshot_may_drive_ar12_through_ar17_closeout":false,"operator_surface_must_label_snapshot_non_authoritative":true,"stable_inventory_generation_must_preserve_snapshot_without_advancing_it":true,"future_acceptance_requires_source_projection_commit":false}}
"#,
        )?;
        for relative in [
            "architecture/python-estate-ar6.json",
            "architecture/credential-authority.json",
            "architecture/credential-lifecycle.json",
            "architecture/profile-security.json",
            "architecture/operator-contract.json",
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

    #[test]
    fn premature_production_gate_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;
        fs::write(
            root.join("docs/status.json"),
            br#"{"schema_version":6,"production_ready":false,"current":{"architecture_complete":false,"production_core_gate":"AUTHORIZED"}}
"#,
        )?;
        assert!(run(&root).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
