use opsctl::d1::executor_admission::{
    ExecutorAdmissionExpectation, bind_current_reconstruction_executor_admission,
    serialize_current_reconstruction_executor_admission,
};
use opsctl::d1::transaction::{TargetIdentity, TransactionPhase};
use opsctl::d1::{
    D1CurrentReconstructionPlanRequest, current_reconstruction_plan, repository_projection,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const OBSERVED_AT: i64 = 1_789_420_000;
const FRESH_UNTIL: i64 = OBSERVED_AT + 900;
const ISSUED_AT: i64 = OBSERVED_AT + 10;
const EXPIRES_AT: i64 = OBSERVED_AT + 600;
const EVALUATED_AT: i64 = OBSERVED_AT + 20;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "opsctl-v23b-current-reconstruction-executor-admission-{name}-{}",
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

fn build_projection(name: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let root = repository_root();
    let repository = current_repository(&root)?;
    let release_contract = catalog_release_contract(&repository)?;
    let ledger = fixture_path(&format!("{name}-ledger.json"));
    let release = fixture_path(&format!("{name}-release.json"));
    let target = fixture_path(&format!("{name}-target.json"));
    for path in [&ledger, &release, &target] {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
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
    let source_sha = "11".repeat(20);
    let tree_sha = "22".repeat(20);
    let release_set_id = format!("release-set-v3-sha256-{}", "33".repeat(32));
    let projection = current_reconstruction_plan(D1CurrentReconstructionPlanRequest {
        root: &root,
        ledger_json: &ledger,
        release_manifest: &release,
        target_json: &target,
        source_sha: &source_sha,
        tree_sha: &tree_sha,
        release_set_id: &release_set_id,
        observed_at_unix_seconds: OBSERVED_AT,
        observation_source: "hosted-read-only-staging-observation",
    })?;
    for path in [&ledger, &release, &target] {
        fs::remove_file(path)?;
    }
    Ok(serde_json::from_str(&projection)?)
}

fn target(projection: &Value) -> Result<TargetIdentity, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(
        projection["plan"]["provider_observation"]["target"].clone(),
    )?)
}

fn authorization(projection: &Value) -> Value {
    json!({
        "schema_version": 1,
        "transaction_id": projection["reconstruction_id"],
        "target": projection["plan"]["provider_observation"]["target"],
        "phase": "ORDINARY",
        "authorized_provider_effects": ["D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION"],
        "issued_at_unix_seconds": ISSUED_AT,
        "expires_at_unix_seconds": EXPIRES_AT,
        "observation_fresh_until_unix_seconds": FRESH_UNTIL,
        "authorization_reference": "issue:584:v23b-reconstruction-executor-admission-fixture"
    })
}

fn expectation(
    projection: &Value,
) -> Result<ExecutorAdmissionExpectation, Box<dyn std::error::Error>> {
    Ok(ExecutorAdmissionExpectation {
        transaction_id: projection["reconstruction_id"]
            .as_str()
            .ok_or_else(|| std::io::Error::other("projection missing reconstruction_id"))?
            .to_owned(),
        source_sha: projection["plan"]["source_sha"]
            .as_str()
            .ok_or_else(|| std::io::Error::other("projection missing source_sha"))?
            .to_owned(),
        tree_sha: projection["plan"]["tree_sha"]
            .as_str()
            .ok_or_else(|| std::io::Error::other("projection missing tree_sha"))?
            .to_owned(),
        component: "catalog".to_owned(),
        target: target(projection)?,
        phase: TransactionPhase::Ordinary,
    })
}

fn canonical_sha256(value: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let canonical = serde_json_canonicalizer::to_string(value)?;
    let digest = Sha256::digest(canonical.as_bytes());
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(output)
}

fn reseal_projection(projection: &mut Value) -> Result<(), Box<dyn std::error::Error>> {
    let observation = projection["plan"]["provider_observation"].clone();
    projection["plan"]["observation_digest"] = json!(canonical_sha256(&observation)?);
    let plan = projection["plan"].clone();
    projection["reconstruction_id"] = json!(canonical_sha256(&plan)?);
    Ok(())
}

fn reason_code(error: &opsctl::d1::D1Error) -> Option<&str> {
    error
        .gate_result_json()
        .get("reason_code")
        .and_then(Value::as_str)
}

