use super::model::D1Error;
use super::transaction_core::{RecoveryStrategy, TransactionKind, TransactionPhase};
use serde_json::{Value, json};

const POLICY_SCHEMA_VERSION: u64 = 1;
const ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS: u64 = 3600;
const ORDINARY_TRANSACTION_KIND: TransactionKind = TransactionKind::D1Migration;
const ORDINARY_TRANSACTION_PHASE: TransactionPhase = TransactionPhase::Ordinary;
const ORDINARY_RECOVERY_STRATEGY: RecoveryStrategy = RecoveryStrategy::NoopRetry;

pub(crate) fn ordinary_projection() -> Value {
    json!({
        "schema_version": POLICY_SCHEMA_VERSION,
        "transaction_kind": ORDINARY_TRANSACTION_KIND,
        "phase": ORDINARY_TRANSACTION_PHASE,
        "observation_freshness_max_age_seconds": ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS,
        "recovery_strategy": ORDINARY_RECOVERY_STRATEGY,
    })
}

pub(crate) fn validate_transaction_policy_binding(
    prepare: &Value,
    transaction: &Value,
) -> Result<(), D1Error> {
    let policy = prepare
        .pointer("/plan/transaction_policy")
        .ok_or_else(|| D1Error::new("PREPARE_READY plan is missing typed transaction_policy"))?;
    let expected = if prepare
        .pointer("/plan/schema_version")
        .and_then(Value::as_u64)
        == Some(2)
    {
        staged_policy(
            prepare
                .get("plan")
                .ok_or_else(|| D1Error::new("missing staged plan"))?,
        )?
    } else {
        ordinary_projection()
    };
    if policy != &expected {
        return Err(D1Error::new(
            "PREPARE_READY transaction_policy differs from its typed D1 phase policy",
        ));
    }

    for (input_field, policy_field) in [
        ("transaction_kind", "transaction_kind"),
        ("phase", "phase"),
        (
            "freshness_max_age_seconds",
            "observation_freshness_max_age_seconds",
        ),
        ("recovery_strategy", "recovery_strategy"),
    ] {
        let actual = transaction.get(input_field).ok_or_else(|| {
            D1Error::new(format!(
                "transaction identity input is missing {input_field}"
            ))
        })?;
        let expected_value = expected.get(policy_field).ok_or_else(|| {
            D1Error::new(format!(
                "typed ordinary D1 policy is missing {policy_field}"
            ))
        })?;
        if actual != expected_value {
            return Err(D1Error::new(format!(
                "transaction {input_field} drifted from typed ordinary D1 policy"
            )));
        }
    }

    Ok(())
}

/// Phase/recovery validation is shared by sealing and immutable projection revalidation.
/// Staged plans retain the full CURRENT desired target while sealing only the next effect.
pub(super) fn staged_policy(plan: &Value) -> Result<Value, D1Error> {
    if plan.get("schema_version").and_then(Value::as_u64) != Some(2)
        || plan.get("command").and_then(Value::as_str) != Some("d1 plan")
        || plan.get("allowed").and_then(Value::as_bool) != Some(true)
        || plan
            .get("desired_target_revision")
            .and_then(Value::as_str)
            .is_none()
        || plan.get("transition_evidence").is_none()
    {
        return Err(D1Error::new(
            "staged transaction requires the versioned native CURRENT stage plan",
        ));
    }
    let contracts: Vec<MigrationContractDto> = serde_json::from_value(
        plan.get("planned_migration_contracts")
            .ok_or_else(|| D1Error::new("staged plan has no migration contracts"))?
            .clone(),
    )
    .map_err(|e| D1Error::new(format!("staged migration contract schema: {e}")))?;
    let contracts = contracts
        .into_iter()
        .map(MigrationContractDto::typed)
        .collect::<Result<Vec<_>, _>>()?;
    let names: Vec<String> = serde_json::from_value(
        plan.get("planned_migrations")
            .ok_or_else(|| D1Error::new("staged plan lacks migration names"))?
            .clone(),
    )
    .map_err(|e| D1Error::new(format!("staged migration names: {e}")))?;
    if contracts
        .iter()
        .map(|m| &m.migration_file)
        .collect::<Vec<_>>()
        != names.iter().collect::<Vec<_>>()
        || contracts.last().map(|m| m.migration_file.as_str())
            != plan.get("target_revision").and_then(Value::as_str)
        || plan.get("current_repository_revision") != plan.get("desired_target_revision")
    {
        return Err(D1Error::new(
            "CURRENT stage effect boundary/desired target differs from its exact selected contracts",
        ));
    }
    super::contract_transition::stage_policy(&contracts)
}

