use opsctl::{Invocation, d1::D1Action, execute};
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from("tools/opsctl/tests/fixtures").join(name)
}

fn expect_duplicate_member(invocation: Invocation, member: &str) -> Result<(), String> {
    let message = match execute(invocation) {
        Ok(output) => {
            return Err(format!(
                "duplicate D1 JSON member unexpectedly admitted; output={output}"
            ));
        }
        Err(error) => error.to_string(),
    };
    if !message.contains("strict JSON admission failed") {
        return Err(format!("unexpected D1 admission boundary: {message}"));
    }
    let expected = format!("duplicate JSON object member: {member}");
    if !message.contains(&expected) {
        return Err(format!("duplicate-member proof missing: {message}"));
    }
    Ok(())
}

#[test]
fn d1_ledger_rejects_duplicate_member_before_provider_interpretation() -> Result<(), String> {
    expect_duplicate_member(
        Invocation::D1 {
            root: Some(repo_root()),
            action: D1Action::Status,
            component: "resolver".to_owned(),
            ledger_json: fixture("e3-d1-ledger-duplicate-member.json"),
            release_manifest: None,
            current_manifest: None,
            known_good_manifest: None,
            preconditions_json: None,
        },
        "name",
    )
}

#[test]
fn d1_release_manifest_rejects_duplicate_member_before_typed_interpretation() -> Result<(), String>
{
    expect_duplicate_member(
        Invocation::D1 {
            root: Some(repo_root()),
            action: D1Action::Plan,
            component: "resolver".to_owned(),
            ledger_json: PathBuf::from("tests/d1-evolution/valid-behind-resolver-ledger.json"),
            release_manifest: Some(fixture("e3-d1-release-duplicate-member.json")),
            current_manifest: None,
            known_good_manifest: None,
            preconditions_json: None,
        },
        "target_schema_revision",
    )
}

#[test]
fn d1_preconditions_reject_duplicate_member_before_typed_interpretation() -> Result<(), String> {
    expect_duplicate_member(
        Invocation::D1 {
            root: Some(repo_root()),
            action: D1Action::Plan,
            component: "resolver".to_owned(),
            ledger_json: PathBuf::from("tests/d1-evolution/valid-behind-resolver-ledger.json"),
            release_manifest: Some(PathBuf::from(
                "tests/d1-evolution/resolver-release-compatible.json",
            )),
            current_manifest: None,
            known_good_manifest: None,
            preconditions_json: Some(fixture("e3-d1-preconditions-duplicate-member.json")),
        },
        "completed",
    )
}

#[test]
fn d1_ledger_preserves_provider_owned_extra_field_tolerance_after_strict_parse()
-> Result<(), Box<dyn Error>> {
    let output = execute(Invocation::D1 {
        root: Some(repo_root()),
        action: D1Action::Status,
        component: "resolver".to_owned(),
        ledger_json: fixture("e3-d1-ledger-provider-extra-fields.json"),
        release_manifest: None,
        current_manifest: None,
        known_good_manifest: None,
        preconditions_json: None,
    })?;
    let value: Value = serde_json::from_str(&output)?;
    assert_eq!(value["ledger_state"], "EXACT");
    assert_eq!(value["decision"], "SAFE");
    assert_eq!(value["allowed"], true);
    assert_eq!(value["mutation_executed"], false);
    Ok(())
}
