use crate::canonical::{canonical_json, canonical_pretty_json, parse_strict_json, sha256_hex};
use opsctl_core::hosted_evidence::{
    EvidenceBindingV1, EvidenceEnvironment, EvidenceIssuer, EvidenceOutcome, EvidencePolicyError,
    EvidencePolicyV1, EvidenceSource, EvidenceSubject, EvidenceTarget, EvidenceTrustState,
    HostedEvidenceEnvelopeV1, HostedEvidenceObservationV1,
};
use opsctl_core::review_evidence::{
    AcceptedReviewV1, ProviderReviewFactV1, PullRequestReviewState, RequiredReviewClaimV1,
    ReviewAttestationObservationV1, ReviewAttestationPolicyError, ReviewAttestationPolicyV1,
    ReviewClaimStatus, ReviewKind,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

const OBSERVATION_KIND: &str = "REVIEW_EVIDENCE_OBSERVATION";
const PAYLOAD_KIND: &str = "REVIEW_EVIDENCE_PAYLOAD";
const ARTIFACT_KIND: &str = "REVIEW_EVIDENCE_ARTIFACT";
const ENVELOPE_KIND: &str = "HOSTED_EVIDENCE_ENVELOPE";
const SCHEMA_VERSION: u64 = 1;
const DIGEST_ALGORITHM: &str = "SHA-256";
const DIGEST_SCOPE: &str = "RFC8785_CANONICAL_REVIEW_PAYLOAD_BYTES";
const REVIEW_ISSUER: &str = "github-actions";
const REVIEW_SOURCE: &str = "external-review-attestation-gate/github-review-attestations";
const REVIEW_TARGET: &str = "iamaman11/part-crm-emai-profile";
const REVIEW_ENVIRONMENT: &str = "governance";
const REVIEW_MAX_VALIDITY_SECONDS: u64 = 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewEvidenceAdapterError {
    message: String,
}

impl ReviewEvidenceAdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ReviewEvidenceAdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReviewEvidenceAdapterError {}

impl From<EvidencePolicyError> for ReviewEvidenceAdapterError {
    fn from(error: EvidencePolicyError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<ReviewAttestationPolicyError> for ReviewEvidenceAdapterError {
    fn from(error: ReviewAttestationPolicyError) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewObservationDto {
    schema_version: u64,
    kind: String,
    repository: String,
    subject: String,
    provider_repository: String,
    provider_subject: String,
    source_run_id: u64,
    source_run_attempt: u32,
    observed_at_unix_seconds: i64,
    valid_until_unix_seconds: i64,
    production_mutation: bool,
    required_claims: Vec<RequiredReviewClaimDto>,
    provider_reviews: Vec<ProviderReviewDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RequiredReviewClaimDto {
    evidence_id: String,
    gate: String,
    status: ReviewClaimStatusDto,
    subject: String,
    claim_sha256: String,
    reviewer: String,
    reviewed_at_unix_seconds: i64,
    execution_window_start_unix_seconds: i64,
    review_kind: ReviewKindDto,
    review_reference: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderReviewDto {
    review_kind: ReviewKindDto,
    review_reference: String,
    author: String,
    body: String,
    observed_at_unix_seconds: i64,
    pull_request_state: Option<PullRequestReviewStateDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ReviewKindDto {
    IssueComment,
    PullRequestReview,
}

impl From<ReviewKindDto> for ReviewKind {
    fn from(value: ReviewKindDto) -> Self {
        match value {
            ReviewKindDto::IssueComment => Self::IssueComment,
            ReviewKindDto::PullRequestReview => Self::PullRequestReview,
        }
    }
}

impl From<ReviewKind> for ReviewKindDto {
    fn from(value: ReviewKind) -> Self {
        match value {
            ReviewKind::IssueComment => Self::IssueComment,
            ReviewKind::PullRequestReview => Self::PullRequestReview,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ReviewClaimStatusDto {
    Passed,
    Failed,
}

impl From<ReviewClaimStatusDto> for ReviewClaimStatus {
    fn from(value: ReviewClaimStatusDto) -> Self {
        match value {
            ReviewClaimStatusDto::Passed => Self::Passed,
            ReviewClaimStatusDto::Failed => Self::Failed,
        }
    }
}

impl From<ReviewClaimStatus> for ReviewClaimStatusDto {
    fn from(value: ReviewClaimStatus) -> Self {
        match value {
            ReviewClaimStatus::Passed => Self::Passed,
            ReviewClaimStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum PullRequestReviewStateDto {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

impl From<PullRequestReviewStateDto> for PullRequestReviewState {
    fn from(value: PullRequestReviewStateDto) -> Self {
        match value {
            PullRequestReviewStateDto::Approved => Self::Approved,
            PullRequestReviewStateDto::ChangesRequested => Self::ChangesRequested,
            PullRequestReviewStateDto::Commented => Self::Commented,
            PullRequestReviewStateDto::Dismissed => Self::Dismissed,
            PullRequestReviewStateDto::Pending => Self::Pending,
        }
    }
}

impl From<PullRequestReviewState> for PullRequestReviewStateDto {
    fn from(value: PullRequestReviewState) -> Self {
        match value {
            PullRequestReviewState::Approved => Self::Approved,
            PullRequestReviewState::ChangesRequested => Self::ChangesRequested,
            PullRequestReviewState::Commented => Self::Commented,
            PullRequestReviewState::Dismissed => Self::Dismissed,
            PullRequestReviewState::Pending => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AcceptedReviewDto {
    evidence_id: String,
    gate: String,
    status: ReviewClaimStatusDto,
    subject: String,
    claim_sha256: String,
    reviewer: String,
    reviewed_at_unix_seconds: i64,
    review_kind: ReviewKindDto,
    review_reference: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TrustStateDto {
    Trusted,
    Untrusted,
    Unknown,
}

impl From<EvidenceTrustState> for TrustStateDto {
    fn from(value: EvidenceTrustState) -> Self {
        match value {
            EvidenceTrustState::Trusted => Self::Trusted,
            EvidenceTrustState::Untrusted => Self::Untrusted,
            EvidenceTrustState::Unknown => Self::Unknown,
        }
    }
}

impl From<TrustStateDto> for EvidenceTrustState {
    fn from(value: TrustStateDto) -> Self {
        match value {
            TrustStateDto::Trusted => Self::Trusted,
            TrustStateDto::Untrusted => Self::Untrusted,
            TrustStateDto::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OutcomeDto {
    Pass,
    Fail,
}

impl From<EvidenceOutcome> for OutcomeDto {
    fn from(value: EvidenceOutcome) -> Self {
        match value {
            EvidenceOutcome::Passed => Self::Pass,
            EvidenceOutcome::Failed => Self::Fail,
        }
    }
}

impl From<OutcomeDto> for EvidenceOutcome {
    fn from(value: OutcomeDto) -> Self {
        match value {
            OutcomeDto::Pass => Self::Passed,
            OutcomeDto::Fail => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeDto {
    schema_version: u64,
    kind: String,
    issuer: String,
    source: String,
    target: String,
    environment: String,
    subject: String,
    source_run_id: u64,
    source_run_attempt: u32,
    observed_at_unix_seconds: i64,
    valid_until_unix_seconds: i64,
    trust_state: TrustStateDto,
    outcome: OutcomeDto,
    production_mutation: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewPayloadDto {
    schema_version: u64,
    kind: String,
    observation: ReviewObservationDto,
    accepted_reviews: Vec<AcceptedReviewDto>,
    envelope: EnvelopeDto,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DigestDto {
    algorithm: String,
    scope: String,
    value: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewArtifactDto {
    schema_version: u64,
    kind: String,
    digest: DigestDto,
    payload: ReviewPayloadDto,
}

#[must_use]
pub fn is_review_observation_json(input: &str) -> bool {
    contract_kind(input).as_deref() == Some(OBSERVATION_KIND)
}

#[must_use]
pub fn is_review_artifact_json(input: &str) -> bool {
    contract_kind(input).as_deref() == Some(ARTIFACT_KIND)
}

fn contract_kind(input: &str) -> Option<String> {
    let value = parse_strict_json(input).ok()?;
    value.get("kind")?.as_str().map(str::to_owned)
}

pub fn seal_review_json(
    observation_json: &str,
    evaluated_at_unix_seconds: i64,
    expected_subject: &str,
) -> Result<String, ReviewEvidenceAdapterError> {
    validate_expected_subject(expected_subject)?;
    let value = parse_strict_json(observation_json).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_OBSERVATION_JSON: {error}"))
    })?;
    let dto: ReviewObservationDto = serde_json::from_value(value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_OBSERVATION_SCHEMA: {error}"))
    })?;
    validate_observation_contract(&dto, expected_subject)?;

    let domain_observation = observation_from_dto(&dto);
    let review_policy = ReviewAttestationPolicyV1::new(REVIEW_TARGET, expected_subject)?;
    let review_decision = review_policy.evaluate(&domain_observation)?;
    let hosted_observation = hosted_observation(&dto)?;
    let envelope = hosted_policy(expected_subject)?.evaluate(
        hosted_observation,
        evaluated_at_unix_seconds,
    )?;

    let rendered = render_artifact(dto, &review_decision.accepted_reviews, &envelope)?;
    let verified = verify_review_json(&rendered, evaluated_at_unix_seconds, expected_subject)?;
    if rendered != verified {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ROUNDTRIP_MISMATCH: seal/verify changed canonical evidence bytes",
        ));
    }
    Ok(rendered)
}

pub fn verify_review_json(
    artifact_json: &str,
    evaluated_at_unix_seconds: i64,
    expected_subject: &str,
) -> Result<String, ReviewEvidenceAdapterError> {
    validate_expected_subject(expected_subject)?;
    let value = parse_strict_json(artifact_json).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_JSON: {error}"))
    })?;
    let artifact: ReviewArtifactDto = serde_json::from_value(value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_SCHEMA: {error}"))
    })?;
    validate_artifact_contract(&artifact)?;

    let payload_value = serde_json::to_value(&artifact.payload).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_SCHEMA: {error}"))
    })?;
    let canonical_payload = canonical_json(&payload_value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_CANONICAL: {error}"))
    })?;
    if sha256_hex(canonical_payload.as_bytes()) != artifact.digest.value {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_DIGEST_MISMATCH: canonical payload digest does not match artifact",
        ));
    }

    validate_observation_contract(&artifact.payload.observation, expected_subject)?;
    let domain_observation = observation_from_dto(&artifact.payload.observation);
    let review_decision = ReviewAttestationPolicyV1::new(REVIEW_TARGET, expected_subject)?
        .evaluate(&domain_observation)?;
    let expected_reviews: Vec<AcceptedReviewDto> = review_decision
        .accepted_reviews
        .iter()
        .map(accepted_review_to_dto)
        .collect();
    if expected_reviews != artifact.payload.accepted_reviews {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_SEMANTIC_MISMATCH: durable accepted reviews differ from typed review policy output",
        ));
    }

    let envelope = envelope_from_dto(&artifact.payload.envelope)?;
    let reevaluated = hosted_policy(expected_subject)?
        .evaluate(envelope.as_observation(), evaluated_at_unix_seconds)?;
    if reevaluated != envelope {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_HOSTED_SEMANTIC_MISMATCH: hosted envelope changed after EvidencePolicyV1 evaluation",
        ));
    }

    let rendered = render_artifact(
        artifact.payload.observation.clone(),
        &review_decision.accepted_reviews,
        &reevaluated,
    )?;
    if artifact_json != rendered {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ARTIFACT_NOT_CANONICAL: durable artifact bytes must use the canonical pretty projection",
        ));
    }
    Ok(rendered)
}

fn validate_observation_contract(
    dto: &ReviewObservationDto,
    expected_subject: &str,
) -> Result<(), ReviewEvidenceAdapterError> {
    if dto.schema_version != SCHEMA_VERSION || dto.kind != OBSERVATION_KIND {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_OBSERVATION_CONTRACT: unsupported schema_version or kind",
        ));
    }
    if dto.repository != REVIEW_TARGET || dto.subject != expected_subject {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_DECLARED_BINDING_MISMATCH: repository or subject differs from the expected consumer",
        ));
    }
    Ok(())
}

fn validate_artifact_contract(
    artifact: &ReviewArtifactDto,
) -> Result<(), ReviewEvidenceAdapterError> {
    if artifact.schema_version != SCHEMA_VERSION || artifact.kind != ARTIFACT_KIND {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ARTIFACT_CONTRACT: unsupported schema_version or kind",
        ));
    }
    if artifact.payload.schema_version != SCHEMA_VERSION || artifact.payload.kind != PAYLOAD_KIND {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_PAYLOAD_CONTRACT: unsupported schema_version or kind",
        ));
    }
    if artifact.digest.algorithm != DIGEST_ALGORITHM || artifact.digest.scope != DIGEST_SCOPE {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_DIGEST_CONTRACT: digest algorithm or scope drifted",
        ));
    }
    if !is_lower_hex(&artifact.digest.value, 64) {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_DIGEST_INVALID: digest must be exactly 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

fn observation_from_dto(dto: &ReviewObservationDto) -> ReviewAttestationObservationV1 {
    ReviewAttestationObservationV1 {
        repository: dto.repository.clone(),
        subject: dto.subject.clone(),
        provider_repository: dto.provider_repository.clone(),
        provider_subject: dto.provider_subject.clone(),
        observed_at_unix_seconds: dto.observed_at_unix_seconds,
        required_claims: dto
            .required_claims
            .iter()
            .map(|claim| RequiredReviewClaimV1 {
                evidence_id: claim.evidence_id.clone(),
                gate: claim.gate.clone(),
                status: claim.status.into(),
                subject: claim.subject.clone(),
                claim_sha256: claim.claim_sha256.clone(),
                reviewer: claim.reviewer.clone(),
                reviewed_at_unix_seconds: claim.reviewed_at_unix_seconds,
                execution_window_start_unix_seconds: claim.execution_window_start_unix_seconds,
                review_kind: claim.review_kind.into(),
                review_reference: claim.review_reference.clone(),
            })
            .collect(),
        provider_reviews: dto
            .provider_reviews
            .iter()
            .map(|provider| ProviderReviewFactV1 {
                review_kind: provider.review_kind.into(),
                review_reference: provider.review_reference.clone(),
                author: provider.author.clone(),
                body: provider.body.clone(),
                observed_at_unix_seconds: provider.observed_at_unix_seconds,
                pull_request_state: provider.pull_request_state.map(Into::into),
            })
            .collect(),
    }
}

fn hosted_observation(
    dto: &ReviewObservationDto,
) -> Result<HostedEvidenceObservationV1, ReviewEvidenceAdapterError> {
    Ok(HostedEvidenceObservationV1 {
        binding: hosted_binding(&dto.subject)?,
        source_run_id: dto.source_run_id,
        source_run_attempt: dto.source_run_attempt,
        observed_at_unix_seconds: dto.observed_at_unix_seconds,
        valid_until_unix_seconds: dto.valid_until_unix_seconds,
        trust_state: EvidenceTrustState::Trusted,
        outcome: EvidenceOutcome::Passed,
        production_mutation: dto.production_mutation,
    })
}

fn hosted_policy(expected_subject: &str) -> Result<EvidencePolicyV1, ReviewEvidenceAdapterError> {
    EvidencePolicyV1::new(
        hosted_binding(expected_subject)?,
        REVIEW_MAX_VALIDITY_SECONDS,
    )
    .map_err(Into::into)
}

fn hosted_binding(subject: &str) -> Result<EvidenceBindingV1, ReviewEvidenceAdapterError> {
    Ok(EvidenceBindingV1 {
        issuer: EvidenceIssuer::new(REVIEW_ISSUER)?,
        source: EvidenceSource::new(REVIEW_SOURCE)?,
        target: EvidenceTarget::new(REVIEW_TARGET)?,
        environment: EvidenceEnvironment::new(REVIEW_ENVIRONMENT)?,
        subject: EvidenceSubject::new(subject)?,
    })
}

fn envelope_from_dto(
    dto: &EnvelopeDto,
) -> Result<HostedEvidenceEnvelopeV1, ReviewEvidenceAdapterError> {
    if dto.schema_version != SCHEMA_VERSION || dto.kind != ENVELOPE_KIND {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_ENVELOPE_CONTRACT: unsupported schema_version or kind",
        ));
    }
    Ok(HostedEvidenceEnvelopeV1 {
        binding: EvidenceBindingV1 {
            issuer: EvidenceIssuer::new(dto.issuer.clone())?,
            source: EvidenceSource::new(dto.source.clone())?,
            target: EvidenceTarget::new(dto.target.clone())?,
            environment: EvidenceEnvironment::new(dto.environment.clone())?,
            subject: EvidenceSubject::new(dto.subject.clone())?,
        },
        source_run_id: dto.source_run_id,
        source_run_attempt: dto.source_run_attempt,
        observed_at_unix_seconds: dto.observed_at_unix_seconds,
        valid_until_unix_seconds: dto.valid_until_unix_seconds,
        trust_state: dto.trust_state.into(),
        outcome: dto.outcome.into(),
        production_mutation: dto.production_mutation,
    })
}

