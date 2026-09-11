#![cfg(test)]

use super::{D1RollbackSchemaDecision, repository_projection, rollback_schema_compatibility};
use opsctl_core::release::SchemaCompatibilityWindow;
use serde_json::Value;
use std::error::Error;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn historical_release_schema(root: &Path) -> Result<SchemaCompatibilityWindow, Box<dyn Error>> {
    let projection: Value = serde_json::from_str(&repository_projection(root)?)?;
    let catalog = projection["components"]
        .as_array()
        .and_then(|components| {
            components
                .iter()
                .find(|component| component["component_id"] == "catalog")
        })
        .ok_or("Catalog projection is missing")?;
    let historical = &catalog["historical_epoch"];
    let revision = historical["final_revision"]
        .as_str()
        .ok_or("historical final revision is missing")?;
    let history_digest = historical["accepted_history_digest"]
        .as_str()
        .ok_or("historical accepted digest is missing")?;
    let policy_digest = catalog["compatibility_policy_digest"]
        .as_str()
        .ok_or("Catalog policy digest is missing")?;
    Ok(SchemaCompatibilityWindow {
        database_component: "catalog".to_owned(),
        target_schema_revision: revision.to_owned(),
        supported_schema_min: revision.to_owned(),
        supported_schema_max: revision.to_owned(),
        migration_history_digest: history_digest.to_owned(),
        compatibility_policy_digest: policy_digest.to_owned(),
    })
}

#[test]
fn historical_runtime_is_present_time_compatible_through_rollback_safe_expand_prefix()
-> Result<(), Box<dyn Error>> {
    let root = repository_root();
    let release = historical_release_schema(&root)?;
    let verdict =
        rollback_schema_compatibility(&root, &release, "0031_device_binding_governance.sql");
    assert_eq!(verdict.decision, D1RollbackSchemaDecision::Compatible);
    assert_eq!(
        verdict.reason_code,
        "D1_PRESENT_TIME_ROLLBACK_SCHEMA_COMPATIBLE"
    );
    Ok(())
}

#[test]
fn historical_runtime_stops_before_fail_forward_contract() -> Result<(), Box<dyn Error>> {
    let root = repository_root();
    let release = historical_release_schema(&root)?;
    let verdict = rollback_schema_compatibility(
        &root,
        &release,
        "0032_pas2_payload_fingerprint_contract.sql",
    );
    assert_eq!(verdict.decision, D1RollbackSchemaDecision::Incompatible);
    assert_eq!(verdict.reason_code, "CATALOG_SCHEMA_UNSUPPORTED");
    Ok(())
}

#[test]
fn historical_runtime_extension_requires_exact_frozen_lineage_identity()
-> Result<(), Box<dyn Error>> {
    let root = repository_root();
    let mut release = historical_release_schema(&root)?;
    release.migration_history_digest =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
    let verdict =
        rollback_schema_compatibility(&root, &release, "0031_device_binding_governance.sql");
    assert_eq!(verdict.decision, D1RollbackSchemaDecision::Incompatible);
    assert_eq!(verdict.reason_code, "CATALOG_SCHEMA_UNSUPPORTED");
    Ok(())
}
