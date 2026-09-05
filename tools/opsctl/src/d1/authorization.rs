use super::model::D1Error;
use super::transaction::{TargetIdentity, TransactionPhase, TransactionProjection};
use super::transaction_integrity::revalidate_transaction_projection;
use crate::canonical::{canonical_json, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const AUTHORIZATION_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionAuthorizationInput {
    pub schema_version: u64,
    pub transaction_id: String,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub authorized_provider_effects: Vec<String>,
    pub issued_at_unix_seconds: i64,
    pub expires_at_unix_seconds: i64,
    pub observation_fresh_until_unix_seconds: i64,
    pub authorization_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionAuthorizationBinding {
    pub schema_version: u64,
    pub status: String,
    pub mode: String,
    pub authorization_consumed: bool,
    pub mutation_executed: bool,
    pub provider_mutation_executed: bool,
    pub transaction_id: String,
    pub authorization_digest: String,
    pub target: TargetIdentity,
    pub phase: TransactionPhase,
    pub authorized_provider_effects: Vec<String>,
    pub issued_at_unix_seconds: i64,
    pub expires_at_unix_seconds: i64,
    pub observation_fresh_until_unix_seconds: i64,
    pub authorization_reference: String,
    pub evaluated_at_unix_seconds: i64,
}

pub fn bind_transaction_authorization(
    transaction: &TransactionProjection,
    authorization_value: &Value,
    evaluated_at_unix_seconds: i64,
) -> Result<TransactionAuthorizationBinding, D1Error> {
    revalidate_transaction_projection(transaction)?;
    if evaluated_at_unix_seconds <= 0 {
        return Err(D1Error::new(
            "authorization evaluation timestamp must be positive",
        ));
    }

    let authorization: TransactionAuthorizationInput =
        serde_json::from_value(authorization_value.clone()).map_err(|error| {
            D1Error::new(format!(
                "transaction authorization does not match the typed contract: {error}"
            ))
        })?;
    validate_authorization_input(transaction, &authorization, evaluated_at_unix_seconds)?;

    let canonical_authorization =
        canonical_json(&serde_json::to_value(&authorization).map_err(|error| {
            D1Error::new(format!(
                "cannot serialize transaction authorization: {error}"
            ))
        })?)
        .map_err(D1Error::new)?;

    Ok(TransactionAuthorizationBinding {
        schema_version: AUTHORIZATION_SCHEMA_VERSION,
        status: "AUTHORIZATION_VERIFIED".to_owned(),
        mode: "read-only".to_owned(),
        authorization_consumed: false,
        mutation_executed: false,
        provider_mutation_executed: false,
        transaction_id: authorization.transaction_id,
        authorization_digest: sha256_hex(canonical_authorization.as_bytes()),
        target: authorization.target,
        phase: authorization.phase,
        authorized_provider_effects: authorization.authorized_provider_effects,
        issued_at_unix_seconds: authorization.issued_at_unix_seconds,
        expires_at_unix_seconds: authorization.expires_at_unix_seconds,
        observation_fresh_until_unix_seconds: authorization.observation_fresh_until_unix_seconds,
        authorization_reference: authorization.authorization_reference,
        evaluated_at_unix_seconds,
    })
}

pub fn serialize_authorization_binding(
    binding: &TransactionAuthorizationBinding,
) -> Result<String, D1Error> {
    let value = serde_json::to_value(binding).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize transaction authorization binding: {error}"
        ))
    })?;
    canonical_json(&value).map_err(D1Error::new)
}

