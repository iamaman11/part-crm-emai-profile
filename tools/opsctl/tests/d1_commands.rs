use opsctl::{Invocation, d1::D1Action, execute};
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn parse_output(output: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(output)
}

#[test]
fn resolver_exact_status_is_deterministic_and_read_only() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let invocation = Invocation::D1 {
        root: Some(root.clone()),
        action: D1Action::Status,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/valid-exact-resolver-ledger.json"),
        release_manifest: None,
        authority: None,
    };
    let first = execute(invocation)?;
    let second = execute(Invocation::D1 {
        root: Some(root),
        action: D1Action::Status,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/valid-exact-resolver-ledger.json"),
        release_manifest: None,
        authority: None,
    })?;
    assert_eq!(first, second);
    let value = parse_output(&first)?;
    assert_eq!(value["ledger_state"], "EXACT");
    assert_eq!(value["decision"], "SAFE");
    assert_eq!(value["mutation_executed"], false);
    assert_eq!(value["component"], "resolver");
    Ok(())
}

#[test]
fn resolver_known_prefix_builds_exact_missing_suffix() -> Result<(), Box<dyn Error>> {
    let output = execute(Invocation::D1 {
        root: Some(repo_root()),
        action: D1Action::Plan,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/valid-behind-resolver-ledger.json"),
        release_manifest: Some(PathBuf::from(
            "tests/d1-evolution/resolver-release-compatible.json",
        )),
        authority: None,
    })?;
    let value = parse_output(&output)?;
    assert_eq!(value["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(value["decision"], "MIGRATION_REQUIRED");
    assert_eq!(
        value["planned_migrations"],
        serde_json::json!([
            "0003_lookup_hmac_versions.sql",
            "0004_refresh_owner_hmac_version.sql"
        ])
    );
    assert_eq!(value["mutation_executed"], false);
    Ok(())
}

#[test]
fn resolver_diverged_ledger_fails_closed_without_mutation() -> Result<(), Box<dyn Error>> {
    let output = execute(Invocation::D1 {
        root: Some(repo_root()),
        action: D1Action::Status,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/invalid-diverged-resolver-ledger.json"),
        release_manifest: None,
        authority: None,
    })?;
    let value = parse_output(&output)?;
    assert_eq!(value["ledger_state"], "DIVERGED");
    assert_eq!(value["decision"], "RECOVERY_REQUIRED");
    assert_eq!(value["allowed"], false);
    assert_eq!(value["mutation_executed"], false);
    Ok(())
}
