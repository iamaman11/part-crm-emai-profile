use super::model::{ComponentAuthority, D1Error, MigrationClass, ReleaseSchemaContract, RolloutOrder};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

const PREDECESSOR_REVISION: &str = "0031_device_binding_governance.sql";
const CONTRACT_REVISION: &str = "0032_pas2_payload_fingerprint_contract.sql";
const EVIDENCE_KIND: &str = "D1_CONTRACT_TRANSITION_EVIDENCE";
const RECOVERY_STRATEGY: &str = "FAIL_FORWARD_ONLY";
const MAX_EVIDENCE_AGE_SECONDS: i64 = 900;
const REQUIRED_PRECONDITIONS: [&str; 2] = [
    "request_digest_readers_writers_retired",
    "server_owned_payload_fingerprint_active",
];

pub(super) struct ContractTransitionInput<'a> {
    pub(super) authority: &'a ComponentAuthority,
    pub(super) remote_names: &'a [String],
    pub(super) release: &'a ReleaseSchemaContract,
    pub(super) evidence: &'a Value,
    pub(super) evaluated_at_unix_seconds: i64,
    pub(super) expected_source_sha: &'a str,
    pub(super) expected_release_set_id: &'a str,
    pub(super) release_manifest_sha256: &'a str,
    pub(super) repository_identity_sha256: &'a str,
    pub(super) ledger_sha256: &'a str,
}

fn exact_keys(object: &Map<String, Value>, expected: &[&str], label: &str) -> Result<(), D1Error> {
    let observed = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if observed != expected {
        return Err(D1Error::new(format!(
            "{label} keys are not the exact governed schema: expected={expected:?}, observed={observed:?}"
        )));
    }
    Ok(())
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, D1Error> {
    value
        .as_object()
        .ok_or_else(|| D1Error::new(format!("{label} must be one JSON object")))
}

fn required_string(object: &Map<String, Value>, key: &str, label: &str) -> Result<String, D1Error> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| D1Error::new(format!("{label}.{key} must be a non-empty string")))
}

fn required_true(object: &Map<String, Value>, key: &str, label: &str) -> Result<(), D1Error> {
    if object.get(key).and_then(Value::as_bool) != Some(true) {
        return Err(D1Error::new(format!("{label}.{key} must be true")));
    }
    Ok(())
}

fn valid_source_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_release_set_id(value: &str) -> bool {
    let digest = value
        .strip_prefix("release-set-v2-sha256-")
        .or_else(|| value.strip_prefix("release-set-v3-sha256-"));
    digest.is_some_and(valid_sha256)
}

fn validate_release_contract(
    authority: &ComponentAuthority,
    release: &ReleaseSchemaContract,
) -> Result<(), D1Error> {
    if authority.component_id != "catalog" || release.database_component != "catalog" {
        return Err(D1Error::new(
            "contract-transition is bounded to the Catalog component",
        ));
    }
    if authority.current_repository_revision != CONTRACT_REVISION {
        return Err(D1Error::new(
            "Catalog repository authority does not end at the governed 0032 CONTRACT",
        ));
    }
    if release.target_schema_revision != PREDECESSOR_REVISION
        || release.supported_schema_min != PREDECESSOR_REVISION
        || release.supported_schema_max != CONTRACT_REVISION
        || release.migration_history_digest != authority.history_digest
        || release.compatibility_policy_digest != authority.policy_digest
    {
        return Err(D1Error::new(
            "contract-transition release schema window must be exact 0031..0032 and match typed repository identity",
        ));
    }
    let contract = authority
        .post_epoch
        .last()
        .ok_or_else(|| D1Error::new("Catalog 0032 CONTRACT metadata is missing"))?;
    let mut preconditions = contract.contract_preconditions.clone();
    preconditions.sort();
    if contract.migration_file != CONTRACT_REVISION
        || contract.migration_class != MigrationClass::Contract
        || contract.rollout_order != RolloutOrder::SeparateContractRelease
        || !contract.fail_forward_required
        || !contract.destructive
        || contract.code_rollback_allowed
        || preconditions != REQUIRED_PRECONDITIONS
    {
        return Err(D1Error::new(
            "Catalog 0032 CONTRACT metadata drifted from the fail-forward separate-release invariant",
        ));
    }
    Ok(())
}