#[test]
fn reconstruction_executor_admission_seals_exact_read_only_bootstrap_operation()
-> Result<(), Box<dyn std::error::Error>> {
    let projection = build_projection("positive")?;
    let expected = expectation(&projection)?;
    let first = bind_current_reconstruction_executor_admission(
        &projection,
        &authorization(&projection),
        EVALUATED_AT,
        &expected,
    )?;
    let second = bind_current_reconstruction_executor_admission(
        &projection,
        &authorization(&projection),
        EVALUATED_AT,
        &expected,
    )?;

    assert_eq!(first.status, "RECONSTRUCTION_EXECUTOR_ADMISSION_VERIFIED");
    assert_eq!(first.mode, "read-only");
    assert_eq!(first.operation_id, expected.transaction_id);
    assert_eq!(first.source_sha, expected.source_sha);
    assert_eq!(first.tree_sha, expected.tree_sha);
    assert_eq!(first.component, "catalog");
    assert_eq!(first.target, expected.target);
    assert_eq!(first.phase, TransactionPhase::Ordinary);
    assert_eq!(
        first.execution_plan.kind,
        "D1_CURRENT_FRESH_ZERO_RECONSTRUCTION"
    );
    assert_eq!(
        first.execution_plan.provider_effect,
        "D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION"
    );
    assert_eq!(first.execution_plan.predecessor_migrations, Vec::<String>::new());
    assert_eq!(
        first.execution_plan.target_schema_revision,
        projection["plan"]["target_schema_revision"]
    );
    assert_eq!(
        first.execution_plan.expected_ledger_migrations.last(),
        Some(&first.execution_plan.target_schema_revision)
    );
    assert!(first.execution_plan.apply_required);
    assert!(!first.authorization_consumed);
    assert!(!first.mutation_executed);
    assert!(!first.provider_mutation_executed);
    assert_eq!(first.authorization_digest, second.authorization_digest);
    assert_eq!(
        serialize_current_reconstruction_executor_admission(&first)?,
        serialize_current_reconstruction_executor_admission(&second)?
    );
    Ok(())
}

#[test]
fn reconstruction_executor_admission_rejects_checkout_identity_target_component_and_phase_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let projection = build_projection("expectation-negative")?;
    let input = authorization(&projection);

    let mut wrong_source = expectation(&projection)?;
    wrong_source.source_sha = "aa".repeat(20);
    let error = bind_current_reconstruction_executor_admission(
        &projection,
        &input,
        EVALUATED_AT,
        &wrong_source,
    )
    .err()
    .ok_or_else(|| std::io::Error::other("source drift unexpectedly passed"))?;
    assert_eq!(reason_code(&error), Some("SOURCE_TREE_RECONSTRUCTION_DRIFT"));

    let mut wrong_tree = expectation(&projection)?;
    wrong_tree.tree_sha = "bb".repeat(20);
    assert!(
        bind_current_reconstruction_executor_admission(
            &projection,
            &input,
            EVALUATED_AT,
            &wrong_tree
        )
        .is_err()
    );

    let mut wrong_id = expectation(&projection)?;
    wrong_id.transaction_id = "cc".repeat(32);
    assert!(
        bind_current_reconstruction_executor_admission(
            &projection,
            &input,
            EVALUATED_AT,
            &wrong_id
        )
        .is_err()
    );

    let mut wrong_target = expectation(&projection)?;
    wrong_target.target.database_id = "different-database".to_owned();
    assert!(
        bind_current_reconstruction_executor_admission(
            &projection,
            &input,
            EVALUATED_AT,
            &wrong_target
        )
        .is_err()
    );

    let mut wrong_component = expectation(&projection)?;
    wrong_component.component = "resolver".to_owned();
    assert!(
        bind_current_reconstruction_executor_admission(
            &projection,
            &input,
            EVALUATED_AT,
            &wrong_component
        )
        .is_err()
    );

    let mut wrong_phase = expectation(&projection)?;
    wrong_phase.phase = TransactionPhase::Contract;
    assert!(
        bind_current_reconstruction_executor_admission(
            &projection,
            &input,
            EVALUATED_AT,
            &wrong_phase
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn reconstruction_executor_admission_rejects_authorization_scope_and_projection_tamper()
-> Result<(), Box<dyn std::error::Error>> {
    let projection = build_projection("authorization-negative")?;
    let expected = expectation(&projection)?;

    let mut wrong_effect = authorization(&projection);
    wrong_effect["authorized_provider_effects"] = json!(["D1_MIGRATIONS_APPLY_EXACT_PLAN"]);
    let error = bind_current_reconstruction_executor_admission(
        &projection,
        &wrong_effect,
        EVALUATED_AT,
        &expected,
    )
    .err()
    .ok_or_else(|| std::io::Error::other("wrong effect unexpectedly passed"))?;
    assert_eq!(reason_code(&error), Some("INVALID_AUTHORIZATION"));

    let mut widened_effect = authorization(&projection);
    widened_effect["authorized_provider_effects"] =
        json!(["D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION", "D1_DELETE"]);
    assert!(
        bind_current_reconstruction_executor_admission(
            &projection,
            &widened_effect,
            EVALUATED_AT,
            &expected
        )
        .is_err()
    );

    let mut consumed = projection.clone();
    consumed["authorization_consumed"] = json!(true);
    let consumed_expected = expectation(&consumed)?;
    let error = bind_current_reconstruction_executor_admission(
        &consumed,
        &authorization(&consumed),
        EVALUATED_AT,
        &consumed_expected,
    )
    .err()
    .ok_or_else(|| std::io::Error::other("consumed reconstruction unexpectedly passed"))?;
    assert_eq!(reason_code(&error), Some("SOURCE_TREE_RECONSTRUCTION_DRIFT"));

    let mut nonempty = projection.clone();
    nonempty["plan"]["provider_observation"]["remote_migrations"] =
        json!(["0001_historical_state.sql"]);
    reseal_projection(&mut nonempty)?;
    let nonempty_expected = expectation(&nonempty)?;
    assert!(
        bind_current_reconstruction_executor_admission(
            &nonempty,
            &authorization(&nonempty),
            EVALUATED_AT,
            &nonempty_expected
        )
        .is_err()
    );
    Ok(())
}
