use super::model::D1Error;
use serde_json::{Value, json};

const POLICY_SCHEMA_VERSION: u64 = 1;
const ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS: u64 = 900;
const ORDINARY_RECOVERY_STRATEGY: &str = "NOOP_RETRY";

pub(crate) fn ordinary_projection() -> Value {
    json!({
        "schema_version": POLICY_SCHEMA_VERSION,
        "transaction_kind": "D1_MIGRATION",
        "phase": "ORDINARY",
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

    let transaction_kind = transaction
        .get("transaction_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("transaction identity input is missing transaction_kind"))?;
    if transaction_kind != "D1_MIGRATION" {
        return Err(D1Error::new(
            "ordinary D1 transaction policy requires transaction_kind=D1_MIGRATION",
        ));
    }

    let phase = transaction
        .get("phase")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("transaction identity input is missing phase"))?;
    if phase != "ORDINARY" {
        return Err(D1Error::new(
            "ordinary D1 transaction policy requires phase=ORDINARY",
        ));
    }

    let freshness = transaction
        .get("freshness_max_age_seconds")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            D1Error::new("transaction identity input is missing freshness_max_age_seconds")
        })?;
    if freshness != ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS {
        return Err(D1Error::new(format!(
            "transaction freshness drifted from typed ordinary D1 policy: expected={ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS} actual={freshness}"
        )));
    }

    let recovery = transaction
        .get("recovery_strategy")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("transaction identity input is missing recovery_strategy"))?;
    if recovery != ORDINARY_RECOVERY_STRATEGY {
        return Err(D1Error::new(format!(
            "transaction recovery strategy drifted from typed ordinary D1 policy: expected={ORDINARY_RECOVERY_STRATEGY} actual={recovery}"
        )));
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
            "transaction_kind": "D1_MIGRATION",
            "phase": "ORDINARY",
            "freshness_max_age_seconds": ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS,
            "recovery_strategy": ORDINARY_RECOVERY_STRATEGY,
        })
    }

    #[test]
    fn ordinary_policy_projection_is_stable_and_typed() {
        let policy = ordinary_projection();
        assert_eq!(policy["schema_version"], POLICY_SCHEMA_VERSION);
        assert_eq!(policy["transaction_kind"], "D1_MIGRATION");
        assert_eq!(policy["phase"], "ORDINARY");
        assert_eq!(
            policy["observation_freshness_max_age_seconds"],
            ORDINARY_OBSERVATION_FRESHNESS_MAX_AGE_SECONDS
        );
        assert_eq!(policy["recovery_strategy"], ORDINARY_RECOVERY_STRATEGY);
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
        changed["recovery_strategy"] = json!("ROLL_FORWARD");
        assert!(validate_ordinary_transaction_binding(&prepare(), &changed).is_err());
    }

    #[test]
    fn prepare_policy_drift_is_rejected() {
        let mut changed = prepare();
        changed["plan"]["transaction_policy"]["observation_freshness_max_age_seconds"] =
            json!(901);
        assert!(validate_ordinary_transaction_binding(&changed, &transaction()).is_err());
    }
}
