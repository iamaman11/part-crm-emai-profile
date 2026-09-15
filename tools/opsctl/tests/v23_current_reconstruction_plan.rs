use opsctl::d1::{
    D1CurrentReconstructionPlanRequest, current_reconstruction_plan, repository_projection,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "opsctl-v23b-current-reconstruction-{name}-{}",
        std::process::id()
    ))
}

fn current_repository(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&repository_projection(root)?)?)
}

fn catalog_release_contract(repository: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    repository
        .get("components")
        .and_then(Value::as_array)
        .and_then(|components| {
            components.iter().find(|component| {
                component.get("component_id").and_then(Value::as_str) == Some("catalog")
            })
        })
        .and_then(|component| component.get("release_schema_contract"))
        .cloned()
        .ok_or_else(|| {
            std::io::Error::other("typed repository projection is missing Catalog release contract")
                .into()
        })
}

fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec(value)?)?;
    Ok(())
}

fn build_plan(
    root: &Path,
    ledger: &Path,
    release: &Path,
    target: &Path,
) -> Result<String, opsctl::d1::D1Error> {
    let source_sha = "11".repeat(20);
    let tree_sha = "22".repeat(20);
    let release_set_id = format!("release-set-v3-sha256-{}", "33".repeat(32));
    current_reconstruction_plan(D1CurrentReconstructionPlanRequest {
        root,
        ledger_json: ledger,
        release_manifest: release,
        target_json: target,
        source_sha: &source_sha,
        tree_sha: &tree_sha,
        release_set_id: &release_set_id,
        observed_at_unix_seconds: 1_789_410_000,
        observation_source: "hosted-read-only-staging-observation",
    })
}

