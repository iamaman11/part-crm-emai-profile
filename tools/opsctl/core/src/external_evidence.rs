use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEvidencePolicyError {
    code: &'static str,
    detail: String,
}

impl ExternalEvidencePolicyError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl Display for ExternalEvidencePolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for ExternalEvidencePolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExternalGate {
    LegacyCredentialRotation,
    CloudflareEnvironment,
    WindowsPrimaryHost,
    WindowsSecondaryHost,
    TrustedWindowsSigning,
    OfflineKeyEscrowRestore,
    PrivacyRetentionApproval,
    ProductLicense,
    RealFingerprintCertification,
    ProductionDeviceKeyUnwrap,
    RemoteR2D1Atomicity,
    IndependentSecurityReview,
}

impl ExternalGate {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyCredentialRotation => "legacy_credential_rotation",
            Self::CloudflareEnvironment => "cloudflare_environment",
            Self::WindowsPrimaryHost => "windows_primary_host",
            Self::WindowsSecondaryHost => "windows_secondary_host",
            Self::TrustedWindowsSigning => "trusted_windows_signing",
            Self::OfflineKeyEscrowRestore => "offline_key_escrow_restore",
            Self::PrivacyRetentionApproval => "privacy_retention_approval",
            Self::ProductLicense => "product_license",
            Self::RealFingerprintCertification => "real_fingerprint_certification",
            Self::ProductionDeviceKeyUnwrap => "production_device_key_unwrap",
            Self::RemoteR2D1Atomicity => "remote_r2_d1_atomicity",
            Self::IndependentSecurityReview => "independent_security_review",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "legacy_credential_rotation" => Some(Self::LegacyCredentialRotation),
            "cloudflare_environment" => Some(Self::CloudflareEnvironment),
            "windows_primary_host" => Some(Self::WindowsPrimaryHost),
            "windows_secondary_host" => Some(Self::WindowsSecondaryHost),
            "trusted_windows_signing" => Some(Self::TrustedWindowsSigning),
            "offline_key_escrow_restore" => Some(Self::OfflineKeyEscrowRestore),
            "privacy_retention_approval" => Some(Self::PrivacyRetentionApproval),
            "product_license" => Some(Self::ProductLicense),
            "real_fingerprint_certification" => Some(Self::RealFingerprintCertification),
            "production_device_key_unwrap" => Some(Self::ProductionDeviceKeyUnwrap),
            "remote_r2_d1_atomicity" => Some(Self::RemoteR2D1Atomicity),
            "independent_security_review" => Some(Self::IndependentSecurityReview),
            _ => None,
        }
    }
}

