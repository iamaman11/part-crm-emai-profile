use super::model::{D1Error, GateResult};
use super::transaction::{RecoveryStrategy, TargetIdentity, TransactionProjection};
use super::transaction_integrity::revalidate_transaction_projection;
use crate::canonical::canonical_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::str::FromStr;

const OPERATOR_OUTCOME_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum D1OperatorOutcomeKind {
    PrepareBlocked,
    AuthorizationRequired,
    StaleObservation,
    NoAuthorization,
    MultipleAuthorizations,
    InvalidAuthorization,
    StaleAuthorization,
    SourceTreeTransactionDrift,
    TargetPrestateDrift,
    PrewriteAbort,
    ExecutorFailedNoEffect,
    RecoveryRequired,
    CompletedVerified,
    ReplayOrNoop,
    ReceiptOrEvidenceMissingOrMismatched,
}

impl D1OperatorOutcomeKind {
    pub const ALL: [Self; 15] = [
        Self::PrepareBlocked,
        Self::AuthorizationRequired,
        Self::StaleObservation,
        Self::NoAuthorization,
        Self::MultipleAuthorizations,
        Self::InvalidAuthorization,
        Self::StaleAuthorization,
        Self::SourceTreeTransactionDrift,
        Self::TargetPrestateDrift,
        Self::PrewriteAbort,
        Self::ExecutorFailedNoEffect,
        Self::RecoveryRequired,
        Self::CompletedVerified,
        Self::ReplayOrNoop,
        Self::ReceiptOrEvidenceMissingOrMismatched,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrepareBlocked => "PREPARE_BLOCKED",
            Self::AuthorizationRequired => "AUTHORIZATION_REQUIRED",
            Self::StaleObservation => "STALE_OBSERVATION",
            Self::NoAuthorization => "NO_AUTHORIZATION",
            Self::MultipleAuthorizations => "MULTIPLE_AUTHORIZATIONS",
            Self::InvalidAuthorization => "INVALID_AUTHORIZATION",
            Self::StaleAuthorization => "STALE_AUTHORIZATION",
            Self::SourceTreeTransactionDrift => "SOURCE_TREE_TRANSACTION_DRIFT",
            Self::TargetPrestateDrift => "TARGET_PRESTATE_DRIFT",
            Self::PrewriteAbort => "PREWRITE_ABORT",
            Self::ExecutorFailedNoEffect => "EXECUTOR_FAILED_NO_EFFECT",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
            Self::CompletedVerified => "COMPLETED_VERIFIED",
            Self::ReplayOrNoop => "REPLAY_OR_NOOP",
            Self::ReceiptOrEvidenceMissingOrMismatched => {
                "RECEIPT_OR_EVIDENCE_MISSING_OR_MISMATCHED"
            }
        }
    }

    #[must_use]
    pub const fn status(self) -> &'static str {
        match self {
            Self::AuthorizationRequired | Self::NoAuthorization => "ACTION_REQUIRED",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
            Self::CompletedVerified => "COMPLETED",
            Self::ReplayOrNoop => "NOOP",
            Self::ExecutorFailedNoEffect | Self::PrewriteAbort => "FAILED_NO_EFFECT",
            Self::PrepareBlocked
            | Self::StaleObservation
            | Self::MultipleAuthorizations
            | Self::InvalidAuthorization
            | Self::StaleAuthorization
            | Self::SourceTreeTransactionDrift
            | Self::TargetPrestateDrift
            | Self::ReceiptOrEvidenceMissingOrMismatched => "BLOCKED",
        }
    }

    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::PrepareBlocked => {
                "Canonical D1 Prepare rejected the current observed target or inputs."
            }
            Self::AuthorizationRequired => {
                "A fresh immutable transaction is prepared and requires one exact transaction-scoped authorization."
            }
            Self::StaleObservation => {
                "The provider observation bound to the operation is outside its typed freshness window."
            }
            Self::NoAuthorization => {
                "No valid exact authorization exists for the current fresh transaction."
            }
            Self::MultipleAuthorizations => {
                "More than one valid exact authorization exists for the same transaction."
            }
            Self::InvalidAuthorization => {
                "An authorization candidate exists but does not satisfy the typed transaction authorization contract."
            }
            Self::StaleAuthorization => {
                "The exact authorization is no longer valid within its typed expiry/freshness boundary."
            }
            Self::SourceTreeTransactionDrift => {
                "The prepared transaction no longer binds the exact checked-out source/tree transaction identity."
            }
            Self::TargetPrestateDrift => {
                "The provider target no longer matches the immutable prepared predecessor state."
            }
            Self::PrewriteAbort => {
                "The sole executor aborted before a provider write because a fail-closed admission fence did not pass."
            }
            Self::ExecutorFailedNoEffect => {
                "The sole executor failed and typed receipt/post-state evidence verifies that no provider effect occurred."
            }
            Self::RecoveryRequired => {
                "The sole executor receipt and post-state evidence require explicit recovery disposition."
            }
            Self::CompletedVerified => {
                "The sole executor completed and fresh typed post-state evidence verifies the expected state."
            }
            Self::ReplayOrNoop => {
                "The operation is a mechanical replay/no-op and requires no new provider effect."
            }
            Self::ReceiptOrEvidenceMissingOrMismatched => {
                "Terminal receipt or required evidence is missing, ambiguous, stale, or does not bind the exact transaction."
            }
        }
    }

    #[must_use]
    pub const fn remediation(self) -> &'static str {
        match self {
            Self::PrepareBlocked => {
                "Use the canonical PREPARE_BLOCKED GateResult remediation, repair only the named natural-owner condition, then rerun the zero-input operator."
            }
            Self::AuthorizationRequired => {
                "Record exactly one fresh immutable OWNER authorization for this TransactionId in the CURRENT stage issue, then rerun the zero-input operator once."
            }
            Self::StaleObservation => {
                "Run the existing read-only observation owner again and rebuild Prepare; do not reuse the stale transaction or authorization."
            }
            Self::NoAuthorization => {
                "Do not dispatch the executor. Record exactly one valid transaction-scoped authorization or allow the transaction to expire and rebuild it."
            }
            Self::MultipleAuthorizations => {
                "Do not dispatch the executor. Resolve the ambiguous authorization set in the CURRENT stage issue and prepare a fresh transaction before retrying."
            }
            Self::InvalidAuthorization => {
                "Use the embedded typed authorization diagnostic, correct the authorization envelope without broadening effect scope, and rerun only while the transaction remains fresh."
            }
            Self::StaleAuthorization => {
                "Do not reuse the expired authorization. Re-observe/re-prepare if necessary and obtain a new exact transaction-scoped authorization."
            }
            Self::SourceTreeTransactionDrift => {
                "Discard the stale/drifted transaction, establish fresh protected-main authority, then re-observe and re-prepare before requesting authorization."
            }
            Self::TargetPrestateDrift => {
                "Do not write. Re-observe the exact target and build a new canonical Prepare/TransactionId for the newly observed predecessor state."
            }
            Self::PrewriteAbort => {
                "Do not bypass the failing fence. Use the executor/receipt diagnostic to repair the named pre-write condition, then start again from fresh observation and Prepare."
            }
            Self::ExecutorFailedNoEffect => {
                "Use the terminal ExecutionReceipt diagnostic, repair the executor failure, then restart from fresh observation/Prepare with a new authorization if a write is still required."
            }
            Self::RecoveryRequired => {
                "Stop automatic progression. Follow the typed receipt/post-state recovery disposition through the existing recovery owner; do not auto-restore or silently retry."
            }
            Self::CompletedVerified => {
                "Record the immutable receipt and post-state evidence locators in the CURRENT stage issue; no recovery action is required."
            }
            Self::ReplayOrNoop => {
                "Record the mechanical no-op/replay proof; do not consume stale authorization or introduce a synthetic write merely to create evidence."
            }
            Self::ReceiptOrEvidenceMissingOrMismatched => {
                "Fail closed. Reconcile the exact sole-executor attempt and existing immutable evidence owners; never infer success from logs or target revision alone."
            }
        }
    }
}

