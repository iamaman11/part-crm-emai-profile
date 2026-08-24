use std::fmt::{Display, Formatter};

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_REVIEW_BODY_BYTES: usize = 1_024;
const MAX_REVIEW_ITEMS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttestationPolicyError {
    code: &'static str,
    detail: String,
}

impl ReviewAttestationPolicyError {
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

impl Display for ReviewAttestationPolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for ReviewAttestationPolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewKind {
    IssueComment,
    PullRequestReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewClaimStatus {
    Passed,
    Failed,
}

impl ReviewClaimStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredReviewClaimV1 {
    pub evidence_id: String,
    pub gate: String,
    pub status: ReviewClaimStatus,
    pub subject: String,
    pub claim_sha256: String,
    pub reviewer: String,
    pub reviewed_at_unix_seconds: i64,
    pub execution_window_start_unix_seconds: i64,
    pub review_kind: ReviewKind,
    pub review_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderReviewFactV1 {
    pub review_kind: ReviewKind,
    pub review_reference: String,
    pub author: String,
    pub body: String,
    pub observed_at_unix_seconds: i64,
    pub pull_request_state: Option<PullRequestReviewState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttestationObservationV1 {
    pub repository: String,
    pub subject: String,
    pub provider_repository: String,
    pub provider_subject: String,
    pub observed_at_unix_seconds: i64,
    pub required_claims: Vec<RequiredReviewClaimV1>,
    pub provider_reviews: Vec<ProviderReviewFactV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedReviewV1 {
    pub evidence_id: String,
    pub gate: String,
    pub status: ReviewClaimStatus,
    pub subject: String,
    pub claim_sha256: String,
    pub reviewer: String,
    pub reviewed_at_unix_seconds: i64,
    pub review_kind: ReviewKind,
    pub review_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttestationDecisionV1 {
    pub accepted_reviews: Vec<AcceptedReviewV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttestationPolicyV1 {
    expected_repository: String,
    expected_subject: String,
}

impl ReviewAttestationPolicyV1 {
    pub fn new(
        expected_repository: impl Into<String>,
        expected_subject: impl Into<String>,
    ) -> Result<Self, ReviewAttestationPolicyError> {
        let expected_repository = expected_repository.into();
        let expected_subject = expected_subject.into();
        validate_identifier("expected_repository", &expected_repository)?;
        validate_commit_subject("expected_subject", &expected_subject)?;
        Ok(Self {
            expected_repository,
            expected_subject,
        })
    }

    pub fn evaluate(
        &self,
        observation: &ReviewAttestationObservationV1,
    ) -> Result<ReviewAttestationDecisionV1, ReviewAttestationPolicyError> {
        validate_identifier("repository", &observation.repository)?;
        validate_commit_subject("subject", &observation.subject)?;
        validate_identifier("provider_repository", &observation.provider_repository)?;
        validate_commit_subject("provider_subject", &observation.provider_subject)?;

        if observation.repository != self.expected_repository
            || observation.provider_repository != self.expected_repository
        {
            return Err(ReviewAttestationPolicyError::new(
                "REVIEW_ATTESTATION_REPOSITORY_MISMATCH",
                "declared and provider-observed repositories must equal the expected repository",
            ));
        }
        if observation.subject != self.expected_subject
            || observation.provider_subject != self.expected_subject
        {
            return Err(ReviewAttestationPolicyError::new(
                "REVIEW_ATTESTATION_SUBJECT_MISMATCH",
                "declared and provider-observed subjects must equal the expected source commit",
            ));
        }
        if observation.observed_at_unix_seconds <= 0 {
            return Err(ReviewAttestationPolicyError::new(
                "REVIEW_ATTESTATION_TIMESTAMP_INVALID",
                "observation timestamp must be positive Unix seconds",
            ));
        }
        if observation.required_claims.len() > MAX_REVIEW_ITEMS
            || observation.provider_reviews.len() > MAX_REVIEW_ITEMS
        {
            return Err(ReviewAttestationPolicyError::new(
                "REVIEW_ATTESTATION_CARDINALITY_EXCEEDED",
                format!("review observations are bounded to {MAX_REVIEW_ITEMS} items"),
            ));
        }

        for (index, claim) in observation.required_claims.iter().enumerate() {
            validate_claim(claim, observation.observed_at_unix_seconds)?;
            if claim.subject != self.expected_subject {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_CLAIM_SUBJECT_MISMATCH",
                    format!("required claim at index {index} is bound to the wrong subject"),
                ));
            }
            if observation.required_claims[..index]
                .iter()
                .any(|prior| prior.evidence_id == claim.evidence_id)
            {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_CLAIM_AMBIGUOUS",
                    format!("duplicate evidence_id {}", claim.evidence_id),
                ));
            }
        }

        for (index, provider) in observation.provider_reviews.iter().enumerate() {
            validate_provider_review(provider, observation.observed_at_unix_seconds)?;
            if observation.provider_reviews[..index].iter().any(|prior| {
                prior.review_kind == provider.review_kind
                    && prior.review_reference == provider.review_reference
            }) {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_PROVIDER_REVIEW_AMBIGUOUS",
                    format!(
                        "duplicate provider review reference {}",
                        provider.review_reference
                    ),
                ));
            }
            if !observation.required_claims.iter().any(|claim| {
                claim.review_kind == provider.review_kind
                    && claim.review_reference == provider.review_reference
            }) {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_PROVIDER_REVIEW_UNBOUND",
                    format!(
                        "provider review {} has no required claim",
                        provider.review_reference
                    ),
                ));
            }
        }

