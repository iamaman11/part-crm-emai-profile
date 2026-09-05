use crate::canonical::{canonical_json, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::model::D1Error;

pub(super) const TRANSACTION_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum TransactionKind {
    Ordinary,
    Contract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum RecoveryStrategy {
    ForwardOnly,
    FailForwardOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SourceIdentity {
    pub(super) source_sha: String,
    pub(super) tree_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ReleaseIdentity {
    pub(super) release_candidate_id: String,
    pub(super) release_set_manifest_sha256: String,
    pub(super) component_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct PolicyIdentity {
    pub(super) planner_policy_sha256: String,
    pub(super) migration_lineage_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct TargetIdentity {
    pub(super) environment: String,
    pub(super) account_id: String,
    pub(super) database_component: String,
    pub(super) database_name: String,
    pub(super) database_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ObservationIdentity {
    pub(super) observation_sha256: String,
    pub(super) observed_at: String,
    pub(super) freshness_max_age_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LedgerIdentity {
    pub(super) ledger_sha256: String,
    pub(super) remote_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct PlannedMigrationIdentity {
    pub(super) migration_file: String,
    pub(super) content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SchemaWindow {
    pub(super) target_revision: String,
    pub(super) supported_schema_min: String,
    pub(super) supported_schema_max: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct EvidenceReference {
    pub(super) evidence_class: String,
    pub(super) evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExpectedPostState {
    pub(super) ledger_sha256: String,
    pub(super) target_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct MigrationTransactionPlan {
    pub(super) schema_version: u32,
    pub(super) transaction_kind: TransactionKind,
    pub(super) source: SourceIdentity,
    pub(super) release: ReleaseIdentity,
    pub(super) policy: PolicyIdentity,
    pub(super) target: TargetIdentity,
    pub(super) observation: ObservationIdentity,
    pub(super) predecessor: LedgerIdentity,
    pub(super) planned_migrations: Vec<PlannedMigrationIdentity>,
    pub(super) schema_window: SchemaWindow,
    pub(super) preconditions: Vec<EvidenceReference>,
    pub(super) recovery_strategy: RecoveryStrategy,
    pub(super) expected_post_state: ExpectedPostState,
    pub(super) allowed_provider_effects: Vec<String>,
    pub(super) forbidden_provider_effects: Vec<String>,
}

impl MigrationTransactionPlan {
    pub(super) fn canonical_json(&self) -> Result<String, D1Error> {
        self.validate()?;
        let value = serde_json::to_value(self).map_err(|error| {
            D1Error::new(format!("cannot project D1 transaction plan to JSON: {error}"))
        })?;
        canonical_json(&value).map_err(D1Error::new)
    }

    pub(super) fn transaction_id(&self) -> Result<String, D1Error> {
        let canonical = self.canonical_json()?;
        Ok(format!(
            "d1-transaction-v1-sha256-{}",
            sha256_hex(canonical.as_bytes())
        ))
    }

    pub(super) fn envelope_json(&self) -> Result<Value, D1Error> {
        let canonical = self.canonical_json()?;
        let transaction_id = format!(
            "d1-transaction-v1-sha256-{}",
            sha256_hex(canonical.as_bytes())
        );
        let plan = serde_json::from_str::<Value>(&canonical).map_err(|error| {
            D1Error::new(format!(
                "cannot parse canonical D1 transaction plan projection: {error}"
            ))
        })?;
        Ok(json!({
            "schema_version": 1,
            "transaction_id": transaction_id,
            "plan": plan,
        }))
    }

    fn validate(&self) -> Result<(), D1Error> {
        if self.schema_version != TRANSACTION_PLAN_SCHEMA_VERSION {
            return Err(D1Error::new(format!(
                "unsupported D1 transaction plan schema_version {}; expected {}",
                self.schema_version, TRANSACTION_PLAN_SCHEMA_VERSION
            )));
        }
        require_git_sha("source.source_sha", &self.source.source_sha)?;
        require_git_sha("source.tree_sha", &self.source.tree_sha)?;
        require_release_candidate_id(&self.release.release_candidate_id)?;
        for (field, digest) in [
            (
                "release.release_set_manifest_sha256",
                self.release.release_set_manifest_sha256.as_str(),
            ),
            (
                "release.component_manifest_sha256",
                self.release.component_manifest_sha256.as_str(),
            ),
            (
                "policy.planner_policy_sha256",
                self.policy.planner_policy_sha256.as_str(),
            ),
            (
                "policy.migration_lineage_sha256",
                self.policy.migration_lineage_sha256.as_str(),
            ),
            (
                "observation.observation_sha256",
                self.observation.observation_sha256.as_str(),
            ),
            (
                "predecessor.ledger_sha256",
                self.predecessor.ledger_sha256.as_str(),
            ),
            (
                "expected_post_state.ledger_sha256",
                self.expected_post_state.ledger_sha256.as_str(),
            ),
        ] {
            require_sha256(field, digest)?;
        }
        require_non_empty("target.environment", &self.target.environment)?;
        require_non_empty("target.account_id", &self.target.account_id)?;
        require_non_empty("target.database_component", &self.target.database_component)?;
        require_non_empty("target.database_name", &self.target.database_name)?;
        require_non_empty("target.database_id", &self.target.database_id)?;
        require_non_empty("observation.observed_at", &self.observation.observed_at)?;
        if self.observation.freshness_max_age_seconds == 0 {
            return Err(D1Error::new(
                "observation.freshness_max_age_seconds must be greater than zero",
            ));
        }
        for migration in &self.planned_migrations {
            require_non_empty("planned_migrations[].migration_file", &migration.migration_file)?;
            require_sha256(
                "planned_migrations[].content_sha256",
                &migration.content_sha256,
            )?;
        }
        for evidence in &self.preconditions {
            require_non_empty("preconditions[].evidence_class", &evidence.evidence_class)?;
            require_sha256("preconditions[].evidence_sha256", &evidence.evidence_sha256)?;
        }
        for field in [
            self.schema_window.target_revision.as_str(),
            self.schema_window.supported_schema_min.as_str(),
            self.schema_window.supported_schema_max.as_str(),
            self.expected_post_state.target_revision.as_str(),
        ] {
            require_non_empty("schema revision", field)?;
        }
        require_unique_non_empty(
            "allowed_provider_effects",
            &self.allowed_provider_effects,
        )?;
        require_unique_non_empty(
            "forbidden_provider_effects",
            &self.forbidden_provider_effects,
        )?;
        if self
            .allowed_provider_effects
            .iter()
            .any(|effect| self.forbidden_provider_effects.contains(effect))
        {
            return Err(D1Error::new(
                "a provider effect cannot be both allowed and forbidden in one D1 transaction plan",
            ));
        }
        Ok(())
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), D1Error> {
    if value.trim().is_empty() {
        return Err(D1Error::new(format!(
            "D1 transaction plan field {field} must not be empty"
        )));
    }
    Ok(())
}

fn require_git_sha(field: &str, value: &str) -> Result<(), D1Error> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(D1Error::new(format!(
            "D1 transaction plan field {field} must be one lowercase 40-hex Git SHA"
        )));
    }
    Ok(())
}

fn require_sha256(field: &str, value: &str) -> Result<(), D1Error> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(D1Error::new(format!(
            "D1 transaction plan field {field} must be one lowercase 64-hex SHA-256 digest"
        )));
    }
    Ok(())
}

fn require_release_candidate_id(value: &str) -> Result<(), D1Error> {
    const PREFIX: &str = "release-set-v3-sha256-";
    let Some(digest) = value.strip_prefix(PREFIX) else {
        return Err(D1Error::new(
            "D1 transaction plan release_candidate_id must use release-set-v3-sha256 identity",
        ));
    };
    require_sha256("release.release_candidate_id", digest)
}

fn require_unique_non_empty(field: &str, values: &[String]) -> Result<(), D1Error> {
    let mut sorted = values.to_vec();
    for value in &sorted {
        require_non_empty(field, value)?;
    }
    sorted.sort();
    if sorted.windows(2).any(|window| window[0] == window[1]) {
        return Err(D1Error::new(format!(
            "D1 transaction plan field {field} must not contain duplicates"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        EvidenceReference, ExpectedPostState, LedgerIdentity, MigrationTransactionPlan,
        ObservationIdentity, PlannedMigrationIdentity, PolicyIdentity, RecoveryStrategy,
        ReleaseIdentity, SchemaWindow, SourceIdentity, TargetIdentity, TransactionKind,
        TRANSACTION_PLAN_SCHEMA_VERSION,
    };

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn fixture() -> MigrationTransactionPlan {
        MigrationTransactionPlan {
            schema_version: TRANSACTION_PLAN_SCHEMA_VERSION,
            transaction_kind: TransactionKind::Ordinary,
            source: SourceIdentity {
                source_sha: "741f3d148f9ed20863f61e996b5329931528c142".to_owned(),
                tree_sha: "2ab38fc72c576c4a4dbd607cbfc6ca3c9c931388".to_owned(),
            },
            release: ReleaseIdentity {
                release_candidate_id: "release-set-v3-sha256-cf139252c26aec89f1c6d078b18a7f3fc5f22cdb438f59e3d765ddf88c356325".to_owned(),
                release_set_manifest_sha256: digest('a'),
                component_manifest_sha256: digest('b'),
            },
            policy: PolicyIdentity {
                planner_policy_sha256: digest('c'),
                migration_lineage_sha256: digest('d'),
            },
            target: TargetIdentity {
                environment: "tx3-fixture".to_owned(),
                account_id: "account-fixture".to_owned(),
                database_component: "catalog".to_owned(),
                database_name: "catalog-fixture".to_owned(),
                database_id: "00000000-0000-4000-8000-000000000001".to_owned(),
            },
            observation: ObservationIdentity {
                observation_sha256: digest('e'),
                observed_at: "2026-09-05T20:22:54Z".to_owned(),
                freshness_max_age_seconds: 900,
            },
            predecessor: LedgerIdentity {
                ledger_sha256: digest('f'),
                remote_revision: Some("0026_outbound_mail_intents.sql".to_owned()),
            },
            planned_migrations: vec![PlannedMigrationIdentity {
                migration_file: "0027_pas2_payload_fingerprint_expand.sql".to_owned(),
                content_sha256: digest('1'),
            }],
            schema_window: SchemaWindow {
                target_revision: "0031_device_binding_governance.sql".to_owned(),
                supported_schema_min: "0031_device_binding_governance.sql".to_owned(),
                supported_schema_max: "0032_pas2_payload_fingerprint_contract.sql".to_owned(),
            },
            preconditions: vec![EvidenceReference {
                evidence_class: "typed-preconditions".to_owned(),
                evidence_sha256: digest('2'),
            }],
            recovery_strategy: RecoveryStrategy::ForwardOnly,
            expected_post_state: ExpectedPostState {
                ledger_sha256: digest('3'),
                target_revision: "0031_device_binding_governance.sql".to_owned(),
            },
            allowed_provider_effects: vec!["D1_MIGRATIONS_APPLY_EXACT_PLAN".to_owned()],
            forbidden_provider_effects: vec![
                "D1_CREATE_DATABASE".to_owned(),
                "D1_TIME_TRAVEL_RESTORE".to_owned(),
            ],
        }
    }

    #[test]
    fn canonical_transaction_identity_is_deterministic_and_self_excluding() -> Result<(), super::D1Error> {
        let first = fixture();
        let second = fixture();
        assert_eq!(first.canonical_json()?, second.canonical_json()?);
        assert_eq!(first.transaction_id()?, second.transaction_id()?);
        let envelope = first.envelope_json()?;
        assert_eq!(
            envelope.get("transaction_id").and_then(serde_json::Value::as_str),
            Some(first.transaction_id()?.as_str())
        );
        assert!(
            first
                .canonical_json()?
                .find("transaction_id")
                .is_none(),
            "transaction_id must hash only the canonical plan, never itself"
        );
        Ok(())
    }

    #[test]
    fn security_relevant_drift_changes_transaction_identity() -> Result<(), super::D1Error> {
        let baseline = fixture().transaction_id()?;

        let mut source = fixture();
        source.source.source_sha = "841f3d148f9ed20863f61e996b5329931528c142".to_owned();
        assert_ne!(baseline, source.transaction_id()?);

        let mut release = fixture();
        release.release.release_set_manifest_sha256 = digest('4');
        assert_ne!(baseline, release.transaction_id()?);

        let mut observation = fixture();
        observation.observation.observation_sha256 = digest('5');
        assert_ne!(baseline, observation.transaction_id()?);

        let mut target = fixture();
        target.target.database_id = "00000000-0000-4000-8000-000000000002".to_owned();
        assert_ne!(baseline, target.transaction_id()?);

        let mut migration = fixture();
        migration.planned_migrations[0].content_sha256 = digest('6');
        assert_ne!(baseline, migration.transaction_id()?);

        let mut policy = fixture();
        policy.policy.planner_policy_sha256 = digest('7');
        assert_ne!(baseline, policy.transaction_id()?);
        Ok(())
    }

    #[test]
    fn invalid_or_ambiguous_identity_inputs_fail_closed() {
        let mut invalid_source = fixture();
        invalid_source.source.source_sha = "HEAD".to_owned();
        assert!(invalid_source.transaction_id().is_err());

        let mut invalid_release = fixture();
        invalid_release.release.release_candidate_id = "latest".to_owned();
        assert!(invalid_release.transaction_id().is_err());

        let mut stale_forever = fixture();
        stale_forever.observation.freshness_max_age_seconds = 0;
        assert!(stale_forever.transaction_id().is_err());

        let mut duplicate_effect = fixture();
        duplicate_effect.allowed_provider_effects.push(
            duplicate_effect.allowed_provider_effects[0].clone(),
        );
        assert!(duplicate_effect.transaction_id().is_err());

        let mut contradictory_effect = fixture();
        contradictory_effect
            .forbidden_provider_effects
            .push("D1_MIGRATIONS_APPLY_EXACT_PLAN".to_owned());
        assert!(contradictory_effect.transaction_id().is_err());
    }

    #[test]
    fn contract_transaction_kind_and_recovery_are_identity_bound() -> Result<(), super::D1Error> {
        let ordinary = fixture().transaction_id()?;
        let mut contract = fixture();
        contract.transaction_kind = TransactionKind::Contract;
        contract.recovery_strategy = RecoveryStrategy::FailForwardOnly;
        assert_ne!(ordinary, contract.transaction_id()?);
        Ok(())
    }
}
