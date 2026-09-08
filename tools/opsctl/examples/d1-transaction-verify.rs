#![forbid(unsafe_code)]

use opsctl::canonical::{canonical_json, parse_strict_json};
use opsctl::d1::operator_outcome::D1OperatorOutcomeKind;
use opsctl::d1::transaction::TransactionProjection;
use opsctl::d1::transaction_integrity::revalidate_transaction_projection;
use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Default)]
struct Args {
    transaction_json: Option<PathBuf>,
    expected_source_sha: Option<String>,
    expected_tree_sha: Option<String>,
    expected_environment: Option<String>,
    evaluated_at_unix_seconds: Option<i64>,
}

#[derive(Debug)]
struct VerifyFailure {
    kind: D1OperatorOutcomeKind,
    detail: String,
}

impl VerifyFailure {
    fn new(kind: D1OperatorOutcomeKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    fn diagnostic_json(&self) -> Value {
        json!({
            "schema_version": 1,
            "reason_code": self.kind.as_str(),
            "summary": self.kind.summary(),
            "detail": self.detail,
            "remediation": self.kind.remediation(),
            "tool": {
                "name": "opsctl",
                "surface": "d1",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })
    }

    fn canonical_diagnostic(&self) -> Result<String, Box<dyn Error>> {
        canonical_json(&self.diagnostic_json()).map_err(Into::into)
    }
}

impl fmt::Display for VerifyFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for VerifyFailure {}

fn next_value(
    iterator: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<OsString, Box<dyn Error>> {
    iterator
        .next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn utf8_value(
    iterator: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<String, Box<dyn Error>> {
    next_value(iterator, flag)?
        .into_string()
        .map_err(|_| format!("{flag} value must be valid UTF-8").into())
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), Box<dyn Error>> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be supplied only once").into());
    }
    Ok(())
}

fn parse_args<I>(args: I) -> Result<Args, Box<dyn Error>>
where
    I: IntoIterator<Item = OsString>,
{
    let mut iterator = args.into_iter();
    let _program = iterator.next();
    let mut args = Args::default();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or("transaction-verify flags must be valid UTF-8")?;
        match flag {
            "--transaction-json" => set_once(
                &mut args.transaction_json,
                PathBuf::from(next_value(&mut iterator, flag)?),
                flag,
            )?,
            "--expected-source-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_source_sha, value, flag)?;
            }
            "--expected-tree-sha" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_tree_sha, value, flag)?;
            }
            "--expected-environment" => {
                let value = utf8_value(&mut iterator, flag)?;
                set_once(&mut args.expected_environment, value, flag)?;
            }
            "--evaluated-at-unix-seconds" => {
                let value = utf8_value(&mut iterator, flag)?.parse::<i64>()?;
                set_once(&mut args.evaluated_at_unix_seconds, value, flag)?;
            }
            other => return Err(format!("unsupported transaction-verify argument: {other}").into()),
        }
    }
    Ok(args)
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} is required").into())
}

fn transaction_drift(detail: impl Into<String>) -> VerifyFailure {
    VerifyFailure::new(D1OperatorOutcomeKind::SourceTreeTransactionDrift, detail)
}

fn stale_observation(detail: impl Into<String>) -> VerifyFailure {
    VerifyFailure::new(D1OperatorOutcomeKind::StaleObservation, detail)
}