fn validate_exact_predecessor(authority: &ComponentAuthority, remote_names: &[String]) -> Result<(), D1Error> {
    let contract_index = authority
        .ordered_history
        .iter()
        .position(|name| name == CONTRACT_REVISION)
        .ok_or_else(|| D1Error::new("Catalog 0032 CONTRACT is absent from typed history"))?;
    if contract_index == 0 || authority.ordered_history[contract_index - 1] != PREDECESSOR_REVISION {
        return Err(D1Error::new(
            "Catalog 0032 CONTRACT does not have the exact governed 0031 predecessor",
        ));
    }
    if remote_names != &authority.ordered_history[..contract_index] {
        return Err(D1Error::new(
            "contract-transition requires the remote ledger to equal the exact canonical prefix through 0031",
        ));
    }
    Ok(())
}

fn validate_evidence(input: &ContractTransitionInput<'_>) -> Result<i64, D1Error> {
    let evidence = object(input.evidence, "contract-transition evidence")?;
    exact_keys(
        evidence,
        &[
            "component",
            "contract_revision",
            "deployment",
            "environment",
            "kind",
            "ledger_sha256",
            "observed_at_unix_seconds",
            "preconditions",
            "predecessor_revision",
            "recovery_strategy",
            "release_manifest_sha256",
            "repository_identity_sha256",
            "schema_version",
        ],
        "contract-transition evidence",
    )?;
    if evidence.get("schema_version").and_then(Value::as_u64) != Some(1)
        || evidence.get("kind").and_then(Value::as_str) != Some(EVIDENCE_KIND)
        || evidence.get("environment").and_then(Value::as_str) != Some("staging")
        || evidence.get("component").and_then(Value::as_str) != Some("catalog")
        || evidence.get("predecessor_revision").and_then(Value::as_str) != Some(PREDECESSOR_REVISION)
        || evidence.get("contract_revision").and_then(Value::as_str) != Some(CONTRACT_REVISION)
        || evidence.get("recovery_strategy").and_then(Value::as_str) != Some(RECOVERY_STRATEGY)
    {
        return Err(D1Error::new(
            "contract-transition evidence identity/boundary is invalid",
        ));
    }
    for (field, expected) in [
        ("release_manifest_sha256", input.release_manifest_sha256),
        ("repository_identity_sha256", input.repository_identity_sha256),
        ("ledger_sha256", input.ledger_sha256),
    ] {
        let observed = required_string(evidence, field, "contract-transition evidence")?;
        if !valid_sha256(&observed) || observed != expected {
            return Err(D1Error::new(format!(
                "contract-transition evidence {field} does not match the exact evaluated input"
            )));
        }
    }

    let observed_at = evidence
        .get("observed_at_unix_seconds")
        .and_then(Value::as_i64)
        .ok_or_else(|| D1Error::new("contract-transition evidence observed_at_unix_seconds is missing"))?;
    if observed_at < 0
        || input.evaluated_at_unix_seconds < observed_at
        || input.evaluated_at_unix_seconds - observed_at > MAX_EVIDENCE_AGE_SECONDS
    {
        return Err(D1Error::new(
            "contract-transition evidence is stale or from the future",
        ));
    }

    if !valid_source_sha(input.expected_source_sha) {
        return Err(D1Error::new("expected source SHA must be 40 lowercase hexadecimal characters"));
    }
    if !valid_release_set_id(input.expected_release_set_id) {
        return Err(D1Error::new("expected Release Set identity is malformed or unsupported"));
    }
    let deployment = object(
        evidence
            .get("deployment")
            .ok_or_else(|| D1Error::new("contract-transition evidence deployment is missing"))?,
        "contract-transition evidence deployment",
    )?;
    exact_keys(
        deployment,
        &[
            "active_version_ids",
            "quiescent",
            "release_set_id",
            "single_version",
            "source_sha",
            "traffic_percent",
        ],
        "contract-transition evidence deployment",
    )?;
    let release_set_id = required_string(deployment, "release_set_id", "deployment")?;
    let source_sha = required_string(deployment, "source_sha", "deployment")?;
    if release_set_id != input.expected_release_set_id || !valid_release_set_id(&release_set_id) {
        return Err(D1Error::new(
            "observed deployment Release Set does not match the exact expected Release Set",
        ));
    }
    if source_sha != input.expected_source_sha || !valid_source_sha(&source_sha) {
        return Err(D1Error::new(
            "observed deployment source SHA does not match the exact expected accepted source",
        ));
    }
    required_true(deployment, "single_version", "deployment")?;
    required_true(deployment, "quiescent", "deployment")?;
    if deployment.get("traffic_percent").and_then(Value::as_f64) != Some(100.0) {
        return Err(D1Error::new(
            "contract-transition requires exactly 100 percent traffic on the sole active Worker version",
        ));
    }
    let active_versions = deployment
        .get("active_version_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| D1Error::new("deployment.active_version_ids must be an array"))?;
    if active_versions.len() != 1
        || active_versions[0]
            .as_str()
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(D1Error::new(
            "contract-transition requires exactly one non-empty active Worker version id",
        ));
    }

    let preconditions = object(
        evidence
            .get("preconditions")
            .ok_or_else(|| D1Error::new("contract-transition evidence preconditions are missing"))?,
        "contract-transition evidence preconditions",
    )?;
    exact_keys(
        preconditions,
        &REQUIRED_PRECONDITIONS,
        "contract-transition evidence preconditions",
    )?;
    for precondition in REQUIRED_PRECONDITIONS {
        required_true(preconditions, precondition, "preconditions")?;
    }
    Ok(input.evaluated_at_unix_seconds - observed_at)
}

