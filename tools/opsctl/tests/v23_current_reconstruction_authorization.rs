use opsctl::d1::authorization::bind_current_reconstruction_authorization;
use opsctl::d1::{
    D1CurrentReconstructionPlanRequest, current_reconstruction_plan, repository_projection,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const OBSERVED_AT: i64 = 1_789_410_000;
const FRESH_UNTIL: i64 = OBSERVED_AT + 3600;
const ISSUED_AT: i64 = OBSERVED_AT + 10;
const EXPIRES_AT: i64 = OBSERVED_AT + 600;
const EVALUATED_AT: i64 = OBSERVED_AT + 20;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "opsctl-v23b-current-reconstruction-authorization-{name}-{}",
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
        "authorization_reference": "issue:584:v23b-reconstruction-authorization-fixture"
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
fn current_reconstruction_binds_through_the_shared_exact_authorization_core()
-> Result<(), Box<dyn std::error::Error>> {
    let projection = build_projection("positive")?;
    let input = authorization(&projection);
    let first = bind_current_reconstruction_authorization(&projection, &input, EVALUATED_AT)?;
    let second = bind_current_reconstruction_authorization(&projection, &input, EVALUATED_AT)?;

    assert_eq!(first.status, "AUTHORIZATION_VERIFIED");
    assert_eq!(first.mode, "read-only");
    assert_eq!(first.transaction_id, projection["reconstruction_id"]);
    assert_eq!(first.target.environment, "staging");
    assert_eq!(
        first.phase,
        opsctl::d1::transaction::TransactionPhase::Ordinary
    );
    assert_eq!(
        first.authorized_provider_effects,
        vec!["D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION".to_owned()]
    );
    assert_eq!(first.authorization_digest, second.authorization_digest);
    assert!(!first.authorization_consumed);
    assert!(!first.mutation_executed);
    assert!(!first.provider_mutation_executed);
    Ok(())
}

#[test]
fn reconstruction_authorization_rejects_scope_target_phase_and_freshness_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let projection = build_projection("authorization-negative")?;

    let mut wrong_effect = authorization(&projection);
    wrong_effect["authorized_provider_effects"] = json!(["D1_MIGRATIONS_APPLY_EXACT_PLAN"]);
    assert!(
        bind_current_reconstruction_authorization(&projection, &wrong_effect, EVALUATED_AT)
            .is_err()
    );

    let mut widened_effect = authorization(&projection);
    widened_effect["authorized_provider_effects"] =
        json!(["D1_BOOTSTRAP_CURRENT_EXACT_CONSTRUCTION", "D1_DELETE"]);
    assert!(
        bind_current_reconstruction_authorization(&projection, &widened_effect, EVALUATED_AT)
            .is_err()
    );

    let mut wrong_target = authorization(&projection);
    wrong_target["target"]["database_id"] = json!("different-database");
    assert!(
        bind_current_reconstruction_authorization(&projection, &wrong_target, EVALUATED_AT)
            .is_err()
    );

    let mut wrong_phase = authorization(&projection);
    wrong_phase["phase"] = json!("CONTRACT");
    assert!(
        bind_current_reconstruction_authorization(&projection, &wrong_phase, EVALUATED_AT).is_err()
    );

    let mut wrong_identity = authorization(&projection);
    wrong_identity["transaction_id"] = json!("ff".repeat(32));
    assert!(
        bind_current_reconstruction_authorization(&projection, &wrong_identity, EVALUATED_AT)
            .is_err()
    );

    let mut forged_freshness = authorization(&projection);
    forged_freshness["observation_fresh_until_unix_seconds"] = json!(FRESH_UNTIL + 1);
    assert!(
        bind_current_reconstruction_authorization(&projection, &forged_freshness, EVALUATED_AT)
            .is_err()
    );

    let mut overlong = authorization(&projection);
    overlong["expires_at_unix_seconds"] = json!(FRESH_UNTIL + 1);
    assert!(
        bind_current_reconstruction_authorization(&projection, &overlong, EVALUATED_AT).is_err()
    );

    let stale = authorization(&projection);
    let error = bind_current_reconstruction_authorization(&projection, &stale, EXPIRES_AT + 1)
        .err()
        .ok_or_else(|| std::io::Error::other("expired reconstruction authorization passed"))?;
    assert_eq!(reason_code(&error), Some("STALE_AUTHORIZATION"));
    Ok(())
}

#[test]
fn reconstruction_projection_tamper_is_rejected_even_when_forged_fields_are_rehashed()
-> Result<(), Box<dyn std::error::Error>> {
    let projection = build_projection("projection-negative")?;

    let mut consumed = projection.clone();
    consumed["authorization_consumed"] = json!(true);
    assert!(
        bind_current_reconstruction_authorization(
            &consumed,
            &authorization(&consumed),
            EVALUATED_AT
        )
        .is_err()
    );

    let mut production = projection.clone();
    production["plan"]["provider_observation"]["target"]["environment"] = json!("production");
    reseal_projection(&mut production)?;
    assert!(
        bind_current_reconstruction_authorization(
            &production,
            &authorization(&production),
            EVALUATED_AT
        )
        .is_err()
    );

    let mut migration_effect = projection.clone();
    migration_effect["plan"]["allowed_provider_effects"] =
        json!(["D1_MIGRATIONS_APPLY_EXACT_PLAN"]);
    reseal_projection(&mut migration_effect)?;
    let mut migration_auth = authorization(&migration_effect);
    migration_auth["authorized_provider_effects"] = json!(["D1_MIGRATIONS_APPLY_EXACT_PLAN"]);
    assert!(
        bind_current_reconstruction_authorization(&migration_effect, &migration_auth, EVALUATED_AT)
            .is_err()
    );

    let mut nonempty = projection.clone();
    nonempty["plan"]["provider_observation"]["remote_migrations"] =
        json!(["0001_historical_state.sql"]);
    reseal_projection(&mut nonempty)?;
    assert!(
        bind_current_reconstruction_authorization(
            &nonempty,
            &authorization(&nonempty),
            EVALUATED_AT
        )
        .is_err()
    );

    let mut post_state_drift = projection.clone();
    post_state_drift["plan"]["expected_post_state"]["target_schema_revision"] =
        json!("0001_historical_state.sql");
    reseal_projection(&mut post_state_drift)?;
    assert!(
        bind_current_reconstruction_authorization(
            &post_state_drift,
            &authorization(&post_state_drift),
            EVALUATED_AT
        )
        .is_err()
    );
    Ok(())
}