fn validate_authorization_input(
    transaction: &TransactionProjection,
    authorization: &TransactionAuthorizationInput,
    evaluated_at_unix_seconds: i64,
) -> Result<(), D1Error> {
    if authorization.schema_version != AUTHORIZATION_SCHEMA_VERSION {
        return Err(D1Error::new(format!(
            "transaction authorization schema_version must be {AUTHORIZATION_SCHEMA_VERSION}"
        )));
    }
    validate_sha256(
        &authorization.transaction_id,
        "authorization transaction_id",
    )?;
    if authorization.transaction_id != transaction.transaction_id {
        return Err(D1Error::new(
            "authorization transaction_id must exactly equal the prepared transaction_id",
        ));
    }
    validate_target(&authorization.target)?;
    if authorization.target != transaction.transaction_plan.target {
        return Err(D1Error::new(
            "authorization target must exactly equal the prepared transaction target",
        ));
    }
    if authorization.phase != transaction.transaction_plan.phase {
        return Err(D1Error::new(
            "authorization phase must exactly equal the prepared transaction phase",
        ));
    }
    validate_effect_scope(
        &authorization.authorized_provider_effects,
        "authorization authorized_provider_effects",
    )?;
    if authorization.authorized_provider_effects
        != transaction.transaction_plan.allowed_provider_effects
    {
        return Err(D1Error::new(
            "authorization provider effect scope must exactly equal the prepared transaction allowed effects",
        ));
    }
    validate_non_empty(
        &authorization.authorization_reference,
        "authorization_reference",
    )?;
    if authorization.issued_at_unix_seconds <= 0
        || authorization.expires_at_unix_seconds <= authorization.issued_at_unix_seconds
    {
        return Err(D1Error::new(
            "authorization timestamps require positive issued_at and expires_at > issued_at",
        ));
    }
    if authorization.issued_at_unix_seconds < transaction.transaction_plan.observed_at_unix_seconds
    {
        return Err(D1Error::new(
            "authorization cannot be issued before the provider observation used by the transaction",
        ));
    }

    let freshness_seconds =
        i64::try_from(transaction.transaction_plan.freshness_max_age_seconds)
            .map_err(|_| D1Error::new("transaction freshness window does not fit i64"))?;
    let expected_fresh_until = transaction
        .transaction_plan
        .observed_at_unix_seconds
        .checked_add(freshness_seconds)
        .ok_or_else(|| D1Error::new("transaction freshness deadline overflow"))?;
    if authorization.observation_fresh_until_unix_seconds != expected_fresh_until {
        return Err(D1Error::new(
            "authorization observation freshness deadline must be derived exactly from the prepared transaction",
        ));
    }
    if authorization.expires_at_unix_seconds > expected_fresh_until {
        return Err(D1Error::new(
            "authorization expiry cannot outlive the prepared provider observation freshness window",
        ));
    }
    if evaluated_at_unix_seconds < authorization.issued_at_unix_seconds {
        return Err(D1Error::new("authorization is not yet valid"));
    }
    if evaluated_at_unix_seconds > authorization.expires_at_unix_seconds {
        return Err(D1Error::new("authorization has expired"));
    }
    if evaluated_at_unix_seconds > expected_fresh_until {
        return Err(D1Error::new(
            "prepared provider observation is stale for authorization",
        ));
    }
    Ok(())
}

