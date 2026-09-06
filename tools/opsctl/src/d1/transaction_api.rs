use super::model::D1Error;
use crate::canonical::{parse_strict_json, sha256_hex};
use serde_json::Value;

pub use super::transaction_core::{
    MigrationTransactionPlan, PlannedMigrationDigest, ProviderObservationBundle,
    ProviderObservationInput, RecoveryStrategy, TargetIdentity, TransactionIdentityInput,
    TransactionKind, TransactionPhase, TransactionProjection, serialize_transaction_projection,
};

pub fn build_transaction_projection(
    prepare: &Value,
    observation_value: &Value,
    repository_value: &Value,
    transaction_value: &Value,
    release_manifest_raw: &[u8],
) -> Result<TransactionProjection, D1Error> {
    verify_release_manifest_binding(prepare, transaction_value, release_manifest_raw)?;
    super::transaction_core::build_transaction_projection(
        prepare,
        observation_value,
        repository_value,
        transaction_value,
    )
}

fn verify_release_manifest_binding(
    prepare: &Value,
    transaction_value: &Value,
    release_manifest_raw: &[u8],
) -> Result<(), D1Error> {
    let release_manifest_text = std::str::from_utf8(release_manifest_raw)
        .map_err(|_| D1Error::new("release manifest must be valid UTF-8"))?;
    let release_manifest = parse_strict_json(release_manifest_text)
        .map_err(|error| D1Error::new(format!("release manifest is not strict bounded JSON: {error}")))?;
    if !release_manifest.is_object() {
        return Err(D1Error::new("release manifest must be one JSON object"));
    }

    let component = prepare
        .pointer("/plan/component")
        .and_then(Value::as_str)
        .ok_or_else(|| D1Error::new("PREPARE_READY input is missing plan.component"))?;
    let digests = transaction_value
        .get("release_manifest_digests")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            D1Error::new("transaction identity input is missing release_manifest_digests")
        })?;
    if digests.len() != 1 || !digests.contains_key(component) {
        return Err(D1Error::new(format!(
            "transaction identity release_manifest_digests must contain exactly the prepared component {component}"
        )));
    }
    let expected = digests
        .get(component)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            D1Error::new(format!(
                "transaction identity input is missing release manifest digest for {component}"
            ))
        })?;
    let actual = sha256_hex(release_manifest_raw);
    if expected != actual {
        return Err(D1Error::new(format!(
            "transaction release manifest digest does not match exact PREPARE_READY target manifest bytes: component={component} expected={expected} actual={actual}"
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
            "plan": {"component": "catalog"}
        })
    }

    fn manifest() -> Vec<u8> {
        b"{\"schema_version\":1}\n".to_vec()
    }

    fn transaction_input(raw: &[u8]) -> Value {
        json!({
            "release_manifest_digests": {
                "catalog": sha256_hex(raw)
            }
        })
    }

    #[test]
    fn exact_manifest_bytes_are_bound_to_prepared_component() -> Result<(), D1Error> {
        let raw = manifest();
        verify_release_manifest_binding(&prepare(), &transaction_input(&raw), &raw)
    }

    #[test]
    fn manifest_content_drift_is_rejected() {
        let raw = manifest();
        let changed = b"{\"schema_version\":2}\n";
        assert!(verify_release_manifest_binding(&prepare(), &transaction_input(&raw), changed).is_err());
    }

    #[test]
    fn extra_unbound_manifest_digest_is_rejected() {
        let raw = manifest();
        let mut input = transaction_input(&raw);
        input["release_manifest_digests"]["resolver"] = json!("00".repeat(32));
        assert!(verify_release_manifest_binding(&prepare(), &input, &raw).is_err());
    }

    #[test]
    fn malformed_manifest_is_rejected() {
        let raw = b"{\"schema_version\":1,\"schema_version\":1}";
        assert!(verify_release_manifest_binding(&prepare(), &transaction_input(raw), raw).is_err());
    }
}
