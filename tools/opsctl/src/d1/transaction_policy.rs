use super::model::D1Error;
use super::transaction_core::{RecoveryStrategy, TransactionKind, TransactionPhase};
use serde_json::{Value, json};

const POLICY_SCHEMA_VERSION: u64 = 1;
const ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS: u64 = 900;
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

pub(crate) fn validate_ordinary_transaction_binding(
    prepare: &Value,
    transaction: &Value,
) -> Result<(), D1Error> {
    let policy = prepare
        .pointer("/plan/transaction_policy")
        .ok_or_else(|| D1Error::new("PREPARE_READY plan is missing typed transaction_policy"))?;
    let expected = ordinary_projection();
    if policy != &expected {
        return Err(D1Error::new(
            "PREPARE_READY transaction_policy drifted from typed ordinary D1 transaction policy",
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
        validate_ordinary_transaction_binding(&prepare(), &transaction())
    }

    #[test]
    fn freshness_drift_is_rejected() {
        let mut changed = transaction();
        changed["freshness_max_age_seconds"] = json!(901);
        assert!(validate_ordinary_transaction_binding(&prepare(), &changed).is_err());
    }

    #[test]
    fn recovery_drift_is_rejected() {
        let mut changed = transaction();
        changed["recovery_strategy"] = json!(RecoveryStrategy::RollForward);
        assert!(validate_ordinary_transaction_binding(&prepare(), &changed).is_err());
    }

    #[test]
    fn prepare_policy_drift_is_rejected() {
        let mut changed = prepare();
        changed["plan"]["transaction_policy"]["observation_freshness_max_age_seconds"] = json!(901);
        assert!(validate_ordinary_transaction_binding(&changed, &transaction()).is_err());
    }
}