pub(super) fn evaluate(input: ContractTransitionInput<'_>) -> Result<String, D1Error> {
    validate_release_contract(input.authority, input.release)?;
    validate_exact_predecessor(input.authority, input.remote_names)?;
    let evidence_age_seconds = validate_evidence(&input)?;
    let contract = input
        .authority
        .post_epoch
        .last()
        .ok_or_else(|| D1Error::new("Catalog 0032 CONTRACT metadata is missing"))?;
    let output = json!({
        "schema_version": 1,
        "command": "d1 contract-transition",
        "status": "ok",
        "mode": "read-only",
        "mutation_executed": false,
        "component": "catalog",
        "ledger_state": "EXACT",
        "decision": "MIGRATION_REQUIRED",
        "remote_revision": PREDECESSOR_REVISION,
        "target_revision": CONTRACT_REVISION,
        "current_repository_revision": input.authority.current_repository_revision,
        "history_digest": input.authority.history_digest,
        "planned_migrations": [CONTRACT_REVISION],
        "planned_migration_contracts": [{
            "migration_file": contract.migration_file,
            "migration_class": contract.migration_class.as_str(),
            "rollout_order": contract.rollout_order.as_str(),
            "fail_forward_required": contract.fail_forward_required,
            "destructive": contract.destructive,
            "code_rollback_allowed": contract.code_rollback_allowed,
            "contract_preconditions": contract.contract_preconditions,
        }],
        "rollback_context_complete": false,
        "recovery_strategy": RECOVERY_STRATEGY,
        "evidence_age_seconds": evidence_age_seconds,
        "reason_codes": [
            "EXACT_0031_PREDECESSOR",
            "PHASE_C_PRECONDITIONS_VERIFIED",
            "SINGLE_VERSION_QUIESCENCE_VERIFIED",
            "FAIL_FORWARD_RECOVERY_ACKNOWLEDGED"
        ],
        "allowed": true
    });
    serde_json::to_string(&output)
        .map(|value| value + "\n")
        .map_err(|error| D1Error::new(format!("cannot serialize contract-transition result: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{valid_release_set_id, valid_sha256, valid_source_sha};

    #[test]
    fn identity_validators_are_strict() {
        assert!(valid_source_sha(&"a".repeat(40)));
        assert!(!valid_source_sha(&"A".repeat(40)));
        assert!(!valid_source_sha(&"a".repeat(39)));
        assert!(valid_sha256(&"b".repeat(64)));
        assert!(!valid_sha256(&"g".repeat(64)));
        assert!(valid_release_set_id(&format!(
            "release-set-v3-sha256-{}",
            "c".repeat(64)
        )));
        assert!(!valid_release_set_id(&format!(
            "release-set-v4-sha256-{}",
            "c".repeat(64)
        )));
    }
}