// Closed transport admission ends here; phase/recovery semantics take only typed owner contracts.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationContractDto {
    migration_file: String,
    migration_class: String,
    rollout_order: String,
    fail_forward_required: bool,
    destructive: bool,
    code_rollback_allowed: bool,
    contract_preconditions: Vec<String>,
}
impl MigrationContractDto {
    fn typed(self) -> Result<super::model::MigrationContract, D1Error> {
        use super::model::{MigrationClass, MigrationContract, RolloutOrder};
        Ok(MigrationContract {
            migration_file: self.migration_file,
            migration_class: MigrationClass::ALL
                .into_iter()
                .find(|v| v.as_str() == self.migration_class)
                .ok_or_else(|| D1Error::new("unknown migration class in staged contract"))?,
            rollout_order: RolloutOrder::ALL
                .into_iter()
                .find(|v| v.as_str() == self.rollout_order)
                .ok_or_else(|| D1Error::new("unknown rollout order in staged contract"))?,
            fail_forward_required: self.fail_forward_required,
            destructive: self.destructive,
            code_rollback_allowed: self.code_rollback_allowed,
            contract_preconditions: self.contract_preconditions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prepare() -> Value {
        json!({
            "plan": {
                "transaction_policy": ordinary_projection()
            }
        })
    }

    fn transaction() -> Value {
        json!({
            "transaction_kind": ORDINARY_TRANSACTION_KIND,
            "phase": ORDINARY_TRANSACTION_PHASE,
            "freshness_max_age_seconds": ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS,
            "recovery_strategy": ORDINARY_RECOVERY_STRATEGY,
        })
    }

    #[test]
    fn ordinary_policy_projection_is_stable_and_typed() {
        let policy = ordinary_projection();
        assert_eq!(ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS, 3600);
        assert_eq!(policy["schema_version"], POLICY_SCHEMA_VERSION);
        assert_eq!(policy["transaction_kind"], json!(ORDINARY_TRANSACTION_KIND));
        assert_eq!(policy["phase"], json!(ORDINARY_TRANSACTION_PHASE));
        assert_eq!(
            policy["observation_freshness_max_age_seconds"],
            ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS
        );
        assert_eq!(
            policy["recovery_strategy"],
            json!(ORDINARY_RECOVERY_STRATEGY)
        );
    }

    #[test]
    fn exact_policy_binding_is_accepted() -> Result<(), D1Error> {
        validate_transaction_policy_binding(&prepare(), &transaction())
    }

    #[test]
    fn freshness_drift_is_rejected() {
        let mut changed = transaction();
        changed["freshness_max_age_seconds"] = json!(3601);
        assert!(validate_transaction_policy_binding(&prepare(), &changed).is_err());
    }

    #[test]
    fn recovery_drift_is_rejected() {
        let mut changed = transaction();
        changed["recovery_strategy"] = json!(RecoveryStrategy::RollForward);
        assert!(validate_transaction_policy_binding(&prepare(), &changed).is_err());
    }

    #[test]
    fn prepare_policy_drift_is_rejected() {
        let mut changed = prepare();
        changed["plan"]["transaction_policy"]["observation_freshness_max_age_seconds"] =
            json!(3601);
        assert!(validate_transaction_policy_binding(&changed, &transaction()).is_err());
    }
}