fn envelope_to_dto(envelope: &HostedEvidenceEnvelopeV1) -> EnvelopeDto {
    EnvelopeDto {
        schema_version: SCHEMA_VERSION,
        kind: ENVELOPE_KIND.to_owned(),
        issuer: envelope.binding.issuer.as_str().to_owned(),
        source: envelope.binding.source.as_str().to_owned(),
        target: envelope.binding.target.as_str().to_owned(),
        environment: envelope.binding.environment.as_str().to_owned(),
        subject: envelope.binding.subject.as_str().to_owned(),
        source_run_id: envelope.source_run_id,
        source_run_attempt: envelope.source_run_attempt,
        observed_at_unix_seconds: envelope.observed_at_unix_seconds,
        valid_until_unix_seconds: envelope.valid_until_unix_seconds,
        trust_state: envelope.trust_state.into(),
        outcome: envelope.outcome.into(),
        production_mutation: envelope.production_mutation,
    }
}

fn accepted_review_to_dto(review: &AcceptedReviewV1) -> AcceptedReviewDto {
    AcceptedReviewDto {
        evidence_id: review.evidence_id.clone(),
        gate: review.gate.clone(),
        status: review.status.into(),
        subject: review.subject.clone(),
        claim_sha256: review.claim_sha256.clone(),
        reviewer: review.reviewer.clone(),
        reviewed_at_unix_seconds: review.reviewed_at_unix_seconds,
        review_kind: review.review_kind.into(),
        review_reference: review.review_reference.clone(),
    }
}

