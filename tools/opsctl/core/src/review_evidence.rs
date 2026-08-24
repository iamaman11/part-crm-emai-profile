use std::fmt::{Display, Formatter};

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_REVIEW_BODY_BYTES: usize = 1_024;
const MAX_ITEMS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPolicyError {
    code: &'static str,
    detail: String,
}

impl ReviewPolicyError {
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

impl Display for ReviewPolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for ReviewPolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewKind {
    IssueComment,
    PullRequestReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    Passed,
    Failed,
}

impl ReviewStatus {
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
    pub status: ReviewStatus,
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
pub struct ReviewObservationV1 {
    pub repository: String,
    pub subject: String,
    pub provider_repository: String,
    pub provider_subject: String,
    pub observed_at_unix_seconds: i64,
    pub required_claims: Vec<RequiredReviewClaimV1>,
    pub provider_reviews: Vec<ProviderReviewFactV1>,
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
    ) -> Result<Self, ReviewPolicyError> {
        let expected_repository = expected_repository.into();
        let expected_subject = expected_subject.into();
        validate_identifier("expected_repository", &expected_repository)?;
        validate_subject("expected_subject", &expected_subject)?;
        Ok(Self {
            expected_repository,
            expected_subject,
        })
    }

    pub fn evaluate(&self, observation: &ReviewObservationV1) -> Result<(), ReviewPolicyError> {
        validate_identifier("repository", &observation.repository)?;
        validate_identifier("provider_repository", &observation.provider_repository)?;
        validate_subject("subject", &observation.subject)?;
        validate_subject("provider_subject", &observation.provider_subject)?;

        if observation.repository != self.expected_repository
            || observation.provider_repository != self.expected_repository
        {
            return Err(ReviewPolicyError::new(
                "REVIEW_REPOSITORY_MISMATCH",
                "declared and provider-observed repositories must match the expected repository",
            ));
        }
        if observation.subject != self.expected_subject
            || observation.provider_subject != self.expected_subject
        {
            return Err(ReviewPolicyError::new(
                "REVIEW_SUBJECT_MISMATCH",
                "declared and provider-observed subjects must match the expected source SHA",
            ));
        }
        if observation.observed_at_unix_seconds <= 0 {
            return Err(ReviewPolicyError::new(
                "REVIEW_TIMESTAMP_INVALID",
                "observation timestamp must be positive Unix seconds",
            ));
        }
        if observation.required_claims.len() > MAX_ITEMS
            || observation.provider_reviews.len() > MAX_ITEMS
        {
            return Err(ReviewPolicyError::new(
                "REVIEW_CARDINALITY_EXCEEDED",
                format!("review observations are bounded to {MAX_ITEMS} items"),
            ));
        }

        for (index, claim) in observation.required_claims.iter().enumerate() {
            validate_claim(claim, observation.observed_at_unix_seconds)?;
            if claim.subject != self.expected_subject {
                return Err(ReviewPolicyError::new(
                    "REVIEW_CLAIM_SUBJECT_MISMATCH",
                    format!("claim {} is bound to the wrong subject", claim.evidence_id),
                ));
            }
            if observation.required_claims[..index]
                .iter()
                .any(|prior| prior.evidence_id == claim.evidence_id)
            {
                return Err(ReviewPolicyError::new(
                    "REVIEW_CLAIM_AMBIGUOUS",
                    format!("duplicate evidence_id {}", claim.evidence_id),
                ));
            }
        }

        for (index, review) in observation.provider_reviews.iter().enumerate() {
            validate_provider_review(review, observation.observed_at_unix_seconds)?;
            if observation.provider_reviews[..index].iter().any(|prior| {
                prior.review_kind == review.review_kind
                    && prior.review_reference == review.review_reference
            }) {
                return Err(ReviewPolicyError::new(
                    "REVIEW_PROVIDER_AMBIGUOUS",
                    format!("duplicate provider review {}", review.review_reference),
                ));
            }
        }

        for claim in &observation.required_claims {
            let mut matches = observation.provider_reviews.iter().filter(|review| {
                review.review_kind == claim.review_kind
                    && review.review_reference == claim.review_reference
            });
            let review = matches.next().ok_or_else(|| {
                ReviewPolicyError::new(
                    "REVIEW_MISSING",
                    format!("required review {} is missing", claim.review_reference),
                )
            })?;
            if matches.next().is_some() {
                return Err(ReviewPolicyError::new(
                    "REVIEW_PROVIDER_AMBIGUOUS",
                    format!("required review {} is ambiguous", claim.review_reference),
                ));
            }
            if review.author != claim.reviewer {
                return Err(ReviewPolicyError::new(
                    "REVIEW_REVIEWER_MISMATCH",
                    format!("review {} has the wrong reviewer", claim.review_reference),
                ));
            }
            if review.observed_at_unix_seconds != claim.reviewed_at_unix_seconds {
                return Err(ReviewPolicyError::new(
                    "REVIEW_TIME_MISMATCH",
                    format!("review {} has the wrong timestamp", claim.review_reference),
                ));
            }
            if review.body != expected_claim_body(claim) {
                return Err(ReviewPolicyError::new(
                    "REVIEW_CLAIM_MISMATCH",
                    format!("review {} does not contain the exact claim", claim.review_reference),
                ));
            }
            match claim.review_kind {
                ReviewKind::IssueComment if review.pull_request_state.is_some() => {
                    return Err(ReviewPolicyError::new(
                        "REVIEW_STATE_INVALID",
                        "issue comments must not carry pull-request review state",
                    ));
                }
                ReviewKind::PullRequestReview
                    if review.pull_request_state != Some(PullRequestReviewState::Approved) =>
                {
                    return Err(ReviewPolicyError::new(
                        "REVIEW_STATE_REJECTED",
                        "pull-request review attestation must be approved",
                    ));
                }
                _ => {}
            }
        }

        Ok(())
    }
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

fn validate_claim(claim: &RequiredReviewClaimV1, observed_at: i64) -> Result<(), ReviewPolicyError> {
    validate_identifier("evidence_id", &claim.evidence_id)?;
    validate_identifier("gate", &claim.gate)?;
    validate_subject("claim.subject", &claim.subject)?;
    validate_identifier("reviewer", &claim.reviewer)?;
    validate_identifier("review_reference", &claim.review_reference)?;
    if !is_lower_hex(&claim.claim_sha256, 64) {
        return Err(ReviewPolicyError::new(
            "REVIEW_CLAIM_DIGEST_INVALID",
            "claim_sha256 must be 64 lowercase hexadecimal characters",
        ));
    }
    if claim.execution_window_start_unix_seconds <= 0
        || claim.reviewed_at_unix_seconds < claim.execution_window_start_unix_seconds
        || claim.reviewed_at_unix_seconds > observed_at
    {
        return Err(ReviewPolicyError::new(
            "REVIEW_TIME_INVALID",
            "review time must be inside the observed execution window",
        ));
    }
    Ok(())
}

fn validate_provider_review(
    review: &ProviderReviewFactV1,
    observed_at: i64,
) -> Result<(), ReviewPolicyError> {
    validate_identifier("provider.review_reference", &review.review_reference)?;
    validate_identifier("provider.author", &review.author)?;
    if review.body.len() > MAX_REVIEW_BODY_BYTES
        || review.body.chars().any(char::is_control)
        || contains_secret_shape(&review.body)
    {
        return Err(ReviewPolicyError::new(
            "REVIEW_PROVIDER_BODY_INVALID",
            "provider review body is overlong, contains control characters, or resembles secret material",
        ));
    }
    if review.observed_at_unix_seconds <= 0 || review.observed_at_unix_seconds > observed_at {
        return Err(ReviewPolicyError::new(
            "REVIEW_PROVIDER_TIME_INVALID",
            "provider review timestamp must not be in the future",
        ));
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), ReviewPolicyError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ReviewPolicyError::new(
            "REVIEW_IDENTIFIER_INVALID",
            format!("{field} is empty, overlong, padded, or contains control characters"),
        ));
    }
    Ok(())
}

