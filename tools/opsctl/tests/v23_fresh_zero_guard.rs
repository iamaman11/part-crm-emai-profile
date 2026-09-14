use opsctl::d1::{self, D1Action, D1RunRequest};
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn unique_temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "opsctl-v23-{label}-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

fn current_catalog_release_contract(root: &std::path::Path) -> Result<Value, Box<dyn Error>> {
    let projection: Value = serde_json::from_str(&d1::repository_projection(root)?)?;
    let components = projection["components"]
        .as_array()
        .ok_or_else(|| io::Error::other("typed D1 repository projection is missing components"))?;
    let catalog = components
        .iter()
        .find(|component| component["component_id"] == "catalog")
        .ok_or_else(|| io::Error::other("typed D1 repository projection is missing Catalog"))?;
    let contract = catalog["release_schema_contract"].clone();
    if !contract.is_object() {
        return Err(io::Error::other(
            "typed Catalog projection is missing release_schema_contract",
        )
        .into());
    }
    Ok(contract)
}

#[test]
fn empty_catalog_ledger_cannot_bypass_historical_compatibility_as_fresh_zero()
-> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let contract = current_catalog_release_contract(&root)?;
    let ledger_path = unique_temp_path("empty-ledger");
    let manifest_path = unique_temp_path("current-contract");

    fs::write(&ledger_path, "{\"rows\":[]}")?;
    fs::write(
        &manifest_path,
        serde_json::to_vec(&json!({ "schema_contract": contract }))?,
    )?;

    let result = d1::run(D1RunRequest {
        root: &root,
        action: D1Action::Plan,
        component: "catalog",
        ledger_json: &ledger_path,
        release_manifest: Some(&manifest_path),
        current_manifest: Some(&manifest_path),
        known_good_manifest: Some(&manifest_path),
        preconditions_json: None,
    });

    let _ = fs::remove_file(&ledger_path);
    let _ = fs::remove_file(&manifest_path);

    let output = result?;
    let value: Value = serde_json::from_str(&output)?;
    assert_eq!(value["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(value["decision"], "MIGRATION_REQUIRED");
    assert_eq!(
        value["reason_codes"],
        json!(["HISTORICAL_COMPATIBILITY_UNKNOWN"])
    );
    assert_eq!(value["allowed"], false);
    assert_eq!(value["mutation_executed"], false);
    assert!(
        value["planned_migrations"]
            .as_array()
            .is_some_and(|planned| !planned.is_empty()),
        "the plan may expose the canonical historical suffix for diagnosis, but must never authorize it"
    );
    Ok(())
}