fn render_artifact(
    observation: ReviewObservationDto,
    accepted_reviews: &[AcceptedReviewV1],
    envelope: &HostedEvidenceEnvelopeV1,
) -> Result<String, ReviewEvidenceAdapterError> {
    let payload = ReviewPayloadDto {
        schema_version: SCHEMA_VERSION,
        kind: PAYLOAD_KIND.to_owned(),
        observation,
        accepted_reviews: accepted_reviews.iter().map(accepted_review_to_dto).collect(),
        envelope: envelope_to_dto(envelope),
    };
    let payload_value = serde_json::to_value(&payload).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_SCHEMA: {error}"))
    })?;
    let canonical_payload = canonical_json(&payload_value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_PAYLOAD_CANONICAL: {error}"))
    })?;
    let artifact = ReviewArtifactDto {
        schema_version: SCHEMA_VERSION,
        kind: ARTIFACT_KIND.to_owned(),
        digest: DigestDto {
            algorithm: DIGEST_ALGORITHM.to_owned(),
            scope: DIGEST_SCOPE.to_owned(),
            value: sha256_hex(canonical_payload.as_bytes()),
        },
        payload,
    };
    let value = serde_json::to_value(artifact).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_SCHEMA: {error}"))
    })?;
    canonical_pretty_json(&value).map_err(|error| {
        ReviewEvidenceAdapterError::new(format!("REVIEW_EVIDENCE_ARTIFACT_CANONICAL: {error}"))
    })
}