#[test]
fn current_empty_staging_target_gets_one_deterministic_non_mutating_plan()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let repository = current_repository(&root)?;
    let release_contract = catalog_release_contract(&repository)?;
    let ledger = fixture_path("positive-ledger.json");
    let release = fixture_path("positive-release.json");
    let target = fixture_path("positive-target.json");
    write_json(&ledger, &json!({"rows": []}))?;
    write_json(&release, &json!({"schema_contract": release_contract}))?;
    write_json(
        &target,
        &json!({
            "environment": "staging",
            "account_id": "account-current",
            "database_name": "catalog-staging",
            "database_id": "database-current"
        }),
    )?;

    let first = build_plan(&root, &ledger, &release, &target)?;
    let second = build_plan(&root, &ledger, &release, &target)?;
    assert_eq!(first, second);
    let projection: Value = serde_json::from_str(&first)?;
    assert_eq!(projection["status"], "RECONSTRUCTION_PREPARED");
    assert_eq!(projection["mode"], "read-only");
    assert_eq!(projection["authorization_required"], true);
    assert_eq!(projection["authorization_consumed"], false);
    assert_eq!(projection["mutation_executed"], false);
    assert_eq!(projection["provider_mutation_executed"], false);
    assert_eq!(projection["plan"]["disposition"], "PREPROD_BASELINE_DRIFT");
    assert_eq!(projection["plan"]["component"], "catalog");
    assert_eq!(
        projection["plan"]["provider_observation"]["target"]["environment"],
        "staging"
    );
    assert_eq!(
        projection["plan"]["allowed_provider_effects"],
        json!(["D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION"])
    );
    let forbidden = projection["plan"]["forbidden_provider_effects"]
        .as_array()
        .ok_or_else(|| std::io::Error::other("forbidden_provider_effects must be an array"))?;
    for effect in [
        "D1_MIGRATIONS_APPLY_EXACT_PLAN",
        "D1_CREATE",
        "D1_DELETE",
        "D1_TIME_TRAVEL_RESTORE",
        "RESOURCE_AUTO_PROVISION",
        "PRODUCTION_MUTATION",
    ] {
        assert!(forbidden.iter().any(|value| value.as_str() == Some(effect)));
    }
    assert_eq!(
        projection["plan"]["repository_identity_sha256"],
        repository["repository_identity_sha256"]
    );
    assert_eq!(
        projection["plan"]["construction_sha256"],
        repository["fresh_zero_construction"]["construction_sha256"]
    );
    let target_schema =
        repository["fresh_zero_construction"]["construction"]["target_schema_revision"].clone();
    assert_eq!(projection["plan"]["target_schema_revision"], target_schema);
    assert_eq!(
        projection["plan"]["expected_post_state"]["target_schema_revision"],
        target_schema
    );

    for path in [&ledger, &release, &target] {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[test]
fn reconstruction_rejects_production_nonempty_and_historical_target()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let repository = current_repository(&root)?;
    let release_contract = catalog_release_contract(&repository)?;
    let ledger = fixture_path("negative-ledger.json");
    let release = fixture_path("negative-release.json");
    let target = fixture_path("negative-target.json");
    write_json(&ledger, &json!({"rows": []}))?;
    write_json(
        &release,
        &json!({"schema_contract": release_contract.clone()}),
    )?;
    write_json(
        &target,
        &json!({
            "environment": "production",
            "account_id": "account-current",
            "database_name": "catalog-production",
            "database_id": "database-production"
        }),
    )?;
    assert!(build_plan(&root, &ledger, &release, &target).is_err());

    write_json(
        &target,
        &json!({
            "environment": "staging",
            "account_id": "account-current",
            "database_name": "catalog-staging",
            "database_id": "database-current"
        }),
    )?;
    write_json(
        &ledger,
        &json!({"rows": [{"id": 1, "name": "0001_historical_state.sql"}]}),
    )?;
    assert!(build_plan(&root, &ledger, &release, &target).is_err());

    write_json(&ledger, &json!({"rows": []}))?;
    let mut historical_release = release_contract;
    historical_release["target_schema_revision"] = json!("0001_historical_state.sql");
    write_json(&release, &json!({"schema_contract": historical_release}))?;
    assert!(build_plan(&root, &ledger, &release, &target).is_err());

    for path in [&ledger, &release, &target] {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[test]
fn protected_executor_has_one_machine_distinct_reconstruction_mode()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/d1-migration-executor.yml"))?
        .replace("\r\n", "\n");

    assert!(workflow.contains(
        "          - migration\n          - reconstruction\n          - time_travel_restore"
    ));
    assert!(workflow.contains("  reconstruct:\n"));
    assert!(workflow.contains("if: inputs.operation_mode == 'reconstruction'"));

    let start = workflow
        .find("\n  reconstruct:\n")
        .ok_or_else(|| std::io::Error::other("sole executor is missing reconstruct job"))?;
    let remainder = &workflow[start + 1..];
    let end = remainder.find("\n  authorize_restore:\n").ok_or_else(|| {
        std::io::Error::other(
            "reconstruct job must remain inside the sole executor before restore owner",
        )
    })?;
    let reconstruct = &remainder[..end];

    for required in [
        "D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION",
        "d1-current-reconstruction-materialize.py materialize",
        "--reconstruction-json",
        "RECONSTRUCTION_EXECUTOR_ADMISSION_VERIFIED",
        "RECONSTRUCTION_APPLIED",
        "verify-post-state",
        "RECONSTRUCTION_POST_STATE_VERIFIED",
        "environment: staging",
        "[[ \"$AUTHORIZATION_DIGEST\" =~ ^[0-9a-f]{64}$ ]]",
        "test \"$CONFIRMATION\" = \"$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID\"",
        "--experimental-provision=false",
        "--experimental-auto-create=false",
    ] {
        assert!(
            reconstruct.contains(required),
            "reconstruction executor is missing required invariant: {required}"
        );
    }
    for forbidden in [
        "d1 migrations apply",
        "MIGRATION_APPLIED",
        "D1_MIGRATIONS_APPLY_EXACT_PLAN",
        "d1 time-travel restore",
        "--experimental-auto-create=true",
    ] {
        assert!(
            !reconstruct.contains(forbidden),
            "reconstruction executor reached forbidden path: {forbidden}"
        );
    }
    Ok(())
}
