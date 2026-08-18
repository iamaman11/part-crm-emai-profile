use opsctl::{Invocation, d1::D1Action, execute};
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn repo(path: &str) -> PathBuf {
    PathBuf::from(path)
}

fn parse(output: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(output)
}

fn write_fixture(label: &str, value: &Value) -> Result<PathBuf, Box<dyn Error>> {
    let directory = std::env::temp_dir().join(format!("opsctl-d1-matrix-{}", std::process::id()));
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{label}.json"));
    fs::write(&path, serde_json::to_vec_pretty(value)?)?;
    Ok(path)
}

fn ledger(names: &[&str]) -> Value {
    json!({
        "rows": names
            .iter()
            .enumerate()
            .map(|(index, name)| json!({"id": index + 1, "name": name}))
            .collect::<Vec<_>>()
    })
}

fn release(target: &str, minimum: &str, maximum: &str, history: &str, policy: &str) -> Value {
    json!({
        "schema_contract": {
            "database_component": "resolver",
            "target_schema_revision": target,
            "supported_schema_min": minimum,
            "supported_schema_max": maximum,
            "migration_history_digest": history,
            "compatibility_policy_digest": policy
        }
    })
}

fn single_transition_authority(
    migration_class: &str,
    rollout_order: &str,
    fail_forward_required: bool,
    history: &str,
) -> Value {
    json!({
        "kind": "D1_EVOLUTION_AUTHORITY",
        "components": [{
            "component_id": "resolver",
            "historical_epoch": {
                "ordered_history": [{"name": "0001_base.sql"}]
            },
            "post_epoch_migrations": [{
                "component": "resolver",
                "migration_file": "0002_transition.sql",
                "migration_revision": "0002_transition.sql",
                "migration_class": migration_class,
                "rollout_order": rollout_order,
                "fail_forward_required": fail_forward_required,
                "destructive": migration_class == "CONTRACT",
                "code_rollback_allowed": !fail_forward_required && migration_class != "CONTRACT",
                "contract_preconditions": []
            }],
            "current_repository_revision": "0002_transition.sql",
            "history_digest": history
        }]
    })
}

#[allow(clippy::too_many_arguments)]
fn invoke(
    action: D1Action,
    ledger_json: PathBuf,
    release_manifest: Option<PathBuf>,
    current_manifest: Option<PathBuf>,
    known_good_manifest: Option<PathBuf>,
    preconditions_json: Option<PathBuf>,
    authority: Option<PathBuf>,
) -> Result<Value, Box<dyn Error>> {
    let output = execute(Invocation::D1 {
        root: Some(root()),
        action,
        component: "resolver".to_owned(),
        ledger_json,
        release_manifest,
        current_manifest,
        known_good_manifest,
        preconditions_json,
        authority,
    })?;
    Ok(parse(&output)?)
}

fn plan(
    target: &str,
    current: Option<&str>,
    known_good: Option<&str>,
    preconditions: Option<&str>,
) -> Result<Value, Box<dyn Error>> {
    invoke(
        D1Action::Plan,
        repo("tests/d1-evolution/synthetic-ledger-base.json"),
        Some(repo(target)),
        current.map(repo),
        known_good.map(repo),
        preconditions.map(repo),
        Some(repo("tests/d1-evolution/synthetic-authority.json")),
    )
}

#[test]
fn status_covers_exact_and_known_prefix() -> Result<(), Box<dyn Error>> {
    let exact_ledger = write_fixture(
        "status-exact",
        &ledger(&[
            "0001_base.sql",
            "0002_expand.sql",
            "0003_backfill.sql",
            "0004_contract.sql",
        ]),
    )?;
    let exact = invoke(
        D1Action::Status,
        exact_ledger,
        None,
        None,
        None,
        None,
        Some(repo("tests/d1-evolution/synthetic-authority.json")),
    )?;
    assert_eq!(exact["ledger_state"], "EXACT");
    assert_eq!(exact["decision"], "SAFE");
    assert_eq!(exact["allowed"], true);

    let behind = invoke(
        D1Action::Status,
        repo("tests/d1-evolution/synthetic-ledger-base.json"),
        None,
        None,
        None,
        None,
        Some(repo("tests/d1-evolution/synthetic-authority.json")),
    )?;
    assert_eq!(behind["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(behind["decision"], "MIGRATION_REQUIRED");
    assert_eq!(behind["allowed"], true);
    Ok(())
}

#[test]
fn noncanonical_ledgers_all_require_recovery() -> Result<(), Box<dyn Error>> {
    let fixtures = [
        ("DIVERGED", ledger(&["0002_expand.sql"]), "matrix-diverged"),
        (
            "UNKNOWN_MIGRATION",
            ledger(&["0001_base.sql", "9999_unknown.sql"]),
            "matrix-unknown",
        ),
        (
            "CORRUPT_LEDGER",
            ledger(&["0001_base.sql", "0001_base.sql"]),
            "matrix-corrupt",
        ),
    ];
    for (expected_state, document, label) in fixtures {
        let value = invoke(
            D1Action::Status,
            write_fixture(label, &document)?,
            None,
            None,
            None,
            None,
            Some(repo("tests/d1-evolution/synthetic-authority.json")),
        )?;
        assert_eq!(value["ledger_state"], expected_state);
        assert_eq!(value["decision"], "RECOVERY_REQUIRED");
        assert_eq!(value["allowed"], false);
    }
    Ok(())
}

#[test]
fn expand_with_complete_rollback_context_is_migration_required() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-expand.json",
        Some("tests/d1-evolution/synthetic-release-base.json"),
        Some("tests/d1-evolution/synthetic-release-base.json"),
        None,
    )?;
    assert_eq!(value["ledger_state"], "BEHIND_KNOWN_PREFIX");
    assert_eq!(value["decision"], "MIGRATION_REQUIRED");
    assert_eq!(value["rollback_context_complete"], true);
    assert_eq!(value["allowed"], true);
    assert_eq!(value["planned_migrations"], json!(["0002_expand.sql"]));
    assert_eq!(
        value["planned_migration_contracts"][0]["migration_class"],
        "EXPAND"
    );
    Ok(())
}