pub const ALL_EXTERNAL_GATES: [ExternalGate; 12] = [
    ExternalGate::LegacyCredentialRotation,
    ExternalGate::CloudflareEnvironment,
    ExternalGate::WindowsPrimaryHost,
    ExternalGate::WindowsSecondaryHost,
    ExternalGate::TrustedWindowsSigning,
    ExternalGate::OfflineKeyEscrowRestore,
    ExternalGate::PrivacyRetentionApproval,
    ExternalGate::ProductLicense,
    ExternalGate::RealFingerprintCertification,
    ExternalGate::ProductionDeviceKeyUnwrap,
    ExternalGate::RemoteR2D1Atomicity,
    ExternalGate::IndependentSecurityReview,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExternalEnvironment {
    None,
    Dev,
    Staging,
    Production,
}

impl ExternalEnvironment {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "dev" => Some(Self::Dev),
            "staging" => Some(Self::Staging),
            "production" => Some(Self::Production),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalEvidenceStatus {
    Pending,
    Passed,
    Failed,
}

impl ExternalEvidenceStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "passed" => Some(Self::Passed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalCheckOutcome {
    Pass,
    Fail,
}

impl ExternalCheckOutcome {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pass" => Some(Self::Pass),
            "fail" => Some(Self::Fail),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEvidenceCheck {
    pub code: String,
    pub outcome: ExternalCheckOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEvidenceRecordV1 {
    pub evidence_id: String,
    pub evidence_date: u32,
    pub gate: ExternalGate,
    pub status: ExternalEvidenceStatus,
    pub observed_at_sort_key: u64,
    pub observed_date: u32,
    pub environment: ExternalEnvironment,
    pub checks: Vec<ExternalEvidenceCheck>,
    pub artifact_digest_count: usize,
    pub has_review: bool,
    pub reviewed_at_sort_key: Option<u64>,
    pub supersedes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalReadinessEntry {
    pub gate: ExternalGate,
    pub environment: ExternalEnvironment,
    pub evidence_id: String,
    pub status: ExternalEvidenceStatus,
    pub observed_date: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalReadinessSummaryV1 {
    pub active_records: Vec<ExternalReadinessEntry>,
    pub total_records: usize,
    pub active_pending: usize,
    pub active_passed: usize,
    pub active_failed: usize,
    pub mandatory_requirements: usize,
    pub satisfied_requirements: usize,
    pub missing_requirements: Vec<(ExternalGate, ExternalEnvironment)>,
    pub eligible_for_production_review: bool,
}

pub fn validate_external_evidence(
    records: &[ExternalEvidenceRecordV1],
) -> Result<ExternalReadinessSummaryV1, ExternalEvidencePolicyError> {
    for record in records {
        validate_record(record)?;
    }
    let active = validate_lineage(records)?;
    build_readiness(records, &active)
}

fn validate_record(record: &ExternalEvidenceRecordV1) -> Result<(), ExternalEvidencePolicyError> {
    if record.evidence_date != record.observed_date {
        return Err(ExternalEvidencePolicyError::new(
            "EXTERNAL_EVIDENCE_ID_DATE_MISMATCH",
            format!(
                "{} evidence-id date does not match observed UTC date",
                record.evidence_id
            ),
        ));
    }
    if !allowed_environments(record.gate).contains(&record.environment) {
        return Err(ExternalEvidencePolicyError::new(
            "EXTERNAL_EVIDENCE_ENVIRONMENT_INVALID",
            format!(
                "{} is not allowed for {}",
                record.environment.as_str(),
                record.gate.as_str()
            ),
        ));
    }
    if let Some(reviewed_at) = record.reviewed_at_sort_key
        && reviewed_at < record.observed_at_sort_key
    {
        return Err(ExternalEvidencePolicyError::new(
            "EXTERNAL_EVIDENCE_REVIEW_PRECEDES_OBSERVATION",
            format!("{} review predates observation", record.evidence_id),
        ));
    }

    let required = required_checks(record.gate);
    let mut seen = BTreeSet::new();
    let mut failed = false;
    for check in &record.checks {
        if !required.contains(&check.code.as_str()) {
            return Err(ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_CHECK_UNKNOWN",
                format!(
                    "{} check {} is not defined for {}",
                    record.evidence_id,
                    check.code,
                    record.gate.as_str()
                ),
            ));
        }
        if !seen.insert(check.code.as_str()) {
            return Err(ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_CHECK_DUPLICATE",
                format!("{} repeats check {}", record.evidence_id, check.code),
            ));
        }
        failed |= check.outcome == ExternalCheckOutcome::Fail;
    }

    match record.status {
        ExternalEvidenceStatus::Pending => {
            if record.has_review || failed {
                return Err(ExternalEvidencePolicyError::new(
                    "EXTERNAL_EVIDENCE_PENDING_TERMINAL_STATE",
                    format!(
                        "{} pending evidence cannot contain terminal review/failure",
                        record.evidence_id
                    ),
                ));
            }
        }
        ExternalEvidenceStatus::Passed => {
            if !record.has_review || record.artifact_digest_count == 0 {
                return Err(ExternalEvidencePolicyError::new(
                    "EXTERNAL_EVIDENCE_PASS_PROOF_INCOMPLETE",
                    format!(
                        "{} passed evidence requires terminal review and artifact digest",
                        record.evidence_id
                    ),
                ));
            }
            if failed || required.iter().any(|code| !seen.contains(code)) {
                return Err(ExternalEvidencePolicyError::new(
                    "EXTERNAL_EVIDENCE_PASS_CHECKS_INCOMPLETE",
                    format!(
                        "{} passed evidence does not prove every required check",
                        record.evidence_id
                    ),
                ));
            }
        }
        ExternalEvidenceStatus::Failed => {
            if !record.has_review || !failed {
                return Err(ExternalEvidencePolicyError::new(
                    "EXTERNAL_EVIDENCE_FAILED_STATE_INCOMPLETE",
                    format!(
                        "{} failed evidence requires terminal review and failed check",
                        record.evidence_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_lineage(
    records: &[ExternalEvidenceRecordV1],
) -> Result<Vec<&ExternalEvidenceRecordV1>, ExternalEvidencePolicyError> {
    let mut by_id = BTreeMap::new();
    for record in records {
        if by_id.insert(record.evidence_id.as_str(), record).is_some() {
            return Err(ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_ID_DUPLICATE",
                format!("duplicate evidence id {}", record.evidence_id),
            ));
        }
    }

    let mut child_by_parent = BTreeMap::new();
    for record in records {
        let Some(parent_id) = record.supersedes.as_deref() else {
            continue;
        };
        let parent = by_id.get(parent_id).copied().ok_or_else(|| {
            ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_LINEAGE_DANGLING",
                format!("{} supersedes missing {parent_id}", record.evidence_id),
            )
        })?;
        if parent.gate != record.gate {
            return Err(ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_LINEAGE_GATE_MISMATCH",
                format!("{} crosses gate lineage", record.evidence_id),
            ));
        }
        if record.observed_at_sort_key <= parent.observed_at_sort_key {
            return Err(ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_LINEAGE_NOT_NEWER",
                format!("{} must be newer than {parent_id}", record.evidence_id),
            ));
        }
        if child_by_parent
            .insert(parent_id, record.evidence_id.as_str())
            .is_some()
        {
            return Err(ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_LINEAGE_FORK",
                format!("{parent_id} has more than one successor"),
            ));
        }
    }

    for record in records {
        let mut current = record;
        let mut seen = BTreeSet::new();
        while let Some(parent_id) = current.supersedes.as_deref() {
            if !seen.insert(current.evidence_id.as_str()) {
                return Err(ExternalEvidencePolicyError::new(
                    "EXTERNAL_EVIDENCE_LINEAGE_CYCLE",
                    format!("cycle detected at {}", current.evidence_id),
                ));
            }
            current = by_id.get(parent_id).copied().ok_or_else(|| {
                ExternalEvidencePolicyError::new(
                    "EXTERNAL_EVIDENCE_LINEAGE_DANGLING",
                    format!("missing lineage parent {parent_id}"),
                )
            })?;
        }
    }

    let mut active_by_gate = BTreeMap::new();
    for record in records {
        if child_by_parent.contains_key(record.evidence_id.as_str()) {
            continue;
        }
        if let Some(previous) = active_by_gate.insert(record.gate, record) {
            return Err(ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_MULTIPLE_ACTIVE_LINEAGES",
                format!(
                    "{} and {} are both active for {}",
                    previous.evidence_id,
                    record.evidence_id,
                    record.gate.as_str()
                ),
            ));
        }
    }
    Ok(active_by_gate.into_values().collect())
}

fn build_readiness(
    records: &[ExternalEvidenceRecordV1],
    active: &[&ExternalEvidenceRecordV1],
) -> Result<ExternalReadinessSummaryV1, ExternalEvidencePolicyError> {
    let mut active_records = active
        .iter()
        .map(|record| ExternalReadinessEntry {
            gate: record.gate,
            environment: record.environment,
            evidence_id: record.evidence_id.clone(),
            status: record.status,
            observed_date: record.observed_date,
        })
        .collect::<Vec<_>>();
    active_records.sort_by(|left, right| {
        left.gate
            .cmp(&right.gate)
            .then(left.environment.cmp(&right.environment))
            .then(left.evidence_id.cmp(&right.evidence_id))
    });

    let passed = active
        .iter()
        .filter(|record| record.status == ExternalEvidenceStatus::Passed)
        .map(|record| (record.gate, record.environment))
        .collect::<BTreeSet<_>>();
    let missing_requirements = mandatory_requirements()
        .iter()
        .copied()
        .filter(|requirement| !passed.contains(requirement))
        .collect::<Vec<_>>();
    let mandatory_count = mandatory_requirements().len();
    let satisfied_requirements = mandatory_count
        .checked_sub(missing_requirements.len())
        .ok_or_else(|| {
            ExternalEvidencePolicyError::new(
                "EXTERNAL_EVIDENCE_READINESS_COUNT_INVALID",
                "missing requirements exceeded mandatory requirements",
            )
        })?;

    Ok(ExternalReadinessSummaryV1 {
        active_pending: active
            .iter()
            .filter(|record| record.status == ExternalEvidenceStatus::Pending)
            .count(),
        active_passed: active
            .iter()
            .filter(|record| record.status == ExternalEvidenceStatus::Passed)
            .count(),
        active_failed: active
            .iter()
            .filter(|record| record.status == ExternalEvidenceStatus::Failed)
            .count(),
        active_records,
        total_records: records.len(),
        mandatory_requirements: mandatory_count,
        satisfied_requirements,
        eligible_for_production_review: missing_requirements.is_empty(),
        missing_requirements,
    })
}

#[must_use]
pub const fn mandatory_requirements() -> &'static [(ExternalGate, ExternalEnvironment)] {
    &[
        (ExternalGate::CloudflareEnvironment, ExternalEnvironment::Production),
        (ExternalGate::IndependentSecurityReview, ExternalEnvironment::None),
        (ExternalGate::LegacyCredentialRotation, ExternalEnvironment::None),
        (ExternalGate::OfflineKeyEscrowRestore, ExternalEnvironment::Production),
        (ExternalGate::PrivacyRetentionApproval, ExternalEnvironment::None),
        (ExternalGate::ProductLicense, ExternalEnvironment::None),
        (ExternalGate::ProductionDeviceKeyUnwrap, ExternalEnvironment::Production),
        (ExternalGate::RealFingerprintCertification, ExternalEnvironment::Production),
        (ExternalGate::RemoteR2D1Atomicity, ExternalEnvironment::Production),
        (ExternalGate::TrustedWindowsSigning, ExternalEnvironment::Production),
        (ExternalGate::WindowsPrimaryHost, ExternalEnvironment::Production),
        (ExternalGate::WindowsSecondaryHost, ExternalEnvironment::Production),
    ]
}

#[must_use]
pub const fn allowed_environments(gate: ExternalGate) -> &'static [ExternalEnvironment] {
    match gate {
        ExternalGate::LegacyCredentialRotation
        | ExternalGate::PrivacyRetentionApproval
        | ExternalGate::ProductLicense
        | ExternalGate::IndependentSecurityReview => &[ExternalEnvironment::None],
        ExternalGate::CloudflareEnvironment => &[
            ExternalEnvironment::Dev,
            ExternalEnvironment::Staging,
            ExternalEnvironment::Production,
        ],
        ExternalGate::WindowsPrimaryHost
        | ExternalGate::WindowsSecondaryHost
        | ExternalGate::OfflineKeyEscrowRestore
        | ExternalGate::RealFingerprintCertification
        | ExternalGate::RemoteR2D1Atomicity => &[
            ExternalEnvironment::Staging,
            ExternalEnvironment::Production,
        ],
        ExternalGate::TrustedWindowsSigning | ExternalGate::ProductionDeviceKeyUnwrap => {
            &[ExternalEnvironment::Production]
        }
    }
}

#[must_use]
pub const fn required_checks(gate: ExternalGate) -> &'static [&'static str] {
    match gate {
        ExternalGate::LegacyCredentialRotation => &[
            "old_credential_revoked",
            "old_credential_authentication_rejected",
            "provider_access_logs_reviewed",
            "replacement_in_approved_secret_store",
            "repository_regression_scan_passed",
        ],
        ExternalGate::CloudflareEnvironment => &[
            "isolated_resources_provisioned",
            "access_policy_enforced",
            "cost_limit_configured",
            "remote_smoke_passed",
        ],
        ExternalGate::WindowsPrimaryHost => &[
            "physical_host_attested",
            "bridge_release_executed",
            "real_camouhost_lifecycle_passed",
            "metadata_only_support_bundle_reviewed",
        ],
        ExternalGate::WindowsSecondaryHost => &[
            "independent_physical_host_attested",
            "device_grant_applied",
            "restore_and_launch_passed",
            "revocation_enforced",
        ],
        ExternalGate::TrustedWindowsSigning => &[
            "trusted_certificate_chain_verified",
            "signed_binary_digest_verified",
            "windows_verification_passed",
            "update_verification_passed",
        ],
        ExternalGate::OfflineKeyEscrowRestore => &[
            "dual_control_exercised",
            "clean_environment_restore_passed",
            "rotation_recovery_passed",
            "key_loss_policy_approved",
        ],
        ExternalGate::PrivacyRetentionApproval => &[
            "retention_values_approved",
            "acceptable_use_policy_approved",
            "export_delete_flow_approved",
            "support_access_policy_approved",
        ],
        ExternalGate::ProductLicense => &[
            "license_selected",
            "third_party_notices_reviewed",
            "redistribution_rights_approved",
        ],
        ExternalGate::RealFingerprintCertification => &[
            "ten_cold_launches_completed",
            "profile_stable_signals_passed",
            "origin_deterministic_signals_passed",
            "network_coherence_passed",
            "specialized_sites_reviewed",
            "no_unresolved_fail_signals",
            "cross_profile_isolation_passed",
        ],
        ExternalGate::ProductionDeviceKeyUnwrap => &[
            "os_key_protection_verified",
            "device_identity_verified",
            "unwrap_authorization_verified",
            "revocation_verified",
            "recovery_path_verified",
        ],
        ExternalGate::RemoteR2D1Atomicity => &[
            "immutable_upload_verified",
            "pointer_cas_verified",
            "nonce_claim_verified",
            "rollback_verified",
            "orphan_reconciliation_verified",
        ],
        ExternalGate::IndependentSecurityReview => &[
            "reviewer_independence_confirmed",
            "threat_model_scope_reviewed",
            "cryptographic_scope_reviewed",
            "findings_resolved_or_accepted",
            "residual_risk_accepted",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExternalCheckOutcome, ExternalEnvironment, ExternalEvidenceCheck, ExternalEvidenceRecordV1,
        ExternalEvidenceStatus, ExternalGate, mandatory_requirements, required_checks,
        validate_external_evidence,
    };

    fn passed_record(
        evidence_id: &str,
        gate: ExternalGate,
        environment: ExternalEnvironment,
        date: u32,
        observed: u64,
    ) -> ExternalEvidenceRecordV1 {
        ExternalEvidenceRecordV1 {
            evidence_id: evidence_id.to_owned(),
            evidence_date: date,
            gate,
            status: ExternalEvidenceStatus::Passed,
            observed_at_sort_key: observed,
            observed_date: date,
            environment,
            checks: required_checks(gate)
                .iter()
                .map(|code| ExternalEvidenceCheck {
                    code: (*code).to_owned(),
                    outcome: ExternalCheckOutcome::Pass,
                })
                .collect(),
            artifact_digest_count: 1,
            has_review: true,
            reviewed_at_sort_key: Some(observed + 1),
            supersedes: None,
        }
    }

    #[test]
    fn complete_mandatory_evidence_is_eligible() -> Result<(), Box<dyn std::error::Error>> {
        let records = mandatory_requirements()
            .iter()
            .enumerate()
            .map(|(index, (gate, environment))| {
                passed_record(
                    &format!("ev-20260824-gate-{index:02}"),
                    *gate,
                    *environment,
                    20_260_824,
                    20_260_824_120_000 + index as u64,
                )
            })
            .collect::<Vec<_>>();
        let summary = validate_external_evidence(&records)?;
        assert!(summary.eligible_for_production_review);
        assert_eq!(summary.satisfied_requirements, summary.mandatory_requirements);
        Ok(())
    }

    #[test]
    fn passed_record_requires_every_check_review_and_digest() {
        let mut record = passed_record(
            "ev-20260824-product-license",
            ExternalGate::ProductLicense,
            ExternalEnvironment::None,
            20_260_824,
            20_260_824_120_000,
        );
        record.checks.pop();
        assert_eq!(
            validate_external_evidence(&[record])
                .err()
                .map(|error| error.code()),
            Some("EXTERNAL_EVIDENCE_PASS_CHECKS_INCOMPLETE")
        );
    }

    #[test]
    fn environment_and_date_drift_fail_closed() {
        let mut environment = passed_record(
            "ev-20260824-trusted-signing",
            ExternalGate::TrustedWindowsSigning,
            ExternalEnvironment::Staging,
            20_260_824,
            20_260_824_120_000,
        );
        assert_eq!(
            validate_external_evidence(&[environment.clone()])
                .err()
                .map(|error| error.code()),
            Some("EXTERNAL_EVIDENCE_ENVIRONMENT_INVALID")
        );
        environment.environment = ExternalEnvironment::Production;
        environment.evidence_date = 20_260_823;
        assert_eq!(
            validate_external_evidence(&[environment])
                .err()
                .map(|error| error.code()),
            Some("EXTERNAL_EVIDENCE_ID_DATE_MISMATCH")
        );
    }

    #[test]
    fn dangling_and_forked_lineage_fail_closed() {
        let mut dangling = passed_record(
            "ev-20260824-product-license",
            ExternalGate::ProductLicense,
            ExternalEnvironment::None,
            20_260_824,
            20_260_824_120_000,
        );
        dangling.supersedes = Some("ev-20260823-missing".to_owned());
        assert_eq!(
            validate_external_evidence(&[dangling])
                .err()
                .map(|error| error.code()),
            Some("EXTERNAL_EVIDENCE_LINEAGE_DANGLING")
        );

        let parent = passed_record(
            "ev-20260822-product-license",
            ExternalGate::ProductLicense,
            ExternalEnvironment::None,
            20_260_822,
            20_260_822_120_000,
        );
        let mut left = passed_record(
            "ev-20260823-product-license-left",
            ExternalGate::ProductLicense,
            ExternalEnvironment::None,
            20_260_823,
            20_260_823_120_000,
        );
        left.supersedes = Some(parent.evidence_id.clone());
        let mut right = passed_record(
            "ev-20260824-product-license-right",
            ExternalGate::ProductLicense,
            ExternalEnvironment::None,
            20_260_824,
            20_260_824_120_000,
        );
        right.supersedes = Some(parent.evidence_id.clone());
        assert_eq!(
            validate_external_evidence(&[parent, left, right])
                .err()
                .map(|error| error.code()),
            Some("EXTERNAL_EVIDENCE_LINEAGE_FORK")
        );
    }
}
