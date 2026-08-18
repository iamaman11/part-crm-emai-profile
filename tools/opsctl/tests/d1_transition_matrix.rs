use opsctl::{Invocation, d1::D1Action, execute};
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn parse(output: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(output)
}

fn plan(
    target: &str,
    current: &str,
    known_good: &str,
    preconditions: Option<&str>,
) -> Result<Value, Box<dyn Error>> {
    let output = execute(Invocation::D1 {
        root: Some(root()),
        action: D1Action::Plan,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/synthetic-ledger-base.json"),
        release_manifest: Some(PathBuf::from(target)),
        current_manifest: Some(PathBuf::from(current)),
        known_good_manifest: Some(PathBuf::from(known_good)),
        preconditions_json: preconditions.map(PathBuf::from),
        authority: Some(PathBuf::from("tests/d1-evolution/synthetic-authority.json")),
    })?;
    Ok(parse(&output)?)
}

#[test]
fn expand_with_complete_rollback_context_is_migration_required() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-expand.json",
        "tests/d1-evolution/synthetic-release-base.json",
        "tests/d1-evolution/synthetic-release-base.json",
        None,
    )?;
    assert_eq!(value["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(value["decision"], "MIGRATION_REQUIRED");
    assert_eq!(value["rollback_context_complete"], true);
    assert_eq!(value["allowed"], true);
    assert_eq!(
        value["planned_migrations"],
        serde_json::json!(["0002_expand.sql"])
    );
    Ok(())
}

#[test]
fn contract_without_retirement_evidence_is_blocked() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-contract.json",
        "tests/d1-evolution/synthetic-release-base.json",
        "tests/d1-evolution/synthetic-known-good-contract-compatible.json",
        Some("tests/d1-evolution/synthetic-contract-preconditions-missing.json"),
    )?;
    assert_eq!(value["decision"], "CONTRACT_BLOCKED");
    assert_eq!(value["allowed"], false);
    assert!(
        value["reason_codes"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| {
                item.as_str()
                    .is_some_and(|text| text.starts_with("CONTRACT_PRECONDITION_MISSING:"))
            }))
    );
    Ok(())
}

#[test]
fn contract_with_complete_evidence_and_compatible_known_good_is_safe() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-contract.json",
        "tests/d1-evolution/synthetic-release-base.json",
        "tests/d1-evolution/synthetic-known-good-contract-compatible.json",
        Some("tests/d1-evolution/synthetic-contract-preconditions-complete.json"),
    )?;
    assert_eq!(value["decision"], "SAFE");
    assert_eq!(value["allowed"], true);
    assert_eq!(value["rollback_context_complete"], true);
    Ok(())
}

#[test]
fn contract_that_strands_known_good_is_blocked() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-contract.json",
        "tests/d1-evolution/synthetic-release-base.json",
        "tests/d1-evolution/synthetic-known-good-contract-incompatible.json",
        Some("tests/d1-evolution/synthetic-contract-preconditions-complete.json"),
    )?;
    assert_eq!(value["decision"], "CODE_ROLLBACK_BLOCKED");
    assert_eq!(value["allowed"], false);
    assert_eq!(
        value["reason_codes"],
        serde_json::json!(["KNOWN_GOOD_INCOMPATIBLE_AFTER_MIGRATION"])
    );
    Ok(())
}

#[test]
fn known_ahead_schema_can_be_runtime_compatible_but_not_post_apply_exact() -> Result<(), Box<dyn Error>> {
    let compatibility = execute(Invocation::D1 {
        root: Some(root()),
        action: D1Action::Compatibility,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/synthetic-ledger-through-backfill.json"),
        release_manifest: Some(PathBuf::from("tests/d1-evolution/synthetic-release-expand.json")),
        current_manifest: None,
        known_good_manifest: None,
        preconditions_json: None,
        authority: Some(PathBuf::from("tests/d1-evolution/synthetic-authority.json")),
    })?;
    let compatibility = parse(&compatibility)?;
    assert_eq!(compatibility["ledger_state"], "AHEAD_KNOWN_COMPATIBLE");
    assert_eq!(compatibility["decision"], "CODE_ROLLBACK_SAFE");
    assert_eq!(compatibility["allowed"], true);

    let verify = execute(Invocation::D1 {
        root: Some(root()),
        action: D1Action::Verify,
        component: "resolver".to_owned(),
        ledger_json: PathBuf::from("tests/d1-evolution/synthetic-ledger-through-backfill.json"),
        release_manifest: Some(PathBuf::from("tests/d1-evolution/synthetic-release-expand.json")),
        current_manifest: None,
        known_good_manifest: None,
        preconditions_json: None,
        authority: Some(PathBuf::from("tests/d1-evolution/synthetic-authority.json")),
    })?;
    let verify = parse(&verify)?;
    assert_eq!(verify["decision"], "RECOVERY_REQUIRED");
    assert_eq!(verify["allowed"], false);
    Ok(())
}