#[test]
fn backfill_is_an_explicit_planned_contract() -> Result<(), Box<dyn Error>> {
    let ledger_path = write_fixture(
        "through-expand",
        &ledger(&["0001_base.sql", "0002_expand.sql"]),
    )?;
    let target_path = write_fixture(
        "backfill-target",
        &release(
            "0003_backfill.sql",
            "0001_base.sql",
            "0003_backfill.sql",
            "synthetic-history-v1",
            "synthetic-backfill-policy",
        ),
    )?;
    let value = invoke(
        D1Action::Plan,
        ledger_path,
        Some(target_path),
        Some(repo("tests/d1-evolution/synthetic-release-expand.json")),
        Some(repo("tests/d1-evolution/synthetic-release-expand.json")),
        None,
        Some(repo("tests/d1-evolution/synthetic-authority.json")),
    )?;
    assert_eq!(value["decision"], "MIGRATION_REQUIRED");
    assert_eq!(value["allowed"], true);
    assert_eq!(
        value["planned_migration_contracts"][0]["migration_class"],
        "BACKFILL"
    );
    Ok(())
}

#[test]
fn missing_current_runtime_context_blocks_plan() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-expand.json",
        None,
        Some("tests/d1-evolution/synthetic-release-base.json"),
        None,
    )?;
    assert_eq!(value["decision"], "CODE_ROLLBACK_BLOCKED");
    assert_eq!(
        value["reason_codes"],
        json!(["CURRENT_RUNTIME_CONTEXT_MISSING"])
    );
    assert_eq!(value["allowed"], false);
    Ok(())
}

#[test]
fn missing_known_good_runtime_context_blocks_plan() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-expand.json",
        Some("tests/d1-evolution/synthetic-release-base.json"),
        None,
        None,
    )?;
    assert_eq!(value["decision"], "CODE_ROLLBACK_BLOCKED");
    assert_eq!(
        value["reason_codes"],
        json!(["KNOWN_GOOD_RUNTIME_CONTEXT_MISSING"])
    );
    assert_eq!(value["allowed"], false);
    Ok(())
}

#[test]
fn incompatible_current_runtime_requires_recovery() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-expand.json",
        Some("tests/d1-evolution/synthetic-known-good-contract-compatible.json"),
        Some("tests/d1-evolution/synthetic-release-base.json"),
        None,
    )?;
    assert_eq!(value["decision"], "RECOVERY_REQUIRED");
    assert_eq!(
        value["reason_codes"],
        json!(["CURRENT_RUNTIME_ALREADY_SCHEMA_INCOMPATIBLE"])
    );
    assert_eq!(value["allowed"], false);
    Ok(())
}

#[test]
fn contract_without_retirement_evidence_is_blocked() -> Result<(), Box<dyn Error>> {
    let value = plan(
        "tests/d1-evolution/synthetic-release-contract.json",
        Some("tests/d1-evolution/synthetic-release-base.json"),
        Some("tests/d1-evolution/synthetic-known-good-contract-compatible.json"),
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
    assert!(
        value["planned_migration_contracts"]
            .as_array()
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["migration_class"] == "CONTRACT")
            })
    );
    Ok(())
}

#[test]
fn contract_with_complete_evidence_and_compatible_known_good_is_safe() -> Result<(), Box<dyn Error>>
{
    let value = plan(
        "tests/d1-evolution/synthetic-release-contract.json",
        Some("tests/d1-evolution/synthetic-release-base.json"),
        Some("tests/d1-evolution/synthetic-known-good-contract-compatible.json"),
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
        Some("tests/d1-evolution/synthetic-release-base.json"),
        Some("tests/d1-evolution/synthetic-known-good-contract-incompatible.json"),
        Some("tests/d1-evolution/synthetic-contract-preconditions-complete.json"),
    )?;
    assert_eq!(value["decision"], "CODE_ROLLBACK_BLOCKED");
    assert_eq!(value["allowed"], false);
    assert_eq!(
        value["reason_codes"],
        json!(["KNOWN_GOOD_INCOMPATIBLE_AFTER_MIGRATION"])
    );
    Ok(())
}