impl FromStr for D1OperatorOutcomeKind {
    type Err = D1Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| D1Error::new(format!("unsupported D1 operator outcome kind: {value}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1OperatorOutcomeContext {
    pub source_sha: String,
    pub tree_sha: String,
    pub current_stage_issue: u64,
    pub transaction_id: Option<String>,
    pub target: Option<TargetIdentity>,
    pub owner_diagnostic: Option<Value>,
    pub evidence_refs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct D1OperatorOutcome {
    pub schema_version: u64,
    pub contract: &'static str,
    pub status: &'static str,
    pub outcome: &'static str,
    pub reason_code: &'static str,
    pub summary: &'static str,
    pub remediation: &'static str,
    pub mode: &'static str,
    pub operator_has_provider_credentials: bool,
    pub source_sha: String,
    pub tree_sha: String,
    pub current_stage_issue: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_diagnostic: Option<Value>,
    pub evidence_refs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct D1OperatorTransactionVerification {
    pub schema_version: u64,
    pub status: &'static str,
    pub mode: &'static str,
    pub authorization_consumed: bool,
    pub mutation_executed: bool,
    pub provider_mutation_executed: bool,
    pub transaction_id: String,
    pub source_sha: String,
    pub tree_sha: String,
    pub release_candidate_id: String,
    pub component: String,
    pub target: TargetIdentity,
    pub fresh_until_unix_seconds: i64,
    pub schema_target: String,
    pub recovery_strategy: RecoveryStrategy,
}

pub fn build_operator_outcome(
    kind: D1OperatorOutcomeKind,
    context: D1OperatorOutcomeContext,
) -> Result<D1OperatorOutcome, D1Error> {
    validate_hex(&context.source_sha, 40, "operator outcome source_sha")?;
    validate_hex(&context.tree_sha, 40, "operator outcome tree_sha")?;
    if context.current_stage_issue == 0 {
        return Err(D1Error::new(
            "operator outcome current_stage_issue must be positive",
        ));
    }
    if let Some(transaction_id) = context.transaction_id.as_deref() {
        validate_hex(transaction_id, 64, "operator outcome transaction_id")?;
    }
    if let Some(target) = context.target.as_ref() {
        for (label, value) in [
            ("target.environment", target.environment.as_str()),
            ("target.account_id", target.account_id.as_str()),
            ("target.database_name", target.database_name.as_str()),
            ("target.database_id", target.database_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(D1Error::new(format!(
                    "operator outcome {label} must not be empty"
                )));
            }
        }
    }
    if let Some(diagnostic) = context.owner_diagnostic.as_ref() {
        validate_owner_diagnostic(diagnostic)?;
    }
    for (name, reference) in &context.evidence_refs {
        if name.trim().is_empty() || reference.trim().is_empty() {
            return Err(D1Error::new(
                "operator outcome evidence refs require non-empty names and references",
            ));
        }
    }

    Ok(D1OperatorOutcome {
        schema_version: OPERATOR_OUTCOME_SCHEMA_VERSION,
        contract: "D1_OPERATOR_OUTCOME_V1",
        status: kind.status(),
        outcome: kind.as_str(),
        reason_code: kind.as_str(),
        summary: kind.summary(),
        remediation: kind.remediation(),
        mode: "read-only",
        operator_has_provider_credentials: false,
        source_sha: context.source_sha,
        tree_sha: context.tree_sha,
        current_stage_issue: context.current_stage_issue,
        transaction_id: context.transaction_id,
        target: context.target,
        owner_diagnostic: context.owner_diagnostic,
        evidence_refs: context.evidence_refs,
    })
}

pub fn verify_operator_transaction(
    transaction: &TransactionProjection,
    expected_source_sha: &str,
    expected_tree_sha: &str,
    expected_environment: &str,
    evaluated_at_unix_seconds: i64,
) -> Result<D1OperatorTransactionVerification, D1Error> {
    revalidate_transaction_projection(transaction).map_err(|error| {
        operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            format!("prepared transaction integrity revalidation failed: {error}"),
        )
    })?;

    let plan = &transaction.transaction_plan;
    if plan.source_sha != expected_source_sha {
        return Err(operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "prepared transaction source_sha does not equal exact checked-out source",
        ));
    }
    if plan.tree_sha != expected_tree_sha {
        return Err(operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "prepared transaction tree_sha does not equal exact checked-out tree",
        ));
    }
    if plan.target.environment != expected_environment {
        return Err(operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "prepared transaction target environment does not equal operator environment",
        ));
    }
    if expected_environment != "staging" {
        return Err(operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "ordinary D1 operator currently permits staging only",
        ));
    }
    if evaluated_at_unix_seconds <= 0 {
        return Err(operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "transaction evaluation timestamp must be positive",
        ));
    }

    let freshness = i64::try_from(plan.freshness_max_age_seconds).map_err(|_| {
        operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "transaction freshness window does not fit i64",
        )
    })?;
    let fresh_until_unix_seconds = plan
        .observed_at_unix_seconds
        .checked_add(freshness)
        .ok_or_else(|| {
            operator_block(
                D1OperatorOutcomeKind::SourceTreeTransactionDrift,
                "transaction freshness deadline overflow",
            )
        })?;
    if evaluated_at_unix_seconds > fresh_until_unix_seconds {
        return Err(operator_block(
            D1OperatorOutcomeKind::StaleObservation,
            "prepared provider observation is stale",
        ));
    }

    let components = plan.release_manifest_digests.keys().collect::<Vec<_>>();
    if components.len() != 1 {
        return Err(operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "prepared ordinary transaction must bind exactly one release-manifest component",
        ));
    }
    let component = components[0].as_str();
    if !matches!(component, "catalog" | "resolver") {
        return Err(operator_block(
            D1OperatorOutcomeKind::SourceTreeTransactionDrift,
            "prepared transaction component is unsupported",
        ));
    }

    Ok(D1OperatorTransactionVerification {
        schema_version: OPERATOR_OUTCOME_SCHEMA_VERSION,
        status: "TRANSACTION_VERIFIED",
        mode: "read-only",
        authorization_consumed: false,
        mutation_executed: false,
        provider_mutation_executed: false,
        transaction_id: transaction.transaction_id.clone(),
        source_sha: plan.source_sha.clone(),
        tree_sha: plan.tree_sha.clone(),
        release_candidate_id: plan.release_candidate_id.clone(),
        component: component.to_owned(),
        target: plan.target.clone(),
        fresh_until_unix_seconds,
        schema_target: plan.schema_target.clone(),
        recovery_strategy: plan.recovery_strategy,
    })
}