        let mut accepted_reviews = Vec::with_capacity(observation.required_claims.len());
        for claim in &observation.required_claims {
            let matching: Vec<&ProviderReviewFactV1> = observation
                .provider_reviews
                .iter()
                .filter(|provider| {
                    provider.review_kind == claim.review_kind
                        && provider.review_reference == claim.review_reference
                })
                .collect();
            if matching.is_empty() {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_MISSING_REVIEW",
                    format!("required review {} is missing", claim.review_reference),
                ));
            }
            if matching.len() != 1 {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_PROVIDER_REVIEW_AMBIGUOUS",
                    format!("required review {} is ambiguous", claim.review_reference),
                ));
            }
            let provider = matching[0];
            if provider.author != claim.reviewer {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_REVIEWER_MISMATCH",
                    format!("review {} was authored by an unexpected reviewer", claim.review_reference),
                ));
            }
            if provider.observed_at_unix_seconds != claim.reviewed_at_unix_seconds {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_REVIEW_TIME_MISMATCH",
                    format!("review {} timestamp does not match the attested timestamp", claim.review_reference),
                ));
            }
            let expected_body = expected_claim_body(claim);
            if provider.body != expected_body {
                return Err(ReviewAttestationPolicyError::new(
                    "REVIEW_ATTESTATION_CLAIM_MISMATCH",
                    format!("review {} does not contain the exact bound claim", claim.review_reference),
                ));
            }
            match claim.review_kind {
                ReviewKind::IssueComment => {
                    if provider.pull_request_state.is_some() {
                        return Err(ReviewAttestationPolicyError::new(
                            "REVIEW_ATTESTATION_REVIEW_STATE_INVALID",
                            "issue comment observations must not carry pull-request review state",
                        ));
                    }
                }
                ReviewKind::PullRequestReview => {
                    if provider.pull_request_state != Some(PullRequestReviewState::Approved) {
                        return Err(ReviewAttestationPolicyError::new(
                            "REVIEW_ATTESTATION_REVIEW_STATE_REJECTED",
                            "pull-request review attestation must be in APPROVED state",
                        ));
                    }
                }
            }
            accepted_reviews.push(AcceptedReviewV1 {
                evidence_id: claim.evidence_id.clone(),
                gate: claim.gate.clone(),
                status: claim.status,
                subject: claim.subject.clone(),
                claim_sha256: claim.claim_sha256.clone(),
                reviewer: claim.reviewer.clone(),
                reviewed_at_unix_seconds: claim.reviewed_at_unix_seconds,
                review_kind: claim.review_kind,
                review_reference: claim.review_reference.clone(),
            });
        }

        Ok(ReviewAttestationDecisionV1 { accepted_reviews })
    }
}

fn validate_claim(
    claim: &RequiredReviewClaimV1,
    observation_time: i64,
) -> Result<(), ReviewAttestationPolicyError> {
    validate_identifier("evidence_id", &claim.evidence_id)?;
    validate_identifier("gate", &claim.gate)?;
    validate_commit_subject("claim.subject", &claim.subject)?;
    if !is_lower_hex(&claim.claim_sha256, 64) {
        return Err(ReviewAttestationPolicyError::new(
            "REVIEW_ATTESTATION_CLAIM_DIGEST_INVALID",
            "claim_sha256 must be exactly 64 lowercase hexadecimal characters",
        ));
    }
    validate_identifier("reviewer", &claim.reviewer)?;
    validate_identifier("review_reference", &claim.review_reference)?;
    if claim.reviewed_at_unix_seconds <= 0
        || claim.execution_window_start_unix_seconds <= 0
        || claim.reviewed_at_unix_seconds < claim.execution_window_start_unix_seconds
        || claim.reviewed_at_unix_seconds > observation_time
    {
        return Err(ReviewAttestationPolicyError::new(
            "REVIEW_ATTESTATION_REVIEW_TIME_INVALID",
            "review time must be inside the observed execution window and not in the future",
        ));
    }
    Ok(())
}