fn validate_expected_subject(subject: &str) -> Result<(), ReviewEvidenceAdapterError> {
    if !is_lower_hex(subject, 40) {
        return Err(ReviewEvidenceAdapterError::new(
            "REVIEW_EVIDENCE_EXPECTED_SUBJECT_INVALID: expected source commit must be exactly 40 lowercase hexadecimal characters",
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

#[cfg(test)]
mod tests {
    use super::{seal_review_json, verify_review_json};
    use serde_json::{Value, json};

    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OBSERVED_AT: i64 = 1_700_000_000;
    const EVALUATED_AT: i64 = 1_700_000_010;

    fn observation() -> Value {
        json!({
            "schema_version": 1,
            "kind": "REVIEW_EVIDENCE_OBSERVATION",
            "repository": "iamaman11/part-crm-emai-profile",
            "subject": SUBJECT,
            "provider_repository": "iamaman11/part-crm-emai-profile",
            "provider_subject": SUBJECT,
            "source_run_id": 42,
            "source_run_attempt": 1,
            "observed_at_unix_seconds": OBSERVED_AT,
            "valid_until_unix_seconds": OBSERVED_AT + 3600,
            "production_mutation": false,
            "required_claims": [],
            "provider_reviews": []
        })
    }

    fn render_observation(value: &Value) -> String {
        serde_json::to_string(value).expect("observation json")
    }

    #[test]
    fn zero_obligation_observation_roundtrips_byte_stably() {
        let input = render_observation(&observation());
        let first = seal_review_json(&input, EVALUATED_AT, SUBJECT).expect("seal");
        let second = seal_review_json(&input, EVALUATED_AT, SUBJECT).expect("seal repeat");
        assert_eq!(first, second);
        assert_eq!(
            first,
            verify_review_json(&first, EVALUATED_AT, SUBJECT).expect("verify")
        );
    }

    #[test]
    fn rejects_wrong_version_unknown_field_and_duplicate_key() {
        let mut wrong_version = observation();
        wrong_version["schema_version"] = json!(2);
        assert!(
            seal_review_json(&render_observation(&wrong_version), EVALUATED_AT, SUBJECT).is_err()
        );

        let mut unknown = observation();
        unknown["token"] = json!("secret");
        assert!(seal_review_json(&render_observation(&unknown), EVALUATED_AT, SUBJECT).is_err());

        let duplicate = format!(
            "{{\"schema_version\":1,\"schema_version\":1,\"kind\":\"REVIEW_EVIDENCE_OBSERVATION\",\"repository\":\"iamaman11/part-crm-emai-profile\",\"subject\":\"{SUBJECT}\",\"provider_repository\":\"iamaman11/part-crm-emai-profile\",\"provider_subject\":\"{SUBJECT}\",\"source_run_id\":42,\"source_run_attempt\":1,\"observed_at_unix_seconds\":{OBSERVED_AT},\"valid_until_unix_seconds\":{},\"production_mutation\":false,\"required_claims\":[],\"provider_reviews\":[]}}",
            OBSERVED_AT + 3600
        );
        assert!(seal_review_json(&duplicate, EVALUATED_AT, SUBJECT).is_err());
    }

    #[test]
    fn rejects_wrong_repository_subject_and_production_mutation() {
        let mut wrong_repository = observation();
        wrong_repository["provider_repository"] = json!("other/repository");
        assert!(
            seal_review_json(
                &render_observation(&wrong_repository),
                EVALUATED_AT,
                SUBJECT
            )
            .is_err()
        );

        let mut wrong_subject = observation();
        wrong_subject["provider_subject"] =
            json!("cccccccccccccccccccccccccccccccccccccccc");
        assert!(
            seal_review_json(&render_observation(&wrong_subject), EVALUATED_AT, SUBJECT).is_err()
        );

        let mut mutation = observation();
        mutation["production_mutation"] = json!(true);
        assert!(seal_review_json(&render_observation(&mutation), EVALUATED_AT, SUBJECT).is_err());
    }

    #[test]
    fn generic_evidence_policy_owns_freshness_and_replay() {
        let mut future = observation();
        future["observed_at_unix_seconds"] = json!(EVALUATED_AT + 1);
        future["valid_until_unix_seconds"] = json!(EVALUATED_AT + 100);
        assert!(seal_review_json(&render_observation(&future), EVALUATED_AT, SUBJECT).is_err());

        let mut expired = observation();
        expired["valid_until_unix_seconds"] = json!(EVALUATED_AT);
        assert!(seal_review_json(&render_observation(&expired), EVALUATED_AT, SUBJECT).is_err());

        let mut oversized = observation();
        oversized["valid_until_unix_seconds"] = json!(OBSERVED_AT + 3601);
        assert!(seal_review_json(&render_observation(&oversized), EVALUATED_AT, SUBJECT).is_err());
    }

    #[test]
    fn digest_and_exact_bytes_reject_tamper() {
        let input = render_observation(&observation());
        let artifact = seal_review_json(&input, EVALUATED_AT, SUBJECT).expect("seal");
        let mut value: Value = serde_json::from_str(&artifact).expect("artifact json");
        value["digest"]["value"] = json!(
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        );
        let tampered = serde_json::to_string_pretty(&value).expect("tampered json");
        assert!(verify_review_json(&tampered, EVALUATED_AT, SUBJECT).is_err());

        let noncanonical = artifact.trim_end().to_owned();
        assert_ne!(artifact, noncanonical);
        assert!(verify_review_json(&noncanonical, EVALUATED_AT, SUBJECT).is_err());
    }
}