pub fn serialize_operator_outcome(outcome: &D1OperatorOutcome) -> Result<String, D1Error> {
    let value = serde_json::to_value(outcome)
        .map_err(|error| D1Error::new(format!("cannot serialize D1 operator outcome: {error}")))?;
    canonical_json(&value).map_err(D1Error::new)
}

pub fn serialize_operator_transaction_verification(
    verification: &D1OperatorTransactionVerification,
) -> Result<String, D1Error> {
    let value = serde_json::to_value(verification).map_err(|error| {
        D1Error::new(format!(
            "cannot serialize D1 operator transaction verification: {error}"
        ))
    })?;
    canonical_json(&value).map_err(D1Error::new)
}

fn operator_block(kind: D1OperatorOutcomeKind, summary: impl Into<String>) -> D1Error {
    D1Error::blocked(GateResult::blocked(
        "OPERATOR",
        "d1.operator.transaction",
        kind.as_str(),
        summary,
        None,
        None,
        kind.remediation(),
    ))
}

fn validate_owner_diagnostic(value: &Value) -> Result<(), D1Error> {
    let object = value
        .as_object()
        .ok_or_else(|| D1Error::new("operator owner_diagnostic must be one typed JSON object"))?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(D1Error::new(
            "operator owner_diagnostic must use typed D1 diagnostic schema_version=1",
        ));
    }
    for field in ["reason_code", "remediation"] {
        if object
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(D1Error::new(format!(
                "operator owner_diagnostic is missing non-empty {field}"
            )));
        }
    }
    let tool = object
        .get("tool")
        .and_then(Value::as_object)
        .ok_or_else(|| D1Error::new("operator owner_diagnostic is missing typed tool identity"))?;
    if tool.get("name").and_then(Value::as_str) != Some("opsctl")
        || tool.get("surface").and_then(Value::as_str) != Some("d1")
    {
        return Err(D1Error::new(
            "operator owner_diagnostic must originate from the typed opsctl d1 owner",
        ));
    }
    Ok(())
}

