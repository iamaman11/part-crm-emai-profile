use std::fmt::{Display, Formatter};

const MAX_IDENTIFIER_BYTES: usize = 256;
const REVIEW_CLAIM_DOMAIN: &str = "external-evidence-review-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePolicyError {
    code: &'static str,
    detail: String,
}

impl EvidencePolicyError {
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

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl Display for EvidencePolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for EvidencePolicyError {}

fn validate_identifier(field: &'static str, value: String) -> Result<String, EvidencePolicyError> {
    if value.is_empty() {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} must not be empty"),
        ));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} exceeds {MAX_IDENTIFIER_BYTES} bytes"),
        ));
    }
    if value.trim() != value {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} must not contain leading or trailing whitespace"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(EvidencePolicyError::new(
            "HOSTED_EVIDENCE_IDENTIFIER_INVALID",
            format!("{field} must not contain control characters"),
        ));
    }
    Ok(value)
}

macro_rules! evidence_identifier {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, EvidencePolicyError> {
                validate_identifier($field, value.into()).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

evidence_identifier!(EvidenceIssuer, "issuer");
evidence_identifier!(EvidenceSource, "source");
evidence_identifier!(EvidenceTarget, "target");
evidence_identifier!(EvidenceEnvironment, "environment");
evidence_identifier!(EvidenceSubject, "subject");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceTrustState {
    Trusted,
    Untrusted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceOutcome {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBindingV1 {
    pub issuer: EvidenceIssuer,
    pub source: EvidenceSource,
    pub target: EvidenceTarget,
    pub environment: EvidenceEnvironment,
    pub subject: EvidenceSubject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceObservationV1 {
    pub binding: EvidenceBindingV1,
    pub source_run_id: u64,
    pub source_run_attempt: u32,
    pub observed_at_unix_seconds: i64,
    pub valid_until_unix_seconds: i64,
    pub trust_state: EvidenceTrustState,
    pub outcome: EvidenceOutcome,
    pub production_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceEnvelopeV1 {
    pub binding: EvidenceBindingV1,
    pub source_run_id: u64,
    pub source_run_attempt: u32,
    pub observed_at_unix_seconds: i64,
    pub valid_until_unix_seconds: i64,
    pub trust_state: EvidenceTrustState,
    pub outcome: EvidenceOutcome,
    pub production_mutation: bool,
}

impl HostedEvidenceEnvelopeV1 {
    #[must_use]
    pub fn as_observation(&self) -> HostedEvidenceObservationV1 {
        HostedEvidenceObservationV1 {
            binding: self.binding.clone(),
            source_run_id: self.source_run_id,
            source_run_attempt: self.source_run_attempt,
            observed_at_unix_seconds: self.observed_at_unix_seconds,
            valid_until_unix_seconds: self.valid_until_unix_seconds,
            trust_state: self.trust_state,
            outcome: self.outcome,
            production_mutation: self.production_mutation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePolicyV1 {
    expected_binding: EvidenceBindingV1,
    max_validity_seconds: i64,
}

impl EvidencePolicyV1 {
    pub fn new(
        expected_binding: EvidenceBindingV1,
        max_validity_seconds: u64,
    ) -> Result<Self, EvidencePolicyError> {
        let max_validity_seconds = i64::try_from(max_validity_seconds).map_err(|_| {
            EvidencePolicyError::new(
                "HOSTED_EVIDENCE_POLICY_INVALID",
                "max validity does not fit the supported timestamp domain",
            )
        })?;
        if max_validity_seconds <= 0 {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_POLICY_INVALID",
                "max validity must be greater than zero",
            ));
        }
        Ok(Self {
            expected_binding,
            max_validity_seconds,
        })
    }

    pub fn evaluate(
        &self,
        observation: HostedEvidenceObservationV1,
        evaluated_at_unix_seconds: i64,
    ) -> Result<HostedEvidenceEnvelopeV1, EvidencePolicyError> {
        if observation.binding != self.expected_binding {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_BINDING_MISMATCH",
                "issuer/source/target/environment/subject binding does not match the expected consumer",
            ));
        }
        if observation.production_mutation {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_PRODUCTION_MUTATION_FORBIDDEN",
                "PF-2 hosted evidence must prove production_mutation=false",
            ));
        }
        if observation.source_run_id == 0 || observation.source_run_attempt == 0 {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_RUN_IDENTITY_INVALID",
                "source run id and attempt must both be greater than zero",
            ));
        }
        if observation.observed_at_unix_seconds <= 0
            || observation.valid_until_unix_seconds <= 0
            || evaluated_at_unix_seconds <= 0
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_TIMESTAMP_INVALID",
                "observation, validity and evaluation timestamps must be positive Unix seconds",
            ));
        }
        let validity_seconds = observation
            .valid_until_unix_seconds
            .checked_sub(observation.observed_at_unix_seconds)
            .ok_or_else(|| {
                EvidencePolicyError::new(
                    "HOSTED_EVIDENCE_FRESHNESS_WINDOW_INVALID",
                    "validity window arithmetic overflowed",
                )
            })?;
        if validity_seconds <= 0 {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_FRESHNESS_WINDOW_INVALID",
                "valid_until must be strictly later than observed_at",
            ));
        }
        if validity_seconds > self.max_validity_seconds {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_FRESHNESS_WINDOW_TOO_LARGE",
                format!(
                    "validity window {validity_seconds}s exceeds policy maximum {}s",
                    self.max_validity_seconds
                ),
            ));
        }
        if evaluated_at_unix_seconds < observation.observed_at_unix_seconds {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_OBSERVATION_FROM_FUTURE",
                "evaluation time precedes the observation time",
            ));
        }
        if evaluated_at_unix_seconds >= observation.valid_until_unix_seconds {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_EXPIRED_OR_REPLAYED",
                "evidence is expired at the explicit evaluation time",
            ));
        }
        if observation.trust_state != EvidenceTrustState::Trusted {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_UNTRUSTED",
                "only explicitly trusted hosted observations may become durable evidence",
            ));
        }
        if observation.outcome != EvidenceOutcome::Passed {
            return Err(EvidencePolicyError::new(
                "HOSTED_EVIDENCE_OUTCOME_REJECTED",
                "only a passed hosted observation may become accepted evidence",
            ));
        }

        Ok(HostedEvidenceEnvelopeV1 {
            binding: observation.binding,
            source_run_id: observation.source_run_id,
            source_run_attempt: observation.source_run_attempt,
            observed_at_unix_seconds: observation.observed_at_unix_seconds,
            valid_until_unix_seconds: observation.valid_until_unix_seconds,
            trust_state: observation.trust_state,
            outcome: observation.outcome,
            production_mutation: observation.production_mutation,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewAttestationStatus {
    Passed,
    Failed,
}

impl ReviewAttestationStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttestationObservationV1 {
    pub expected_repository: EvidenceTarget,
    pub observed_repository: EvidenceTarget,
    pub evidence_id: EvidenceSubject,
    pub gate: EvidenceSource,
    pub status: ReviewAttestationStatus,
    pub expected_reference: String,
    pub observed_reference: String,
    pub expected_reviewer: String,
    pub observed_reviewer: Option<String>,
    pub expected_reviewed_at: String,
    pub observed_reviewed_at: Option<String>,
    pub claim_sha256: String,
    pub observed_body: Option<String>,
    pub provider_object_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReviewAttestationPolicyV1;

impl ReviewAttestationPolicyV1 {
    pub fn evaluate(
        self,
        observation: &ReviewAttestationObservationV1,
    ) -> Result<(), EvidencePolicyError> {
        if !observation.provider_object_available {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_PROVIDER_OBJECT_UNAVAILABLE",
                "the exact referenced GitHub review/comment object is unavailable",
            ));
        }
        if !observation
            .expected_repository
            .as_str()
            .eq_ignore_ascii_case(observation.observed_repository.as_str())
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REPOSITORY_MISMATCH",
                "the observed review object belongs to a different repository",
            ));
        }
        if observation.expected_reference != observation.observed_reference {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REFERENCE_MISMATCH",
                "the observed provider object does not match the record review reference",
            ));
        }
        let observed_reviewer = observation.observed_reviewer.as_deref().ok_or_else(|| {
            EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REVIEWER_MISSING",
                "the observed provider object has no reviewer identity",
            )
        })?;
        if !observation
            .expected_reviewer
            .eq_ignore_ascii_case(observed_reviewer)
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_REVIEWER_MISMATCH",
                "the observed reviewer does not match the terminal evidence record",
            ));
        }
        let observed_reviewed_at = observation
            .observed_reviewed_at
            .as_deref()
            .ok_or_else(|| {
                EvidencePolicyError::new(
                    "HOSTED_REVIEW_ATTESTATION_TIMESTAMP_MISSING",
                    "the observed provider object has no effective review timestamp",
                )
            })?;
        if observation.expected_reviewed_at != observed_reviewed_at {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_TIMESTAMP_MISMATCH",
                "the observed review timestamp does not match the terminal evidence record",
            ));
        }
        if observation.claim_sha256.len() != 64
            || !observation
                .claim_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_CLAIM_DIGEST_INVALID",
                "claim_sha256 must be exactly 64 lowercase hexadecimal characters",
            ));
        }
        let observed_body = observation.observed_body.as_deref().ok_or_else(|| {
            EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_BODY_MISSING",
                "the observed provider object has no review body",
            )
        })?;
        let expected_body = format!(
            "{REVIEW_CLAIM_DOMAIN}\nevidence_id={}\ngate={}\nstatus={}\nclaim_sha256={}",
            observation.evidence_id.as_str(),
            observation.gate.as_str(),
            observation.status.as_str(),
            observation.claim_sha256,
        );
        if observed_body != expected_body {
            return Err(EvidencePolicyError::new(
                "HOSTED_REVIEW_ATTESTATION_BODY_MISMATCH",
                "the observed GitHub review body does not match the canonical terminal claim",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome, EvidencePolicyV1,
        EvidenceSource, EvidenceSubject, EvidenceTarget, EvidenceTrustState,
        HostedEvidenceObservationV1, ReviewAttestationObservationV1, ReviewAttestationPolicyV1,
        ReviewAttestationStatus,
    };

    fn binding(subject: &str) -> Result<EvidenceBindingV1, Box<dyn std::error::Error>> {
        Ok(EvidenceBindingV1 {
            issuer: EvidenceIssuer::new("github-actions")?,
            source: EvidenceSource::new("operational-credential-hosted-state")?,
            target: EvidenceTarget::new("iamaman11/part-crm-emai-profile")?,
            environment: EvidenceEnvironment::new("staging")?,
            subject: EvidenceSubject::new(subject)?,
        })
    }

    fn observation() -> Result<HostedEvidenceObservationV1, Box<dyn std::error::Error>> {
        Ok(HostedEvidenceObservationV1 {
            binding: binding("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")?,
            source_run_id: 42,
            source_run_attempt: 1,
            observed_at_unix_seconds: 1_700_000_000,
            valid_until_unix_seconds: 1_700_003_600,
            trust_state: EvidenceTrustState::Trusted,
            outcome: EvidenceOutcome::Passed,
            production_mutation: false,
        })
    }

    fn policy() -> Result<EvidencePolicyV1, Box<dyn std::error::Error>> {
        Ok(EvidencePolicyV1::new(
            binding("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")?,
            3_600,
        )?)
    }

    fn review_observation()
    -> Result<ReviewAttestationObservationV1, Box<dyn std::error::Error>> {
        let digest = "11".repeat(32);
        Ok(ReviewAttestationObservationV1 {
            expected_repository: EvidenceTarget::new("acme/profile-platform")?,
            observed_repository: EvidenceTarget::new("acme/profile-platform")?,
            evidence_id: EvidenceSubject::new("ev-20260806-terminal")?,
            gate: EvidenceSource::new("product_license")?,
            status: ReviewAttestationStatus::Passed,
            expected_reference:
                "https://github.com/acme/profile-platform/issues/9#issuecomment-101".to_owned(),
            observed_reference:
                "https://github.com/acme/profile-platform/issues/9#issuecomment-101".to_owned(),
            expected_reviewer: "reviewer-one".to_owned(),
            observed_reviewer: Some("Reviewer-One".to_owned()),
            expected_reviewed_at: "2026-08-06T14:40:00Z".to_owned(),
            observed_reviewed_at: Some("2026-08-06T14:40:00Z".to_owned()),
            claim_sha256: digest.clone(),
            observed_body: Some(format!(
                "external-evidence-review-v1\nevidence_id=ev-20260806-terminal\ngate=product_license\nstatus=passed\nclaim_sha256={digest}"
            )),
            provider_object_available: true,
        })
    }

    #[test]
    fn accepts_fresh_trusted_exact_binding() -> Result<(), Box<dyn std::error::Error>> {
        let observation = observation()?;
        let envelope = policy()?.evaluate(observation.clone(), 1_700_000_010)?;
        assert_eq!(envelope.as_observation(), observation);
        Ok(())
    }

    #[test]
    fn rejects_source_or_target_binding_drift() -> Result<(), Box<dyn std::error::Error>> {
        let mut source_drift = observation()?;
        source_drift.binding.source = EvidenceSource::new("other-source")?;
        assert_eq!(
            policy()?
                .evaluate(source_drift, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_BINDING_MISMATCH")
        );

        let mut target_drift = observation()?;
        target_drift.binding.target = EvidenceTarget::new("other/repository")?;
        assert_eq!(
            policy()?
                .evaluate(target_drift, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_BINDING_MISMATCH")
        );
        Ok(())
    }

    #[test]
    fn rejects_production_mutation() -> Result<(), Box<dyn std::error::Error>> {
        let mut mutated = observation()?;
        mutated.production_mutation = true;
        assert_eq!(
            policy()?
                .evaluate(mutated, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_PRODUCTION_MUTATION_FORBIDDEN")
        );
        Ok(())
    }

    #[test]
    fn rejects_untrusted_failed_and_unknown_observations() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut untrusted = observation()?;
        untrusted.trust_state = EvidenceTrustState::Untrusted;
        assert_eq!(
            policy()?
                .evaluate(untrusted, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_UNTRUSTED")
        );

        let mut unknown = observation()?;
        unknown.trust_state = EvidenceTrustState::Unknown;
        assert_eq!(
            policy()?
                .evaluate(unknown, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_UNTRUSTED")
        );

        let mut failed = observation()?;
        failed.outcome = EvidenceOutcome::Failed;
        assert_eq!(
            policy()?
                .evaluate(failed, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_OUTCOME_REJECTED")
        );
        Ok(())
    }

    #[test]
    fn rejects_impossible_oversized_future_and_replayed_freshness()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut impossible = observation()?;
        impossible.valid_until_unix_seconds = impossible.observed_at_unix_seconds;
        assert_eq!(
            policy()?
                .evaluate(impossible, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_FRESHNESS_WINDOW_INVALID")
        );

        let mut oversized = observation()?;
        oversized.valid_until_unix_seconds = oversized.observed_at_unix_seconds + 3_601;
        assert_eq!(
            policy()?
                .evaluate(oversized, 1_700_000_010)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_FRESHNESS_WINDOW_TOO_LARGE")
        );

        assert_eq!(
            policy()?
                .evaluate(observation()?, 1_699_999_999)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_OBSERVATION_FROM_FUTURE")
        );
        assert_eq!(
            policy()?
                .evaluate(observation()?, 1_700_003_600)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_EVIDENCE_EXPIRED_OR_REPLAYED")
        );
        Ok(())
    }

    #[test]
    fn identifiers_reject_ambiguous_whitespace() {
        assert!(EvidenceIssuer::new(" github-actions").is_err());
        assert!(EvidenceSource::new("").is_err());
    }

    #[test]
    fn review_attestation_accepts_exact_provider_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        ReviewAttestationPolicyV1.evaluate(&review_observation()?)?;
        Ok(())
    }

    #[test]
    fn review_attestation_rejects_provider_binding_drift()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut foreign_repository = review_observation()?;
        foreign_repository.observed_repository = EvidenceTarget::new("other/repository")?;
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&foreign_repository)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_REPOSITORY_MISMATCH")
        );

        let mut wrong_reference = review_observation()?;
        wrong_reference.observed_reference =
            "https://github.com/acme/profile-platform/issues/9#issuecomment-999".to_owned();
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&wrong_reference)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_REFERENCE_MISMATCH")
        );
        Ok(())
    }

    #[test]
    fn review_attestation_rejects_unavailable_or_mutated_provider_object()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut unavailable = review_observation()?;
        unavailable.provider_object_available = false;
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&unavailable)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_PROVIDER_OBJECT_UNAVAILABLE")
        );

        let mut wrong_reviewer = review_observation()?;
        wrong_reviewer.observed_reviewer = Some("another-reviewer".to_owned());
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&wrong_reviewer)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_REVIEWER_MISMATCH")
        );

        let mut edited_timestamp = review_observation()?;
        edited_timestamp.observed_reviewed_at = Some("2026-08-06T14:41:00Z".to_owned());
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&edited_timestamp)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_TIMESTAMP_MISMATCH")
        );

        let mut wrong_body = review_observation()?;
        wrong_body.observed_body = Some("external-evidence-review-v1\nwrong=true".to_owned());
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&wrong_body)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_BODY_MISMATCH")
        );
        Ok(())
    }

    #[test]
    fn review_attestation_rejects_noncanonical_claim_digest()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut invalid = review_observation()?;
        invalid.claim_sha256 = "AA".repeat(32);
        assert_eq!(
            ReviewAttestationPolicyV1
                .evaluate(&invalid)
                .err()
                .map(|error| error.code()),
            Some("HOSTED_REVIEW_ATTESTATION_CLAIM_DIGEST_INVALID")
        );
        Ok(())
    }
}
