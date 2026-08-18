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

fn status_invocation(root: PathBuf, ledger: &str) -> Invocation {
    Invocation::D1 {
        root: Some(root),
        action: D1Action::Status,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from(ledger),
        release_manifest: None,
        current_manifest: None,
        known_good_manifest: None,
        preconditions_json: None,
        authority: None,
    }
}

#[test]
fn resolver_exact_status_is_deterministic_and_read_only() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    let first = execute(status_invocation(
        root.clone(),
        "tests/d1-evolution/valid-exact-resolver-ledger.json",
    ))?;
    let second = execute(status_invocation(
        root,
        "tests/d1-evolution/valid-exact-resolver-ledger.json",
    ))?;
    assert_eq!(first, second);
    let value = parse_output(&first)?;
    assert_eq!(value["ledger_state"], "EXACT");
    assert_eq!(value["decision"], "SAFE");
    assert_eq!(value["mutation_executed"], false);
    assert_eq!(value["component"], "resolver");
    Ok(())
}

#[test]
fn resolver_historical_prefix_is_plannable_but_not_auto_mutable() -> Result<(), Box<dyn Error>> {
    let output = execute(Invocation::D1 {
        root: Some(repo_root()),
        action: D1Action::Plan,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/valid-behind-resolver-ledger.json"),
        release_manifest: Some(PathBuf::from(
            "tests/d1-evolution/resolver-release-compatible.json",
        )),
        current_manifest: None,
        known_good_manifest: None,
        preconditions_json: None,
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
    assert_eq!(
        value["reason_codes"],
        serde_json::json!(["HISTORICAL_COMPATIBILITY_UNKNOWN"])
    );
    assert_eq!(value["allowed"], false);
    assert_eq!(value["mutation_executed"], false);
    Ok(())
}

#[test]
fn resolver_diverged_ledger_fails_closed_without_mutation() -> Result<(), Box<dyn Error>> {
    let output = execute(status_invocation(
        repo_root(),
        "tests/d1-evolution/invalid-diverged-resolver-ledger.json",
    ))?;
    let value = parse_output(&output)?;
    assert_eq!(value["ledger_state"], "DIVERGED");
    assert_eq!(value["decision"], "RECOVERY_REQUIRED");
    assert_eq!(value["allowed"], false);
    assert_eq!(value["mutation_executed"], false);
    Ok(())
}