fn validate_provider_review(
    provider: &ProviderReviewFactV1,
    observation_time: i64,
) -> Result<(), ReviewAttestationPolicyError> {
    validate_identifier("provider.review_reference", &provider.review_reference)?;
    validate_identifier("provider.author", &provider.author)?;
    if provider.body.len() > MAX_REVIEW_BODY_BYTES
        || provider.body.chars().any(char::is_control)
        || contains_secret_shape(&provider.body)
    {
        return Err(ReviewAttestationPolicyError::new(
            "REVIEW_ATTESTATION_PROVIDER_BODY_INVALID",
            "provider review body is unbounded, contains control characters, or resembles secret material",
        ));
    }
    if provider.observed_at_unix_seconds <= 0 || provider.observed_at_unix_seconds > observation_time {
        return Err(ReviewAttestationPolicyError::new(
            "REVIEW_ATTESTATION_PROVIDER_TIME_INVALID",
            "provider review timestamp must be positive and no later than the observation time",
        ));
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), ReviewAttestationPolicyError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ReviewAttestationPolicyError::new(
            "REVIEW_ATTESTATION_IDENTIFIER_INVALID",
            format!("{field} is empty, overlong, padded, or contains control characters"),
        ));
    }
    Ok(())
}

fn validate_commit_subject(
    field: &'static str,
    value: &str,
) -> Result<(), ReviewAttestationPolicyError> {
    if !is_lower_hex(value, 40) {
        return Err(ReviewAttestationPolicyError::new(
            "REVIEW_ATTESTATION_SUBJECT_INVALID",
            format!("{field} must be exactly 40 lowercase hexadecimal characters"),
        ));
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn contains_secret_shape(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "authorization:",
        "bearer ",
        "github_pat_",
        "ghp_",
        "api_token",
        "client_secret",
        "secret_access_key",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[must_use]
pub fn expected_claim_body(claim: &RequiredReviewClaimV1) -> String {
    format!(
        "external-evidence-review:v1 evidence_id={} gate={} status={} subject={} claim_sha256={}",
        claim.evidence_id,
        claim.gate,
        claim.status.as_str(),
        claim.subject,
        claim.claim_sha256
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderReviewFactV1, PullRequestReviewState, RequiredReviewClaimV1,
        ReviewAttestationObservationV1, ReviewAttestationPolicyV1, ReviewClaimStatus, ReviewKind,
        expected_claim_body,
    };

    const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn policy() -> ReviewAttestationPolicyV1 {
        ReviewAttestationPolicyV1::new(REPOSITORY, SUBJECT).expect("valid policy")
    }

    fn claim(kind: ReviewKind) -> RequiredReviewClaimV1 {
        RequiredReviewClaimV1 {
            evidence_id: "security-review".to_owned(),
            gate: "independent_security_review".to_owned(),
            status: ReviewClaimStatus::Passed,
            subject: SUBJECT.to_owned(),
            claim_sha256: DIGEST.to_owned(),
            reviewer: "trusted-reviewer".to_owned(),
            reviewed_at_unix_seconds: 1_700_000_010,
            execution_window_start_unix_seconds: 1_700_000_000,
            review_kind: kind,
            review_reference: "42".to_owned(),
        }
    }

    fn observation(kind: ReviewKind) -> ReviewAttestationObservationV1 {
        let claim = claim(kind);
        let body = expected_claim_body(&claim);
        ReviewAttestationObservationV1 {
            repository: REPOSITORY.to_owned(),
            subject: SUBJECT.to_owned(),
            provider_repository: REPOSITORY.to_owned(),
            provider_subject: SUBJECT.to_owned(),
            observed_at_unix_seconds: 1_700_000_020,
            required_claims: vec![claim],
            provider_reviews: vec![ProviderReviewFactV1 {
                review_kind: kind,
                review_reference: "42".to_owned(),
                author: "trusted-reviewer".to_owned(),
                body,
                observed_at_unix_seconds: 1_700_000_010,
                pull_request_state: match kind {
                    ReviewKind::IssueComment => None,
                    ReviewKind::PullRequestReview => Some(PullRequestReviewState::Approved),
                },
            }],
        }
    }

    #[test]
    fn accepts_zero_current_review_obligations_when_provider_binding_is_exact() {
        let observation = ReviewAttestationObservationV1 {
            repository: REPOSITORY.to_owned(),
            subject: SUBJECT.to_owned(),
            provider_repository: REPOSITORY.to_owned(),
            provider_subject: SUBJECT.to_owned(),
            observed_at_unix_seconds: 1_700_000_020,
            required_claims: Vec::new(),
            provider_reviews: Vec::new(),
        };
        assert!(policy().evaluate(&observation).is_ok());
    }

    #[test]
    fn accepts_exact_issue_comment_attestation() {
        let decision = policy()
            .evaluate(&observation(ReviewKind::IssueComment))
            .expect("exact review accepted");
        assert_eq!(decision.accepted_reviews.len(), 1);
    }

    #[test]
    fn accepts_exact_approved_pull_request_review() {
        assert!(
            policy()
                .evaluate(&observation(ReviewKind::PullRequestReview))
                .is_ok()
        );
    }

    #[test]
    fn rejects_wrong_repository_or_subject() {
        let mut wrong_repository = observation(ReviewKind::IssueComment);
        wrong_repository.provider_repository = "other/repository".to_owned();
        assert_eq!(
            policy()
                .evaluate(&wrong_repository)
                .expect_err("wrong repository rejected")
                .code(),
            "REVIEW_ATTESTATION_REPOSITORY_MISMATCH"
        );

        let mut wrong_subject = observation(ReviewKind::IssueComment);
        wrong_subject.provider_subject = "cccccccccccccccccccccccccccccccccccccccc".to_owned();
        assert_eq!(
            policy()
                .evaluate(&wrong_subject)
                .expect_err("wrong subject rejected")
                .code(),
            "REVIEW_ATTESTATION_SUBJECT_MISMATCH"
        );
    }

    #[test]
    fn rejects_missing_or_ambiguous_review() {
        let mut missing = observation(ReviewKind::IssueComment);
        missing.provider_reviews.clear();
        assert_eq!(
            policy()
                .evaluate(&missing)
                .expect_err("missing review rejected")
                .code(),
            "REVIEW_ATTESTATION_MISSING_REVIEW"
        );

        let mut ambiguous = observation(ReviewKind::IssueComment);
        ambiguous.provider_reviews.push(ambiguous.provider_reviews[0].clone());
        assert_eq!(
            policy()
                .evaluate(&ambiguous)
                .expect_err("ambiguous review rejected")
                .code(),
            "REVIEW_ATTESTATION_PROVIDER_REVIEW_AMBIGUOUS"
        );
    }

    #[test]
    fn rejects_reviewer_claim_and_time_drift() {
        let mut wrong_reviewer = observation(ReviewKind::IssueComment);
        wrong_reviewer.provider_reviews[0].author = "other-reviewer".to_owned();
        assert_eq!(
            policy()
                .evaluate(&wrong_reviewer)
                .expect_err("reviewer mismatch rejected")
                .code(),
            "REVIEW_ATTESTATION_REVIEWER_MISMATCH"
        );

        let mut wrong_claim = observation(ReviewKind::IssueComment);
        wrong_claim.provider_reviews[0].body.push('x');
        assert_eq!(
            policy()
                .evaluate(&wrong_claim)
                .expect_err("claim mismatch rejected")
                .code(),
            "REVIEW_ATTESTATION_CLAIM_MISMATCH"
        );

        let mut future = observation(ReviewKind::IssueComment);
        future.provider_reviews[0].observed_at_unix_seconds = 1_700_000_030;
        assert_eq!(
            policy()
                .evaluate(&future)
                .expect_err("future provider review rejected")
                .code(),
            "REVIEW_ATTESTATION_PROVIDER_TIME_INVALID"
        );
    }

    #[test]
    fn rejects_non_approved_pull_request_review() {
        let mut rejected = observation(ReviewKind::PullRequestReview);
        rejected.provider_reviews[0].pull_request_state =
            Some(PullRequestReviewState::ChangesRequested);
        assert_eq!(
            policy()
                .evaluate(&rejected)
                .expect_err("non-approved review rejected")
                .code(),
            "REVIEW_ATTESTATION_REVIEW_STATE_REJECTED"
        );
    }

    #[test]
    fn rejects_secret_shaped_provider_body() {
        let mut secret = observation(ReviewKind::IssueComment);
        secret.provider_reviews[0].body = "Authorization: Bearer secret".to_owned();
        assert_eq!(
            policy()
                .evaluate(&secret)
                .expect_err("secret shaped body rejected")
                .code(),
            "REVIEW_ATTESTATION_PROVIDER_BODY_INVALID"
        );
    }
}