#[test]
fn repair_fail_forward_transition_is_explicitly_blocked() -> Result<(), Box<dyn Error>>
{
    let authority = write_fixture(
        "repair-authority",
        &single_transition_authority(
            "REPAIR",
            "SEPARATE_CONTRACT_RELEASE",
            true,
            "repair-history",
        ),
    )?;
    let target = write_fixture(
        "repair-target",
        &release(
            "0002_transition.sql",
            "0002_transition.sql",
            "0002_transition.sql",
            "repair-history",
            "repair-target-policy",
        ),
    )?;
    let runtime = write_fixture(
        "repair-runtime",
        &release(
            "0001_base.sql",
            "0001_base.sql",
            "0002_transition.sql",
            "repair-history",
            "repair-runtime-policy",
        ),
    )?;
    let value = invoke(
        D1Action::Plan,
        write_fixture("repair-ledger", &ledger(&["0001_base.sql"]))?,
        Some(target),
        Some(runtime.clone()),
        Some(runtime),
        None,
        Some(authority),
    )?;
    assert_eq!(value["decision"], "FAIL_FORWARD_REQUIRED");
    assert_eq!(value["allowed"], false);
    assert_eq!(
        value["reason_codes"],
        json!(["EXPLICIT_FAIL_FORWARD_TRANSITION"])
    );
    assert_eq!(
        value["planned_migration_contracts"][0]["migration_class"],
        "REPAIR"
    );
    Ok(())
}

#[test]
fn rollout_order_distinguishes_migrate_first_and_deploy_first() -> Result<(), Box<dyn Error>> {
    for (rollout, expected_decision, expected_allowed) in [
        ("MIGRATE_BEFORE_CODE", "MIGRATE_FIRST", true),
        ("CODE_BEFORE_MIGRATE", "DEPLOY_FIRST", false),
    ] {
        let suffix = expected_decision.to_ascii_lowercase();
        let history = format!("{suffix}-history");
        let authority = write_fixture(
            &format!("{suffix}-authority"),
            &single_transition_authority("EXPAND", rollout, false, &history),
        )?;
        let target = write_fixture(
            &format!("{suffix}-target"),
            &release(
                "0002_transition.sql",
                "0001_base.sql",
                "0002_transition.sql",
                &history,
                &format!("{suffix}-target-policy"),
            ),
        )?;
        let runtime = write_fixture(
            &format!("{suffix}-runtime"),
            &release(
                "0001_base.sql",
                "0001_base.sql",
                "0002_transition.sql",
                &history,
                &format!("{suffix}-runtime-policy"),
            ),
        )?;
        let value = invoke(
            D1Action::Plan,
            write_fixture(&format!("{suffix}-ledger"), &ledger(&["0001_base.sql"]))?,
            Some(target),
            Some(runtime.clone()),
            Some(runtime),
            None,
            Some(authority),
        )?;
        assert_eq!(value["decision"], expected_decision);
        assert_eq!(value["allowed"], expected_allowed);
    }
    Ok(())
}

#[test]
fn known_ahead_schema_covers_compatible_incompatible_and_verify_exactness()
-> Result<(), Box<dyn Error>> {
    let compatibility = invoke(
        D1Action::Compatibility,
        repo("tests/d1-evolution/synthetic-ledger-through-backfill.json"),
        Some(repo("tests/d1-evolution/synthetic-release-expand.json")),
        None,
        None,
        None,
        Some(repo("tests/d1-evolution/synthetic-authority.json")),
    )?;
    assert_eq!(compatibility["ledger_state"], "AHEAD_KNOWN_COMPATIBLE");
    assert_eq!(compatibility["decision"], "CODE_ROLLBACK_SAFE");
    assert_eq!(compatibility["allowed"], true);

    let narrow = write_fixture(
        "expand-narrow",
        &release(
            "0002_expand.sql",
            "0001_base.sql",
            "0002_expand.sql",
            "synthetic-history-v1",
            "synthetic-policy-expand-narrow",
        ),
    )?;
    let incompatible = invoke(
        D1Action::Compatibility,
        repo("tests/d1-evolution/synthetic-ledger-through-backfill.json"),
        Some(narrow),
        None,
        None,
        None,
        Some(repo("tests/d1-evolution/synthetic-authority.json")),
    )?;
    assert_eq!(incompatible["ledger_state"], "AHEAD_KNOWN_INCOMPATIBLE");
    assert_eq!(incompatible["decision"], "CODE_ROLLBACK_BLOCKED");
    assert_eq!(incompatible["allowed"], false);

    let verify = invoke(
        D1Action::Verify,
        repo("tests/d1-evolution/synthetic-ledger-through-backfill.json"),
        Some(repo("tests/d1-evolution/synthetic-release-expand.json")),
        None,
        None,
        None,
        Some(repo("tests/d1-evolution/synthetic-authority.json")),
    )?;
    assert_eq!(verify["decision"], "RECOVERY_REQUIRED");
    assert_eq!(verify["allowed"], false);
    assert_eq!(
        verify["reason_codes"],
        json!(["POST_APPLY_TARGET_MISMATCH"])
    );
    Ok(())
}