fn verify(args: Args) -> Result<String, VerifyFailure> {
    let transaction_path = args
        .transaction_json
        .ok_or_else(|| transaction_drift("--transaction-json is required"))?;
    let raw = fs::read_to_string(transaction_path)
        .map_err(|error| transaction_drift(format!("cannot read prepared transaction: {error}")))?;
    let value = parse_strict_json(&raw).map_err(|error| {
        transaction_drift(format!(
            "prepared transaction is not strict bounded JSON: {error}"
        ))
    })?;
    let transaction: TransactionProjection = serde_json::from_value(value).map_err(|error| {
        transaction_drift(format!(
            "prepared transaction does not match the typed contract: {error}"
        ))
    })?;
    revalidate_transaction_projection(&transaction).map_err(|error| {
        transaction_drift(format!(
            "prepared transaction integrity revalidation failed: {error}"
        ))
    })?;

    let expected_source = args
        .expected_source_sha
        .ok_or_else(|| transaction_drift("--expected-source-sha is required"))?;
    let expected_tree = args
        .expected_tree_sha
        .ok_or_else(|| transaction_drift("--expected-tree-sha is required"))?;
    let expected_environment = args
        .expected_environment
        .ok_or_else(|| transaction_drift("--expected-environment is required"))?;
    let evaluated_at = args
        .evaluated_at_unix_seconds
        .ok_or_else(|| transaction_drift("--evaluated-at-unix-seconds is required"))?;
    let plan = &transaction.transaction_plan;
    if plan.source_sha != expected_source {
        return Err(transaction_drift(
            "prepared transaction source_sha does not equal exact checked-out source",
        ));
    }
    if plan.tree_sha != expected_tree {
        return Err(transaction_drift(
            "prepared transaction tree_sha does not equal exact checked-out tree",
        ));
    }
    if plan.target.environment != expected_environment {
        return Err(transaction_drift(
            "prepared transaction target environment does not equal operator environment",
        ));
    }
    if expected_environment != "staging" {
        return Err(transaction_drift(
            "ordinary D1 operator currently permits staging only",
        ));
    }
    if evaluated_at <= 0 {
        return Err(transaction_drift(
            "transaction evaluation timestamp must be positive",
        ));
    }
    let freshness = i64::try_from(plan.freshness_max_age_seconds)
        .map_err(|_| transaction_drift("transaction freshness window does not fit i64"))?;
    let fresh_until = plan
        .observed_at_unix_seconds
        .checked_add(freshness)
        .ok_or_else(|| transaction_drift("transaction freshness deadline overflow"))?;
    if evaluated_at > fresh_until {
        return Err(stale_observation("prepared provider observation is stale"));
    }

    let components = plan
        .release_manifest_digests
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    if components.len() != 1 {
        return Err(transaction_drift(
            "prepared ordinary transaction must bind exactly one release-manifest component",
        ));
    }
    let component = components[0].clone();
    if !matches!(component.as_str(), "catalog" | "resolver") {
        return Err(transaction_drift(
            "prepared transaction component is unsupported",
        ));
    }

    canonical_json(&json!({
        "schema_version": 1,
        "status": "TRANSACTION_VERIFIED",
        "mode": "read-only",
        "authorization_consumed": false,
        "mutation_executed": false,
        "provider_mutation_executed": false,
        "transaction_id": transaction.transaction_id,
        "source_sha": plan.source_sha,
        "tree_sha": plan.tree_sha,
        "release_candidate_id": plan.release_candidate_id,
        "component": component,
        "target": plan.target,
        "fresh_until_unix_seconds": fresh_until,
        "schema_target": plan.schema_target,
        "recovery_strategy": plan.recovery_strategy,
    }))
    .map_err(|error| transaction_drift(format!("cannot serialize transaction verification: {error}")))
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = parse_args(env::args_os())?;
    match verify(args) {
        Ok(output) => {
            println!("{output}");
            Ok(())
        }
        Err(error) => {
            println!("{}", error.canonical_diagnostic()?);
            Err(error.into())
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsctl::canonical::{canonical_json, sha256_hex};
    use opsctl::d1::transaction::{
        MigrationTransactionPlan, PlannedMigrationDigest, ProviderObservationBundle,
        RecoveryStrategy, TargetIdentity, TransactionKind, TransactionPhase,
    };
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn transaction() -> TransactionProjection {
        let target = TargetIdentity {
            environment: "staging".to_owned(),
            account_id: "account-1".to_owned(),
            database_name: "d1-rehearsal".to_owned(),
            database_id: "database-1".to_owned(),
        };
        let observation_input = opsctl::d1::transaction::ProviderObservationInput {
            schema_version: 1,
            target: target.clone(),
            observed_at_unix_seconds: 1_788_640_000,
            observation_source: "fixture".to_owned(),
            remote_ledger_sha256: "22".repeat(32),
            remote_migrations: vec!["0041.sql".to_owned()],
            wrangler_pending_migrations: vec!["0042.sql".to_owned()],
            deployment_identity: Some("deployment-1".to_owned()),
            time_travel_bookmark_capable: true,
        };
        let observation_value =
            serde_json::to_value(&observation_input).expect("observation value");
        let observation_digest = sha256_hex(
            canonical_json(&observation_value)
                .expect("canonical observation")
                .as_bytes(),
        );
        let provider_observation = ProviderObservationBundle {
            schema_version: 1,
            observation_digest: observation_digest.clone(),
            target: target.clone(),
            observed_at_unix_seconds: observation_input.observed_at_unix_seconds,
            observation_source: observation_input.observation_source,
            remote_ledger_sha256: observation_input.remote_ledger_sha256,
            remote_migrations: observation_input.remote_migrations,
            wrangler_pending_migrations: observation_input.wrangler_pending_migrations,
            deployment_identity: observation_input.deployment_identity,
            time_travel_bookmark_capable: observation_input.time_travel_bookmark_capable,
        };
        let plan = MigrationTransactionPlan {
            schema_version: 1,
            repository_identity_sha256: "33".repeat(32),
            planner_policy_digest: "44".repeat(32),
            transaction_kind: TransactionKind::D1Migration,
            phase: TransactionPhase::Ordinary,
            source_sha: "55".repeat(20),
            tree_sha: "66".repeat(20),
            release_candidate_id: format!("release-set-v3-sha256-{}", "77".repeat(32)),
            release_manifest_digests: BTreeMap::from([("catalog".to_owned(), "88".repeat(32))]),
            migration_lineage_digest: "99".repeat(32),
            target,
            observation_digest,
            observed_at_unix_seconds: observation_input.observed_at_unix_seconds,
            freshness_max_age_seconds: 900,
            predecessor_ledger_sha256: "22".repeat(32),
            planned_migrations: vec![PlannedMigrationDigest {
                migration_file: "0042.sql".to_owned(),
                content_sha256: "aa".repeat(32),
            }],
            schema_target: "0042.sql".to_owned(),
            supported_schema_min: "0042.sql".to_owned(),
            supported_schema_max: "0042.sql".to_owned(),
            precondition_evidence_refs: vec!["fixture:precondition".to_owned()],
            recovery_strategy: RecoveryStrategy::NoopRetry,
            expected_post_state: json!({"revision": "0042.sql"}),
            allowed_provider_effects: vec!["D1_MIGRATIONS_APPLY_EXACT_PLAN".to_owned()],
            forbidden_provider_effects: vec!["PRODUCTION_MUTATION".to_owned()],
        };
        let transaction_id = sha256_hex(
            canonical_json(&serde_json::to_value(&plan).expect("plan value"))
                .expect("canonical plan")
                .as_bytes(),
        );
        TransactionProjection {
            schema_version: 1,
            status: "TRANSACTION_PREPARED".to_owned(),
            mode: "read-only".to_owned(),
            authorization_consumed: false,
            mutation_executed: false,
            provider_mutation_executed: false,
            provider_observation,
            transaction_id,
            transaction_plan: plan,
        }
    }

    fn write_fixture(transaction: &TransactionProjection) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir().join(format!("d1-transaction-verify-{unique}.json"));
        fs::write(
            &path,
            serde_json::to_string(transaction).expect("serialize transaction"),
        )
        .expect("write fixture");
        path
    }

    fn args(path: PathBuf, evaluated_at: i64) -> Args {
        Args {
            transaction_json: Some(path),
            expected_source_sha: Some("55".repeat(20)),
            expected_tree_sha: Some("66".repeat(20)),
            expected_environment: Some("staging".to_owned()),
            evaluated_at_unix_seconds: Some(evaluated_at),
        }
    }

    #[test]
    fn exact_fresh_transaction_is_verified() {
        let transaction = transaction();
        let path = write_fixture(&transaction);
        let output = verify(args(path.clone(), 1_788_640_100)).expect("verified");
        fs::remove_file(path).ok();
        let value: serde_json::Value = serde_json::from_str(&output).expect("output json");
        assert_eq!(value["status"], "TRANSACTION_VERIFIED");
        assert_eq!(value["component"], "catalog");
        assert_eq!(value["fresh_until_unix_seconds"], 1_788_640_900);
    }

    #[test]
    fn stale_transaction_has_typed_stale_observation_diagnostic() {
        let transaction = transaction();
        let path = write_fixture(&transaction);
        let error = verify(args(path.clone(), 1_788_640_901)).expect_err("stale transaction");
        fs::remove_file(path).ok();
        assert_eq!(error.kind, D1OperatorOutcomeKind::StaleObservation);
        let diagnostic = error.diagnostic_json();
        assert_eq!(diagnostic["reason_code"], "STALE_OBSERVATION");
        assert_eq!(diagnostic["tool"]["name"], "opsctl");
        assert_eq!(diagnostic["tool"]["surface"], "d1");
    }

    #[test]
    fn source_drift_has_typed_transaction_drift_diagnostic() {
        let transaction = transaction();
        let path = write_fixture(&transaction);
        let mut input = args(path.clone(), 1_788_640_100);
        input.expected_source_sha = Some("ff".repeat(20));
        let error = verify(input).expect_err("source drift");
        fs::remove_file(path).ok();
        assert_eq!(error.kind, D1OperatorOutcomeKind::SourceTreeTransactionDrift);
        assert_eq!(
            error.diagnostic_json()["reason_code"],
            "SOURCE_TREE_TRANSACTION_DRIFT"
        );
    }

    #[test]
    fn transaction_identity_tamper_has_typed_transaction_drift_diagnostic() {
        let mut transaction = transaction();
        transaction.transaction_plan.planned_migrations[0].content_sha256 = "bb".repeat(32);
        let path = write_fixture(&transaction);
        let error = verify(args(path.clone(), 1_788_640_100)).expect_err("tamper");
        fs::remove_file(path).ok();
        assert_eq!(error.kind, D1OperatorOutcomeKind::SourceTreeTransactionDrift);
    }
}