fn validate_effect_scope(effects: &[String], label: &str) -> Result<(), D1Error> {
    let mut unique = BTreeSet::new();
    for effect in effects {
        validate_non_empty(effect, label)?;
        if !unique.insert(effect) {
            return Err(D1Error::new(format!("{label} must not contain duplicates")));
        }
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity) -> Result<(), D1Error> {
    validate_non_empty(&target.environment, "target.environment")?;
    validate_non_empty(&target.account_id, "target.account_id")?;
    validate_non_empty(&target.database_name, "target.database_name")?;
    validate_non_empty(&target.database_id, "target.database_id")?;
    Ok(())
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), D1Error> {
    if value.trim().is_empty() {
        return Err(D1Error::new(format!("{label} must not be empty")));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), D1Error> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(D1Error::new(format!(
            "{label} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::transaction::{
        MigrationTransactionPlan, PlannedMigrationDigest, ProviderObservationBundle,
        ProviderObservationInput, RecoveryStrategy, TransactionKind,
    };
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    const OBSERVED_AT: i64 = 1_788_640_000;
    const FRESH_UNTIL: i64 = OBSERVED_AT + 900;
    const ISSUED_AT: i64 = OBSERVED_AT + 10;
    const EXPIRES_AT: i64 = OBSERVED_AT + 600;
    const EVALUATED_AT: i64 = OBSERVED_AT + 20;

    fn target() -> TargetIdentity {
        TargetIdentity {
            environment: "rehearsal".to_owned(),
            account_id: "account-1".to_owned(),
            database_name: "d1-rehearsal".to_owned(),
            database_id: "database-1".to_owned(),
        }
    }

    fn transaction() -> Result<TransactionProjection, D1Error> {
        let target = target();
        let observation_input = ProviderObservationInput {
            schema_version: 1,
            target: target.clone(),
            observed_at_unix_seconds: OBSERVED_AT,
            observation_source: "fixture".to_owned(),
            remote_ledger_sha256: "22".repeat(32),
            remote_migrations: vec!["0030_profile_generation_successor_commit.sql".to_owned()],
            wrangler_pending_migrations: vec!["0031_device_binding_governance.sql".to_owned()],
            deployment_identity: Some("deployment-1".to_owned()),
            time_travel_bookmark_capable: true,
        };
        let observation_value = serde_json::to_value(&observation_input)
            .map_err(|error| D1Error::new(format!("cannot serialize observation fixture: {error}")))?;
        let canonical_observation = canonical_json(&observation_value).map_err(D1Error::new)?;
        let observation_digest = sha256_hex(canonical_observation.as_bytes());
        let provider_observation = ProviderObservationBundle {
            schema_version: observation_input.schema_version,
            observation_digest: observation_digest.clone(),
            target: observation_input.target,
            observed_at_unix_seconds: observation_input.observed_at_unix_seconds,
            observation_source: observation_input.observation_source,
            remote_ledger_sha256: observation_input.remote_ledger_sha256,
            remote_migrations: observation_input.remote_migrations,
            wrangler_pending_migrations: observation_input.wrangler_pending_migrations,
            deployment_identity: observation_input.deployment_identity,
            time_travel_bookmark_capable: observation_input.time_travel_bookmark_capable,
        };
        let transaction_plan = MigrationTransactionPlan {
            schema_version: 1,
            repository_identity_sha256: "44".repeat(32),
            planner_policy_digest: "55".repeat(32),
            transaction_kind: TransactionKind::D1Migration,
            phase: TransactionPhase::Ordinary,
            source_sha: "66".repeat(20),
            tree_sha: "77".repeat(20),
            release_candidate_id: format!("release-set-v3-sha256-{}", "88".repeat(32)),
            release_manifest_digests: BTreeMap::from([("catalog".to_owned(), "99".repeat(32))]),
            migration_lineage_digest: "aa".repeat(32),
            target,
            observation_digest,
            observed_at_unix_seconds: OBSERVED_AT,
            freshness_max_age_seconds: 900,
            predecessor_ledger_sha256: "22".repeat(32),
            planned_migrations: vec![PlannedMigrationDigest {
                migration_file: "0031_device_binding_governance.sql".to_owned(),
                content_sha256: "bb".repeat(32),
            }],
            schema_target: "0031_device_binding_governance.sql".to_owned(),
            supported_schema_min: "0031_device_binding_governance.sql".to_owned(),
            supported_schema_max: "0032_pas2_payload_fingerprint_contract.sql".to_owned(),
            precondition_evidence_refs: vec!["fixture:precondition".to_owned()],
            recovery_strategy: RecoveryStrategy::NoopRetry,
            expected_post_state: json!({"revision": "0031_device_binding_governance.sql"}),
            allowed_provider_effects: vec!["D1_MIGRATIONS_APPLY_EXACT_PLAN".to_owned()],
            forbidden_provider_effects: vec![
                "D1_CREATE".to_owned(),
                "D1_DELETE".to_owned(),
                "PRODUCTION_MUTATION".to_owned(),
            ],
        };
        let plan_value = serde_json::to_value(&transaction_plan)
            .map_err(|error| D1Error::new(format!("cannot serialize transaction fixture: {error}")))?;
        let canonical_plan = canonical_json(&plan_value).map_err(D1Error::new)?;
        Ok(TransactionProjection {
            schema_version: 1,
            status: "TRANSACTION_PREPARED".to_owned(),
            mode: "read-only".to_owned(),
            authorization_consumed: false,
            mutation_executed: false,
            provider_mutation_executed: false,
            provider_observation,
            transaction_id: sha256_hex(canonical_plan.as_bytes()),
            transaction_plan,
        })
    }

    fn authorization(transaction: &TransactionProjection) -> Value {
        json!({
            "schema_version": 1,
            "transaction_id": transaction.transaction_id,
            "target": {
                "environment": "rehearsal",
                "account_id": "account-1",
                "database_name": "d1-rehearsal",
                "database_id": "database-1"
            },
            "phase": "ORDINARY",
            "authorized_provider_effects": ["D1_MIGRATIONS_APPLY_EXACT_PLAN"],
            "issued_at_unix_seconds": ISSUED_AT,
            "expires_at_unix_seconds": EXPIRES_AT,
            "observation_fresh_until_unix_seconds": FRESH_UNTIL,
            "authorization_reference": "issue:597:authorization-fixture"
        })
    }

    #[test]
    fn exact_authorization_binds_without_consuming_or_mutating() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let binding = bind_transaction_authorization(
            &transaction,
            &authorization(&transaction),
            EVALUATED_AT,
        )?;
        assert_eq!(binding.status, "AUTHORIZATION_VERIFIED");
        assert_eq!(binding.transaction_id, transaction.transaction_id);
        assert_eq!(binding.target, transaction.transaction_plan.target);
        assert_eq!(binding.phase, TransactionPhase::Ordinary);
        assert!(!binding.authorization_consumed);
        assert!(!binding.mutation_executed);
        assert!(!binding.provider_mutation_executed);
        assert_eq!(binding.authorization_digest.len(), 64);
        Ok(())
    }

    #[test]
    fn identical_authorization_has_deterministic_digest() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let input = authorization(&transaction);
        let left = bind_transaction_authorization(&transaction, &input, EVALUATED_AT)?;
        let right = bind_transaction_authorization(&transaction, &input, EVALUATED_AT)?;
        assert_eq!(left.authorization_digest, right.authorization_digest);
        Ok(())
    }

    #[test]
    fn transaction_id_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["transaction_id"] = json!("ff".repeat(32));
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn forged_transaction_plan_with_stale_id_is_rejected() -> Result<(), D1Error> {
        let mut transaction = transaction()?;
        transaction.transaction_plan.schema_target = "forged-schema.sql".to_owned();
        let input = authorization(&transaction);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn forged_provider_observation_digest_is_rejected() -> Result<(), D1Error> {
        let mut transaction = transaction()?;
        transaction.provider_observation.deployment_identity = Some("forged-deployment".to_owned());
        transaction.transaction_plan.observation_digest =
            transaction.provider_observation.observation_digest.clone();
        let input = authorization(&transaction);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn sealed_wrangler_pending_drift_is_rejected() -> Result<(), D1Error> {
        let mut transaction = transaction()?;
        transaction.provider_observation.wrangler_pending_migrations = Vec::new();
        let input = authorization(&transaction);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn target_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["target"]["database_id"] = json!("different-database");
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn provider_effect_widening_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["authorized_provider_effects"] =
            json!(["D1_MIGRATIONS_APPLY_EXACT_PLAN", "D1_DELETE"]);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn duplicate_provider_effect_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["authorized_provider_effects"] = json!([
            "D1_MIGRATIONS_APPLY_EXACT_PLAN",
            "D1_MIGRATIONS_APPLY_EXACT_PLAN"
        ]);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn phase_drift_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["phase"] = json!("CONTRACT");
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn expired_authorization_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let input = authorization(&transaction);
        assert!(bind_transaction_authorization(&transaction, &input, EXPIRES_AT + 1).is_err());
        Ok(())
    }

    #[test]
    fn authorization_not_yet_valid_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let input = authorization(&transaction);
        assert!(bind_transaction_authorization(&transaction, &input, ISSUED_AT - 1).is_err());
        Ok(())
    }

    #[test]
    fn authorization_cannot_outlive_observation_freshness() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["expires_at_unix_seconds"] = json!(FRESH_UNTIL + 1);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn forged_freshness_deadline_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["observation_fresh_until_unix_seconds"] = json!(FRESH_UNTIL + 1);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn authorization_before_observation_is_rejected() -> Result<(), D1Error> {
        let transaction = transaction()?;
        let mut input = authorization(&transaction);
        input["issued_at_unix_seconds"] = json!(OBSERVED_AT - 1);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn transaction_projection_observation_drift_is_rejected() -> Result<(), D1Error> {
        let mut transaction = transaction()?;
        transaction.provider_observation.observation_digest = "bb".repeat(32);
        let input = authorization(&transaction);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }

    #[test]
    fn already_consumed_projection_is_rejected() -> Result<(), D1Error> {
        let mut transaction = transaction()?;
        transaction.authorization_consumed = true;
        let input = authorization(&transaction);
        assert!(bind_transaction_authorization(&transaction, &input, EVALUATED_AT).is_err());
        Ok(())
    }
}