fn validate_subject(field: &'static str, value: &str) -> Result<(), ReviewPolicyError> {
    if !is_lower_hex(value, 40) {
        return Err(ReviewPolicyError::new(
            "REVIEW_SUBJECT_INVALID",
            format!("{field} must be a 40-character lowercase commit SHA"),
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
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderReviewFactV1, PullRequestReviewState, RequiredReviewClaimV1,
        ReviewAttestationPolicyV1, ReviewKind, ReviewObservationV1, ReviewStatus,
        expected_claim_body,
    };

    const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn claim(kind: ReviewKind) -> RequiredReviewClaimV1 {
        RequiredReviewClaimV1 {
            evidence_id: "security-review".to_owned(),
            gate: "independent_security_review".to_owned(),
            status: ReviewStatus::Passed,
            subject: SUBJECT.to_owned(),
            claim_sha256: DIGEST.to_owned(),
            reviewer: "trusted-reviewer".to_owned(),
            reviewed_at_unix_seconds: 1_700_000_010,
            execution_window_start_unix_seconds: 1_700_000_000,
            review_kind: kind,
            review_reference: "42".to_owned(),
        }
    }

    fn observation(kind: ReviewKind) -> ReviewObservationV1 {
        let claim = claim(kind);
        ReviewObservationV1 {
            repository: REPOSITORY.to_owned(),
            subject: SUBJECT.to_owned(),
            provider_repository: REPOSITORY.to_owned(),
            provider_subject: SUBJECT.to_owned(),
            observed_at_unix_seconds: 1_700_000_020,
            required_claims: vec![claim.clone()],
            provider_reviews: vec![ProviderReviewFactV1 {
                review_kind: kind,
                review_reference: "42".to_owned(),
                author: "trusted-reviewer".to_owned(),
                body: expected_claim_body(&claim),
                observed_at_unix_seconds: 1_700_000_010,
                pull_request_state: match kind {
                    ReviewKind::IssueComment => None,
                    ReviewKind::PullRequestReview => Some(PullRequestReviewState::Approved),
                },
            }],
        }
    }

    fn policy() -> ReviewAttestationPolicyV1 {
        ReviewAttestationPolicyV1::new(REPOSITORY, SUBJECT).expect("valid policy")
    }

    #[test]
    fn zero_obligation_provider_binding_is_valid() {
        let observation = ReviewObservationV1 {
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
    fn exact_comment_and_approved_review_are_valid() {
        assert!(policy().evaluate(&observation(ReviewKind::IssueComment)).is_ok());
        assert!(
            policy()
                .evaluate(&observation(ReviewKind::PullRequestReview))
                .is_ok()
        );
    }

    #[test]
    fn wrong_repository_subject_reviewer_or_claim_fail_closed() {
        let mut value = observation(ReviewKind::IssueComment);
        value.provider_repository = "other/repo".to_owned();
        assert_eq!(
            policy().evaluate(&value).expect_err("repository rejected").code(),
            "REVIEW_REPOSITORY_MISMATCH"
        );

        let mut value = observation(ReviewKind::IssueComment);
        value.provider_subject = "cccccccccccccccccccccccccccccccccccccccc".to_owned();
        assert_eq!(
            policy().evaluate(&value).expect_err("subject rejected").code(),
            "REVIEW_SUBJECT_MISMATCH"
        );

        let mut value = observation(ReviewKind::IssueComment);
        value.provider_reviews[0].author = "other-reviewer".to_owned();
        assert_eq!(
            policy().evaluate(&value).expect_err("reviewer rejected").code(),
            "REVIEW_REVIEWER_MISMATCH"
        );

        let mut value = observation(ReviewKind::IssueComment);
        value.provider_reviews[0].body.push('x');
        assert_eq!(
            policy().evaluate(&value).expect_err("claim rejected").code(),
            "REVIEW_CLAIM_MISMATCH"
        );
    }

    #[test]
    fn missing_ambiguous_nonapproved_and_secret_reviews_fail_closed() {
        let mut value = observation(ReviewKind::IssueComment);
        value.provider_reviews.clear();
        assert_eq!(
            policy().evaluate(&value).expect_err("missing rejected").code(),
            "REVIEW_MISSING"
        );

        let mut value = observation(ReviewKind::IssueComment);
        value.provider_reviews.push(value.provider_reviews[0].clone());
        assert_eq!(
            policy().evaluate(&value).expect_err("ambiguous rejected").code(),
            "REVIEW_PROVIDER_AMBIGUOUS"
        );

        let mut value = observation(ReviewKind::PullRequestReview);
        value.provider_reviews[0].pull_request_state =
            Some(PullRequestReviewState::ChangesRequested);
        assert_eq!(
            policy().evaluate(&value).expect_err("state rejected").code(),
            "REVIEW_STATE_REJECTED"
        );

        let mut value = observation(ReviewKind::IssueComment);
        value.provider_reviews[0].body = "Authorization: Bearer secret".to_owned();
        assert_eq!(
            policy().evaluate(&value).expect_err("secret rejected").code(),
            "REVIEW_PROVIDER_BODY_INVALID"
        );
    }
}