fn validate_hex(value: &str, length: usize, label: &str) -> Result<(), D1Error> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(D1Error::new(format!(
            "{label} must be exactly {length} lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context() -> D1OperatorOutcomeContext {
        D1OperatorOutcomeContext {
            source_sha: "aa".repeat(20),
            tree_sha: "bb".repeat(20),
            current_stage_issue: 642,
            transaction_id: Some("cc".repeat(32)),
            target: Some(TargetIdentity {
                environment: "staging".to_owned(),
                account_id: "account-1".to_owned(),
                database_name: "d1-rehearsal".to_owned(),
                database_id: "database-1".to_owned(),
            }),
            owner_diagnostic: None,
            evidence_refs: BTreeMap::new(),
        }
    }

    #[test]
    fn every_required_kind_has_stable_reason_and_remediation() -> Result<(), D1Error> {
        for kind in D1OperatorOutcomeKind::ALL {
            let outcome = build_operator_outcome(kind, context())?;
            assert_eq!(outcome.contract, "D1_OPERATOR_OUTCOME_V1");
            assert_eq!(outcome.outcome, kind.as_str());
            assert_eq!(outcome.reason_code, kind.as_str());
            assert!(!outcome.summary.is_empty());
            assert!(!outcome.remediation.is_empty());
            assert_eq!(outcome.mode, "read-only");
            assert!(!outcome.operator_has_provider_credentials);
        }
        Ok(())
    }

    #[test]
    fn serialization_is_canonical_and_secret_free_by_contract() -> Result<(), D1Error> {
        let mut input = context();
        input.evidence_refs.insert(
            "receipt".to_owned(),
            "github-actions:run:123:artifact:456".to_owned(),
        );
        let outcome = build_operator_outcome(D1OperatorOutcomeKind::CompletedVerified, input)?;
        let serialized = serialize_operator_outcome(&outcome)?;
        let value: Value = serde_json::from_str(&serialized)
            .map_err(|error| D1Error::new(format!("cannot parse outcome test JSON: {error}")))?;
        assert_eq!(serialized, canonical_json(&value).map_err(D1Error::new)?);
        assert_eq!(value["outcome"], "COMPLETED_VERIFIED");
        assert_eq!(value["operator_has_provider_credentials"], false);
        Ok(())
    }

    #[test]
    fn typed_owner_diagnostic_may_be_embedded_without_reinterpretation() -> Result<(), D1Error> {
        let diagnostic = json!({
            "schema_version": 1,
            "reason_code": "D1_TEST_BLOCK",
            "remediation": "repair the typed owner condition",
            "tool": {"name": "opsctl", "surface": "d1", "version": "fixture"}
        });
        let mut input = context();
        input.owner_diagnostic = Some(diagnostic.clone());
        let outcome = build_operator_outcome(D1OperatorOutcomeKind::PrepareBlocked, input)?;
        assert_eq!(outcome.owner_diagnostic, Some(diagnostic));
        Ok(())
    }

    #[test]
    fn untyped_owner_diagnostic_is_rejected() {
        let mut input = context();
        input.owner_diagnostic = Some(json!({"reason_code": "raw-log"}));
        assert!(build_operator_outcome(D1OperatorOutcomeKind::PrepareBlocked, input).is_err());
    }
}
