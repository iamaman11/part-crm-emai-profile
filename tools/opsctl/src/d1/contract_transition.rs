//! Explicit CURRENT forward stages. Observed runtime metadata is never a target.
use super::compatibility::{
    RuntimeSchemaContext, runtime_supports_index, runtime_supports_remote,
    validate_release_contract,
};
use super::model::{
    ComponentAuthority, D1Error, MigrationClass, MigrationContract, ReleaseSchemaContract,
    RolloutOrder,
};
use super::status::revision_index;
use serde::{Deserialize, Serialize};
use serde_json::json;

const MAX_EVIDENCE_AGE_SECONDS: i64 = 900;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransitionEvidence {
    pub schema_version: u64,
    pub kind: String,
    pub component: String,
    pub environment: String,
    pub observed_at_unix_seconds: i64,
    pub first_deployment_observed_at_unix_seconds: i64,
    pub deployment_observed_at_unix_seconds: i64,
    pub deployment_before: Deployment,
    pub evaluated_at_unix_seconds: i64,
    pub current_manifest_sha256: String,
    pub target_manifest_sha256: String,
    pub repository_identity_sha256: String,
    pub ledger_sha256: String,
    pub deployment: Deployment,
    pub target_source_sha: String,
    pub target_release_set_id: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Deployment {
    pub source_sha: String,
    pub release_set_id: String,
    pub deployment_id: String,
    pub worker_name: String,
    pub versions: Vec<WorkerVersion>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkerVersion {
    pub version_id: String,
    pub percentage: f64,
}

pub(super) struct CurrentTransitionInput<'a> {
    pub authority: &'a ComponentAuthority,
    pub remote_names: &'a [String],
    pub current: &'a RuntimeSchemaContext,
    pub target: &'a ReleaseSchemaContract,
    pub evidence: &'a TransitionEvidence,
    pub current_manifest_sha256: &'a str,
    pub target_manifest_sha256: &'a str,
    pub repository_identity_sha256: &'a str,
    pub ledger_sha256: &'a str,
    pub current_source_sha: &'a str,
    pub target_source_sha: &'a str,
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn provider_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

fn source(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn release(value: &str) -> bool {
    value
        .strip_prefix("release-set-v3-sha256-")
        .is_some_and(digest)
}

/// The sole phase/recovery rule, used by native planning and strict transaction adapters.
pub(super) fn stage_policy(contracts: &[MigrationContract]) -> Result<serde_json::Value, D1Error> {
    let first = contracts
        .first()
        .ok_or_else(|| D1Error::new("CURRENT stage has no migration contracts"))?;
    let (phase, recovery) = if first.migration_class == MigrationClass::Contract {
        let mut requirements = first
            .contract_preconditions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        requirements.sort_unstable();
        if contracts.len() != 1
            || first.rollout_order != RolloutOrder::SeparateContractRelease
            || !first.fail_forward_required
            || !first.destructive
            || first.code_rollback_allowed
            || requirements
                != [
                    "request_digest_readers_writers_retired",
                    "server_owned_payload_fingerprint_active",
                ]
        {
            return Err(D1Error::new(
                "CONTRACT must remain one separate fail-forward effect with the known readiness requirements",
            ));
        }
        ("CONTRACT", "FAIL_FORWARD_ONLY")
    } else {
        if contracts.iter().any(|m| {
            m.migration_class != MigrationClass::Expand
                || m.destructive
                || m.fail_forward_required
                || !m.code_rollback_allowed
                || m.rollout_order != RolloutOrder::MigrateBeforeCode
                || !m.contract_preconditions.is_empty()
        }) {
            return Err(D1Error::new(
                "forward CURRENT stage requires independently completed CONTRACT and only additive migrate-before-code changes",
            ));
        }
        ("ORDINARY", "ROLL_FORWARD")
    };
    Ok(
        json!({"schema_version":2,"transaction_kind":"D1_MIGRATION","phase":phase,
        "observation_freshness_max_age_seconds":MAX_EVIDENCE_AGE_SECONDS,"recovery_strategy":recovery}),
    )
}

pub(super) fn evaluate(input: CurrentTransitionInput<'_>) -> Result<String, D1Error> {
    let e = input.evidence;
    if e.schema_version != 2
        || e.kind != "D1_CURRENT_TRANSITION_EVIDENCE"
        || e.component != "catalog"
        || e.environment != "staging"
        || input.authority.component_id != "catalog"
    {
        return Err(D1Error::new(
            "CURRENT transition requires exact v2 staging Catalog evidence",
        ));
    }
    validate_release_contract(input.authority, input.target)?;
    for (observed, expected) in [
        (&e.current_manifest_sha256, input.current_manifest_sha256),
        (&e.target_manifest_sha256, input.target_manifest_sha256),
        (
            &e.repository_identity_sha256,
            input.repository_identity_sha256,
        ),
        (&e.ledger_sha256, input.ledger_sha256),
    ] {
        if !digest(observed) || observed != expected {
            return Err(D1Error::new(
                "CURRENT transition evidence differs from exact evaluated manifest/repository/ledger identity",
            ));
        }
    }
    if !source(&e.deployment.source_sha)
        || e.deployment.source_sha != input.current_source_sha
        || !source(&e.target_source_sha)
        || e.target_source_sha != input.target_source_sha
        || !release(&e.deployment.release_set_id)
        || !release(&e.target_release_set_id)
    {
        return Err(D1Error::new(
            "CURRENT transition deployed and target source/Release Set identities are invalid",
        ));
    }
    if e.observed_at_unix_seconds <= 0
        || e.evaluated_at_unix_seconds < e.observed_at_unix_seconds
        || e.evaluated_at_unix_seconds - e.observed_at_unix_seconds > MAX_EVIDENCE_AGE_SECONDS
    {
        return Err(D1Error::new(
            "CURRENT transition observation is stale or from the future",
        ));
    }
    let interval =
        e.deployment_observed_at_unix_seconds - e.first_deployment_observed_at_unix_seconds;
    if e.first_deployment_observed_at_unix_seconds <= 0
        || !(30..=300).contains(&interval)
        || e.deployment_observed_at_unix_seconds > e.evaluated_at_unix_seconds
        || e.evaluated_at_unix_seconds - e.deployment_observed_at_unix_seconds
            > MAX_EVIDENCE_AGE_SECONDS
        || e.deployment != e.deployment_before
    {
        return Err(D1Error::new(
            "CURRENT transition requires two equal deployment observations30..300 seconds apart, fresh within900 seconds",
        ));
    }
    if e.deployment.worker_name.is_empty()
        || e.deployment.worker_name.len() > 63
        || !e
            .deployment
            .worker_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(D1Error::new(
            "CURRENT transition observed Worker name is not a bounded provider resource name",
        ));
    }
    if !provider_uuid(&e.deployment.deployment_id)
        || e.deployment.versions.len() != 1
        || !provider_uuid(&e.deployment.versions[0].version_id)
        || e.deployment.versions[0].percentage != 100.0
    {
        return Err(D1Error::new(
            "CURRENT transition requires one quiescent deployed Worker version with exactly100 percent traffic",
        ));
    }
    let a = input.authority;
    if a.ordered_history.get(..input.remote_names.len()) != Some(input.remote_names)
        || input.remote_names.len() < a.historical_len
        || !runtime_supports_remote(a, input.current.schema(), input.remote_names.len())?
    {
        return Err(D1Error::new(
            "CURRENT transition requires a canonical predecessor supported by the actually deployed runtime",
        ));
    }
    let start = input.remote_names.len();
    let desired = revision_index(&a.ordered_history, &input.target.target_schema_revision)? + 1;
    if desired != a.ordered_history.len() || start > desired {
        return Err(D1Error::new(
            "CURRENT transition must advance toward the full CURRENT repository target",
        ));
    }
    if start == desired {
        return serde_json::to_string(&json!({"schema_version":1,"command":"d1 plan","status":"ok",
            "mode":"read-only","mutation_executed":false,"component":"catalog","ledger_state":"EXACT",
            "decision":"SAFE","remote_revision":input.remote_names.last(),"target_revision":input.target.target_schema_revision,
            "current_repository_revision":a.current_repository_revision,"history_digest":a.history_digest,
            "planned_migrations":[],"planned_migration_contracts":[],"rollback_context_complete":true,"reason_codes":[],
            "allowed":true,"transaction_policy":super::transaction_policy::ordinary_projection()}))
            .map(|s| s + "\n").map_err(|e| D1Error::new(e.to_string()));
    }
    let pending = &a.post_epoch[start - a.historical_len..desired - a.historical_len];
    let first = pending
        .first()
        .ok_or_else(|| D1Error::new("CURRENT transition has no pending migration"))?;
    let is_contract = first.migration_class == MigrationClass::Contract;
    let selected = if is_contract { &pending[..1] } else { pending };
    let policy = stage_policy(selected)?;
    if is_contract && !runtime_supports_index(a, input.current.schema(), start)? {
        return Err(D1Error::new(
            "CONTRACT prerequisites are not proved by the immutable deployed runtime schema window",
        ));
    }
    let readiness = if is_contract {
        "CONTRACT_READINESS_FROM_IMMUTABLE_RUNTIME_WINDOW"
    } else {
        "EXPLICIT_CURRENT_TARGET_FORWARD_RECOVERY"
    };
    let stop = start + selected.len();
    let target_revision = &a.ordered_history[stop - 1];
    let contracts = selected.iter().map(|m| json!({
        "migration_file":m.migration_file,"migration_class":m.migration_class.as_str(),
        "rollout_order":m.rollout_order.as_str(),"fail_forward_required":m.fail_forward_required,
        "destructive":m.destructive,"code_rollback_allowed":m.code_rollback_allowed,
        "contract_preconditions":m.contract_preconditions,
    })).collect::<Vec<_>>();
    let value = json!({
        "schema_version":2,"command":"d1 plan","status":"ok","mode":"read-only",
        "mutation_executed":false,"component":"catalog","ledger_state":"BEHIND_KNOWN_PREFIX",
        "decision":"MIGRATION_REQUIRED","remote_revision":input.remote_names.last(),
        "target_revision":target_revision,"desired_target_revision":input.target.target_schema_revision,
        "current_repository_revision":a.current_repository_revision,"history_digest":a.history_digest,
        "planned_migrations":a.ordered_history[start..stop],"planned_migration_contracts":contracts,
        "pending_migrations_to_desired_target":a.ordered_history[start..desired],
        "rollback_context_complete":false,"reason_codes":[readiness],"allowed":true,
        "transition_evidence":e,
        "transaction_policy":policy,
    });
    serde_json::to_string(&value)
        .map(|s| s + "\n")
        .map_err(|e| D1Error::new(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::model::MigrationContract;
    use super::*;
    fn authority() -> ComponentAuthority {
        ComponentAuthority {
            component_id: "catalog".into(),
            historical_len: 1,
            ordered_history: vec![
                "base.sql".into(),
                "contract.sql".into(),
                "expand.sql".into(),
            ],
            current_repository_revision: "expand.sql".into(),
            history_digest: "a".repeat(64),
            policy_digest: "b".repeat(64),
            post_epoch: vec![
                MigrationContract {
                    migration_file: "contract.sql".into(),
                    migration_class: MigrationClass::Contract,
                    rollout_order: RolloutOrder::SeparateContractRelease,
                    fail_forward_required: true,
                    destructive: true,
                    code_rollback_allowed: false,
                    contract_preconditions: vec![
                        "server_owned_payload_fingerprint_active".into(),
                        "request_digest_readers_writers_retired".into(),
                    ],
                },
                MigrationContract {
                    migration_file: "expand.sql".into(),
                    migration_class: MigrationClass::Expand,
                    rollout_order: RolloutOrder::MigrateBeforeCode,
                    fail_forward_required: false,
                    destructive: false,
                    code_rollback_allowed: true,
                    contract_preconditions: vec![],
                },
            ],
        }
    }
    fn schema(a: &ComponentAuthority, min: usize, max: usize) -> ReleaseSchemaContract {
        ReleaseSchemaContract {
            database_component: "catalog".into(),
            target_schema_revision: a.ordered_history[min].clone(),
            supported_schema_min: a.ordered_history[min].clone(),
            supported_schema_max: a.ordered_history[max].clone(),
            migration_history_digest: a.history_digest.clone(),
            compatibility_policy_digest: a.policy_digest.clone(),
        }
    }
    fn evidence() -> TransitionEvidence {
        TransitionEvidence {
            schema_version: 2,
            kind: "D1_CURRENT_TRANSITION_EVIDENCE".into(),
            component: "catalog".into(),
            environment: "staging".into(),
            observed_at_unix_seconds: 100,
            first_deployment_observed_at_unix_seconds: 60,
            deployment_observed_at_unix_seconds: 100,
            deployment_before: Deployment {
                source_sha: "a".repeat(40),
                release_set_id: format!("release-set-v3-sha256-{}", "a".repeat(64)),
                deployment_id: "11111111-1111-4111-8111-111111111111".into(),
                worker_name: "worker-1".into(),
                versions: vec![WorkerVersion {
                    version_id: "22222222-2222-4222-8222-222222222222".into(),
                    percentage: 100.0,
                }],
            },
            evaluated_at_unix_seconds: 110,
            current_manifest_sha256: "c".repeat(64),
            target_manifest_sha256: "d".repeat(64),
            repository_identity_sha256: "e".repeat(64),
            ledger_sha256: "f".repeat(64),
            target_source_sha: "b".repeat(40),
            target_release_set_id: format!("release-set-v3-sha256-{}", "b".repeat(64)),
            deployment: Deployment {
                source_sha: "a".repeat(40),
                release_set_id: format!("release-set-v3-sha256-{}", "a".repeat(64)),
                deployment_id: "11111111-1111-4111-8111-111111111111".into(),
                worker_name: "worker-1".into(),
                versions: vec![WorkerVersion {
                    version_id: "22222222-2222-4222-8222-222222222222".into(),
                    percentage: 100.0,
                }],
            },
        }
    }
    fn invoke(
        a: &ComponentAuthority,
        current: ReleaseSchemaContract,
        remote: &[String],
        e: &TransitionEvidence,
    ) -> Result<String, D1Error> {
        let context = RuntimeSchemaContext::new(a, current)?;
        evaluate(CurrentTransitionInput {
            authority: a,
            remote_names: remote,
            current: &context,
            target: &schema(a, 2, 2),
            evidence: e,
            current_manifest_sha256: &"c".repeat(64),
            target_manifest_sha256: &"d".repeat(64),
            repository_identity_sha256: &"e".repeat(64),
            ledger_sha256: &"f".repeat(64),
            current_source_sha: &"a".repeat(40),
            target_source_sha: &"b".repeat(40),
        })
    }
    #[test]
    fn explicit_stages_reject_missing_readiness_unknown_ledger_and_mixed_or_stale_observation()
    -> Result<(), D1Error> {
        let a = authority();
        let e = evidence();
        let c = schema(&a, 0, 1);
        assert!(invoke(&a, c.clone(), &a.ordered_history[..1], &e).is_ok());
        assert!(invoke(&a, schema(&a, 0, 0), &a.ordered_history[..1], &e).is_err());
        for remote in [
            vec!["unknown.sql".into()],
            vec![],
            vec!["base.sql".into(), "base.sql".into()],
        ] {
            assert!(invoke(&a, c.clone(), &remote, &e).is_err());
        }
        let mut changed = e.clone();
        changed.environment = "production".into();
        assert!(invoke(&a, c.clone(), &a.ordered_history[..1], &changed).is_err());
        let mut changed = e.clone();
        changed.evaluated_at_unix_seconds = 1001;
        assert!(invoke(&a, c.clone(), &a.ordered_history[..1], &changed).is_err());
        let mut changed = e.clone();
        changed.deployment.versions[0].percentage = 99.0;
        assert!(invoke(&a, c.clone(), &a.ordered_history[..1], &changed).is_err());
        let mut changed = e.clone();
        changed.deployment.versions.push(WorkerVersion {
            version_id: "other".into(),
            percentage: 0.0,
        });
        assert!(invoke(&a, c.clone(), &a.ordered_history[..1], &changed).is_err());
        let mut changed = a.clone();
        changed.post_epoch[0]
            .contract_preconditions
            .push("unknown prerequisite".into());
        assert!(invoke(&changed, c.clone(), &changed.ordered_history[..1], &e).is_err());
        let mut changed = a.clone();
        changed.post_epoch[1].destructive = true;
        assert!(invoke(&changed, c.clone(), &changed.ordered_history[..2], &e).is_err());
        // A complete database with the old incompatible runtime cannot be mislabeled healthy/no-op.
        assert!(invoke(&a, c, &a.ordered_history, &e).is_err());
        let exact: serde_json::Value =
            serde_json::from_str(&invoke(&a, schema(&a, 2, 2), &a.ordered_history, &e)?)
                .map_err(|x| D1Error::new(x.to_string()))?;
        assert_eq!(exact["decision"], "SAFE");
        assert_eq!(exact["planned_migrations"], json!([]));
        Ok(())
    }
}
